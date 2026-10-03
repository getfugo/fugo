//! The LiveReload script `serve` adds to every HTML output: placed at the start of the
//! document's head, after an optional doctype, `<html>` and `<head>` start tag (and any white
//! space, comments or processing instructions between them).

use ssg_base::url::UrlRef;

/// The `<script>` element loading `livereload.js` from the server at `base` (its path and
/// port are used).
#[must_use]
pub fn script(base: &UrlRef) -> String {
    let path = String::from_utf8_lossy(base.path());
    let path = path.strip_suffix('/').unwrap_or(&path);
    let port = String::from_utf8_lossy(base.port());
    let reload = format!("{path}/livereload");
    let src = format!(
        "{path}/livereload.js?mindelay=10&v=2&port={port}&path={}",
        reload.strip_prefix('/').unwrap_or(&reload)
    );
    format!(
        r#"<script src="{}" data-no-instant defer></script>"#,
        escape_html(&src)
    )
}

/// `html` with `script` inserted at the start of the head.
#[must_use]
pub fn inject(html: &[u8], script: &str) -> Vec<u8> {
    let at = head_start(html);
    let mut out = Vec::with_capacity(html.len() + script.len());
    out.extend_from_slice(&html[..at]);
    out.extend_from_slice(script.as_bytes());
    out.extend_from_slice(&html[at..]);
    out
}

/// Where the head's content starts: past the doctype, `<html …>` and `<head …>`, each
/// optional and in this order.
fn head_start(html: &[u8]) -> usize {
    let mut at = 0;
    for tag in [Opening::Doctype, Opening::Tag("html"), Opening::Tag("head")] {
        at += skip_ignorable(&html[at..]);
        at += tag.matched_len(&html[at..]);
    }
    at
}

#[derive(Clone, Copy)]
enum Opening {
    /// `<!doctype` + white space + anything up to `>`.
    Doctype,
    /// `<name>` or `<name` + white space + anything up to `>`.
    Tag(&'static str),
}

impl Opening {
    fn matched_len(self, s: &[u8]) -> usize {
        let (name, needs_space) = match self {
            Self::Doctype => ("!doctype", true),
            Self::Tag(name) => (name, false),
        };
        let Some(rest) = s.strip_prefix(b"<") else {
            return 0;
        };
        if rest.len() < name.len() || !rest[..name.len()].eq_ignore_ascii_case(name.as_bytes()) {
            return 0;
        }
        let rest = &rest[name.len()..];
        let head = 1 + name.len();
        match rest.first() {
            Some(b'>') if !needs_space => head + 1,
            Some(&c) if is_space(c) => rest
                .iter()
                .position(|&b| b == b'>')
                .map_or(0, |gt| head + gt + 1),
            _ => 0,
        }
    }
}

/// The length of the white space, comments and processing instructions at the start of `s`.
fn skip_ignorable(s: &[u8]) -> usize {
    let mut at = 0;
    loop {
        let rest = &s[at..];
        let n = if rest.first().is_some_and(|&c| is_space(c)) {
            rest.iter()
                .position(|&c| !is_space(c))
                .unwrap_or(rest.len())
        } else if rest.starts_with(b"<!--") {
            memchr::memmem::find(&rest[4..], b"-->").map_or(0, |end| 4 + end + 3)
        } else if rest.starts_with(b"<?") {
            memchr::memmem::find(&rest[2..], b"?>").map_or(0, |end| 2 + end + 2)
        } else {
            0
        };
        if n == 0 {
            return at;
        }
        at += n;
    }
}

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\x0c' | b'\r')
}

fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '\'' => out.push_str("&#39;"),
            '"' => out.push_str("&#34;"),
            c => out.push(c),
        }
    }
    out
}
