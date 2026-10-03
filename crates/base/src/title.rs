//! Title case (`titleCaseStyle`).
//!
//! AP and Chicago are the rules of jdkato/prose v1.2.1 (MIT), Go's title converter: every
//! word is capitalised except the listed small words (and, for Chicago, prepositions) that are
//! neither first nor last and follow a space, `-` or `/`. The converter locates each word in a
//! copy of the input where typographic quotes, dashes and the ellipsis are replaced by ASCII,
//! and advances through that copy by the word's character count; both are part of the rules
//! (they decide which words count as first or last, and what precedes them), so they are kept.

use crate::text;

/// A title case style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style {
    /// Associated Press (Go's default).
    #[default]
    Ap,
    /// Chicago Manual of Style.
    Chicago,
    /// Every word's first letter upper-cased (Go's `strings.Title`).
    Go,
    /// Only the first letter upper-cased.
    FirstUpper,
    /// Unchanged.
    None,
}

impl Style {
    /// The style of a `titleCaseStyle` value (ASCII case ignored); anything unknown is AP.
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "chicago" => Self::Chicago,
            "go" => Self::Go,
            "firstupper" => Self::FirstUpper,
            "none" => Self::None,
            _ => Self::Ap,
        }
    }
}

/// `s` in title case.
#[must_use]
pub fn title_case(s: &str, style: Style) -> String {
    match style {
        Style::Ap => prose(s, false),
        Style::Chicago => prose(s, true),
        Style::Go => go_title(s),
        Style::FirstUpper => {
            let mut chars = s.chars();
            chars.next().map_or_else(String::new, |c| {
                let mut out = String::with_capacity(s.len());
                out.push(text::upper_char(c));
                out.push_str(chars.as_str());
                out
            })
        }
        Style::None => s.to_owned(),
    }
}

const SMALL_WORDS: &[&str] = &[
    "a", "an", "and", "as", "at", "but", "by", "en", "for", "if", "in", "nor", "of", "on", "or",
    "per", "the", "to", "vs", "vs.", "via", "v", "v.",
];

const PREPOSITIONS: &[&str] = &[
    "with",
    "from",
    "into",
    "during",
    "including",
    "until",
    "against",
    "among",
    "throughout",
    "despite",
    "towards",
    "upon",
    "concerning",
    "about",
    "over",
    "through",
    "before",
    "between",
    "after",
    "since",
    "without",
    "under",
    "within",
    "along",
    "following",
    "across",
    "beyond",
    "around",
    "down",
    "near",
    "above",
];

/// `s` with typographic quotes and dashes replaced by ASCII and `…` by `...`.
fn ascii_punctuation(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\u{201c}' | '\u{201d}' => out.push('"'),
            '\u{2018}' | '\u{2019}' => out.push('\''),
            '\u{2013}' | '\u{2014}' => out.push('-'),
            '\u{2026}' => out.push_str("..."),
            c => out.push(c),
        }
    }
    out
}

/// A word: a letter or number followed by anything up to white space (`\t \n \f \r` and
/// space), `-` or `/`.
fn next_word(s: &str, from: usize) -> Option<(usize, usize)> {
    let start = from + s[from..].find(|c: char| text::is_letter(c) || text::is_number(c))?;
    let end = s[start..]
        .find(['\t', '\n', '\x0c', '\r', ' ', '-', '/'])
        .map_or(s.len(), |i| start + i);
    Some((start, end))
}

fn prose(s: &str, chicago: bool) -> String {
    let t = ascii_punctuation(s);
    let tb = t.as_bytes();
    // The byte at `i`, or the first byte when out of range (`t` is never empty when a word
    // was found).
    let at = |i: isize| -> u8 {
        usize::try_from(i)
            .ok()
            .and_then(|i| tb.get(i).copied())
            .unwrap_or(tb[0])
    };
    let len = isize::try_from(t.len()).expect("string length fits isize");
    let mut out = String::with_capacity(s.len());
    let mut idx: isize = 0;
    let mut copied = 0;
    let mut from = 0;
    while let Some((a, b)) = next_word(s, from) {
        out.push_str(&s[copied..a]);
        let word = &s[a..b];
        let lower = text::to_lower(word);
        let found = usize::try_from(idx)
            .ok()
            .and_then(|i| tb.get(i..))
            .and_then(|rest| find_bytes(rest, word.as_bytes()))
            .map_or(-1, |p| isize::try_from(p).expect("fits"));
        let pos = found + idx;
        let prev = at(pos - 1);
        let chars = isize::try_from(word.chars().count()).expect("fits");
        idx = pos + chars;
        let bounding = pos == 0 || idx == len;
        let small = !bounding
            && (SMALL_WORDS.contains(&lower.as_str())
                || (chicago && PREPOSITIONS.contains(&lower.as_str())));
        if small
            && matches!(prev, b' ' | b'-' | b'/')
            && at(pos - 2) != b':'
            && at(pos - 2) != b'-'
            && (at(pos + chars) != b'-' || at(pos - 1) == b'-')
        {
            out.push_str(&lower);
        } else {
            let mut cs = word.chars();
            if let Some(c) = cs.next() {
                out.push(text::title_char(c));
                out.push_str(cs.as_str());
            }
        }
        copied = b;
        from = b;
    }
    out.push_str(&s[copied..]);
    out
}

/// The byte offset of the first `needle` in `haystack`.
fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Go's `strings.Title`: the first letter after a separator (anything but ASCII
/// alphanumerics, `_`, letters and digits; only white space outside ASCII) is title-cased.
fn go_title(s: &str) -> String {
    let is_separator = |c: char| {
        if c.is_ascii() {
            !(c.is_ascii_alphanumeric() || c == '_')
        } else if text::is_letter(c) || text::is_digit(c) {
            false
        } else {
            c.is_whitespace()
        }
    };
    let mut prev = ' ';
    s.chars()
        .map(|c| {
            let out = if is_separator(prev) {
                text::title_char(c)
            } else {
                c
            };
            prev = c;
            out
        })
        .collect()
}
