//! `target: es5` end to end: each fixture project (`fixtures/es5/`: arrows, templates, CommonJS
//! dependencies, JSON/text/binary loaders, `export *`, dynamic import, TypeScript enums and
//! namespaces, JSX), in every format, minified and not, must build for `es5` (the build checks
//! that the bundle is ES5) and print under node what its `es2015` build prints.

use std::path::Path;
use std::sync::Arc;

use serde_json::json;
use ssg_jsbuild::{JsBuildError, JsBuildOptions, JsBuilder, MountedDirs, Source};

const FIXTURES: &[(&str, &str)] = &[
    ("basic", "main.js"),
    ("cjs", "main.js"),
    ("ts", "main.ts"),
    ("jsx", "main.jsx"),
    ("exports", "main.js"),
];

fn build(fixture: &Path, entry: &str, format: &str, minify: bool, target: &str) -> Vec<u8> {
    let contents = std::fs::read(fixture.join(entry)).unwrap();
    let media_type = match Path::new(entry).extension().and_then(|e| e.to_str()) {
        Some("ts") => "text/typescript",
        Some("jsx") => "text/jsx",
        _ => "text/javascript",
    };
    let source = Source {
        path: entry,
        media_type,
        contents: &contents,
    };
    let options = JsBuildOptions::from_json(&json!({
        "format": format,
        "minify": minify,
        "target": target,
        "loaders": {".bin": "binary"},
    }))
    .unwrap();
    let root = fixture.parent().unwrap();
    JsBuilder::new(root.to_path_buf(), root.join("public"))
        .build(
            Arc::new(MountedDirs::new().mount(fixture, "")),
            &source,
            &options,
        )
        .unwrap_or_else(|e| panic!("{} {format} {target}: {e}", fixture.display()))
        .code
}

#[test]
fn fixtures_build_for_es5_and_behave_as_es2015() {
    let Some(node) = crate::run::node("fixtures_build_for_es5_and_behave_as_es2015") else {
        return;
    };
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/it/fixtures/es5");
    let dir = crate::scratch("es5-run");
    let run = |name: &str, code: &[u8]| {
        let file = dir.join(name);
        std::fs::write(&file, code).unwrap();
        let out = std::process::Command::new(&node)
            .arg(&file)
            .output()
            .expect("node");
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    };
    let mut failures = Vec::new();
    for (name, entry) in FIXTURES {
        let fixture = fixtures.join(name);
        for (format, ext) in [("iife", "js"), ("cjs", "cjs"), ("esm", "mjs")] {
            for minify in [false, true] {
                let label = format!("{name}-{format}{}", if minify { "-min" } else { "" });
                let es2015 = build(&fixture, entry, format, minify, "es2015");
                let es5 = build(&fixture, entry, format, minify, "es5");
                let want = run(&format!("{label}-es2015.{ext}"), &es2015);
                let got = run(&format!("{label}-es5.{ext}"), &es5);
                if want != got || want.is_empty() {
                    failures.push(format!("{label}:\n  es2015: {want:?}\n  es5:    {got:?}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn es5_errors_are_esbuilds() {
    let root = crate::scratch("es5-errors");
    let assets = root.join("assets");
    std::fs::create_dir_all(&assets).unwrap();
    let source = "var ok = 1;\nconst x = 2;\nclass A {}\n";
    std::fs::write(assets.join("a.js"), source).unwrap();
    let err = JsBuilder::new(root.clone(), root.join("public"))
        .build(
            Arc::new(MountedDirs::new().mount(&assets, "")),
            &Source {
                path: "a.js",
                media_type: "text/javascript",
                contents: source.as_bytes(),
            },
            &JsBuildOptions::from_json(&json!({"target": "es5"})).unwrap(),
        )
        .unwrap_err();
    let JsBuildError::Build(diags) = err else {
        panic!("{err:?}")
    };
    let got: Vec<(u32, u32, &str)> = diags
        .iter()
        .map(|d| {
            let p = d.position.as_ref().expect("position");
            (p.line, p.column, d.text.as_str())
        })
        .collect();
    assert_eq!(
        got,
        [
            (
                2,
                0,
                "Transforming const to the configured target environment (\"es5\") is not supported yet"
            ),
            (
                3,
                0,
                "Transforming class syntax to the configured target environment (\"es5\") is not supported yet"
            ),
        ]
    );
}
