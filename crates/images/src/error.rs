//! The crate's error type.

use std::io;
use std::path::PathBuf;

use ssg_base::ImageOpId;

use crate::font::FontId;
use crate::format::ImageFormat;

/// Why an image could not be described, planned, processed or published.
#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    /// A processing spec (`"600x400 webp q75"` or typed kwargs) that is not valid.
    #[error("invalid image spec {spec:?}: {reason}")]
    Spec { spec: String, reason: String },
    /// An `[imaging]` setting that is not valid.
    #[error("invalid imaging setting `{key}`: {reason}")]
    Config { key: &'static str, reason: String },
    /// A colour that is not `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa`.
    #[error("invalid color {0:?}: expected 3, 4, 6 or 8 hexadecimal digits")]
    Color(String),
    /// A filter whose arguments cannot be applied to its input.
    #[error("{filter} filter: {reason}")]
    Filter {
        filter: &'static str,
        reason: String,
    },
    /// Reading a source or writing a cache or output file failed.
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    /// A source (or a cached intermediate) that the codecs cannot read.
    #[error("{what}: cannot decode image: {source}")]
    Decode {
        what: String,
        source: image::ImageError,
    },
    /// Encoding the result failed.
    #[error("cannot encode {format} image: {reason}")]
    Encode { format: ImageFormat, reason: String },
    /// An operation id this queue never handed out.
    #[error("unknown image operation {0}")]
    UnknownOp(ImageOpId),
    /// An image in memory this queue was never given (`ImageQueue::add_memory`).
    #[error("{0}: unknown image in memory")]
    UnknownMemory(String),
    /// The EXIF data of a source could not be read.
    #[error("{what}: cannot read EXIF data: {reason}")]
    Exif { what: String, reason: String },
    /// Bytes that are not a TrueType or OpenType font the text filter can use.
    #[error("{what}: not a usable font: {reason}")]
    Font { what: String, reason: String },
    /// A font id this queue never registered.
    #[error("unknown font {0}")]
    UnknownFont(FontId),
    /// A QR code that cannot be made.
    #[error("QR code: {0}")]
    Qr(String),
    /// Processing the image of a published file failed (after the templates ran, so no
    /// template position): the file, its planned size and format, and what it was made from.
    #[error("processing {target} ({width}x{height} {format} from {input}): {source}")]
    Process {
        target: String,
        width: u32,
        height: u32,
        format: ImageFormat,
        input: String,
        source: Box<ImageError>,
    },
}

impl ImageError {
    pub(crate) fn spec(spec: impl Into<String>, reason: impl Into<String>) -> Self {
        Self::Spec {
            spec: spec.into(),
            reason: reason.into(),
        }
    }

    pub(crate) fn filter(filter: &'static str, reason: impl Into<String>) -> Self {
        Self::Filter {
            filter,
            reason: reason.into(),
        }
    }

    pub(crate) fn io(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
