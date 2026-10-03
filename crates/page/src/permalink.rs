//! `[permalinks]` patterns: `/:year/:month/:slug/`, `:sections[1:]`, Go date layouts.
//!
//! A pattern is parsed once into literal text and attributes. An attribute is `:` followed by
//! ASCII word characters, optionally with one `[…]` suffix (`:sections[1:]`); `\:` is a
//! literal colon. Attributes that are not tokens are Go time layouts (`:2006`, `:Jan`, `:02`),
//! translated to `strftime` at parse time.

use std::collections::BTreeMap;

use jiff::Zoned;
use jiff::civil::Weekday;
use ssg_base::PageKind;
use ssg_base::url::SiteUrls;
use ssg_config::sections::Permalinks;

use crate::{GoLayout, PageError};

/// A compiled permalink pattern.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermalinkPattern {
    pieces: Vec<Piece>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Piece {
    Text(String),
    Attr(Attr),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Attr {
    Year,
    Month,
    MonthName,
    Day,
    Weekday,
    WeekdayName,
    YearDay,
    Section,
    Sections,
    SectionsSlice(Slice),
    Title,
    Slug,
    SlugOrFilename,
    Filename,
    ContentBaseName,
    SlugOrContentBaseName,
    /// A Go layout.
    Layout(GoLayout),
}

/// A `:sections[…]` selection.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Slice {
    /// `[]`-less or malformed: nothing.
    Empty,
    /// `[n]` / `[last]`.
    Index(Bound),
    /// `[a:b]` (either may be omitted).
    Range(Option<Bound>, Option<Bound>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Bound {
    At(usize),
    Last,
}

/// A page's file, for `:filename`.
#[derive(Clone, Copy, Debug, Default)]
pub struct PermalinkFile<'a> {
    /// The file name without extension and language (`my-post`, `index`, `_index`).
    pub translation_base_name: &'a str,
    /// The file's directory (`/posts/my-bundle/`); a bundle index is named after its last
    /// element.
    pub dir: &'a str,
}

/// What a pattern reads from a page.
#[derive(Clone, Copy, Debug)]
pub struct PermalinkCtx<'a> {
    /// `.Date`; `None` is Go's zero date (`0001-01-01T00:00:00Z`).
    pub date: Option<&'a Zoned>,
    pub title: &'a str,
    pub slug: &'a str,
    /// `.Section`.
    pub section: &'a str,
    /// The current section's path segments (`["blog", "sub"]`; none for the home page).
    pub sections: &'a [&'a str],
    /// The file, for pages that have one.
    pub file: Option<PermalinkFile<'a>>,
    /// The content file's base name without identifiers, in its own spelling (`My Post`).
    pub content_base_name: &'a str,
    /// Makes titles and names URL-safe (`urlize`).
    pub urls: &'a SiteUrls,
}

impl PermalinkPattern {
    /// Parses a pattern.
    ///
    /// # Errors
    /// [`PageError::PermalinkAttribute`] for an attribute that is neither a token nor a Go
    /// layout.
    pub fn parse(pattern: &str) -> Result<Self, PageError> {
        let mut pieces = Vec::new();
        let mut text = String::new();
        let mut rest = pattern;
        while let Some(c) = rest.chars().next() {
            if let Some(after) = rest.strip_prefix("\\:") {
                text.push(':');
                rest = after;
                continue;
            }
            if c == ':'
                && let Some((attr, after)) = split_attribute(&rest[1..])
            {
                if !text.is_empty() {
                    pieces.push(Piece::Text(std::mem::take(&mut text)));
                }
                let parsed = parse_attr(attr).ok_or_else(|| PageError::PermalinkAttribute {
                    pattern: pattern.to_owned(),
                    attribute: attr.to_owned(),
                })?;
                pieces.push(Piece::Attr(parsed));
                rest = after;
                continue;
            }
            text.push(c);
            rest = &rest[c.len_utf8()..];
        }
        if !text.is_empty() {
            pieces.push(Piece::Text(text));
        }
        Ok(Self { pieces })
    }

