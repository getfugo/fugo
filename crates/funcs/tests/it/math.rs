//! `to_math` (feature `math`) against Go's `transform.ToMath` (KaTeX 0.16.22 with mhchem).
//! `tests/fixtures/tomath.jsonl.gz` holds Go's answers for KaTeX's screenshotter corpus, every
//! formula of the legacy docs site (`docs/content`, its passthrough delimiters), mhchem's manual,
//! KaTeX's options and their weak decoding (crates/funcs/README.md, "to_math fixture").

use std::collections::BTreeSet;

use serde::Deserialize;
use ssg_base::diag::Severity;
use tera::Context;

use crate::support::{Harness, env};

/// A fixture line: the arguments of `transform.ToMath` and Go's answer.
#[derive(Deserialize)]
struct Case {
    expression: String,
    /// The second argument (none: not given).
    options: Option<serde_json::Value>,
    output: String,
    warnings: Vec<String>,
    /// Go's error; `decode: …` for options mapstructure could not decode (our messages
    /// differ).
    err: String,
}

fn cases() -> Vec<Case> {
    ssg_testkit::fixture::read_jsonl(
        &ssg_testkit::fixture::repo_dir().join("crates/funcs/tests/fixtures/tomath.jsonl.gz"),
    )
    .expect("tests/fixtures/tomath.jsonl.gz")
}

/// Every case renders to Go's bytes, with Go's warnings, or fails with Go's error.
#[test]
fn matches_go() {
    let cases = cases();
    assert!(cases.len() > 700, "{} cases", cases.len());
    let (mut rendered, mut failed, mut warned) = (0, 0, 0);
    for case in &cases {
        // a fresh cache per case: each reports its warnings
        let env = env();
        let h = Harness::with_env(&env);
        let mut ctx = Context::new();
        ctx.insert("tex", &case.expression);
        let src = if let Some(options) = &case.options {
            ctx.insert("opts", options);
            "{{ tex | to_math(options=opts) }}"
        } else {
            "{{ tex | to_math }}"
        };
        let what = format!("{:?} {:?}", case.expression, case.options);
        match h.render(src, &ctx) {
            Ok(out) => {
                assert!(case.err.is_empty(), "{what}: Go failed with {}", case.err);
                assert_eq!(out, case.output, "{what}");
                let got: BTreeSet<String> = env
                    .diagnostics
                    .report()
                    .into_iter()
                    .map(|d| {
                        assert_eq!(d.severity, Severity::Warning, "{what}");
                        d.message
                    })
                    .collect();
                let want: BTreeSet<String> = case
                    .warnings
                    .iter()
                    .map(|w| format!("to_math: {w}"))
                    .collect();
                assert_eq!(got, want, "{what}");
                rendered += 1;
                warned += usize::from(!want.is_empty());
            }
            Err(e) => {
                assert!(
                    !case.err.is_empty(),
                    "{what}: Go rendered it, this port: {e}"
                );
                if let Some(go) = case.err.strip_prefix("decode: ") {
                    assert!(e.contains("to_math: option"), "{what}: {e} (Go: {go})");
                } else {
                    assert!(e.contains(&format!("to_math: {}", case.err)), "{what}: {e}");
                }
                failed += 1;
            }
        }
    }
    eprintln!(
        "to_math: {} cases as Go: {rendered} rendered ({warned} with warnings), {failed} errors",
        cases.len()
    );
}

/// Every formula of the documentation site renders as its passthrough render hook renders it,
/// mhchem included.
#[test]
fn renders_every_docs_formula() {
    let re = regex::Regex::new(r"(?s)\$\$(.+?)\$\$|\\\[(.+?)\\\]|\\\((.+?)\\\)").expect("valid");
    let docs = ssg_testkit::fixture::legacy_docs().join("content");
    let mut formulas = BTreeSet::new();
    let mut dirs = vec![docs];
    while let Some(dir) = dirs.pop() {
        for e in std::fs::read_dir(&dir).expect("docs dir").flatten() {
            let p = e.path();
            if p.is_dir() {
                dirs.push(p);
            } else if p.extension().is_some_and(|x| x == "md") {
                let text = std::fs::read_to_string(&p).expect("utf-8 markdown");
                for caps in re.captures_iter(&text) {
                    let (tex, display) = match (caps.get(1), caps.get(2), caps.get(3)) {
                        (Some(m), _, _) | (_, Some(m), _) => (m.as_str(), true),
                        (_, _, Some(m)) => (m.as_str(), false),
                        _ => continue,
                    };
                    // configuration listings (`['\[', '\]']`) and regular expressions are not math
                    let latex_like = tex.contains(['^', '_', '\\', '=']);
                    let listing = tex.contains(['\'', '"']) || tex.contains("\\|");
                    if latex_like && !listing {
                        formulas.insert((tex.to_owned(), display));
                    }
                }
            }
        }
    }
    assert!(formulas.len() >= 5, "found only {}", formulas.len());
    let h = Harness::new();
    let mut ctx = Context::new();
    let mut mhchem = 0;
    for (tex, display) in &formulas {
        ctx.insert("tex", tex);
        let src = format!(
            "{{{{ tex | to_math(options={{\"output\": \"htmlAndMathml\", \"displayMode\": {display}}}) }}}}"
        );
        let out = h
            .render(&src, &ctx)
            .unwrap_or_else(|e| panic!("{tex}: {e}"));
        let class = if *display {
            "<span class=\"katex-display\">"
        } else {
            "<span class=\"katex\">"
        };
        assert!(out.starts_with(class), "{tex}: {out}");
        assert!(
            out.contains("<annotation encoding=\"application/x-tex\">"),
            "{tex}: {out}"
        );
        assert!(!out.contains("katex-error"), "{tex}: {out}");
        mhchem += usize::from(tex.contains("\\ce{") || tex.contains("\\pu{"));
    }
    assert!(mhchem >= 1, "no mhchem formula among {}", formulas.len());
    eprintln!(
        "to_math: {} distinct docs formulas rendered, {mhchem} with mhchem",
        formulas.len()
    );
}

