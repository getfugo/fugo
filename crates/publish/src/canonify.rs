//! `canonifyURLs` and `relativeURLs`: root-relative URLs in rendered output are rewritten to
//! absolute (or dot-relative) ones before the output is minified.
//!
//! A candidate is the text right after one of `src=`, `href=`, `url=`, `action=` or `srcset=`
//! (case-sensitive substring matches, so `data-src=` and `content="0; url=/x"` count too):
//!
//! - after `src=`, `href=`, `url=` and `action=`, an optional quote and then a root-relative
//!   URL (`/x`, not `//host/x`): the leading `/` becomes the prefix, and the base URL's own
//!   path (`docs/` for `https://example.org/docs/`) is dropped when the URL repeats it;
//! - after `srcset=`, a quoted list of candidates starting with a root-relative URL: every
//!   root-relative candidate in the list is rewritten, and the list's white space is collapsed
//!   to single spaces.
//!
//! HTML outputs use `"` and `'` as quotes; RSS uses their entity forms `&#34;` and `&#39;`,
//! because feed content is escaped HTML.

use std::borrow::Cow;
use std::sync::LazyLock;

use aho_corasick::{AhoCorasick, Input};
use ssg_base::paths::OutputPath;
use ssg_base::url::UrlRef;

/// The quote spelling of an output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quoting {
    /// `"` and `'` (HTML outputs).
    Html,
    /// `&#34;` and `&#39;` (escaped HTML inside XML, e.g. RSS).
    Xml,
}

impl Quoting {
    fn quotes(self) -> [&'static [u8]; 2] {
        match self {
            Self::Html => [b"\"", b"'"],
            Self::Xml => [b"&#34;", b"&#39;"],
        }
    }

    /// The quote `input` starts with, if any.
    fn at(self, input: &[u8]) -> Option<&'static [u8]> {
        self.quotes().into_iter().find(|q| input.starts_with(q))
    }
}

/// What follows a candidate prefix.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Candidate {
    /// `src=`, `href=`, `url=`, `action=`: one URL, quoted or not.
    Single,
    /// `srcset=`: a quoted candidate list.
    SrcSet,
}

const PREFIXES: [(&str, Candidate); 5] = [
    ("src=", Candidate::Single),
    ("href=", Candidate::Single),
    ("url=", Candidate::Single),
    ("action=", Candidate::Single),
    ("srcset=", Candidate::SrcSet),
];

/// The longest `srcset` value (up to its closing quote) that is rewritten.
const MAX_SRCSET: usize = 2000;

static FINDER: LazyLock<AhoCorasick> = LazyLock::new(|| {
    AhoCorasick::new(PREFIXES.map(|(p, _)| p)).expect("the candidate prefixes form an automaton")
});

/// Rewrites root-relative URLs of an output with a fixed prefix: the base URL (with a trailing
/// slash) for `canonifyURLs`, or the output's dot-relative path to the root (`../../`) for
/// `relativeURLs`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UrlRewriter {
    prefix: Vec<u8>,
    /// The prefix's own path without its leading slash (`docs/`), dropped from URLs that
    /// already start with it.
    root: Vec<u8>,
}

impl UrlRewriter {
    /// A rewriter writing `prefix` in place of the leading `/`.
    #[must_use]
    pub fn new(prefix: &str) -> Self {
        let root = UrlRef::parse(prefix)
            .map(|u| {
                let path = u.path();
                path.strip_prefix(b"/").unwrap_or(path).to_vec()
            })
            .unwrap_or_default();
        Self {
            prefix: prefix.as_bytes().to_vec(),
            root,
        }
    }

    /// `canonifyURLs`: the base URL, with a trailing slash added when missing.
    #[must_use]
    pub fn absolute(base_url: &str) -> Self {
        if base_url.ends_with('/') {
            Self::new(base_url)
        } else {
            Self::new(&format!("{base_url}/"))
        }
    }

