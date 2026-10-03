//! `resources.GetRemote`: remote resources through the `[caches.getresource]` file cache.
//!
//! - A request is typed ([`RemoteOptions`]): method, headers, body, `key` and the response
//!   headers to keep in `.Data.Headers`. Equivalent option maps (key order, key case) are the
//!   same request; one build fetches a request once.
//! - Cache entries are raw HTTP responses (status line, headers, blank line, body), the format the
//!   Go build writes too. An entry is named by [`cache_key`]: the hash of the `key` option when
//!   there is one (so changing `key` refetches), else of the request.
//! - **Importer.** When an entry is missing, the store looks for the entry the Go build would
//!   have written for the same call — named by [`go_keys`], Go's hash of the URL and the option
//!   map — in the cache directory itself and in [`RemoteConfig::import_dirs`], and copies it
//!   under this crate's name. A cache that the Go build filled (the directory of the Go
//!   program's cache-directory environment variable) is thereby replayed without the network.
//! - Only then, and only when [`RemoteConfig::network`] allows it, the URL is fetched with
//!   `ureq`; the response is cached unless it is a redirect or `maxAge` is zero.
//! - The resource is named like Go's (`<file stem>_<Go's user key><suffix>`, e.g.
//!   `/data_13295982728060263486.json`), so its URLs equal the Go build's. A 404 gives no
//!   resource; any other status outside 2xx is [`RemoteError::Status`].

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use ssg_base::paths::{self, UrlPath};
use ssg_base::url::{Component, UrlRef, unescape};
use ssg_base::{LangIdx, Map, Params, ResourceId, Value, text};
use ssg_config::global::{MaxAge, SecurityPolicy, Whitelist};
use ssg_config::{Config, MediaType, MediaTypes};
use xxhash_rust::xxh3::xxh3_128;

use crate::gohash;
use crate::store::{Body, NewResource, Origin, PublishPolicy, ResourceStore, lock};

/// `[caches.getresource]` and `[security.http]` as the store uses them.
#[derive(Clone, Debug)]
pub struct RemoteConfig {
    /// The cache directory; `None` disables the file cache.
    pub cache_dir: Option<PathBuf>,
    pub max_age: MaxAge,
    /// More directories holding caches the Go build wrote (entries named by [`go_keys`]).
    pub import_dirs: Vec<PathBuf>,
    /// Whether a missing entry may be fetched from the network.
    pub network: bool,
    pub http_urls: Whitelist,
    pub http_methods: Whitelist,
    /// Content types whose `Content-Type` header is trusted as the media type.
    pub http_media_types: Whitelist,
    /// The timeout of one fetch.
    pub timeout: Duration,
}

impl Default for RemoteConfig {
    fn default() -> Self {
        let sec = SecurityPolicy::default();
        Self {
            cache_dir: None,
            max_age: MaxAge::Forever,
            import_dirs: Vec::new(),
            network: true,
            http_urls: sec.http_urls,
            http_methods: sec.http_methods,
            http_media_types: sec.http_media_types,
            timeout: Duration::from_secs(30),
        }
    }
}

impl RemoteConfig {
    /// `[caches.getresource]`, `[security.http]` and `timeout` of a project; the network is
    /// allowed.
    #[must_use]
    pub fn from_config(cfg: &Config) -> Self {
        let cache = cfg.caches.get("getresource");
        Self {
            cache_dir: cache.map(|c| c.path.clone()),
            max_age: cache.map_or(MaxAge::Forever, |c| c.max_age),
            import_dirs: Vec::new(),
            network: true,
            http_urls: cfg.security.http_urls.clone(),
            http_methods: cfg.security.http_methods.clone(),
            http_media_types: cfg.security.http_media_types.clone(),
            timeout: cfg.timeout,
        }
    }
}

