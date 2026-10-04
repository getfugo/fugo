//! The site's base URL and the URLs of its pages (`BaseUrl`, `SiteUrls`).

use super::*;

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

    pub(super) fn from_url(mut url: UrlRef) -> Result<Self, UrlError> {
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

    pub(super) fn base_url_root(&self, path: &str) -> &str {
        if path.starts_with('/') {
            self.base_url.without_path()
        } else {
            self.base_url.as_str()
        }
    }

    /// `target` joined onto the language prefix, unless `input` already starts with the
    /// prefix (`None` then, or when there is no prefix).
    pub(super) fn with_language(
        &self,
        input: &str,
        target: &str,
        add_slash: bool,
    ) -> Option<String> {
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

    pub(super) fn abs(&self, input: &str, add_language: bool) -> String {
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

    pub(super) fn rel(&self, input: &str, add_language: bool) -> String {
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

    pub(super) fn prepend(base_path: &str, rel: &str) -> String {
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
