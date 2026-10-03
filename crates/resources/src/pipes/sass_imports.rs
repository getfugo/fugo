//! The URLs of the `@import`, `@use` and `@forward` rules of a Sass source, and a rewrite of the
//! ones a resolver maps to another path (`sass.rs` uses it to send imports grass would miss to
//! the file dart-sass finds).

use std::ops::Range;

/// The URL of an import-like rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Url<'a> {
    /// The byte range of the URL in the source, quotes included.
    pub range: Range<usize>,
    /// The URL without quotes.
    pub text: &'a str,
    /// Whether the rule is `@import` (else `@use` or `@forward`).
    pub for_import: bool,
}

/// The URLs of the `@import`, `@use` and `@forward` rules of `src`, outside comments and
/// strings. `@import` takes a comma-separated list; `@use` and `@forward` one URL. `indented`
/// is the indented (`.sass`) syntax, where a rule ends at the line end and `@import` URLs may be
/// unquoted. A URL with an escape or an interpolation is left out.
pub(super) fn urls(src: &str, indented: bool) -> Vec<Url<'_>> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => i = line_end(b, i),
            b'/' if b.get(i + 1) == Some(&b'*') => i = comment_end(b, i, indented),
            b'"' | b'\'' => i = string_end(b, i),
            b'@' => {
                let name_end = i + 1 + b[i + 1..].iter().take_while(|c| is_name(**c)).count();
                i = match &src[i + 1..name_end] {
                    "import" => rule_urls(src, name_end, true, indented, &mut out),
                    "use" | "forward" => rule_urls(src, name_end, false, indented, &mut out),
                    _ => name_end,
                };
            }
            _ => i += 1,
        }
    }
    out
}

/// `src` with each URL `resolve` maps to a path replaced by that path as a double-quoted string,
/// or `None` when it maps none.
pub(super) fn rewrite(
    src: &str,
    indented: bool,
    mut resolve: impl FnMut(&Url<'_>) -> Option<String>,
) -> Option<String> {
    let mut out = String::new();
    let mut last = 0;
    for url in urls(src, indented) {
        if let Some(path) = resolve(&url) {
            out.push_str(&src[last..url.range.start]);
            out.push('"');
            for c in path.chars() {
                if matches!(c, '"' | '\\') {
                    out.push('\\');
                }
                out.push(c);
            }
            out.push('"');
            last = url.range.end;
        }
    }
    if last == 0 {
        return None;
    }
    out.push_str(&src[last..]);
    Some(out)
}

/// Collects the URLs of the rule whose name ends at `i`; returns where scanning resumes.
fn rule_urls<'a>(
    src: &'a str,
    mut i: usize,
    for_import: bool,
    indented: bool,
    out: &mut Vec<Url<'a>>,
) -> usize {
    let b = src.as_bytes();
    loop {
        i = skip_space(b, i, indented);
        let start = i;
        match b.get(i) {
            Some(b'"' | b'\'') => {
                i = string_end(b, i);
                if i >= start + 2 && b[i - 1] == b[start] {
                    let text = &src[start + 1..i - 1];
                    if !text.contains('\\') && !text.contains("#{") {
                        out.push(Url {
                            range: start..i,
                            text,
                            for_import,
                        });
                    }
                }
            }
            Some(&c) if indented && for_import && !matches!(c, b',' | b';' | b'\n') => {
                while i < b.len() && !matches!(b[i], b',' | b';' | b'\n' | b' ' | b'\t' | b'\r') {
                    i += 1;
                }
                let text = &src[start..i];
                if !text.starts_with("url(") && !text.contains('\\') && !text.contains("#{") {
                    out.push(Url {
                        range: start..i,
                        text,
                        for_import,
                    });
                }
            }
            _ => return i,
        }
        let after = skip_space(b, i, indented);
        if !(for_import && b.get(after) == Some(&b',')) {
            return i;
        }
        i = after + 1;
    }
}

