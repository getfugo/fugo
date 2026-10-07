//! Fonts to cut down, a sink that keeps files in memory, and reading a font back.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use allsorts::binary::read::ReadScope;
use allsorts::cff::CFF;
use allsorts::font_data::FontData;
use allsorts::tables::FontTableProvider;
use fontcull_read_fonts::types::Tag;
use fontcull_read_fonts::{FontRef, TableProvider};
use ssg_base::Sink;
use ssg_base::paths::OutputPath;
use ssg_config::{CliOverrides, Config, LoadOptions};
use ssg_testkit::fixture::testdata_dir;

/// Mulish Black: TrueType outlines, kerning (`GPOS`) and ligatures (`GSUB`).
pub fn mulish() -> Vec<u8> {
    std::fs::read(testdata_dir().join("legacy-docs/assets/opengraph/mulish-black.ttf"))
        .expect("mulish-black.ttf")
}

/// Mulish as a variable font (a weight axis): its layout tables have variation data.
pub fn mulish_variable() -> Vec<u8> {
    std::fs::read(testdata_dir().join("legacy-docs/static/fonts/Mulish-VariableFont_wght.ttf"))
        .expect("Mulish-VariableFont_wght.ttf")
}

/// The tables of TrueType outlines and their instructions, and `maxp` (version 1.0 counts their
/// points).
const TRUETYPE: [&[u8; 4]; 6] = [b"glyf", b"loca", b"cvt ", b"fpgm", b"prep", b"maxp"];

/// Mulish Black with CFF outlines, like Font Awesome 7's fonts: its tables but `glyf`, `loca`
/// and the TrueType instructions, a version 0.5 `maxp`, and a `CFF ` table of as many glyphs,
/// each empty (`endchar`).
pub fn mulish_cff() -> Vec<u8> {
    let ttf = mulish();
    let font = FontRef::new(&ttf).expect("font");
    let glyphs = font.maxp().expect("maxp").num_glyphs();
    let mut tables: Vec<([u8; 4], Vec<u8>)> = font
        .table_directory
        .table_records()
        .iter()
        .map(|r| r.tag().to_be_bytes())
        .filter(|tag| !TRUETYPE.contains(&tag))
        .map(|tag| {
            let data = font.table_data(Tag::new(&tag)).expect("table");
            (tag, data.as_bytes().to_vec())
        })
        .collect();
    // Version 0.5, the `maxp` of a font with CFF outlines: the number of glyphs alone.
    let maxp = [&[0, 0, 0x50, 0][..], &glyphs.to_be_bytes()].concat();
    tables.push((*b"maxp", maxp));
    let cff = cff(glyphs);
    let parsed = ReadScope::new(&cff).read::<CFF<'_>>().expect("a CFF table");
    assert_eq!(
        parsed.fonts[0].char_strings_index.len(),
        usize::from(glyphs)
    );
    tables.push((*b"CFF ", cff));
    tables.sort_by_key(|(tag, _)| *tag);
    let mut out = b"OTTO".to_vec();
    out.extend(u16::try_from(tables.len()).expect("tables").to_be_bytes());
    out.extend([0; 6]); // the binary search hints, which no reader here uses
    let mut offset = 12 + 16 * tables.len();
    for (tag, data) in &tables {
        out.extend(tag);
        out.extend([0; 4]); // the checksum, which no reader here checks
        out.extend(u32::try_from(offset).expect("offset").to_be_bytes());
        out.extend(u32::try_from(data.len()).expect("length").to_be_bytes());
        offset += data.len().div_ceil(4) * 4;
    }
    for (_, data) in &tables {
        out.extend(data);
        out.resize(out.len().div_ceil(4) * 4, 0);
    }
    out
}

/// A `CFF ` table of `glyphs` empty glyphs: `.notdef`, then `g1`, `g2`, …
fn cff(glyphs: u16) -> Vec<u8> {
    let names: Vec<Vec<u8>> = (1..glyphs).map(|g| format!("g{g}").into_bytes()).collect();
    let header = [1, 0, 4, 4]; // version 1.0, offsets of 4 bytes
    let name = cff_index(&[b"MulishCFF".to_vec()]);
    let strings = cff_index(&names);
    let global_subrs = cff_index(&[]);
    // Charset format 2: one range of glyph names, from the first string of the String INDEX
    // (SID 391, after the standard strings).
    let mut charset = vec![2];
    charset.extend(391_u16.to_be_bytes());
    charset.extend((glyphs - 2).to_be_bytes());
    let charstrings = cff_index(&vec![vec![14]; usize::from(glyphs)]); // endchar
    let private = [139, 20]; // defaultWidthX 0
    // The offsets in the Top DICT are 5-byte integers: its length does not depend on them.
    let top_len = cff_index(&[top_dict(0, 0, 0)]).len();
    let charset_at = header.len() + name.len() + top_len + strings.len() + global_subrs.len();
    let charstrings_at = charset_at + charset.len();
    let private_at = charstrings_at + charstrings.len();
    let top = cff_index(&[top_dict(charset_at, charstrings_at, private_at)]);
    [
        &header[..],
        &name,
        &top,
        &strings,
        &global_subrs,
        &charset,
        &charstrings,
        &private,
    ]
    .concat()
}