    /// Expands the pattern for a page.
    ///
    /// # Errors
    /// [`PageError::SectionSlice`] for a `:sections[a:b]` whose `a` is after `b` for this
    /// page.
    pub fn expand(&self, c: &PermalinkCtx<'_>) -> Result<String, PageError> {
        let mut out = String::new();
        for piece in &self.pieces {
            match piece {
                Piece::Text(t) => out.push_str(t),
                Piece::Attr(a) => out.push_str(&self.attr(a, c)?),
            }
        }
        Ok(out)
    }

    fn attr(&self, a: &Attr, c: &PermalinkCtx<'_>) -> Result<String, PageError> {
        let zero;
        let date = if let Some(d) = c.date {
            d
        } else {
            zero = zero_date();
            &zero
        };
        let urlize = |s: &str| c.urls.urlize(s);
        let slug_or = |other: String| {
            if c.slug.is_empty() {
                other
            } else {
                urlize(c.slug)
            }
        };
        Ok(match a {
            Attr::Year => date.year().to_string(),
            Attr::Month => format!("{:02}", date.month()),
            Attr::MonthName => date.strftime("%B").to_string(),
            Attr::Day => format!("{:02}", date.day()),
            Attr::Weekday => weekday_number(date.weekday()).to_string(),
            Attr::WeekdayName => date.strftime("%A").to_string(),
            Attr::YearDay => date.day_of_year().to_string(),
            Attr::Section => c.section.to_owned(),
            Attr::Sections => format!("/{}", c.sections.join("/")),
            Attr::SectionsSlice(s) => {
                let picked = s
                    .select(c.sections)
                    .ok_or_else(|| PageError::SectionSlice {
                        pattern: self.source(),
                        slice: format!("{s:?}"),
                    })?;
                ssg_base::paths::join(picked)
            }
            Attr::Title => urlize(c.title),
            Attr::Slug => slug_or(urlize(c.title)),
            Attr::SlugOrFilename => slug_or(filename(c)),
            Attr::Filename => filename(c),
            Attr::ContentBaseName => urlize(c.content_base_name),
            Attr::SlugOrContentBaseName => slug_or(urlize(c.content_base_name)),
            Attr::Layout(layout) => layout.format(date),
        })
    }

    /// The pattern as written back (for error messages).
    fn source(&self) -> String {
        self.pieces
            .iter()
            .map(|p| match p {
                Piece::Text(t) => t.replace(':', "\\:"),
                Piece::Attr(a) => format!(":{a:?}"),
            })
            .collect()
    }
}

/// Go's zero time, the date of pages without one.
fn zero_date() -> Zoned {
    jiff::civil::date(1, 1, 1)
        .at(0, 0, 0, 0)
        .to_zoned(jiff::tz::TimeZone::UTC)
        .expect("0001-01-01 is in range")
}

/// Sunday = 0 (Go's `time.Weekday`).
fn weekday_number(w: Weekday) -> i8 {
    w.to_sunday_zero_offset()
}

/// `:filename`: the file's base name; a bundle index is named after its directory, a branch
/// index has none.
fn filename(c: &PermalinkCtx<'_>) -> String {
    let Some(f) = c.file else {
        return String::new();
    };
    let name = match f.translation_base_name {
        "index" => {
            let dir = f.dir.strip_suffix('/').unwrap_or(f.dir);
            ssg_base::paths::split(dir).1
        }
        "_index" => return String::new(),
        n => n,
    };
    c.urls.urlize(name)
}

/// Splits `word[slice]` off the text after a `:`: ASCII word characters, then an optional
/// `[` … `]` holding at least one character (the first `]` after it closes; no newline).
fn split_attribute(s: &str) -> Option<(&str, &str)> {
    let word = s
        .bytes()
        .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
        .count();
    if word == 0 {
        return None;
    }
    let mut end = word;
    if s[word..].starts_with('[') {
        let inner = &s[word + 1..];
        let mut chars = inner.char_indices();
        if let Some((_, first)) = chars.next()
            && first != '\n'
            && let Some((i, _)) = chars
                .take_while(|(_, c)| *c != '\n')
                .find(|(_, c)| *c == ']')
        {
            end = word + 1 + i + 1;
        }
    }
    Some((&s[..end], &s[end..]))
}

