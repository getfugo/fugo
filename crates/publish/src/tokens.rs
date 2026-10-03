//! URL tokens: the URL-shaped words of rendered outputs, which decide which lazily published
//! resources (assets, remote and transformed resources, `publishResources = false` bundles)
//! are written (docs/rust-port/REWRITE_PLAN.md §3.4).
//!
//! An output is decoded first (HTML character references such as `&amp;`, `&#39;` and
//! `&#x27;`, then JSON string escapes such as `\/` and `\u0026`), so that `/b/Lay&#39;s.jpg`
//! and `\/b\/salt-\u0026-vinegar.jpg` yield the URLs they spell. The text is then split into
//! words at white space, `"`, `<`, `>`, `` ` ``, parentheses, braces and backslashes. A word
//! containing `/` is a token, and so is every `/`-containing part of it split at `'`, `,` and
//! `;` (for `src='/x'` and `srcset="/a 1x,/b 2x"`), and the value of an unquoted attribute
//! (`href=/x`). Each token is also added without its `?query` and `#fragment`. Tokens
//! starting with `./` or `../` (from `relativeURLs`) are resolved against the output's
//! directory.
//!
//! The resolution against resource URLs lives in `ssg-resources`, which does not depend on
//! this crate: `ssg-build` hands [`UrlTokens::iter`] to the resource store.

use std::collections::BTreeSet;

use ssg_base::paths::{self, OutputPath};

/// The longest token kept (longer words are not URLs of published files).
const MAX_TOKEN: usize = 2048;

/// A sorted set of URL tokens.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UrlTokens(BTreeSet<String>);

impl UrlTokens {
    /// No tokens.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The tokens of `text`, an output at `from` (`None`: relative tokens stay as written).
    #[must_use]
    pub fn extract(text: &str, from: Option<&OutputPath>) -> Self {
        let mut t = Self::new();
        t.add_text(text, from);
        t
    }

    /// Adds the tokens of `text`, an output at `from`.
    pub fn add_text(&mut self, text: &str, from: Option<&OutputPath>) {
        if !text.contains('/') {
            return;
        }
        let decoded = decode_json(&decode_html(text));
        let dir = from.map(|p| paths::dir(p.as_str()).to_owned());
        for word in decoded.split(is_hard_delimiter) {
            if word.is_empty() || word.len() > MAX_TOKEN || !word.contains('/') {
                continue;
            }
            self.add_word(word, dir.as_deref());
            if word.contains(is_soft_delimiter) {
                for part in word.split(is_soft_delimiter) {
                    if part.contains('/') {
                        self.add_word(part, dir.as_deref());
                    }
                }
            }
        }
    }

    /// Adds `word`, and its value when it is an unquoted attribute (`href=/x`).
    fn add_word(&mut self, word: &str, dir: Option<&str>) {
        self.add_token(word, dir);
        if let Some((name, value)) = word.split_once('=')
            && !name.contains('/')
            && value.contains('/')
        {
            self.add_token(value, dir);
        }
    }

    fn add_token(&mut self, token: &str, dir: Option<&str>) {
        let resolved = match dir {
            Some(dir) if token.starts_with("./") || token.starts_with("../") => {
                let (path, rest) = token.split_at(token.find(['?', '#']).unwrap_or(token.len()));
                let mut joined = paths::clean(&format!("{dir}/{path}"));
                if path.ends_with('/') && !joined.ends_with('/') {
                    joined.push('/');
                }
                format!("{joined}{rest}")
            }
            _ => token.to_owned(),
        };
        if let Some(end) = resolved.find(['?', '#'])
            && resolved[..end].contains('/')
        {
            self.0.insert(resolved[..end].to_owned());
        }
        self.0.insert(resolved);
    }

    /// Adds every token of `other`.
    pub fn merge(&mut self, other: Self) {
        if self.0.is_empty() {
            self.0 = other.0;
        } else {
            self.0.extend(other.0);
        }
    }

    /// The tokens, sorted.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &str> + Clone {
        self.0.iter().map(String::as_str)
    }

    /// Whether `token` is one of the tokens.
    #[must_use]
    pub fn contains(&self, token: &str) -> bool {
        self.0.contains(token)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<'a> IntoIterator for &'a UrlTokens {
    type Item = &'a str;
    type IntoIter =
        std::iter::Map<std::collections::btree_set::Iter<'a, String>, fn(&String) -> &str>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter().map(String::as_str)
    }
}

