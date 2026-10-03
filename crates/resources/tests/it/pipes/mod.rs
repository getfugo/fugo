//! The pipes (T42): shared helpers. A test project is a copy of a fixture site in a temporary
//! directory, with a store whose [`TransformEnv`] the test controls.

mod jsbuild;
mod minify;
mod postprocess;
mod template;
mod tocss;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value as J;
use ssg_base::{Idx as _, LangIdx, ResourceId};
use ssg_resources::{ResourceStore, StoreConfig, TransformEnv};
use ssg_vfs::Vfs;

use crate::support::config;

/// A copy of a fixture site (or a site used in place) and its store.
pub struct Project {
    pub dir: PathBuf,
    pub store: ResourceStore,
    _tmp: tempfile::TempDir,
}

impl Project {
    pub fn lang(&self) -> LangIdx {
        LangIdx::from_index(0)
    }

    /// The asset at `path`.
    pub fn asset(&self, path: &str) -> ResourceId {
        self.store
            .get_asset(self.lang(), path)
            .unwrap()
            .unwrap_or_else(|| panic!("no asset {path}"))
    }

    /// `text` with the oracle's `$SITE` replaced by the project directory.
    pub fn site(&self, text: &str) -> String {
        text.replace("$SITE", &self.dir.display().to_string())
    }
}

/// The resource-transformers fixture directory `name` (`t16site`).
pub fn fixture_site(name: &str) -> PathBuf {
    ssg_testkit::fixture::testdata("oracle/resource-transformers").join(name)
}

fn copy_tree(from: &Path, to: &Path) {
    copy_tree_except(from, to, &[]);
}

fn copy_tree_except(from: &Path, to: &Path, skip: &[&str]) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let name = e.file_name();
        if skip.iter().any(|s| name == *s) {
            continue;
        }
        // The fixture's node_modules is stored as _node_modules (it is gitignored otherwise).
        let target = if name == "_node_modules" {
            "node_modules".into()
        } else {
            name
        };
        if e.file_type().unwrap().is_dir() {
            copy_tree_except(&e.path(), &to.join(target), &[]);
        } else {
            std::fs::copy(e.path(), to.join(target)).unwrap();
        }
    }
}

/// A copy of `src` as `<tmp>/site` (the oracles print the working directory's name), with
/// `edit` applied to its environment.
pub fn project(src: &Path, edit: impl FnOnce(&mut TransformEnv)) -> Project {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().canonicalize().unwrap().join("site");
    copy_tree(src, &dir);
    in_dir(tmp, dir, edit)
}

/// A copy of `src` as `<tmp>/site` without its top-level entries named in `skip` (a local
/// build's `public/`, `node_modules/`), with `edit` applied to its environment.
pub fn project_except(src: &Path, skip: &[&str], edit: impl FnOnce(&mut TransformEnv)) -> Project {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().canonicalize().unwrap().join("site");
    copy_tree_except(src, &dir, skip);
    in_dir(tmp, dir, edit)
}

/// A small site in a temporary directory: `config.toml` and the given files.
pub fn mini_site(files: &[(&str, &str)]) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    for (name, body) in files {
        let path = tmp.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }
    tmp
}

fn in_dir(tmp: tempfile::TempDir, dir: PathBuf, edit: impl FnOnce(&mut TransformEnv)) -> Project {
    // T00's fixture conversion re-serialised the JSON sources compactly; the oracles read the
    // original text (it shows in source maps and bundles).
    for (file, text) in [
        (
            "assets/js/data.json",
            "{\"items\": [1, 2, 3], \"name\": \"data\"}\n",
        ),
        ("assets/js/data/config.json", "{\"mode\": \"test\"}\n"),
    ] {
        if dir.join(file).is_file() {
            std::fs::write(dir.join(file), text).unwrap();
        }
    }
    let home = tmp.path().canonicalize().unwrap().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let cfg = config(&dir, &home);
    let mut env = TransformEnv::from_config(&cfg);
    edit(&mut env);
    let store = ResourceStore::new(StoreConfig {
        transforms: Arc::new(env),
        ..StoreConfig::from_config(&cfg, Some(Arc::new(Vfs::new(&cfg).unwrap())), None)
    });
    Project {
        dir,
        store,
        _tmp: tmp,
    }
}

/// A fixture case's steps run on the store: `get`, `concat` (of earlier cases), `tocss`, `js`,
/// `minify`, `fingerprint`. The error is the failing step's.
pub fn run_steps(
    p: &Project,
    case: &J,
    done: &BTreeMap<String, ResourceId>,
) -> Result<ResourceId, (usize, String)> {
    use ssg_resources::pipes::{JsBuildSpec, ToCssOptions};
    use ssg_resources::{CallSite, HashAlgo, Transform};
    let s = &p.store;
    let mut id = None;
    for (i, step) in case["steps"].as_array().unwrap().iter().enumerate() {
        let err = |e: &dyn std::fmt::Display| (i, e.to_string());
        let t = match step["op"].as_str().unwrap() {
            "get" => {
                let path = step["path"].as_str().unwrap();
                id = Some(p.asset(path));
                continue;
            }
            "concat" => {
                let refs: Vec<ResourceId> = step["refs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|r| done[r.as_str().unwrap()])
                    .collect();
                let target = step["target"].as_str().unwrap();
                id = Some(
                    s.concat(target, &refs, &CallSite::in_lang(p.lang()))
                        .map_err(|e| err(&e))?,
                );
                continue;
            }
            "tocss" => {
                Transform::ToCss(ToCssOptions::from_json(&step["opts"]).map_err(|e| err(&e))?)
            }
            "js" => Transform::JsBuild(Box::new(
                JsBuildSpec::from_json(&step["opts"]).map_err(|e| err(&e))?,
            )),
            "minify" => Transform::Minify,
            "fingerprint" => Transform::Fingerprint(HashAlgo::Sha256),
            op => panic!("unknown op {op}"),
        };
        let next = s.transform(id.unwrap(), t).map_err(|e| err(&e))?;
        s.realize(next).map_err(|e| err(&e))?;
        id = Some(next);
    }
    Ok(id.unwrap())
}

/// The oracle's `data` document (`"null"` or a JSON object) as a value.
pub fn data(v: &J) -> J {
    crate::support::json_doc(v)
}
