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

/// Coverage table: the coverage index of a glyph.
enum Coverage<'a> {
    /// Sorted glyph ids.
    List(&'a [u8]),
    /// `(start, end, start index)` ranges, sorted.
    Ranges(&'a [u8]),
}

impl<'a> Coverage<'a> {
    fn parse(t: &'a [u8]) -> Option<Self> {
        let n = usize::from(u16_at(t, 2)?);
        match u16_at(t, 0)? {
            1 => Some(Self::List(t.get(4..4 + 2 * n)?)),
            2 => Some(Self::Ranges(t.get(4..4 + 6 * n)?)),
            _ => None,
        }
    }

    fn index(&self, g: u16) -> Option<usize> {
        match self {
            Self::List(ids) => {
                let n = ids.len() / 2;
                let (mut lo, mut hi) = (0, n);
                while lo < hi {
                    let mid = usize::midpoint(lo, hi);
                    let v = u16_at(ids, 2 * mid)?;
                    if v < g {
                        lo = mid + 1;
                    } else if v > g {
                        hi = mid;
                    } else {
                        return Some(mid);
                    }
                }
                None
            }
            Self::Ranges(r) => {
                let n = r.len() / 6;
                // The last range starting at or before `g`.
                let (mut lo, mut hi) = (0, n);
                while lo < hi {
                    let mid = usize::midpoint(lo, hi);
                    if u16_at(r, 6 * mid)? <= g {
                        lo = mid + 1;
                    } else {
                        hi = mid;
                    }
                }
                let i = lo.checked_sub(1)?;
                let (start, end) = (u16_at(r, 6 * i)?, u16_at(r, 6 * i + 2)?);
                if g < start || g > end {
                    return None;
                }
                Some(usize::from(u16_at(r, 6 * i + 4)?) + usize::from(g - start))
            }
        }
    }
}

/// Class definition table: the class of a glyph (0 when not listed).
enum ClassDef<'a> {
    /// First glyph and one class per glyph.
    Array(u16, &'a [u8]),
    /// `(start, end, class)` ranges, sorted.
    Ranges(&'a [u8]),
}

impl<'a> ClassDef<'a> {
    fn parse(t: &'a [u8]) -> Option<Self> {
        match u16_at(t, 0)? {
            1 => {
                let n = usize::from(u16_at(t, 4)?);
                Some(Self::Array(u16_at(t, 2)?, t.get(6..6 + 2 * n)?))
            }
            2 => {
                let n = usize::from(u16_at(t, 2)?);
                Some(Self::Ranges(t.get(4..4 + 6 * n)?))
            }
            _ => None,
        }
    }

    fn class(&self, g: u16) -> usize {
        let found = match self {
            Self::Array(start, values) => g
                .checked_sub(*start)
                .and_then(|i| u16_at(values, 2 * usize::from(i))),
            Self::Ranges(r) => (0..r.len() / 6).find_map(|i| {
                let (start, end) = (u16_at(r, 6 * i)?, u16_at(r, 6 * i + 2)?);
                (g >= start && g <= end)
                    .then(|| u16_at(r, 6 * i + 4))
                    .flatten()
            }),
        };
        found.map_or(0, usize::from)
    }
}

/// One GPOS pair-adjustment subtable whose values are the first glyph's X advance only (the
/// only kind `x/image` reads).
enum PairTable<'a> {
    /// Format 1: per covered first glyph, `(second glyph, value)` records sorted by glyph.
    Glyphs {
        coverage: Coverage<'a>,
        sets: Vec<&'a [u8]>,
    },
    /// Format 2: a value per (class of the first glyph, class of the second).
    Classes {
        coverage: Coverage<'a>,
        first: ClassDef<'a>,
        second: ClassDef<'a>,
        second_count: usize,
        values: &'a [u8],
    },
}

impl<'a> PairTable<'a> {
    fn parse(t: &'a [u8]) -> Option<Self> {
        let coverage = Coverage::parse(t.get(usize::from(u16_at(t, 2)?)..)?)?;
        // valueFormat1 = X_ADVANCE, valueFormat2 = none.
        if u16_at(t, 4)? != 0x0004 || u16_at(t, 6)? != 0 {
            return None;
        }
        match u16_at(t, 0)? {
            1 => {
                let n = usize::from(u16_at(t, 8)?);
                let sets = (0..n)
                    .map(|i| {
                        let set = t.get(usize::from(u16_at(t, 10 + 2 * i)?)..)?;
                        let count = usize::from(u16_at(set, 0)?);
                        set.get(2..2 + 4 * count)
                    })
                    .collect::<Option<Vec<_>>>()?;
                Some(Self::Glyphs { coverage, sets })
            }
            2 => {
                let first = ClassDef::parse(t.get(usize::from(u16_at(t, 8)?)..)?)?;
                let second = ClassDef::parse(t.get(usize::from(u16_at(t, 10)?)..)?)?;
                let first_count = usize::from(u16_at(t, 12)?);
                let second_count = usize::from(u16_at(t, 14)?);
                let values = t.get(16..16 + 2 * first_count * second_count)?;
                Some(Self::Classes {
                    coverage,
                    first,
                    second,
                    second_count,
                    values,
                })
            }
            _ => None,
        }
    }

    /// The value of pair `(a, b)`, `None` when this subtable does not cover it (the next one
    /// is asked then).
    fn value(&self, a: u16, b: u16) -> Option<i16> {
        match self {
            Self::Glyphs { coverage, sets } => {
                let set = sets.get(coverage.index(a)?)?;
                for i in 0..set.len() / 4 {
                    let second = u16_at(set, 4 * i)?;
                    if second == b {
                        return i16_at(set, 4 * i + 2);
                    }
                    if second > b {
                        return None;
                    }
                }
                None
            }
            Self::Classes {
                coverage,
                first,
                second,
                second_count,
                values,
            } => {
                coverage.index(a)?;
                let i = first.class(a) * second_count + second.class(b);
                i16_at(values, 2 * i)
            }
        }
    }
}

/// The feature indices of the default language system of `script`.
fn script_features(gpos: &[u8], script: &[u8; 4]) -> Option<Vec<usize>> {
    let list = gpos.get(usize::from(u16_at(gpos, 4)?)..)?;
    let count = usize::from(u16_at(list, 0)?);
    let offset = (0..count).find_map(|i| {
        (list.get(2 + 6 * i..6 + 6 * i)? == script).then(|| u16_at(list, 6 + 6 * i))?
    })?;
    let script = list.get(usize::from(offset)..)?;
    let default = usize::from(u16_at(script, 0)?);
    if default == 0 {
        return None;
    }
    let langsys = script.get(default..)?;
    let n = usize::from(u16_at(langsys, 4)?);
    (0..n)
        .map(|i| u16_at(langsys, 6 + 2 * i).map(usize::from))
        .collect()
}

/// The pair tables of the GPOS `kern` feature, in lookup order (`x/image`'s subset).
fn gpos_kerning(gpos: &[u8]) -> Vec<PairTable<'_>> {
    let parse = || -> Option<Vec<PairTable<'_>>> {
        if u16_at(gpos, 0)? != 1 || u16_at(gpos, 2)? > 1 {
            return None;
        }
        let features = script_features(gpos, b"latn")
            .filter(|f| !f.is_empty())
            .or_else(|| script_features(gpos, b"DFLT"))?;
        let feature_list = gpos.get(usize::from(u16_at(gpos, 6)?)..)?;
        let lookup_list = gpos.get(usize::from(u16_at(gpos, 8)?)..)?;
        let mut lookups = Vec::new();
        for f in features {
            if feature_list.get(2 + 6 * f..6 + 6 * f)? != b"kern" {
                continue;
            }
            let feature = feature_list.get(usize::from(u16_at(feature_list, 6 + 6 * f)?)..)?;
            let n = usize::from(u16_at(feature, 2)?);
            for i in 0..n {
                lookups.push(usize::from(u16_at(feature, 4 + 2 * i)?));
            }
        }
        let mut tables = Vec::new();
        'lookups: for l in lookups {
            let lookup = lookup_list.get(usize::from(u16_at(lookup_list, 2 + 2 * l)?)..)?;
            let (kind, flags) = (u16_at(lookup, 0)?, u16_at(lookup, 2)?);
            let n = usize::from(u16_at(lookup, 4)?);
            let mut subtables = Vec::with_capacity(n);
            for i in 0..n {
                let sub = lookup.get(usize::from(u16_at(lookup, 6 + 2 * i)?)..)?;
                match kind {
                    2 => subtables.push(sub),
                    // Extension positioning: a 32-bit offset to the real subtable.
                    9 => {
                        if u16_at(sub, 0)? != 1 {
                            return None;
                        }
                        if u16_at(sub, 2)? != 2 {
                            continue 'lookups;
                        }
                        subtables.push(sub.get(usize::try_from(u32_at(sub, 4)?).ok()?..)?);
                    }
                    _ => continue 'lookups,
                }
            }
            // A mark filtering set is not supported.
            if flags & 0x0010 != 0 {
                continue;
            }
            tables.extend(subtables.into_iter().filter_map(PairTable::parse));
        }
        Some(tables)
    };
    parse().unwrap_or_default()
}

