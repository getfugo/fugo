//! Chroma styles: the bundled style files, entry inheritance and the CSS Chroma derives from
//! them (inline `style` attributes and class stylesheets).
//!
//! The rules (entry syntax, inheritance, synthesised line colours, CSS properties and their
//! compression) are Chroma's (`style.go`, `colour.go`, `formatters/html/html.go`, v2.19.0,
//! MIT), rewritten; the styles in `src/styles/` are Chroma's style files, converted to Rust
//! (crate README, "Lexer and style files").

use std::collections::BTreeMap;
use std::fmt::Write as _;

use crate::token::TokenType;

/// A style (`src/styles/`, Chroma's style files converted): its name and its entries in Chroma's
/// syntax (`bold #rrggbb bg:#rrggbb`).
pub(crate) struct StyleDef {
    pub name: &'static str,
    pub entries: &'static [(TokenType, &'static str)],
}

/// The style Chroma falls back to for an unknown name.
pub const FALLBACK_STYLE: &str = "swapoff";

/// An RGB colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Colour(u32);

impl Colour {
    /// `#rgb`, `#rrggbb` or one of Chroma's ANSI names (`#ansired`, `#red`, …).
    fn parse(s: &str) -> Option<Self> {
        let hex = ansi(s).unwrap_or_else(|| s.strip_prefix('#').unwrap_or(s));
        let expanded: String;
        let hex = if hex.len() == 3 {
            expanded = hex.chars().flat_map(|c| [c, c]).collect();
            expanded.as_str()
        } else {
            hex
        };
        u32::from_str_radix(hex, 16)
            .ok()
            .filter(|n| *n <= 0xff_ffff)
            .map(Self)
    }

    fn rgb(self) -> [u8; 3] {
        let [_, r, g, b] = self.0.to_be_bytes();
        [r, g, b]
    }

    fn brightness(self) -> f64 {
        let [r, g, b] = self.rgb();
        (f64::from(r) + f64::from(g) + f64::from(b)) / 255.0 / 3.0
    }

    /// Chroma's `Brighten` (a negative factor darkens), truncating like Go's `uint8(f)`.
    fn brighten(self, factor: f64) -> Self {
        let [r, g, b] = self.rgb().map(f64::from);
        let [r, g, b] = if factor < 0.0 {
            let f = factor + 1.0;
            [r * f, g * f, b * f]
        } else {
            [
                (255.0 - r) * factor + r,
                (255.0 - g) * factor + g,
                (255.0 - b) * factor + b,
            ]
        };
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "channels stay in 0..=255; Go truncates the same way"
        )]
        let c = |v: f64| u32::from(v as u8);
        Self(c(r) << 16 | c(g) << 8 | c(b))
    }

    fn brighten_or_darken(self, factor: f64) -> Self {
        if self.brightness() < 0.5 {
            self.brighten(factor)
        } else {
            self.brighten(-factor)
        }
    }
}

impl std::fmt::Display for Colour {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{:06x}", self.0)
    }
}

fn ansi(s: &str) -> Option<&'static str> {
    let name = s.strip_prefix('#')?;
    let name = name.strip_prefix("ansi").unwrap_or(name);
    Some(match name {
        "black" => "000000",
        "darkred" => "7f0000",
        "darkgreen" => "007f00",
        "brown" => "7f7fe0",
        "darkblue" => "00007f",
        "purple" => "7f007f",
        "teal" => "007f7f",
        "lightgray" => "e5e5e5",
        "darkgray" => "555555",
        "red" => "ff0000",
        "green" => "00ff00",
        "yellow" => "ffff00",
        "blue" => "0000ff",
        "fuchsia" => "ff00ff",
        "turquoise" => "00ffff",
        "white" => "ffffff",
        _ => return None,
    })
}

/// A style flag that is set, unset, or inherited.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Flag {
    #[default]
    Inherit,
    Yes,
    No,
}

/// The appearance of one token type.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StyleEntry {
    colour: Option<Colour>,
    background: Option<Colour>,
    border: Option<Colour>,
    bold: Flag,
    italic: Flag,
    underline: Flag,
    no_inherit: bool,
}

