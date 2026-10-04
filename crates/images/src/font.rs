//! Fonts of the text filter: TrueType/OpenType bytes with their identity, the default font,
//! and a face that measures, kerns and draws strings the way the Go implementation's does.
//!
//! The Go implementation draws text with `golang.org/x/image/font/opentype` (hinting none, 72 dpi)
//! and `font.Drawer`. This face reproduces what decides where glyphs land, so that line breaks and
//! alignment equal Go's:
//!
//! * sizes are 26.6 fixed point: `ppem = int(0.5 + size·64)`, and every font-unit value `v`
//!   is scaled to `round(v·ppem / unitsPerEm)` (half away from zero), as `sfnt` does;
//! * the ascent is the `hhea` ascender (never the `OS/2` typographic one);
//! * advances come from `hmtx`; glyphs a font lacks are skipped (no advance);
//! * kerning is `x/image`'s subset: GPOS pair adjustments of the `kern` feature of the `latn`
//!   script (else `DFLT`), else the version 0 `kern` table. `opentype.Face.Kern` passes
//!   `unitsPerEm` as the size, so a pair's value in font units is used as a 26.6 value, i.e.
//!   `value / 64` pixels whatever the size. That quirk is reproduced: it moves glyphs.
//!
//! Glyph outlines are rasterised with `ab_glyph` at the sub-pixel position of the pen
//! (its rasteriser and `x/image/vector` both descend from font-rs); coverage becomes an
//! 8-bit mask like `vector`'s (`uint8(255.99998·a)`) and is composited source-over.

use std::fmt;
use std::sync::{Arc, LazyLock};

use ab_glyph::{Font as _, FontRef, GlyphId, PxScale, point};
use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use xxhash_rust::xxh3::xxh3_64;

use crate::color::Color;
use crate::error::ImageError;
use crate::pixels::over;

mod kerning;

use kerning::*;

/// Go Regular, the font the Go implementation embeds (`golang.org/x/image/font/gofont/goregular`
/// v0.28.0; BSD-3-Clause, `THIRD_PARTY/gofont/LICENSE`).
static GO_REGULAR_TTF: &[u8] = include_bytes!("../../../THIRD_PARTY/gofont/Go-Regular.ttf");

static GO_REGULAR: LazyLock<FontData> = LazyLock::new(|| FontData::new(Arc::from(GO_REGULAR_TTF)));

/// The identity of a font: the xxh3 hash of its bytes. In template maps an integer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FontId(u64);

impl FontId {
    /// The id of these font bytes.
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        Self(xxh3_64(bytes))
    }

    /// The raw hash.
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

impl fmt::Display for FontId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

/// A font's bytes, shared, with their identity. Equality and `Debug` use the identity only,
/// so a plan that holds a font hashes (and prints) its id, not its bytes.
#[derive(Clone)]
pub(crate) struct FontData {
    id: FontId,
    bytes: Arc<[u8]>,
}

impl FontData {
    pub(crate) fn new(bytes: Arc<[u8]>) -> Self {
        Self {
            id: FontId::of(&bytes),
            bytes,
        }
    }

    /// The default font (Go Regular).
    pub(crate) fn go_regular() -> Self {
        GO_REGULAR.clone()
    }

    pub(crate) const fn id(&self) -> FontId {
        self.id
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Checks that the bytes are a font this module can use.
    pub(crate) fn validate(&self, what: &str) -> Result<(), ImageError> {
        Face::new(&self.bytes, 12.0, what).map(|_| ())
    }
}

impl PartialEq for FontData {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl fmt::Debug for FontData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "font {}", self.id)
    }
}

// ── the tables x/image reads ─────────────────────────────────────────────────────────────────

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*b.get(at)?, *b.get(at + 1)?]))
}

fn i16_at(b: &[u8], at: usize) -> Option<i16> {
    u16_at(b, at).map(|v| i16::from_be_bytes(v.to_be_bytes()))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes([
        *b.get(at)?,
        *b.get(at + 1)?,
        *b.get(at + 2)?,
        *b.get(at + 3)?,
    ]))
}

/// The bytes of table `tag` of a single (non-collection) font.
fn table<'a>(font: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
    let count = usize::from(u16_at(font, 4)?);
    (0..count).find_map(|i| {
        let rec = 12 + 16 * i;
        if font.get(rec..rec + 4)? != tag {
            return None;
        }
        let offset = usize::try_from(u32_at(font, rec + 8)?).ok()?;
        let len = usize::try_from(u32_at(font, rec + 12)?).ok()?;
        font.get(offset..offset.checked_add(len)?)
    })
}

// ── the face ─────────────────────────────────────────────────────────────────────────────────

/// `fixed.Int26_6.Ceil`.
pub(crate) const fn ceil_26_6(v: i64) -> i64 {
    (v + 0x3f) >> 6
}

/// `sfnt`'s scaling of a font-unit value `v · ppem` (26.6) by `units_per_em`: rounded half
/// away from zero.
const fn scale(v: i64, units_per_em: i64) -> i64 {
    let v = if v >= 0 {
        v + units_per_em / 2
    } else {
        v - units_per_em / 2
    };
    v / units_per_em
}