#[test]
fn inline_and_block() {
    let h = Harness::new();
    let ctx = Context::new();
    let inline = h.render("{{ 'a^2 + b^2 = c^2' | to_math }}", &ctx).unwrap();
    let block = h
        .render(
            "{{ '\\\\frac{1}{2}' | to_math(options={\"displayMode\": true}) }}",
            &ctx,
        )
        .unwrap();
    let both = h
        .render(
            "{{ '\\\\ce{H2O}' | to_math(options={\"output\": \"htmlAndMathml\"}) }}",
            &ctx,
        )
        .unwrap();
    ssg_testkit::snapshot::settings().bind(|| {
        insta::assert_yaml_snapshot!("to_math", [inline, block, both]);
    });
}

/// Go's `try (transform.ToMath …)`: `optional=true` turns an error into a warning (id
/// `to_math`, like `get_remote(optional=true)`) and the result into none.
#[test]
fn optional_turns_errors_into_warnings() {
    let env = env();
    let h = Harness::with_env(&env);
    let ctx = Context::new();
    let e = h
        .render("{{ '\\\\nosuchcommand{x}' | to_math }}", &ctx)
        .expect_err("an unknown command is an error");
    assert!(
        e.contains("to_math: KaTeX parse error: Undefined control sequence: \\nosuchcommand"),
        "{e}"
    );
    assert!(env.diagnostics.report().is_empty());
    let out = h
        .render(
            "{% set m = '\\\\nosuchcommand{x}' | to_math(optional=true) %}{{ m is none }}",
            &ctx,
        )
        .expect("optional");
    assert_eq!(out, "true");
    let report = env.diagnostics.report();
    assert_eq!(report.len(), 1, "{report:?}");
    assert_eq!(report[0].severity, Severity::Warning);
    assert_eq!(report[0].id.as_deref(), Some("to_math"));
    assert!(
        report[0]
            .message
            .starts_with("to_math: KaTeX parse error: Undefined control sequence"),
        "{}",
        report[0].message
    );
    // throwOnError false: KaTeX renders the unknown command in errorColor, no error
    let out = h
        .render(
            "{{ '\\\\nosuchcommand{x}' | to_math(options={\"throwOnError\": false}) }}",
            &ctx,
        )
        .unwrap();
    assert!(
        out.contains("<mstyle mathcolor=\"#cc0000\"><mtext>\\nosuchcommand</mtext>"),
        "{out}"
    );
    // invalid options are errors too
    let e = h
        .render("{{ 'x' | to_math(options={\"strict\": \"nope\"}) }}", &ctx)
        .unwrap_err();
    assert!(e.contains("to_math: invalid strict mode"), "{e}");
}

/// `strict: "warn"` reports KaTeX's warnings once per formula and build (Go's cache).
#[test]
fn strict_warnings_once() {
    let env = env();
    let h = Harness::with_env(&env);
    let ctx = Context::new();
    let src = "{{ 'é' | to_math(options={\"strict\": \"warn\"}) }}{{ 'é' | to_math(options={\"strict\": \"warn\"}) }}";
    h.render(src, &ctx).unwrap();
    let report = env.diagnostics.report();
    assert_eq!(report.len(), 1, "{report:?}");
    assert_eq!(
        report[0].message,
        "to_math: katex: LaTeX-incompatible input and strict mode is set to 'warn': \
         Accented Unicode text character \"é\" used in math mode [unicodeTextInMathMode]"
    );
}
