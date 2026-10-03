//! Agreement with the Go oracle cases of the old port's `nh-tplfuncs` (extracted from the
//! `go-parity-final` tag into `tests/fixtures/tplfuncs.jsonl.gz` by
//! `tests/fixtures/extract_tplfuncs.py`). Run with `--nocapture` for the per-family table;
//! `NH_ORACLE_FAMILY=<family>` lists every disagreement of one family.
//!
//! A case agrees when both sides fail (error texts are never compared), or both succeed with the
//! same value: numbers compare numerically, and where Go returns `template.HTML` but this port a
//! plain string (this port returns text and leaves escaping to autoescape), the text may equal Go's
//! HTML as is, once escaped, or Go's HTML with its character references decoded (the same
//! page either way).
//!
//! The extraction skips what has no native counterpart: Go-only argument types (float32, named
//! types, typed nils, structs, bytes), `time.Time` printed as text (this port dates are
//! `{rfc3339, unix}` maps), Go panics and reflection errors, XML and CSV output.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value as J;
use tera::{Context, Value};

use crate::support::Harness;

#[derive(Deserialize)]
struct Case {
    fam: String,
    f: String,
    input: J,
    input_safe: bool,
    kwargs: BTreeMap<String, J>,
    lang: String,
    want: Want,
    src: String,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Want {
    Ok { ok: J, safe: bool },
    Err { err: String },
}

/// The families the acceptance criterion (REWRITE_PLAN.md §8.2, T31) gates at 95 %.
const GATED: &[&str] = &[
    "sort_by",
    "sets",
    "truncate_html",
    "humanize",
    "urlize",
    "plainify",
    "jsonify",
    "remarshal",
    "html_escape (safe input)",
    "default_if_empty",
    "pad",
];

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&#34;")
        .replace('\'', "&#39;")
}

/// The text of Go's HTML (character references decoded, with our `html_unescape`).
fn unescape(h: &Harness, s: &str) -> String {
    let mut ctx = Context::new();
    ctx.insert("s", s);
    h.eval("s | html_unescape", &ctx)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// JSON equality with numbers compared as floats.
fn same_json(a: &J, b: &J) -> bool {
    match (a, b) {
        (J::Number(x), J::Number(y)) => x.as_f64() == y.as_f64(),
        (J::Array(x), J::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same_json(a, b))
        }
        (J::Object(x), J::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| same_json(v, w)))
        }
        _ => a == b,
    }
}

fn agrees(h: &Harness, got: &Result<Value, String>, want: &Want) -> bool {
    match (got, want) {
        (Err(_), Want::Err { .. }) => true,
        (Ok(v), Want::Ok { ok, safe }) => {
            let got_json = serde_json::to_value(v).unwrap_or(J::Null);
            if same_json(&got_json, ok) {
                return true;
            }
            match (v.as_str(), ok.as_str()) {
                (Some(g), Some(w)) if *safe && !v.is_safe() => {
                    escape(g) == w || g == unescape(h, w)
                }
                _ => false,
            }
        }
        _ => false,
    }
}

fn eval(h: &Harness, c: &Case) -> Result<Value, String> {
    let mut ctx = Context::new();
    ctx.insert("lang", &c.lang);
    let input = ssg_testkit::tera_value(&c.input);
    let input = match input.as_str() {
        Some(s) if c.input_safe => Value::safe_string(s),
        _ => input,
    };
    ctx.insert_value("__in", input);
    let mut args = Vec::new();
    for (k, v) in &c.kwargs {
        ctx.insert_value(format!("__kw_{k}"), ssg_testkit::tera_value(v));
        args.push(format!("{k}=__kw_{k}"));
    }
    let call = format!("{}({})", c.f, args.join(", "));
    let is_function = matches!(c.f.as_str(), "querify" | "path_join" | "join_url");
    let expr = if is_function {
        call
    } else {
        format!("__in | {call}")
    };
    h.eval(&expr, &ctx)
}

#[test]
fn tplfuncs_agreement() {
    let cases: Vec<Case> = ssg_testkit::fixture::read_jsonl(
        &ssg_testkit::fixture::repo_dir().join("crates/funcs/tests/fixtures/tplfuncs.jsonl.gz"),
    )
    .expect("fixture");
    let h = Harness::new();
    // family → (agree, total, first disagreements)
    let mut stats: BTreeMap<String, (usize, usize, Vec<String>)> = BTreeMap::new();
    for c in &cases {
        let got = eval(&h, c);
        let ok = agrees(&h, &got, &c.want);
        let fam = if c.fam == "html_escape" && c.input_safe {
            "html_escape (safe input)".to_owned()
        } else {
            c.fam.clone()
        };
        let e = stats.entry(fam).or_default();
        e.1 += 1;
        if ok {
            e.0 += 1;
        } else if e.2.len() < 8 || std::env::var("NH_ORACLE_FAMILY").is_ok_and(|f| f == c.fam) {
            let want = match &c.want {
                Want::Ok { ok, safe } => format!("ok {ok} (safe {safe})"),
                Want::Err { err } => format!("err {err}"),
            };
            let got = match &got {
                Ok(v) => format!(
                    "ok {} (safe {})",
                    serde_json::to_value(v).unwrap_or(J::Null),
                    v.is_safe()
                ),
                Err(e) => format!("err {}", e.lines().last().unwrap_or_default()),
            };
            e.2.push(format!(
                "  {} {}({}) {:?}\n    want {want}\n    got  {got}",
                c.src,
                c.f,
                serde_json::to_string(&c.kwargs).unwrap_or_default(),
                c.input.to_string().chars().take(120).collect::<String>(),
            ));
        }
    }
    let mut report = String::new();
    let mut failing = Vec::new();
    for (fam, (agree, total, samples)) in &stats {
        #[allow(clippy::cast_precision_loss)]
        let rate = *agree as f64 * 100.0 / *total as f64;
        report.push_str(&format!("{fam:28} {agree:6}/{total:<6} {rate:6.2}%\n"));
        if GATED.contains(&fam.as_str()) && rate < 95.0 {
            failing.push(fam.clone());
        }
        if rate < 100.0 {
            for s in samples {
                report.push_str(s);
                report.push('\n');
            }
        }
    }
    eprintln!("{report}");
    for fam in GATED {
        assert!(stats.contains_key(*fam), "no cases for gated family {fam}");
    }
    assert!(failing.is_empty(), "below 95 %: {failing:?}");
}
