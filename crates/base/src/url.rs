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

/// The `username[:password]` of an authority (decoded).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct UserInfo {
    pub username: Vec<u8>,
    pub password: Option<Vec<u8>>,
}

impl fmt::Display for UserInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&escape(&self.username, Component::UserPassword))?;
        if let Some(p) = &self.password {
            write!(f, ":{}", escape(p, Component::UserPassword))?;
        }
        Ok(())
    }
}

/// A parsed URI reference. Components are stored decoded; [`fmt::Display`] re-escapes them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct UrlRef {
    scheme: String,
    opaque: String,
    user: Option<UserInfo>,
    host: Vec<u8>,
    path: Vec<u8>,
    /// The escaping of the path as parsed, when it differs from the default escaping.
    raw_path: String,
    /// `scheme:/path`: an empty authority that is not written back.
    omit_host: bool,
    /// A lone `?` with an empty query.
    force_query: bool,
    raw_query: String,
    fragment: Vec<u8>,
    raw_fragment: String,
}

impl UrlRef {
    /// Parses a URI reference.
    ///
    /// # Errors
    /// Control characters, a leading `:`, a `:` in the first segment of a relative path, bad
    /// percent-escapes, or an invalid host, port or user info.
    pub fn parse(input: &str) -> Result<Self, UrlError> {
        let (rest, fragment) = match input.split_once('#') {
            Some((r, f)) => (r, Some(f)),
            None => (input, None),
        };
        let mut url = Self::parse_without_fragment(input, rest)?;
        if let Some(f) = fragment {
            url.fragment = unescape(f, Component::Fragment)?;
            if escape(&url.fragment, Component::Fragment) != f {
                f.clone_into(&mut url.raw_fragment);
            }
        }
        Ok(url)
    }

    fn parse_without_fragment(input: &str, raw: &str) -> Result<Self, UrlError> {
        if raw.bytes().any(|b| b < 0x20 || b == 0x7f) {
            return Err(UrlError::ControlCharacter(input.to_owned()));
        }
        let mut url = Self::default();
        if raw == "*" {
            url.path = b"*".to_vec();
            return Ok(url);
        }
        let (scheme, rest) =
            split_scheme(raw).ok_or_else(|| UrlError::MissingScheme(input.to_owned()))?;
        url.scheme = scheme.to_ascii_lowercase();
        let mut rest = rest;
        if rest.ends_with('?') && rest.matches('?').count() == 1 {
            url.force_query = true;
            rest = &rest[..rest.len() - 1];
        } else if let Some((r, q)) = rest.split_once('?') {
            q.clone_into(&mut url.raw_query);
            rest = r;
        }
        if !rest.starts_with('/') {
            if !url.scheme.is_empty() {
                rest.clone_into(&mut url.opaque);
                return Ok(url);
            }
            let first_segment = rest.split('/').next().unwrap_or_default();
            if first_segment.contains(':') {
                return Err(UrlError::ColonInFirstSegment(input.to_owned()));
            }
        }
        if (!url.scheme.is_empty() || !rest.starts_with("///")) && rest.starts_with("//") {
            let authority = &rest[2..];
            let (authority, path) = match authority.find('/') {
                Some(i) => authority.split_at(i),
                None => (authority, ""),
            };
            let (user, host) = parse_authority(authority)?;
            url.user = user;
            url.host = host;
            rest = path;
        } else if !url.scheme.is_empty() && rest.starts_with('/') {
            url.omit_host = true;
        }
        url.path = unescape(rest, Component::Path)?;
        if escape(&url.path, Component::Path) != rest {
            rest.clone_into(&mut url.raw_path);
        }
        Ok(url)
    }

    /// The lower-cased scheme (empty for a relative reference).
    #[must_use]
    pub fn scheme(&self) -> &str {
        &self.scheme
    }

    /// Whether the reference has a scheme.
    #[must_use]
    pub fn is_absolute(&self) -> bool {
        !self.scheme.is_empty()
    }

