//! `js.Build` for this port: bundles a script asset in process with rolldown.
//!
//! - [`JsBuildOptions`]: the typed `js.Build` options, decoded from the template's map.
//! - [`JsBuilder`]: `js.Build` itself: bundles an asset, resolving imports in the assets
//!   ([`Assets`]) before `node_modules`, with `@params` and external/linked source maps.
//!
//! rolldown does most of the work. The crate adds what `js.Build` does beyond it: Go's
//! assets-first resolution, esbuild's `inject`, external and `NODE_ENV` behaviour, CSS imports
//! (including `local-css` modules), TC39 decorators, and the `es5` target.

#![forbid(unsafe_code)]

mod build;
mod css;
mod executor;
mod lower;
mod options;
mod plugin;
mod resolve;
mod sourcemap;

pub use build::{Diagnostic, JsBuildError, JsBuildOutput, JsBuilder, Position, Source};
pub use options::{
    DropKind, Format, JsBuildOptions, Jsx, Loader, OptionsError, Platform, SourceMap, Target,
};
pub use resolve::{AssetEntry, Assets, MountedDirs, resolve_component};
pub use sourcemap::file_url;
