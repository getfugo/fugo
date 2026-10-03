//! Errors of the file system layer.

use std::path::PathBuf;

use ssg_base::glob::GlobError;

/// Why the mounts cannot be set up or a walk failed.
#[derive(Debug, thiserror::Error)]
pub enum VfsError {
    #[error("module.mounts[{index}]: target {target:?} is not under a component directory")]
    InvalidTarget { index: usize, target: String },
    #[error("mount {mount:?}: language {lang:?} is not configured")]
    UnknownLanguage { mount: String, lang: String },
    #[error("mount {mount:?}: {error}")]
    Glob {
        mount: String,
        #[source]
        error: GlobError,
    },
    #[error("ignoreFiles pattern {pattern:?}: {error}")]
    IgnorePattern {
        pattern: String,
        #[source]
        error: regex::Error,
    },
    #[error("theme {name:?} not found in {}", dir.display())]
    ThemeNotFound { name: String, dir: PathBuf },
    #[error("{}: file name is not UTF-8", path.display())]
    NonUtf8 { path: PathBuf },
    #[error("{}: {error}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        error: std::io::Error,
    },
}

impl VfsError {
    pub(crate) fn io(path: impl Into<PathBuf>, error: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            error,
        }
    }
}
