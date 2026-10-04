//! The writer of `tests/data/chroma-testdata.json.gz` from Chroma's own lexer test suite (crate
//! README, "Fixtures"), in two steps around the Chroma oracle:
//! `FUGO_HL_TESTDATA=<chroma dir>:<cases.jsonl>` writes the oracle's input, and
//! `FUGO_HL_TESTDATA=<chroma dir>:<cases.jsonl>:<analyse.txt>:<output.json.gz>` the fixture
//! (relative paths are relative to the repository root).
//!
//! `<chroma dir>` is github.com/alecthomas/chroma/v2@v2.19.0 (e.g. from the Go module cache).
//! Each `lexers/testdata/**/*.actual` input becomes a case with the lexer Chroma's `TestLexers`
//! picks for it (the file's base name, or the directory name) and the FNV-1a hash of its
//! `*.expected` tokens (type and text of each coalesced token, joined by NUL: the hash
//! `lexers.rs` computes); the inputs of `lexers/testdata/analysis/` are added without tokens.
//! Every case also records the lexer Chroma's `lexers.Analyse` picks for its text (the Go
//! implementation's `guessSyntax`), from the oracle's `analyse.txt` (one line per case).

use std::io::Write as _;
use std::path::Path;

use serde_json::{Value, json};
use ssg_testkit::fixture::repo_dir;

use crate::corpus::fnv;

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| {
            e.expect("directory entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

/// The cases of Chroma's test suite, in its file order: `{file, lexer, code, hash}`, the
/// analysis inputs without `hash`.
fn cases(chroma: &Path) -> Vec<serde_json::Map<String, Value>> {
    let root = chroma.join("lexers/testdata");
    let mut out = Vec::new();
    for name in names(&root) {
        let path = root.join(&name);
        if name == "analysis" {
            continue;
        }
        let files: Vec<(String, std::path::PathBuf, String)> = if path.is_dir() {
            names(&path)
                .into_iter()
                .filter(|f| f.ends_with(".actual"))
                .map(|f| (name.clone(), path.join(&f), format!("{name}/{f}")))
                .collect()
        } else if let Some(stem) = name.strip_suffix(".actual") {
            let lexer = stem.split('.').next().unwrap_or(stem).to_owned();
            vec![(lexer, path.clone(), name.clone())]
        } else {
            continue;
        };
        for (lexer, actual, rel) in files {
            let expected = actual.with_extension("expected");
            let tokens: Vec<Value> = serde_json::from_str(&read(&expected))
                .unwrap_or_else(|e| panic!("{}: {e}", expected.display()));
            let mut parts = Vec::new();
            for t in &tokens {
                parts.push(t["type"].as_str().expect("a token type").to_owned());
                parts.push(t["value"].as_str().expect("a token value").to_owned());
            }
            let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
            let case =
                json!({ "file": rel, "lexer": lexer, "code": read(&actual), "hash": fnv(&parts) });
            out.push(case.as_object().expect("an object").clone());
        }
    }
    let analysis = root.join("analysis");
    for name in names(&analysis) {
        if name.ends_with(".actual") {
            let lexer = name.split('.').next().unwrap_or(&name).to_owned();
            let case = json!({ "file": format!("analysis/{name}"), "lexer": lexer, "code": read(&analysis.join(&name)) });
            out.push(case.as_object().expect("an object").clone());
        }
    }
    out
}

#[test]
fn chroma_testdata() {
    let Ok(arg) = std::env::var("FUGO_HL_TESTDATA") else {
        return;
    };
    let args: Vec<std::path::PathBuf> = arg.split(':').map(|a| repo_dir().join(a)).collect();
    let mut all = cases(&args[0]);
    match args.as_slice() {
        [_, jsonl] => {
            let mut text = String::new();
            for c in &all {
                text.push_str(&json!({ "lang": c["lexer"], "code": c["code"] }).to_string());
                text.push('\n');
            }
            std::fs::write(jsonl, text).unwrap_or_else(|e| panic!("{}: {e}", jsonl.display()));
        }
        [_, _, analyse, output] => {
            let picks = read(analyse);
            for (c, pick) in all.iter_mut().zip(picks.split('\n')) {
                c.insert("analyse".into(), Value::String(pick.to_owned()));
            }
            let file = std::fs::File::create(output)
                .unwrap_or_else(|e| panic!("{}: {e}", output.display()));
            let mut gz = flate2::write::GzEncoder::new(file, flate2::Compression::best());
            gz.write_all(serde_json::to_string(&all).expect("JSON").as_bytes())
                .expect("write");
            gz.finish().expect("write");
            println!("{} cases", all.len());
        }
        _ => panic!("FUGO_HL_TESTDATA=<chroma dir>:<cases.jsonl>[:<analyse.txt>:<output.json.gz>]"),
    }
}
