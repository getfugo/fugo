//! The URL functions of the manifest extractor: `urllib.parse.urlsplit`, `urljoin`,
//! `urlunsplit`, `unquote` and `quote` of CPython 3.14 (see [`crate::py`] for why). Translated
//! from CPython 3.14.7's `Lib/urllib/parse.py` (PSF-2.0, THIRD_PARTY/cpython/LICENSE;
//! PROVENANCE.md).

use unicode_normalization::UnicodeNormalization;

const SCHEME_CHARS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789+-.";
const USES_RELATIVE: [&str; 20] = [
    "", "ftp", "http", "gopher", "nntp", "imap", "wais", "file", "https", "shttp", "mms",
    "prospero", "rtsp", "rtsps", "rtspu", "sftp", "svn", "svn+ssh", "ws", "wss",
];
const USES_NETLOC: [&str; 27] = [
    "",
    "ftp",
    "http",
    "gopher",
    "nntp",
    "telnet",
    "imap",
    "wais",
    "file",
    "mms",
    "https",
    "shttp",
    "snews",
    "prospero",
    "rtsp",
    "rtsps",
    "rtspu",
    "rsync",
    "svn",
    "svn+ssh",
    "sftp",
    "nfs",
    "git",
    "git+ssh",
    "ws",
    "wss",
    "itms-services",
];

/// The components of a URL; `None` is a component the URL does not have (`_urlsplit`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Parts {
    pub scheme: Option<String>,
    pub netloc: Option<String>,
    pub path: String,
    pub query: Option<String>,
    pub fragment: Option<String>,
}

/// `urlsplit(url)`: the components, absent ones empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Split {
    pub scheme: String,
    pub netloc: String,
    pub path: String,
    pub query: String,
    pub fragment: String,
}

/// `urlsplit(url)`.
///
/// # Errors
/// An invalid bracketed host or a netloc that NFKC normalization changes into one with
/// delimiters (Python's `ValueError`).
pub fn urlsplit(url: &str) -> Result<Split, String> {
    let p = split_parts(url)?;
    Ok(Split {
        scheme: p.scheme.unwrap_or_default(),
        netloc: p.netloc.unwrap_or_default(),
        path: p.path,
        query: p.query.unwrap_or_default(),
        fragment: p.fragment.unwrap_or_default(),
    })
}

/// `_urlsplit(url, None)`.
///
/// # Errors
/// As [`urlsplit`].
pub fn split_parts(url: &str) -> Result<Parts, String> {
    let url = url.trim_start_matches(|c: char| c <= ' ');
    let mut url: String = url
        .chars()
        .filter(|c| !matches!(c, '\t' | '\r' | '\n'))
        .collect();
    let mut parts = Parts::default();
    if let Some(i) = url.find(':')
        && i > 0
        && url.as_bytes()[0].is_ascii_alphabetic()
        && url[..i].chars().all(|c| SCHEME_CHARS.contains(c))
    {
        parts.scheme = Some(url[..i].to_ascii_lowercase());
        url = url[i + 1..].to_owned();
    }
    if url.starts_with("//") {
        let delim = url[2..].find(['/', '?', '#']).map_or(url.len(), |d| d + 2);
        let netloc = url[2..delim].to_owned();
        url = url[delim..].to_owned();
        if netloc.contains('[') != netloc.contains(']') {
            return Err("Invalid IPv6 URL".into());
        }
        if netloc.contains('[') {
            check_bracketed_netloc(&netloc)?;
        }
        parts.netloc = Some(netloc);
    }
    if let Some((rest, fragment)) = url.split_once('#') {
        parts.fragment = Some(fragment.to_owned());
        url = rest.to_owned();
    }
    if let Some((rest, query)) = url.split_once('?') {
        parts.query = Some(query.to_owned());
        url = rest.to_owned();
    }
    if let Some(netloc) = &parts.netloc {
        check_netloc(netloc)?;
    }
    parts.path = url;
    Ok(parts)
}

