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

/// The value of a serialisable view, as a map (views are structs).
fn to_map<T: Serialize>(v: &T) -> tera::Map {
    tera::Value::from_serializable(v)
        .into_map()
        .unwrap_or_default()
}

/// The link value of page `id`.
fn page_link(model: &Model, id: PageId) -> PageLink {
    let p = &model.pages[id];
    let (permalink, rel_permalink) = p.links().map_or_else(Default::default, |l| {
        (l.permalink.to_string(), l.rel_permalink.escaped())
    });
    PageLink {
        id: id.raw(),
        kind: p.kind,
        path: p.path(),
        lang: model.config.sites[p.lang].language.key.clone(),
        title: p.title.clone(),
        link_title: p.link_title.clone(),
        permalink,
        rel_permalink,
    }
}

fn file_view(p: &ssg_site::Page) -> Option<FileView> {
    let src = p.source.as_ref()?;
    let rel = &src.file.rel;
    let (dir, file) = rel
        .rsplit_once('/')
        .map_or(("", rel.as_str()), |(d, f)| (d, f));
    let base = file.rsplit_once('.').map_or(file, |(b, _)| b);
    let unique_id = {
        use md5::Digest;
        md5::Md5::digest(rel.as_bytes())
            .iter()
            .fold(String::with_capacity(32), |mut s, b| {
                use std::fmt::Write;
                let _ = write!(s, "{b:02x}");
                s
            })
    };
    Some(FileView {
        path: rel.clone(),
        dir: if dir.is_empty() {
            String::new()
        } else {
            format!("{dir}/")
        },
        base_file_name: base.to_owned(),
        content_base_name: src.file_info.original.name.clone(),
        unique_id,
        is_content_adapter: src.file_info.kind == ssg_vfs::BundleKind::ContentAdapter,
    })
}

fn output_formats(
    model: &Model,
    media_types: &IdVec<FormatId, tera::Value>,
    p: &ssg_site::Page,
) -> tera::Value {
    let cfg = &model.config;
    let mut m = tera::Map::with_capacity(p.urls.len());
    for u in &p.urls {
        let Some(l) = &u.links else { continue };
        let f = cfg.output_formats.get(u.format);
        let v = OutputFormatView {
            name: f.name.clone(),
            rel: f.rel.clone(),
            media_type: media_types[u.format].clone(),
            permalink: l.permalink.to_string(),
            rel_permalink: l.rel_permalink.escaped(),
            is_plain_text: f.escaping == Escaping::Plain,
            is_html: f.is_html,
        };
        m.insert(f.name.clone().into(), tera::Value::from_serializable(&v));
    }
    tera::Value::from(m)
}

fn menu_entry(model: &Model, e: &MenuEntry) -> MenuEntryView {
    MenuEntryView {
        identifier: e.identifier.clone(),
        key_name: e.key_name().to_owned(),
        name: e.name.clone(),
        title: e.title.clone(),
        url: e.url.clone(),
        weight: e.weight,
        parent: e.parent.clone(),
        pre: tera::Value::safe_string(&e.pre),
        post: tera::Value::safe_string(&e.post),
        params: params_value(&e.params),
        page: e.page.map(|p| page_link(model, p)),
        children: e.children.iter().map(|c| menu_entry(model, c)).collect(),
        has_children: e.has_children(),
    }
}

/// A list of strings; the shared `empty` value when there are none.
fn strings(empty: &tera::Value, s: &[String]) -> tera::Value {
    if s.is_empty() {
        empty.clone()
    } else {
        tera::Value::from_serializable(s)
    }
}

/// A map of `a`'s entries followed by `b`'s, allocated once.
fn merged(a: &tera::Value, b: tera::Map) -> tera::Value {
    let a = a.as_map();
    let mut m = tera::Map::with_capacity(a.map_or(0, tera::Map::len) + b.len());
    if let Some(a) = a {
        m.extend(a.iter().map(|(k, v)| (k.clone(), v.clone())));
    }
    m.extend(b);
    tera::Value::from(m)
}

/// Values many pages share, serialised once: dates repeated within a page, sitemap settings.
#[derive(Default)]
struct Interner {
    sitemaps: std::sync::Mutex<Vec<(String, u64, bool, tera::Value)>>,
}