    /// The decoded host, with any port.
    #[must_use]
    pub fn host(&self) -> &[u8] {
        &self.host
    }

    /// Whether the reference has a (non-empty) host.
    #[must_use]
    pub fn has_host(&self) -> bool {
        !self.host.is_empty()
    }

    /// The host without port and IPv6 brackets.
    #[must_use]
    pub fn hostname(&self) -> &[u8] {
        split_host_port(&self.host).0
    }

    /// The port, without the colon (empty when there is none).
    #[must_use]
    pub fn port(&self) -> &[u8] {
        split_host_port(&self.host).1
    }

    /// Replaces the host (decoded).
    pub fn set_host(&mut self, host: impl Into<Vec<u8>>) {
        self.host = host.into();
    }

    /// The decoded path.
    #[must_use]
    pub fn path(&self) -> &[u8] {
        &self.path
    }

    /// Replaces the decoded path. The escaping seen when parsing is still used if it decodes
    /// to the new path.
    pub fn set_path(&mut self, path: impl Into<Vec<u8>>) {
        self.path = path.into();
    }

    /// The raw query, without `?`.
    #[must_use]
    pub fn raw_query(&self) -> &str {
        &self.raw_query
    }

    /// Replaces the raw query.
    pub fn set_raw_query(&mut self, query: &str) {
        query.clone_into(&mut self.raw_query);
    }

    /// The decoded fragment.
    #[must_use]
    pub fn fragment(&self) -> &[u8] {
        &self.fragment
    }

    /// Replaces the decoded fragment.
    pub fn set_fragment(&mut self, fragment: impl Into<Vec<u8>>) {
        self.fragment = fragment.into();
    }

    /// Replaces the scheme.
    pub fn set_scheme(&mut self, scheme: &str) {
        scheme.clone_into(&mut self.scheme);
    }

    /// The opaque part of `scheme:opaque` (a scheme followed by something other than `/`).
    #[must_use]
    pub fn opaque(&self) -> &str {
        &self.opaque
    }

    /// Replaces the opaque part.
    pub fn set_opaque(&mut self, opaque: String) {
        self.opaque = opaque;
    }

    /// The escaped path: the parsed escaping when it is valid and still decodes to the path,
    /// else the path escaped as a [`Component::Path`].
    #[must_use]
    pub fn escaped_path(&self) -> Cow<'_, str> {
        if !self.raw_path.is_empty()
            && valid_encoded(&self.raw_path, Component::Path)
            && unescape(&self.raw_path, Component::Path).is_ok_and(|p| p == self.path)
        {
            return Cow::Borrowed(&self.raw_path);
        }
        if self.path == b"*" {
            return Cow::Borrowed("*");
        }
        escape(&self.path, Component::Path)
    }

    /// The escaped fragment (same rule as [`escaped_path`](Self::escaped_path)).
    #[must_use]
    pub fn escaped_fragment(&self) -> Cow<'_, str> {
        if !self.raw_fragment.is_empty()
            && valid_encoded(&self.raw_fragment, Component::Fragment)
            && unescape(&self.raw_fragment, Component::Fragment).is_ok_and(|f| f == self.fragment)
        {
            return Cow::Borrowed(&self.raw_fragment);
        }
        escape(&self.fragment, Component::Fragment)
    }
}