/// Why a remote resource could not be had.
#[derive(Clone, Debug, thiserror::Error)]
pub enum RemoteError {
    #[error("options of resources.GetRemote: {0}")]
    Options(String),
    #[error("{url:?} is not a URL: {reason}")]
    InvalidUrl { url: String, reason: String },
    #[error("{url:?}: only http and https URLs can be fetched")]
    UnsupportedScheme { url: String },
    #[error("{value:?} is not allowed by security.http.{policy}")]
    NotAllowed { policy: &'static str, value: String },
    #[error("{url}: not in the getresource cache, and the network is disabled")]
    Offline { url: String },
    #[error("{url}: {reason}")]
    Network { url: String, reason: String },
    /// A response status outside 2xx (except 404); `data` is `.Data` of the error, with the
    /// body under `Body` (not for HEAD).
    #[error("{url}: the server answered {status}")]
    Status {
        url: String,
        code: u16,
        status: String,
        data: Map,
    },
    #[error("{url}: cannot tell the media type of the response")]
    MediaType { url: String },
    #[error("getresource cache {path}: {reason}")]
    Cache { path: PathBuf, reason: String },
}

/// The options map of `resources.GetRemote`, typed. Option names ignore case.
#[derive(Clone, Debug, PartialEq)]
pub struct RemoteOptions {
    /// Upper case; `GET` by default.
    pub method: String,
    /// Request headers, sorted by name; a list value gives several headers.
    pub headers: Vec<(String, Vec<String>)>,
    pub body: Vec<u8>,
    /// The `key` option: the cache entry's identity instead of the request's.
    pub key: Option<String>,
    /// Response headers to keep in `.Data.Headers` (matched ignoring case).
    pub response_headers: Vec<String>,
    /// The map as given (for the Go build's cache names).
    raw: Option<Map>,
}

impl Default for RemoteOptions {
    fn default() -> Self {
        Self {
            method: "GET".to_owned(),
            headers: Vec::new(),
            body: Vec::new(),
            key: None,
            response_headers: Vec::new(),
            raw: None,
        }
    }
}

fn value_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.to_string()),
        Value::Int(i) => Some(i.to_string()),
        Value::Float(f) => Some(f.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Date(d) => Some(d.to_string()),
        Value::Null | Value::Array(_) | Value::Map(_) => None,
    }
}

fn string_list(v: &Value, what: &str) -> Result<Vec<String>, RemoteError> {
    let bad = || RemoteError::Options(format!("{what} must be a string or a list of strings"));
    match v {
        Value::Array(items) => items
            .iter()
            .map(|i| value_string(i).ok_or_else(bad))
            .collect(),
        Value::Null => Ok(Vec::new()),
        other => Ok(vec![value_string(other).ok_or_else(bad)?]),
    }
}

impl RemoteOptions {
    /// Decodes the options map (`None`: no options).
    ///
    /// # Errors
    /// A `method` that is not a string, `headers` that is not a map, a `body` that is neither
    /// a string nor a list of bytes, or header values that are not strings.
    pub fn from_map(m: Option<&Map>) -> Result<Self, RemoteError> {
        let mut o = Self::default();
        let Some(m) = m else {
            return Ok(o);
        };
        for (k, v) in m.iter() {
            match text::to_lower(k).as_str() {
                "method" => {
                    let Value::String(s) = v else {
                        return Err(RemoteError::Options("method must be a string".into()));
                    };
                    o.method = s.to_ascii_uppercase();
                }
                "headers" => {
                    let Value::Map(h) = v else {
                        return Err(RemoteError::Options("headers must be a map".into()));
                    };
                    for (name, value) in h.iter() {
                        o.headers
                            .push((name.to_owned(), string_list(value, "a header value")?));
                    }
                }
                "body" => {
                    o.body = match v {
                        Value::String(s) => s.as_bytes().to_vec(),
                        Value::Array(items) => items
                            .iter()
                            .map(|i| i.as_i64().and_then(|b| u8::try_from(b).ok()))
                            .collect::<Option<Vec<u8>>>()
                            .ok_or_else(|| {
                                RemoteError::Options("body must be a string or bytes".into())
                            })?,
                        Value::Null => Vec::new(),
                        _ => {
                            return Err(RemoteError::Options(
                                "body must be a string or bytes".into(),
                            ));
                        }
                    };
                }
                "key" => o.key = value_string(v),
                "responseheaders" => {
                    o.response_headers = string_list(v, "responseHeaders")?;
                }
                _ => {}
            }
        }
        o.headers.sort();
        o.raw = Some(m.clone());
        Ok(o)
    }