fn parse_attr(attr: &str) -> Option<Attr> {
    Some(match attr {
        "year" => Attr::Year,
        "month" => Attr::Month,
        "monthname" => Attr::MonthName,
        "day" => Attr::Day,
        "weekday" => Attr::Weekday,
        "weekdayname" => Attr::WeekdayName,
        "yearday" => Attr::YearDay,
        "section" => Attr::Section,
        "sections" => Attr::Sections,
        "title" => Attr::Title,
        "slug" => Attr::Slug,
        "slugorfilename" => Attr::SlugOrFilename,
        "filename" => Attr::Filename,
        "contentbasename" => Attr::ContentBaseName,
        "slugorcontentbasename" => Attr::SlugOrContentBaseName,
        _ => {
            if let Some(cut) = attr.strip_prefix("sections")
                && cut.starts_with('[')
            {
                return Some(Attr::SectionsSlice(Slice::parse(cut)));
            }
            return GoLayout::parse(attr).map(Attr::Layout);
        }
    })
}

impl Slice {
    /// `[…]`, trimmed and lower-cased: `[n]`, `[last]`, `[a:b]`; anything else selects
    /// nothing. Numbers that do not parse count as 0.
    fn parse(cut: &str) -> Self {
        let cut = cut.trim().to_lowercase();
        let Some(inner) = cut
            .strip_prefix('[')
            .and_then(|c| c.strip_suffix(']'))
            .filter(|i| !i.is_empty())
        else {
            return Self::Empty;
        };
        let bound = |s: &str| -> Option<Bound> {
            match s {
                "" => None,
                "last" => Some(Bound::Last),
                n => {
                    Some(Bound::At(n.parse::<i64>().map_or(0, |v| {
                        usize::try_from(v.max(0)).unwrap_or(usize::MAX)
                    })))
                }
            }
        };
        match inner.split_once(':') {
            None => Self::Index(bound(inner).unwrap_or(Bound::At(0))),
            Some((a, b)) => {
                // `[a:b:c]` reads as `[a:b]`.
                let b = b.split(':').next().unwrap_or_default();
                Self::Range(bound(a), bound(b))
            }
        }
    }

    /// The selected entries; `None` when the range is reversed for these entries.
    fn select<'a>(&self, s: &'a [&'a str]) -> Option<&'a [&'a str]> {
        if s.is_empty() {
            return Some(&[]);
        }
        let resolve = |b: Bound| match b {
            Bound::Last => Some(s.len() - 1),
            Bound::At(n) => (n < s.len()).then_some(n),
        };
        match self {
            Self::Empty => Some(&[]),
            Self::Index(b) => Some(match resolve(*b) {
                Some(i) if !s[i].is_empty() => &s[i..=i],
                _ => &[],
            }),
            Self::Range(a, b) => {
                let Some(lo) = a.map_or(Some(0), resolve) else {
                    return Some(&[]);
                };
                // An upper bound past the end is the end.
                let hi = b.map_or(s.len(), |b| resolve(b).unwrap_or(s.len()));
                if lo > hi { None } else { Some(&s[lo..hi]) }
            }
        }
    }
}

/// The compiled `[permalinks]` of a site: per kind, section (or taxonomy) → pattern.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PermalinkPatterns(BTreeMap<PageKind, BTreeMap<String, PermalinkPattern>>);

impl PermalinkPatterns {
    /// Compiles a site's `[permalinks]`. Keys are trimmed of spaces and slashes (`/` is the
    /// root section `""`); of two keys that trim alike the first in byte order wins.
    ///
    /// # Errors
    /// The first pattern that does not parse.
    pub fn compile(config: &Permalinks) -> Result<Self, PageError> {
        let mut out: BTreeMap<PageKind, BTreeMap<String, PermalinkPattern>> = BTreeMap::new();
        for kind in ssg_config::sections::PERMALINK_KINDS {
            let Some(patterns) = config.of_kind(kind) else {
                continue;
            };
            let entry = out.entry(kind).or_default();
            for (key, pattern) in patterns {
                let key = key.trim_matches([' ', '/']).to_owned();
                if let std::collections::btree_map::Entry::Vacant(slot) = entry.entry(key) {
                    slot.insert(PermalinkPattern::parse(pattern)?);
                }
            }
        }
        Ok(Self(out))
    }

    /// The pattern of pages of `kind` whose section (taxonomy for taxonomy and term pages) is
    /// `section`.
    #[must_use]
    pub fn get(&self, kind: PageKind, section: &str) -> Option<&PermalinkPattern> {
        self.0.get(&kind)?.get(section)
    }
}
