//! Views and render state (REWRITE_PLAN.md §2.5).
//!
//! - [`ViewCache`]: page and site values pre-serialised once as `tera::Value`s and shared
//!   through `Arc`: a Meta generation for the content phase and one Full generation per
//!   [`HookVariant`], frozen before any layout renders. Relation lists hold summary values;
//!   [`ViewGeneration::full`] gives a page's full value (summary + relations).
//! - [`views`]: the view structs and their documented keys.
//! - Resource values ([`resource_view`], [`post_processed_view`]) and pager values
//!   ([`pager_view`]).
//! - [`NavSite`]: the site model as `ssg-nav` reads it (menus, aliases, pagination lists,
//!   related content).
//! - Render state: [`RenderScope`] (carried as `__nh`; no thread-locals), the
//!   [`ContentRenderer`] callback, [`PageStores`], [`PaginationRecorder`],
//!   [`DeferredRegistry`].

#![forbid(unsafe_code)]

mod cache;
mod content;
mod nav;
mod pager;
mod pagination;
mod resource;
mod scope;
mod stores;
pub mod views;

pub use cache::{Contents, ViewCache, ViewError, ViewGeneration, ViewInputs};
pub use content::{
    ContentError, ContentRenderer, ExpandedSource, RenderStringOptions, RenderedContent,
};
pub use nav::NavSite;
pub use pager::{TargetError, page_target, pager_url, pager_view};
pub use pagination::{PaginationConflict, PaginationRecorder, Recorded};
pub use resource::{PageResource, page_resources, post_processed_view, resource_view};
pub use scope::{HookVariant, MAX_DEPTH, Phase, RenderScope, SCOPE_KEY, Stage};
pub use stores::{Deferred, DeferredRegistry, PageStores};
