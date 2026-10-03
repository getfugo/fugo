//! Heading anchors (`autoHeadingIDType`) and their de-duplication.

use std::collections::HashSet;

use crate::text;

/// How heading text becomes an anchor id.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Style {
    /// GitHub: letters, digits and `_` lower-cased, space and `-` become `-`, the rest dropped.
    #[default]
    Github,
    /// [`Style::Github`] after removing accents, keeping ASCII only.
    GithubAscii,
    /// Blackfriday: runs of anything but letters and numbers become one `-` (none at the
    /// ends).
    Blackfriday,
}

/// The anchor id of heading text `s`.
#[must_use]
pub fn anchorize(s: &str, style: Style) -> String {
    match style {
        Style::Github => github(s.trim(), false),
        Style::GithubAscii => github(text::remove_accents(s).trim(), true),
        Style::Blackfriday => blackfriday(s),
    }
}

fn github(s: &str, ascii_only: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if ascii_only && !c.is_ascii() {
            continue;
        }
        if c == '-' || c == ' ' {
            out.push('-');
        } else if c == '_' || text::is_letter(c) || text::is_digit(c) {
            out.push(text::lower_char(c));
        }
    }
    out
}

fn blackfriday(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending_dash = false;
    for c in s.chars() {
        if text::is_letter(c) || text::is_number(c) {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(text::lower_char(c));
        } else {
            pending_dash = true;
        }
    }
    out
}

/// Makes anchor ids unique within a page: the second `intro` becomes `intro-1`, the third
/// `intro-2`.
#[derive(Clone, Debug, Default)]
pub struct Deduper {
    seen: HashSet<String>,
}

impl Deduper {
    /// An empty deduper.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Marks `id` as taken (an id given explicitly in the document).
    pub fn reserve(&mut self, id: &str) {
        self.seen.insert(id.to_owned());
    }

    /// `id` if it is free, else `id-N` with the smallest free `N` from 1; the result is taken.
    /// An empty `id` is replaced by `fallback` (`heading` for headings) first.
    pub fn unique(&mut self, id: String, fallback: &str) -> String {
        let id = if id.is_empty() {
            fallback.to_owned()
        } else {
            id
        };
        let id = if self.seen.contains(&id) {
            (1..)
                .map(|n| format!("{id}-{n}"))
                .find(|c| !self.seen.contains(c))
                .expect("an unbounded range finds a free id")
        } else {
            id
        };
        self.seen.insert(id.clone());
        id
    }
}
