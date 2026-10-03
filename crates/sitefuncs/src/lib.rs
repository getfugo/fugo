//! Site-bound Tera functions as small handle structs (REWRITE_PLAN.md §1.2, §2.6, §4.2–4.6):
//! every `ssg_funcs::spec::FUNCS` entry marked site-bound (73 names), registered by
//! [`register`].
//!
//! Each function is a struct that holds only the `Arc`s it needs (`GetPage { views }`,
//! `I18n { views, i18n }`, …); [`Handles`] is the bag they are cloned from. Every call checks
//! its kwargs against the spec entry and takes its safety from it.
//!
//! **Render scope.** Functions read the render position from the context value `__nh`
//! ([`ssg_view::RenderScope::from_state`]); there are no thread-locals.
//! - Page-relative functions (`get_page`, `ref`, `param`, `store_*`, `i18n`, `page_content`, …)
//!   also take `page=`: the scope's page, language and format are then that page's (primary
//!   format). Without `page=` and without a scope (a component that did not declare `@__nh`)
//!   they fail with a hint. `i18n` and the URL filters fall back to the context's `lang`.
//! - `paginator`, `paginate`, `return_value` and `defer` need the real scope.
//!
//! **Where the state lives** (all owned by the render session, shared through `Handles`):
//!
//! | Function | State |
//! |---|---|
//! | `paginator`, `paginate` | [`ssg_view::PaginationRecorder`]: first call per (page, format) records, an equal re-call reuses, another list or size is an error naming both call positions; pager `__nh.pager` (wave 2), also inside `partial()` (the child scope keeps the pager) |
//! | `store_set`, `store_get` | [`ssg_view::PageStores`] (content-phase writes buffered in `__nh.txn`) |
//! | `partial`, `return_value` | [`Frames`]: each `partial()` call allocates a frame, `return_value` writes it |
//! | `partial_cached` | `Handles::partial_cache`, the whole [`PartialResult`] per (name, key) |
//! | `defer` | [`ssg_view::DeferredRegistry`]; returns `__nh_defer_<key>__` |
//! | `related` | [`RelatedCache`]: one index per candidate list |
//! | resources, images | `ResourceStore` (lazy transforms, post-process placeholders), `ImageQueue` |
//! | `add_page`, `add_resource`, `enable_all_languages`, the adapter's store | [`ContentAdapters`]: the runs of content adapters (phase `Adapter`, the run in `__nh.adapter`) |
//!
//! **Content seam.** `page_content` and friends, `markdownify`, `render_string`,
//! `render_shortcodes`, `resource_content` of a bundled page and `partial()` call the render
//! session through `Weak<dyn ContentRenderer>` (`Handles::renderer`, filled after the session
//! exists). `execute_as_template` renders the asset's source with the build's Tera instance
//! (`Handles::templates`, filled after `layouts::load`).
//!
//! **Positions.** Tera 2.4 gives functions no call position. A pagination record's position is
//! the partial template the call ran in (its file), or `layout of <page> (<format>)` for a call
//! in the layout itself; Tera adds the conflicting call's own position to the error.

#![forbid(unsafe_code)]

mod adapters;
mod call;
mod content;
mod images;
mod lists;
mod pages;
mod pagination;
mod resources;
mod templates;
mod urls;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};

use dashmap::DashMap;
use ssg_base::diag::Diagnostics;
use ssg_base::{FrameId, LangIdx, PageId};
use ssg_highlight::Highlight;
use ssg_images::ImageQueue;
use ssg_layouts::{TemplateName, Templates};
use ssg_locale::Translations;
use ssg_nav::{Menus, NavError, RelatedIndex};
use ssg_resources::ResourceStore;
use ssg_site::Model;
use ssg_view::{ContentRenderer, DeferredRegistry, PageStores, PaginationRecorder, ViewCache};

pub use adapters::{AdapterRun, ContentAdapters};

/// A cached `partial_cached` result: the rendered text, or the partial's `return_value`.
#[derive(Clone, Debug, PartialEq)]
pub enum PartialResult {
    Text(String),
    Value(tera::Value),
}

/// The frames of `partial()` calls: `return_value` writes the value of the frame in its scope
/// (`__nh.frame`); the template a frame renders gives pagination records their position.
#[derive(Debug, Default)]
pub struct Frames {
    next: AtomicU64,
    values: DashMap<FrameId, tera::Value>,
    templates: DashMap<FrameId, TemplateName>,
}

