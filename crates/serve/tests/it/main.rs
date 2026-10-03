//! Integration tests of `ssg-serve` (the crate's single test binary, REWRITE_PLAN.md §2.2):
//! servers on free ports over temporary copies of small sites and of the testsite, driven with
//! a plain HTTP/1.1 client and a LiveReload WebSocket client.

mod serve;
mod testsite;

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use ssg_build::BuildRequest;
use ssg_config::CliOverrides;
use ssg_serve::{Event, Port, Reporter, ServeOptions, Server};
use ssg_testkit::txtar::Archive;

/// How long anything may take before a test gives up (builds of a debug binary on a busy
/// machine included); the edit → reload times themselves are printed.
pub const PATIENCE: Duration = Duration::from_secs(20);

/// The server's events, one line each.
#[derive(Debug, Default)]
pub struct Recorder {
    lines: Mutex<Vec<String>>,
}

impl Reporter for Recorder {
    fn report(&self, event: &Event<'_>) {
        let line = match event {
            Event::Built { first, report, .. } => {
                format!("built first={first} outputs={}", report.outputs)
            }
            Event::BuildFailed { error } => format!("build failed: {error}"),
            Event::StaticSynced { files, .. } => format!("static synced {files}"),
            Event::ChangeDetected { kind, paths, .. } => format!("change {kind:?} {paths:?}"),
            Event::Reload { command } => format!("reload {command}"),
            Event::ConfigFailed { error } => format!("config failed: {error}"),
            Event::Error { message } => format!("error {message}"),
            Event::Listening { url, .. } => format!("listening {url}"),
            other => format!("{other:?}"),
        };
        self.lines
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(line);
    }
}

impl Recorder {
    pub fn lines(&self) -> Vec<String> {
        self.lines
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The events starting with `prefix`.
    pub fn count(&self, prefix: &str) -> usize {
        self.lines()
            .iter()
            .filter(|l| l.starts_with(prefix))
            .count()
    }

    /// Waits until there are `n` events starting with `prefix`.
    pub fn wait_for(&self, prefix: &str, n: usize) {
        let deadline = Instant::now() + PATIENCE;
        while self.count(prefix) < n {
            assert!(
                Instant::now() < deadline,
                "no {n}×{prefix:?} within {PATIENCE:?}: {:#?}",
                self.lines()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

/// A site from txtar text in a new temporary directory.
pub fn site(text: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    Archive::parse(text)
        .write_to(dir.path())
        .expect("write the site");
    dir
}

/// Serves `dir` on a free port of 127.0.0.1 in the development environment.
pub fn serve(dir: &Path, tweak: impl FnOnce(&mut ServeOptions)) -> (Server, Arc<Recorder>) {
    let mut o = ServeOptions {
        build: BuildRequest {
            source: dir.to_owned(),
            cli: CliOverrides {
                environment: Some("development".to_owned()),
                ..CliOverrides::default()
            },
            clock: Some("2026-01-01T00:00:00Z".parse().expect("clock")),
            ..BuildRequest::default()
        },
        port: Port::Exact(0),
        ..ServeOptions::default()
    };
    tweak(&mut o);
    let recorder = Arc::new(Recorder::default());
    let server = Server::start(&o, &(Arc::clone(&recorder) as Arc<dyn Reporter>))
        .unwrap_or_else(|e| panic!("server: {e}"));
    (server, recorder)
}

/// An HTTP response.
#[derive(Debug)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Reply {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// `GET path` with `headers`.
pub fn get(addr: SocketAddr, path: &str, headers: &[(&str, &str)]) -> Reply {
    request(addr, "GET", path, headers)
}

/// One HTTP/1.1 request on a new connection (`Connection: close`).
pub fn request(addr: SocketAddr, method: &str, path: &str, headers: &[(&str, &str)]) -> Reply {
    let mut s = TcpStream::connect(addr).expect("connect");
    s.set_read_timeout(Some(PATIENCE)).expect("timeout");
    let mut req = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost:{}\r\nConnection: close\r\n",
        addr.port()
    );
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    s.write_all(req.as_bytes()).expect("send");
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).expect("read");
    let split = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .expect("end of the head");
    let head = String::from_utf8_lossy(&raw[..split]).into_owned();
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|l| l.split(' ').nth(1))
        .and_then(|c| c.parse().ok())
        .expect("status");
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .collect();
    let mut body = raw[split + 4..].to_vec();
    let chunked = headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("transfer-encoding") && v.contains("chunked"));
    if chunked {
        body = dechunk(&body);
    }
    Reply {
        status,
        headers,
        body,
    }
}

fn dechunk(mut raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let Some(eol) = raw.windows(2).position(|w| w == b"\r\n") else {
            return out;
        };
        let size = usize::from_str_radix(String::from_utf8_lossy(&raw[..eol]).trim(), 16)
            .expect("chunk size");
        if size == 0 {
            return out;
        }
        out.extend_from_slice(&raw[eol + 2..eol + 2 + size]);
        raw = &raw[eol + 2 + size + 2..];
    }
}

/// A browser's LiveReload connection.
pub struct LiveReload {
    ws: tungstenite::WebSocket<TcpStream>,
}

impl LiveReload {
    /// Connects to `path` and completes the handshake (`hello` both ways).
    pub fn connect(addr: SocketAddr, path: &str) -> Self {
        let stream = TcpStream::connect(addr).expect("connect");
        stream.set_read_timeout(Some(PATIENCE)).expect("timeout");
        let (ws, response) = tungstenite::client(format!("ws://{addr}{path}"), stream)
            .unwrap_or_else(|e| panic!("WebSocket handshake: {e}"));
        assert_eq!(response.status().as_u16(), 101);
        ws.get_ref()
            .set_read_timeout(Some(Duration::from_millis(50)))
            .expect("timeout");
        let mut lr = Self { ws };
        lr.ws
            .send(tungstenite::Message::text(
                r#"{"command":"hello","protocols":["http://livereload.com/protocols/official-6","http://livereload.com/protocols/official-7"],"ver":"4.0.2"}"#,
            ))
            .expect("hello");
        let hello = lr.next(PATIENCE).expect("the server's hello");
        assert!(
            hello.contains(r#""command":"hello""#)
                && hello.contains("http://livereload.com/protocols/official-7"),
            "{hello}"
        );
        lr
    }

    /// The next command within `within`.
    pub fn next(&mut self, within: Duration) -> Option<String> {
        let deadline = Instant::now() + within;
        loop {
            match self.ws.read() {
                Ok(tungstenite::Message::Text(t)) => return Some(t.as_str().to_owned()),
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    if Instant::now() >= deadline {
                        return None;
                    }
                }
                Err(e) => panic!("LiveReload connection: {e}"),
            }
        }
    }

    /// The next command, which must come within [`PATIENCE`].
    pub fn expect(&mut self) -> String {
        self.next(PATIENCE)
            .unwrap_or_else(|| panic!("no LiveReload command within {PATIENCE:?}"))
    }
}

/// Writes `text` to `path` (creating directories).
pub fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdir");
    }
    std::fs::write(path, text).expect("write");
}
