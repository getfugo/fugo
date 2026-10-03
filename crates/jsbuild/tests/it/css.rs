//! CSS imported from scripts against esbuild 0.25.6 (`fixtures/css-esbuild.json`): what an
//! entry importing CSS files sees (each import's default export and namespace, printed as JSON
//! by the bundle under node) must be what it saw when esbuild bundled it, key order included;
//! and the inputs this port deliberately rejects (see `css.rs`) must be errors.

use std::sync::Arc;

use serde_json::{Value as Json, json};
use ssg_jsbuild::{JsBuildError, JsBuildOptions, JsBuilder, MountedDirs, Source};

fn fixture() -> Json {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/it/fixtures/css-esbuild.json"
    );
    serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("fixture JSON")
}

/// The fixture's entry: imports each file (default and namespace) and prints what it got.
fn entry_js(imports: &[&str]) -> String {
    let mut js = String::new();
    for (i, p) in imports.iter().enumerate() {
        js.push_str(&format!(
            "import d{i} from \"./{p}\";\nimport * as n{i} from \"./{p}\";\n"
        ));
    }
    js.push_str("const out = {};\n");
    for (i, p) in imports.iter().enumerate() {
        js.push_str(&format!(
            "out[{p:?}] = {{ default: d{i}, ns: Object.keys(n{i}).sort().map(k => \
             [k, k === \"default\" ? n{i}.default === d{i} : n{i}[k]]) }};\n"
        ));
    }
    js.push_str("console.log(JSON.stringify(out));\n");
    js
}

/// Builds `entry.js` importing `imports` in a site of `files` with `loaders`.
fn build(
    name: &str,
    files: &[Json],
    imports: &[&str],
    loaders: &serde_json::Map<String, Json>,
) -> Result<String, JsBuildError> {
    let root = crate::scratch(&format!("css-{name}"));
    let assets = root.join("assets");
    for f in files {
        let file = assets.join(f[0].as_str().unwrap());
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, f[1].as_str().unwrap()).unwrap();
    }
    let entry = entry_js(imports);
    std::fs::write(assets.join("entry.js"), &entry).unwrap();
    let options = JsBuildOptions::from_json(&json!({
        "format": "esm",
        "platform": "node",
        "loaders": loaders,
    }))
    .unwrap();
    let source = Source {
        path: "entry.js",
        media_type: "text/javascript",
        contents: entry.as_bytes(),
    };
    let out = JsBuilder::new(root.clone(), root.join("public")).build(
        Arc::new(MountedDirs::new().mount(&assets, "")),
        &source,
        &options,
    )?;
    let script = root.join("out.mjs");
    std::fs::write(&script, &out.code).unwrap();
    Ok(script.to_string_lossy().into_owned())
}

#[test]
fn css_imports_see_what_esbuild_gave() {
    let Some(node) = crate::run::node("css_imports_see_what_esbuild_gave") else {
        return;
    };
    let fx = fixture();
    let mut failures = Vec::new();
    for case in fx["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let imports: Vec<&str> = case["imports"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i.as_str().unwrap())
            .collect();
        let loaders = case["loaders"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| (l[0].as_str().unwrap().to_owned(), l[1].clone()))
            .collect();
        let script = match build(name, case["files"].as_array().unwrap(), &imports, &loaders) {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!("{name}: {e}"));
                continue;
            }
        };
        let out = std::process::Command::new(&node)
            .arg(&script)
            .output()
            .expect("node");
        let got = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        let want = case["esbuild"].as_str().unwrap();
        if !out.status.success() || got != want {
            failures.push(format!(
                "{name}:\n  esbuild: {want}\n  ours: {got}{}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn css_inputs_we_reject() {
    let fx = fixture();
    for case in fx["errors"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let files = case["files"].as_array().unwrap();
        let imports: Vec<&str> = files.iter().map(|f| f[0].as_str().unwrap()).collect();
        let err = build(name, files, &imports, &serde_json::Map::new()).expect_err(name);
        let JsBuildError::Build(diags) = &err else {
            panic!("{name}: {err:?}")
        };
        let message = case["message"].as_str().unwrap();
        assert!(
            diags[0].text.starts_with(message),
            "{name}: {diags:?}, want {message:?}"
        );
    }
}
