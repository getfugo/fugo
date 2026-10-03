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

/// The script without an inline source map (checked separately), and that map's JSON.
fn split_inline_map(code: &str) -> (String, Option<String>) {
    const MARK: &str = "//# sourceMappingURL=data:application/json;base64,";
    let Some(i) = code.find(MARK) else {
        return (code.to_owned(), None);
    };
    let start = i + MARK.len();
    // The oracle records the map decoded (and pretty-printed), as `DECODED(<map>)`.
    if let Some(map) = code[start..].strip_prefix("DECODED(") {
        let close = map.rfind(')').expect("DECODED(...)");
        let end = start + "DECODED(".len() + close + 1;
        return (
            format!("{}{}", &code[..i], &code[end..]),
            Some(map[..close].to_owned()),
        );
    }
    let end = code[start..].find('\n').map_or(code.len(), |n| start + n);
    let decoded = String::from_utf8(base64_decode(&code[start..end])).unwrap();
    (format!("{}{}", &code[..i], &code[end..]), Some(decoded))
}

fn base64_decode(s: &str) -> Vec<u8> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let (mut acc, mut bits) = (0u32, 0);
    for c in s.bytes().filter(|&c| c != b'=') {
        let v = ALPHABET.iter().position(|&a| a == c).expect("base64");
        acc = (acc << 6) | u32::try_from(v).unwrap();
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((acc >> bits) & 0xFF).unwrap());
        }
    }
    out
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

/// The oracle's error text, split into Go's `"file:line:col": message` position (if any) and
/// its message.
fn expected_error(err: &str) -> (Option<(String, u32, u32)>, String) {
    let detail = err.split_once("): ").map_or(err, |(_, d)| d);
    if let Some(rest) = detail.strip_prefix('"')
        && let Some((pos, msg)) = rest.split_once("\": ")
    {
        let mut it = pos.rsplitn(3, ':');
        let col = it.next().unwrap().parse().unwrap();
        let line = it.next().unwrap().parse().unwrap();
        let file = it.next().unwrap().to_owned();
        return (Some((file, line, col)), msg.to_owned());
    }
    (None, detail.to_owned())
}

/// Checks an error outcome against the oracle's text; `Err` explains a mismatch.
fn check_error(outcome: &Outcome, want: &str, site: &str) -> Result<(), String> {
    let (pos, msg) = expected_error(&want.replace("$SITE", site));
    let quoted = msg.split('"').nth(1).map(str::to_owned).unwrap_or_default();
    // The wording this port keeps from esbuild.
    let same_text =
        msg.starts_with("Could not resolve") || msg.contains("configured target environment");
    let ok = match outcome {
        Outcome::Build(JsBuildError::Build(diags)) => {
            let d = &diags[0];
            let got_pos = d
                .position
                .as_ref()
                .map(|p| (p.file.to_string_lossy().into_owned(), p.line, p.column));
            got_pos == pos && (!same_text || d.text == msg)
        }
        Outcome::Options(OptionsError::Value { value, .. }) => pos.is_none() && *value == quoted,
        Outcome::Options(OptionsError::Type { option, .. }) => {
            msg.contains("error(s) decoding") && msg.to_lowercase().contains(&format!("'{option}'"))
        }
        Outcome::Build(JsBuildError::InjectNotFound(p)) => {
            msg.starts_with("inject: file") && *p == quoted
        }
        Outcome::Build(JsBuildError::InjectAbsolute(_)) => msg.starts_with("inject: absolute"),
        Outcome::Build(JsBuildError::UnsupportedMediaType(m)) => {
            msg.starts_with("unsupported Media Type") && *m == quoted
        }
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        Err(format!("want error {want:?}, got {}", describe(outcome)))
    }
}

fn describe(o: &Outcome) -> String {
    match o {
        Outcome::Built(out) => format!("output {:?}", String::from_utf8_lossy(&out.code)),
        Outcome::Raw(b) => format!("raw {:?}", String::from_utf8_lossy(b)),
        Outcome::Options(e) => format!("options error {e:?}"),
        Outcome::Build(e) => format!("build error {e:?}"),
    }
}

