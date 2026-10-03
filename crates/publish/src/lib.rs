//! Publishing: sinks, canonify, minify dispatch, URL-token extraction, held outputs and the
//! static sync (docs/rust-port/REWRITE_PLAN.md §2.6, §3.4).
//!
//! - [`DiskSink`] and [`MemorySink`] implement [`ssg_base::Sink`].
//! - [`Publisher::emit`] takes every rendered [`Output`]: canonify / relative URLs
//!   ([`canonify`]), LiveReload script, [`UrlTokens`], then either holds it
//!   (deferred placeholders: written unpatched, patched in the sink by
//!   [`Publisher::patch_held`]) or minifies and writes it.
//! - [`HtmlElements`] are the tags, classes and ids of an HTML output (for `purge_css`).
//! - [`UrlTokens`] are handed to the resource store by `ssg-build` (the store takes any
//!   iterator of `&str`, so it does not depend on this crate).
//! - [`sync_static_dir`] / [`sync_static`] copy the static mounts (phase E1).

#![forbid(unsafe_code)]

pub mod canonify;
mod elements;
pub mod livereload;
mod publisher;
mod sink;
mod static_sync;
mod tokens;

use std::path::{Path, PathBuf};

use ssg_base::LangIdx;
use ssg_base::paths::OutputPath;

pub use canonify::{Quoting, UrlRewriter};
pub use elements::HtmlElements;
pub use publisher::{
    Emitted, Output, PLACEHOLDER_PREFIXES, PublishSettings, Publisher, SiteLinks, page_names,
};
pub use sink::{DiskSink, MemorySink};
pub use static_sync::{StaticSyncOptions, sync_static, sync_static_dir};
pub use tokens::UrlTokens;

/// A publishing failure.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PublishError {
    #[error("writing {path}: {source}")]
    Write {
        path: OutputPath,
        source: std::io::Error,
    },
    /// A held output could not be read back from the sink.
    #[error("reading {path}: {source}")]
    Read {
        path: OutputPath,
        source: std::io::Error,
    },
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path}: placeholder {placeholder} was not replaced by the deferred wave")]
    UnresolvedPlaceholder {
        path: OutputPath,
        placeholder: String,
    },
    #[error("deferred replacements: {0}")]
    Placeholders(String),
    #[error("output of language {0:?}, which is not configured")]
    UnknownLanguage(LangIdx),
    #[error(transparent)]
    Minify(#[from] ssg_minify::MinifyError),
    #[error(transparent)]
    Vfs(#[from] ssg_vfs::VfsError),
}

impl PublishError {
    fn io(path: impl AsRef<Path>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.as_ref().to_owned(),
            source,
        }
    }
}
