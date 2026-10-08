//! A font with CFF outlines cut down, by allsorts: klippa copies a `CFF ` table whole.
//!
//! allsorts writes the tables that draw the glyphs (`cmap`, `head`, `hhea`, `hmtx`, `maxp`,
//! `name`, `OS/2`, `post` and `CFF `) and no others: it does not cut layout tables down. A font
//! with any other table that changes what the text looks like (`GSUB`, `GPOS`, vertical
//! metrics, colour glyphs, …) is left whole; Font Awesome 7's fonts have none.
//!
//! allsorts keeps the String INDEX of the CFF table whole: the names of every glyph of the
//! font. The strings that no glyph kept and no Top DICT entry names are emptied, in their
//! places, so that no string id changes.

use std::borrow::Cow;
use std::collections::BTreeSet;

use allsorts::binary::read::ReadScope;
use allsorts::binary::write::{WriteBinary, WriteBuffer};
use allsorts::cff::{CFF, IndexU16, MaybeOwnedIndex, Operand, Operator};
use allsorts::error::ParseError;
use allsorts::font_data::FontData;
use allsorts::subset::{CmapTarget, SubsetProfile, subset, whole_font};
use allsorts::tables::FontTableProvider;
use allsorts::tag;
use fontcull_read_fonts::FontRef;

use crate::subset::{DROP_TABLES, SubsetError};

/// The tables allsorts writes.
const KEPT: &[&[u8; 4]] = &[
    b"CFF ", b"OS/2", b"cmap", b"head", b"hhea", b"hmtx", b"maxp", b"name", b"post",
];

/// Tables that can go, besides the ones HarfBuzz drops: TrueType's instructions and device
/// metrics, which CFF outlines do not use, and data no browser reads.
const UNUSED: &[&[u8; 4]] = &[
    b"cvt ", b"fpgm", b"prep", b"gasp", b"hdmx", b"VDMX", b"FFTM", b"meta",
];

/// The Top DICT entries of a font whose glyphs have names (not CIDs) that are string ids.
const STRING_OPERATORS: [Operator; 8] = [
    Operator::Version,
    Operator::Notice,
    Operator::Copyright,
    Operator::FullName,
    Operator::FamilyName,
    Operator::Weight,
    Operator::PostScript,
    Operator::BaseFontName,
];

/// The id of the first string of the String INDEX, after the 391 standard strings.
const FIRST_STRING: usize = 391;

/// Why `font` must be left whole: the tables it has that allsorts would leave out.
pub(crate) fn lost_tables(font: &FontRef<'_>) -> Option<String> {
    let lost: Vec<String> = font
        .table_directory
        .table_records()
        .iter()
        .map(|r| r.tag().to_be_bytes())
        .filter(|t| !KEPT.contains(&t) && !UNUSED.contains(&t) && !DROP_TABLES.contains(&t))
        .map(|t| String::from_utf8_lossy(&t).trim_end().to_owned())
        .collect();
    let (last, rest) = lost.split_last()?;
    let tables = if rest.is_empty() {
        format!("{last} table")
    } else {
        format!("{} and {last} tables", rest.join(", "))
    };
    Some(format!(
        "a font with CFF outlines whose {tables} the subsetter cannot cut down yet"
    ))
}

/// Cuts the OpenType font `sfnt`, which has CFF outlines, down to `.notdef` and `glyphs`.
pub(crate) fn cut(
    sfnt: &[u8],
    glyphs: impl IntoIterator<Item = u16>,
) -> Result<Vec<u8>, SubsetError> {
    let mut ids: Vec<u16> = std::iter::once(0).chain(glyphs).collect();
    ids.sort_unstable();
    ids.dedup();
    let font = ReadScope::new(sfnt)
        .read::<FontData<'_>>()
        .map_err(|e| SubsetError::new("not a font", e))?;
    let provider = font
        .table_provider(0)
        .map_err(|e| SubsetError::new("reading the font", e))?;
    let out = subset(
        &provider,
        &ids,
        &SubsetProfile::Minimal,
        CmapTarget::Unicode,
    )
    .map_err(|e| SubsetError::new("subsetting", e))?;
    without_unused_strings(&out)
}

