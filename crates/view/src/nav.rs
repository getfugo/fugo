//! The site [`Model`] as `ssg-nav` reads it ([`NavModel`]): menus, the alias plan, the
//! default pagination list and the related index all go through [`NavSite`].
//!
//! The facts `PageFacts` borrows that the model does not store in that shape (the outputs as
//! `(format, paths)` pairs, the escaped primary link, the bundle type) are computed once here.

use std::sync::Arc;

use ssg_base::{FormatId, IdVec, LangIdx, PageId};
use ssg_nav::{NavModel, PageFacts, Rendering};
use ssg_page::TargetPaths;
use ssg_site::{Model, PageRole};
use ssg_vfs::BundleKind;

/// What [`NavSite`] precomputes per page.
#[derive(Debug)]
struct Facts {
    rel_permalink: String,
    bundle_type: &'static str,
    outputs: Vec<(FormatId, TargetPaths)>,
}

/// The [`NavModel`] of a site [`Model`].
#[derive(Debug)]
pub struct NavSite {
    model: Arc<Model>,
    facts: IdVec<PageId, Facts>,
    tree: IdVec<LangIdx, Vec<PageId>>,
}

/// `leaf`, `branch` or empty: the bundle type of a page's file.
pub(crate) fn bundle_type(p: &ssg_site::Page) -> &'static str {
    if p.source.is_none() || p.role != PageRole::Standalone {
        return "";
    }
    match p.path_info.kind {
        BundleKind::Leaf => "leaf",
        BundleKind::Branch => "branch",
        _ => "",
    }
}

impl NavSite {
    /// The adapter of `model`.
    #[must_use]
    pub fn new(model: Arc<Model>) -> Self {
        let facts = model
            .pages
            .iter()
            .map(|p| Facts {
                rel_permalink: p
                    .links()
                    .map_or_else(String::new, |l| l.rel_permalink.escaped()),
                bundle_type: bundle_type(p),
                outputs: p.urls.iter().map(|u| (u.format, u.paths.clone())).collect(),
            })
            .collect();
        let tree = model
            .sites
            .iter()
            .map(|s| s.tree.iter().map(|(_, id)| id).collect())
            .collect();
        Self { model, facts, tree }
    }

    /// The model.
    #[must_use]
    pub fn model(&self) -> &Arc<Model> {
        &self.model
    }
}

impl NavModel for NavSite {
    fn page(&self, id: PageId) -> PageFacts<'_> {
        let p = &self.model.pages[id];
        let f = &self.facts[id];
        let m = &p.meta;
        PageFacts {
            id,
            lang: p.lang,
            kind: p.kind,
            lang_key: &self.model.config.sites[p.lang].language.key,
            section: &p.section,
            title: &p.title,
            link_title: &p.link_title,
            name: self.model.page_name(id),
            slug: m.slug.as_deref().unwrap_or_default(),
            description: &m.description,
            page_type: &p.r#type,
            layout: m.layout.as_deref().unwrap_or_default(),
            bundle_type: f.bundle_type,
            draft: m.draft,
            weight: m.weight,
            keywords: &m.keywords,
            aliases: &m.aliases,
            dates: &m.dates,
            params: &m.params,
            menus: &m.menus,
            rel_permalink: &f.rel_permalink,
            fragments: &[],
            outputs: &f.outputs,
            rendering: if p.rendered() && p.role == PageRole::Standalone {
                Rendering::Rendered
            } else {
                Rendering::NotRendered
            },
            list: m.build.list,
        }
    }

    fn tree_pages(&self, lang: LangIdx) -> Vec<PageId> {
        self.tree[lang].clone()
    }

    fn resolve_page_ref(&self, lang: LangIdx, reference: &str) -> Option<PageId> {
        self.model.get_page(lang, reference, None).ok().flatten()
    }

    fn is_ancestor(&self, ancestor: PageId, page: PageId) -> bool {
        self.model.is_ancestor(ancestor, page)
    }

    fn pages(&self, page: PageId) -> &[PageId] {
        &self.model.pages[page].pages
    }

    fn regular_pages(&self, page: PageId) -> &[PageId] {
        &self.model.pages[page].regular_pages
    }

    fn site_regular_pages(&self, lang: LangIdx) -> &[PageId] {
        &self.model.sites[lang].regular_pages
    }

    fn home(&self, lang: LangIdx) -> Option<PageId> {
        Some(self.model.sites[lang].home)
    }
}
