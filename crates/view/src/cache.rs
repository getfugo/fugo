//! The view cache (REWRITE_PLAN.md §2.5): page and site values pre-serialised once and shared
//! through `Arc` as `tera::Value`s.
//!
//! - **Meta generation** (phase C0, content phase): summaries without content fields.
//! - **Full generations** (phase D, one per hook variant): summaries with the content fields
//!   of that variant. All are frozen in a `OnceLock` before any layout renders.
//!
//! Every list (relations, site lists, terms, pagers) holds **summary** values of its own
//! generation, so values are acyclic and each summary is one allocation shared by every list
//! it is in. A page's **full** value (the rendered page, `deref`, `get_page`) is its summary
//! plus its relations, assembled on first use (`OnceLock`; pure view assembly, no rendering,
//! so blocking initialisation is safe).
//!
//! What does not depend on content (params, output formats, resources, terms, links,
//! languages, menus, `site.data`, `site.config`) is serialised once and shared by all
//! generations; values many pages repeat (media types, sitemap settings, equal dates, empty
//! lists) are shared too, and variants whose content of a page is the same `Arc` share its
//! content values.
//!
//! **Memory.** A layout job renders its page with [`ViewGeneration::page_value`] (built for
//! the job, not kept); [`ViewGeneration::full`] keeps the values `deref` and `get_page` share.
//! On the docs site the Meta and Full generations keep 1.85× the model's heap (the `it`
//! test `memory::real_sites`).

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use rayon::prelude::*;
use serde::Serialize;
use ssg_base::{FormatId, IdVec, Idx, LangIdx, PageId, PageKind, ResourceId, TaxonomyIdx};
use ssg_config::output::{Escaping, Listing};
use ssg_nav::{MenuEntry, Menus};
use ssg_resources::{ResourceError, ResourceStore};
use ssg_site::{Model, PageRole};

use crate::content::{ContentError, ContentRenderer, RenderedContent};
use crate::nav::{NavSite, bundle_type};
use crate::resource::page_resources;
use crate::scope::{HookVariant, Phase, RenderScope};
use crate::views::{
    ContentView, DateView, FileView, FragmentsView, LanguageView, MediaTypeView, MenuEntryView,
    OutputFormatView, PageLink, PageRelations, PageSummaryView, SiteConfigView, SiteView,
    SitemapView, TaxonomyView, TermEntryView, TermView, params_value,
};

mod generation;
mod shared;

use shared::*;

/// The rendered content of every page in one hook variant (`None`: no content).
pub type Contents = IdVec<PageId, Option<Arc<RenderedContent>>>;

/// What the views are built from.
#[derive(Clone)]
pub struct ViewInputs {
    pub model: Arc<Model>,
    /// Bundle files are registered here; resource values carry its ids.
    pub store: Arc<ResourceStore>,
    /// `site.menus` (`ssg_nav::build_menus` over [`NavSite`]).
    pub menus: Arc<Menus>,
}

/// Why the views could not be built.
#[derive(Debug, thiserror::Error)]
pub enum ViewError {
    #[error("page {page}: {source}")]
    Resources {
        page: String,
        #[source]
        source: ResourceError,
    },
}

/// Generation-independent values and the model, shared by every generation.
struct Shared {
    model: Arc<Model>,
    nav: NavSite,
    store: Arc<ResourceStore>,
    menus: Arc<Menus>,
    /// `.Resources` of every page (store ids).
    resources: IdVec<PageId, Vec<ResourceId>>,
    /// The Meta summaries (content-free, `raw_content` included): the base of every
    /// generation's summaries.
    base: IdVec<PageId, tera::Value>,
    /// The empty list (shared by every empty list value).
    empty: tera::Value,
    links: IdVec<PageId, tera::Value>,
    /// Global `.Prev`/`.Next` and in-section neighbours: (prev = older, next = newer).
    prev_next: IdVec<PageId, (Option<PageId>, Option<PageId>)>,
    prev_next_in_section: IdVec<PageId, (Option<PageId>, Option<PageId>)>,
    all_pages: Vec<PageId>,
    languages: IdVec<LangIdx, tera::Value>,
    all_languages: tera::Value,
    data: tera::Value,
    configs: IdVec<LangIdx, tera::Value>,
    menu_values: IdVec<LangIdx, tera::Value>,
    sitemap_abs_urls: IdVec<LangIdx, Option<String>>,
}