fn is_hard_delimiter(c: char) -> bool {
    c.is_whitespace() || matches!(c, '"' | '<' | '>' | '`' | '(' | ')' | '{' | '}' | '\\')
}

fn is_soft_delimiter(c: char) -> bool {
    matches!(c, '\'' | ',' | ';')
}

/// Decodes the character references a URL in HTML can carry: `&amp;`, `&quot;`, `&apos;`,
/// `&lt;`, `&gt;` and numeric references; anything else stays as written.
fn decode_html(s: &str) -> std::borrow::Cow<'_, str> {
    if !s.contains('&') {
        return s.into();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let reference = rest[1..].find(';').filter(|&n| n <= 8).and_then(|n| {
            let name = &rest[1..=n];
            let c = match name {
                "amp" => Some('&'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "lt" => Some('<'),
                "gt" => Some('>'),
                _ => name.strip_prefix('#').and_then(|num| {
                    match num.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok(),
                        None => num.parse().ok(),
                    }
                    .and_then(char::from_u32)
                }),
            };
            c.map(|c| (c, n + 2))
        });
        match reference {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out.into()
}

/// Decodes JSON string escapes (`\/`, `\"`, `\\`, `\n`, `\uXXXX` with surrogate pairs); an
/// invalid escape stays as written.
fn decode_json(s: &str) -> String {
    if !s.contains('\\') {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('\\') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let (decoded, len) = match rest.as_bytes().get(1) {
            Some(b'/') => (Some('/'), 2),
            Some(b'"') => (Some('"'), 2),
            Some(b'\\') => (Some('\\'), 2),
            Some(b'n') => (Some('\n'), 2),
            Some(b't') => (Some('\t'), 2),
            Some(b'r') => (Some('\r'), 2),
            Some(b'u') => unicode_escape(rest),
            _ => (None, 1),
        };
        match decoded {
            Some(c) => out.push(c),
            None => out.push_str(&rest[..len]),
        }
        rest = &rest[len..];
    }
    out.push_str(rest);
    out
}

/// `\uXXXX` (or a surrogate pair `😀`) at the start of `s`: the character and the
/// escape's length.
fn unicode_escape(s: &str) -> (Option<char>, usize) {
    let unit = |at: usize| {
        s.get(at..at + 6)
            .filter(|e| e.starts_with("\\u"))
            .and_then(|e| u32::from_str_radix(&e[2..], 16).ok())
    };
    match unit(0) {
        Some(hi @ 0xD800..=0xDBFF) => match unit(6) {
            Some(lo @ 0xDC00..=0xDFFF) => (
                char::from_u32(0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00)),
                12,
            ),
            _ => (None, 6),
        },
        Some(u) => (char::from_u32(u), 6),
        None => (None, 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(text: &str, from: Option<&str>) -> Vec<String> {
        let from = from.map(OutputPath::new);
        UrlTokens::extract(text, from.as_ref())
            .0
            .into_iter()
            .collect()
    }

    #[test]
    fn escapes_and_srcset() {
        let t = tokens(
            concat!(
                r#"<img src="/b/salt-&amp;-vinegar/Lay&#39;s.jpg" srcset="/a.jpg 1x,/b.jpg?v=1 2x">"#,
                r#"<script>{"u":"\/c\/x\u0026y.png"}</script>"#
            ),
            None,
        );
        for want in [
            "/b/salt-&-vinegar/Lay's.jpg",
            "/a.jpg",
            "/b.jpg",
            "/b.jpg?v=1",
            "/c/x&y.png",
        ] {
            assert!(t.iter().any(|x| x == want), "{want} not in {t:?}");
        }
    }

    #[test]
    fn relative_and_unquoted() {
        let t = tokens(
            "<a href=../../css/a.css#x><link href='/x/y.css'>",
            Some("/a/b/index.html"),
        );
        assert!(t.contains(&"/css/a.css".to_owned()), "{t:?}");
        assert!(t.contains(&"/css/a.css#x".to_owned()), "{t:?}");
        assert!(t.contains(&"/x/y.css".to_owned()), "{t:?}");
    }
}