/// A Top DICT: `charset`, `CharStrings` and the 2-byte `Private` DICT at these offsets.
fn top_dict(charset: usize, charstrings: usize, private: usize) -> Vec<u8> {
    let int = |n: usize| {
        let mut out = vec![29];
        out.extend(i32::try_from(n).expect("offset").to_be_bytes());
        out
    };
    [
        int(charset),
        vec![15],
        int(charstrings),
        vec![17],
        int(2),
        int(private),
        vec![18],
    ]
    .concat()
}

/// A CFF INDEX of `items`, with offsets of 4 bytes.
fn cff_index(items: &[Vec<u8>]) -> Vec<u8> {
    let mut out = u16::try_from(items.len())
        .expect("count")
        .to_be_bytes()
        .to_vec();
    if items.is_empty() {
        return out;
    }
    out.push(4);
    let mut offset = 1u32;
    out.extend(offset.to_be_bytes());
    for item in items {
        offset += u32::try_from(item.len()).expect("length");
        out.extend(offset.to_be_bytes());
    }
    for item in items {
        out.extend(item);
    }
    out
}

/// The OpenType font of a TrueType, OpenType, WOFF or WOFF2 font.
pub fn sfnt(bytes: &[u8]) -> Vec<u8> {
    let font = ReadScope::new(bytes)
        .read::<FontData<'_>>()
        .expect("a font");
    if let FontData::OpenType(_) = font {
        return bytes.to_vec();
    }
    let provider = font.table_provider(0).expect("tables");
    let tags = provider.table_tags().expect("tags");
    allsorts::subset::whole_font(&provider, &tags).expect("decoded")
}

/// The characters the font `bytes` maps to a glyph, among `of`.
pub fn mapped(bytes: &[u8], of: &str) -> String {
    let sfnt = sfnt(bytes);
    let font = FontRef::new(&sfnt).expect("font");
    let cmap = font.cmap().expect("cmap");
    of.chars()
        .filter(|&c| cmap.map_codepoint(c).is_some())
        .collect()
}

/// The glyphs of the font `bytes`.
pub fn glyph_count(bytes: &[u8]) -> u16 {
    let sfnt = sfnt(bytes);
    FontRef::new(&sfnt)
        .expect("font")
        .maxp()
        .expect("maxp")
        .num_glyphs()
}

/// Whether the font `bytes` has the table `tag`.
pub fn has_table(bytes: &[u8], tag: &[u8; 4]) -> bool {
    let sfnt = sfnt(bytes);
    FontRef::new(&sfnt)
        .expect("font")
        .table_data(Tag::new(tag))
        .is_some()
}

/// The configuration of a project whose `config.toml` is `toml`.
pub fn load(toml: &str) -> Config {
    let dir = tempfile::tempdir().expect("tempdir");
    write(
        dir.path(),
        "config.toml",
        &format!("baseURL = \"https://example.org/\"\n{toml}"),
    );
    ssg_config::load(&LoadOptions {
        source: dir.path().to_path_buf(),
        config_files: Vec::new(),
        cli: CliOverrides::default(),
        env: Vec::new(),
    })
    .expect("config")
}

fn write(dir: &Path, path: &str, text: &str) {
    std::fs::write(dir.join(path), text).expect("write");
}

/// Files in memory.
#[derive(Default)]
pub struct TestSink(pub Mutex<BTreeMap<OutputPath, Vec<u8>>>);

impl TestSink {
    pub fn get(&self, path: &str) -> Vec<u8> {
        self.0
            .lock()
            .expect("lock")
            .get(&OutputPath::new(path))
            .unwrap_or_else(|| panic!("no {path}"))
            .clone()
    }
}

impl Sink for TestSink {
    fn write(&self, path: &OutputPath, bytes: &[u8]) -> std::io::Result<()> {
        self.0
            .lock()
            .expect("lock")
            .insert(path.clone(), bytes.to_vec());
        Ok(())
    }

    fn exists(&self, path: &OutputPath) -> bool {
        self.0.lock().expect("lock").contains_key(path)
    }

    fn read(&self, path: &OutputPath) -> std::io::Result<Vec<u8>> {
        self.0
            .lock()
            .expect("lock")
            .get(path)
            .cloned()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, path.to_string()))
    }
}
