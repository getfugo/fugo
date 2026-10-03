//! Image processing: [`ImageSpec`], operations and [`ImageFilter`]s (text and dithering
//! included), codecs, the deferred [`ImageQueue`] and its cache (`[caches.images]`), EXIF
//! metadata, and QR code images ([`qr_png`]).
//!
//! A template asks for an operation (`resize "600x400 webp"`, `images.Filter`); the resource
//! layer calls [`ImageQueue::enqueue`], which plans it from the source's metadata and returns
//! the final file name and size at once. Build phase E6 calls [`ImageQueue::process`] for the
//! operations whose URLs were published; it decodes, runs the planned steps with rayon and
//! encodes (JPEG with [`jpeg`], a port of Go's encoder; PNG, GIF, TIFF and BMP with `image`;
//! WebP with libwebp). See the crate README for the accepted differences from the Go
//! implementation.

#![forbid(unsafe_code)]

#[macro_use]
mod named;

mod codec;
mod color;
mod dither;
mod error;
pub mod exif;
mod filter;
mod font;
mod format;
mod gift;
pub mod jpeg;
mod jpegdec;
mod pixels;
mod plan;
mod qr;
mod queue;
mod settings;
mod smartcrop;
mod spec;
mod text;

pub use color::Color;
pub use dither::{DitherMethod, DitherSpec};
pub use error::ImageError;
pub use exif::Exif;
pub use filter::{ImageFilter, ImageInput, MAX_PADDING, MemoryImage, PaddingSpec};
pub use font::FontId;
pub use format::ImageFormat;
pub use plan::{Size, cover_size, crop_rect, fit_size, resize_size, rotated_size};
pub use qr::{QrLevel, QrModules, qr_modules, qr_png};
pub use queue::{Enqueued, ImageCache, ImageQueue};
pub use settings::{DEFAULT_EXIF_EXCLUDE, ExifPart, ExifSettings, Imaging};
pub use spec::{Action, Anchor, Hint, ImageSpec, Resample, ResolvedSpec};
pub use text::{AlignX, AlignY, FontInput, TextSpec};

/// The size and format of an encoded image, from its header (`.Width` and `.Height` of an
/// image that is not processed).
///
/// # Errors
/// Bytes that are not an image in a supported format.
pub fn probe(bytes: &[u8], what: &str) -> Result<(Size, ImageFormat), ImageError> {
    codec::probe(bytes, what)
}

/// [`probe`] of a file; reads its header only.
///
/// # Errors
/// An unreadable file, or one that is not an image in a supported format.
pub fn probe_file(path: &std::path::Path) -> Result<(Size, ImageFormat), ImageError> {
    codec::probe_file(path)
}