impl fmt::Display for UrlRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut out = String::new();
        if !self.scheme.is_empty() {
            out.push_str(&self.scheme);
            out.push(':');
        }
        if self.opaque.is_empty() {
            let has_authority =
                !self.scheme.is_empty() || !self.host.is_empty() || self.user.is_some();
            if has_authority && !(self.omit_host && self.host.is_empty() && self.user.is_none()) {
                if !self.host.is_empty() || !self.path.is_empty() || self.user.is_some() {
                    out.push_str("//");
                }
                if let Some(u) = &self.user {
                    out.push_str(&u.to_string());
                    out.push('@');
                }
                out.push_str(&escape(&self.host, Component::Host));
            }
            let path = self.escaped_path();
            if !path.is_empty() && !path.starts_with('/') && !self.host.is_empty() {
                out.push('/');
            }
            if out.is_empty() && path.split('/').next().is_some_and(|s| s.contains(':')) {
                // A relative path whose first segment has a colon would read as a scheme.
                out.push_str("./");
            }
            out.push_str(&path);
        } else {
            out.push_str(&self.opaque);
        }
        if self.force_query || !self.raw_query.is_empty() {
            out.push('?');
            out.push_str(&self.raw_query);
        }
        if !self.fragment.is_empty() {
            out.push('#');
            out.push_str(&self.escaped_fragment());
        }
        f.write_str(&out)
    }
}

/// Splits `scheme:rest`. `None` for a leading `:`; an empty scheme when there is none.
fn split_scheme(raw: &str) -> Option<(&str, &str)> {
    for (i, c) in raw.bytes().enumerate() {
        match c {
            b'a'..=b'z' | b'A'..=b'Z' => {}
            b'0'..=b'9' | b'+' | b'-' | b'.' if i > 0 => {}
            b':' if i == 0 => return None,
            b':' => return Some((&raw[..i], &raw[i + 1..])),
            _ => return Some(("", raw)),
        }
    }
    Some(("", raw))
}

type Authority = (Option<UserInfo>, Vec<u8>);

fn parse_authority(authority: &str) -> Result<Authority, UrlError> {
    let (userinfo, host) = match authority.rfind('@') {
        Some(i) => (Some(&authority[..i]), &authority[i + 1..]),
        None => (None, authority),
    };
    let host = parse_host(host)?;
    let Some(userinfo) = userinfo else {
        return Ok((None, host));
    };
    let valid = userinfo.chars().all(|c| {
        c.is_ascii_alphanumeric()
            || matches!(
                c,
                '-' | '.'
                    | '_'
                    | ':'
                    | '~'
                    | '!'
                    | '$'
                    | '&'
                    | '\''
                    | '('
                    | ')'
                    | '*'
                    | '+'
                    | ','
                    | ';'
                    | '='
                    | '%'
                    | '@'
            )
    });
    if !valid {
        return Err(UrlError::InvalidUserInfo(userinfo.to_owned()));
    }
    let user = match userinfo.split_once(':') {
        None => UserInfo {
            username: unescape(userinfo, Component::UserPassword)?,
            password: None,
        },
        Some((u, p)) => UserInfo {
            username: unescape(u, Component::UserPassword)?,
            password: Some(unescape(p, Component::UserPassword)?),
        },
    };
    Ok((Some(user), host))
}

/// Whether `s` is empty or `:` followed by digits.
fn valid_optional_port(s: &str) -> bool {
    s.is_empty()
        || s.strip_prefix(':')
            .is_some_and(|d| d.bytes().all(|c| c.is_ascii_digit()))
}

fn parse_host(host: &str) -> Result<Vec<u8>, UrlError> {
    if host.starts_with('[') {
        let Some(close) = host.rfind(']') else {
            return Err(UrlError::UnclosedIpLiteral(host.to_owned()));
        };
        let port = &host[close + 1..];
        if !valid_optional_port(port) {
            return Err(UrlError::InvalidPort(port.to_owned()));
        }
        if let Some(zone) = host[..close].find("%25") {
            let mut out = unescape(&host[..zone], Component::Host)?;
            out.extend(unescape(&host[zone..close], Component::Zone)?);
            out.extend(unescape(&host[close..], Component::Host)?);
            return Ok(out);
        }
    } else if let Some(colon) = host.rfind(':') {
        let port = &host[colon..];
        if !valid_optional_port(port) {
            return Err(UrlError::InvalidPort(port.to_owned()));
        }
    }
    unescape(host, Component::Host)
}

