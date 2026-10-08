//! WOFF 2.0 (W3C) by ttf2woff2: the tables after the `glyf`/`loca` transform, Brotli 11.
//!
//! ttf2woff2 refuses a font with CFF outlines by its flavour alone (`OTTO`), but WOFF2 stores
//! such a font as its tables, untransformed, which is what ttf2woff2 does with a font that has
//! no `glyf` table. So a CFF font is given to it as TrueType, and its flavour is set back in the
//! header it writes.

use std::borrow::Cow;

use crate::subset::SubsetError;

/// The WOFF2 font of the OpenType font `sfnt`.
///
/// # Errors
/// `sfnt` is not an OpenType font, or a TrueType one whose glyphs cannot be read.
pub fn encode(sfnt: &[u8]) -> Result<Vec<u8>, SubsetError> {
    let cff = sfnt.starts_with(b"OTTO");
    let mut input = Cow::Borrowed(sfnt);
    if cff {
        input.to_mut()[..4].copy_from_slice(&[0, 1, 0, 0]);
    }
    let mut out = ttf2woff2::encode(&input, ttf2woff2::BrotliQuality::default())
        .map_err(|e| SubsetError::new("writing WOFF2", e))?;
    if cff {
        // The header: `wOF2`, then the flavour.
        out[4..8].copy_from_slice(b"OTTO");
    }
    Ok(out)
}