impl Frames {
    /// A new frame rendering `template`.
    pub(crate) fn open(&self, template: &TemplateName) -> FrameId {
        let id = FrameId::from_raw(self.next.fetch_add(1, Ordering::Relaxed) + 1);
        self.templates.insert(id, template.clone());
        id
    }

    /// Closes `frame`: its returned value, if `return_value` ran.
    pub(crate) fn close(&self, frame: FrameId) -> Option<tera::Value> {
        self.templates.remove(&frame);
        self.values.remove(&frame).map(|(_, v)| v)
    }

    /// `return_value` in `frame`.
    pub(crate) fn set(&self, frame: FrameId, value: tera::Value) {
        self.values.insert(frame, value);
    }

    /// The template `frame` renders.
    pub(crate) fn template(&self, frame: FrameId) -> Option<TemplateName> {
        self.templates.get(&frame).map(|t| t.clone())
    }
}

/// A related-content index's key: the language and the candidate list.
type RelatedKey = (LangIdx, Vec<PageId>);

/// Related-content indices, one per (language, candidate list): `related(pages=site.pages)` and
/// `related(pages=site.regular_pages)` search different indices (REWRITE_PLAN.md §4.6).
#[derive(Debug, Default)]
pub struct RelatedCache {
    indices: Mutex<HashMap<RelatedKey, Arc<RelatedIndex>>>,
}

impl RelatedCache {
    /// The index over `candidates`, built on first use with language `lang`'s `[related]`
    /// configuration.
    ///
    /// # Errors
    /// A candidate with a value that cannot be a keyword.
    pub(crate) fn index(
        &self,
        views: &ViewCache,
        lang: LangIdx,
        candidates: &[PageId],
    ) -> Result<Arc<RelatedIndex>, NavError> {
        let key = (lang, candidates.to_vec());
        if let Some(i) = self
            .indices
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&key)
        {
            return Ok(Arc::clone(i));
        }
        let cfg = &views.model().config.sites[lang].related;
        let built = Arc::new(RelatedIndex::build(views.nav(), cfg, candidates)?);
        Ok(Arc::clone(
            self.indices
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(key)
                .or_insert(built),
        ))
    }
}

/// Everything site functions need; each function struct clones only its own `Arc`s.
///
/// The session creates the two slots empty, calls [`register`], loads the templates, then
/// fills `templates` (`Arc::downgrade` of the loaded [`Templates`]) and `renderer` (the
/// session as `Weak<dyn ContentRenderer>`) before the first render.
#[derive(Clone)]
pub struct Handles {
    pub model: Arc<Model>,
    pub views: Arc<ViewCache>,
    pub store: Arc<ResourceStore>,
    pub images: Arc<ImageQueue>,
    pub stores: Arc<PageStores>,
    pub pagination: Arc<PaginationRecorder>,
    pub deferred: Arc<DeferredRegistry>,
    /// The `purge_css` plans; the publisher replaces their placeholders per page.
    pub css_purges: Arc<ssg_minify::CssPurges>,
    pub menus: Arc<Menus>,
    pub related: Arc<RelatedCache>,
    pub i18n: Arc<Translations>,
    pub diagnostics: Arc<Diagnostics>,
    /// The `highlight` filter's highlighter (the site's `[markup.highlight]` defaults).
    pub highlight: Arc<Highlight>,
    /// Set by the session once it exists (`Arc::downgrade`); content-dependent functions
    /// upgrade it per call.
    pub renderer: Arc<OnceLock<Weak<dyn ContentRenderer>>>,
    /// Set once the templates are loaded: partial lookup, `template_exists`, and the Tera
    /// instance `execute_as_template` renders asset sources with.
    pub templates: Arc<OnceLock<Weak<Templates>>>,
    /// `return_value` frames of `partial()` calls.
    pub frames: Arc<Frames>,
    /// `partial_cached` results by (name, variant key).
    pub partial_cache: Arc<DashMap<(String, String), PartialResult>>,
    /// The runs of content adapters (only a session that runs adapters starts any).
    pub adapters: Arc<ContentAdapters>,
}

/// Registers every site-bound function, filter and test of `ssg_funcs::spec::FUNCS` on
/// `t` (call it after `ssg_funcs::register_pure`, before templates are added).
pub fn register(t: &mut tera::Tera, h: &Handles) {
    let mut r = call::Registrar::new(t);
    adapters::register(&mut r, h);
    pages::register(&mut r, h);
    lists::register(&mut r, h);
    pagination::register(&mut r, h);
    content::register(&mut r, h);
    urls::register(&mut r, h);
    resources::register(&mut r, h);
    images::register(&mut r, h);
    templates::register(&mut r, h);
}
