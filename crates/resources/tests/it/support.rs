//! Shared helpers: projects and stores, a memory sink, and the record of a resource the Go
//! oracles print.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::{Value as J, json};
use sha2::{Digest, Sha256};
use ssg_base::Sink;
use ssg_base::paths::OutputPath;
use ssg_config::{Config, LoadOptions, load};
use ssg_resources::{Resource, ResourceStore, StoreConfig};
use ssg_vfs::Vfs;

/// The Go oracles' synthetic site (`testdata/oracle/resources/site`).
pub fn synth_dir() -> PathBuf {
    ssg_testkit::fixture::testdata("oracle/resources/site")
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let dest = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_dir(&e.path(), &dest);
        } else {
            std::fs::copy(e.path(), dest).unwrap();
        }
    }
}

/// A copy of the synthetic site in `tmp` whose text files hold the bytes the Go oracle read:
/// the fixture conversion (T00) re-serialised the site's JSON files (`assets/data/x.json`,
/// `content/blog/bundle1/data.json`), so they are restored from the recorded contents.
pub fn synth_site(tmp: &Path) -> PathBuf {
    rule("resources", "fixture_json");
    let dir = tmp.join("site");
    copy_dir(&synth_dir(), &dir);
    let fx: J = ssg_testkit::fixture::oracle("oracle/resources/resources/synth.json.gz");
    for r in fx["records"].as_array().unwrap() {
        let (Some(file), Some(content)) = (r["rd"]["file"].as_str(), r["base"]["content"].as_str())
        else {
            continue;
        };
        let rel = file.trim_start_matches("crates/nh-resources/tests/fixtures/site/");
        let path = dir.join(rel);
        if std::fs::read(&path).unwrap() != content.as_bytes() {
            std::fs::write(&path, content).unwrap();
        }
    }
    dir
}

/// The repository root.
pub use ssg_testkit::fixture::repo_dir;

/// The Go program's configuration file name, which a frozen fixture of the Go build has.
const GO_CONFIG_FILE: &str = "hugo.toml";

/// The configuration of the project at `dir`, with a private home (cache) directory. A frozen
/// fixture of the Go build read in place (the legacy docs site, `testdata/legacy-docs`) has the
/// Go program's configuration file, which this port does not look for: it is named explicitly,
/// as `--config` with that file would.
pub fn config(dir: &Path, home: &Path) -> Config {
    let go_site = !dir.join("config.toml").exists() && dir.join(GO_CONFIG_FILE).exists();
    load(&LoadOptions {
        source: dir.to_owned(),
        config_files: if go_site {
            vec![GO_CONFIG_FILE.into()]
        } else {
            Vec::new()
        },
        env: vec![("HOME".into(), home.to_str().unwrap().into())],
        ..LoadOptions::default()
    })
    .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
}

/// A store over the project at `dir` (assets from its union view).
pub fn store(dir: &Path, home: &Path) -> ResourceStore {
    let cfg = config(dir, home);
    let vfs = Arc::new(Vfs::new(&cfg).unwrap());
    ResourceStore::new(StoreConfig::from_config(&cfg, Some(vfs), None))
}

/// A sink that keeps what is written.
#[derive(Default)]
pub struct MemSink(pub Mutex<BTreeMap<String, Vec<u8>>>);

impl Sink for MemSink {
    fn write(&self, path: &OutputPath, bytes: &[u8]) -> std::io::Result<()> {
        self.0
            .lock()
            .unwrap()
            .insert(path.relative().to_owned(), bytes.to_vec());
        Ok(())
    }

    fn exists(&self, path: &OutputPath) -> bool {
        self.0.lock().unwrap().contains_key(path.relative())
    }

    fn read(&self, path: &OutputPath) -> std::io::Result<Vec<u8>> {
        self.0
            .lock()
            .unwrap()
            .get(path.relative())
            .cloned()
            .ok_or_else(|| std::io::ErrorKind::NotFound.into())
    }
}

impl MemSink {
    /// `path -> "len:sha256"`, as the oracles print published files.
    pub fn files(&self) -> J {
        J::Object(
            self.0
                .lock()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), json!(format!("{}:{}", v.len(), sha(v)))))
                .collect(),
        )
    }
}

pub fn sha(b: &[u8]) -> String {
    Sha256::digest(b)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A JSON document string of the oracle (`"{}"`, `"null"`) as a value; `null` and `{}` are
/// one thing (Go's nil and empty maps).
pub fn json_doc(s: &J) -> J {
    match s.as_str().map(serde_json::from_str::<J>) {
        Some(Ok(J::Null)) => json!({}),
        Some(Ok(v)) => v,
        _ => s.clone(),
    }
}

/// The oracle's record of a resource (`rsupport.Rec`), for the fields this crate models; the
/// content as text or as a hash, as `want` has it (none without `want`).
pub fn rec(store: &ResourceStore, r: &Resource, want: Option<&J>) -> J {
    let mut o = serde_json::Map::new();
    o.insert("name".into(), json!(r.name));
    o.insert("title".into(), json!(r.title));
    o.insert("nameNormalized".into(), json!(r.name_normalized));
    o.insert("mediaType".into(), json!(r.media_type_string()));
    o.insert("resourceType".into(), json!(r.resource_type()));
    o.insert("data".into(), serde_json::to_value(&r.data).unwrap());
    o.insert(
        "params".into(),
        serde_json::to_value(r.params.as_map()).unwrap(),
    );
    if let Some(w) = want {
        let b = store.content(r.id).unwrap();
        if w.get("content").is_some() {
            o.insert("content".into(), json!(String::from_utf8_lossy(&b)));
        } else {
            o.insert("contentSha".into(), json!(sha(&b)));
        }
        o.insert("contentLen".into(), json!(b.len()));
    }
    o.insert("relPermalink".into(), json!(r.rel_permalink));
    o.insert("permalink".into(), json!(r.permalink.as_str()));
    J::Object(o)
}

/// The oracle record's fields that [`rec`] produces, with its JSON document strings decoded.
pub fn want(w: &J, skip: &[&str]) -> J {
    let mut o = serde_json::Map::new();
    for k in [
        "name",
        "title",
        "nameNormalized",
        "mediaType",
        "resourceType",
        "data",
        "params",
        "content",
        "contentSha",
        "contentLen",
        "relPermalink",
        "permalink",
    ] {
        if skip.contains(&k) {
            continue;
        }
        if let Some(v) = w.get(k) {
            let v = if matches!(k, "data" | "params") {
                json_doc(v)
            } else {
                v.clone()
            };
            o.insert(k.into(), v);
        }
    }
    J::Object(o)
}

/// The differences between `want` and `got` over `want`'s keys.
pub fn diff(what: &str, want: &J, got: &J) -> Vec<String> {
    let mut out = Vec::new();
    for (k, w) in want.as_object().unwrap() {
        let g = got.get(k).unwrap_or(&J::Null);
        if g != w {
            out.push(format!("{what}.{k}: want {w}, got {g}"));
        }
    }
    out
}

/// The reviewed differences (`crates/resources/expected_diffs.toml`).
pub fn expected_diffs() -> toml::Table {
    toml::from_str(include_str!("../../expected_diffs.toml")).unwrap()
}

/// Asserts that `[section] key` is a reviewed difference (a test relies on it).
pub fn rule(section: &str, key: &str) {
    assert!(
        expected_diffs()
            .get(section)
            .and_then(|s| s.get(key))
            .is_some(),
        "expected_diffs.toml has no [{section}] {key}"
    );
}