impl Interner {
    fn sitemap(&self, s: &ssg_config::sections::SitemapConfig) -> tera::Value {
        let mut all = self
            .sitemaps
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let key = (s.change_freq.as_str(), s.priority.to_bits(), s.disable);
        if let Some(v) = all
            .iter()
            .find(|(c, p, d, _)| (c.as_str(), *p, *d) == key)
            .map(|e| e.3.clone())
        {
            return v;
        }
        let v = tera::Value::from_serializable(&SitemapView {
            change_freq: s.change_freq.clone(),
            priority: s.priority,
            disable: s.disable,
        });
        all.push((s.change_freq.clone(), key.1, s.disable, v.clone()));
        v
    }
}

/// The four dates of a page; equal instants share one value.
fn dates(d: &ssg_page::Dates) -> [tera::Value; 4] {
    let all = [&d.date, &d.lastmod, &d.publish_date, &d.expiry_date];
    let mut out: [tera::Value; 4] = std::array::from_fn(|_| tera::Value::none());
    for i in 0..4 {
        let Some(z) = all[i] else { continue };
        out[i] = (0..i).find(|&j| all[j].as_ref() == Some(z)).map_or_else(
            || tera::Value::from_serializable(&DateView::new(z)),
            |j| out[j].clone(),
        );
    }
    out
}

/// `(prev, next)` in `list` (newest first): prev is the older neighbour.
fn neighbours(out: &mut IdVec<PageId, (Option<PageId>, Option<PageId>)>, list: &[PageId]) {
    for (i, &q) in list.iter().enumerate() {
        out[q] = (list.get(i + 1).copied(), i.checked_sub(1).map(|j| list[j]));
    }
}