/// One generation of page and site values.
pub struct ViewGeneration {
    shared: Arc<Shared>,
    /// Relation-free page values; every list holds these (Arc-shared).
    pub summaries: IdVec<PageId, tera::Value>,
    /// Page link values (`PageLink`).
    pub links: IdVec<PageId, tera::Value>,
    full: IdVec<PageId, OnceLock<tera::Value>>,
    /// Per language and taxonomy: the listed terms as `(key, TermEntryView)`.
    terms: IdVec<LangIdx, IdVec<TaxonomyIdx, Vec<(String, tera::Value)>>>,
    /// `site`, per language.
    pub sites: IdVec<LangIdx, tera::Value>,
}

impl std::fmt::Debug for ViewGeneration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewGeneration")
            .field("pages", &self.summaries.len())
            .field("sites", &self.sites.len())
            .finish_non_exhaustive()
    }
}

/// The content maps of the Full generations, by `RenderedContent` allocation: variants whose
/// rendering of a page is the same `Arc` share one set of values.
struct ContentMemo {
    maps: std::sync::Mutex<std::collections::HashMap<usize, tera::Map>>,
    /// `fragments` of a page without headings.
    no_fragments: tera::Value,
}

impl Default for ContentMemo {
    fn default() -> Self {
        Self {
            maps: std::sync::Mutex::default(),
            no_fragments: tera::Value::from_serializable(&FragmentsView::new(
                &ssg_markup::Fragments::default(),
            )),
        }
    }
}

impl ContentMemo {
    fn content(&self, c: Option<&Arc<RenderedContent>>) -> tera::Map {
        let key = c.map(|c| Arc::as_ptr(c) as usize);
        if let Some(k) = key
            && let Some(m) = self
                .maps
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get(&k)
        {
            return m.clone();
        }
        let mut view = ContentView::new(c.map(Arc::as_ref));
        if c.is_none_or(|c| c.fragments.headings.is_empty() && c.fragments.identifiers.is_empty()) {
            view.fragments = self.no_fragments.clone();
        }
        let m = to_map(&view);
        if let Some(k) = key {
            self.maps
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(k, m.clone());
        }
        m
    }
}

/// The frozen page and site values of a build.
pub struct ViewCache {
    shared: Arc<Shared>,
    meta: ViewGeneration,
    full: OnceLock<BTreeMap<HookVariant, ViewGeneration>>,
}

impl std::fmt::Debug for ViewCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewCache")
            .field("pages", &self.shared.model.pages.len())
            .field("frozen", &self.full.get().map(BTreeMap::len))
            .finish_non_exhaustive()
    }
}

impl ViewCache {
    /// The cache with its Meta generation (phase C0): bundle files are registered in the
    /// store, every summary and site value is serialised.
    ///
    /// # Errors
    /// Invalid `resources` front matter.
    pub fn new(inputs: ViewInputs) -> Result<Self, ViewError> {
        let shared = Arc::new(Shared::new(inputs)?);
        Ok(Self {
            meta: ViewGeneration::build(&shared, None, &ContentMemo::default()),
            shared,
            full: OnceLock::new(),
        })
    }

    /// Builds and freezes the Full generations from the rendered content of every variant
    /// (phase D). A second call is ignored.
    pub fn freeze(&self, contents: &BTreeMap<HookVariant, Contents>) {
        self.full.get_or_init(|| {
            let memo = ContentMemo::default();
            contents
                .iter()
                .map(|(&v, c)| (v, ViewGeneration::build(&self.shared, Some(c), &memo)))
                .collect()
        });
    }

