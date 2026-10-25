//! A proxy to an `https://` server (a TLS echo server on a free port here, answering as
//! `localhost` with a certificate of the test authority, `tests/fixtures/tls/`): the request
//! goes in TLS, with the server's own name as `Host` and for SNI, and HTTP/1.1 asked for in
//! ALPN; a certificate the proxy cannot check is a 502.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::thread;

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};

use super::{proxied_site, proxies};
use crate::{PATIENCE, get, serve};

const AUTHORITY: &[u8] = include_bytes!("../../../fixtures/tls/ca.pem");
const CERTIFICATE: &[u8] = include_bytes!("../../../fixtures/tls/localhost.pem");
const KEY: &[u8] = include_bytes!("../../../fixtures/tls/localhost.key");

/// The other server, `https://localhost:<port>`: it answers each request with the request's
/// head, and the name the client asked for (SNI) and the protocol they agreed on (ALPN, of
/// `h2` and `http/1.1`) in `X-Sni` and `X-Alpn`.
fn https_upstream() -> String {
    let certificates = CertificateDer::pem_slice_iter(CERTIFICATE)
        .collect::<Result<Vec<_>, _>>()
        .expect("certificate");
    let key = PrivateKeyDer::from_pem_slice(KEY).expect("key");
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("versions")
        .with_no_client_auth()
        .with_single_cert(certificates, key)
        .expect("the server's certificate");
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    let config = Arc::new(config);
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("address").port();
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let config = Arc::clone(&config);
            thread::spawn(move || {
                stream.set_read_timeout(Some(PATIENCE)).expect("timeout");
                let connection = ServerConnection::new(config).expect("connection");
                let mut tls = StreamOwned::new(connection, stream);
                let mut head = String::new();
                let mut reader = BufReader::new(&mut tls);
                loop {
                    let mut line = String::new();
                    // A client that refused the certificate is gone before its request.
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        return;
                    }
                    if line == "\r\n" {
                        break;
                    }
                    head.push_str(&line);
                }
                let sni = tls.conn.server_name().unwrap_or_default().to_owned();
                let alpn = tls
                    .conn
                    .alpn_protocol()
                    .map(|p| String::from_utf8_lossy(p).into_owned())
                    .unwrap_or_default();
                let _ = write!(
                    tls,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nX-Sni: {sni}\r\nX-Alpn: {alpn}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{head}",
                    head.len()
                );
                tls.conn.send_close_notify();
                let _ = tls.flush();
            });
        }
    });
    format!("https://localhost:{port}")
}

#[test]
fn reaches_an_https_server_by_its_own_name() {
    let up = https_upstream();
    let up_port = up.rsplit(':').next().expect("port");
    let dir = proxied_site(&proxies(&[("/api/", &up)]));
    let (server, events) = serve(dir.path(), |o| {
        o.proxy_roots = vec![CertificateDer::from_pem_slice(AUTHORITY).expect("authority")];
    });
    let addr = server.local_addrs()[0];
    let port = addr.port();
    assert_eq!(
        events.count(&format!("Proxying {{ from: \"/api/\", to: \"{up}\" }}")),
        1,
        "{:#?}",
        events.lines()
    );

    let r = get(addr, "/api/items?x=1", &[]);
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(
        (r.header("x-sni"), r.header("x-alpn")),
        (Some("localhost"), Some("http/1.1"))
    );
    let echo = r.text().to_ascii_lowercase();
    assert!(
        echo.starts_with("get /api/items?x=1 http/1.1\r\n"),
        "{echo}"
    );
    // Its own name as `Host`, the browser's in `X-Forwarded-Host`.
    for line in [
        format!("host: localhost:{up_port}\r\n"),
        format!("x-forwarded-host: localhost:{port}\r\n"),
        "x-forwarded-proto: http\r\n".to_owned(),
    ] {
        assert!(echo.contains(&line), "{line:?} in {echo}");
    }
    assert_eq!(events.count("error"), 0, "{:#?}", events.lines());
    server.shutdown();
}

#[test]
fn a_certificate_the_proxy_cannot_check_is_a_bad_gateway() {
    let up = https_upstream();
    let dir = proxied_site(&proxies(&[("/api/", &up)]));
    // Mozilla's authorities alone, which did not sign the test server's certificate.
    let (server, events) = serve(dir.path(), |_| {});
    let addr = server.local_addrs()[0];

    let r = get(addr, "/api/x", &[]);
    assert_eq!(r.status, 502, "{}", r.text());
    let message = format!("proxy: GET /api/x: {up}: ");
    let text = r.text();
    assert!(
        text.starts_with(&message) && text.contains("certificate"),
        "{text}"
    );
    assert_eq!(
        events.count(&format!("error {message}")),
        1,
        "{:#?}",
        events.lines()
    );
    server.shutdown();
}