fn check_bracketed_netloc(netloc: &str) -> Result<(), String> {
    let host_port = netloc.rsplit_once('@').map_or(netloc, |(_, h)| h);
    let hostname = match host_port.split_once('[') {
        Some((before, bracketed)) => {
            if !before.is_empty() {
                return Err("Invalid IPv6 URL".into());
            }
            let (host, port) = bracketed.split_once(']').unwrap_or((bracketed, ""));
            if !port.is_empty() && !port.starts_with(':') {
                return Err("Invalid IPv6 URL".into());
            }
            host
        }
        None => host_port.split_once(':').map_or(host_port, |(h, _)| h),
    };
    if hostname.starts_with(['v', 'V']) {
        let rest = &hostname[1..];
        let hex = rest.chars().take_while(char::is_ascii_hexdigit).count();
        let after = &rest[hex..];
        if hex == 0 || !after.starts_with('.') || after.len() < 2 || after.contains('\n') {
            return Err("IPvFuture address is invalid".into());
        }
        return Ok(());
    }
    let address = hostname.split_once('%').map_or(hostname, |(a, _)| a);
    if hostname.parse::<std::net::Ipv4Addr>().is_ok() {
        return Err("An IPv4 address cannot be in brackets".into());
    }
    address
        .parse::<std::net::Ipv6Addr>()
        .map(|_| ())
        .map_err(|_| format!("'{hostname}' does not appear to be an IPv4 or IPv6 address"))
}

fn check_netloc(netloc: &str) -> Result<(), String> {
    if netloc.is_empty() || netloc.is_ascii() {
        return Ok(());
    }
    let n: String = netloc
        .chars()
        .filter(|c| !matches!(c, '@' | ':' | '#' | '?'))
        .collect();
    let normalized: String = n.nfkc().collect();
    if n != normalized && normalized.contains(['/', '?', '#', '@', ':']) {
        return Err(format!(
            "netloc '{netloc}' contains invalid characters under NFKC normalization"
        ));
    }
    Ok(())
}

/// `_urlunsplit`.
fn unsplit(
    scheme: Option<&str>,
    netloc: Option<&str>,
    path: &str,
    query: Option<&str>,
    fragment: Option<&str>,
) -> String {
    let mut url = path.to_owned();
    if let Some(netloc) = netloc {
        if !url.is_empty() && !url.starts_with('/') {
            url.insert(0, '/');
        }
        url = format!("//{netloc}{url}");
    } else if url.starts_with("//") {
        url = format!("//{url}");
    }
    if let Some(scheme) = scheme.filter(|s| !s.is_empty()) {
        url = format!("{scheme}:{url}");
    }
    if let Some(query) = query {
        url = format!("{url}?{query}");
    }
    if let Some(fragment) = fragment {
        url = format!("{url}#{fragment}");
    }
    url
}

/// `urlunsplit((scheme, netloc, path, query, fragment))`.
#[must_use]
pub fn urlunsplit(s: &Split) -> String {
    let netloc = if s.netloc.is_empty() {
        let scheme_with_netloc = !s.scheme.is_empty() && USES_NETLOC.contains(&s.scheme.as_str());
        (scheme_with_netloc && (s.path.is_empty() || s.path.starts_with('/'))).then_some("")
    } else {
        Some(s.netloc.as_str())
    };
    let some = |x: &str| (!x.is_empty()).then(|| x.to_owned());
    unsplit(
        some(&s.scheme).as_deref(),
        netloc,
        &s.path,
        some(&s.query).as_deref(),
        some(&s.fragment).as_deref(),
    )
}

