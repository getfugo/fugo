//! `js.Build` against the `jsbuild` Go oracle (`testdata/oracle/resource-transformers/jsbuild`):
//! the Go implementation's `js.Build` (esbuild 0.25.6 linked in) on the `t16site` fixture site
//! (62 cases) and on the docs site's scripts (6 cases).
//!
//! A case gets an asset (or concatenates earlier cases' results), then optionally runs
//! `js.Build` and fingerprints. This port bundles with rolldown, so the bytes differ; what must
//! match is what the scripts do. Each built script and the oracle's run under node with
//! recording stand-ins for the browser (`run::trace`), and the two traces must be equal. The
//! modules bundled (esbuild's `// <module>` comments, rolldown's `//#region` ones) must be among
//! the oracle's files (rolldown leaves out modules it inlined). Errors must be at the same position; errors whose wording this port keeps from
//! esbuild (unresolved imports, the es5 target) must have the same text too.
//!
//! External/linked source maps must name every bundled file by URL, with the file's contents,
//! and include the files the Go implementation's map names.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value as Json;
use ssg_jsbuild::{
    JsBuildError, JsBuildOptions, JsBuildOutput, JsBuilder, MountedDirs, OptionsError, Source,
};
use ssg_testkit::fixture::oracle;

mod check;
use check::*;

/// The namespace prefix of bundled modules in the recorded Go output; this port writes
/// `ns-ssg-`.
const GO_NAMESPACE: &str = "ns-hugo-";

/// What a case produced.
enum Outcome {
    Built(JsBuildOutput),
    /// No `js` step: the asset itself.
    Raw(Vec<u8>),
    Options(OptionsError),
    Build(JsBuildError),
}

struct Site {
    root: PathBuf,
    assets: MountedDirs,
}

/// The fixture's site: t16site is copied (its `_node_modules` becomes `node_modules`, mounted
/// at `assets/vendor` as its config.toml says); of the docs site only `assets/` is copied, so a
/// local `docs/node_modules` (gitignored) cannot resolve what the oracle could not.
fn site(fixture_dir: &str) -> Site {
    if fixture_dir == "docs" {
        let root = crate::scratch("jsbuild-docs").join("site");
        copy_tree(
            &crate::repo_root().join("testdata/legacy-docs/assets"),
            &root.join("assets"),
        );
        let assets = MountedDirs::new().mount(root.join("assets"), "");
        return Site { root, assets };
    }
    let name = Path::new(fixture_dir).file_name().unwrap();
    let src = ssg_testkit::fixture::testdata("oracle/resource-transformers").join(name);
    let root = crate::scratch("jsbuild-site")
        .canonicalize()
        .unwrap()
        .join("site");
    copy_tree(&src, &root);
    // T00's fixture conversion re-serialized the site's JSON sources compactly; the oracle ran
    // on the original text, which ends up in source maps. Restore it.
    for (file, text) in [
        (
            "assets/js/data.json",
            "{\"items\": [1, 2, 3], \"name\": \"data\"}\n",
        ),
        ("assets/js/data/config.json", "{\"mode\": \"test\"}\n"),
    ] {
        std::fs::write(root.join(file), text).unwrap();
    }
    let assets = MountedDirs::new()
        .mount(root.join("assets"), "")
        .mount(root.join("node_modules"), "vendor");
    Site { root, assets }
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let name = e.file_name();
        let target_name = if name == "_node_modules" {
            "node_modules".into()
        } else {
            name
        };
        if e.file_type().unwrap().is_dir() {
            copy_tree(&e.path(), &to.join(target_name));
        } else {
            std::fs::copy(e.path(), to.join(target_name)).unwrap();
        }
    }
}

fn media_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or_default() {
        "js" => "text/javascript",
        "ts" => "text/typescript",
        "tsx" => "text/tsx",
        "jsx" => "text/jsx",
        "json" => "application/json",
        _ => "application/octet-stream",
    }
}

