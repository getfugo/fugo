//! Forwarding a request to a proxy's server, over a new HTTP/1.1 connection (hyper's client):
//! the method, the path and query as they came, the headers without the hop-by-hop ones (RFC
//! 9110 §7.6.1) and with `X-Forwarded-For`, `-Host` and `-Proto`, `Host` kept, and the body as
//! it arrives; the answer streams back the same way. An upgrade (a WebSocket) joins the two
//! connections until one side closes or the server stops. A server that cannot be reached is a
//! `502 Bad Gateway`, reported as an error.

use std::fmt::Display;
use std::net::SocketAddr;

use axum::body::Body;
use axum::extract::{ConnectInfo, Request};
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, Version, header};
use axum::response::Response;
use hyper::upgrade::OnUpgrade;
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;

use super::Proxy;
use crate::{Event, Shared};

/// The answer of `proxy`'s server to `req`.
pub(crate) async fn forward(proxy: &Proxy, shared: &Shared, mut req: Request) -> Response {
    let what = format!("{} {}", req.method(), req.uri());
    let downstream = req.extensions_mut().remove::<OnUpgrade>();
    let client = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip().to_canonical());
    let headers = req.headers_mut();
    strip_hop_by_hop(headers, downstream.is_some());
    if let Some(ip) = client {
        let ip = ip.to_string();
        let forwarded = match headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            Some(earlier) => format!("{earlier}, {ip}"),
            None => ip,
        };
        set(headers, "x-forwarded-for", &forwarded);
    }
    let host = headers
        .get(header::HOST)
        .cloned()
        .or_else(|| HeaderValue::from_str(&proxy.to.host).ok());
    if let Some(host) = host {
        headers.entry("x-forwarded-host").or_insert(host.clone());
        headers.insert(header::HOST, host);
    }
    headers
        .entry("x-forwarded-proto")
        .or_insert(HeaderValue::from_static("http"));
    // The origin form (`/api/x?y`): the request goes to the server itself, not to a proxy.
    *req.uri_mut() = req
        .uri()
        .path_and_query()
        .map_or_else(|| Uri::from_static("/"), |pq| Uri::from(pq.clone()));
    *req.version_mut() = Version::HTTP_11;

    let stream = match TcpStream::connect(proxy.to.addr.as_str()).await {
        Ok(s) => s,
        Err(e) => return bad_gateway(shared, proxy, &what, e),
    };
    let (mut sender, connection) =
        match hyper::client::conn::http1::handshake(TokioIo::new(stream)).await {
            Ok(h) => h,
            Err(e) => return bad_gateway(shared, proxy, &what, e),
        };
    tokio::spawn(async move {
        // Ends with the answer's body (or the upgraded connection).
        let _ = connection.with_upgrades().await;
    });
    let mut response = match sender.send_request(req).await {
        Ok(r) => r,
        Err(e) => return bad_gateway(shared, proxy, &what, e),
    };
    match downstream {
        Some(downstream) if response.status() == StatusCode::SWITCHING_PROTOCOLS => {
            strip_hop_by_hop(response.headers_mut(), true);
            let upstream = hyper::upgrade::on(&mut response);
            let mut stopped = shared.shutdown.clone();
            tokio::spawn(async move {
                let (Ok(down), Ok(up)) = tokio::join!(downstream, upstream) else {
                    return;
                };
                let (mut down, mut up) = (TokioIo::new(down), TokioIo::new(up));
                tokio::select! {
                    _ = tokio::io::copy_bidirectional(&mut down, &mut up) => {}
                    _ = stopped.wait_for(|stop| *stop) => {}
                }
            });
        }
        _ => strip_hop_by_hop(response.headers_mut(), false),
    }
    // The server's own version, not the other server's (hyper answers a 1.0 client in 1.0).
    *response.version_mut() = Version::HTTP_11;
    response.map(Body::new)
}

/// Removes the headers of one connection only: those `Connection` names, and RFC 9110's
/// `Connection`, `Proxy-Connection`, `Keep-Alive`, `TE`, `Transfer-Encoding` and `Upgrade`.
/// An upgrade keeps `Upgrade`, with `Connection: upgrade`.
fn strip_hop_by_hop(headers: &mut HeaderMap, upgrade: bool) {
    let named: Vec<String> = headers
        .get_all(header::CONNECTION)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .map(|name| name.trim().to_ascii_lowercase())
        .filter(|name| !name.is_empty())
        .collect();
    for name in named.iter().map(String::as_str).chain([
        "connection",
        "proxy-connection",
        "keep-alive",
        "te",
        "transfer-encoding",
        "upgrade",
    ]) {
        if !(upgrade && name == "upgrade") {
            headers.remove(name);
        }
    }
    if upgrade {
        headers.insert(header::CONNECTION, HeaderValue::from_static("upgrade"));
    }
}

fn set(headers: &mut HeaderMap, name: &'static str, value: &str) {
    if let Ok(v) = HeaderValue::from_str(value) {
        headers.insert(name, v);
    }
}

/// The `502 Bad Gateway` of a request `what` that `proxy` could not forward.
fn bad_gateway(shared: &Shared, proxy: &Proxy, what: &str, error: impl Display) -> Response {
    let message = format!("proxy: {what}: {}: {error}", proxy.url());
    shared.reporter.report(&Event::Error { message: &message });
    let mut r = Response::new(Body::from(format!("{message}\n")));
    *r.status_mut() = StatusCode::BAD_GATEWAY;
    r.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hop_by_hop_headers() {
        let mut h = HeaderMap::new();
        for (k, v) in [
            ("connection", "keep-alive, X-Secret"),
            ("keep-alive", "timeout=5"),
            ("x-secret", "1"),
            ("te", "trailers"),
            ("transfer-encoding", "chunked"),
            ("upgrade", "websocket"),
            ("content-type", "application/json"),
            ("content-length", "2"),
        ] {
            h.insert(k, HeaderValue::from_static(v));
        }
        let mut upgrading = h.clone();
        strip_hop_by_hop(&mut h, false);
        let names = |h: &HeaderMap| {
            let mut names: Vec<&str> = h.keys().map(header::HeaderName::as_str).collect();
            names.sort_unstable();
            names.join(" ")
        };
        assert_eq!(names(&h), "content-length content-type");
        strip_hop_by_hop(&mut upgrading, true);
        assert_eq!(
            names(&upgrading),
            "connection content-length content-type upgrade"
        );
        assert_eq!(upgrading["connection"], "upgrade");
        assert_eq!(upgrading["upgrade"], "websocket");
    }
}
