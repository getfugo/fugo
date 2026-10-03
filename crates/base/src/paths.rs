//! Path newtypes and slash-path helpers.
//!
//! Each path flavour of the model is its own type with exactly one constructor, so a content
//! key cannot be passed where an output path is expected:
//!
//! | type | example | rule |
//! |---|---|---|
//! | [`ContentKey`] | `posts/my-post` (`.Path` is `/posts/my-post`) | lower case, space → `-` |
//! | [`TermKey`] | `/tags/rust-lang` | the same over `/<plural>/<value>`, not sanitised |
//! | [`OutputPath`] | `/posts/my-post/index.html` | clean file path under `publishDir` |
//! | [`UrlPath`] | `/posts/my post/` | unescaped link path |
//! | [`Permalink`] | `https://example.org/posts/my%20post/` | absolute, escaped |
//!
//! The free functions are lexical operations on `/`-separated paths (never the OS path
//! separator).

use std::fmt;
use std::sync::Arc;

use serde::Serialize;

use crate::text;
use crate::url::{BaseUrl, Component, escape, unescape, valid_encoded};

macro_rules! str_newtype_common {
    ($name:ident) => {
        impl $name {
            /// The path as a string.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

/// The key of a page in its language's content tree; `.Path` without the leading slash.
/// The home page has the empty key.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct ContentKey(Arc<str>);

str_newtype_common!(ContentKey);

impl ContentKey {
    /// The key of a logical content path (extension, language suffix and bundle index names
    /// already removed by the path parser): lower-cased, spaces replaced by `-`, surrounding
    /// slashes removed. Nothing else is changed.
    #[must_use]
    pub fn from_source(rel: &str) -> Self {
        Self(normalize_key(rel.trim_matches('/')).into())
    }

    /// The home page's key.
    #[must_use]
    pub fn home() -> Self {
        Self::default()
    }

    /// Whether this is the home page's key.
    #[must_use]
    pub fn is_home(&self) -> bool {
        self.0.is_empty()
    }

    /// Go's `.Path`: the key with a leading slash (`/` for the home page).
    #[must_use]
    pub fn to_path(&self) -> String {
        format!("/{}", self.0)
    }

    /// The segments (none for the home page).
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.0.split('/').filter(|s| !s.is_empty())
    }

    /// The first segment (the section of a regular page), empty for the home page.
    #[must_use]
    pub fn first_segment(&self) -> &str {
        self.segments().next().unwrap_or_default()
    }

    /// The parent key; `None` for the home page.
    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        if self.is_home() {
            return None;
        }
        Some(match self.0.rfind('/') {
            Some(i) => Self(self.0[..i].into()),
            None => Self::home(),
        })
    }

    /// Whether `prefix`'s segments are the first segments of this key (every key starts with
    /// the home key).
    #[must_use]
    pub fn starts_with_segments(&self, prefix: &Self) -> bool {
        prefix.is_home()
            || self
                .0
                .strip_prefix(&*prefix.0)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    }
}

/// Lower case (simple mappings) with spaces replaced by `-`: the normalisation of content keys
/// and term keys.
#[must_use]
pub fn normalize_key(s: &str) -> String {
    s.chars()
        .map(|c| if c == ' ' { '-' } else { text::lower_char(c) })
        .collect()
}

/// The key of a taxonomy term: `/<plural>/<value>`, normalised like a [`ContentKey`] but not
/// sanitised (`/tags/c++`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct TermKey(Arc<str>);

str_newtype_common!(TermKey);

impl TermKey {
    /// The key of `value` in the taxonomy `plural`.
    #[must_use]
    pub fn new(plural: &str, value: &str) -> Self {
        Self(normalize_key(&format!("/{plural}/{value}")).into())
    }
}

/// A file path under `publishDir`, starting with `/` (`/posts/one/index.html`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct OutputPath(Arc<str>);

str_newtype_common!(OutputPath);

impl OutputPath {
    /// A clean, rooted path; `..` cannot climb above the root.
    #[must_use]
    pub fn new(path: &str) -> Self {
        Self(clean(&format!("/{path}")).into())
    }

    /// The path without its leading slash, for joining onto the publish directory.
    #[must_use]
    pub fn relative(&self) -> &str {
        self.0.trim_start_matches('/')
    }
}

/// An unescaped link path starting with `/` (`/posts/my post/`).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct UrlPath(Arc<str>);

str_newtype_common!(UrlPath);

impl UrlPath {
    /// The path with a leading slash added when missing; a trailing slash is kept.
    #[must_use]
    pub fn new(path: &str) -> Self {
        if path.starts_with('/') {
            Self(path.into())
        } else {
            Self(format!("/{path}").into())
        }
    }

    /// The path percent-escaped as a URL path (upper-case hex; `/` and the sub-delimiters
    /// stay), like Go's `url.URL.EscapedPath` of the path parsed as a URL: `%XX` sequences in
    /// the path are escapes. A path that is already a valid escaping (only characters a path
    /// may hold, and every `%` starting an escape) that contains at least one `%XX` escape is
    /// returned as it is (`/a%2Fb/` stays); a path without `%` is always escaped (`/Lay's/` →
    /// `/Lay%27s/`), as Go does for a URL built from a path rather than parsed from text;
    /// otherwise its escapes are decoded and the result escaped (`/a b%2F/` → `/a%20b//`). A
    /// `%` that starts no escape is escaped (`/100%/` → `/100%25/`).
    #[must_use]
    pub fn escaped(&self) -> String {
        let Ok(decoded) = unescape(&self.0, Component::Path) else {
            return escape(self.0.as_bytes(), Component::Path).into_owned();
        };
        // Go keeps a raw path only when it came from parsed text that carried escapes (a
        // front-matter `url` such as `/a%2Fb/`); a path without any `%` is escaped like a URL
        // built from a file path (`Lay's.txt` → `Lay%27s.txt`).
        if self.0.contains('%') && valid_encoded(&self.0, Component::Path) {
            return self.0.to_string();
        }
        escape(&decoded, Component::Path).into_owned()
    }
}

/// An absolute, escaped URL of a page or resource.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct Permalink(Arc<str>);

str_newtype_common!(Permalink);

impl Permalink {
    /// `path`, escaped, appended to the base URL.
    #[must_use]
    pub fn new(base: &BaseUrl, path: &UrlPath) -> Self {
        Self::from_escaped(base, &path.escaped())
    }

    /// An already escaped link path (`/a%20b/`, as [`UrlPath::escaped`] returns it, or one
    /// with a query) appended to the base URL as it is; a leading `/` is dropped because the
    /// base URL ends with one.
    #[must_use]
    pub fn from_escaped(base: &BaseUrl, escaped: &str) -> Self {
        let rel = escaped.strip_prefix('/').unwrap_or(escaped);
        Self(format!("{}{rel}", base.as_str()).into())
    }
}

/// Go's path sanitiser (`MakePath`): keeps letters, decimal digits, marks, `%XX` escapes and
/// `. / \ _ # + ~ - @`; drops everything else; a run of white space between kept characters
/// becomes one `-` (none at the start, none next to an existing `-`).
#[must_use]
pub fn sanitize(s: &str) -> String {
    let allowed = |i: usize, c: char| -> bool {
        c != ' '
            && (text::is_letter(c)
                || text::is_digit(c)
                || text::is_mark(c)
                || matches!(c, '.' | '/' | '\\' | '_' | '#' | '+' | '~' | '-' | '@')
                || (c == '%'
                    && s.as_bytes()
                        .get(i + 1..i + 3)
                        .is_some_and(|h| h.iter().all(u8::is_ascii_hexdigit))
                    && i + 2 < s.len()))
    };
    if s.char_indices().all(|(i, c)| allowed(i, c)) {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut pending_hyphen = false;
    let mut was_hyphen = false;
    for (i, c) in s.char_indices() {
        if allowed(i, c) {
            was_hyphen = c == '-';
            if pending_hyphen {
                if !was_hyphen {
                    out.push('-');
                }
                pending_hyphen = false;
            }
            out.push(c);
        } else if !out.is_empty() && !was_hyphen && c.is_whitespace() {
            pending_hyphen = true;
        }
    }
    out
}

/// A title from a file or directory name: surrounding white space removed, `-` → space.
#[must_use]
pub fn make_title(name: &str) -> String {
    name.trim().replace('-', " ")
}

/// The shortest lexically equivalent path: `.` and empty segments removed, `..` resolved
/// (not above the root of a rooted path). An empty result is `.`.
#[must_use]
pub fn clean(p: &str) -> String {
    String::from_utf8(clean_bytes(p.as_bytes())).expect("splitting at '/' keeps UTF-8")
}

/// [`clean`] over bytes (decoded URL paths need not be UTF-8).
#[must_use]
pub fn clean_bytes(p: &[u8]) -> Vec<u8> {
    let rooted = p.first() == Some(&b'/');
    let mut stack: Vec<&[u8]> = Vec::new();
    for seg in p.split(|&c| c == b'/') {
        match seg {
            b"" | b"." => {}
            b".." => {
                if stack.last().is_some_and(|s| *s != b"..") {
                    stack.pop();
                } else if !rooted {
                    stack.push(seg);
                }
            }
            _ => stack.push(seg),
        }
    }
    let mut out = Vec::with_capacity(p.len());
    if rooted {
        out.push(b'/');
    }
    out.extend(stack.join(&b'/'));
    if out.is_empty() {
        out.push(b'.');
    }
    out
}

/// Joins the non-empty elements with `/` and [`clean`]s the result; empty when all elements
/// are empty.
#[must_use]
pub fn join(elems: &[&str]) -> String {
    let bytes: Vec<&[u8]> = elems.iter().map(|e| e.as_bytes()).collect();
    String::from_utf8(join_bytes(&bytes)).expect("joining UTF-8 at '/' keeps UTF-8")
}

/// [`join`] over bytes.
#[must_use]
pub fn join_bytes(elems: &[&[u8]]) -> Vec<u8> {
    let parts: Vec<&[u8]> = elems.iter().copied().filter(|e| !e.is_empty()).collect();
    if parts.is_empty() {
        return Vec::new();
    }
    clean_bytes(&parts.join(&b'/'))
}

/// `(directory with its trailing slash, file name)`, split after the last `/`.
#[must_use]
pub fn split(p: &str) -> (&str, &str) {
    match p.rfind('/') {
        Some(i) => p.split_at(i + 1),
        None => ("", p),
    }
}

/// The last element: trailing slashes removed first; `.` for an empty path, `/` for a path of
/// slashes.
#[must_use]
pub fn base(p: &str) -> &str {
    if p.is_empty() {
        return ".";
    }
    let trimmed = p.trim_end_matches('/');
    if trimmed.is_empty() {
        return "/";
    }
    split(trimmed).1
}

/// All but the last element, [`clean`]ed (`.` when there is no directory).
#[must_use]
pub fn parent(p: &str) -> String {
    clean(split(p).0)
}

/// The directory part without its trailing slash (`/a/b.md` → `/a`, `/a` → `/`, `a` → ``).
#[must_use]
pub fn dir(p: &str) -> &str {
    let d = split(p).0;
    if d.len() > 1 {
        d.strip_suffix('/').unwrap_or(d)
    } else {
        d
    }
}

/// The extension of the last element, with its dot (`.md`), or empty.
#[must_use]
pub fn ext(p: &str) -> &str {
    let name = split(p).1;
    name.rfind('.').map_or("", |i| &name[i..])
}

/// [`ext`] without the dot.
#[must_use]
pub fn ext_no_delimiter(p: &str) -> &str {
    let e = ext(p);
    e.strip_prefix('.').unwrap_or(e)
}

/// `p` without its extension.
#[must_use]
pub fn trim_ext(p: &str) -> &str {
    &p[..p.len() - ext(p).len()]
}

/// `(file name without extension, extension)`; the name is empty for a directory path (a
/// trailing slash, `.`, `..` or `/`).
#[must_use]
pub fn file_and_ext(p: &str) -> (&str, &str) {
    let e = ext(p);
    let b = base(p);
    let name = if p.ends_with('/') || b.is_empty() || matches!(b, "." | ".." | "/") {
        ""
    } else if e.is_empty() {
        b
    } else {
        &b[..b.rfind('.').unwrap_or(b.len())]
    };
    (name, e)
}

/// The file name without extension ([`file_and_ext`]).
#[must_use]
pub fn filename(p: &str) -> &str {
    file_and_ext(p).0
}

/// The pretty output path of a link path: `/a/b.html` → `/a/b/index.html`,
/// `/a/b` → `/a/b/index.html`.
#[must_use]
pub fn prettify_url_path(p: &str) -> String {
    if ext(p).is_empty() {
        if p.len() < 2 {
            return "/".to_owned();
        }
        return join(&[p, "index.html"]);
    }
    let (name, e) = file_and_ext(p);
    if name == "index" {
        return clean(p);
    }
    join(&[&parent(p), name, &format!("index{e}")])
}

/// The pretty URL of a link path: `/a/b.html` → `/a/b`, `/a/index.html` → `/a`.
#[must_use]
pub fn prettify_url(p: &str) -> String {
    let x = prettify_url_path(p);
    if base(&x) == "index.html" {
        return parent(&x);
    }
    if p.is_empty() {
        return "/".to_owned();
    }
    x
}

/// The ugly URL of a link path: `/a/b/` → `/a/b.html`, `/a/index.html` → `/a.html`.
#[must_use]
pub fn uglify(p: &str) -> String {
    if ext(p).is_empty() {
        if p.len() < 2 {
            return "/".to_owned();
        }
        return format!("{}.html", clean(p));
    }
    let (name, e) = file_and_ext(p);
    if name == "index" {
        let d = parent(p);
        if d.len() > 1 {
            return format!("{d}{e}");
        }
        return p.to_owned();
    }
    if name.is_empty() {
        return format!("{}index{e}", parent(p));
    }
    clean(p)
}
