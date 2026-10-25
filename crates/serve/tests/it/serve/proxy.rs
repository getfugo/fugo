//! The proxies of `--proxy` and `[[server.proxies]]`: the requests of a path go to another
//! server (an echo server on a free port here) with their path, headers and body, its answers
//! come back as they are written, a WebSocket goes through, and a server that is down is a 502.
//! `https.rs`: a proxy to an `https://` server.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use ssg_serve::{Reporter, Server};

use crate::{PATIENCE, Recorder, get, options, serve, site, write};

mod https;

/// A site whose configuration ends with `extra`.
fn proxied_site(extra: &str) -> tempfile::TempDir {
    site(&format!(
        r#"
-- config.toml --
{}
-- layouts/home.html --
<html><body>home</body></html>
-- content/_index.md --
---
title: Home
---
-- static/apiary.txt --
bees
"#,
        config(extra)
    ))
}

fn config(extra: &str) -> String {
    format!(
        r#"baseURL = "https://example.org/"
title = "Proxy"
disableKinds = ["taxonomy", "term", "sitemap", "rss", "robotsTXT"]
{extra}"#
    )
}

fn proxies(entries: &[(&str, &str)]) -> String {
    entries
        .iter()
        .map(|(from, to)| format!("[[server.proxies]]\nfrom = {from:?}\nto = {to:?}\n"))
        .collect()
}

/// The other server: it answers each request with the request it got (`201` for a POST),
/// `/api/stream` in two parts (the second once [`Upstream::release`] is sent), and accepts a
/// WebSocket at `/api/ws` that sends each text message back.
struct Upstream {
    url: String,
    release: Sender<()>,
}

fn upstream() -> Upstream {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("address");
    let (release, released) = mpsc::channel();
    let released = Arc::new(Mutex::new(released));
    thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let released = Arc::clone(&released);
            thread::spawn(move || answer(stream, &released));
        }
    });
    Upstream {
        url: format!("http://{addr}"),
        release,
    }
}

fn answer(mut stream: TcpStream, released: &Mutex<Receiver<()>>) {
    stream.set_read_timeout(Some(PATIENCE)).expect("timeout");
    if request_line(&stream).starts_with("GET /api/ws ") {
        let mut ws = tungstenite::accept(stream).expect("the WebSocket handshake");
        while let Ok(m) = ws.read() {
            if m.is_text() {
                ws.send(m).expect("send");
            } else if m.is_close() {
                return;
            }
        }
        return;
    }
    let mut reader = BufReader::new(stream.try_clone().expect("clone"));
    let mut head = String::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            return;
        }
        if line == "\r\n" {
            break;
        }
        head.push_str(&line);
    }
    let length = head
        .lines()
        .find_map(|l| {
            let l = l.to_ascii_lowercase();
            l.strip_prefix("content-length:")
                .map(|n| n.trim().parse::<usize>().expect("length"))
        })
        .unwrap_or(0);
    let mut body = vec![0; length];
    reader.read_exact(&mut body).expect("body");
    if head.starts_with("GET /api/stream ") {
        let _ = stream.write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n6\r\nfirst\n\r\n",
        );
        let _ = released
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .recv_timeout(PATIENCE);
        let _ = stream.write_all(b"7\r\nsecond\n\r\n0\r\n\r\n");
        return;
    }
    let echo = format!("{head}\r\n{}", String::from_utf8_lossy(&body));
    // A POST is answered in HTTP/1.0 (as Python's http.server answers), which the browser
    // gets in the server's HTTP/1.1.
    let status = if head.starts_with("POST ") {
        "HTTP/1.0 201 Created"
    } else {
        "HTTP/1.1 200 OK"
    };
    let _ = stream.write_all(
        format!(
            "{status}\r\nContent-Type: text/plain\r\nX-Upstream: yes\r\nKeep-Alive: timeout=5\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{echo}",
            echo.len()
        )
        .as_bytes(),
    );
}

/// The request line, left in the stream (a WebSocket's handshake is tungstenite's to read).
fn request_line(stream: &TcpStream) -> String {
    let deadline = Instant::now() + PATIENCE;
    let mut buf = [0; 256];
    loop {
        let n = stream.peek(&mut buf).unwrap_or(0);
        let text = String::from_utf8_lossy(&buf[..n]);
        if let Some((line, _)) = text.split_once("\r\n") {
            return line.to_owned();
        }
        if n == 0 || Instant::now() >= deadline {
            return text.into_owned();
        }
        thread::sleep(Duration::from_millis(5));
    }
}