/// What a case produced, and the script it built.
fn run_case(
    builder: &JsBuilder,
    site: &Site,
    case: &Json,
    done: &BTreeMap<String, Vec<u8>>,
) -> (Outcome, Vec<u8>) {
    let steps = case["steps"].as_array().unwrap();
    let (path, contents) = match steps[0]["op"].as_str().unwrap() {
        "get" => {
            let path = steps[0]["path"].as_str().unwrap().to_owned();
            let file = site.root.join("assets").join(&path);
            let file = if file.exists() {
                file
            } else {
                site.root
                    .join("node_modules")
                    .join(path.strip_prefix("vendor/").unwrap())
            };
            (path, std::fs::read(file).unwrap())
        }
        "concat" => {
            // Go separates concatenated scripts with "\n;\n".
            let parts: Vec<&[u8]> = steps[0]["refs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| done[r.as_str().unwrap()].as_slice())
                .collect();
            (
                steps[0]["target"].as_str().unwrap().to_owned(),
                parts.join(&b"\n;\n"[..]),
            )
        }
        op => panic!("unknown op {op}"),
    };
    let Some(js) = steps.iter().find(|s| s["op"] == "js") else {
        return (Outcome::Raw(contents.clone()), contents);
    };
    let opts = match JsBuildOptions::from_json(&js["opts"]) {
        Ok(o) => o,
        Err(e) => return (Outcome::Options(e), contents),
    };
    let source = Source {
        path: &path,
        media_type: media_type(&path),
        contents: &contents,
    };
    let outcome = match builder.build(Arc::new(site.assets.clone()), &source, &opts) {
        Ok(out) => Outcome::Built(out),
        Err(e) => Outcome::Build(e),
    };
    (outcome, contents)
}

/// The files a bundle is made of, relative to the site (`entry` for the entry script, which
/// has any of the names in `entry`), from esbuild's `// <module>` comments or rolldown's
/// `//#region <module>` ones.
fn modules(code: &str, site: &str, entry: &[String]) -> BTreeSet<String> {
    code.lines()
        .filter_map(|l| {
            let l = l.trim_start();
            if let Some(m) = l.strip_prefix("//#region ") {
                return Some(m.trim().to_owned());
            }
            let m = l.strip_prefix("// ")?;
            (m == "<stdin>"
                || m.starts_with("ns-ssg")
                || m.starts_with("node_modules/")
                || m.starts_with("assets/")
                || m.starts_with("../"))
            .then(|| m.to_owned())
        })
        .filter_map(|m| {
            // Virtual modules: `@params`, rolldown's runtime and helpers.
            if m.starts_with("ns-ssg-params") || m.starts_with('\0') || m.starts_with("\\0") {
                return None;
            }
            let m = m.strip_prefix("ns-ssg-imp:").unwrap_or(&m).to_owned();
            let m = m
                .strip_prefix(site)
                .map_or(m.as_str(), |r| r.trim_start_matches('/'))
                .to_owned();
            Some(if m == "<stdin>" || entry.contains(&m) {
                "entry".to_owned()
            } else {
                m
            })
        })
        .collect()
}

/// The output format a case asked for.
fn format_of(case: &Json) -> String {
    case["steps"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["op"] == "js")
        .and_then(|s| s["opts"].as_object())
        .and_then(|o| o.iter().find(|(k, _)| k.eq_ignore_ascii_case("format")))
        .and_then(|(_, v)| v.as_str())
        .map_or_else(|| "iife".to_owned(), str::to_ascii_lowercase)
}

fn run_topic(topic: &str) {
    let test = format!("jsbuild_{topic}");
    let Some(node) = crate::run::node(&test) else {
        return;
    };
    let fx: Json = oracle(&format!(
        "oracle/resource-transformers/jsbuild/{topic}.json.gz"
    ));
    let site = site(fx["dir"].as_str().unwrap());
    let builder = JsBuilder::new(site.root.clone(), site.root.join("public"));
    let checker = Checker {
        node: &node,
        dir: crate::scratch(&format!("{test}-run")),
        site: site.root.to_string_lossy().into_owned(),
    };

    let cases = fx["cases"].as_array().unwrap();
    let results = fx["results"].as_array().unwrap();
    assert_eq!(cases.len(), results.len());
    let mut done = BTreeMap::new();
    let mut failures = Vec::new();
    for (case, want) in cases.iter().zip(results) {
        let name = case["name"].as_str().unwrap();
        let (outcome, contents) = run_case(&builder, &site, case, &done);
        match &outcome {
            Outcome::Built(out) => {
                done.insert(name.to_owned(), out.code.clone());
            }
            Outcome::Raw(b) => {
                done.insert(name.to_owned(), b.clone());
            }
            _ => {}
        }
        if let Err(e) = checker.check_case(case, want, &outcome, &contents) {
            failures.push(format!("{name}: {e}"));
        }
    }
    eprintln!(
        "jsbuild/{topic}: {}/{} cases match",
        cases.len() - failures.len(),
        cases.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn jsbuild_synth() {
    run_topic("synth");
}

#[test]
fn jsbuild_docs() {
    run_topic("docs");
}
