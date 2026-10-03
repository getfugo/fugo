//! The [`ResourceStore`]: bundle, asset and remote resources, named-target resources, the
//! fingerprint transform, front-matter metadata and the `Resources` collection lookups, and
//! publishing by URL token (docs/rust-port/REWRITE_PLAN.md §2.4, §3.4, task T40).
//!
//! - [`ResourceStore`] owns every resource of a build in an arena indexed by
//!   [`ResourceId`](ssg_base::ResourceId). Resources are immutable ([`Resource`]); a
//!   transform, a metadata override or a processed image is a new resource.
//! - Identity: assets memoize on their path (per language on multihost sites), transforms on
//!   `(source, transform)`, and resources that *name* a target path (`from_string`, `concat`,
//!   `from_template_output`, `copy`) on that path: within one language a second call with other
//!   inputs is a [`ResourceError::TargetConflict`] naming both call sites; across languages the
//!   earlier language's resource wins.
//! - [`meta`]: front matter `resources` metadata (`src` globs, `name`/`title` with `:counter`,
//!   `params`) and `Get`/`GetMatch`/`Match`/`ByType` over any [`meta::Named`] list.
//! - [`ResourceStore::publish`] writes the resources a build references: bundle resources with
//!   [`PublishPolicy::Eager`], and every other resource whose permalink or relative permalink
//!   appears among the URL tokens the publisher extracted (raw, HTML- or JSON-escaped, or
//!   percent-encoded forms), or that the `publish` filter marked. Processed images go to the
//!   [`ImageQueue`](ssg_images::ImageQueue).
//! - [`ResourceStore::get_remote`]: `resources.GetRemote` over the `[caches.getresource]` file
//!   cache (entries are raw HTTP responses), with `[security.http]` checks, network fetches
//!   through `ureq`, and an importer of caches the Go build wrote (see [`remote`]).
//!
//! - [`pipes`]: the transforms beyond `fingerprint` (`minify`, `to_css` with grass, `js_build`
//!   with rolldown), computed lazily, all in process;
//!   `post_process` placeholders; `execute_as_template` (the template engine is a seam).

#![forbid(unsafe_code)]

mod gohash;
pub mod meta;
pub mod pipes;
mod publish;
pub mod remote;
mod store;

pub use pipes::{PipeError, PostProcessId, PpField, TemplateExecutor, Transform, TransformEnv};
pub use publish::PublishStats;
pub use remote::{RemoteConfig, RemoteError, RemoteOptions, cache_key, go_keys};
pub use store::{
    AdapterResource, Body, BundleResource, CallSite, HashAlgo, LangTarget, Origin, PublishPolicy,
    QrOptions, Resource, ResourceError, ResourceKind, ResourceStore, StoreConfig, qr_target,
};
