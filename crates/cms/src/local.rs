//! The editor's API on this computer: `fugo server` answers the requests of the editor that
//! the Worker answers in production (`web/assets/worker.ts`), with the local git repository
//! for the git host, so the editor can be tried — drafts, publishing and all — without a git
//! host, a Worker or signing in.
//!
//! - **Who:** nobody signs in. The person is git's author identity (`user.name` and
//!   `user.email`), with every role of `[cms.roles]`, who may write anything in the editor's
//!   areas ([`crate::paths`]) and publish.
//! - **Where:** the branch checked out (not `cms.git.branch`). A draft is a branch `cms/<id>`
//!   made from it, as on the git host; there are no pull requests. Saving with workflow
//!   `direct`, and publishing, commit onto it and move the checkout with it (a fast-forward,
//!   which refuses to overwrite uncommitted changes of the same files), and the server
//!   rebuilds the site from the files. Nothing is pushed.
//! - **Safety:** the API answers only requests from this computer (a loopback connection)
//!   to `localhost`, `127.0.0.1` or `[::1]` (the `Host` header: no DNS rebinding), and a POST
//!   only from the editor's own page (`Origin`), as JSON. Writes are made one at a time.

use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use serde::Serialize;
use serde_json::{Map, Value};
use ssg_config::Config;

use crate::config::{CmsConfig, Workflow};
use crate::paths::Area;

mod api;
mod git;
mod rules;

/// The headers of every answer.
pub const HEADERS: &[(&str, &str)] = &[
    ("content-type", "application/json; charset=utf-8"),
    ("cache-control", "no-store"),
    ("x-content-type-options", "nosniff"),
];

/// The host names the API answers at.
const LOCAL_HOSTS: &[&str] = &["localhost", "127.0.0.1", "[::1]"];

/// The editor of a build, with what its API needs: the settings and the content index.
#[derive(Debug)]
pub struct Editor {
    /// The editor's URL path (`/admin/`).
    pub path: String,
    /// The API's URL path (`/admin/api/`).
    pub api: String,
    project_dir: PathBuf,
    workflow: Workflow,
    roles: Vec<String>,
    areas: Vec<Area>,
    max_upload: u64,
    /// `cms.git.repo`, as the Worker reports it.
    repo: String,
    /// The content index (`GET site`), as JSON.
    index: String,
}

/// A request to the API.
#[derive(Debug, Default)]
pub struct Call {
    pub method: String,
    /// The path after the API's (`file`, `save`).
    pub name: String,
    /// The query string, without `?`.
    pub query: String,
    /// The `Host` header.
    pub host: Option<String>,
    /// Whether the connection comes from this computer (a loopback address).
    pub loopback: bool,
    pub origin: Option<String>,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
}

/// An answer of the API: its status and JSON body (sent with [`HEADERS`]).
#[derive(Debug)]
pub struct Answer {
    pub status: u16,
    pub body: String,
}

impl Answer {
    /// The answer to a body larger than [`Editor::body_limit`].
    #[must_use]
    pub fn too_large() -> Self {
        HttpError::new(413, "too large").into()
    }
}

/// An error answer: its status, message and extra fields (`stale`, `conflicts`).
#[derive(Debug)]
pub(crate) struct HttpError {
    status: u16,
    message: String,
    extra: Map<String, Value>,
}

impl HttpError {
    pub(crate) fn new(status: u16, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
            extra: Map::new(),
        }
    }

    pub(crate) fn with(mut self, key: &str, value: impl Serialize) -> Self {
        self.extra.insert(
            key.to_owned(),
            serde_json::to_value(value).unwrap_or_default(),
        );
        self
    }
}

impl From<HttpError> for Answer {
    fn from(e: HttpError) -> Self {
        let mut body = Map::new();
        body.insert("error".to_owned(), e.message.into());
        body.extend(e.extra);
        Self {
            status: e.status,
            body: Value::Object(body).to_string(),
        }
    }
}

/// Writes to the repository are made one at a time (each [`Editor`] is a build's: the lock is
/// the process's).
static WRITES: Mutex<()> = Mutex::new(());

impl Editor {
    pub(crate) fn new(
        cfg: &Config,
        cms: &CmsConfig,
        areas: Vec<Area>,
        (path, api): (String, String),
        index: String,
    ) -> Self {
        Self {
            path,
            api,
            project_dir: cfg.project_dir.clone(),
            workflow: cms.workflow,
            roles: cms.roles.keys().cloned().collect(),
            areas,
            max_upload: cms.max_upload,
            repo: cms.git.repo.clone(),
            index,
        }
    }

    /// The largest request body: an upload as base64, and room for the rest.
    #[must_use]
    pub fn body_limit(&self) -> usize {
        usize::try_from((self.max_upload * 14).div_ceil(10)).unwrap_or(usize::MAX) + (1 << 20)
    }