impl StyleEntry {
    /// Parses a Pygments-style entry (`bold #f00 bg:#000 noinherit`).
    fn parse(s: &str) -> Result<Self, String> {
        let mut e = Self::default();
        for part in s.split_whitespace() {
            match part {
                "italic" => e.italic = Flag::Yes,
                "noitalic" => e.italic = Flag::No,
                "bold" => e.bold = Flag::Yes,
                "nobold" => e.bold = Flag::No,
                "underline" => e.underline = Flag::Yes,
                "nounderline" => e.underline = Flag::No,
                "inherit" => e.no_inherit = false,
                "noinherit" => e.no_inherit = true,
                "bg:" => e.background = None,
                _ => {
                    let (slot, c) = if let Some(c) = part.strip_prefix("bg:") {
                        (&mut e.background, c)
                    } else if let Some(c) = part.strip_prefix("border:") {
                        (&mut e.border, c)
                    } else {
                        (&mut e.colour, part)
                    };
                    if !c.starts_with('#') {
                        return Err(format!("unknown style element {part:?}"));
                    }
                    *slot =
                        Some(Colour::parse(c).ok_or_else(|| format!("invalid colour {part:?}"))?);
                }
            }
        }
        Ok(e)
    }

    fn is_zero(&self) -> bool {
        *self == Self::default()
    }

    /// Chroma's `Inherit`: unset fields come from the ancestors, the last one first, until an
    /// entry says `noinherit`.
    fn inherit(self, ancestors: &[Self]) -> Self {
        let mut out = self;
        for a in ancestors.iter().rev() {
            if out.no_inherit {
                return out;
            }
            out.colour = out.colour.or(a.colour);
            out.background = out.background.or(a.background);
            out.border = out.border.or(a.border);
            if out.bold == Flag::Inherit {
                out.bold = a.bold;
            }
            if out.italic == Flag::Inherit {
                out.italic = a.italic;
            }
            if out.underline == Flag::Inherit {
                out.underline = a.underline;
            }
        }
        out
    }

    /// Chroma's `Sub`: the fields of `self` that differ from `e`.
    fn sub(self, e: Self) -> Self {
        fn keep<T: PartialEq + Default>(a: T, b: &T) -> T {
            if a == *b { T::default() } else { a }
        }
        Self {
            colour: keep(self.colour, &e.colour),
            background: keep(self.background, &e.background),
            border: keep(self.border, &e.border),
            bold: keep(self.bold, &e.bold),
            italic: keep(self.italic, &e.italic),
            underline: keep(self.underline, &e.underline),
            no_inherit: false,
        }
    }

    /// The CSS declarations of the entry (Chroma's `StyleEntryToCSS`).
    fn to_css(self) -> String {
        let mut parts = Vec::new();
        if let Some(c) = self.colour {
            parts.push(format!("color: {c}"));
        }
        if let Some(c) = self.background {
            parts.push(format!("background-color: {c}"));
        }
        if self.bold == Flag::Yes {
            parts.push("font-weight: bold".to_owned());
        }
        if self.italic == Flag::Yes {
            parts.push("font-style: italic".to_owned());
        }
        if self.underline == Flag::Yes {
            parts.push("text-decoration: underline".to_owned());
        }
        parts.join("; ")
    }
}

/// A named Chroma style.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    name: String,
    entries: BTreeMap<TokenType, StyleEntry>,
}

impl Style {
    /// A style from its definition.
    fn from_def(def: &StyleDef) -> Result<Self, String> {
        let mut entries = BTreeMap::new();
        for (ty, style) in def.entries {
            entries.insert(*ty, StyleEntry::parse(style)?);
        }
        Ok(Self {
            name: def.name.to_owned(),
            entries,
        })
    }

