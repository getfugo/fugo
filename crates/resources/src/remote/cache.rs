//! The cache of remote resources: the Go build's names for its entries, HTTP responses as entries,
//! and their freshness.

use super::*;

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
pub(super) fn canonical_header(name: &str) -> String {
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
    pub(super) fn header(&self, name: &str) -> Option<&str> {
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
    pub(super) fn data(&self, wanted: &[String], with_body: bool) -> Map {
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
pub(super) type Slot = Arc<Mutex<Option<Result<Option<ResourceId>, RemoteError>>>>;

#[derive(Default)]
pub(crate) struct RemoteState {
    pub(super) memo: Mutex<HashMap<(LangIdx, Vec<u8>), Slot>>,
}

pub(super) fn is_fresh(path: &Path, max_age: MaxAge) -> bool {
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

pub(super) fn cache_error(path: &Path, e: &std::io::Error) -> RemoteError {
    RemoteError::Cache {
        path: path.to_owned(),
        reason: e.to_string(),
    }
}
