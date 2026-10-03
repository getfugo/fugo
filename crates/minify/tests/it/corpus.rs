//! Robustness corpora (T26 acceptance): every input minifies without a panic, the result is
//! idempotent, and it re-parses as the same type.
//!
//! Inputs:
//! - `tests/fixtures/tdewolff-inputs.jsonl.gz`: the unique UTF-8 inputs of the old port's
//!   tdewolff fixtures (`git show go-parity-final:crates/tdewolff-minify/tests/fixtures/…`):
//!   `upstream` (tdewolff's own `_test.go` tables, without `TestCSSInline` and the number
//!   helpers), `literals` and `structured` (every third row), without rows that pass
//!   media-type parameters; and the JS inputs of `crates/tdewolff-minify-js/tests/fixtures`:
//!   `grammar` (every eighth), `adversarial` and `repo` (inputs up to 16 KiB), `literals`.
//!   The `fuzz` rows (mutations of the owner's private site) and that site's inline scripts
//!   were removed.
//!   Records: `{"from", "kind", "input"}`.
//! - `testdata/oracle/commands/e2e/e2e.json.gz`: the Go `--minify` output trees of the e2e sites
//!   (HTML, XML, JSON and CSS already minified by tdewolff).
//! - `testdata/oracle/sitebuild/build/*.json.gz`: the unminified HTML outputs of the build oracle.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value as Json;
use ssg_minify::{Minifier, MinifyTarget, Options};
use ssg_testkit::fixture::{read_json, read_jsonl, repo_dir, testdata};

struct Input {
    from: String,
    kind: MinifyTarget,
    input: String,
}

#[derive(Deserialize)]
struct Record {
    from: String,
    kind: String,
    input: String,
}

fn kind(name: &str) -> MinifyTarget {
    match name {
        "html" => MinifyTarget::Html,
        "css" => MinifyTarget::Css,
        "js" => MinifyTarget::Js,
        "json" => MinifyTarget::Json,
        "svg" => MinifyTarget::Svg,
        "xml" => MinifyTarget::Xml,
        other => panic!("unknown kind {other}"),
    }
}

/// The kind of an output file, by extension.
fn kind_of_path(path: &str) -> Option<MinifyTarget> {
    let ext = Path::new(path).extension()?.to_str()?;
    Some(match ext {
        "html" | "htm" => MinifyTarget::Html,
        "css" => MinifyTarget::Css,
        "js" => MinifyTarget::Js,
        "json" => MinifyTarget::Json,
        "svg" => MinifyTarget::Svg,
        "xml" => MinifyTarget::Xml,
        _ => return None,
    })
}

fn salvaged() -> Vec<Input> {
    let path = repo_dir().join("crates/minify/tests/fixtures/tdewolff-inputs.jsonl.gz");
    read_jsonl::<Record>(&path)
        .unwrap_or_else(|e| panic!("{e}"))
        .into_iter()
        .map(|r| Input {
            kind: kind(&r.kind),
            from: r.from,
            input: r.input,
        })
        .collect()
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect()
}

/// The Go `--minify` outputs of the e2e sites.
fn e2e_outputs() -> Vec<Input> {
    let doc: Json = read_json(&testdata("oracle/commands/e2e/e2e.json.gz")).expect("e2e");
    let mut out = Vec::new();
    for case in doc["cases"].as_array().expect("cases") {
        let Some(tree) = case["result"]["tree"].as_object() else {
            continue;
        };
        for (path, bytes) in tree {
            let (Some(kind), Some(bytes)) = (kind_of_path(path), bytes.as_str()) else {
                continue;
            };
            if let Ok(input) = String::from_utf8(hex(bytes)) {
                out.push(Input {
                    from: "e2e-go-minified".to_owned(),
                    kind,
                    input,
                });
            }
        }
    }
    out
}

