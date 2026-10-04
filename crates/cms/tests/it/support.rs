//! A project in a temporary directory, its configuration, and a sink that keeps files in memory.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use ssg_base::Sink;
use ssg_base::paths::OutputPath;
use ssg_config::{CliOverrides, Config, LoadOptions};

/// Writes `files` (`path`, text) under `dir`.
pub fn write_files(dir: &Path, files: &[(&str, &str)]) {
    for (path, text) in files {
        let p = dir.join(path);
        std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
        std::fs::write(p, text).expect("write");
    }
}

/// The configuration of the project in `dir` for `environment`.
pub fn load(dir: &Path, environment: &str) -> Config {
    ssg_config::load(&LoadOptions {
        source: dir.to_path_buf(),
        config_files: Vec::new(),
        cli: CliOverrides {
            environment: Some(environment.to_owned()),
            ..CliOverrides::default()
        },
        env: Vec::new(),
    })
    .expect("config")
}

#[derive(Default)]
pub struct TestSink(pub Mutex<BTreeMap<String, Vec<u8>>>);

impl TestSink {
    pub fn text(&self, path: &str) -> String {
        let files = self.0.lock().expect("lock");
        String::from_utf8(
            files
                .get(path)
                .unwrap_or_else(|| panic!("no {path}"))
                .clone(),
        )
        .expect("utf-8")
    }

    pub fn paths(&self) -> Vec<String> {
        self.0.lock().expect("lock").keys().cloned().collect()
    }
}

impl Sink for TestSink {
    fn write(&self, path: &OutputPath, bytes: &[u8]) -> std::io::Result<()> {
        self.0
            .lock()
            .expect("lock")
            .insert(path.relative().to_owned(), bytes.to_vec());
        Ok(())
    }

    fn exists(&self, path: &OutputPath) -> bool {
        self.0.lock().expect("lock").contains_key(path.relative())
    }

    fn read(&self, path: &OutputPath) -> std::io::Result<Vec<u8>> {
        self.0
            .lock()
            .expect("lock")
            .get(path.relative())
            .cloned()
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))
    }
}

/// A `[cms]` table for `config.toml`, with `extra` lines appended.
pub fn cms_toml(extra: &str) -> String {
    format!(
        r#"
[cms]
[cms.git]
repo = "owner/site"
[cms.login]
team = "team"
aud = "aud-1"
[cms.roles.owner]
edit = ["**"]
publish = true
{extra}
"#
    )
}

/// The settings at the top of a published `_worker.js` (`const __CMS_SETTINGS__ = {…};`).
pub fn settings_of(worker: &str) -> serde_json::Value {
    let start = worker.find(SETTINGS).expect("settings") + SETTINGS.len();
    let end = worker[start..]
        .find(";\n\nconst __CMS_INDEX__")
        .expect("end")
        + start;
    serde_json::from_str(&worker[start..end]).expect("settings json")
}

const SETTINGS: &str = "const __CMS_SETTINGS__ = ";
const INDEX: &str = "const __CMS_INDEX__ = ";

/// The content index of a published `_worker.js` (`const __CMS_INDEX__ = "<json>";`).
pub fn index_of(worker: &str) -> serde_json::Value {
    let start = worker.find(INDEX).expect("index") + INDEX.len();
    let end = worker[start..].find(";\n").expect("end") + start;
    let text: String = serde_json::from_str(&worker[start..end]).expect("a JSON string");
    serde_json::from_str(&text).expect("index json")
}
