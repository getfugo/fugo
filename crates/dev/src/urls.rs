//! The URLs of a manifest: an internal URL (below one of the site's base URLs, root-relative, or
//! relative to the page) becomes a site path (`/a/b/`, with its query and fragment), an external
//! one stays whole; both percent-decoded and NFC-normalised, so that `/th/%E0%B8%82/` and
//! `/th/ข/` are the same link. Paths get the L1 normalisation ([`crate::manifest::norm_path`]).

use percent_encoding::percent_decode_str;
use unicode_normalization::UnicodeNormalization;
use url::Url;

use crate::manifest::norm_path;
use crate::{Fail, fail};

/// Schemes whose URLs are never site paths.
const NOT_LINKS: [&str; 4] = ["mailto:", "tel:", "javascript:", "data:"];

/// A base URL: its host (with a port when not the default) and its path, ending in `/`.
#[derive(Clone, Debug)]
struct Base {
    host: String,
    path: String,
}

/// The base URLs of a site.
#[derive(Clone, Debug)]
pub struct SiteUrls {
    bases: Vec<Base>,
}

/// Percent-decoded (invalid UTF-8 replaced) and NFC-normalised.
#[must_use]
pub fn clean(s: &str) -> String {
    percent_decode_str(s).decode_utf8_lossy().nfc().collect()
}

fn host_of(url: &Url) -> String {
    let host = url.host_str().unwrap_or("").to_lowercase();
    match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host,
    }
}

/// The site's address against which page-relative links are resolved.
fn site_root() -> Url {
    Url::parse("http://site.invalid/").expect("a valid URL")
}

impl SiteUrls {
    /// # Errors
    /// A base URL that does not parse.
    pub fn new(base_urls: &[String]) -> Result<SiteUrls, Fail> {
        let bases = base_urls
            .iter()
            .map(|b| {
                let url = Url::parse(b).map_err(|e| fail!("base URL {b}: {e}"))?;
                let path = url.path().to_owned();
                let path = if path.ends_with('/') {
                    path
                } else {
                    path + "/"
                };
                Ok(Base {
                    host: host_of(&url),
                    path,
                })
            })
            .collect::<Result<_, Fail>>()?;
        Ok(SiteUrls { bases })
    }

    /// The site path of an internal URL on the page served at `page`, or `None` for an external
    /// one (or a mail, phone, script or data URL).
    #[must_use]
    pub fn internal(&self, raw: &str, page: &str) -> Option<String> {
        let raw = raw.trim();
        if raw.is_empty() || NOT_LINKS.iter().any(|s| raw.starts_with(s)) {
            return None;
        }
        let absolute = if raw.starts_with("//") {
            Url::parse(&format!("http:{raw}")).ok()
        } else {
            Url::parse(raw).ok()
        };
        let url = match absolute {
            Some(url) => {
                let host = host_of(&url);
                let with_slash = format!("{}/", url.path());
                let base = self
                    .bases
                    .iter()
                    .find(|b| b.host == host && with_slash.starts_with(&b.path))?;
                let rest = url.path().get(base.path.len()..).unwrap_or("");
                return Some(site_path(&format!("/{rest}"), url.query(), url.fragment()));
            }
            None => site_root().join(page).and_then(|p| p.join(raw)).ok()?,
        };
        let mut path = url.path().to_owned();
        if raw.starts_with('/') {
            // A root-relative link may name the base URL's path.
            let with_slash = format!("{path}/");
            if let Some(base) = self
                .bases
                .iter()
                .find(|b| b.path != "/" && with_slash.starts_with(&b.path))
            {
                path = format!("/{}", path.get(base.path.len()..).unwrap_or(""));
            }
        }
        Some(site_path(&path, url.query(), url.fragment()))
    }

    /// [`SiteUrls::internal`], else the whole URL cleaned.
    #[must_use]
    pub fn any(&self, raw: &str, page: &str) -> String {
        self.internal(raw, page)
            .unwrap_or_else(|| clean(raw.trim()))
    }
}

fn site_path(path: &str, query: Option<&str>, fragment: Option<&str>) -> String {
    let mut out = norm_path(&clean(path));
    if let Some(q) = query.filter(|q| !q.is_empty()) {
        out.push('?');
        out.push_str(&clean(q));
    }
    if let Some(f) = fragment.filter(|f| !f.is_empty()) {
        out.push('#');
        out.push_str(&clean(f));
    }
    out
}