/// The unminified HTML outputs recorded by the Go site-build oracle.
fn build_outputs() -> Vec<Input> {
    let dir = testdata("oracle/sitebuild/build");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("build oracle dir")
        .map(|e| e.expect("entry").path())
        .collect();
    files.sort();
    let mut out = Vec::new();
    for file in files {
        let doc: Json = read_json(&file).unwrap_or_else(|e| panic!("{e}"));
        for event in doc["events"].as_array().into_iter().flatten() {
            let (Some(path), Some(bytes)) = (event["path"].as_str(), event["bytes"].as_str())
            else {
                continue;
            };
            if let Some(kind) = kind_of_path(path) {
                out.push(Input {
                    from: "go-site-build".to_owned(),
                    kind,
                    input: bytes.to_owned(),
                });
            }
        }
    }
    out
}

#[derive(Default)]
struct Tally {
    files: usize,
    bytes_in: usize,
    bytes_out: usize,
    rejected: usize,
    panics: usize,
    not_idempotent: usize,
    reparse: usize,
    /// HTML inputs the spec tokenizer reports errors for (not checked for idempotence/re-parse).
    malformed: usize,
    /// … of which the output is not idempotent (informational).
    malformed_unstable: usize,
    examples: Vec<String>,
}

struct Report(BTreeMap<(String, String), Tally>);

impl Report {
    fn total(&self, f: impl Fn(&Tally) -> usize) -> usize {
        self.0.values().map(f).sum()
    }

    fn print(&self) {
        let mut s = String::from(
            "from                 kind   files  rejected  panics  !idem  !reparse  malformed(!idem)  bytes in -> out\n",
        );
        for ((from, kind), t) in &self.0 {
            let _ = writeln!(
                s,
                "{from:<20} {kind:<5} {:>6} {:>9} {:>7} {:>6} {:>9} {:>10}({:>5})  {} -> {}",
                t.files,
                t.rejected,
                t.panics,
                t.not_idempotent,
                t.reparse,
                t.malformed,
                t.malformed_unstable,
                t.bytes_in,
                t.bytes_out
            );
        }
        for t in self.0.values() {
            for e in &t.examples {
                let _ = writeln!(s, "  {e}");
            }
        }
        eprintln!("{s}");
    }
}

fn clip(s: &str) -> String {
    let mut end = s.len().min(160);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{:?}", &s[..end])
}

fn run(m: &Minifier, inputs: &[Input]) -> Report {
    let mut report = BTreeMap::<(String, String), Tally>::new();
    for inp in inputs {
        let t = report
            .entry((inp.from.clone(), format!("{:?}", inp.kind).to_lowercase()))
            .or_default();
        t.files += 1;
        t.bytes_in += inp.input.len();
        let note = |t: &mut Tally, what: &str, detail: String| {
            if t.examples.len() < 40 {
                t.examples.push(format!(
                    "{} {:?} {what}: {} | {detail}",
                    inp.from,
                    inp.kind,
                    clip(&inp.input)
                ));
            }
        };
        let first = catch_unwind(AssertUnwindSafe(|| m.minify(inp.kind, &inp.input)));
        let out = match first {
            Err(_) => {
                t.panics += 1;
                t.examples.insert(
                    0,
                    format!("{} {:?} panic: {:?}", inp.from, inp.kind, inp.input),
                );
                continue;
            }
            Ok(Err(e)) => {
                t.rejected += 1;
                if std::env::var_os("FUGO_MINIFY_SHOW_REJECTED").is_some() {
                    note(t, "rejected", e.to_string());
                }
                continue;
            }
            Ok(Ok(out)) => out.into_owned(),
        };
        t.bytes_out += out.len();
        if inp.kind == MinifyTarget::Html && !html_well_formed(&inp.input) {
            t.malformed += 1;
            match catch_unwind(AssertUnwindSafe(|| m.minify(inp.kind, &out))) {
                Err(_) => {
                    t.panics += 1;
                    note(t, "panic on its own output", clip(&out));
                }
                Ok(again) => {
                    if again.ok().as_deref() != Some(out.as_str()) {
                        t.malformed_unstable += 1;
                    }
                }
            }
            continue;
        }
        match catch_unwind(AssertUnwindSafe(|| m.minify(inp.kind, &out))) {
            Err(_) => {
                t.panics += 1;
                note(t, "panic on its own output", clip(&out));
            }
            Ok(Err(e)) => {
                t.not_idempotent += 1;
                note(t, "output rejected", format!("{e} in {}", clip(&out)));
            }
            Ok(Ok(again)) if again != out => {
                t.not_idempotent += 1;
                let at = out
                    .char_indices()
                    .zip(again.chars())
                    .find(|((_, a), b)| a != b)
                    .map_or(out.len().min(again.len()), |((i, _), _)| i);
                let from = out.floor_char_boundary(at.saturating_sub(40));
                let to = again.floor_char_boundary(at.saturating_sub(40));
                note(
                    t,
                    "not idempotent",
                    format!("…{} -> …{}", clip(&out[from..]), clip(&again[to..])),
                );
            }
            Ok(Ok(_)) => {}
        }
        if let Err(why) = reparse(inp.kind, &inp.input, &out) {
            t.reparse += 1;
            note(t, "re-parse", format!("{why} in {}", clip(&out)));
        }
    }
    Report(report)
}

