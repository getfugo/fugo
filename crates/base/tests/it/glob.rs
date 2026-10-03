//! `glob` against the `common/glob` oracle (gobwas/glob as the Go implementation uses it).

use std::collections::BTreeSet;

use serde_json::Value as J;
use ssg_base::glob::{self, Case, GlobOpts, Separator};

use crate::support::{Tally, fixture, text};

/// The patterns gobwas mis-matches (`expected_diffs.toml`, `[glob]`).
fn gobwas_bugs() -> BTreeSet<String> {
    let path = ssg_testkit::fixture::repo_dir().join("crates/base/expected_diffs.toml");
    let doc: toml::Table = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    doc["glob"]
        .as_table()
        .unwrap()
        .values()
        .flat_map(|v| v.as_array().unwrap())
        .map(|e| e[0].as_str().unwrap().to_owned())
        .collect()
}

/// The configurations the oracle's `raw` results use, by separator list: only `""` (no
/// separator) and `"/"` exist in Go.
fn raw_opts(separators: &str) -> Option<GlobOpts> {
    let separator = match separators {
        "" => Separator::None,
        "/" => Separator::Slash,
        _ => return None,
    };
    Some(GlobOpts {
        case: Case::Sensitive,
        separator,
    })
}

fn check_bits(
    t: &mut Tally,
    bugs: &BTreeSet<String>,
    what: &str,
    pattern: &str,
    opts: GlobOpts,
    inputs: &[Option<&str>],
    want: &J,
) {
    let compiled = glob::compile(pattern, opts);
    let Some(bits) = want.as_str() else {
        t.check(compiled.is_err(), || {
            format!("{what} {pattern:?}: want a compile error")
        });
        return;
    };
    let g = match compiled {
        Ok(g) => g,
        Err(e) => {
            t.check(false, || {
                format!("{what} {pattern:?}: unexpected error {e}")
            });
            return;
        }
    };
    for (input, bit) in inputs.iter().zip(bits.bytes()) {
        let Some(input) = input else {
            t.skip(|| format!("{what} {pattern:?}: input is not UTF-8"));
            continue;
        };
        let got = g.is_match(input);
        if got != (bit == b'1') && bugs.contains(pattern) {
            t.deviation(|| format!("{what} {pattern:?} on {input:?}: gobwas bug"));
            continue;
        }
        t.check(got == (bit == b'1'), || {
            format!(
                "{what} {} on {}: want {}, got {got}",
                J::from(pattern),
                J::from(*input),
                bit == b'1'
            )
        });
    }
}

#[test]
fn match_oracle() {
    let f = fixture("oracle/common/glob/match.json.gz");
    let inputs: Vec<Option<&str>> = f["inputs"].as_array().unwrap().iter().map(text).collect();
    let separators: Vec<&str> = f["separators"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap())
        .collect();
    let bugs = gobwas_bugs();
    let mut t = Tally::new("glob/match");
    for c in f["cases"].as_array().unwrap() {
        let Some(pattern) = text(&c["pattern"]) else {
            t.skip(|| format!("{}: pattern is not UTF-8", c["pattern"]));
            continue;
        };
        if pattern.contains('\u{fffd}') {
            // gobwas rejects U+FFFD in a pattern (it cannot tell it from a decoding error).
            t.deviation(|| format!("{pattern:?}: U+FFFD is an ordinary character here"));
            continue;
        }
        check_bits(
            &mut t,
            &bugs,
            "getGlob",
            pattern,
            GlobOpts::default(),
            &inputs,
            // The recorded Go data names the `GetGlob` results by the Go program's name.
            &c["hugo"],
        );
        for (sep, want) in separators.iter().zip(c["raw"].as_array().unwrap()) {
            match raw_opts(sep) {
                Some(o) => {
                    check_bits(
                        &mut t,
                        &bugs,
                        &format!("raw[{sep}]"),
                        pattern,
                        o,
                        &inputs,
                        want,
                    );
                }
                None => t.skip(|| format!("{pattern:?}: separator set {sep:?} not supported")),
            }
        }
    }
    t.finish();
}

#[test]
fn compile_oracle() {
    let f = fixture("oracle/common/glob/compile.json.gz");
    let mut t = Tally::new("glob/compile");
    for c in f["cases"].as_array().unwrap() {
        let Some(pattern) = text(&c["pattern"]) else {
            t.skip(|| format!("{}: pattern is not UTF-8", c["pattern"]));
            continue;
        };
        if pattern.contains('\u{fffd}') {
            t.deviation(|| format!("{pattern:?}: U+FFFD is an ordinary character here"));
            continue;
        }
        let want_ok = c["getGlob"].get("ok").is_some();
        let got = glob::compile(pattern, GlobOpts::default());
        t.check(got.is_ok() == want_ok, || {
            format!("{pattern:?}: want ok={want_ok}, got {:?}", got.err())
        });
    }
    t.finish();
}

#[test]
fn go_patterns() {
    let g = |p: &str| glob::compile(p, GlobOpts::default()).unwrap();
    assert!(g("**.json").is_match("a/b/c.JSON"));
    assert!(g("**/_index.md").is_match("a/_index.md"));
    assert!(!g("**/_index.md").is_match("_index.md"));
    assert!(g("images/**.{png,jpg}").is_match("images/a/b.jpg"));
    assert!(!g("images/*.{png,jpg}").is_match("images/a/b.jpg"));
    assert!(g("{a{b,c},d}").is_match("ac"));
    assert!(g("[!a]").is_match("/"));
    assert!(g("[]-a]").is_match("^"));
    assert!(glob::compile("[!]", GlobOpts::default()).is_err());
    assert!(glob::compile("[a-", GlobOpts::default()).is_err());
    assert!(glob::compile("[b-a]", GlobOpts::default()).is_err());
}