/// `text` sent as it is to `addr`; the answer, until the server closes the connection.
fn raw(addr: SocketAddr, text: &str) -> String {
    let mut s = TcpStream::connect(addr).expect("connect");
    s.set_read_timeout(Some(PATIENCE)).expect("timeout");
    s.write_all(text.as_bytes()).expect("send");
    let mut out = Vec::new();
    s.read_to_end(&mut out).expect("read");
    String::from_utf8_lossy(&out).into_owned()
}

#[test]
fn forwards_the_requests_of_its_path() {
    let up = upstream();
    let dir = proxied_site(&proxies(&[("/api/", &up.url)]));
    let (server, events) = serve(dir.path(), |_| {});
    let addr = server.local_addrs()[0];
    let port = addr.port();
    assert_eq!(
        events.count(&format!(
            "Proxying {{ from: \"/api/\", to: \"{}\" }}",
            up.url
        )),
        1,
        "{:#?}",
        events.lines()
    );

    let r = get(
        addr,
        "/api/items?x=1&y=%20",
        &[
            ("X-Custom", "a"),
            ("Keep-Alive", "timeout=1"),
            ("X-Forwarded-For", "203.0.113.7"),
        ],
    );
    assert_eq!(r.status, 200, "{}", r.text());
    assert_eq!(r.header("x-upstream"), Some("yes"));
    assert_eq!(r.header("keep-alive"), None, "{:?}", r.headers);
    let echo = r.text().to_ascii_lowercase();
    assert!(
        echo.starts_with("get /api/items?x=1&y=%20 http/1.1\r\n"),
        "{echo}"
    );
    for line in [
        format!("host: localhost:{port}\r\n"),
        "x-custom: a\r\n".to_owned(),
        "x-forwarded-for: 203.0.113.7, 127.0.0.1\r\n".to_owned(),
        format!("x-forwarded-host: localhost:{port}\r\n"),
        "x-forwarded-proto: http\r\n".to_owned(),
    ] {
        assert!(echo.contains(&line), "{line:?} in {echo}");
    }
    assert!(!echo.contains("keep-alive"), "{echo}");
    assert!(!echo.contains("connection: close"), "{echo}");

    // The path itself, and a body.
    assert!(
        get(addr, "/api", &[])
            .text()
            .starts_with("GET /api HTTP/1.1\r\n")
    );
    let body = r#"{"name":"a"}"#;
    let post = raw(
        addr,
        &format!(
            "POST /api/items HTTP/1.1\r\nHost: localhost:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        ),
    );
    assert!(post.starts_with("HTTP/1.1 201 Created\r\n"), "{post}");
    assert!(post.ends_with(&format!("\r\n\r\n{body}")), "{post}");

    // Other paths are the site's, those that only start like the proxy's included.
    assert_eq!(get(addr, "/apiary.txt", &[]).text(), "bees\n");
    assert!(get(addr, "/", &[]).text().contains("home"));
    assert_eq!(get(addr, "/apix/", &[]).status, 404);
    assert_eq!(events.count("error"), 0, "{:#?}", events.lines());
    server.shutdown();
}

#[test]
fn streams_answers_and_upgrades() {
    let up = upstream();
    let dir = proxied_site(&proxies(&[("/api/", &up.url)]));
    let (server, _events) = serve(dir.path(), |_| {});
    let addr = server.local_addrs()[0];

    // The first part arrives while the other server still holds the second.
    let mut s = TcpStream::connect(addr).expect("connect");
    s.set_read_timeout(Some(PATIENCE)).expect("timeout");
    write!(
        s,
        "GET /api/stream HTTP/1.1\r\nHost: localhost:{}\r\nConnection: close\r\n\r\n",
        addr.port()
    )
    .expect("send");
    let mut got = Vec::new();
    let has = |got: &[u8], what: &[u8]| got.windows(what.len()).any(|w| w == what);
    while !has(&got, b"first\n") {
        let mut buf = [0; 1024];
        let n = s
            .read(&mut buf)
            .expect("the first part before the second is sent");
        assert!(n > 0, "closed before the first part");
        got.extend_from_slice(&buf[..n]);
    }
    assert!(!has(&got, b"second"));
    up.release.send(()).expect("release");
    s.read_to_end(&mut got).expect("read");
    let text = String::from_utf8_lossy(&got).to_ascii_lowercase();
    assert!(has(&got, b"second\n"), "{text}");
    assert!(text.contains("content-type: text/event-stream"), "{text}");

    let stream = TcpStream::connect(addr).expect("connect");
    stream.set_read_timeout(Some(PATIENCE)).expect("timeout");
    let (mut ws, response) = tungstenite::client(format!("ws://{addr}/api/ws"), stream)
        .unwrap_or_else(|e| panic!("the WebSocket handshake through the proxy: {e}"));
    assert_eq!(response.status().as_u16(), 101);
    for text in ["ping", "pong"] {
        ws.send(tungstenite::Message::text(text)).expect("send");
        assert_eq!(ws.read().expect("echo").into_text().expect("text"), text);
    }
    let _ = ws.close(None);
    server.shutdown();
}

#[test]
fn flags_replace_the_configuration_s_proxy_and_a_server_down_is_a_bad_gateway() {
    let up = upstream();
    let dead = {
        let l = TcpListener::bind("127.0.0.1:0").expect("bind");
        format!("http://{}", l.local_addr().expect("address"))
    };
    let dir = proxied_site(&proxies(&[("/api/", &up.url), ("/other", &up.url)]));
    // The later flag of a path wins, and the flags win over the configuration.
    let (server, events) = serve(dir.path(), |o| {
        o.proxies = vec![
            "/api/=http://127.0.0.1:1".parse().expect("proxy"),
            format!("/api={dead}").parse().expect("proxy"),
        ];
    });
    let addr = server.local_addrs()[0];

    let r = get(addr, "/api/x?y", &[]);
    assert_eq!(r.status, 502, "{}", r.text());
    assert_eq!(r.header("content-type"), Some("text/plain; charset=utf-8"));
    let message = format!("proxy: GET /api/x?y: {dead}: ");
    assert!(r.text().starts_with(&message), "{}", r.text());
    assert_eq!(
        events.count(&format!("error {message}")),
        1,
        "{:#?}",
        events.lines()
    );
    assert!(
        get(addr, "/other/x", &[])
            .text()
            .starts_with("GET /other/x HTTP/1.1")
    );
    assert_eq!(events.count("Proxying"), 2, "{:#?}", events.lines());
    server.shutdown();
}

#[test]
fn wrong_proxies_are_configuration_errors() {
    let start = |dir: &Path| {
        let recorder: Arc<dyn Reporter> = Arc::new(Recorder::default());
        Server::start(&options(dir), &recorder).map(|_| ())
    };
    for (extra, says) in [
        (
            proxies(&[("api", "http://127.0.0.1:1")]),
            r#"config: server.proxies[0].from: "api" is not a path"#,
        ),
        (
            proxies(&[("/a/", "http://127.0.0.1:1"), ("/b/", "ws://example.org")]),
            "config: server.proxies[1].to: \"ws://example.org\" is not an http:// or https:// URL",
        ),
        (
            proxies(&[("/a/", "http://127.0.0.1:1"), ("/a", "http://127.0.0.1:2")]),
            r#"config: server.proxies[1].from: "/a" is the path of server.proxies[0] too"#,
        ),
        (
            "[[server.proxies]]\nfrom = \"/a/\"\nto = \"http://127.0.0.1:1\"\nport = 1\n"
                .to_owned(),
            "config: server.proxies[0]: unknown field `port`",
        ),
        (
            "[[server.proxies]]\nfrom = \"/a/\"\n".to_owned(),
            "config: server.proxies[0]: missing field `to`",
        ),
    ] {
        let dir = proxied_site(&extra);
        let error = start(dir.path()).expect_err(&extra).to_string();
        assert!(error.starts_with(says), "{extra}: {error}");
    }
    // The other keys of `[server]` (the Go server's) are not read.
    let dir = proxied_site("[[server.headers]]\nfor = \"/**\"\n");
    start(dir.path()).expect("a server without proxies");
}

#[test]
fn a_reloaded_configuration_changes_the_proxies() {
    let up = upstream();
    let dir = proxied_site("");
    let (server, events) = serve(dir.path(), |_| {});
    let addr = server.local_addrs()[0];
    assert_eq!(get(addr, "/v2/x", &[]).status, 404);

    let file = dir.path().join("config.toml");
    write(&file, &config(&proxies(&[("/v2/", "ftp://nowhere")])));
    events.wait_for("config failed: config: server.proxies[0].to: ", 1);
    write(&file, &config(&proxies(&[("/v2/", &up.url)])));
    events.wait_for("built first=false", 1);
    let r = get(addr, "/v2/x", &[]);
    assert!(r.text().starts_with("GET /v2/x HTTP/1.1"), "{}", r.text());
    server.shutdown();
}