    /// Answers a request of the editor (blocking: it runs git).
    #[must_use]
    pub fn answer(&self, call: &Call) -> Answer {
        match self.route(call) {
            Ok(body) => Answer { status: 200, body },
            Err(e) => e.into(),
        }
    }

    fn route(&self, call: &Call) -> Result<String, HttpError> {
        if !call.loopback || !call.host.as_deref().is_some_and(is_local_host) {
            return Err(HttpError::new(
                403,
                "the editor's local API answers only on this computer, at localhost",
            ));
        }
        let method = call.method.as_str();
        if method != "GET" && method != "POST" {
            return Err(HttpError::new(405, "method not allowed"));
        }
        let body = if method == "POST" {
            check_post(call, self.body_limit())?
        } else {
            Map::new()
        };
        let name = call.name.as_str();
        if method == "GET" && name == "site" {
            return Ok(self.index.clone());
        }
        let _writing =
            (method == "POST").then(|| WRITES.lock().unwrap_or_else(PoisonError::into_inner));
        let api = api::Api::open(self)?;
        let param = |key: &str| query_param(&call.query, key);
        let out = match (method, name) {
            ("GET", "me") => api.me(),
            ("GET", "file") => api.file(param("path").as_deref(), param("draft").as_deref())?,
            ("GET", "drafts") => api.drafts()?,
            ("GET", "draft") => api.draft(param("id").as_deref())?,
            ("POST", "save") => api.save(&body)?,
            ("POST", "publish") => api.publish(&body)?,
            ("POST", "discard") => api.discard(&body)?,
            _ => return Err(HttpError::new(404, format!("no API {method} {name}"))),
        };
        Ok(out.to_string())
    }
}

/// A POST is same-origin (no cross-site forms) JSON, as a JSON object.
fn check_post(call: &Call, limit: usize) -> Result<Map<String, Value>, HttpError> {
    let own = call.host.as_deref().map(|h| format!("http://{h}"));
    if call.origin.is_none() || call.origin != own {
        return Err(HttpError::new(403, "cross-site request"));
    }
    if !call
        .content_type
        .as_deref()
        .is_some_and(|t| t.starts_with("application/json"))
    {
        return Err(HttpError::new(415, "send JSON"));
    }
    if call.body.len() > limit {
        return Err(HttpError::new(413, "too large"));
    }
    match serde_json::from_slice(&call.body) {
        Ok(Value::Object(body)) => Ok(body),
        _ => Err(HttpError::new(400, "invalid JSON")),
    }
}

/// Whether a `Host` header names this computer.
fn is_local_host(host: &str) -> bool {
    let name = match host.strip_prefix('[') {
        Some(rest) => rest
            .split_once(']')
            .map_or(host, |(ip, _)| &host[..ip.len() + 2]),
        None => host.split(':').next().unwrap_or(host),
    };
    LOCAL_HOSTS.contains(&name.to_ascii_lowercase().as_str())
}

/// The first value of `key` in a query string (`URLSearchParams.get`).
fn query_param(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        (form_decode(k) == key).then(|| form_decode(v))
    })
}

/// A part of a query string, decoded: `+` is a space, `%XX` a byte (kept as it is when it is
/// not a hex pair).
fn form_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = |c: u8| char::from(c).to_digit(16);
        match b[i] {
            b'+' => out.push(b' '),
            b'%' => match (
                b.get(i + 1).copied().and_then(hex),
                b.get(i + 2).copied().and_then(hex),
            ) {
                (Some(h), Some(l)) => {
                    out.push(u8::try_from(h * 16 + l).unwrap_or(b'?'));
                    i += 2;
                }
                _ => out.push(b'%'),
            },
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn only_local_host_names() {
        for ok in [
            "localhost",
            "localhost:1313",
            "127.0.0.1:8080",
            "[::1]:1313",
            "LOCALHOST",
        ] {
            assert!(is_local_host(ok), "{ok}");
        }
        for bad in [
            "evil.example",
            "evil.example:1313",
            "127.0.0.2",
            "[::2]:1",
            "localhost.evil.example",
        ] {
            assert!(!is_local_host(bad), "{bad}");
        }
    }

    #[test]
    fn query_values_are_decoded() {
        let q = "path=content%2Fa%20b.md&draft=&x=1+2&bad=%zz%4";
        assert_eq!(query_param(q, "path").as_deref(), Some("content/a b.md"));
        assert_eq!(query_param(q, "draft").as_deref(), Some(""));
        assert_eq!(query_param(q, "x").as_deref(), Some("1 2"));
        assert_eq!(query_param(q, "bad").as_deref(), Some("%zz%4"));
        assert_eq!(query_param(q, "none"), None);
    }

    #[test]
    fn errors_carry_their_fields() {
        let a: Answer = HttpError::new(409, "stale").with("stale", ["a.md"]).into();
        assert_eq!(a.status, 409);
        assert_eq!(
            serde_json::from_str::<Value>(&a.body).expect("json"),
            json!({ "error": "stale", "stale": ["a.md"] })
        );
    }
}
