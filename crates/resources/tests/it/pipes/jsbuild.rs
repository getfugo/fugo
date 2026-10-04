//! `js_build` through the store against the `jsbuild` oracle (Go's `js.Build` on t16site,
//! 62 cases, and on the docs site's scripts, 6 cases): asset → (concat) → `js_build` →
//! (`fingerprint`). This port bundles with rolldown, so scripts differ from esbuild's bytes
//! (ssg-jsbuild's tests compare what they do); here media types, data and links must be
//! Go's, with fingerprints and `Data.Integrity` of the same form, and errors must be errors
//! at the same file and line. Published files: the script and, for external and linked source
//! maps, the map next to it.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value as J;
use ssg_base::ResourceId;

use super::{Project, data, fixture_site, project, project_except, run_steps};
use crate::support::MemSink;

/// A link or published path with its fingerprint (a long hex segment of the file name) as `H`.
fn unhashed(path: &str) -> String {
    let (dir, name) = path.rsplit_once('/').unwrap_or(("", path));
    let name: Vec<&str> = name
        .split('.')
        .map(|seg| {
            if seg.len() >= 32 && seg.bytes().all(|b| b.is_ascii_hexdigit()) {
                "H"
            } else {
                seg
            }
        })
        .collect();
    format!("{dir}/{}", name.join("."))
}

/// `data` without `Integrity`, and the algorithm prefix of its `Integrity` (`sha256-`).
fn split_integrity(mut data: J) -> (J, Option<String>) {
    let integrity = data
        .as_object_mut()
        .and_then(|m| m.remove("Integrity"))
        .and_then(|i| Some(i.as_str()?.split_once('-')?.0.to_owned()));
    (data, integrity)
}

fn run(p: &Project, fixture: &str) -> (usize, usize, Vec<String>) {
    let fx: J = ssg_testkit::fixture::oracle(fixture);
    let mut done: BTreeMap<String, ResourceId> = BTreeMap::new();
    let mut failures = Vec::new();
    let (mut built, mut errors) = (0, 0);
    let mut links = Vec::new();
    let mut want_published = BTreeSet::new();
    for (case, want) in fx["cases"]
        .as_array()
        .unwrap()
        .iter()
        .zip(fx["results"].as_array().unwrap())
    {
        let name = case["name"].as_str().unwrap();
        let err = want
            .get("contentErr")
            .or_else(|| want.get("stepErr"))
            .and_then(J::as_str);
        match (run_steps(p, case, &done), err) {
            (Ok(id), None) => {
                done.insert(name.to_owned(), id);
                let r = p.store.resource(id);
                if want["mediaType"] != r.media_type_string() {
                    failures.push(format!("{name}: media type {}", r.media_type_string()));
                }
                let (want_data, want_integrity) = split_integrity(data(&want["data"]));
                let (got_data, got_integrity) =
                    split_integrity(serde_json::to_value(&r.data).unwrap());
                if want_data != got_data || want_integrity != got_integrity {
                    failures.push(format!("{name}: data {:?}", r.data));
                }
                if let Some(link) = want.get("relPermalink").and_then(J::as_str) {
                    if unhashed(link) != unhashed(&r.rel_permalink) {
                        failures.push(format!("{name}: link {}", r.rel_permalink));
                    }
                    links.push(r.rel_permalink.clone());
                    want_published.extend(
                        want["published"]
                            .as_object()
                            .unwrap()
                            .keys()
                            .map(|k| unhashed(k)),
                    );
                }
                built += 1;
            }
            (Err((_, e)), Some(w)) => {
                let at = w
                    .split('"')
                    .find(|s| s.starts_with("$SITE/"))
                    .map(|s| p.site(s));
                if let Some(at) = at {
                    let file_line = at.rsplit_once(':').unwrap().0;
                    if !e.contains(&format!("{file_line}:")) {
                        failures.push(format!("{name}: error {e:?}, want at {at}"));
                    }
                }
                errors += 1;
            }
            (Ok(_), Some(w)) => failures.push(format!("{name}: built, Go fails: {w}")),
            (Err((_, e)), None) => failures.push(format!("{name}: {e}")),
        }
    }
    // What the links publish: the scripts and their source maps.
    let sink = MemSink::default();
    p.store
        .publish(links.iter().map(String::as_str), &sink)
        .unwrap();
    let got: BTreeSet<String> = sink.0.lock().unwrap().keys().map(|k| unhashed(k)).collect();
    if got != want_published {
        failures.push(format!("published {got:?}, want {want_published:?}"));
    }
    (built, errors, failures)
}