/// The font `otf` with the strings of its CFF table that no glyph or Top DICT entry names
/// emptied.
fn without_unused_strings(otf: &[u8]) -> Result<Vec<u8>, SubsetError> {
    let read = |e: ParseError| SubsetError::new("reading the cut font", e);
    let font = ReadScope::new(otf).read::<FontData<'_>>().map_err(read)?;
    let provider = font
        .table_provider(0)
        .map_err(|e| SubsetError::new("reading the cut font", e))?;
    let data = provider.read_table_data(tag::CFF).map_err(read)?;
    let mut cff = ReadScope::new(&data).read::<CFF<'_>>().map_err(read)?;
    let [top] = cff.fonts.as_slice() else {
        return Ok(otf.to_vec());
    };
    // Its glyphs have CIDs, not names: few strings.
    if top.is_cid_keyed() {
        return Ok(otf.to_vec());
    }
    let glyphs = (1..top.char_strings_index.len()).filter_map(|g| u16::try_from(g).ok());
    let mut used: BTreeSet<usize> = glyphs
        .filter_map(|g| top.charset.id_for_glyph(g))
        .map(usize::from)
        .collect();
    for op in STRING_OPERATORS {
        if let Some([Operand::Integer(id)]) = top.top_dict.get(op) {
            used.extend(usize::try_from(*id).ok());
        }
    }
    let strings = &cff.string_index;
    let index = cff_index((0..strings.len()).map(|i| {
        let kept = used.contains(&(FIRST_STRING + i));
        kept.then(|| strings.read_object(i))
            .flatten()
            .unwrap_or_default()
    }))?;
    cff.string_index =
        MaybeOwnedIndex::Borrowed(ReadScope::new(&index).read::<IndexU16>().map_err(read)?);
    let mut table = WriteBuffer::new();
    CFF::write(&mut table, &cff).map_err(|e| SubsetError::new("writing CFF", e))?;
    let tags = provider
        .table_tags()
        .ok_or_else(|| SubsetError::new("reading the cut font", "no table directory"))?;
    let tables = WithCff {
        tables: &provider,
        cff: table.bytes(),
    };
    whole_font(&tables, &tags).map_err(|e| SubsetError::new("writing the font", e))
}

/// A CFF INDEX of `items`, whose offsets take as few bytes as they can (allsorts writes them
/// as they are).
fn cff_index<'a>(items: impl ExactSizeIterator<Item = &'a [u8]>) -> Result<Vec<u8>, SubsetError> {
    let too_big = |e| SubsetError::new("writing CFF", e);
    let mut out = u16::try_from(items.len())
        .map_err(too_big)?
        .to_be_bytes()
        .to_vec();
    let items: Vec<&[u8]> = items.collect();
    if items.is_empty() {
        return Ok(out);
    }
    let last = u32::try_from(1 + items.iter().map(|i| i.len()).sum::<usize>()).map_err(too_big)?;
    // 1 to 4 bytes: those of the last offset that are not leading zeros.
    let size = 4 - usize::try_from(last.leading_zeros() / 8).unwrap_or(0);
    out.push(u8::try_from(size).unwrap_or(4));
    let mut offset = 1_u32;
    out.extend(&offset.to_be_bytes()[4 - size..]);
    for item in &items {
        offset += u32::try_from(item.len()).map_err(too_big)?;
        out.extend(&offset.to_be_bytes()[4 - size..]);
    }
    for item in items {
        out.extend(item);
    }
    Ok(out)
}

/// The tables of a font, but its `CFF ` table.
struct WithCff<'a, P> {
    tables: &'a P,
    cff: &'a [u8],
}

impl<P: FontTableProvider> FontTableProvider for WithCff<'_, P> {
    fn table_data(&self, tag: u32) -> Result<Option<Cow<'_, [u8]>>, ParseError> {
        if tag == tag::CFF {
            return Ok(Some(Cow::Borrowed(self.cff)));
        }
        self.tables.table_data(tag)
    }

    fn has_table(&self, tag: u32) -> bool {
        tag == tag::CFF || self.tables.has_table(tag)
    }

    fn table_tags(&self) -> Option<Vec<u32>> {
        self.tables.table_tags()
    }
}