    /// The request's identity: method, URL, headers, body and the kept response headers (not
    /// `key`).
    fn identity(&self, url: &str) -> Vec<u8> {
        let mut s = format!("{}\n{url}\n", self.method).into_bytes();
        for (k, vs) in &self.headers {
            for v in vs {
                s.extend_from_slice(format!("{}: {v}\n", text::to_lower(k)).as_bytes());
            }
        }
        for h in &self.response_headers {
            s.extend_from_slice(format!("<{}\n", text::to_lower(h)).as_bytes());
        }
        s.push(b'\n');
        s.extend_from_slice(&self.body);
        s
    }

    fn is_head(&self) -> bool {
        self.method == "HEAD"
    }
}

/// The name of a request's cache entry (32 hex digits).
#[must_use]
pub fn cache_key(url: &str, o: &RemoteOptions) -> String {
    let h = match &o.key {
        Some(k) => xxh3_128(format!("key\n{k}").as_bytes()),
        None => xxh3_128(&o.identity(url)),
    };
    format!("{h:032x}")
}

// ── The Go build's cache names ───────────────────────────────────────────────────────────────
//
// The Go build names a getresource entry by the decimal xxHash64 structure hash (`crate::gohash`)
// of `[url, options]`, or of the `key` option.

/// Go's `(user key, options key)` of `GetRemote url options`: the options key hashes the URL
/// and the option map without `key` (its name ignores case); the user key hashes the `key`
/// option, or is the options key.
#[must_use]
pub fn go_keys(url: &str, options: Option<&Map>) -> (String, String) {
    let mut options = options.cloned();
    let key_value = options.as_mut().and_then(|m| {
        let name = m.keys().find(|k| k.eq_ignore_ascii_case("key"))?.to_owned();
        m.remove(&name)
    });
    let options_key = gohash::list([gohash::string(url), gohash::map(options.as_ref())]);
    let user_key = key_value.as_ref().map_or(options_key, gohash::value);
    (user_key.to_string(), options_key.to_string())
}

// ── HTTP responses as cache entries ──────────────────────────────────────────────────────────

/// A response: what a cache entry holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Response {
    /// `200 OK`.
    pub(crate) status: String,
    pub(crate) code: u16,
    /// Canonical names (`Content-Type`), in the order received.
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Vec<u8>,
}

/// `content-type` → `Content-Type`.
fn canonical_header(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut upper = true;
    for c in name.chars() {
        out.push(if upper {
            c.to_ascii_uppercase()
        } else {
            c.to_ascii_lowercase()
        });
        upper = c == '-';
    }
    out
}

impl Response {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// Parses `HTTP/x.y CODE TEXT`, header lines, an empty line and the body.
    pub(crate) fn parse(bytes: &[u8]) -> Option<Self> {
        let split = bytes
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .map(|i| (i, i + 4))
            .or_else(|| {
                bytes
                    .windows(2)
                    .position(|w| w == b"\n\n")
                    .map(|i| (i, i + 2))
            })?;
        let head = std::str::from_utf8(&bytes[..split.0]).ok()?;
        let mut lines = head.lines();
        let status_line = lines.next()?;
        let (version, status) = status_line.split_once(' ')?;
        if !version.starts_with("HTTP/") {
            return None;
        }
        let status = status.trim().to_owned();
        let code = status.split(' ').next()?.parse().ok()?;
        let headers = lines
            .filter_map(|l| l.split_once(':'))
            .map(|(k, v)| (canonical_header(k.trim()), v.trim().to_owned()))
            .collect();
        Some(Self {
            status,
            code,
            headers,
            body: bytes[split.1..].to_vec(),
        })
    }

    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        let mut out = format!("HTTP/1.1 {}\r\n", self.status).into_bytes();
        for (k, v) in &self.headers {
            out.extend_from_slice(format!("{k}: {v}\r\n").as_bytes());
        }
        out.extend_from_slice(b"\r\n");
        out.extend_from_slice(&self.body);
        out
    }

    /// `.Data`: `ContentLength` (−1 when unknown), `ContentType`, `Headers` (the wanted ones),
    /// `Status`, `StatusCode`, `TransferEncoding`, and `Body` when asked.
    fn data(&self, wanted: &[String], with_body: bool) -> Map {
        let mut headers = Map::new();
        for w in wanted {
            let values: Vec<Value> = self
                .headers
                .iter()
                .filter(|(k, _)| k.eq_ignore_ascii_case(w))
                .map(|(_, v)| Value::from(v.as_str()))
                .collect();
            if let Some((k, _)) = self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(w)) {
                headers.insert(k.as_str(), Value::array(values));
            }
        }
        let mut m = Map::new();
        let length = self
            .header("Content-Length")
            .and_then(|l| l.parse::<i64>().ok())
            .unwrap_or(-1);
        m.insert("ContentLength", Value::Int(length));
        m.insert(
            "ContentType",
            Value::from(self.header("Content-Type").unwrap_or_default()),
        );
        m.insert("Headers", Value::map(headers));
        m.insert("Status", Value::from(self.status.as_str()));
        m.insert("StatusCode", Value::Int(i64::from(self.code)));
        m.insert("TransferEncoding", Value::Null);
        if with_body {
            m.insert(
                "Body",
                Value::from(String::from_utf8_lossy(&self.body).as_ref()),
            );
        }
        m
    }
}