    /// `relativeURLs`: the path from `target`'s directory back to the publish root.
    #[must_use]
    pub fn relative(target: &OutputPath) -> Self {
        Self::new(&dotted_path_to_root(target))
    }

    /// Rewrites `input`; unchanged input is borrowed.
    #[must_use]
    pub fn rewrite<'a>(&self, input: &'a [u8], quoting: Quoting) -> Cow<'a, [u8]> {
        let mut out: Option<Vec<u8>> = None;
        // `copied`: input before it is in `out`; `cursor`: where the next prefix search starts.
        let mut copied = 0;
        let mut cursor = 0;
        while let Some(m) = FINDER.find(Input::new(input).span(cursor..input.len())) {
            let after = m.end();
            let rewritten = match PREFIXES[m.pattern().as_usize()].1 {
                Candidate::Single => self.single(input, after, quoting),
                Candidate::SrcSet => self.srcset(input, after, quoting),
            };
            match rewritten {
                Some((at, replacement, end)) => {
                    let buf = out.get_or_insert_with(|| Vec::with_capacity(input.len() + 256));
                    buf.extend_from_slice(&input[copied..at]);
                    buf.extend_from_slice(&replacement);
                    copied = end;
                    cursor = end;
                }
                None => cursor = after,
            }
        }
        match out {
            Some(mut buf) => {
                buf.extend_from_slice(&input[copied..]);
                Cow::Owned(buf)
            }
            None => Cow::Borrowed(input),
        }
    }

    /// [`rewrite`](Self::rewrite) of text: the rewrite only replaces ASCII bytes and inserts the
    /// (UTF-8) prefix, so the result stays UTF-8.
    #[must_use]
    pub fn rewrite_str<'a>(&self, input: &'a str, quoting: Quoting) -> Cow<'a, str> {
        match self.rewrite(input.as_bytes(), quoting) {
            Cow::Borrowed(_) => Cow::Borrowed(input),
            Cow::Owned(bytes) => Cow::Owned(
                String::from_utf8(bytes)
                    .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned()),
            ),
        }
    }

    /// The URL of a `src=`-like prefix ending at `after`: `(start, replacement, end)`, where
    /// `input[start..end]` is the `/` (plus the repeated root) that `replacement` replaces.
    fn single(
        &self,
        input: &[u8],
        after: usize,
        quoting: Quoting,
    ) -> Option<(usize, Vec<u8>, usize)> {
        let at = after + quoting.at(&input[after..]).map_or(0, <[u8]>::len);
        if !is_root_relative(&input[at..]) {
            return None;
        }
        let end = at + 1 + self.root_len(&input[at + 1..]);
        Some((at, self.prefix.clone(), end))
    }

    /// The candidate list of a `srcset=` ending at `after`, up to and including the first byte
    /// of its closing quote.
    fn srcset(
        &self,
        input: &[u8],
        after: usize,
        quoting: Quoting,
    ) -> Option<(usize, Vec<u8>, usize)> {
        let quote = quoting.at(&input[after..])?;
        let at = after + quote.len();
        if !is_root_relative(&input[at..]) {
            return None;
        }
        let close = memchr::memmem::find(&input[at..], quote).filter(|&n| n <= MAX_SRCSET)?;
        let end = at + close + 1;
        let mut replacement = Vec::with_capacity(end - at + self.prefix.len());
        for (i, field) in fields(&input[at..end]).enumerate() {
            if i > 0 {
                replacement.push(b' ');
            }
            match field.strip_prefix(b"/") {
                Some(rest) => {
                    replacement.extend_from_slice(&self.prefix);
                    replacement.extend_from_slice(&rest[self.root_len(rest)..]);
                }
                None => replacement.extend_from_slice(field),
            }
        }
        Some((at, replacement, end))
    }

    /// How many bytes of `rest` (a URL after its leading `/`) repeat the prefix's root.
    fn root_len(&self, rest: &[u8]) -> usize {
        if !self.root.is_empty() && rest.starts_with(&self.root) {
            self.root.len()
        } else {
            0
        }
    }
}

