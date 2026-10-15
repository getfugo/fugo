//! Image metadata (`[imaging] stripMetadata`, `copyright`, `author`, `titleFromFileName`): an
//! image without what it says about where, when and with what it was taken, and with the
//! notice the site writes into it instead.
//!
//! [`rewrite_metadata`] rewrites the container of a JPEG, PNG or WebP file without decoding its
//! pixels. It removes EXIF (camera, dates, GPS position), XMP, IPTC, comments, text chunks and
//! vendor data, and keeps what changes how the image looks: the colour profile and other colour
//! information. It writes an EXIF block with the orientation, the author and the copyright, and
//! an XMP packet with the title, the author and the copyright ([`Notice`]); [`strip_metadata`]
//! writes only the orientation. [`ImageMetadata`] is what a build does with the images it
//! publishes, and [`Credit`] the copyright notice and author an image gives itself.

use exif::{In, Reader, Tag};

mod credit;
mod jpeg;
mod notice;
mod png;
mod published;
mod webp;

pub use credit::{Credit, CreditError};
pub use notice::Notice;
pub use published::ImageMetadata;

use notice::Blocks;

/// The bytes of `bytes` without metadata but the orientation. `None` when they are not a
/// JPEG, PNG or WebP image or have no metadata to remove.
#[must_use]
pub fn strip_metadata(bytes: &[u8]) -> Option<Vec<u8>> {
    rewrite_metadata(bytes, &Notice::default())
}

/// The bytes of `bytes` with `notice` and the orientation as their only metadata. `None` when
/// they are not a JPEG, PNG or WebP image, or already are what this would write.
#[must_use]
pub fn rewrite_metadata(bytes: &[u8], notice: &Notice) -> Option<Vec<u8>> {
    let out = if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        jpeg::rewrite(bytes, notice)
    } else if bytes.starts_with(png::SIGNATURE) {
        png::rewrite(bytes, notice)
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        webp::rewrite(bytes, notice)
    } else {
        return None;
    };
    (out != bytes).then_some(out)
}

/// The orientation (2–8) of an EXIF block (TIFF data, optionally after JPEG's `Exif\0\0`);
/// `None` for the normal orientation, none or an unreadable block.
fn orientation(exif: &[u8]) -> Option<u8> {
    let tiff = exif.strip_prefix(b"Exif\0\0").unwrap_or(exif);
    let v = Reader::new()
        .read_raw(tiff.to_vec())
        .ok()?
        .get_field(Tag::Orientation, In::PRIMARY)?
        .value
        .get_uint(0)?;
    u8::try_from(v).ok().filter(|v| (2..=8).contains(v))
}
