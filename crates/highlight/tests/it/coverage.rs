//! Docs corpus acceptance: every piece of code the docs highlight renders, the classes written
//! are Chroma's, the token classes are Chroma's (`tests/data/chroma-tokens.json.gz`) and the HTML
//! is the Go implementation's, byte for byte (`tests/data/legacy-docs-html.json.gz`, hashes);
//! see the crate README for how the fixtures are made.
//!
//! `FUGO_HL_PAIRS=1` prints the most frequent class differences per lexer;
//! `FUGO_HL_OURS=<file>` writes our HTML per item (JSON) for comparing with the Go output.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use ssg_config::markup::HighlightConfig;
use ssg_highlight::{Highlight, OptionsArg, TokenType};
use ssg_testkit::fixture::{read_json, repo_dir};

use crate::corpus::{self, Item, Opts};

#[derive(Deserialize)]
struct Tokens {
    key: String,
    lexer: String,
    #[serde(default)]
    runs: Vec<(String, usize)>,
}

#[derive(Deserialize)]
struct Html {
    key: String,
    hash: String,
}

/// The docs site's `[markup.highlight]` (from the legacy docs site's configuration file).
fn docs_config() -> HighlightConfig {
    HighlightConfig {
        line_numbers_in_table: false,
        no_classes: false,
        style: "solarized-dark".to_owned(),
        wrapper_class: "highlight not-prose".to_owned(),
        ..HighlightConfig::default()
    }
}