fn split_host_port(host_port: &[u8]) -> (&[u8], &[u8]) {
    let (mut host, mut port): (&[u8], &[u8]) = (host_port, b"");
    if let Some(colon) = host_port.iter().rposition(|&c| c == b':')
        && host_port[colon + 1..].iter().all(u8::is_ascii_digit)
    {
        host = &host_port[..colon];
        port = &host_port[colon + 1..];
    }
    if let Some(inner) = host.strip_prefix(b"[").and_then(|h| h.strip_suffix(b"]")) {
        host = inner;
    }
    (host, port)
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

/// The site's base URL, normalised to end with a slash.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BaseUrl {
    url: UrlRef,
    with_path: String,
    without_path: String,
    base_path: String,
}

impl Default for BaseUrl {
    fn default() -> Self {
        Self::parse("/").expect("\"/\" is a base URL")
    }
}

impl BaseUrl {
    /// Parses `baseURL`; an empty value means `/`.
    ///
    /// # Errors
    /// When it is not a URL reference, or its decoded path is not UTF-8.
    pub fn parse(s: &str) -> Result<Self, UrlError> {
        Self::from_url(UrlRef::parse(s)?)
    }

    fn from_url(mut url: UrlRef) -> Result<Self, UrlError> {
        if !url.path.ends_with(b"/") {
            url.path.push(b'/');
        }
        let with_path = url.to_string();
        let base_path = String::from_utf8(url.path.clone())
            .map_err(|_| UrlError::NonUtf8Path(with_path.clone()))?;
        let mut no_path = url.clone();
        no_path.path.clear();
        let without_path = no_path.to_string();
        Ok(Self {
            url,
            with_path,
            without_path,
            base_path,
        })
    }

    /// The URL, with its path (`https://example.org/docs/`).
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.with_path
    }

    /// [`as_str`](Self::as_str) without its trailing slash.
    #[must_use]
    pub fn with_path_no_trailing_slash(&self) -> &str {
        self.with_path.strip_suffix('/').unwrap_or(&self.with_path)
    }

    /// The URL without its path (`https://example.org`).
    #[must_use]
    pub fn without_path(&self) -> &str {
        &self.without_path
    }

    /// The decoded path, with a trailing slash (`/docs/`).
    #[must_use]
    pub fn base_path(&self) -> &str {
        &self.base_path
    }

    /// The decoded path without its trailing slash (`/docs`, or empty).
    #[must_use]
    pub fn base_path_no_trailing_slash(&self) -> &str {
        self.base_path.strip_suffix('/').unwrap_or(&self.base_path)
    }

    /// [`as_str`](Self::as_str) with the decoded path removed from its end (the path is
    /// removed only when it appears unescaped).
    #[must_use]
    pub fn host_url(&self) -> &str {
        self.with_path
            .strip_suffix(self.base_path.as_str())
            .unwrap_or(&self.with_path)
    }

    /// The port number; `None` when there is none or it is not a valid port.
    #[must_use]
    pub fn port(&self) -> Option<u16> {
        std::str::from_utf8(self.url.port())
            .ok()
            .and_then(|p| p.parse().ok())
    }

    /// The parsed URL.
    #[must_use]
    pub fn url(&self) -> &UrlRef {
        &self.url
    }

    /// The same URL with another protocol: `webcal://`, `webcal:` or `webcal`.
    ///
    /// # Errors
    /// When `protocol` ends with `:` but the URL has a host (an opaque protocol cannot keep
    /// it).
    pub fn with_protocol(&self, protocol: &str) -> Result<Self, UrlError> {
        let mut u = self.url.clone();
        let (scheme, full, opaque) = if let Some(s) = protocol.strip_suffix("://") {
            (s, true, false)
        } else if let Some(s) = protocol.strip_suffix(':') {
            (s, false, true)
        } else {
            (protocol, false, false)
        };
        scheme.clone_into(&mut u.scheme);
        if full && !u.opaque.is_empty() {
            u.opaque = format!("//{}", u.opaque);
        } else if opaque && u.opaque.is_empty() {
            return Err(UrlError::OpaqueProtocol {
                url: self.with_path.clone(),
                protocol: protocol.to_owned(),
            });
        }
        Self::from_url(u)
    }

    /// The same URL with another port.
    #[must_use]
    pub fn with_port(&self, port: u16) -> Self {
        let mut u = self.url.clone();
        let mut host = u.hostname().to_vec();
        host.extend_from_slice(format!(":{port}").as_bytes());
        u.host = host;
        Self::from_url(u).expect("the path of a base URL is UTF-8")
    }
}

