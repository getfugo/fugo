//! Per-page rules (docs/rust-port/REWRITE_PLAN.md §2.4, phase B2/B4): typed front matter,
//! dates, build policy, cascade, target paths and permalinks, the default sort order and
//! default titles.
//!
//! - [`capture_overrides`] reads front matter `kind`/`lang`/`path` before a page enters its
//!   content tree; [`Cascade`] hands front matter down the tree; [`meta_from_params`] decodes
//!   the reserved keys into [`PageMeta`] and resolves the dates with a [`DateResolver`].
//! - [`AdapterPage`] and [`meta_from_adapter`] do the same for the maps a content adapter
//!   passes to `add_page`.
//! - [`Markup::detect`] picks the content renderer.
//! - [`PermalinkPatterns`] compiles `[permalinks]`; [`target_paths`] gives a page's output
//!   file and link in one format, [`links`] its `.RelPermalink` and `.Permalink`.
//! - [`default_order`] is the default page order, [`default_title`] the title of pages
//!   without one.

#![forbid(unsafe_code)]

mod adapter;
mod build;
mod cascade;
mod dates;
mod error;
mod golayout;
mod markup;
mod meta;
mod paths;
mod permalink;
mod sort;
mod title;
mod value;

pub use adapter::{AdapterPage, meta_from_adapter};
pub use build::{BuildPolicy, ListMode, RenderMode};
pub use cascade::{Cascade, CascadeRule, CascadeTarget, MatchCtx};
pub use dates::{DateOutcome, DateResolver, Dates, FileCtx};
pub use error::PageError;
pub use golayout::{GoLayout, format_go_layout};
pub use markup::{Markup, MarkupSource};
pub use meta::{
    CaptureOverrides, Cjk, MetaCtx, PageMenuEntry, PageMeta, ResourceMetaRule, capture_overrides,
    meta_from_params,
};
pub use paths::{
    LangPrefix, Links, PathShape, ResourceBase, SourcePath, TargetPaths, UrlInputs, links,
    target_paths,
};
pub use permalink::{PermalinkCtx, PermalinkFile, PermalinkPattern, PermalinkPatterns};
pub use sort::{SortKey, default_order};
pub use title::default_title;