/// Whether `out` parses as `kind` (and, where the check is exact, means the same as `input`).
fn reparse(kind: MinifyTarget, input: &str, out: &str) -> Result<(), String> {
    match kind {
        MinifyTarget::Html => reparse_html(input, out),
        MinifyTarget::Css => {
            let parse = |s| {
                lightningcss::stylesheet::StyleSheet::parse(
                    s,
                    lightningcss::stylesheet::ParserOptions::default(),
                )
                .map(drop)
                .map_err(|e| e.to_string())
            };
            // Input lightningcss rejects passes through (css_tolerance.rs): nothing to check.
            if parse(input).is_err() {
                return Ok(());
            }
            parse(out)
        }
        MinifyTarget::Js => {
            let alloc = oxc_allocator::Allocator::default();
            let r =
                oxc_parser::Parser::new(&alloc, out, oxc_span::SourceType::unambiguous()).parse();
            r.errors.first().map_or(Ok(()), |e| Err(e.to_string()))
        }
        MinifyTarget::Json => match serde_json::from_str::<Json>(input) {
            Ok(before) => match serde_json::from_str::<Json>(out) {
                Ok(after) if after == before => Ok(()),
                Ok(_) => Err("different value".to_owned()),
                Err(e) => Err(e.to_string()),
            },
            // Lenient input (trailing commas, raw control characters): nothing to compare.
            Err(_) => Ok(()),
        },
        MinifyTarget::Svg | MinifyTarget::Xml => reparse_xml(input, out),
    }
}

fn elements(doc: &roxmltree::Document<'_>) -> Vec<String> {
    doc.descendants()
        .filter(roxmltree::Node::is_element)
        .map(|n| {
            let name = n.tag_name();
            format!(
                "{}:{}/{}",
                name.namespace().unwrap_or_default(),
                name.name(),
                n.attributes().len()
            )
        })
        .collect()
}

fn reparse_xml(input: &str, out: &str) -> Result<(), String> {
    let opts = || roxmltree::ParsingOptions {
        allow_dtd: true,
        ..roxmltree::ParsingOptions::default()
    };
    let Ok(before) = roxmltree::Document::parse_with_options(input, opts()) else {
        return Ok(());
    };
    let after = roxmltree::Document::parse_with_options(out, opts()).map_err(|e| e.to_string())?;
    if elements(&before) == elements(&after) {
        Ok(())
    } else {
        Err("different elements".to_owned())
    }
}