/// `/x` but not `//host` nor a lone `/` at the end of the input.
fn is_root_relative(s: &[u8]) -> bool {
    matches!(s, [b'/', next, ..] if *next != b'/')
}

/// The white-space separated fields of `s` (Unicode white space; bytes that are not UTF-8 are
/// never white space).
fn fields(s: &[u8]) -> impl Iterator<Item = &[u8]> {
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut start: Option<usize> = None;
    let mut pos = 0;
    for chunk in s.utf8_chunks() {
        for c in chunk.valid().chars() {
            if c.is_whitespace() {
                if let Some(st) = start.take() {
                    spans.push((st, pos));
                }
            } else if start.is_none() {
                start = Some(pos);
            }
            pos += c.len_utf8();
        }
        if !chunk.invalid().is_empty() {
            start.get_or_insert(pos);
            pos += chunk.invalid().len();
        }
    }
    if let Some(st) = start {
        spans.push((st, pos));
    }
    spans.into_iter().map(move |(a, b)| &s[a..b])
}

/// The path from the directory of `target` back to the publish root: `./` for a file at the
/// root, `../` for one level down (`/posts/index.html`), and so on. A path that does not end
/// in an extension of one to six characters is a directory (`/posts/one` is `/posts/one/`).
#[must_use]
pub fn dotted_path_to_root(target: &OutputPath) -> String {
    let path = target.as_str();
    let dir = if ends_like_file(path) {
        path.rsplit_once('/').map_or("", |(d, _)| d)
    } else {
        path
    };
    let depth = dir.split('/').filter(|s| !s.is_empty()).count();
    if depth == 0 {
        "./".to_owned()
    } else {
        "../".repeat(depth)
    }
}

/// Whether the path ends in `.` plus one to six characters (none of them a line break).
fn ends_like_file(path: &str) -> bool {
    path.char_indices().rev().take(7).any(|(i, c)| {
        let rest = &path[i + 1..];
        c == '.' && (1..=6).contains(&rest.chars().count()) && !rest.contains('\n')
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rw(prefix: &str, s: &str) -> String {
        UrlRewriter::new(prefix)
            .rewrite_str(s, Quoting::Html)
            .into_owned()
    }

    #[test]
    fn spec_vectors() {
        let b = "https://shop.example/";
        assert_eq!(
            rw(b, r#"<a href="/about">"#),
            r#"<a href="https://shop.example/about">"#
        );
        assert_eq!(
            rw(b, "<a href=/about>"),
            "<a href=https://shop.example/about>"
        );
        assert_eq!(
            rw(b, r#"<a href="//cdn.x/a.js">"#),
            r#"<a href="//cdn.x/a.js">"#
        );
        assert_eq!(
            rw(b, r#"<img srcset="/a.png 1x,  https://x/b.png   2x">"#),
            r#"<img srcset="https://shop.example/a.png 1x, https://x/b.png 2x">"#
        );
        assert_eq!(rw(b, "<img srcset=/a.png>"), "<img srcset=/a.png>");
        assert_eq!(rw(b, r#"<a HREF="/x">"#), r#"<a HREF="/x">"#);
        assert_eq!(
            rw("https://example.org/docs/", r#"href="/docs/a""#),
            r#"href="https://example.org/docs/a""#
        );
    }

    #[test]
    fn dotted() {
        for (p, want) in [
            ("/index.html", "./"),
            ("/posts/index.html", "../"),
            ("/a/b/index.html", "../../"),
            ("/a/b", "../../"),
            ("/404.html", "./"),
        ] {
            assert_eq!(dotted_path_to_root(&OutputPath::new(p)), want, "{p}");
        }
    }
}
