//! URL references, percent-escaping and the site's base URL.
//!
//! [`UrlRef`] splits a URI reference into scheme, authority, path, query and fragment with the
//! rules Go's URLs are defined by (RFC 3986 syntax, parsed leniently): the path, host and
//! fragment are kept percent-decoded; an escaping of the original input is remembered and
//! reused when it still decodes to the component (so `%2F` survives a round trip), otherwise
//! the component is escaped with the component's character set ([`Component`]). Hex digits are
//! written in upper case.
//!
//! [`BaseUrl`] is the site's `baseURL` (always with a trailing slash) and [`SiteUrls`] the
//! URL helpers a language of a site uses (`absURL`, `relURL`, `urlize`, …).

use std::borrow::Cow;
use std::fmt;

use crate::paths;
use crate::text;

mod reference;
mod site;

pub use reference::*;
pub use site::*;

/// Why a URL reference could not be parsed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum UrlError {
    #[error("URL {0:?} contains an ASCII control character")]
    ControlCharacter(String),
    #[error("URL {0:?} starts with ':' (missing scheme)")]
    MissingScheme(String),
    #[error("URL {0:?} has a ':' in its first path segment (use \"./\" to make it a path)")]
    ColonInFirstSegment(String),
    #[error("invalid percent-escape {0:?}")]
    InvalidEscape(String),
    #[error("invalid character {0:?} in host")]
    InvalidHostCharacter(char),
    #[error("invalid port {0:?}")]
    InvalidPort(String),
    #[error("host {0:?} has no closing ']'")]
    UnclosedIpLiteral(String),
    #[error("invalid user info {0:?}")]
    InvalidUserInfo(String),
    #[error("cannot make a permalink from the absolute link {0:?}")]
    AbsoluteLink(String),
    #[error("cannot give {url:?} the protocol {protocol:?}: it has no host")]
    OpaqueProtocol { url: String, protocol: String },
    #[error("the base URL {0:?} decodes to a path that is not valid UTF-8")]
    NonUtf8Path(String),
}

/// The URL component whose character set an escape uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Component {
    /// A whole path: only `?` of the reserved characters is escaped.
    Path,
    /// One path segment: `/`, `;`, `,` and `?` are escaped as well.
    PathSegment,
    /// A host name.
    Host,
    /// An IPv6 zone identifier (`%25en0`).
    Zone,
    /// User name or password.
    UserPassword,
    /// A query key or value: every reserved character is escaped, space becomes `+`.
    QueryComponent,
    /// A fragment: the reserved characters and `!()*` stay.
    Fragment,
}

/// Whether byte `c` must be percent-escaped in `component`.
#[must_use]
pub fn should_escape(c: u8, component: Component) -> bool {
    use Component as C;
    if c.is_ascii_alphanumeric() {
        return false;
    }
    if matches!(component, C::Host | C::Zone)
        && matches!(
            c,
            b'!' | b'$'
                | b'&'
                | b'\''
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'='
                | b':'
                | b'['
                | b']'
                | b'<'
                | b'>'
                | b'"'
        )
    {
        return false;
    }
    match c {
        b'-' | b'_' | b'.' | b'~' => return false,
        b'$' | b'&' | b'+' | b',' | b'/' | b':' | b';' | b'=' | b'?' | b'@' => match component {
            C::Path => return c == b'?',
            C::PathSegment => return matches!(c, b'/' | b';' | b',' | b'?'),
            C::UserPassword => return matches!(c, b'@' | b'/' | b'?' | b':'),
            C::QueryComponent => return true,
            C::Fragment => return false,
            C::Host | C::Zone => {}
        },
        _ => {}
    }
    !(component == C::Fragment && matches!(c, b'!' | b'(' | b')' | b'*'))
}

/// Percent-escapes the bytes of `s` that [`should_escape`] in `component` (upper-case hex;
/// space becomes `+` in a query component).
#[must_use]
pub fn escape(s: &[u8], component: Component) -> Cow<'_, str> {
    if !s.iter().any(|&c| should_escape(c, component)) {
        // Nothing to escape means every byte is ASCII.
        return Cow::Borrowed(std::str::from_utf8(s).expect("ASCII"));
    }
    let mut out = String::with_capacity(s.len() + 8);
    for &c in s {
        if !should_escape(c, component) {
            out.push(char::from(c));
        } else if c == b' ' && component == Component::QueryComponent {
            out.push('+');
        } else {
            push_hex(&mut out, c);
        }
    }
    Cow::Owned(out)
}

fn push_hex(out: &mut String, c: u8) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    out.push('%');
    out.push(char::from(HEX[usize::from(c >> 4)]));
    out.push(char::from(HEX[usize::from(c & 15)]));
}

fn hex_value(c: u8) -> Option<u8> {
    char::from(c)
        .to_digit(16)
        .map(|d| u8::try_from(d).expect("hex digit"))
}