impl Shared {
    fn new(inputs: ViewInputs) -> Result<Self, ViewError> {
        let ViewInputs {
            model,
            store,
            menus,
        } = inputs;
        let cfg = &model.config;
        let nav = NavSite::new(Arc::clone(&model));

        let resources = page_resources(&model, &store)?;
        let no_resources = tera::Value::from(Vec::<tera::Value>::new());
        let resource_values: Vec<tera::Value> = resources
            .iter()
            .map(|rs| {
                if rs.is_empty() {
                    return no_resources.clone();
                }
                tera::Value::from(
                    rs.iter()
                        .map(|r| tera::Value::from_serializable(&r.view))
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        let resources: IdVec<PageId, Vec<ResourceId>> = resources
            .into_iter()
            .map(|rs| rs.into_iter().map(|r| r.id).collect())
            .collect();

        let languages: IdVec<LangIdx, tera::Value> = cfg
            .sites
            .iter()
            .map(|s| tera::Value::from_serializable(&LanguageView::new(s)))
            .collect();
        let links: IdVec<PageId, tera::Value> = model
            .pages
            .ids()
            .map(|id| tera::Value::from_serializable(&page_link(&model, id)))
            .collect();

        let media_types: IdVec<FormatId, tera::Value> = cfg
            .output_formats
            .iter()
            .map(|(_, f)| {
                tera::Value::from_serializable(&MediaTypeView::new(
                    cfg.media_types.get(f.media_type),
                ))
            })
            .collect();
        let empty = tera::Value::from(Vec::<tera::Value>::new());
        let interner = Interner::default();
        let base: IdVec<PageId, tera::Value> = model
            .pages
            .as_slice()
            .par_iter()
            .zip(resource_values)
            .map(|(p, resources)| {
                let raw_content = tera::Value::from(p.source.as_ref().map_or("", |s| s.body()));
                let m = &p.meta;
                let [date, lastmod, publish_date, expiry_date] = dates(&m.dates);
                let site = &model.sites[p.lang];
                let mut terms = tera::Map::with_capacity(site.taxonomies.len());
                for (tx, t) in site.taxonomies.iter_enumerated() {
                    let links: Vec<tera::Value> = p
                        .terms
                        .iter()
                        .filter(|&&(x, _)| x == tx)
                        .map(|&(_, term)| links[t.terms[term].page].clone())
                        .collect();
                    let list = if links.is_empty() {
                        empty.clone()
                    } else {
                        tera::Value::from(links)
                    };
                    terms.insert(t.def.plural.clone().into(), list);
                }
                let (permalink, rel_permalink) = p.links().map_or_else(Default::default, |l| {
                    (l.permalink.to_string(), l.rel_permalink.escaped())
                });
                let bt = bundle_type(p);
                tera::Value::from_serializable(&PageSummaryView {
                    id: p.id.raw(),
                    kind: p.kind,
                    lang: cfg.sites[p.lang].language.key.clone(),
                    path: p.path(),
                    section: p.section.clone(),
                    r#type: p.r#type.clone(),
                    layout: m.layout.clone(),
                    bundle_type: (!bt.is_empty()).then_some(bt),
                    name: model.page_name(p.id).to_owned(),
                    title: p.title.clone(),
                    link_title: p.link_title.clone(),
                    description: m.description.clone(),
                    date,
                    lastmod,
                    publish_date,
                    expiry_date,
                    weight: m.weight,
                    draft: m.draft,
                    params: params_value(&m.params),
                    keywords: strings(&empty, &m.keywords),
                    aliases: strings(&empty, &m.aliases),
                    permalink,
                    rel_permalink,
                    is_home: p.kind == PageKind::Home,
                    is_section: p.kind == PageKind::Section,
                    is_page: p.kind == PageKind::Page,
                    is_node: p.kind.is_branch() || p.kind == PageKind::NotFound,
                    is_translated: p.translations.len() > 1,
                    file: file_view(p),
                    git_info: None,
                    sitemap: interner.sitemap(&m.sitemap),
                    language: languages[p.lang].clone(),
                    output_formats: output_formats(&model, &media_types, p),
                    resources,
                    terms: tera::Value::from(terms),
                    raw_content,
                })
            })
            .collect::<Vec<_>>()
            .into();

        let mut prev_next = model.pages.iter().map(|_| (None, None)).collect();
        for s in &model.sites {
            neighbours(&mut prev_next, &s.regular_pages);
        }
        let mut prev_next_in_section = model.pages.iter().map(|_| (None, None)).collect();
        for p in &model.pages {
            if matches!(p.kind, PageKind::Section | PageKind::Home) {
                neighbours(&mut prev_next_in_section, &p.regular_pages);
            }
        }

        let all_pages = model
            .sites
            .iter()
            .flat_map(|s| s.pages.iter().copied())
            .collect();
        let all_languages = tera::Value::from(languages.as_slice());
        let data = ssg_base::Value::Map(Arc::clone(&model.data)).to_tera();
        let configs = cfg
            .sites
            .iter()
            .map(|s| tera::Value::from_serializable(&SiteConfigView::new(cfg, s)))
            .collect();
        let menu_values = cfg
            .sites
            .ids()
            .map(|l| {
                let mut m = tera::Map::new();
                if let Some(sm) = menus.0.get(l) {
                    for (name, entries) in &sm.menus {
                        let list: Vec<MenuEntryView> =
                            entries.iter().map(|e| menu_entry(&model, e)).collect();
                        m.insert(name.clone().into(), tera::Value::from_serializable(&list));
                    }
                }
                tera::Value::from(m)
            })
            .collect();
        let sitemap_abs_urls = cfg
            .sites
            .ids()
            .map(|l| {
                model
                    .pages
                    .iter()
                    .find(|p| p.lang == l && p.kind == PageKind::Sitemap)
                    .and_then(|p| p.links())
                    .map(|l| l.permalink.to_string())
            })
            .collect();

        Ok(Self {
            model,
            nav,
            store,
            menus,
            resources,
            base,
            empty,
            links,
            prev_next,
            prev_next_in_section,
            all_pages,
            languages,
            all_languages,
            data,
            configs,
            menu_values,
            sitemap_abs_urls,
        })
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

impl ViewGeneration {
    /// A generation: the Meta one (`contents: None`) or the Full one of a variant.
    fn build(shared: &Arc<Shared>, contents: Option<&Contents>, memo: &ContentMemo) -> Self {
        let model = &shared.model;
        let summaries: IdVec<PageId, tera::Value> = match contents {
            None => shared.base.clone(),
            Some(c) => model
                .pages
                .as_slice()
                .par_iter()
                .map(|p| {
                    let content = memo.content(c.get(p.id).and_then(Option::as_ref));
                    merged(&shared.base[p.id], content)
                })
                .collect::<Vec<_>>()
                .into(),
        };
        let full = model.pages.iter().map(|_| OnceLock::new()).collect();
        let mut g = Self {
            shared: Arc::clone(shared),
            summaries,
            links: shared.links.clone(),
            full,
            terms: IdVec::new(),
            sites: IdVec::new(),
        };
        g.terms = model
            .sites
            .iter()
            .map(|site| {
                site.taxonomies
                    .iter()
                    .map(|t| {
                        t.listed_terms(model)
                            .map(|(_, term)| {
                                let key = t.key_of(term);
                                let pages: Vec<PageId> =
                                    term.members.iter().map(|w| w.page).collect();
                                let v = TermEntryView {
                                    name: term.term.clone(),
                                    key: key.clone(),
                                    count: pages.len(),
                                    page: g.summaries[term.page].clone(),
                                    pages: g.list(&pages),
                                };
                                (key, tera::Value::from_serializable(&v))
                            })
                            .collect()
                    })
                    .collect()
            })
            .collect();
        g.sites = model.sites.ids().map(|l| g.site_value(l)).collect();
        g
    }

    /// A list value of the summaries of `ids`.
    #[must_use]
    pub fn list(&self, ids: &[PageId]) -> tera::Value {
        if ids.is_empty() {
            return self.shared.empty.clone();
        }
        tera::Value::from(
            ids.iter()
                .map(|&p| self.summaries[p].clone())
                .collect::<Vec<_>>(),
        )
    }

    /// The summary of `id`, or none.
    #[must_use]
    pub fn opt(&self, id: Option<PageId>) -> tera::Value {
        id.map_or_else(tera::Value::none, |p| self.summaries[p].clone())
    }

    fn site_value(&self, lang: LangIdx) -> tera::Value {
        let sh = &self.shared;
        let model = &sh.model;
        let cfg = &model.config;
        let site_cfg = &cfg.sites[lang];
        let site = &model.sites[lang];
        let mut taxonomies = tera::Map::new();
        for (t, terms) in site.taxonomies.iter().zip(&self.terms[lang]) {
            let mut m = tera::Map::new();
            for (key, v) in terms {
                m.insert(key.clone().into(), v.clone());
            }
            taxonomies.insert(t.def.plural.clone().into(), tera::Value::from(m));
        }
        tera::Value::from_serializable(&SiteView {
            title: site_cfg.title.clone(),
            base_url: site_cfg.base_url.as_str().to_owned(),
            lang: site_cfg.language.key.clone(),
            language_code: site_cfg.language.code.clone(),
            language: sh.languages[lang].clone(),
            languages: sh.all_languages.clone(),
            is_multilingual: cfg.sites.len() > 1,
            copyright: site_cfg.copyright.clone(),
            params: params_value(&site_cfg.params),
            data: sh.data.clone(),
            home: self.summaries[site.home].clone(),
            pages: self.list(&site.pages),
            regular_pages: self.list(&site.regular_pages),
            all_pages: self.list(&sh.all_pages),
            sections: self.list(&model.pages[site.home].sections),
            main_sections: site.main_sections.clone(),
            taxonomies: tera::Value::from(taxonomies),
            menus: sh.menu_values[lang].clone(),
            last_mod: site.last_mod.as_ref().map(DateView::new),
            config: sh.configs[lang].clone(),
            sitemap_abs_url: sh.sitemap_abs_urls[lang].clone(),
            server_port: site_cfg.base_url.port().unwrap_or(0),
        })
    }

    /// The full value of page `id`: its summary plus its relations (built on first use).
    #[must_use]
    pub fn full(&self, id: PageId) -> tera::Value {
        self.full[id].get_or_init(|| self.full_value(id)).clone()
    }

    /// The full value of page `id` without keeping it: what a layout job renders its own page
    /// with (the value is the job's; [`full`](Self::full) keeps the values `deref` and
    /// `get_page` share). Equal to `full(id)`; the cached one is reused when it exists.
    #[must_use]
    pub fn page_value(&self, id: PageId) -> tera::Value {
        if let Some(v) = self.full[id].get() {
            return v.clone();
        }
        self.full_value(id)
    }

    /// `.RegularPagesRecursive`, as Go: for the home page and a section, the regular pages
    /// below it that are listed locally (so `build.list = "local"` pages of nested sections
    /// too, which `site.regular_pages` leaves out); else `.RegularPages`.
    fn regular_pages_recursive(&self, id: PageId) -> Vec<PageId> {
        let model = &self.shared.model;
        let p = &model.pages[id];
        let local = &model.sites[p.lang].regular_pages_local;
        match p.kind {
            PageKind::Home => local.clone(),
            PageKind::Section => local
                .iter()
                .copied()
                .filter(|&q| model.is_ancestor(id, q))
                .collect(),
            _ => p.regular_pages.clone(),
        }
    }

    fn full_value(&self, id: PageId) -> tera::Value {
        let sh = &self.shared;
        let model = &sh.model;
        let p = &model.pages[id];
        let site = &model.sites[p.lang];
        let summary = &self.summaries[id];
        let taxonomy = p
            .taxonomy
            .filter(|_| p.kind == PageKind::Taxonomy)
            .map(|t| {
                let def = &site.taxonomies[t].def;
                TaxonomyView {
                    singular: def.singular.clone(),
                    plural: def.plural.clone(),
                    terms: tera::Value::from(
                        self.terms[p.lang][t]
                            .iter()
                            .map(|(_, v)| v.clone())
                            .collect::<Vec<_>>(),
                    ),
                }
            });
        let term = match (p.kind, p.taxonomy, p.term) {
            (PageKind::Term, Some(t), Some(term)) => {
                let tx = &site.taxonomies[t];
                let term = &tx.terms[term];
                Some(TermView {
                    name: p.name().to_owned(),
                    term: term.term.clone(),
                    key: tx.key_of(term),
                    singular: tx.def.singular.clone(),
                    plural: tx.def.plural.clone(),
                })
            }
            _ => None,
        };
        // Every output format of the page but the first, less the `notAlternative` ones.
        let formats = summary
            .as_map()
            .and_then(|m| m.get(&tera::value::Key::Str("output_formats")))
            .and_then(tera::Value::as_map);
        let alternative = p
            .urls
            .iter()
            .filter(|u| u.links.is_some())
            .skip(1)
            .map(|u| model.config.output_formats.get(u.format))
            .filter(|f| f.listing != Listing::NotAlternative)
            .filter_map(|f| formats?.get(&tera::value::Key::Str(&f.name)).cloned())
            .collect::<Vec<_>>();
        let translations: Vec<PageId> = p
            .translations
            .iter()
            .copied()
            .filter(|&q| q != id)
            .collect();
        let (prev, next) = sh.prev_next[id];
        let (prev_in_section, next_in_section) = sh.prev_next_in_section[id];
        let r = PageRelations {
            parent: p.parent.map(|q| self.summaries[q].clone()),
            current_section: self.summaries[p.current_section].clone(),
            first_section: self.summaries[p.first_section].clone(),
            ancestors: self.list(&p.ancestors),
            pages: self.list(&p.pages),
            regular_pages: self.list(&p.regular_pages),
            regular_pages_recursive: self.list(&self.regular_pages_recursive(id)),
            sections: self.list(&p.sections),
            prev: prev.map(|q| self.summaries[q].clone()),
            next: next.map(|q| self.summaries[q].clone()),
            prev_in_section: prev_in_section.map(|q| self.summaries[q].clone()),
            next_in_section: next_in_section.map(|q| self.summaries[q].clone()),
            translations: self.list(&translations),
            all_translations: self.list(&p.translations),
            alternative_output_formats: tera::Value::from(alternative),
            taxonomy,
            term,
        };
        merged(summary, to_map(&r))
    }

    /// The model the values were built from.
    #[must_use]
    pub fn model(&self) -> &Arc<Model> {
        &self.shared.model
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
