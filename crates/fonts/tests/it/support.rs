//! Fonts to cut down, a sink that keeps files in memory, and reading a font back.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use allsorts::binary::read::ReadScope;
use allsorts::font_data::FontData;
use allsorts::tables::FontTableProvider;
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
        .table_data(fontcull_read_fonts::types::Tag::new(tag))
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