/// `urljoin(base, url)`.
///
/// # Errors
/// As [`urlsplit`], for either URL.
pub fn urljoin(base: &str, url: &str) -> Result<String, String> {
    if base.is_empty() {
        return Ok(url.to_owned());
    }
    if url.is_empty() {
        return Ok(base.to_owned());
    }
    let b = split_parts(base)?;
    let u = split_parts(url)?;
    let scheme = u.scheme.clone().or_else(|| b.scheme.clone());
    if scheme != b.scheme
        || scheme
            .as_deref()
            .is_some_and(|s| !s.is_empty() && !USES_RELATIVE.contains(&s))
    {
        return Ok(url.to_owned());
    }
    let mut netloc = u.netloc.clone();
    if scheme
        .as_deref()
        .is_none_or(|s| s.is_empty() || USES_NETLOC.contains(&s))
    {
        if netloc.as_deref().is_some_and(|n| !n.is_empty()) {
            return Ok(unsplit(
                scheme.as_deref(),
                netloc.as_deref(),
                &u.path,
                u.query.as_deref(),
                u.fragment.as_deref(),
            ));
        }
        netloc = b.netloc.clone();
    }
    let (mut query, mut fragment) = (u.query.clone(), u.fragment.clone());
    if u.path.is_empty() {
        if query.is_none() {
            query = b.query.clone();
            if fragment.is_none() {
                fragment = b.fragment.clone();
            }
        }
        return Ok(unsplit(
            scheme.as_deref(),
            netloc.as_deref(),
            &b.path,
            query.as_deref(),
            fragment.as_deref(),
        ));
    }
    let mut base_parts: Vec<&str> = b.path.split('/').collect();
    if base_parts.last() != Some(&"") {
        base_parts.pop();
    }
    let segments: Vec<&str> = if u.path.starts_with('/') {
        u.path.split('/').collect()
    } else {
        let mut s = base_parts;
        s.extend(u.path.split('/'));
        if s.len() >= 2 {
            let last = s[s.len() - 1];
            let mut kept = vec![s[0]];
            kept.extend(s[1..s.len() - 1].iter().copied().filter(|x| !x.is_empty()));
            kept.push(last);
            s = kept;
        }
        s
    };
    let mut resolved: Vec<&str> = Vec::new();
    for &seg in &segments {
        match seg {
            ".." => {
                resolved.pop();
            }
            "." => {}
            _ => resolved.push(seg),
        }
    }
    if matches!(segments.last(), Some(&("." | ".."))) {
        resolved.push("");
    }
    let path = resolved.join("/");
    let path = if path.is_empty() {
        "/".to_owned()
    } else {
        path
    };
    Ok(unsplit(
        scheme.as_deref(),
        netloc.as_deref(),
        &path,
        query.as_deref(),
        fragment.as_deref(),
    ))
}

/// `unquote(s)`: `%XX` escapes decoded, each run of ASCII characters as UTF-8 with invalid
/// sequences replaced (U+FFFD); other characters kept.
#[must_use]
pub fn unquote(s: &str) -> String {
    if !s.contains('%') {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while !rest.is_empty() {
        let ascii = rest.find(|c: char| !c.is_ascii()).unwrap_or(rest.len());
        if ascii == 0 {
            let other = rest.find(|c: char| c.is_ascii()).unwrap_or(rest.len());
            out.push_str(&rest[..other]);
            rest = &rest[other..];
            continue;
        }
        out.push_str(&String::from_utf8_lossy(&unquote_bytes(
            &rest.as_bytes()[..ascii],
        )));
        rest = &rest[ascii..];
    }
    out
}

fn unquote_bytes(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && let (Some(h), Some(l)) = (b.get(i + 1).and_then(hex), b.get(i + 2).and_then(hex))
        {
            out.push(h << 4 | l);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

fn hex(c: &u8) -> Option<u8> {
    (*c as char).to_digit(16).and_then(|d| u8::try_from(d).ok())
}

/// `quote(s, safe=…)`: every UTF-8 byte that is neither unreserved (letters, digits, `_.-~`)
/// nor an ASCII character of `safe` as `%XX`.
#[must_use]
pub fn quote(s: &str, safe: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        let c = b as char;
        if b.is_ascii_alphanumeric()
            || matches!(c, '_' | '.' | '-' | '~')
            || (b.is_ascii() && safe.contains(c))
        {
            out.push(c);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