fn fixture<T: serde::de::DeserializeOwned>(name: &str) -> T {
    read_json(&repo_dir().join("crates/highlight/tests/data").join(name))
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// `FUGO_HL_DUMP=<file>`: writes the corpus as the oracle's input (README).
#[test]
fn dump_corpus() {
    let Some(path) = std::env::var_os("FUGO_HL_DUMP") else {
        return;
    };
    let items: Vec<_> = corpus::load()
        .into_iter()
        .map(|i| {
            serde_json::json!({
                "key": i.key, "html_key": i.html_key, "lang": i.lang, "code": i.code,
                "opts": i.options.to_json(),
            })
        })
        .collect();
    let text = serde_json::to_string(&items).expect("json");
    std::fs::write(path, text).expect("write");
}

/// The class family used for "same kind of token": the category, with strings, numbers and
/// other literals apart.
fn family(t: TokenType) -> Option<TokenType> {
    let c = t.category()?;
    if c == TokenType::Literal {
        t.sub_category()
    } else {
        Some(c)
    }
}

fn type_of_class(class: &str) -> Option<TokenType> {
    TokenType::ALL
        .iter()
        .copied()
        .find(|t| t.is_standard() && t.class() == class)
}

/// Characters compared: not whitespace, and classified by Chroma (not text or whitespace).
#[derive(Default)]
struct Tally {
    chars: usize,
    /// We write a class too.
    classified: usize,
    /// Same [`family`].
    same_family: usize,
    /// Same class.
    same_class: usize,
}

impl Tally {
    fn add(&mut self, o: &Self) {
        self.chars += o.chars;
        self.classified += o.classified;
        self.same_family += o.same_family;
        self.same_class += o.same_class;
    }
}

fn pct(n: usize, d: usize) -> f64 {
    #[expect(clippy::cast_precision_loss, reason = "a percentage")]
    let p = if d == 0 {
        100.0
    } else {
        100.0 * n as f64 / d as f64
    };
    p
}

type Pairs = BTreeMap<(String, String, String), usize>;

/// Per character: Chroma's class against ours.
fn compare(ours: &[(TokenType, String)], o: &Tokens, pairs: &mut Pairs) -> Tally {
    let theirs = o
        .runs
        .iter()
        .flat_map(|(class, n)| std::iter::repeat_n(class.as_str(), *n));
    let mine = ours
        .iter()
        .flat_map(|(t, text)| text.chars().map(move |c| (*t, c)));
    let mut tally = Tally::default();
    for (class, (t, c)) in theirs.zip(mine) {
        if c.is_whitespace() || class.is_empty() || class == "w" {
            continue;
        }
        tally.chars += 1;
        let mine = t.class();
        if !mine.is_empty() && mine != "w" {
            tally.classified += 1;
        }
        if type_of_class(class).and_then(family) == family(t) {
            tally.same_family += 1;
        }
        if mine == class {
            tally.same_class += 1;
        } else {
            let key = (o.lexer.clone(), class.to_owned(), mine.to_owned());
            *pairs.entry(key).or_default() += 1;
        }
    }
    tally
}

/// The docs' call: `transform.Highlight` with the fence's options or the shortcode's string.
fn render(hl: &Highlight, item: &Item) -> String {
    let opts = match &item.options {
        Opts::Map(m) => OptionsArg::Map(m),
        Opts::Str(s) => OptionsArg::Str(s),
    };
    hl.highlight_with(&item.code, &item.lang, opts)
        .unwrap_or_else(|e| panic!("{}: {e}", item.file))
}

#[test]
fn docs_corpus() {
    let items = corpus::load();
    let started = std::time::Instant::now();
    let hl = Highlight::new(&docs_config());
    let loaded = started.elapsed();
    let started = std::time::Instant::now();
    for item in &items {
        render(&hl, item);
    }
    println!(
        "Highlight::new {loaded:?}; {} items rendered in {:?}",
        items.len(),
        started.elapsed()
    );
    let tokens: Vec<Tokens> = fixture("chroma-tokens.json.gz");
    let tokens: BTreeMap<&str, &Tokens> = tokens.iter().map(|o| (o.key.as_str(), o)).collect();
    let go_html: Vec<Html> = fixture("legacy-docs-html.json.gz");
    let go_html: BTreeMap<&str, &str> = go_html
        .iter()
        .map(|h| (h.key.as_str(), h.hash.as_str()))
        .collect();

    let chroma_classes: BTreeSet<&str> = TokenType::classes().collect();
    let mut by_lexer: BTreeMap<String, Tally> = BTreeMap::new();
    let mut total = Tally::default();
    let mut pairs = Pairs::new();
    let (mut known, mut identical, mut compared) = (0, 0, 0);
    let mut unknown: BTreeMap<&str, usize> = BTreeMap::new();
    let mut by_source: BTreeMap<String, usize> = BTreeMap::new();
    let mut ours_html = BTreeMap::new();
    for item in &items {
        *by_source.entry(format!("{:?}", item.source)).or_default() += 1;
        let html = render(&hl, item);
        for class in classes_in(&html) {
            assert!(
                chroma_classes.contains(class) || is_wrapper_class(class),
                "{}: class {class:?} is not Chroma's",
                item.file
            );
        }
        if hl.can_highlight(&item.lang) {
            known += 1;
            // Go's wrapper, not the plain `<pre>` of an unknown language.
            assert!(!html.starts_with("<pre"), "{}: {html}", item.file);
        } else {
            *unknown.entry(&item.lang).or_default() += 1;
        }
        ours_html.insert(item.html_key.clone(), html.clone());
        if let Some(want) = go_html.get(item.html_key.as_str()) {
            compared += 1;
            if corpus::fnv(&[&html]) == *want {
                identical += 1;
            }
        }
        let (Some(o), Some(ours)) = (
            tokens.get(item.key.as_str()),
            hl.tokens(&item.code, &item.lang),
        ) else {
            continue;
        };
        let t = compare(&ours, o, &mut pairs);
        by_lexer.entry(o.lexer.clone()).or_default().add(&t);
        total.add(&t);
    }

    if let Some(path) = std::env::var_os("FUGO_HL_OURS") {
        std::fs::write(path, serde_json::to_string(&ours_html).expect("json")).expect("write");
    }
    println!("items {} {by_source:?}", items.len());
    println!("known language {known}, unknown {unknown:?}");
    println!("byte-identical to Go: {identical}/{compared}");
    println!("| lexer | chars | classified % | same family % | same class % |");
    println!("|---|---|---|---|---|");
    for (lexer, t) in by_lexer.iter().chain([(&"all".to_owned(), &total)]) {
        println!(
            "| {lexer} | {} | {:.1} | {:.1} | {:.1} |",
            t.chars,
            pct(t.classified, t.chars),
            pct(t.same_family, t.chars),
            pct(t.same_class, t.chars)
        );
    }
    if std::env::var_os("FUGO_HL_PAIRS").is_some() {
        let mut pairs: Vec<_> = pairs.into_iter().collect();
        pairs.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
        for ((lexer, theirs, mine), n) in pairs.iter().take(60) {
            println!("{lexer:>20} chroma {theirs:>4} ours {mine:>4}: {n}");
        }
    }

    // Everything renders; only languages Chroma does not know stay plain (as in Go).
    assert_eq!(known + unknown.values().sum::<usize>(), items.len());
    assert!(items.len() > 2200, "corpus: {}", items.len());
    assert!(unknown.values().sum::<usize>() <= 1, "{unknown:?}");
    // Go's HTML, byte for byte, and Chroma's token classes, character for character.
    assert!(
        compared > 2200 && identical == compared,
        "{identical}/{compared}"
    );
    assert_eq!(total.classified, total.chars);
    assert_eq!(total.same_class, total.chars);
}

/// Wrapper classes the Go implementation writes around Chroma's output.
fn is_wrapper_class(class: &str) -> bool {
    matches!(class, "highlight" | "not-prose" | "code-inline") || class.starts_with("language-")
}

/// The class names of every `class="…"` attribute.
fn classes_in(html: &str) -> impl Iterator<Item = &str> {
    html.split("class=\"")
        .skip(1)
        .filter_map(|s| s.split_once('"'))
        .flat_map(|(v, _)| v.split_whitespace())
}
