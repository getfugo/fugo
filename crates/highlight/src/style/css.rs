//! The CSS Chroma derives from a style: inline `style` attributes and class stylesheets
//! (`formatters/html/html.go`).

use super::*;

/// The formatter settings that change the CSS Chroma derives from a style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
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