    fn raw(&self, t: TokenType) -> StyleEntry {
        let e = self.entries.get(&t).copied().unwrap_or_default();
        if !e.is_zero() {
            return e;
        }
        // Chroma synthesises line highlight and line number colours from the background.
        // An unset colour reads as white there (Go's `Colour(0) - 1`).
        let white = Colour(0xff_ffff);
        let bg = self
            .entries
            .get(&TokenType::Background)
            .copied()
            .unwrap_or_default();
        match t {
            TokenType::LineHighlight => StyleEntry {
                background: Some(bg.background.unwrap_or(white).brighten_or_darken(0.1)),
                ..StyleEntry::default()
            },
            TokenType::LineNumbers | TokenType::LineNumbersTable => StyleEntry {
                colour: Some(bg.colour.unwrap_or(white).brighten_or_darken(0.5)),
                ..StyleEntry::default()
            },
            _ => e,
        }
    }

    /// The effective entry of `t` (Chroma's `Style.Get`).
    fn get(&self, t: TokenType) -> StyleEntry {
        let or_zero = |t: Option<TokenType>| t.map(|t| self.raw(t)).unwrap_or_default();
        self.raw(t).inherit(&[
            self.raw(TokenType::Background),
            self.raw(TokenType::Text),
            or_zero(t.category()),
            or_zero(t.sub_category()),
        ])
    }
}

/// Every bundled style, by name.
#[derive(Debug)]
pub struct Styles(BTreeMap<String, Style>);

impl Styles {
    /// The bundled styles.
    ///
    /// # Panics
    /// When a bundled entry is malformed (checked by the crate's tests).
    #[must_use]
    pub fn bundled() -> Self {
        let styles = crate::styles::STYLES
            .iter()
            .map(|def| {
                let s = Style::from_def(def).unwrap_or_else(|e| panic!("bundled style: {e}"));
                (s.name.clone(), s)
            })
            .collect();
        Self(styles)
    }

    /// The style named exactly `name`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Style> {
        self.0.get(name)
    }

    /// The fallback style (Chroma's `swapoff`).
    ///
    /// # Panics
    /// Never: the fallback is bundled.
    #[must_use]
    pub fn fallback(&self) -> &Style {
        &self.0[FALLBACK_STYLE]
    }

    /// The names of every bundled style, sorted.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.0.keys().map(String::as_str)
    }
}

/// The formatter settings that change the CSS Chroma derives from a style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CssSettings {
    /// Every class, including those with no declarations (`WithAllClasses`).
    pub all_classes: bool,
    /// 0 or 8 write no `tab-size`.
    pub tab_width: i64,
    /// Lines are highlighted (the pre wrapper becomes a grid).
    pub highlight_lines: bool,
}

/// The CSS declarations of every token type Chroma writes (its `styleToCSS`), uncompressed.
pub(crate) fn css_map(style: &Style, s: CssSettings) -> BTreeMap<TokenType, String> {
    let mut classes = BTreeMap::new();
    let bg = style.get(TokenType::Background);
    for &t in TokenType::ALL.iter().filter(|t| t.is_standard()) {
        let mut entry = style.get(t);
        if t != TokenType::Background {
            entry = entry.sub(bg);
        }
        if !s.all_classes && entry.is_zero() {
            continue;
        }
        classes.insert(t, entry.to_css());
    }
    let tab = if s.tab_width != 0 && s.tab_width != 8 {
        let w = s.tab_width;
        format!("-moz-tab-size: {w}; -o-tab-size: {w}; tab-size: {w};")
    } else {
        String::new()
    };
    let background = classes.entry(TokenType::Background).or_default();
    background.push(';');
    background.push_str(&tab);
    let background = background.clone();
    let pre = classes.entry(TokenType::PreWrapper).or_default();
    pre.push_str(&background);
    if s.highlight_lines {
        pre.push_str("display: grid;");
    }
    let line_numbers = "white-space: pre; -webkit-user-select: none; user-select: none; margin-right: 0.4em; padding: 0 0.4em 0 0.4em;";
    let mut prepend = |t, css: &str| {
        let v = classes.entry(t).or_default();
        v.insert_str(0, css);
    };
    prepend(TokenType::Line, "display: flex;");
    prepend(TokenType::LineNumbers, line_numbers);
    prepend(TokenType::LineNumbersTable, line_numbers);
    prepend(
        TokenType::LineTable,
        "border-spacing: 0; padding: 0; margin: 0; border: 0;",
    );
    prepend(
        TokenType::LineTableTd,
        "vertical-align: top; padding: 0; margin: 0; border: 0;",
    );
    prepend(
        TokenType::LineLink,
        "outline: none; text-decoration: none; color: inherit",
    );
    classes
}