/// The remote state of a store: one result per request.
type Slot = Arc<Mutex<Option<Result<Option<ResourceId>, RemoteError>>>>;

#[derive(Default)]
pub(crate) struct RemoteState {
    memo: Mutex<HashMap<(LangIdx, Vec<u8>), Slot>>,
}

fn is_fresh(path: &Path, max_age: MaxAge) -> bool {
    match max_age {
        MaxAge::Forever => path.is_file(),
        MaxAge::For(age) if age.is_zero() => false,
        MaxAge::For(age) => fs::metadata(path)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|elapsed| elapsed <= age),
    }
}

fn cache_error(path: &Path, e: &std::io::Error) -> RemoteError {
    RemoteError::Cache {
        path: path.to_owned(),
        reason: e.to_string(),
    }
}

impl ResourceStore {
    /// `resources.GetRemote url options` for a template of `lang`: `Ok(None)` for a 404.
    ///
    /// # Errors
    /// See [`RemoteError`].
    pub fn get_remote(
        &self,
        lang: LangIdx,
        url: &str,
        options: &RemoteOptions,
    ) -> Result<Option<ResourceId>, RemoteError> {
        let parsed = UrlRef::parse(url).map_err(|e| RemoteError::InvalidUrl {
            url: url.to_owned(),
            reason: e.to_string(),
        })?;
        if !matches!(
            parsed.scheme().to_ascii_lowercase().as_str(),
            "http" | "https"
        ) {
            return Err(RemoteError::UnsupportedScheme {
                url: url.to_owned(),
            });
        }
        let rc = &self.cfg.remote;
        if !rc.http_urls.accepts(url) {
            return Err(RemoteError::NotAllowed {
                policy: "urls",
                value: url.to_owned(),
            });
        }
        if !rc.http_methods.accepts(&options.method) {
            return Err(RemoteError::NotAllowed {
                policy: "methods",
                value: options.method.clone(),
            });
        }
        let lang = self.global_lang(lang);
        let slot: Slot = Arc::clone(
            lock(&self.remote.memo)
                .entry((lang, options.identity(url)))
                .or_default(),
        );
        let mut slot = lock(&slot);
        if let Some(r) = &*slot {
            return r.clone();
        }
        let result = self.fetch_remote(lang, url, &parsed, options);
        *slot = Some(result.clone());
        result
    }