/// Where kerning values come from.
enum Kerning<'a> {
    Gpos(Vec<PairTable<'a>>),
    /// The first subtable of a version 0 `kern` table: `(left << 16 | right, value)` pairs.
    Table(&'a [u8]),
    None,
}

impl<'a> Kerning<'a> {
    fn of(font: &'a [u8]) -> Self {
        if let Some(gpos) = table(font, b"GPOS") {
            let tables = gpos_kerning(gpos);
            if !tables.is_empty() {
                return Self::Gpos(tables);
            }
        }
        let pairs = || -> Option<&'a [u8]> {
            let kern = table(font, b"kern")?;
            // Version 0, at least one subtable; the first one, horizontal, format 0.
            if u16_at(kern, 0)? != 0 || u16_at(kern, 2)? == 0 || u16_at(kern, 4)? != 0 {
                return None;
            }
            if *kern.get(8)? != 0 || *kern.get(9)? != 0x01 {
                return None;
            }
            let n = usize::from(u16_at(kern, 10)?);
            kern.get(18..18 + 6 * n)
        };
        pairs().map_or(Self::None, Self::Table)
    }

    /// The kerning of glyphs `a` then `b` in font units.
    fn value(&self, a: u16, b: u16) -> i16 {
        match self {
            Self::Gpos(tables) => tables.iter().find_map(|t| t.value(a, b)).unwrap_or(0),
            Self::Table(pairs) => {
                let key = u32::from(a) << 16 | u32::from(b);
                let (mut lo, mut hi) = (0, pairs.len() / 6);
                while lo < hi {
                    let mid = usize::midpoint(lo, hi);
                    let Some(k) = u32_at(pairs, 6 * mid) else {
                        return 0;
                    };
                    if k < key {
                        lo = mid + 1;
                    } else if k > key {
                        hi = mid;
                    } else {
                        return i16_at(pairs, 6 * mid + 4).unwrap_or(0);
                    }
                }
                0
            }
            Self::None => 0,
        }
    }
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
mod tests {
    use super::*;

    #[test]
    fn go_regular_metrics_are_go_s() {
        let font = FontData::go_regular();
        let face = Face::new(font.bytes(), 20.0, "Go Regular").expect("face");
        // hhea ascender 1935 of 2048 units at 20 px: 1935·1280/2048 = 1209.4 → 1209 (26.6).
        assert_eq!(face.ascent(), 1209);
        assert_eq!(ceil_26_6(face.ascent()), 19);
        // Go Regular has neither GPOS nor kern: no kerning.
        assert!(matches!(face.kerning, Kerning::None));
        // 'H' advances 1479 units → 1479·1280/2048 = 924.4 → 924.
        let h = face.glyph('H').expect("H");
        assert_eq!(face.advance(h), scale(1479 * 1280, 2048));
        // A glyph the font lacks is skipped.
        assert_eq!(face.measure("H\u{10FFFD}H"), face.measure("HH"));
    }

    #[test]
    fn gpos_kerning_of_a_real_font() {
        // The docs' opengraph font (Mulish Black): GPOS pair adjustments, no kern table.
        let path = ssg_testkit::fixture::legacy_docs().join("assets/opengraph/mulish-black.ttf");
        let bytes = std::fs::read(&path).expect("mulish-black.ttf");
        let face = Face::new(&bytes, 70.0, "mulish").expect("face");
        let Kerning::Gpos(tables) = &face.kerning else {
            panic!("no GPOS kerning found");
        };
        assert!(!tables.is_empty());
        let kern = |p: &str| {
            let mut c = p.chars();
            face.kern(c.next().expect("a"), c.next().expect("b"))
        };
        let kerned: Vec<(&str, i64)> = ["AV", "To", "Ty", "LT", "Yo", "Wa", "oo"]
            .into_iter()
            .map(|p| (p, kern(p)))
            .collect();
        assert!(
            kerned.iter().filter(|(_, k)| *k < 0).count() >= 4,
            "{kerned:?}"
        );
        // x/image's quirk: the value in font units is the 26.6 kerning, whatever the size.
        let small = Face::new(&bytes, 10.0, "mulish").expect("face");
        assert_eq!(small.kern('A', 'V'), kern("AV"));
        // The measured advance includes it.
        let (a, v) = (face.glyph('A').expect("A"), face.glyph('V').expect("V"));
        assert_eq!(
            face.measure("AV"),
            face.advance(a) + face.advance(v) + kern("AV")
        );
    }

    #[test]
    fn scaling_rounds_half_away_from_zero() {
        assert_eq!(scale(3 * 64, 2), 96);
        assert_eq!(scale(5, 2), 3);
        assert_eq!(scale(-5, 2), -3);
        assert_eq!(ceil_26_6(64), 1);
        assert_eq!(ceil_26_6(65), 2);
        assert_eq!(ceil_26_6(-65), -1);
    }
}