/// Checks a source map: every source a file URL whose contents are in `sourcesContent`
/// (unless contents are left out), and every file of the Go implementation's map among them.
fn check_map(got: &str, want: Option<&str>, entry_contents: &[u8]) -> Result<(), String> {
    let got: Json = serde_json::from_str(got).map_err(|e| e.to_string())?;
    // `mappings` is empty when nothing in the script comes from a source.
    if got["version"] != 3 || got["mappings"].as_str().is_none() {
        return Err(format!("not a source map: {got}"));
    }
    let sources = got["sources"].as_array().cloned().unwrap_or_default();
    let contents = got["sourcesContent"].as_array().cloned();
    if let Some(c) = &contents
        && c.len() != sources.len()
    {
        return Err(format!(
            "{} sources for {} contents",
            sources.len(),
            c.len()
        ));
    }
    for (i, s) in sources.iter().enumerate() {
        let url = s.as_str().ok_or("source not a string")?;
        let path = url
            .strip_prefix("file://")
            .ok_or_else(|| format!("source {url} is not a file URL"))?;
        let path = percent_decode(path);
        let Some(c) = contents.as_ref().and_then(|c| c[i].as_str()) else {
            continue;
        };
        // rolldown records the module it made of a JSON or text file, not the file.
        let script = [".js", ".mjs", ".cjs", ".jsx", ".ts", ".tsx"]
            .iter()
            .any(|e| path.ends_with(e));
        match std::fs::read(&path) {
            Ok(bytes) if bytes == c.as_bytes() || !script => {}
            // The entry's contents are what js.Build got, not the file's.
            _ if c.as_bytes() == entry_contents => {}
            Ok(_) => return Err(format!("contents of {path} differ")),
            Err(e) => return Err(format!("source {path}: {e}")),
        }
    }
    if let Some(want) = want {
        let want: Json = serde_json::from_str(want).map_err(|e| e.to_string())?;
        for s in want["sources"].as_array().into_iter().flatten() {
            let Some(url) = s.as_str() else { continue };
            let exists = url
                .strip_prefix("file://")
                .is_some_and(|p| Path::new(&percent_decode(p)).is_file());
            if exists && !sources.contains(s) {
                return Err(format!("source {s} missing from {sources:?}"));
            }
        }
    }
    Ok(())
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
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

struct Checker<'a> {
    node: &'a Path,
    dir: PathBuf,
    site: String,
}

impl Checker<'_> {
    fn check_case(
        &self,
        case: &Json,
        want: &Json,
        outcome: &Outcome,
        contents: &[u8],
    ) -> Result<(), String> {
        let site = &self.site;
        if let Some(err) = want["contentErr"].as_str() {
            return check_error(outcome, err, site);
        }
        let name = case["name"].as_str().unwrap();
        let expected = want["content"]
            .as_str()
            .unwrap()
            .replace("$SITE", site)
            .replace(GO_NAMESPACE, "ns-ssg-");
        let (code, out) = match outcome {
            Outcome::Built(out) => (String::from_utf8(out.code.clone()).unwrap(), out),
            Outcome::Raw(b) => {
                return if *b == expected.as_bytes() {
                    Ok(())
                } else {
                    Err("the asset changed".to_owned())
                };
            }
            other => return Err(format!("want output, got {}", describe(other))),
        };
        let (code, inline_map) = split_inline_map(&code);
        if let Some(map) = &inline_map {
            check_map(map, None, contents).map_err(|e| format!("inline map: {e}"))?;
        }
        let (expected, _) = split_inline_map(&expected);

        let format = format_of(case);
        let want_trace = crate::run::trace(
            self.node,
            &self.dir,
            &format!("{name}-oracle"),
            expected.as_bytes(),
            &format,
        );
        let got_trace = crate::run::trace(
            self.node,
            &self.dir,
            &format!("{name}-ours"),
            code.as_bytes(),
            &format,
        );
        if got_trace != want_trace {
            return Err(format!(
                "behaviour differs:\n--- got\n{got_trace}--- want\n{want_trace}--- code\n{code}"
            ));
        }

        // The entry's names: its asset (in the assets or the `vendor` mount) or, for a
        // concatenation, its target path.
        let entry = case["steps"][0]["path"]
            .as_str()
            .or_else(|| case["steps"][0]["target"].as_str())
            .unwrap_or_default();
        let entry = [
            entry.to_owned(),
            format!("assets/{entry}"),
            format!("node_modules/{}", entry.trim_start_matches("vendor/")),
        ];
        let (got_modules, want_modules) = (
            modules(&code, site, &entry),
            modules(&expected, site, &entry),
        );
        // rolldown leaves out a module it inlined completely (a constant default export).
        if !want_modules.is_empty() && !got_modules.is_subset(&want_modules) {
            return Err(format!(
                "modules: got {got_modules:?}, want {want_modules:?}"
            ));
        }

        let fingerprinted = case["steps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["op"] == "fingerprint");
        for (path, published) in want["published"].as_object().into_iter().flatten() {
            let published = published
                .as_str()
                .unwrap()
                .replace("$SITE", site)
                .replace(GO_NAMESPACE, "ns-ssg-");
            if let Some(script) = path.strip_suffix(".map") {
                if script != out.target_path {
                    return Err(format!("map at {path}, target {}", out.target_path));
                }
                let map = out.source_map.as_deref().ok_or("no source map")?;
                let map = std::str::from_utf8(map).map_err(|e| e.to_string())?;
                check_map(map, Some(&published), contents)?;
            } else if !fingerprinted && *path != out.target_path {
                return Err(format!("published at {path}, target {}", out.target_path));
            }
        }
        Ok(())
    }
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
