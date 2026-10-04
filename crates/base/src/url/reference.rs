//! A URL reference as Go's net/url parses and prints it (`url.URL`): scheme, user info, host, path,
//! query and fragment.

use super::*;

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
    pub(super) scheme: String,
    pub(super) opaque: String,
    pub(super) user: Option<UserInfo>,
    pub(super) host: Vec<u8>,
    pub(super) path: Vec<u8>,
    /// The escaping of the path as parsed, when it differs from the default escaping.
    pub(super) raw_path: String,
    /// `scheme:/path`: an empty authority that is not written back.
    pub(super) omit_host: bool,
    /// A lone `?` with an empty query.
    pub(super) force_query: bool,
    pub(super) raw_query: String,
    pub(super) fragment: Vec<u8>,
    pub(super) raw_fragment: String,
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

    pub(super) fn parse_without_fragment(input: &str, raw: &str) -> Result<Self, UrlError> {
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
pub(super) fn split_scheme(raw: &str) -> Option<(&str, &str)> {
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

pub(super) type Authority = (Option<UserInfo>, Vec<u8>);

pub(super) fn parse_authority(authority: &str) -> Result<Authority, UrlError> {
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
pub(super) fn valid_optional_port(s: &str) -> bool {
    s.is_empty()
        || s.strip_prefix(':')
            .is_some_and(|d| d.bytes().all(|c| c.is_ascii_digit()))
}

pub(super) fn parse_host(host: &str) -> Result<Vec<u8>, UrlError> {
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

pub(super) fn split_host_port(host_port: &[u8]) -> (&[u8], &[u8]) {
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