/// Chroma's `compressStyle`: no spaces after `:`, `#aabbcc` → `#abc`.
pub(crate) fn compress(css: &str) -> String {
    css.split(';')
        .map(|p| {
            let mut p = p.split_whitespace().collect::<Vec<_>>().join(" ");
            if let Some(at) = p.find(": ") {
                p.replace_range(at..at + 2, ":");
            }
            if p.contains('#') && p.len() >= 6 && p.is_char_boundary(p.len() - 6) {
                let c = &p.as_bytes()[p.len() - 6..];
                if c[0] == c[1] && c[2] == c[3] && c[4] == c[5] {
                    let short = [c[0], c[2], c[4]];
                    let short = String::from_utf8_lossy(&short).into_owned();
                    p.truncate(p.len() - 6);
                    p.push_str(&short);
                }
            }
            p
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// The inline declarations of every token type (compressed, as Chroma writes `style="…"`).
pub(crate) fn inline_map(style: &Style, s: CssSettings) -> BTreeMap<TokenType, String> {
    css_map(style, s)
        .into_iter()
        .map(|(t, css)| (t, compress(&compress(&css))))
        .collect()
}

/// How [`crate::Highlight::css`] writes a stylesheet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CssMode {
    /// Every class, declarations compressed (the Go program's `gen chromastyles`).
    #[default]
    AllClasses,
    /// Only classes with declarations (`gen chromastyles --omitEmpty`).
    OmitEmpty,
}

/// The stylesheet of `style` (Chroma's `WriteCSS` for a formatter without line numbers).
pub(crate) fn stylesheet(style: &Style, mode: CssMode) -> String {
    let settings = CssSettings {
        all_classes: mode == CssMode::AllClasses,
        ..CssSettings::default()
    };
    let mut css = css_map(style, settings);
    if mode == CssMode::AllClasses {
        // Chroma compresses when the formatter does not use classes, which `WithAllClasses`
        // alone does not turn on.
        for v in css.values_mut() {
            *v = compress(v);
        }
    }
    let mut out = String::new();
    let decl = |t| css.get(&t).map_or("", String::as_str);
    let _ = writeln!(
        out,
        "/* Background */ .bg {{ {} }}",
        decl(TokenType::Background)
    );
    let _ = writeln!(
        out,
        "/* PreWrapper */ .chroma {{ {} }}",
        decl(TokenType::PreWrapper)
    );
    let mut types: Vec<_> = css.keys().copied().collect();
    types.sort_by_key(|t| t.number());
    for t in types {
        if matches!(t, TokenType::Background | TokenType::PreWrapper) || t.class().is_empty() {
            continue;
        }
        let _ = writeln!(
            out,
            "/* {} */ .chroma .{} {{ {} }}",
            t.name(),
            t.class(),
            decl(t)
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_style_parses() {
        let styles = Styles::bundled();
        assert_eq!(styles.names().count(), crate::styles::STYLES.len());
        assert_eq!(crate::styles::STYLES.len(), 67);
        assert!(styles.get("solarized-dark").is_some());
        assert_eq!(styles.fallback().name, FALLBACK_STYLE);
    }

    #[test]
    fn entries() {
        let e = StyleEntry::parse("bold #f00 bg:#ansiblue").expect("entry");
        assert_eq!(
            e.to_css(),
            "color: #ff0000; background-color: #0000ff; font-weight: bold"
        );
        assert_eq!(
            compress("color: #ff0000; background-color: #0000ff"),
            "color:#f00;background-color:#00f"
        );
    }
}
