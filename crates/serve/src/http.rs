//! The HTTP side (Go's `fileServer`): the files of the served tree, directory indexes and
//! redirects as Go's `http.FileServer` does them, the 404 page, `livereload.js` and the
//! LiveReload WebSocket.

use std::sync::Arc;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::ws::WebSocketUpgrade;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use ssg_base::url::{Component, unescape};

use crate::tree::{Host, Served};
use crate::{HttpCache, Shared, livereload};

#[derive(Clone)]
struct HostState {
    shared: Arc<Shared>,
    /// The listener (the language of a multihost site).
    index: usize,
}

/// The routes of listener `index`.
pub(crate) fn router(shared: Arc<Shared>, index: usize) -> Router {
    Router::new()
        .fallback(handle)
        .with_state(HostState { shared, index })
}

async fn handle(State(s): State<HostState>, req: Request) -> Response {
    // The parts only: the body is not `Sync`, so a reference to the request cannot be held
    // across an await.
    let (mut req, _body) = req.into_parts();
    let served = s.shared.served();
    let Some(host) = served.hosts.get(s.index) else {
        return plain_not_found();
    };
    let path = decode(req.uri.path());
    if s.shared.live_reload {
        let base = host.base_path.as_str();
        if path.strip_prefix(base) == Some("livereload.js") {
            return respond(StatusCode::OK, livereload::SCRIPT_TYPE, livereload::SCRIPT);
        }
        if path.strip_prefix(base) == Some("livereload") {
            return websocket(&s.shared, &mut req).await;
        }
    }
    let mut response = files(&served, host, &req, &path).await;
    if s.shared.http_cache == HttpCache::Disabled {
        let h = response.headers_mut();
        h.insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store, no-cache, must-revalidate, max-age=0"),
        );
        h.insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    }
    response
}

/// The LiveReload WebSocket, behind Go's origin check.
async fn websocket(shared: &Shared, parts: &mut Parts) -> Response {
    if !livereload::origin_allowed(&parts.headers) {
        return respond(
            StatusCode::FORBIDDEN,
            "text/plain; charset=utf-8",
            "request origin not allowed\n",
        );
    }
    match WebSocketUpgrade::from_request_parts(parts, &()).await {
        Ok(ws) => {
            let commands = shared.reload.subscribe();
            let shutdown = shared.shutdown.clone();
            ws.on_upgrade(move |socket| livereload::session(socket, commands, shutdown))
        }
        Err(rejection) => rejection.into_response(),
    }
}

/// A file of the site, a redirect, or a miss.
async fn files(served: &Served, host: &Host, req: &Parts, path: &str) -> Response {
    let Some(rest) = path.strip_prefix(host.base_path.as_str()) else {
        // `/docs` is the site at `/docs/` (Go's `ServeMux` redirect); anything else is not
        // the site's.
        if format!("{path}/") == host.base_path {
            return redirect(&with_query(&host.base_path, req));
        }
        return plain_not_found();
    };
    if rest == "index.html" || rest.ends_with("/index.html") {
        return redirect(&with_query("./", req));
    }
    let rel = clean(rest);
    let file = format!("{}{rel}", host.root);
    let tree = &served.tree;
    if rest.is_empty() || rest.ends_with('/') {
        let index = if rel.is_empty() {
            format!("{}index.html", host.root)
        } else {
            format!("{file}/index.html")
        };
        if let Some(bytes) = tree.read(&index).await {
            return file_response(served, req, &index, bytes);
        }
        if !rel.is_empty() && tree.is_file(&file).await {
            return redirect(&with_query(&format!("../{}", base_name(&rel)), req));
        }
    } else {
        if let Some(bytes) = tree.read(&file).await {
            return file_response(served, req, &file, bytes);
        }
        if tree.is_file(&format!("{file}/index.html")).await {
            return redirect(&with_query(&format!("{}/", base_name(&rel)), req));
        }
    }
    if is_navigation(&req.headers, path) {
        return not_found_page(served, host, &rel).await;
    }
    plain_not_found()
}

/// Go's default `[[server.redirects]]` (`/**` → `/404.html`, status 404), per language: the
/// 404 page of the language directory the path is in, else the site's, else the first
/// language's.
async fn not_found_page(served: &Served, host: &Host, rel: &str) -> Response {
    let own = host
        .languages
        .iter()
        .filter(|l| rel == l.as_str() || rel.starts_with(&format!("{l}/")))
        .max_by_key(|l| l.len());
    let candidates = own
        .into_iter()
        .map(|l| format!("{}{l}/404.html", host.root))
        .chain([format!("{}404.html", host.root)])
        .chain(
            host.languages
                .iter()
                .map(|l| format!("{}{l}/404.html", host.root)),
        );
    for c in candidates {
        if let Some(bytes) = served.tree.read(&c).await {
            return respond(StatusCode::NOT_FOUND, "text/html; charset=utf-8", bytes);
        }
    }
    respond(
        StatusCode::NOT_FOUND,
        "text/html; charset=utf-8",
        "<h1>Page Not Found</h1>\n",
    )
}

