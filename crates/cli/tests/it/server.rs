//! `server` through the binary: it starts, serves and reports; its flags and their usage
//! errors. What the server does on changes is tested in `ssg-serve`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use crate::{binary, site_from, stderr};

const SITE: &str = "-- config.toml --\nbaseURL = \"https://example.org/\"\ntitle = \"Srv\"\n\
disableKinds = [\"taxonomy\", \"term\", \"sitemap\", \"rss\", \"robotsTXT\"]\n\
-- layouts/home.html --\n<html><head></head><body>{{ site.title }} {{ build.environment }}</body></html>\n\
-- content/_index.md --\n---\ntitle: Home\n---\n";

/// The running binary and the lines of its standard output.
pub(crate) struct Running {
    child: Child,
    lines: mpsc::Receiver<String>,
}

impl Running {
    pub(crate) fn start(dir: &std::path::Path, args: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_fugo"))
            .current_dir(dir)
            .args(args)
            .env_clear()
            .env("HOME", dir)
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("run the binary");
        let out = child.stdout.take().expect("stdout");
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(out).lines() {
                let Ok(line) = line else { return };
                if tx.send(line).is_err() {
                    return;
                }
            }
        });
        Self { child, lines }
    }

    /// Output lines up to one starting with `prefix` (which is included).
    pub(crate) fn until(&self, prefix: &str) -> Vec<String> {
        let mut seen = Vec::new();
        loop {
            match self.lines.recv_timeout(Duration::from_secs(60)) {
                Ok(line) => {
                    let done = line.starts_with(prefix);
                    seen.push(line);
                    if done {
                        return seen;
                    }
                }
                Err(e) => panic!("no {prefix:?} line ({e}); output: {seen:#?}"),
            }
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn get(port: u16, path: &str) -> String {
    let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    s.set_read_timeout(Some(Duration::from_secs(30)))
        .expect("timeout");
    write!(
        s,
        "GET {path} HTTP/1.1\r\nHost: localhost:{port}\r\nConnection: close\r\n\r\n"
    )
    .expect("send");
    let mut out = String::new();
    s.read_to_string(&mut out).expect("read");
    out
}

/// The port of `Web Server is available at http://localhost:<port>/ …`.
fn port_of(line: &str) -> u16 {
    line.split("localhost:")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .and_then(|p| p.parse().ok())
        .unwrap_or_else(|| panic!("no port in {line:?}"))
}

#[test]
fn server_starts_and_serves() {
    let s = site_from(SITE);
    // `serve` is the alias; the camelCase spellings are accepted.
    let run = Running::start(
        s.path(),
        &["serve", "-p", "0", "--noHTTPCache", "--appendPort=true"],
    );
    let lines = run.until("Web Server is available at ");
    let text = lines.join("\n");
    assert!(text.contains("Environment: \"development\""), "{text}");
    assert!(text.contains("Serving pages from memory"), "{text}");
    assert!(text.contains("Watching for changes in "), "{text}");
    assert!(text.contains("Built in "), "{text}");
    let port = port_of(lines.last().expect("line"));
    let home = get(port, "/");
    assert!(home.starts_with("HTTP/1.1 200"), "{home}");
    assert!(home.contains("Srv development"), "{home}");
    assert!(home.contains("livereload.js"), "{home}");
    assert!(
        home.to_ascii_lowercase()
            .contains("cache-control: no-store"),
        "{home}"
    );
    assert!(!s.path().join("public").exists(), "memory only");
}

#[test]
fn server_renders_to_disk_without_live_reload() {
    let s = site_from(SITE);
    let run = Running::start(
        s.path(),
        &[
            "server",
            "--port=0",
            "--render-to-disk",
            "--disable-live-reload",
            "--watch=false",
            "-e",
            "staging",
        ],
    );
    let lines = run.until("Web Server is available at ");
    assert!(
        lines.iter().any(|l| l == "Serving pages from disk"),
        "{lines:#?}"
    );
    assert!(
        !lines.iter().any(|l| l.starts_with("Watching")),
        "{lines:#?}"
    );
    let port = port_of(lines.last().expect("line"));
    let home = std::fs::read_to_string(s.path().join("public/index.html")).expect("public");
    assert!(
        home.contains("Srv staging") && !home.contains("livereload"),
        "{home}"
    );
    assert!(get(port, "/").ends_with(&home));
}

#[test]
fn server_start_errors() {
    // A first build that fails ends the server with the build's report.
    let s = site_from(&SITE.replace("{{ site.title }}", "{{ site.title "));
    let o = binary(s.path(), &["server", "-p", "0"], &[]);
    assert_eq!(o.status.code(), Some(1), "{}", stderr(&o));
    assert!(stderr(&o).contains("ERROR build failed"), "{}", stderr(&o));
    assert!(stderr(&o).contains("home.html"), "{}", stderr(&o));

    for bad in [
        &["server", "-d", "out"][..],
        &["server", "--render-to-disk", "-M"],
        &["server", "--poll", "soon"],
        &["server", "--port", "http"],
        &["server", "--watch", "maybe"],
    ] {
        let o = binary(s.path(), bad, &[]);
        assert_eq!(o.status.code(), Some(2), "{bad:?}: {}", stderr(&o));
        assert!(stderr(&o).contains("error"), "{bad:?}: {}", stderr(&o));
    }
    let o = binary(s.path(), &["server", "--help"], &[]);
    let help = crate::stdout(&o);
    for flag in [
        "--port",
        "--bind",
        "--append-port",
        "--disable-live-reload",
        "--live-reload-port",
        "--navigate-to-changed",
        "--render-to-disk",
        "--no-http-cache",
        "--poll",
        "--watch",
        "--build-drafts",
        "--base-url",
    ] {
        assert!(help.contains(flag), "{flag}: {help}");
    }
}
