//! Linkify with goldmark's rules: bare `http(s)://`, `ftp://` and `www.` URLs and e-mail
//! addresses become links where an inline may start (see [`super::inline`]).

use std::sync::LazyLock;

use regex::Regex;

static URL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"^(?:http|https|ftp)://[-a-zA-Z0-9@:%._\+~#=]{1,256}\.[a-z]+(?::\d+)?(?:[/#?][-a-zA-Z0-9@:%_\+.~#$!?&/=\(\);,'">\^{}\[\]`]*)?"#,
    )
    .expect("valid URL pattern")
});

static WWW: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"^www\.[-a-zA-Z0-9@:%._\+~#=]{1,256}\.[a-z]+(?:[/#?][-a-zA-Z0-9@:%_\+.~#!?&/=\(\);,'">\^{}\[\]`]*)?"#,
    )
    .expect("valid www pattern")
});

static EMAIL_DOMAIN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*",
    )
    .expect("valid domain pattern")
});

/// A link found at the start of a string.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Found {
    pub len: usize,
    pub url: String,
    /// A `www.` link (gets `linkifyProtocol` when rendered).
    pub www: bool,
}

/// Characters after which a link may start (besides where an inline may start).
pub(crate) const TRIGGERS: &[u8] = b" *_~(";

/// The link at the start of `s`, if any.
pub(crate) fn find(s: &str) -> Option<Found> {
    let (len, www, email) = if let Some(m) = URL.find(s) {
        (trim(&s[..m.end()]), false, false)
    } else if let Some(m) = WWW.find(s) {
        (trim(&s[..m.end()]), true, false)
    } else {
        (email(s)?, false, true)
    };
    let b = s.as_bytes();
    let mut len = len;
    while len > 0 && b"?!.,:*_~".contains(&b[len - 1]) {
        len -= 1;
    }
    if len == 0 {
        return None;
    }
    let url = if email {
        format!("mailto:{}", &s[..len])
    } else {
        s[..len].to_owned()
    };
    Some(Found { len, url, www })
}

/// goldmark's trimming of a matched URL: one trailing `.`, unbalanced `)`, or a trailing
/// entity reference.
fn trim(url: &str) -> usize {
    let b = url.as_bytes();
    let mut end = b.len();
    match b.last() {
        Some(b'.') => end -= 1,
        Some(b')') => {
            let closing = b.iter().fold(0i64, |n, &c| match c {
                b')' => n + 1,
                b'(' => n - 1,
                _ => n,
            });
            if closing > 0 {
                end -= usize::try_from(closing).unwrap_or(0).min(end);
            }
        }
        Some(b';') => {
            let body = &b[..end - 1];
            let alnum = body
                .iter()
                .rev()
                .take_while(|c| c.is_ascii_alphanumeric())
                .count();
            if alnum > 0 && body.len() > alnum && body[body.len() - alnum - 1] == b'&' {
                end = body.len() - alnum - 1;
            }
        }
        _ => {}
    }
    end
}

fn is_email_local(c: u8) -> bool {
    c.is_ascii_alphanumeric() || b".!#$%&'*+/=?^_`{|}~-".contains(&c)
}

/// The length of an e-mail address at the start of `s` (which must not start with
/// punctuation).
fn email(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    if b.first().is_some_and(u8::is_ascii_punctuation) {
        return None;
    }
    let at = b.iter().take_while(|&&c| is_email_local(c)).count();
    if at == 0 || b.get(at) != Some(&b'@') {
        return None;
    }
    let domain = EMAIL_DOMAIN.find(&s[at + 1..])?;
    let mut d = domain.as_str();
    if !d.contains('.') {
        return None;
    }
    if let Some(stripped) = d.strip_suffix('.') {
        d = stripped;
    }
    let end = at + 1 + d.len();
    if matches!(b.get(end), Some(b'-' | b'_')) {
        return None;
    }
    Some(end)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Links in plain text (every trigger or link end is a possible start).
    fn urls(s: &str) -> Vec<(String, String)> {
        let b = s.as_bytes();
        let mut out = Vec::new();
        let (mut i, mut head) = (0, true);
        while i < b.len() {
            if head && let Some(f) = find(&s[i..]) {
                let url = if f.www { format!("+{}", f.url) } else { f.url };
                out.push((s[i..i + f.len].to_owned(), url));
                i += f.len;
                continue;
            }
            head = TRIGGERS.contains(&b[i]);
            i += s[i..].chars().next().map_or(1, char::len_utf8);
        }
        out
    }

    #[test]
    fn goldmark_rules() {
        assert_eq!(
            urls("see http://www.copyright.gov."),
            vec![(
                "http://www.copyright.gov".into(),
                "http://www.copyright.gov".into()
            )]
        );
        assert_eq!(
            urls("mail blackb1rd@blackb1rd.me."),
            vec![(
                "blackb1rd@blackb1rd.me".into(),
                "mailto:blackb1rd@blackb1rd.me".into()
            )]
        );
        assert_eq!(urls("WORK:jsmith@example.org"), vec![]);
        assert_eq!(urls("x 'jsmith@example.org'"), vec![]);
        assert_eq!(
            urls("(https://example.org/a_(b)) x"),
            vec![(
                "https://example.org/a_(b)".into(),
                "https://example.org/a_(b)".into()
            )]
        );
        assert_eq!(
            urls("at www.example.org/'"),
            vec![("www.example.org/'".into(), "+www.example.org/'".into())]
        );
        assert_eq!(urls("xhttp://a.org"), vec![]);
    }
}
