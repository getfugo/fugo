//! Chroma's own lexer test suite (`lexers/testdata` of Chroma v2.19.0,
//! `tests/data/chroma-testdata.json.gz`, README "Fixtures"): every input with the lexer
//! Chroma's `TestLexers` picks, our coalesced tokens against the hash of Chroma's `*.expected`
//! tokens; and for every input, the lexer `guessSyntax` picks against Chroma's
//! `lexers.Analyse`.

use serde::Deserialize;
use ssg_config::markup::HighlightConfig;
use ssg_highlight::{Highlight, OptionsArg};
use ssg_testkit::fixture::{read_json, repo_dir};

use crate::corpus::fnv;

#[derive(Deserialize)]
struct Case {
    file: String,
    lexer: String,
    code: String,
    /// The hash of Chroma's tokens (absent for the analysis inputs).
    hash: Option<String>,
    /// The lexer Chroma's `lexers.Analyse` picks (`""`: none).
    analyse: String,
}

fn cases() -> Vec<Case> {
    let path = repo_dir().join("crates/highlight/tests/data/chroma-testdata.json.gz");
    let cases: Vec<Case> = read_json(&path).expect("fixture");
    assert!(cases.len() > 300, "{}", cases.len());
    cases
}

#[test]
fn chroma_lexer_test_suite() {
    let cases = cases();
    let hl = Highlight::new(&HighlightConfig::default());
    let mut differ = Vec::new();
    let mut n = 0;
    for c in &cases {
        let Some(hash) = &c.hash else { continue };
        n += 1;
        let tokens = hl
            .tokens(&c.code, &c.lexer)
            .unwrap_or_else(|| panic!("{}: no lexer {:?}", c.file, c.lexer));
        let parts: Vec<&str> = tokens
            .iter()
            .flat_map(|(t, v)| [t.name(), v.as_str()])
            .collect();
        if fnv(&parts) != *hash {
            differ.push(c.file.as_str());
        }
    }
    println!(
        "Chroma's lexer tests: {}/{n} token streams identical",
        n - differ.len()
    );
    // Raku's lexer is Go code and is not ported (README, deviations).
    assert_eq!(
        differ,
        ["raku/raku.actual", "raku/unterminated_heredoc.actual"]
    );
}

/// `guessSyntax`: the lexer whose analyser scores highest (Chroma's registration order breaks
/// ties), else Chroma's fallback lexer; the language written is its lower-cased name.
#[test]
fn guess_syntax_follows_chromas_analysers() {
    let cases = cases();
    let hl = Highlight::new(&HighlightConfig::default());
    let mut differ = Vec::new();
    for c in &cases {
        let html = hl
            .highlight_with(&c.code, "", OptionsArg::Str("guessSyntax=true"))
            .expect("highlight");
        let want = if c.analyse.is_empty() {
            "fallback".to_owned()
        } else {
            c.analyse.to_lowercase()
        };
        if !html.contains(&format!("data-lang=\"{want}\"")) {
            differ.push(format!("{}: want {want:?}", c.file));
        }
    }
    assert!(differ.is_empty(), "{}", differ.join("\n"));
}
