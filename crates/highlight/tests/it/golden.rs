//! The Go implementation's HTML for a matrix of options (`tests/data/go-html.json`, from its
//! `markup/highlight` with Chroma v2.19.0; README): wrappers, line numbers in a table or
//! inline, highlighted lines, anchors, inline code, classes vs inline styles, styles and the
//! fallback style, languages Chroma does not know.

use serde::Deserialize;
use ssg_config::markup::HighlightConfig;
use ssg_highlight::{CssMode, Highlight, OptionsArg};
use ssg_testkit::fixture::{read_json, repo_dir};

#[derive(Deserialize)]
struct Case {
    lang: String,
    code: String,
    opts: String,
    html: String,
}

#[test]
fn go_html_matrix() {
    let cases: Vec<Case> =
        read_json(&repo_dir().join("crates/highlight/tests/data/go-html.json")).expect("fixture");
    let hl = Highlight::new(&HighlightConfig::default());
    let mut failures = Vec::new();
    for c in &cases {
        let ours = hl
            .highlight_with(&c.code, &c.lang, OptionsArg::Str(&c.opts))
            .expect("options");
        if ours != c.html {
            failures.push(format!(
                "lang {:?} opts {:?}\n--- go\n{}\n--- ours\n{}\n",
                c.lang, c.opts, c.html, ours
            ));
        }
    }
    println!(
        "go html: {}/{} equal",
        cases.len() - failures.len(),
        cases.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    let diags = hl.diagnostics();
    assert_eq!(diags.len(), 1, "one fallback warning (nosuchstyle)");
    assert!(diags[0].message.contains("nosuchstyle"));
}

/// The docs site's style sheet, as the Go program's `gen chromastyles --style=solarized-dark`
/// writes it (with and without `--omitEmpty`, minus the generator comment).
#[test]
fn solarized_dark_css() {
    let hl = Highlight::new(&HighlightConfig::default());
    for (mode, file) in [
        (CssMode::AllClasses, "solarized-dark.css"),
        (CssMode::OmitEmpty, "solarized-dark.omit-empty.css"),
    ] {
        let path = repo_dir().join("crates/highlight/tests/data").join(file);
        let want = std::fs::read_to_string(&path).expect("fixture");
        assert_eq!(
            hl.css("solarized-dark", mode).expect("style"),
            want,
            "{file}"
        );
    }
    assert!(hl.css("no-such-style", CssMode::AllClasses).is_err());
    assert_eq!(hl.style_names().count(), 67);
}

/// `FUGO_HL_LEXERS=1`: prints every Chroma lexer, in Chroma's registration order.
#[test]
fn print_lexers() {
    if std::env::var_os("FUGO_HL_LEXERS").is_none() {
        return;
    }
    let hl = Highlight::new(&HighlightConfig::default());
    for name in hl.lexer_names() {
        println!("{name}");
    }
}