    fn fetch_remote(
        &self,
        lang: LangIdx,
        url: &str,
        parsed: &UrlRef,
        o: &RemoteOptions,
    ) -> Result<Option<ResourceId>, RemoteError> {
        let (go_user_key, _) = go_keys(url, o.raw.as_ref());
        let res = self.cached_response(url, o, &go_user_key)?;
        if res.code == 404 {
            return Ok(None);
        }
        if !(200..300).contains(&res.code) {
            return Err(RemoteError::Status {
                url: url.to_owned(),
                code: res.code,
                status: res.status.clone(),
                data: res.data(&o.response_headers, !o.is_head()),
            });
        }
        let body = if o.is_head() {
            Vec::new()
        } else {
            res.body.clone()
        };

        let url_path = unescape(&String::from_utf8_lossy(parsed.path()), Component::Path)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        let filename = res
            .header("Content-Disposition")
            .and_then(disposition_filename)
            .unwrap_or_else(|| paths::base(&url_path).to_owned());
        let content_type = res.header("Content-Type").unwrap_or_default();
        let media_type = self
            .remote_media_type(o.is_head(), content_type, &filename, &body)
            .ok_or_else(|| RemoteError::MediaType {
                url: url.to_owned(),
            })?;
        let stem = paths::trim_ext(&filename);
        let suffix = if media_type.suffixes.is_empty() {
            String::new()
        } else {
            media_type.full_suffix()
        };
        let link = format!("/{stem}_{go_user_key}{suffix}");
        let data = res.data(&o.response_headers, false);
        Ok(Some(self.push(NewResource {
            origin: Origin::Remote {
                url: url.to_owned(),
            },
            media_type,
            name: link.clone(),
            name_normalized: Some(link.clone()),
            title: link.clone(),
            params: Params::default(),
            data,
            lang,
            target: self.global_target(lang, &link),
            link: UrlPath::new(&link),
            body: Body::Bytes(body.into()),
            policy: PublishPolicy::OnReference,
            kind: None,
        })))
    }

    /// The response from the cache, from an imported entry of the Go build, or from the network.
    fn cached_response(
        &self,
        url: &str,
        o: &RemoteOptions,
        go_user_key: &str,
    ) -> Result<Response, RemoteError> {
        let rc = &self.cfg.remote;
        let ours = rc.cache_dir.as_ref().map(|d| d.join(cache_key(url, o)));
        if let Some(p) = &ours
            && is_fresh(p, rc.max_age)
        {
            let bytes = fs::read(p).map_err(|e| cache_error(p, &e))?;
            return Response::parse(&bytes).ok_or_else(|| RemoteError::Cache {
                path: p.clone(),
                reason: "not an HTTP response".into(),
            });
        }
        let import_from = rc.cache_dir.iter().chain(&rc.import_dirs);
        for dir in import_from {
            let go_entry = dir.join(go_user_key);
            if !is_fresh(&go_entry, rc.max_age) {
                continue;
            }
            let bytes = fs::read(&go_entry).map_err(|e| cache_error(&go_entry, &e))?;
            if let Some(res) = Response::parse(&bytes) {
                if let Some(p) = &ours {
                    write_entry(p, &bytes)?;
                }
                return Ok(res);
            }
        }
        if !rc.network {
            return Err(RemoteError::Offline {
                url: url.to_owned(),
            });
        }
        let res = fetch(url, o, rc.timeout)?;
        let redirect = matches!(res.code, 301 | 302 | 303 | 307 | 308);
        let keep = !matches!(rc.max_age, MaxAge::For(age) if age.is_zero());
        if let Some(p) = &ours
            && !redirect
            && keep
        {
            write_entry(p, &res.to_bytes())?;
        }
        Ok(res)
    }

    /// The media type of a response: a trusted `Content-Type` (HEAD requests, or types
    /// `security.http.mediaTypes` allows), else what the content looks like, narrowed by the
    /// extensions of the `Content-Type` and of the file name.
    fn remote_media_type(
        &self,
        is_head: bool,
        content_type: &str,
        filename: &str,
        body: &[u8],
    ) -> Option<MediaType> {
        let types = &self.cfg.media_types;
        let essence = MediaType::parse(content_type).ok();
        if (is_head || self.cfg.remote.http_media_types.accepts(content_type))
            && let Some(e) = &essence
        {
            return Some(
                types
                    .by_type(&e.type_string())
                    .map_or_else(|| e.clone(), |id| types.get(id).clone()),
            );
        }
        let mut hints: Vec<String> = if content_type.starts_with("text/plain") {
            vec!["txt".to_owned()]
        } else {
            essence
                .as_ref()
                .and_then(|e| mime_guess::get_mime_extensions_str(&e.type_string()))
                .map(|exts| {
                    let mut v: Vec<String> = exts.iter().map(|&x| x.to_owned()).collect();
                    v.sort();
                    v
                })
                .unwrap_or_default()
        };
        if hints.first().is_none_or(|h| h == "txt") {
            let ext = paths::ext_no_delimiter(filename);
            if !ext.is_empty() {
                hints = vec![ext.to_ascii_lowercase()];
            }
        }
        from_content(types, &hints, body)
    }
}