impl fmt::Display for BaseUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.with_path)
    }
}

/// How relative URLs are written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LinkStyle {
    /// Relative URLs keep the base URL's path (`/docs/about/`).
    #[default]
    Relative,
    /// `canonifyURLs`: relative URLs are written without the base path; the publisher makes
    /// them absolute.
    Canonify,
}

/// Whether paths made from titles are lower-cased (`disablePathToLower`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PathCase {
    #[default]
    Lower,
    Preserve,
}

/// Whether paths made from titles lose their accents (`removePathAccents`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Accents {
    #[default]
    Keep,
    Remove,
}

/// The URL helpers of one language of a site (`absURL`, `relURL`, `urlize`, …).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SiteUrls {
    pub base_url: BaseUrl,
    /// The language's URL prefix (`""` or `"th"`).
    pub language_prefix: String,
    pub link_style: LinkStyle,
    pub path_case: PathCase,
    pub accents: Accents,
}

impl SiteUrls {
    /// A path from a title or file name: [`paths::sanitize`], then accents removed if
    /// configured.
    #[must_use]
    pub fn make_path(&self, s: &str) -> String {
        let s = paths::sanitize(s);
        match self.accents {
            Accents::Keep => s,
            Accents::Remove => text::remove_accents(&s),
        }
    }

    /// [`make_path`](Self::make_path), lower-cased unless the case is preserved.
    #[must_use]
    pub fn make_path_sanitized(&self, s: &str) -> String {
        let s = self.make_path(s);
        match self.path_case {
            PathCase::Lower => text::to_lower(&s),
            PathCase::Preserve => s,
        }
    }

    /// `urlize`: [`make_path_sanitized`](Self::make_path_sanitized), then escaped as a URL
    /// reference.
    #[must_use]
    pub fn urlize(&self, s: &str) -> String {
        let path = self.make_path_sanitized(s);
        // A sanitised path keeps no ':', control character or broken escape, so it always
        // parses; escaping it as a path is the same result.
        url_escape(&path).unwrap_or_else(|_| escape(path.as_bytes(), Component::Path).into_owned())
    }

    /// The base path to prepend: empty for relative URLs of a canonified site.
    #[must_use]
    pub fn base_path(&self, relative: bool) -> &str {
        if relative && self.link_style == LinkStyle::Canonify {
            ""
        } else {
            self.base_url.base_path_no_trailing_slash()
        }
    }

    fn base_url_root(&self, path: &str) -> &str {
        if path.starts_with('/') {
            self.base_url.without_path()
        } else {
            self.base_url.as_str()
        }
    }

