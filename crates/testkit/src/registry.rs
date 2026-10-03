//! A local npm registry for the tests of the package installer (no network): package documents
//! and tarballs served over HTTP on `127.0.0.1`, from packages the test describes.
//!
//! ```ignore
//! let registry = Registry::start(&[Package::new("greet", "1.0.0").file("index.js", "…")]);
//! std::fs::write(project.join(".npmrc"), registry.npmrc())?;
//! ```

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex, PoisonError};

use base64::Engine as _;
use serde_json::{Value, json};
use sha2::{Digest, Sha512};

/// A version of a package: its `package.json` fields and its files.
#[derive(Clone, Debug)]
pub struct Package {
    name: String,
    version: String,
    manifest: serde_json::Map<String, Value>,
    files: Vec<(String, Vec<u8>)>,
}

impl Package {
    /// `name@version` with no files and no dependencies.
    #[must_use]
    pub fn new(name: &str, version: &str) -> Self {
        let mut manifest = serde_json::Map::new();
        manifest.insert("name".to_owned(), json!(name));
        manifest.insert("version".to_owned(), json!(version));
        Self {
            name: name.to_owned(),
            version: version.to_owned(),
            manifest,
            files: Vec::new(),
        }
    }

    /// A `package.json` field (`bin`, `dependencies`, `type`, `main`, …).
    #[must_use]
    pub fn field(mut self, key: &str, value: Value) -> Self {
        self.manifest.insert(key.to_owned(), value);
        self
    }

    /// A file of the package, by its path in the package.
    #[must_use]
    pub fn file(mut self, path: &str, contents: &str) -> Self {
        self.files
            .push((path.to_owned(), contents.as_bytes().to_vec()));
        self
    }

    /// The `.tgz` (every path under `package/`, `package.json` first).
    fn tarball(&self) -> Vec<u8> {
        let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut tar = tar::Builder::new(gz);
        let manifest = serde_json::to_vec_pretty(&self.manifest).expect("manifest");
        let files = std::iter::once(("package.json", manifest.as_slice()))
            .chain(self.files.iter().map(|(p, c)| (p.as_str(), c.as_slice())));
        for (path, contents) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(contents.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            tar.append_data(&mut header, format!("package/{path}"), contents)
                .expect("tar entry");
        }
        tar.into_inner().expect("tar").finish().expect("gzip")
    }
}

/// What the registry serves: path (percent-decoded, no leading `/`) → body.
#[derive(Default)]
struct State {
    files: BTreeMap<String, Vec<u8>>,
    documents: BTreeMap<String, Value>,
    requests: Vec<String>,
}

/// The registry: serves until the test process ends.
pub struct Registry {
    url: String,
    state: Arc<Mutex<State>>,
}

impl Registry {
    /// Serves `packages` (the versions of a name make one package document; the version
    /// published last is `latest`).
    ///
    /// # Panics
    /// When no local port can be bound.
    #[must_use]
    pub fn start(packages: &[Package]) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind 127.0.0.1");
        let url = format!("http://{}/", listener.local_addr().expect("local addr"));
        let state = Arc::new(Mutex::new(State::default()));
        let shared = Arc::clone(&state);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let state = Arc::clone(&shared);
                std::thread::spawn(move || serve(&stream, &state));
            }
        });
        let registry = Self { url, state };
        registry.publish(packages);
        registry
    }

    /// Adds versions to the registry.
    pub fn publish(&self, packages: &[Package]) {
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        for p in packages {
            let tgz = p.tarball();
            let base = p.name.rsplit('/').next().unwrap_or(&p.name);
            let path = format!("{}/-/{base}-{}.tgz", p.name, p.version);
            let integrity = format!(
                "sha512-{}",
                base64::engine::general_purpose::STANDARD.encode(Sha512::digest(&tgz))
            );
            let mut version = Value::Object(p.manifest.clone());
            version["dist"] =
                json!({ "tarball": format!("{}{path}", self.url), "integrity": integrity });
            let doc = state
                .documents
                .entry(p.name.clone())
                .or_insert_with(|| json!({ "name": p.name, "dist-tags": {}, "versions": {} }));
            doc["versions"][&p.version] = version;
            doc["dist-tags"]["latest"] = json!(p.version);
            let doc = serde_json::to_vec(doc).expect("document");
            state.files.insert(p.name.clone(), doc);
            state.files.insert(path, tgz);
        }
    }

    /// `http://127.0.0.1:<port>/`.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// An `.npmrc` that points at the registry.
    #[must_use]
    pub fn npmrc(&self) -> String {
        format!("registry={}\n", self.url)
    }

    /// The paths requested so far (percent-decoded).
    #[must_use]
    pub fn requests(&self) -> Vec<String> {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .requests
            .clone()
    }
}

/// One request per connection (`Connection: close`).
fn serve(stream: &TcpStream, state: &Mutex<State>) {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).map_or(true, |n| n == 0) || header == "\r\n" {
            break;
        }
    }
    let path = line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .trim_start_matches('/')
        .replace("%2f", "/")
        .replace("%2F", "/");
    let body = {
        let mut state = state.lock().unwrap_or_else(PoisonError::into_inner);
        state.requests.push(path.clone());
        state.files.get(&path).cloned()
    };
    let mut out = stream;
    let _ = match body {
        Some(body) => out
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .as_bytes(),
            )
            .and_then(|()| out.write_all(&body)),
        None => out
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"),
    };
}
