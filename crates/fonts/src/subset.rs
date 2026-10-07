//! One font cut down to some characters, in its own format.
//!
//! A WOFF or WOFF2 font is decoded to an OpenType font first (allsorts). klippa (HarfBuzz's
//! subsetter, in Rust) keeps the glyphs of the characters, the glyphs that layout features reach
//! from them (ligatures, alternates, accents), and the layout tables, cut down to those glyphs.
//! The result is encoded back into the font's format: OpenType as is, WOFF2 with its
//! `glyf`/`loca` transform (ttf2woff2), WOFF with zlib ([`crate::woff`]). A font with CFF
//! outlines is left whole: klippa does not cut them down yet.

use std::borrow::Cow;
use std::collections::BTreeSet;

use allsorts::binary::read::ReadScope;
use allsorts::font_data::FontData;
use allsorts::tables::FontTableProvider;
use fontcull_klippa::{Plan, SubsetFlags, subset_font};
use fontcull_read_fonts::collections::IntSet;
use fontcull_read_fonts::types::{NameId, Tag};
use fontcull_read_fonts::{FontRef, TableProvider};

/// The tables HarfBuzz's subsetter drops by default (`hb-subset-input.cc`): layout tables it
/// does not cut down (AAT and the legacy `kern`), Graphite, and tables browsers ignore.
const DROP_TABLES: &[&[u8; 4]] = &[
    b"morx", b"mort", b"kerx", b"kern", b"JSTF", b"DSIG", b"EBDT", b"EBLC", b"EBSC", b"SVG ",
    b"PCLT", b"LTSH", b"Feat", b"Glat", b"Gloc", b"Silf", b"Sill",
];

/// The names kept (HarfBuzz's default): copyright, family, subfamily, unique id, full name,
/// version, PostScript name; in English (Windows, `0x0409`).
const NAME_IDS: std::ops::RangeInclusive<u16> = 0..=6;
const NAME_LANGUAGE: u16 = 0x0409;

/// A font file's format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// TrueType or OpenType (`.ttf`, `.otf`).
    OpenType,
    Woff,
    Woff2,
}

impl Format {
    /// The format of `bytes`, by their signature; `None` when they are not a font (a font
    /// collection is not one either).
    #[must_use]
    pub fn sniff(bytes: &[u8]) -> Option<Self> {
        match bytes.get(..4)? {
            [0, 1, 0, 0] | b"true" | b"OTTO" => Some(Self::OpenType),
            b"wOFF" => Some(Self::Woff),
            b"wOF2" => Some(Self::Woff2),
            _ => None,
        }
    }
}

/// What became of a font.
#[derive(Debug)]
pub enum Outcome {
    /// Cut down: the new bytes, smaller than the old ones.
    Cut(Vec<u8>),
    /// The site uses none of its characters: left as it is.
    Unused,
    /// Left whole, for the reason given (cutting it would lose something, or it would not get
    /// smaller).
    Whole(Option<String>),
}

/// Why a font could not be read or written.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct SubsetError(String);

impl SubsetError {
    pub(crate) fn new(what: &str, e: impl std::fmt::Display) -> Self {
        Self(format!("{what}: {e}"))
    }
}

/// Cuts the font `bytes` down to `chars`.
///
/// # Errors
/// Bytes that are not a font, or a font the subsetter or the encoder rejects.
pub fn cut(bytes: &[u8], chars: &BTreeSet<char>) -> Result<Outcome, SubsetError> {
    let format = Format::sniff(bytes)
        .ok_or_else(|| SubsetError("not a TrueType, OpenType, WOFF or WOFF2 font".to_owned()))?;
    let sfnt = match format {
        Format::OpenType => Cow::Borrowed(bytes),
        Format::Woff | Format::Woff2 => Cow::Owned(decode(bytes)?),
    };
    let font = FontRef::new(&sfnt).map_err(|e| SubsetError::new("not a font", e))?;
    let cmap = font.cmap().map_err(|e| SubsetError::new("no cmap", e))?;
    let mut unicodes = IntSet::<u32>::empty();
    for &c in chars {
        if cmap.map_codepoint(c).is_some() {
            unicodes.insert(u32::from(c));
        }
    }
    if unicodes.is_empty() {
        return Ok(Outcome::Unused);
    }
    // klippa 0.1 copies a `CFF ` or `CFF2` table whole while it renumbers the glyphs: each
    // character of the cut font would draw another glyph.
    let cff = [(b"CFF ", "CFF"), (b"CFF2", "CFF2")]
        .into_iter()
        .find_map(|(tag, name)| font.table_data(Tag::new(tag)).map(|_| name));
    if let Some(outlines) = cff {
        return Ok(Outcome::Whole(Some(format!(
            "a font with {outlines} outlines, which the subsetter cannot cut down yet"
        ))));
    }
    let plan = Plan::new(
        &IntSet::empty(),
        &unicodes,
        &font,
        SubsetFlags::default(),
        &DROP_TABLES.iter().map(|t| Tag::new(t)).collect(),
        &IntSet::all(),
        &IntSet::all(),
        &NAME_IDS.map(NameId::new).collect(),
        &std::iter::once(NAME_LANGUAGE).collect(),
    );
    let out = subset_font(&font, &plan).map_err(|e| SubsetError::new("subsetting", e))?;
    if let Some(reason) = lost_layout(&font, &out) {
        return Ok(Outcome::Whole(Some(reason)));
    }
    let encoded = match format {
        Format::OpenType => out,
        Format::Woff2 => ttf2woff2::encode(&out, ttf2woff2::BrotliQuality::default())
            .map_err(|e| SubsetError::new("writing WOFF2", e))?,
        Format::Woff => crate::woff::encode(&out)?,
    };
    if encoded.len() >= bytes.len() {
        return Ok(Outcome::Whole(None));
    }
    Ok(Outcome::Cut(encoded))
}

/// An OpenType font made of the tables of a WOFF or WOFF2 font.
fn decode(bytes: &[u8]) -> Result<Vec<u8>, SubsetError> {
    let font = ReadScope::new(bytes)
        .read::<FontData<'_>>()
        .map_err(|e| SubsetError::new("not a font", e))?;
    let provider = font
        .table_provider(0)
        .map_err(|e| SubsetError::new("reading the font", e))?;
    let tags = provider
        .table_tags()
        .ok_or_else(|| SubsetError("reading the font: no table directory".to_owned()))?;
    allsorts::subset::whole_font(&provider, &tags)
        .map_err(|e| SubsetError::new("decoding the font", e))
}

/// Why the cut font `out` must not replace `font`: klippa cannot yet cut down the layout tables
/// of a variable font (their variation data) and leaves them out, so the font would lose its
/// kerning, mark positioning or ligatures. In a font with fixed outlines a missing table is one
/// that nothing kept used.
fn lost_layout(font: &FontRef<'_>, out: &[u8]) -> Option<String> {
    // Only a variable font has an `fvar` table.
    font.table_data(Tag::new(b"fvar"))?;
    let cut = FontRef::new(out).ok()?;
    let lost: Vec<&str> = ["GSUB", "GPOS"]
        .into_iter()
        .filter(|t| {
            let tag = Tag::new_checked(t.as_bytes()).ok();
            tag.is_some_and(|tag| font.table_data(tag).is_some() && cut.table_data(tag).is_none())
        })
        .collect();
    (!lost.is_empty()).then(|| {
        format!(
            "a variable font whose {} table the subsetter cannot cut down yet (kerning, mark positioning, ligatures)",
            lost.join(" and ")
        )
    })
}