fn write_entry(path: &Path, bytes: &[u8]) -> Result<(), RemoteError> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| cache_error(dir, &e))?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).map_err(|e| cache_error(&tmp, &e))?;
    fs::rename(&tmp, path).map_err(|e| cache_error(path, &e))
}

/// The media type of `content` (sniffed), narrowed by extension hints: the hinted type when
/// both are text formats, the sniffed type when the hints agree or name nothing known, else
/// nothing (an image served as `.js`).
fn from_content(types: &MediaTypes, hints: &[String], content: &[u8]) -> Option<MediaType> {
    let sniffed = sniff(content);
    if sniffed == "application/octet-stream" {
        return None;
    }
    let by_type = |t: &str| types.by_type(t).map(|id| types.get(id).clone());
    let m = by_type(sniffed).or_else(|| {
        (sniffed == "text/xml")
            .then(|| by_type("application/xml"))
            .flatten()
    })?;
    if hints.is_empty() {
        return None;
    }
    let hinted = hints
        .iter()
        .find_map(|h| types.by_suffix(h).map(|id| types.get(id).clone()));
    match hinted {
        None => Some(m),
        Some(mm) if mm == m => Some(m),
        Some(mm) if m.is_text() && mm.is_text() => Some(mm),
        Some(_) => None,
    }
}

