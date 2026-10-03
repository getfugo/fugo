//! The LiveReload protocol (livereload.com protocol 7), as Go's `livereload` package speaks
//! it: the browser loads `livereload.js` and opens a WebSocket to `livereload`; it sends
//! `hello`, the server answers `hello`, then sends one `reload` command per change.
//!
//! - A `reload` whose path is a stylesheet or an image is applied without reloading the page
//!   (`liveCSS`, `liveImg`); any other path reloads the page. [`force_refresh`] uses `/x.js`.
//! - [`navigate`] sends the Go implementation's navigate prefix ([`NAVIGATE_PREFIX`]), which
//!   its plugin bundled into `livereload.js` turns into a navigation (`--navigateToChanged`),
//!   with the port of the page's server in `overrideURL`.
//!
//! `assets/livereload.min.js` is the Go implementation's `livereload/livereload.min.js`, used
//! verbatim: livereload-js 4.0.2 (which bundles core-js 2.6.12 modules) and the Go
//! implementation's navigation plugin for it, bundled and minified by esbuild.
//!
//! livereload-js: Copyright (c) 2010-2015 Andrey Tarantsov. core-js: Copyright (c) 2014-2020
//! Denis Pushkarev. Both MIT:
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy of this
//! software and associated documentation files (the "Software"), to deal in the Software
//! without restriction, including without limitation the rights to use, copy, modify, merge,
//! publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons
//! to whom the Software is furnished to do so, subject to the following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all copies or
//! substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED,
//! INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR
//! PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE
//! FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
//! OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
//! DEALINGS IN THE SOFTWARE.
//!
//! (`THIRD_PARTY/livereload/LICENSE`; the Go implementation's plugin is Apache-2.0,
//! `THIRD_PARTY/hugo/LICENSE`.)

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket};
use axum::http::HeaderMap;
use axum::http::header::{HOST, ORIGIN};
use ssg_base::url::UrlRef;
use tokio::sync::{broadcast, watch};

/// `livereload.js`, served at `<base path>livereload.js`.
pub(crate) const SCRIPT: &str = include_str!("../assets/livereload.min.js");

/// The media type Go serves `livereload.js` with.
pub(crate) const SCRIPT_TYPE: &str = "text/javascript";

/// The answer to the browser's `hello`.
const HELLO: &str = concat!(
    r#"{"command":"hello","protocols":["http://livereload.com/protocols/official-7"],"serverName":""#,
    ssg_base::app_name!(),
    r#""}"#
);

/// The prefix of a path that the Go implementation's plugin of `livereload.js` navigates to
/// (the bundled script recognises this exact literal).
const NAVIGATE_PREFIX: &str = "__ssg_navigate";

/// Reloads `path` in every browser: stylesheets and images in place, the page otherwise.
pub(crate) fn reload(path: &str) -> String {
    reload_message(path, None)
}

/// Reloads the page in every browser.
pub(crate) fn force_refresh() -> String {
    reload("/x.js")
}

/// Sends every browser to `path` on `port` (`--navigateToChanged`); a browser already there
/// reloads.
pub(crate) fn navigate(path: &str, port: Option<u16>) -> String {
    reload_message(&format!("{NAVIGATE_PREFIX}{path}"), port)
}

fn reload_message(path: &str, port: Option<u16>) -> String {
    let path = serde_json::Value::String(path.to_owned());
    let port = port.map_or_else(String::new, |p| format!(r#", "overrideURL": {p}"#));
    format!(
        r#"{{"command":"reload","path":{path},"originalPath":"","liveCSS":true,"liveImg":true{port}}}"#
    )
}

/// Go's origin check for the WebSocket: no `Origin`, the same host as the request's (or its
/// `X-Forwarded-Host`), or the same host name on another port (a multihost site's other
/// servers).
pub(crate) fn origin_allowed(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(ORIGIN) else {
        return true;
    };
    let Some(origin) = origin.to_str().ok().and_then(|o| UrlRef::parse(o).ok()) else {
        return false;
    };
    let host = headers
        .get(HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let forwarded = headers
        .get("x-forwarded-host")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let request_host = if forwarded.is_empty() {
        host
    } else {
        forwarded
    };
    let origin_host = String::from_utf8_lossy(origin.host());
    if origin_host == request_host {
        return true;
    }
    match (split_host_port(&origin_host), split_host_port(host)) {
        (Some((a, _)), Some((b, _))) => a == b,
        _ => false,
    }
}

/// `host:port` (or `[v6]:port`) split in two; `None` without a port.
fn split_host_port(s: &str) -> Option<(&str, &str)> {
    let (host, port) = s.rsplit_once(':')?;
    let host = match host.strip_prefix('[') {
        Some(h) => h.strip_suffix(']')?,
        None if host.contains(':') => return None,
        None => host,
    };
    Some((host, port))
}

/// One browser's connection: answers `hello`, forwards the reload commands, ends when the
/// browser leaves or the server shuts down.
pub(crate) async fn session(
    mut ws: WebSocket,
    mut commands: broadcast::Receiver<Arc<str>>,
    mut shutdown: watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            incoming = ws.recv() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    if text.as_str().contains(r#""command":"hello""#)
                        && ws.send(Message::Text(HELLO.into())).await.is_err()
                    {
                        return;
                    }
                }
                Some(Ok(Message::Close(_)) | Err(_)) | None => return,
                Some(Ok(_)) => {}
            },
            command = commands.recv() => match command {
                Ok(text) => {
                    if ws.send(Message::Text(text.as_ref().into())).await.is_err() {
                        return;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => return,
            },
            () = async {
                let _ = shutdown.wait_for(|stop| *stop).await;
            } => {
                let _ = ws.send(Message::Close(None)).await;
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages() {
        assert_eq!(
            force_refresh(),
            r#"{"command":"reload","path":"/x.js","originalPath":"","liveCSS":true,"liveImg":true}"#
        );
        assert_eq!(
            navigate("/posts/one/", Some(1313)),
            r#"{"command":"reload","path":"__ssg_navigate/posts/one/","originalPath":"","liveCSS":true,"liveImg":true, "overrideURL": 1313}"#
        );
        assert!(reload("/a\"b.css").contains(r#""path":"/a\"b.css""#));
    }

    #[test]
    fn origins() {
        let headers = |pairs: &[(&'static str, &'static str)]| {
            let mut h = HeaderMap::new();
            for (k, v) in pairs {
                h.insert(*k, v.parse().unwrap());
            }
            h
        };
        assert!(origin_allowed(&headers(&[("host", "localhost:1313")])));
        assert!(origin_allowed(&headers(&[
            ("host", "localhost:1313"),
            ("origin", "http://localhost:1313")
        ])));
        // A multihost site's other server.
        assert!(origin_allowed(&headers(&[
            ("host", "localhost:1313"),
            ("origin", "http://localhost:1314")
        ])));
        assert!(origin_allowed(&headers(&[
            ("host", "127.0.0.1:1313"),
            ("x-forwarded-host", "x.github.dev"),
            ("origin", "https://x.github.dev")
        ])));
        assert!(!origin_allowed(&headers(&[
            ("host", "localhost:1313"),
            ("origin", "http://evil.example:1313")
        ])));
        assert!(!origin_allowed(&headers(&[
            ("host", "localhost"),
            ("origin", "http://evil.example")
        ])));
        assert!(origin_allowed(&headers(&[
            ("host", "[::1]:1313"),
            ("origin", "http://[::1]:1314")
        ])));
    }
}
