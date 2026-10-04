//! The configuration file names (`config.*`; not the Go program's configuration file name) and
//! the themes: finding them (theme lists, themes of themes, `[[module.imports]]`, `_vendor`,
//! replacements) and merging their configuration below the project's with Go's `_merge`
//! rules. Tests named after a Go test port its cases with its expected values; the others
//! check the rules the oracle groups `merge/*` and `themes/*` (in `load.rs`) exercise per case.

use std::path::{Path, PathBuf};

use ssg_base::{Map, PageKind, Value};
use ssg_config::merge::{MergeStrategy, merge_themes};
use ssg_config::theme::path_key;
use ssg_config::{Config, ConfigError, LoadOptions, ThemeMounts, load};
use ssg_testkit::fixture::GO_CONFIG_NAME;

mod file_names;
mod imports;
mod merge;
mod rules;

/// A project in a temporary directory (`site/`), loaded with an optional `--config` list.
struct Project {
    tmp: tempfile::TempDir,
}

impl Project {
    fn new(files: &[(&str, &str)]) -> Self {
        let tmp = tempfile::tempdir().expect("temp dir");
        for (name, text) in files {
            let p = tmp.path().join("site").join(name);
            std::fs::create_dir_all(p.parent().expect("parent")).expect("dir");
            std::fs::write(p, text).expect("write");
        }
        Self { tmp }
    }

    fn dir(&self) -> PathBuf {
        self.tmp.path().join("site")
    }

    fn load_with(&self, config_files: &[&str]) -> Result<Config, ConfigError> {
        load(&LoadOptions {
            source: self.dir(),
            config_files: config_files.iter().map(PathBuf::from).collect(),
            env: vec![(
                "XDG_CACHE_HOME".into(),
                self.tmp.path().join("xdg").to_string_lossy().into_owned(),
            )],
            ..LoadOptions::default()
        })
    }

    fn load(&self) -> Result<Config, ConfigError> {
        self.load_with(&[])
    }

    fn ok(&self) -> Config {
        self.load().unwrap_or_else(|e| panic!("{e}"))
    }

    /// `path` relative to the project directory.
    fn rel(&self, path: &Path) -> String {
        path.strip_prefix(self.dir())
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned()
    }
}

/// A TOML document as a configuration tree.
fn toml(s: &str) -> Value {
    Value::from_toml_str(s).expect("toml")
}

/// `params` of the default language as a TOML-comparable tree.
fn params(c: &Config) -> Value {
    Value::map(c.default_site().params.as_map().clone())
}

fn warnings(c: &Config, id: &str) -> Vec<String> {
    c.diagnostics
        .iter()
        .filter(|d| d.id.as_deref() == Some(id))
        .map(ToString::to_string)
        .collect()
}

// ───────────── configuration file names ─────────────

// ───────────── Go ports ─────────────

// ───────────── the merge rules ─────────────

// ───────────── finding themes ─────────────
