//! Why a resource could not be made, read or published.

use super::*;

/// Why a resource could not be made, read or published.
#[derive(Debug, thiserror::Error)]
pub enum ResourceError {
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error(transparent)]
    Vfs(#[from] VfsError),
    #[error(transparent)]
    Glob(#[from] GlobError),
    #[error(transparent)]
    Image(#[from] ImageError),
    #[error("unsupported hash algorithm {0:?}: use md5, sha256, sha384 or sha512")]
    UnsupportedHash(String),
    /// Two calls in one language made different resources for one target path.
    #[error(
        "{target} is created twice in one language with different inputs: at {first} and at {second}"
    )]
    TargetConflict {
        target: OutputPath,
        first: CallSite,
        second: CallSite,
    },
    #[error(
        "resources to concatenate must have one media type: {first} is {first_type:?}, {other} is {other_type:?}"
    )]
    MixedMediaTypes {
        first: String,
        first_type: String,
        other: String,
        other_type: String,
    },
    #[error("an empty target path")]
    EmptyTarget,
    #[error("resource metadata entry {index}: {reason}")]
    Metadata { index: usize, reason: String },
    #[error("{0}: the image has no pixels to read (not a file or a processed image)")]
    NotAnImage(String),
    #[error("writing {path}: {source}")]
    Write { path: OutputPath, source: io::Error },
    /// A transform (`to_css`, `js_build`, …) failed.
    #[error("{resource}: {transform}: {source}")]
    Pipe {
        resource: String,
        transform: &'static str,
        source: Box<PipeError>,
    },
    /// An image resource's `copyright` or `author` param that cannot be written into it.
    #[error(transparent)]
    Credit(#[from] CreditError),
}

impl ResourceError {
    pub(super) fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
