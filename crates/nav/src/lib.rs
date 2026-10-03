//! Menus, pagination arithmetic and pager URLs, the related-content index and the alias plan
//! (docs/rust-port/REWRITE_PLAN.md §2.4, §3.3).
//!
//! Everything reads the site through [`NavModel`] (final titles, links, relations):
//!
//! - [`build_menus`] assembles the configured entries, the `sectionPagesMenu` and the pages'
//!   own entries into [`Menus`]; [`SiteMenus::is_menu_current`] and
//!   [`SiteMenus::has_menu_current`] answer the template queries.
//! - [`Pagination`] splits a page list (or page groups) into [`Pager`]s; [`pager_paths`] gives
//!   pager N's file and link, [`pager_alias`] the `page/1/` redirect, and
//!   [`default_pagination_list`] what `paginator()` paginates. The first call per page and
//!   format is recorded by the render session (`ssg-view`), not here.
//! - [`RelatedIndex`] is the inverted index of `[related]` over an explicit candidate list,
//!   searched with a [`RelatedQuery`].
//! - [`alias_plan`] lists the redirect files of front matter aliases and the main-language
//!   redirect.

#![forbid(unsafe_code)]

mod alias;
mod menu;
mod model;
mod pagination;
mod related;

pub use alias::{
    AliasKind, AliasPlan, AliasProblem, alias_plan, alias_target, language_redirect, page_aliases,
};
pub use menu::{
    MenuEntry, MenuOptions, Menus, SiteMenus, build_menus, build_site_menus, compare_names,
    menu_order, page_menu_entries, sort_by_name,
};
pub use model::{NavModel, PageFacts, Rendering};
pub use pagination::{
    GroupSlice, PageGroup, Pager, PagerSlice, Pagination, PaginationItems, default_pagination_list,
    pager_alias, pager_paths, resolve_pager_size,
};
pub use related::{Related, RelatedIndex, RelatedQuery, related};

/// Errors of menus, pagination, related content and aliases.
#[derive(Debug, thiserror::Error)]
pub enum NavError {
    #[error("the pager size must be a positive integer")]
    PagerSize,
    #[error("paginate takes at most one option (the pager size), got {0}")]
    PagerArgs(usize),
    #[error("related content: no index named {0:?} is configured")]
    UnknownIndex(String),
    #[error("related content: a named keyword list needs an index name")]
    InvalidIndexName,
    #[error("related content: index {index:?} cannot use a {kind} value as keywords")]
    UnsupportedKeyword { index: String, kind: &'static str },
    #[error("alias {alias:?}: {}", match problem {
        AliasProblem::Empty => "empty",
        AliasProblem::Root => "it would replace the home page",
        AliasProblem::Traversal => "it points outside the publish directory",
    })]
    Alias {
        alias: String,
        problem: AliasProblem,
    },
}