/// A font at a size (see the module documentation).
pub(crate) struct Face<'a> {
    font: FontRef<'a>,
    kerning: Kerning<'a>,
    units_per_em: i64,
    /// Pixels per em, 26.6.
    ppem: i64,
    /// The `hhea` ascender, 26.6.
    ascent: i64,
    /// The `ab_glyph` scale whose unit-to-pixel factor is `ppem / units_per_em`.
    px_scale: PxScale,
}

impl<'a> Face<'a> {
    /// `size` in pixels (72 dpi), positive.
    pub(crate) fn new(bytes: &'a [u8], size: f64, what: &str) -> Result<Self, ImageError> {
        let invalid = |reason: &str| ImageError::Font {
            what: what.to_owned(),
            reason: reason.to_owned(),
        };
        let font = FontRef::try_from_slice(bytes).map_err(|e| invalid(&e.to_string()))?;
        let units_per_em = table(bytes, b"head")
            .and_then(|h| u16_at(h, 18))
            .filter(|&u| u > 0)
            .ok_or_else(|| invalid("no units per em in the head table"))?;
        let ascender = table(bytes, b"hhea")
            .and_then(|h| i16_at(h, 4))
            .ok_or_else(|| invalid("no hhea table"))?;
        let units_per_em = i64::from(units_per_em);
        // `fixed.Int26_6(0.5 + size·dpi·64/72)` at 72 dpi.
        let ppem = (0.5 + size * 64.0) as i64;
        let height = font.height_unscaled();
        let px_per_unit = ppem as f32 / 64.0 / units_per_em as f32;
        Ok(Self {
            kerning: Kerning::of(bytes),
            font,
            units_per_em,
            ppem,
            ascent: scale(i64::from(ascender) * ppem, units_per_em),
            px_scale: PxScale::from(if height > 0.0 {
                px_per_unit * height
            } else {
                px_per_unit
            }),
        })
    }

    /// The ascent, 26.6.
    pub(crate) const fn ascent(&self) -> i64 {
        self.ascent
    }

    /// The glyph of `c`; `None` when the font has none (Go skips those).
    fn glyph(&self, c: char) -> Option<GlyphId> {
        let g = self.font.glyph_id(c);
        (g.0 != 0).then_some(g)
    }

    /// The advance of glyph `g`, 26.6.
    fn advance(&self, g: GlyphId) -> i64 {
        let units = self.font.h_advance_unscaled(g) as i64;
        scale(units * self.ppem, self.units_per_em)
    }

    /// The kerning of `a` then `b`, 26.6 (`x/image`'s quirk: font units as 26.6).
    fn kern(&self, a: char, b: char) -> i64 {
        let id = |c| self.font.glyph_id(c).0;
        i64::from(self.kerning.value(id(a), id(b)))
    }

    /// `font.MeasureString`: the advance of `s`, 26.6.
    pub(crate) fn measure(&self, s: &str) -> i64 {
        let mut advance = 0;
        let mut prev: Option<char> = None;
        for c in s.chars() {
            if let Some(p) = prev {
                advance += self.kern(p, c);
            }
            let Some(g) = self.glyph(c) else {
                continue;
            };
            advance += self.advance(g);
            prev = Some(c);
        }
        advance
    }

    /// `font.Drawer.DrawString` of `s` with the pen at `(x, y)` (26.6, `y` on the baseline),
    /// composited over `img` in `color`.
    pub(crate) fn draw(&self, img: &mut RgbaImage, s: &str, (mut x, y): (i64, i64), color: Color) {
        let (w, h) = (i64::from(img.width()), i64::from(img.height()));
        let mut prev: Option<char> = None;
        for c in s.chars() {
            if let Some(p) = prev {
                x += self.kern(p, c);
            }
            let Some(g) = self.glyph(c) else {
                continue;
            };
            let position = point(x as f32 / 64.0, y as f32 / 64.0);
            if let Some(outlined) = self
                .font
                .outline_glyph(g.with_scale_and_position(self.px_scale, position))
            {
                let bounds = outlined.px_bounds();
                let (x0, y0) = (bounds.min.x as i64, bounds.min.y as i64);
                outlined.draw(|gx, gy, coverage| {
                    let (px, py) = (x0 + i64::from(gx), y0 + i64::from(gy));
                    if px < 0 || py < 0 || px >= w || py >= h {
                        return;
                    }
                    // `x/image/vector`: `uint8(almost256 · a)`.
                    let mask = (coverage.clamp(0.0, 1.0) * 255.999_98) as u8;
                    if mask == 0 {
                        return;
                    }
                    let alpha = (u16::from(color.0[3]) * u16::from(mask) + 127) / 255;
                    let top = Rgba([
                        color.0[0],
                        color.0[1],
                        color.0[2],
                        u8::try_from(alpha).unwrap_or(u8::MAX),
                    ]);
                    let (Ok(px), Ok(py)) = (u32::try_from(px), u32::try_from(py)) else {
                        return;
                    };
                    let d = img.get_pixel_mut(px, py);
                    *d = over(*d, top);
                });
            }
            x += self.advance(g);
            prev = Some(c);
        }
    }
}

#[cfg(test)]
mod tests;
