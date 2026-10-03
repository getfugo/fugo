//! `diagrams_goat` (feature `goat`) against Go: GoAT's own example suite and every GoAT
//! diagram of the legacy docs site, byte for byte.
//!
//! - `tests/fixtures/goat/examples/`: bep/goat v0.5.0 `examples/*.txt` and the `*.svg` its
//!   `TestExamples` compares with (`BuildAndWriteSVG`: the `<svg>` element without
//!   `font-family`, around the same body as `.Inner`).
//! - `tests/fixtures/goat/docs.tsv`: for each ```` ```goat ```` code block of
//!   `docs/content/**/*.md`, the width, height and SHA-256 of Go's `.Wrapped`, generated with
//!   Go 1.27 and bep/goat v0.5.0 by walking `docs/content` (`filepath.WalkDir`), matching the
//!   blocks with the regular expression of [`collect`], trimming trailing `\r`/`\n` as Go's
//!   code block `.Inner` does, and printing `goat.BuildSVG(…)`'s `Width`, `Height` and
//!   `sha256.Sum256([]byte(svg.String()))`.

use std::collections::BTreeMap;
use std::path::Path;

use sha2::{Digest, Sha256};
use tera::Context;

use crate::support::Harness;

/// The fixture directory.
fn fixtures() -> std::path::PathBuf {
    ssg_testkit::fixture::repo_dir().join("crates/funcs/tests/fixtures/goat")
}

/// The diagram of `text`: (inner, wrapped, width, height).
fn render(h: &Harness, text: &str) -> (String, String, u64, u64) {
    let mut ctx = Context::new();
    ctx.insert("text", text);
    let d = h
        .eval("diagrams_goat(text=text)", &ctx)
        .unwrap_or_else(|e| panic!("{e}"));
    let d = serde_json::to_value(&d).expect("serializable");
    let s = |key: &str| d[key].as_str().unwrap_or_default().to_owned();
    let n = |key: &str| d[key].as_u64().unwrap_or_default();
    (s("inner"), s("wrapped"), n("width"), n("height"))
}

/// Go's `TestExamples`: every example as `BuildAndWriteSVG` writes it.
#[test]
fn goat_examples() {
    let h = Harness::new();
    let dir = fixtures().join("examples");
    let mut inputs: Vec<_> = std::fs::read_dir(&dir)
        .expect("examples dir")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "txt"))
        .collect();
    inputs.sort();
    assert_eq!(inputs.len(), 21, "goat v0.5.0 has 21 examples");
    let mut failed = Vec::new();
    for input in &inputs {
        let text = std::fs::read_to_string(input).expect("utf-8 example");
        let golden = std::fs::read_to_string(input.with_extension("svg")).expect("golden svg");
        let (inner, _, width, height) = render(&h, &text);
        let svg = format!(
            "<svg class='diagram' xmlns='http://www.w3.org/2000/svg' version='1.1' \
             height='{height}' width='{width}'>\n{inner}</svg>\n"
        );
        if svg != golden {
            let line = svg
                .lines()
                .zip(golden.lines())
                .position(|(a, b)| a != b)
                .unwrap_or_else(|| svg.lines().count().min(golden.lines().count()));
            failed.push(format!(
                "{}: first difference at line {}:\n  ours: {:?}\n  goat: {:?}",
                input.display(),
                line + 1,
                svg.lines().nth(line),
                golden.lines().nth(line)
            ));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// Go's `TestExamplesStableOutput`.
#[test]
fn stable_output() {
    let h = Harness::new();
    let text =
        std::fs::read_to_string(fixtures().join("examples/circuits.txt")).expect("circuits.txt");
    let first = render(&h, &text);
    for _ in 0..2 {
        assert_eq!(render(&h, &text), first);
    }
}

/// The ```` ```goat ```` code blocks of the Markdown files below `dir`, as `(docs/<path from
/// root>#<n>, Go's .Inner)` (`root` is the legacy docs site, which the fixture names `docs/`).
fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, String)>) {
    let re = regex::Regex::new(r"(?ms)^```goat[^\n]*\n(.*?)^```").expect("valid");
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .expect("docs dir")
        .flatten()
        .map(|e| e.path())
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            collect(root, &p, out);
        } else if p.extension().is_some_and(|x| x == "md") {
            let text = std::fs::read_to_string(&p).expect("utf-8 markdown");
            let rel = p.strip_prefix(root).expect("below the root");
            let rel = rel.to_string_lossy().replace('\\', "/");
            for (n, caps) in re.captures_iter(&text).enumerate() {
                let inner = caps[1].trim_end_matches(['\r', '\n']);
                out.push((format!("docs/{rel}#{n}"), inner.to_owned()));
            }
        }
    }
}

/// Every docs diagram: the size and the `.Wrapped` bytes of Go's `diagrams.Goat`.
#[test]
fn every_docs_diagram_as_go() {
    let root = ssg_testkit::fixture::legacy_docs();
    let mut diagrams = Vec::new();
    collect(&root, &root.join("content"), &mut diagrams);
    let expected: BTreeMap<String, (u64, u64, String)> =
        std::fs::read_to_string(fixtures().join("docs.tsv"))
            .expect("docs.tsv")
            .lines()
            .filter(|l| !l.starts_with('#'))
            .map(|l| {
                let f: Vec<&str> = l.split('\t').collect();
                let n = |s: &str| s.parse::<u64>().expect("a size");
                (f[0].to_owned(), (n(f[1]), n(f[2]), f[3].to_owned()))
            })
            .collect();
    assert_eq!(
        diagrams.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
        expected.keys().map(String::as_str).collect::<Vec<_>>(),
        "the docs' goat blocks changed: regenerate docs.tsv"
    );
    let h = Harness::new();
    for (key, text) in &diagrams {
        let (_, wrapped, width, height) = render(&h, text);
        let sha: String = Sha256::digest(wrapped.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(
            (width, height, sha),
            expected[key].clone(),
            "{key}:\n{wrapped}"
        );
    }
}

#[test]
fn diagram() {
    let h = Harness::new();
    let out = h
        .render(
            "{% set d = diagrams_goat(text='+--+\n| A|-->\n+--+') %}\
             {{ d.width }}x{{ d.height }}\n{{ d.inner }}\n{{ d.wrapped }}",
            &Context::new(),
        )
        .expect("renders");
    ssg_testkit::snapshot::settings().bind(|| {
        insta::assert_snapshot!("diagrams_goat", out);
    });
}

/// The fields are safe strings (autoescaping leaves the SVG alone).
#[test]
fn safe_fields() {
    let h = Harness::new();
    let d = h
        .eval("diagrams_goat(text='a<b')", &Context::new())
        .expect("evaluates");
    let safe: Vec<(&str, bool)> = d
        .as_map()
        .expect("a map")
        .iter()
        .filter(|(_, v)| v.as_str().is_some())
        .map(|(k, v)| (k.as_str().unwrap_or_default(), v.is_safe()))
        .collect();
    assert_eq!(safe, [("inner", true), ("wrapped", true)]);
}