/// Start tag names and the number of tokenizer errors; a raw-text or RCDATA element that runs to
/// the end of the input counts as an error.
fn html_tokens(s: &str) -> (Vec<String>, usize) {
    const RAW: [&str; 5] = ["script", "style", "title", "textarea", "xmp"];
    let mut tags = Vec::new();
    let mut errors = 0;
    let mut open_raw: Option<String> = None;
    // Switch to RCDATA/RAWTEXT/script data after `<title>`, `<style>`, `<script>`, … as a tree
    // builder would.
    let mut emitter = html5gum::DefaultEmitter::default();
    emitter.naively_switch_states(true);
    for token in html5gum::Tokenizer::new_with_emitter(s, emitter) {
        match token {
            Ok(html5gum::Token::StartTag(t)) => {
                let name = String::from_utf8_lossy(&t.name).into_owned();
                // `<textarea/>` opens a textarea too.
                if RAW.contains(&name.as_str()) {
                    open_raw = Some(name.clone());
                }
                tags.push(name);
            }
            Ok(html5gum::Token::EndTag(t)) => {
                if open_raw.as_deref().is_some_and(|n| n.as_bytes() == *t.name) {
                    open_raw = None;
                }
            }
            Ok(html5gum::Token::Error(_)) => errors += 1,
            _ => {}
        }
    }
    (tags, errors + usize::from(open_raw.is_some()))
}

/// Tags whose start tag HTML may omit (minify-html omits them with `keepDocumentTags = false`).
const OPTIONAL_START: [&str; 5] = ["html", "head", "body", "tbody", "colgroup"];

/// Whether the spec tokenizer reads `s` without errors and every tag name is a plain
/// `[a-z][a-z0-9-]*` (optionally with one `:` prefix). minify-html's parser is not a spec
/// tokenizer: it ends tag names at `<`, `!`, `*`, `&`, … and re-quotes broken attributes, so such
/// malformed input is read differently and its output is not always a fixed point.
fn html_well_formed(s: &str) -> bool {
    let name_ok = |n: &str| {
        let mut parts = n.splitn(2, ':');
        parts.all(|p| {
            p.starts_with(|c: char| c.is_ascii_lowercase())
                && p.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
    };
    let (tags, errors) = html_tokens(s);
    errors == 0 && tags.iter().all(|t| name_ok(t))
}

/// For well-formed input: the output tokenizes without errors, with the same start tags (apart
/// from optional ones).
fn reparse_html(input: &str, out: &str) -> Result<(), String> {
    let significant = |tags: Vec<String>| -> Vec<String> {
        tags.into_iter()
            .filter(|t| !OPTIONAL_START.contains(&t.as_str()))
            .collect()
    };
    let tags_in = significant(html_tokens(input).0);
    let (tags_out, errors_out) = html_tokens(out);
    let tags_out = significant(tags_out);
    if errors_out > 0 {
        return Err(format!("{errors_out} tokenizer errors"));
    }
    if tags_in != tags_out {
        return Err(format!("start tags {tags_in:?} -> {tags_out:?}"));
    }
    Ok(())
}

fn assert_clean(report: &Report) {
    report.print();
    assert_eq!(report.total(|t| t.panics), 0, "panics");
    assert_eq!(report.total(|t| t.not_idempotent), 0, "not idempotent");
    assert_eq!(report.total(|t| t.reparse), 0, "output does not re-parse");
}

#[test]
fn tdewolff_fixture_inputs() {
    let inputs = salvaged();
    assert!(inputs.len() > 6_000, "{} inputs", inputs.len());
    assert_clean(&run(&Minifier::default(), &inputs));
}

#[test]
fn site_outputs() {
    let mut inputs = e2e_outputs();
    let go = inputs.len();
    inputs.extend(build_outputs());
    assert!(
        go > 50 && inputs.len() > 400,
        "{go} / {} inputs",
        inputs.len()
    );
    let report = run(&Minifier::default(), &inputs);
    assert_eq!(report.total(|t| t.rejected), 0, "rejected site outputs");
    assert_clean(&report);
}

#[test]
fn non_default_options_on_fixture_inputs() {
    let mut o = Options::default();
    o.html.keep_end_tags = false;
    o.html.keep_document_tags = false;
    o.html.comments = ssg_minify::options::HtmlComments::KeepAll;
    o.css.keep_css2 = false;
    o.js.keep_var_names = true;
    o.svg.comments = ssg_minify::options::XmlComments::Keep;
    o.xml.whitespace = ssg_minify::options::XmlWhitespace::Keep;
    let inputs: Vec<Input> = salvaged()
        .into_iter()
        .filter(|i| i.from == "upstream" || i.from == "literals")
        .collect();
    assert_clean(&run(&Minifier::with_options(o, &[]), &inputs));
}