    /// `target` joined onto the language prefix, unless `input` already starts with the
    /// prefix (`None` then, or when there is no prefix).
    fn with_language(&self, input: &str, target: &str, add_slash: bool) -> Option<String> {
        let prefix = self.language_prefix.as_str();
        if prefix.is_empty() {
            return None;
        }
        let in2 = input.strip_prefix('/').unwrap_or(input);
        let has_prefix = in2 == prefix
            || in2
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('/'));
        if has_prefix {
            return None;
        }
        let mut joined = paths::join(&[prefix, target]);
        if add_slash {
            joined.push('/');
        }
        Some(joined)
    }

    /// `absURL`: `input` made absolute against the base URL (inputs starting with `/` against
    /// its host). Absolute and protocol-relative URLs, and inputs that are not URL references,
    /// are returned unchanged.
    #[must_use]
    pub fn abs_url(&self, input: &str) -> String {
        self.abs(input, false)
    }

    /// `absLangURL`: [`abs_url`](Self::abs_url) with the language prefix added.
    #[must_use]
    pub fn abs_lang_url(&self, input: &str) -> String {
        self.abs(input, true)
    }

    fn abs(&self, input: &str, add_language: bool) -> String {
        match is_abs_url(input) {
            Err(_) => return input.to_owned(),
            Ok(true) => return input.to_owned(),
            Ok(false) if input.starts_with("//") => return input.to_owned(),
            Ok(false) => {}
        }
        let base = self.base_url_root(input);
        let add_slash = input.is_empty() || input.ends_with('/');
        let with_lang = add_language
            .then(|| self.with_language(input, input, add_slash))
            .flatten();
        let link = with_lang.as_deref().unwrap_or(input);
        make_permalink(base, link).map_or_else(|_| input.to_owned(), |u| u.to_string())
    }

    /// `relURL`: `input` relative to the server root, with the base path in front (unless
    /// canonified). Absolute URLs on another host and protocol-relative URLs are returned
    /// unchanged.
    #[must_use]
    pub fn rel_url(&self, input: &str) -> String {
        self.rel(input, false)
    }

    /// `relLangURL`: [`rel_url`](Self::rel_url) with the language prefix added.
    #[must_use]
    pub fn rel_lang_url(&self, input: &str) -> String {
        self.rel(input, true)
    }

    fn rel(&self, input: &str, add_language: bool) -> String {
        let Ok(is_abs) = is_abs_url(input) else {
            return input.to_owned();
        };
        let base = self.base_url_root(input);
        if (!input.starts_with(base) && is_abs) || input.starts_with("//") {
            return input.to_owned();
        }
        let mut u: Vec<u8> = input
            .strip_prefix(base)
            .unwrap_or(input)
            .as_bytes()
            .to_vec();
        if add_language {
            let rest = String::from_utf8_lossy(&u).into_owned();
            if let Some(joined) = self.with_language(input, &rest, rest.ends_with('/')) {
                u = joined.into_bytes();
            }
        }
        if self.link_style == LinkStyle::Relative {
            let rel = String::from_utf8_lossy(&u).into_owned();
            match add_context_root(base, &rel) {
                Ok(p) => u = p,
                Err(_) => return input.to_owned(),
            }
        }
        if input.is_empty() && !u.ends_with(b"/") && base.ends_with('/') {
            u.push(b'/');
        }
        if !u.starts_with(b"/") {
            u.insert(0, b'/');
        }
        String::from_utf8(u).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
    }

    /// Prepends the base path to a site-relative path (relative URL flavour).
    #[must_use]
    pub fn prepend_base_path(&self, rel: &str) -> String {
        Self::prepend(self.base_path(true), rel)
    }

    /// Prepends the base path to a site-relative path that will be made absolute.
    #[must_use]
    pub fn prepend_base_path_abs(&self, rel: &str) -> String {
        Self::prepend(self.base_path(false), rel)
    }

    fn prepend(base_path: &str, rel: &str) -> String {
        if base_path.is_empty() {
            return rel.to_owned();
        }
        let mut out = paths::join(&[base_path, rel]);
        if rel.ends_with('/') {
            out.push('/');
        }
        out
    }

    /// `link` appended to `base_url` (with one slash between them).
    #[must_use]
    pub fn permalink_for_base_url(link: &str, base_url: &str) -> String {
        let link = link.strip_prefix('/').unwrap_or(link);
        if base_url.ends_with('/') {
            format!("{base_url}{link}")
        } else {
            format!("{base_url}/{link}")
        }
    }
}