fn check(p: &Project, fixture: &str, cases: usize) {
    let (built, errors, failures) = run(p, fixture);
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(built + errors, cases);
    eprintln!(
        "js_build {fixture}: {built} scripts as Go publishes them, {errors} errors at Go's positions"
    );
}

#[test]
fn js_build_t16site() {
    let p = project(&fixture_site("t16site"), |_| {});
    check(&p, "oracle/resource-transformers/jsbuild/synth.json.gz", 62);
}

#[test]
fn js_build_docs() {
    // A copy without a local build's output and node modules (gitignored): the oracle's
    // `main-unresolved` case needs `alpinejs` not to resolve.
    let p = project_except(
        &crate::support::repo_dir().join("testdata/legacy-docs"),
        &["node_modules", "public", "resources"],
        |_| {},
    );
    check(&p, "oracle/resource-transformers/jsbuild/docs.json.gz", 6);
}

/// A transform that keeps its source's path (`js_build` of a `.js` asset without `targetPath`,
/// `to_css` of a `.css` asset) shares the source's URL: a link to it publishes the result, not
/// the source. A link to a source that has no such transform still publishes the source.
#[test]
fn a_result_on_its_sources_path_is_what_is_published() {
    use ssg_resources::Transform;
    use ssg_resources::pipes::{JsBuildSpec, ToCssOptions};

    let site = super::mini_site(&[
        ("config.toml", "baseURL = \"https://example.org/\"\n"),
        (
            "assets/js/main.js",
            "import { x } from \"./b.js\";\nconsole.log(x);\n",
        ),
        ("assets/js/b.js", "export const x = 41 + 1;\n"),
        ("assets/js/plain.js", "console.log(\"plain\");\n"),
        ("assets/css/a.css", "$c: red;\na { color: $c; }\n"),
    ]);
    let p = project(site.path(), |_| {});
    let s = &p.store;
    let main = p.asset("js/main.js");
    let built = s
        .transform(
            main,
            Transform::JsBuild(Box::new(
                JsBuildSpec::from_json(&serde_json::json!({})).unwrap(),
            )),
        )
        .unwrap();
    s.realize(built).unwrap();
    let css = p.asset("css/a.css");
    let compiled = s
        .transform(
            css,
            Transform::ToCss(ToCssOptions::from_json(&serde_json::json!({})).unwrap()),
        )
        .unwrap();
    s.realize(compiled).unwrap();
    assert_eq!(
        s.resource(built).rel_permalink,
        s.resource(main).rel_permalink
    );
    assert_eq!(
        s.resource(compiled).rel_permalink,
        s.resource(css).rel_permalink
    );
    let plain = p.asset("js/plain.js");

    let sink = MemSink::default();
    let links = [
        s.resource(built).rel_permalink.clone(),
        s.resource(compiled).rel_permalink.clone(),
        s.resource(plain).rel_permalink.clone(),
    ];
    s.publish(links.iter().map(String::as_str), &sink).unwrap();
    let files = sink.0.lock().unwrap();
    let js = String::from_utf8(files["js/main.js"].clone()).unwrap();
    assert!(!js.contains("import"), "the source was published:\n{js}");
    assert!(js.contains("41 + 1") || js.contains("42"), "{js}");
    let css = String::from_utf8(files["css/a.css"].clone()).unwrap();
    assert!(!css.contains('$'), "the source was published:\n{css}");
    assert!(css.contains("color: red"), "{css}");
    assert_eq!(
        String::from_utf8(files["js/plain.js"].clone()).unwrap(),
        "console.log(\"plain\");\n"
    );
}
