//! The summary oracle (`testdata/oracle/page/summary`): Go's summary of the rendered
//! HTML of every page of the Go builds and of variants (other word counts, CJK, dividers), plus
//! 6,000 adversarial calls, against [`ssg_render::summary`]. Markdown and HTML content
//! only (AsciiDoc, RST, Pandoc and Org are external markups this port does not render).
//!
//! Compared: `.Summary`, `.Content` and `.Truncated`. Long strings are compared by SHA-256.

use serde_json::Value as J;
use sha2::{Digest, Sha256};
use ssg_render::summary::{self, Split};

fn digest(s: &str) -> J {
    if s.len() <= 160 {
        return J::from(s);
    }
    let sum = Sha256::digest(s.as_bytes());
    let hex: String = sum.iter().map(|b| format!("{b:02x}")).collect();
    serde_json::json!({ "len": s.len(), "sha256": hex })
}

#[test]
fn summary_oracle() {
    let mut total = 0;
    let mut equal = 0;
    let mut skipped = 0;
    let mut unexpected = Vec::new();
    for file in ["build", "adversarial"] {
        let fx: J = ssg_testkit::fixture::oracle(&format!("oracle/page/summary/{file}.json.gz"));
        let types: Vec<String> = fx["types"]
            .as_array()
            .expect("types")
            .iter()
            .map(|t| t["subType"].as_str().unwrap_or_default().to_owned())
            .collect();
        let inputs = fx["inputs"].as_array().expect("inputs");
        for c in fx["cases"].as_array().expect("cases") {
            let mt = &types[usize::try_from(c["mt"].as_u64().expect("mt")).expect("mt")];
            let input = &inputs[usize::try_from(c["input"].as_u64().expect("input")).expect("i")];
            let (Some(input), Some(want)) = (input.as_str(), c["want"].get("ok")) else {
                skipped += 1; // not UTF-8, or a Go runtime panic
                continue;
            };
            if mt != "markdown" && mt != "html" {
                skipped += 1;
                continue;
            }
            total += 1;
            let got = if c["kind"] == "manual" {
                let divider = c["divider"].as_str().expect("divider");
                summary::manual(input, divider, mt == "markdown").unwrap_or_else(|| Split {
                    content: input.to_owned(),
                    summary: String::new(),
                    truncated: !input.is_empty(),
                })
            } else {
                let words = usize::try_from(c["numWords"].as_i64().unwrap_or(0)).unwrap_or(0);
                summary::auto(input, words, c["isCJK"].as_bool().unwrap_or(false))
            };
            let same = want["summary"] == digest(&got.summary)
                && want["content"] == digest(&got.content)
                && want["truncated"] == got.truncated;
            if same {
                equal += 1;
            } else {
                unexpected.push(format!(
                    "{file} {} {mt} words {} divider {:?}: input {:?}\n   got  {:?} / {:?} / {}\n   want {} / {} / {}",
                    c["kind"],
                    c["numWords"],
                    c["divider"],
                    &input[..input.len().min(200)],
                    got.summary,
                    got.content,
                    got.truncated,
                    want["summary"],
                    want["content"],
                    want["truncated"]
                ));
            }
        }
    }
    eprintln!(
        "summary oracle: {total} Markdown/HTML cases, {equal} equal, {skipped} other markups or not UTF-8"
    );
    for u in unexpected.iter().take(20) {
        eprintln!("{u}");
    }
    assert!(unexpected.is_empty(), "{} unexpected", unexpected.len());
    assert!(total > 11_000 && equal == total);
}