    /// [`freeze`](Self::freeze) with the content of `variants` asked from `renderer` for every
    /// page with a content file (a page without content in a variant has empty fields).
    ///
    /// # Errors
    /// The first failing page (other than [`ContentError::NoContent`]).
    pub fn freeze_from(
        &self,
        renderer: &dyn ContentRenderer,
        variants: &[HookVariant],
    ) -> Result<(), ContentError> {
        let model = &self.shared.model;
        let mut all = BTreeMap::new();
        for &v in variants {
            let c: Vec<Option<Arc<RenderedContent>>> = model
                .pages
                .as_slice()
                .par_iter()
                .map(|p| {
                    if p.source.is_none() || p.role != PageRole::Standalone {
                        return Ok(None);
                    }
                    let format = p.formats.first().copied().unwrap_or(FormatId::from_raw(0));
                    let mut s = RenderScope::layout(p.id, p.lang, format, None);
                    s.phase = Phase::Content;
                    s.variant = v;
                    match renderer.content(p.id, v, &s) {
                        Ok(c) => Ok(Some(c)),
                        Err(ContentError::NoContent(_)) => Ok(None),
                        Err(e) => Err(e),
                    }
                })
                .collect::<Result<_, _>>()?;
            all.insert(v, IdVec::from(c));
        }
        self.freeze(&all);
        Ok(())
    }

    /// Whether the Full generations are frozen.
    #[must_use]
    pub fn is_frozen(&self) -> bool {
        self.full.get().is_some()
    }

    /// The Meta generation.
    #[must_use]
    pub fn meta(&self) -> &ViewGeneration {
        &self.meta
    }

    /// The hook variants of the Full generations (empty before phase D).
    #[must_use]
    pub fn variants(&self) -> Vec<HookVariant> {
        self.full
            .get()
            .map(|f| f.keys().copied().collect())
            .unwrap_or_default()
    }

    /// The generation a render in `phase` with variant `v` sees: Meta in the content phase and
    /// in content adapters, else the Full generation of `v` (of `Html` when `v` has none; Meta
    /// before phase D).
    #[must_use]
    pub fn generation(&self, phase: Phase, v: HookVariant) -> &ViewGeneration {
        if matches!(phase, Phase::Content | Phase::Adapter) {
            return &self.meta;
        }
        let Some(full) = self.full.get() else {
            return &self.meta;
        };
        full.get(&v)
            .or_else(|| full.get(&HookVariant::Html))
            .unwrap_or(&self.meta)
    }

    /// The model the views were built from.
    #[must_use]
    pub fn model(&self) -> &Arc<Model> {
        &self.shared.model
    }

    /// The model as `ssg-nav` reads it (aliases, pagination lists, related content).
    #[must_use]
    pub fn nav(&self) -> &NavSite {
        &self.shared.nav
    }

    /// The resource store the resource values point into.
    #[must_use]
    pub fn store(&self) -> &Arc<ResourceStore> {
        &self.shared.store
    }

    /// The menus `site.menus` shows.
    #[must_use]
    pub fn menus(&self) -> &Arc<Menus> {
        &self.shared.menus
    }

    /// `.Resources` of page `id` (store ids, in `page.resources` order).
    #[must_use]
    pub fn resources(&self, id: PageId) -> &[ResourceId] {
        &self.shared.resources[id]
    }

    /// The number of pages (ids are `0..len`).
    #[must_use]
    pub fn len(&self) -> usize {
        self.shared.model.pages.len()
    }

    /// Whether the site has no page.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether `idx` is a page id of this cache.
    #[must_use]
    pub fn contains(&self, id: PageId) -> bool {
        id.index() < self.len()
    }
}
