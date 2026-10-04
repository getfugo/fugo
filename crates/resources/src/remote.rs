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

mod cache;
mod content;
mod get;

pub use cache::*;
use content::*;

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

#[cfg(test)]
mod tests;