/// A file, or the byte range the request asks for.
fn file_response(served: &Served, req: &Parts, rel: &str, bytes: Bytes) -> Response {
    let content_type = served.media_types.of(rel, &bytes);
    let range = req
        .headers
        .get(header::RANGE)
        .and_then(|v| v.to_str().ok())
        .and_then(|r| byte_range(r, bytes.len()));
    let mut response = match range {
        None => respond(StatusCode::OK, &content_type, bytes),
        Some(Some((start, end))) => {
            let total = bytes.len();
            let mut r = respond(
                StatusCode::PARTIAL_CONTENT,
                &content_type,
                bytes.slice(start..=end),
            );
            insert(
                r.headers_mut(),
                header::CONTENT_RANGE,
                &format!("bytes {start}-{end}/{total}"),
            );
            r
        }
        Some(None) => {
            let mut r = respond(
                StatusCode::RANGE_NOT_SATISFIABLE,
                "text/plain; charset=utf-8",
                "invalid range: failed to overlap\n",
            );
            insert(
                r.headers_mut(),
                header::CONTENT_RANGE,
                &format!("bytes */{}", bytes.len()),
            );
            r
        }
    };
    response
        .headers_mut()
        .insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    response
}

/// A single `bytes=` range of a body of `len` bytes: `None` when there is none (or several),
/// `Some(None)` when it is not satisfiable, else the inclusive bounds.
fn byte_range(spec: &str, len: usize) -> Option<Option<(usize, usize)>> {
    let spec = spec.trim().strip_prefix("bytes=")?;
    if spec.contains(',') || len == 0 {
        return None;
    }
    let (first, last) = spec.split_once('-')?;
    let (first, last) = (first.trim(), last.trim());
    if first.is_empty() {
        let n: usize = last.parse().ok()?;
        if n == 0 {
            return Some(None);
        }
        return Some(Some((len - n.min(len), len - 1)));
    }
    let start: usize = first.parse().ok()?;
    if start >= len {
        return Some(None);
    }
    let end = if last.is_empty() {
        len - 1
    } else {
        let end: usize = last.parse().ok()?;
        if end < start {
            return None;
        }
        end.min(len - 1)
    };
    Some(Some((start, end)))
}

/// Go's test for a page navigation: `Sec-Fetch-Mode: navigate`, else a path that ends with
/// `/`, `html` or `htm`, or has no `.`.
fn is_navigation(headers: &HeaderMap, path: &str) -> bool {
    headers
        .get("sec-fetch-mode")
        .is_some_and(|v| v.as_bytes() == b"navigate")
        || path.ends_with('/')
        || path.ends_with("html")
        || path.ends_with("htm")
        || !path.contains('.')
}

/// The percent-decoded request path (as it is when it does not decode).
fn decode(path: &str) -> String {
    match unescape(path, Component::Path) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(_) => path.to_owned(),
    }
}

/// `path` cleaned like Go's `path.Clean` and made relative: no empty, `.` or `..` segments,
/// no leading or trailing slash.
fn clean(path: &str) -> String {
    let mut segments: Vec<&str> = Vec::new();
    for s in path.split('/') {
        match s {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            s => segments.push(s),
        }
    }
    segments.join("/")
}

fn base_name(rel: &str) -> &str {
    rel.rsplit('/').next().unwrap_or(rel)
}

fn with_query(location: &str, req: &Parts) -> String {
    match req.uri.query() {
        Some(q) if !q.is_empty() => format!("{location}?{q}"),
        _ => location.to_owned(),
    }
}

/// A `301 Moved Permanently` to `location` (relative locations resolve against the request).
fn redirect(location: &str) -> Response {
    let mut r = respond(
        StatusCode::MOVED_PERMANENTLY,
        "text/html; charset=utf-8",
        format!(
            "<a href=\"{}\">Moved Permanently</a>.\n\n",
            html_escape(location)
        ),
    );
    insert(r.headers_mut(), header::LOCATION, location);
    r
}

/// Go's `http.NotFound`.
fn plain_not_found() -> Response {
    let mut r = respond(
        StatusCode::NOT_FOUND,
        "text/plain; charset=utf-8",
        "404 page not found\n",
    );
    r.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    r
}

fn respond(status: StatusCode, content_type: &str, body: impl Into<Body>) -> Response {
    let mut r = Response::new(body.into());
    *r.status_mut() = status;
    insert(r.headers_mut(), header::CONTENT_TYPE, content_type);
    r
}

fn insert(headers: &mut HeaderMap, name: header::HeaderName, value: &str) {
    if let Ok(v) = HeaderValue::from_str(value) {
        headers.insert(name, v);
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&#34;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges() {
        assert_eq!(byte_range("bytes=0-3", 10), Some(Some((0, 3))));
        assert_eq!(byte_range("bytes=5-", 10), Some(Some((5, 9))));
        assert_eq!(byte_range("bytes=-4", 10), Some(Some((6, 9))));
        assert_eq!(byte_range("bytes=-40", 10), Some(Some((0, 9))));
        assert_eq!(byte_range("bytes=8-20", 10), Some(Some((8, 9))));
        assert_eq!(byte_range("bytes=10-", 10), Some(None));
        assert_eq!(byte_range("bytes=-0", 10), Some(None));
        assert_eq!(byte_range("bytes=0-1,4-5", 10), None);
        assert_eq!(byte_range("items=0-1", 10), None);
        assert_eq!(byte_range("bytes=5-2", 10), None);
    }

    #[test]
    fn paths() {
        assert_eq!(clean("a//b/./c/../d/"), "a/b/d");
        assert_eq!(clean("../../etc/passwd"), "etc/passwd");
        assert_eq!(clean(""), "");
        assert_eq!(decode("/posts/ol%C3%A9/"), "/posts/olé/");
        assert_eq!(decode("/100%/"), "/100%/");
        let nav = |p| is_navigation(&HeaderMap::new(), p);
        assert!(nav("/a/") && nav("/a.html") && nav("/a") && nav("/a.htm"));
        assert!(!nav("/a.css") && !nav("/img/x.png"));
        let mut h = HeaderMap::new();
        h.insert("sec-fetch-mode", HeaderValue::from_static("navigate"));
        assert!(is_navigation(&h, "/a.css"));
    }
}
