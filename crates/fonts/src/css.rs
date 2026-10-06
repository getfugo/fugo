//! The characters a site's CSS draws: the strings of its `content` and `quotes` declarations,
//! with CSS escapes decoded, and those of the custom properties they use (`--fa: "\f005"`,
//! which `content: var(--fa)` draws), directly or through other custom properties.
//!
//! The text is scanned as it is, with no CSS parser: style sheets, `<style>` elements, `style`
//! attributes and any other text that looks like such a declaration all count. An extra
//! character only keeps one more glyph; a missed one loses a glyph. A custom property no
//! `content` uses does not count: style sheets name fonts and URLs in custom properties
//! (`--bs-font-sans-serif: "Segoe UI"`), whose characters an icon font would keep for nothing.
//! Nor does the string of a `url()`, an image (`content: url("data:image/svg+xml,…")`).

use std::collections::{BTreeMap, BTreeSet};

/// The longest CSS value looked at (in bytes): a `content` string is short, and a quote that
/// does not start one (`style="…"`) must not swallow the rest of the page.
const MAX_VALUE: usize = 1024;

/// The strings of some CSS, as fonts draw them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CssStrings {
    /// The characters of `content` and `quotes` strings.
    drawn: BTreeSet<char>,
    /// The custom properties `content` and `quotes` use.
    drawn_vars: BTreeSet<String>,
    /// Each custom property's string characters, and the custom properties it uses.
    custom: BTreeMap<String, Value>,
}

/// What a declaration's value holds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Value {
    chars: BTreeSet<char>,
    vars: BTreeSet<String>,
}

impl CssStrings {
    /// Adds the declarations of `css` (a style sheet, or any text holding CSS).
    pub fn add(&mut self, css: &str) {
        let bytes = css.as_bytes();
        let mut from = 0;
        while let Some(colon) = bytes[from..]
            .iter()
            .position(|&b| b == b':')
            .map(|i| from + i)
        {
            from = colon + 1;
            let name_end = bytes[..colon]
                .iter()
                .rposition(|b| !b.is_ascii_whitespace())
                .map_or(0, |i| i + 1);
            let name_start = bytes[..name_end]
                .iter()
                .rposition(|&b| !is_name_byte(b))
                .map_or(0, |i| i + 1);
            let name = &css[name_start..name_end];
            if name.eq_ignore_ascii_case("content") || name.eq_ignore_ascii_case("quotes") {
                let mut value = Value::default();
                from = read_value(css, from, &mut value);
                self.drawn.extend(value.chars);
                self.drawn_vars.extend(value.vars);
            } else if name.len() > 2 && name.starts_with("--") {
                let entry = self.custom.entry(name.to_owned()).or_default();
                from = read_value(css, from, entry);
            }
        }
    }

    /// Adds everything of `other`.
    pub fn merge(&mut self, other: Self) {
        self.drawn.extend(other.drawn);
        self.drawn_vars.extend(other.drawn_vars);
        for (name, value) in other.custom {
            let entry = self.custom.entry(name).or_default();
            entry.chars.extend(value.chars);
            entry.vars.extend(value.vars);
        }
    }

    /// The characters drawn: those of `content` and `quotes`, and those of the custom
    /// properties they use, directly or not.
    #[must_use]
    pub fn chars(&self) -> BTreeSet<char> {
        let mut chars = self.drawn.clone();
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut todo: Vec<&str> = self.drawn_vars.iter().map(String::as_str).collect();
        while let Some(name) = todo.pop() {
            if !seen.insert(name) {
                continue;
            }
            if let Some(v) = self.custom.get(name) {
                chars.extend(v.chars.iter().copied());
                todo.extend(v.vars.iter().map(String::as_str));
            }
        }
        chars
    }
}

fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_'
}

/// Whether the function `name` (`url(`, any case) starts at `i`, before `end`.
fn starts_function(bytes: &[u8], i: usize, end: usize, name: &[u8]) -> bool {
    i + name.len() < end
        && bytes[i..i + name.len()].eq_ignore_ascii_case(name)
        && (i == 0 || !is_name_byte(bytes[i - 1]))
}

/// Reads the declaration value starting at `at` into `value`: its strings' characters and the
/// custom properties it uses (`var(--x)`). Returns where the value ends.
fn read_value(css: &str, at: usize, value: &mut Value) -> usize {
    let mut end = (at + MAX_VALUE).min(css.len());
    while !css.is_char_boundary(end) {
        end -= 1;
    }
    let bytes = css.as_bytes();
    let mut i = at;
    while i < end {
        match bytes[i] {
            b';' | b'}' | b'{' | b'<' | b'>' => return i,
            quote @ (b'"' | b'\'') => {
                let (string, next) = read_string(&css[..end], i + 1, char::from(quote));
                if let Some(string) = string {
                    value.chars.extend(string);
                }
                i = next;
            }
            // An image (`content: url("data:image/svg+xml,…")`): its string is not text.
            b'u' | b'U' if starts_function(bytes, i, end, b"url(") => {
                i += 4;
                while i < end && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                if let Some(&quote @ (b'"' | b'\'')) = bytes[..end].get(i) {
                    i = read_string(&css[..end], i + 1, char::from(quote)).1;
                }
                i = bytes[i..end]
                    .iter()
                    .position(|&b| b == b')')
                    .map_or(end, |p| i + p + 1);
            }
            b'v' | b'V' if starts_function(bytes, i, end, b"var(") => {
                i += 4;
                while i < end && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                let start = i;
                while i < end && is_name_byte(bytes[i]) {
                    i += 1;
                }
                let name = &css[start..i];
                if name.len() > 2 && name.starts_with("--") {
                    value.vars.insert(name.to_owned());
                }
            }
            _ => i += 1,
        }
    }
    i
}

/// The decoded string that starts at `at` (after its opening `quote`), and where the scan goes
/// on; no string when it is not closed before a line break or the end of `css`.
fn read_string(css: &str, at: usize, quote: char) -> (Option<Vec<char>>, usize) {
    let mut string = Vec::new();
    let mut i = at;
    while let Some(c) = css[i..].chars().next() {
        match c {
            '\n' | '\r' => return (None, i),
            '\\' => {
                let (decoded, len) = unescape(&css[i + 1..]);
                string.extend(decoded);
                i += 1 + len;
            }
            c if c == quote => return (Some(string), i + 1),
            c => {
                string.push(c);
                i += c.len_utf8();
            }
        }
    }
    (None, css.len())
}

/// What a CSS escape stands for (`s` follows its backslash), and its length in bytes: up to six
/// hex digits and one white space after them, a line break (a continuation: nothing), or the
/// next character itself.
fn unescape(s: &str) -> (Option<char>, usize) {
    let Some(first) = s.chars().next() else {
        return (None, 0);
    };
    if s.starts_with("\r\n") {
        return (None, 2);
    }
    if matches!(first, '\n' | '\r' | '\x0c') {
        return (None, 1);
    }
    let digits = s.bytes().take(6).take_while(u8::is_ascii_hexdigit).count();
    if digits == 0 {
        return (Some(first), first.len_utf8());
    }
    let code = u32::from_str_radix(&s[..digits], 16).unwrap_or(0xfffd);
    let rest = &s[digits..];
    let space = if rest.starts_with("\r\n") {
        2
    } else {
        usize::from(rest.starts_with([' ', '\t', '\n', '\r', '\x0c']))
    };
    let c = char::from_u32(code)
        .filter(|&c| c != '\0')
        .unwrap_or('\u{fffd}');
    (Some(c), digits + space)
}