/// Decodes the percent-escapes of `s` for `component` (`+` becomes space in a query
/// component). Hosts only accept escapes of non-ASCII bytes (and `%25`), and reject the ASCII
/// characters a host cannot contain.
///
/// # Errors
/// An escape that is not `%` and two hex digits, or a character or escape the host component
/// does not allow.
pub fn unescape(s: &str, component: Component) -> Result<Vec<u8>, UrlError> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' => {
                let (Some(hi), Some(lo)) = (
                    b.get(i + 1).copied().and_then(hex_value),
                    b.get(i + 2).copied().and_then(hex_value),
                ) else {
                    let end = (i + 3).min(b.len());
                    return Err(UrlError::InvalidEscape(
                        String::from_utf8_lossy(&b[i..end]).into_owned(),
                    ));
                };
                let is_25 = &b[i..i + 3] == b"%25";
                let v = hi << 4 | lo;
                let bad = match component {
                    Component::Host => hi < 8 && !is_25,
                    Component::Zone => {
                        !is_25 && v != b' ' && v < 0x80 && should_escape(v, Component::Host)
                    }
                    _ => false,
                };
                if bad {
                    return Err(UrlError::InvalidEscape(s[i..i + 3].to_owned()));
                }
                out.push(v);
                i += 3;
            }
            b'+' if component == Component::QueryComponent => {
                out.push(b' ');
                i += 1;
            }
            c => {
                if matches!(component, Component::Host | Component::Zone)
                    && c < 0x80
                    && should_escape(c, component)
                {
                    return Err(UrlError::InvalidHostCharacter(char::from(c)));
                }
                out.push(c);
                i += 1;
            }
        }
    }
    Ok(out)
}

/// Whether `s` is a valid escaping for `component`: every byte either needs no escape, is a
/// sub-delimiter, `:`, `@`, `[`, `]` or starts an escape.
pub(crate) fn valid_encoded(s: &str, component: Component) -> bool {
    s.bytes().all(|c| {
        matches!(
            c,
            b'!' | b'$'
                | b'&'
                | b'\''
                | b'('
                | b')'
                | b'*'
                | b'+'
                | b','
                | b';'
                | b'='
                | b':'
                | b'@'
                | b'['
                | b']'
                | b'%'
        ) || !should_escape(c, component)
    })
}

/// Percent-escapes `s` as a URL reference: parses it and writes it back.
///
/// # Errors
/// When `s` is not a URL reference ([`UrlRef::parse`]).
pub fn url_escape(s: &str) -> Result<String, UrlError> {
    UrlRef::parse(s).map(|u| u.to_string())
}

/// The escaped path of `s` parsed as a URL reference.
///
/// # Errors
/// When `s` is not a URL reference.
pub fn path_escape(s: &str) -> Result<String, UrlError> {
    UrlRef::parse(s).map(|u| u.escaped_path().into_owned())
}

/// Whether `s` is an absolute URL (has a scheme).
///
/// # Errors
/// When `s` is not a URL reference (and does not start with `http://` or `https://`).
pub fn is_abs_url(s: &str) -> Result<bool, UrlError> {
    if s.starts_with("http://") || s.starts_with("https://") {
        return Ok(true);
    }
    UrlRef::parse(s).map(|u| u.is_absolute())
}

/// Joins the path of `link` onto the path of `host` and takes over `link`'s query and
/// fragment. A trailing slash of `link` (or of `host` when `link` is empty) is kept.
///
/// # Errors
/// When either is not a URL reference, or `link` has a host.
pub fn make_permalink(host: &str, link: &str) -> Result<UrlRef, UrlError> {
    let mut base = UrlRef::parse(host)?;
    let p = UrlRef::parse(link)?;
    if p.has_host() {
        return Err(UrlError::AbsoluteLink(link.to_owned()));
    }
    let mut path = paths::join_bytes(&[base.path(), p.path()]);
    let trailing = (link.is_empty() && host.ends_with('/')) || p.path.ends_with(b"/");
    if trailing && !path.ends_with(b"/") {
        path.push(b'/');
    }
    base.set_path(path);
    base.fragment = p.fragment;
    base.raw_query = p.raw_query;
    Ok(base)
}

/// The decoded path of `base_url` joined with `relative_path` (a trailing slash of
/// `relative_path` is kept).
///
/// # Errors
/// When `base_url` is not a URL reference.
pub fn add_context_root(base_url: &str, relative_path: &str) -> Result<Vec<u8>, UrlError> {
    let url = UrlRef::parse(base_url)?;
    let mut path = paths::join_bytes(&[url.path(), relative_path.as_bytes()]);
    if path != b"/" && relative_path.ends_with('/') {
        path.push(b'/');
    }
    Ok(path)
}
