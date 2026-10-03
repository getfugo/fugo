//! goldmark structure the docs site depends on, against the Go implementation's output: pipe
//! tables as a paragraph transformer (lazy continuation lines, padded headers, rows, escaped
//! pipes, list tightness, task items, the lines before a header, a setext underline or a
//! definition after a table), the context markers of `.RenderShortcodes` includes and the
//! empty text blocks of link reference definitions.
//!
//! The documents and their expected HTML are `tests/data/compat/compat.json`, written by
//! `tests/data/compat/mdcompat.go.txt` (its header has the recipe): the Go implementation's
//! goldmark converter at 44529028 with the markup configuration `default` of the oracles'
//! `mdoracle.Configs` and the table and code block replicas of `mdoracle.ReplicaRenderers`.
//! `<<WRAP:…:WRAP>>` in a document is an include (wrapped in the Go implementation's context
//! markers), its further lines indented like an indented shortcode call.

use std::path::Path;

use serde::Deserialize;
use ssg_markup::{CodeBlockCtx, HookEnv, HookError, HookOut, Hooks, MarkdownOptions, wrap_context};
use ssg_testkit::fixture::read_json;

use super::render_with;

/// The Go implementation's code block replica of the oracle.
struct Replica;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

impl Hooks for Replica {
    fn code_block(&self, env: &HookEnv, c: &CodeBlockCtx) -> Result<HookOut, HookError> {
        Ok(HookOut::Html(format!(
            "<pre data-lang=\"{}\" data-ordinal=\"{}\">{}</pre>\n",
            esc(&c.lang),
            env.ordinal,
            esc(&c.inner)
        )))
    }
}

/// `md` with each `<<WRAP:text:WRAP>>` replaced by the wrapped text, its further lines
/// indented like the call when only spaces precede it on its line.
fn expand(md: &str) -> String {
    let mut s = md.to_owned();
    while let Some(i) = s.find("<<WRAP:") {
        let j = i + s[i..].find(":WRAP>>").expect("closed");
        let (wrapped, _) = wrap_context(&s[i + "<<WRAP:".len()..j]);
        let line = s[..i].rfind('\n').map_or(0, |p| p + 1);
        let indent = if s[line..i].trim().is_empty() {
            s[line..i].to_owned()
        } else {
            String::new()
        };
        let mut out = String::new();
        for (k, l) in wrapped.split_inclusive('\n').enumerate() {
            if k > 0 {
                out.push_str(&indent);
            }
            out.push_str(l);
        }
        s.replace_range(i..j + ":WRAP>>".len(), &out);
    }
    s
}

/// Go's empty row for the closing context marker after a table (not reproduced).
const MARKER_ROW: &str = "\n      <tr>\n          <td></td>\n          <td></td>\n      </tr>";

/// A document of `compat.json` and the Go implementation's HTML for it.
#[derive(Deserialize)]
struct Case {
    name: String,
    markdown: String,
    html: String,
}

#[test]
fn goldmark_structure() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/compat/compat.json");
    let cases: Vec<Case> = read_json(&path).unwrap_or_else(|e| panic!("{e}"));
    assert!(cases.len() >= 53, "{} documents", cases.len());
    let o = MarkdownOptions::default();
    let mut failed = Vec::new();
    for c in &cases {
        let want = if c.name == "include-table-end" {
            c.html.replacen(MARKER_ROW, "", 1)
        } else {
            c.html.clone()
        };
        let got = render_with(&expand(&c.markdown), &o, &Replica).html;
        if got != want {
            eprintln!("--- {}\n want: {want:?}\n got:  {got:?}", c.name);
            failed.push(c.name.as_str());
        }
    }
    assert!(failed.is_empty(), "differ from Go: {failed:?}");
}
