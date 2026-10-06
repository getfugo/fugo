# ssg-fonts

`[fonts]`: the fonts of the published site cut down to the characters it uses, after the last
file is written (build phase E6a, `crates/build/src/fonts.rs`). An icon font such as Font
Awesome keeps the icons the site shows and goes from about 150 KB to 3 KB.

User documentation: `docs/content/asset-pipelines/fonts.md`.

## API

```rust
pub fn settings(cfg: &Config) -> Result<Option<FontsConfig>, FontsError>;
pub struct FontsConfig { pub rules: Vec<Rule> }          // [[fonts.subset]] entries
pub struct Rule { pub patterns, globs, pub from: Source, pub keep: String }
pub enum Source { Text, Content }

pub struct Recorder;                                     // a Sink that notes what goes through it
impl Recorder { pub fn new(inner: Arc<dyn Sink>, text: bool) -> Self; pub fn note(&self, path, bytes); … }
pub fn subset_fonts(config: &FontsConfig, recorder: &Recorder, published: &[OutputPath])
    -> Result<Subsetted, FontsError>;                    // cuts, warnings
pub fn cut(bytes: &[u8], chars: &BTreeSet<char>) -> Result<Outcome, SubsetError>;
pub enum Outcome { Cut(Vec<u8>), Unused, Whole(Option<String>) }
pub struct CssStrings;                                   // add(css), merge(other), chars()
pub fn add_html_text(html: &str, out: &mut BTreeSet<char>);
pub fn encode_woff(sfnt: &[u8]) -> Result<Vec<u8>, SubsetError>;
```

## Behaviour

**Which characters** (`src/css.rs`, `src/chars.rs`). A `content` entry keeps the characters of
the quoted strings of `content` and `quotes` declarations and of the custom properties they use
through `var()` (directly or through other custom properties, across files: `CssStrings`), CSS
escapes decoded, but not the strings of `url()` (images). The text of every HTML and CSS file is
scanned as it is (no CSS parser: a `style` attribute counts, and so does text that only looks
like a declaration). Custom properties no `content` uses do not count: Bootstrap's
`--bs-font-sans-serif: "Segoe UI"` would keep an icon font's letter glyphs. A `text` entry also keeps the
pages' text (html5gum tokens, character references decoded) outside `<script>` and `<style>`,
and the `alt`, `placeholder`, `title` and `value` attributes. `keep` adds its characters. An
extra character costs a glyph; a missed one loses a glyph, so the scan errs on the side of
more.

**Which fonts** (`src/record.rs`, `src/lib.rs`). The `.ttf`, `.otf`, `.woff` and `.woff2`
files the recorder saw written and the static files (passed in, since E1 copies them around
the recorder) whose path an entry's globs match (`ssg_base::glob`, `*` stops at `/`); the first
entry that matches a font decides its characters. An entry that matches no font is a warning
(`fonts-no-match`).

**Cutting** (`src/subset.rs`). WOFF and WOFF2 are decoded to OpenType (allsorts'
`whole_font`). The characters the font maps go to klippa (`fontcull-klippa`, the fork of
fontations' klippa that crates.io carries) with HarfBuzz's defaults but for the layout
features: every feature is kept (a page may turn any on), every script, names 0–6 in English,
hinting, and HarfBuzz's default dropped tables (AAT, `kern`, Graphite, `DSIG`, `SVG `, …). The
result is written in the input's format: OpenType as is, WOFF2 by ttf2woff2 (the `glyf`/`loca`
transform, Brotli 11), WOFF by `src/woff.rs` (zlib per table, when smaller).

**Left as they are.** A font that maps none of the characters (`Outcome::Unused`); a result
that is not smaller (`Whole(None)`); and, with a warning (`fonts-left-whole`), a variable font
that would lose `GSUB` or `GPOS` (klippa 0.1 drops the layout tables of a font with variation
data, which it cannot cut down yet) and a WOFF2 font with CFF outlines (ttf2woff2 writes
TrueType outlines only).

## Gotchas

- A rebuild on disk starts from the whole font: E1 rewrites a static file whose bytes differ
  from its source, so the cut font of the last build is replaced before E6a cuts it again.
- Characters that only scripts add are not seen: `keep` lists them.
- A font is cut down after the pages that link it are written: a fingerprinted font's name and
  `integrity` follow the whole font (the docs page warns against `integrity` and long caching).
- A font collection (`.ttc`) is not a font here.