/// The essence of the content type the bytes look like: Go's `http.DetectContentType` (the
/// WHATWG sniffing rules), its signature table in order, on the first 512 bytes.
fn sniff(b: &[u8]) -> &'static str {
    /// `pat` where the bytes masked with `mask` equal it (`None`: every bit counts).
    enum Sig {
        Html(&'static [u8]),
        Exact(&'static [u8], &'static str),
        Masked(&'static [u8], Option<&'static [u8]>, bool, &'static str),
        Mp4,
    }
    use Sig::{Exact, Html, Masked, Mp4};
    const SIGS: &[Sig] = &[
        Html(b"<!DOCTYPE HTML"),
        Html(b"<HTML"),
        Html(b"<HEAD"),
        Html(b"<SCRIPT"),
        Html(b"<IFRAME"),
        Html(b"<H1"),
        Html(b"<DIV"),
        Html(b"<FONT"),
        Html(b"<TABLE"),
        Html(b"<A"),
        Html(b"<STYLE"),
        Html(b"<TITLE"),
        Html(b"<B"),
        Html(b"<BODY"),
        Html(b"<BR"),
        Html(b"<P"),
        Html(b"<!--"),
        Masked(b"<?xml", None, true, "text/xml"),
        Exact(b"%PDF-", "application/pdf"),
        Exact(b"%!PS-Adobe-", "application/postscript"),
        Masked(b"\xfe\xff\x00\x00", Some(b"\xff\xff\x00\x00"), false, "text/plain"),
        Masked(b"\xff\xfe\x00\x00", Some(b"\xff\xff\x00\x00"), false, "text/plain"),
        Masked(b"\xef\xbb\xbf\x00", Some(b"\xff\xff\xff\x00"), false, "text/plain"),
        Exact(b"\x00\x00\x01\x00", "image/x-icon"),
        Exact(b"\x00\x00\x02\x00", "image/x-icon"),
        Exact(b"BM", "image/bmp"),
        Exact(b"GIF87a", "image/gif"),
        Exact(b"GIF89a", "image/gif"),
        Masked(
            b"RIFF\x00\x00\x00\x00WEBPVP",
            Some(b"\xff\xff\xff\xff\x00\x00\x00\x00\xff\xff\xff\xff\xff\xff"),
            false,
            "image/webp",
        ),
        Exact(b"\x89PNG\r\n\x1a\n", "image/png"),
        Exact(b"\xff\xd8\xff", "image/jpeg"),
        Masked(
            b"FORM\x00\x00\x00\x00AIFF",
            Some(b"\xff\xff\xff\xff\x00\x00\x00\x00\xff\xff\xff\xff"),
            false,
            "audio/aiff",
        ),
        Masked(b"ID3", None, false, "audio/mpeg"),
        Masked(b"OggS\x00", None, false, "application/ogg"),
        Masked(b"MThd\x00\x00\x00\x06", None, false, "audio/midi"),
        Masked(
            b"RIFF\x00\x00\x00\x00AVI ",
            Some(b"\xff\xff\xff\xff\x00\x00\x00\x00\xff\xff\xff\xff"),
            false,
            "video/avi",
        ),
        Masked(
            b"RIFF\x00\x00\x00\x00WAVE",
            Some(b"\xff\xff\xff\xff\x00\x00\x00\x00\xff\xff\xff\xff"),
            false,
            "audio/wave",
        ),
        Mp4,
        Exact(b"\x1a\x45\xdf\xa3", "video/webm"),
        // 34 bytes of anything, then "LP".
        Masked(
            b"\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00LP",
            Some(b"\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\xff\xff"),
            false,
            "application/vnd.ms-fontobject",
        ),
        Exact(b"\x00\x01\x00\x00", "font/ttf"),
        Exact(b"OTTO", "font/otf"),
        Exact(b"ttcf", "font/collection"),
        Exact(b"wOFF", "font/woff"),
        Exact(b"wOF2", "font/woff2"),
        Exact(b"\x1f\x8b\x08", "application/x-gzip"),
        Exact(b"PK\x03\x04", "application/zip"),
        Exact(b"Rar!\x1a\x07\x00", "application/x-rar-compressed"),
        Exact(b"Rar!\x1a\x07\x01\x00", "application/x-rar-compressed"),
        Exact(b"\x00asm", "application/wasm"),
    ];
    let b = &b[..b.len().min(512)];
    let ws = b
        .iter()
        .position(|c| !matches!(c, b'\t' | b'\n' | b'\x0c' | b'\r' | b' '))
        .unwrap_or(b.len());
    let masked = |data: &[u8], pat: &[u8], mask: Option<&[u8]>| {
        data.len() >= pat.len()
            && pat
                .iter()
                .enumerate()
                .all(|(i, &p)| data[i] & mask.map_or(0xff, |m| m[i]) == p)
    };
    for sig in SIGS {
        let hit = match *sig {
            Html(s) => {
                let d = &b[ws..];
                d.len() > s.len()
                    && d[..s.len()].eq_ignore_ascii_case(s)
                    && matches!(d[s.len()], b' ' | b'>')
            }
            Exact(s, _) => b.starts_with(s),
            Masked(pat, mask, skip_ws, _) => masked(if skip_ws { &b[ws..] } else { b }, pat, mask),
            Mp4 => is_mp4(b),
        };
        if hit {
            return match *sig {
                Html(_) => "text/html",
                Mp4 => "video/mp4",
                Exact(_, t) | Masked(_, _, _, t) => t,
            };
        }
    }
    if b[ws..]
        .iter()
        .any(|&c| matches!(c, 0x00..=0x08 | 0x0b | 0x0e..=0x1a | 0x1c..=0x1f))
    {
        return "application/octet-stream";
    }
    "text/plain"
}

/// Go's MP4 signature: an `ftyp` box whose brands include `mp4`.
fn is_mp4(b: &[u8]) -> bool {
    if b.len() < 12 {
        return false;
    }
    let size = u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize;
    if b.len() < size || !size.is_multiple_of(4) || &b[4..8] != b"ftyp" {
        return false;
    }
    (8..size)
        .step_by(4)
        .any(|st| st != 12 && b.get(st..st + 3) == Some(b"mp4".as_slice()))
}

/// The `filename` of a `Content-Disposition` header (`filename*` in RFC 2231/5987 form
/// preferred).
fn disposition_filename(h: &str) -> Option<String> {
    let mut plain = None;
    let mut extended = None;
    for part in h.split(';').skip(1) {
        let Some((k, v)) = part.split_once('=') else {
            continue;
        };
        let v = v.trim();
        match text::to_lower(k.trim()).as_str() {
            "filename" => plain = Some(v.trim_matches('"').to_owned()),
            "filename*" => {
                let encoded = v.splitn(3, '\'').nth(2).unwrap_or(v).trim_matches('"');
                extended = unescape(encoded, Component::PathSegment)
                    .ok()
                    .and_then(|b| String::from_utf8(b).ok());
            }
            _ => {}
        }
    }
    extended.or(plain).filter(|f| !f.is_empty())
}

/// Fetches `url` (no retries; a status outside 2xx is a response, not an error).
fn fetch(url: &str, o: &RemoteOptions, timeout: Duration) -> Result<Response, RemoteError> {
    let net = |e: &dyn std::fmt::Display| RemoteError::Network {
        url: url.to_owned(),
        reason: e.to_string(),
    };
    let config = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(timeout))
        .user_agent(ssg_base::APP_NAME)
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let mut b = ureq::http::Request::builder()
        .method(o.method.as_str())
        .uri(url);
    for (k, vs) in &o.headers {
        for v in vs {
            b = b.header(k.as_str(), v.as_str());
        }
    }
    let res = if o.body.is_empty() {
        agent.run(b.body(()).map_err(|e| net(&e))?)
    } else {
        agent.run(b.body(o.body.clone()).map_err(|e| net(&e))?)
    }
    .map_err(|e| net(&e))?;
    let status = res.status();
    let code = status.as_u16();
    let status_text = match status.canonical_reason() {
        Some(r) => format!("{code} {r}"),
        None => code.to_string(),
    };
    let headers = res
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                canonical_header(k.as_str()),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();
    let body = res
        .into_body()
        .with_config()
        .limit(1 << 30)
        .read_to_vec()
        .map_err(|e| net(&e))?;
    Ok(Response {
        status: status_text,
        code,
        headers,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responses_round_trip() {
        let r = Response::parse(
            b"HTTP/2.0 200 OK\r\ncontent-type: text/plain\r\nX-A: 1\r\n\r\nbody\r\n\r\nmore",
        )
        .unwrap();
        assert_eq!(r.code, 200);
        assert_eq!(r.status, "200 OK");
        assert_eq!(r.header("Content-Type"), Some("text/plain"));
        assert_eq!(r.headers[0].0, "Content-Type");
        assert_eq!(r.body, b"body\r\n\r\nmore");
        assert_eq!(Response::parse(&r.to_bytes()).unwrap(), r);
    }

    #[test]
    fn dispositions() {
        assert_eq!(
            disposition_filename("attachment; filename=\"report.json\"").as_deref(),
            Some("report.json")
        );
        assert_eq!(
            disposition_filename("attachment; filename=x; filename*=UTF-8''na%C3%AFve%20file.txt")
                .as_deref(),
            Some("naïve file.txt")
        );
        assert_eq!(disposition_filename("inline"), None);
    }

    #[test]
    fn sniffing() {
        assert_eq!(sniff(b"{\"a\":1}"), "text/plain");
        assert_eq!(sniff(b"  <!doctype html><p>"), "text/html");
        assert_eq!(sniff(b"<?xml version=\"1.0\"?>"), "text/xml");
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\n...."), "image/png");
        assert_eq!(sniff(b"\x00\x01\x02"), "application/octet-stream");
        assert_eq!(sniff(b"\x00\x01\x00\x00\x00\x12\x01\x00"), "font/ttf");
        assert_eq!(sniff(b"wOF2\x00\x01\x00\x00"), "font/woff2");
        assert_eq!(sniff(b"OTTO\x00\x0b"), "font/otf");
        assert_eq!(sniff(b"RIFF\x10\x00\x00\x00WEBPVP8 "), "image/webp");
        assert_eq!(sniff(b"RIFF\x10\x00\x00\x00WAVEfmt "), "audio/wave");
        assert_eq!(
            sniff(b"\x00\x00\x00\x18ftypmp42\x00\x00\x00\x00mp42isom"),
            "video/mp4"
        );
        assert_eq!(sniff(b"\x00asm\x01\x00\x00\x00"), "application/wasm");
        assert_eq!(sniff(b"\xef\xbb\xbfhello"), "text/plain");
    }
}
