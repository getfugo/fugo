//! The seam between `ssg-nav` and the site model: [`NavModel`] is what menus, pagination,
//! related content and the alias plan read about pages.
//!
//! The site `Model` implements it once its URLs and relations exist (T23b); the oracle tests
//! implement it over the page dumps of the Go fixtures.

use ssg_base::{FormatId, LangIdx, PageId, PageKind, Params};
use ssg_page::{Dates, ListMode, PageMenuEntry, TargetPaths};

/// Whether a page is written to disk (and so can be the target of an alias).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Rendering {
    #[default]
    Rendered,
    /// `build.render = never`/`link`, headless, or a bundled content file.
    NotRendered,
}

/// What `ssg-nav` reads about one page (all values final: after cascade, default titles
/// and URLs).
#[derive(Clone, Copy, Debug)]
pub struct PageFacts<'a> {
    pub id: PageId,
    pub lang: LangIdx,
    pub kind: PageKind,
    /// The language key (`en`), the value of the `lang` related-content index.
    pub lang_key: &'a str,
    /// The first section (`posts` for `/posts/2024/a`); empty at the root.
    pub section: &'a str,
    pub title: &'a str,
    pub link_title: &'a str,
    /// `.Name`: the title, or a bundled file's name.
    pub name: &'a str,
    pub slug: &'a str,
    pub description: &'a str,
    /// `.Type`.
    pub page_type: &'a str,
    pub layout: &'a str,
    /// `leaf`, `branch` or empty.
    pub bundle_type: &'a str,
    pub draft: bool,
    pub weight: i32,
    pub keywords: &'a [String],
    pub aliases: &'a [String],
    pub dates: &'a Dates,
    pub params: &'a Params,
    /// Front matter `menus`.
    pub menus: &'a [PageMenuEntry],
    /// `.RelPermalink` in the primary format (escaped); empty when the page has no link.
    pub rel_permalink: &'a str,
    /// Heading ids (`.Fragments.Identifiers`); empty before the content phase.
    pub fragments: &'a [String],
    /// The page's output formats with their paths, the primary format first.
    pub outputs: &'a [(FormatId, TargetPaths)],
    pub rendering: Rendering,
    /// `build.list`: only pages listed everywhere get section and front matter menu entries.
    pub list: ListMode,
}

/// The site model as `ssg-nav` sees it.
pub trait NavModel {
    /// The page `id`.
    fn page(&self, id: PageId) -> PageFacts<'_>;

    /// The pages of language `lang` in tree (walk) order (pages of their own, not bundled
    /// content files).
    fn tree_pages(&self, lang: LangIdx) -> Vec<PageId>;

    /// The page a menu entry's `pageRef` names (`get_page` semantics), in language `lang`.
    fn resolve_page_ref(&self, lang: LangIdx, reference: &str) -> Option<PageId>;

    /// Whether `ancestor` is an ancestor of `page` in the content tree (not the page itself).
    fn is_ancestor(&self, ancestor: PageId, page: PageId) -> bool;

    /// `.Pages` of a list page.
    fn pages(&self, page: PageId) -> &[PageId];

    /// `.RegularPages` of a list page.
    fn regular_pages(&self, page: PageId) -> &[PageId];

    /// `.Site.RegularPages` of language `lang`.
    fn site_regular_pages(&self, lang: LangIdx) -> &[PageId];

    /// The home page of language `lang`.
    fn home(&self, lang: LangIdx) -> Option<PageId>;
}