/// Skips blanks and comments; in the indented syntax a line end stops it.
fn skip_space(b: &[u8], mut i: usize, indented: bool) -> usize {
    while i < b.len() {
        match b[i] {
            b'\n' if indented => break,
            b' ' | b'\t' | b'\r' | b'\n' | b'\x0c' => i += 1,
            b'/' if b.get(i + 1) == Some(&b'*') => i = comment_end(b, i, indented),
            b'/' if b.get(i + 1) == Some(&b'/') => i = line_end(b, i),
            _ => break,
        }
    }
    i
}

/// The end of the quoted string starting at `i` (after the closing quote, or at the line end
/// of an unterminated string).
fn string_end(b: &[u8], i: usize) -> usize {
    let q = b[i];
    let mut j = i + 1;
    while j < b.len() {
        match b[j] {
            b'\\' => j += 2,
            b'\n' => return j,
            c if c == q => return j + 1,
            _ => j += 1,
        }
    }
    b.len()
}

/// The end of the `/* … */` comment starting at `i`. In the indented syntax a comment without
/// `*/` on its line runs to the line end (its continuation lines are indented text that holds
/// no rule).
fn comment_end(b: &[u8], i: usize, indented: bool) -> usize {
    let rest = &b[i + 2..];
    let close = rest.windows(2).position(|w| w == b"*/");
    if indented
        && let Some(n) = rest.iter().position(|&c| c == b'\n')
        && close.is_none_or(|c| n < c)
    {
        return i + 2 + n;
    }
    close.map_or(b.len(), |c| i + 2 + c + 2)
}

/// The end of the line holding `i` (at its `\n`).
fn line_end(b: &[u8], i: usize) -> usize {
    b[i..]
        .iter()
        .position(|&c| c == b'\n')
        .map_or(b.len(), |n| i + n)
}

fn is_name(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'-' || c == b'_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(src: &str, indented: bool) -> Vec<(&str, bool)> {
        urls(src, indented)
            .into_iter()
            .map(|u| (u.text, u.for_import))
            .collect()
    }

    #[test]
    fn scss_rules() {
        let src = r##"
@import "a", 'b.scss' screen;
@import url(x.css), "c";
@use "d.scss" as d;
@forward 'e.scss' show f;
// @import "commented";
/* @import "commented"; */
.x { content: "@import 'string'"; }
@media print { @import "nested"; }
@importx "no";
@import "#{$i}.scss", "esc\"aped";
"##;
        assert_eq!(
            texts(src, false),
            [
                ("a", true),
                ("b.scss", true),
                ("d.scss", false),
                ("e.scss", false),
                ("nested", true)
            ]
        );
    }

    #[test]
    fn ranges_include_the_quotes() {
        let src = "@import 'a.scss';";
        let u = &urls(src, false)[0];
        assert_eq!(&src[u.range.clone()], "'a.scss'");
    }

    #[test]
    fn indented_rules() {
        let src = "@import a.scss, \"b\"\n// @import c.scss\n/* @import d.scss\n  more\n@import e.sass\n.x\n  y: z\n";
        assert_eq!(
            texts(src, true),
            [("a.scss", true), ("b", true), ("e.sass", true)]
        );
    }

    #[test]
    fn rewrite_replaces_mapped_urls_only() {
        let src = "@import 'a.scss', \"b\";\n@use 'c.scss';\n";
        let out = rewrite(src, false, |u| {
            (u.text != "b").then(|| format!("/root/_{}", u.text))
        })
        .unwrap();
        assert_eq!(
            out,
            "@import \"/root/_a.scss\", \"b\";\n@use \"/root/_c.scss\";\n"
        );
        assert_eq!(rewrite(src, false, |_| None), None);
        assert_eq!(
            rewrite("@import x.sass\n", true, |_| Some("/r/x.sass".into())).unwrap(),
            "@import \"/r/x.sass\"\n"
        );
    }
}
