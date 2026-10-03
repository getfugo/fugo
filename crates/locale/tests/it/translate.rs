//! The Go `i18n` oracle (`oracle/i18n/translate`): 61 sites × their calls.
//!
//! Each site's i18n files are selected like Go does (theme files first, dot files and
//! `ignoreFiles` skipped), loaded with [`TranslationsBuilder`], and every call is rendered with
//! [`Translations::translate`] (an error renders as nothing, as in Go). Not compared: calls
//! that crash Go, Go-only argument types, and messages with syntax the evaluator rejects (they
//! are load errors, checked separately). The rest must agree at ≥ 99%, and every remaining
//! difference must be listed in `expected_diffs.toml`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Deserialize;
use serde_json::Value as J;
use ssg_base::{Idx as _, LangIdx, Value};
use ssg_locale::{Args, I18nError, MessageFile, MessageProblem, TranslationsBuilder};

use crate::common::{expected_diffs, value_of};

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Site>,
}

#[derive(Deserialize)]
struct Site {
    name: String,
    toml: String,
    files: Option<BTreeMap<String, String>>,
    #[serde(default)]
    calls: Vec<Call>,
    err: Option<String>,
}

#[derive(Deserialize)]
struct Call {
    arg: J,
    argtype: String,
    id: String,
    lang: String,
    out: J,
}

struct SiteConfig {
    default_lang: String,
    placeholders: bool,
    languages: Vec<String>,
    ignore: Vec<regex::Regex>,
}

fn site_config(toml_src: &str) -> SiteConfig {
    let t: toml::Table = toml::from_str(toml_src).unwrap();
    let default_lang = t
        .get("defaultContentLanguage")
        .and_then(toml::Value::as_str)
        .unwrap_or("en")
        .to_owned();
    let mut languages: Vec<String> = t
        .get("languages")
        .and_then(toml::Value::as_table)
        .map(|l| l.keys().cloned().collect())
        .unwrap_or_default();
    if !languages.contains(&default_lang) {
        languages.push(default_lang.clone());
    }
    let ignore = t
        .get("ignoreFiles")
        .and_then(toml::Value::as_array)
        .map(|a| {
            a.iter()
                .map(|p| regex::Regex::new(p.as_str().unwrap()).unwrap())
                .collect()
        })
        .unwrap_or_default();
    SiteConfig {
        default_lang,
        placeholders: t
            .get("enableMissingTranslationPlaceholders")
            .and_then(toml::Value::as_bool)
            .unwrap_or(false),
        languages,
        ignore,
    }
}

/// The site's i18n files in load order: theme files, then the project's, each by path.
fn i18n_files<'a>(
    files: &'a BTreeMap<String, String>,
    cfg: &SiteConfig,
) -> Vec<(&'a str, &'a str)> {
    let wanted = |path: &str| {
        let name = path.rsplit('/').next().unwrap();
        !name.starts_with('.') && !cfg.ignore.iter().any(|r| r.is_match(path))
    };
    let mut theme: Vec<_> = files
        .iter()
        .filter(|(p, _)| p.starts_with("themes/") && p.contains("/i18n/"))
        .collect();
    let mut project: Vec<_> = files
        .iter()
        .filter(|(p, _)| p.starts_with("i18n/"))
        .collect();
    theme.sort();
    project.sort();
    theme
        .into_iter()
        .chain(project)
        .filter(|(p, _)| wanted(p))
        .map(|(p, c)| (p.as_str(), c.as_str()))
        .collect()
}

struct Loaded {
    translations: ssg_locale::Translations,
    /// Keys dropped because their message uses unsupported syntax.
    unsupported: BTreeSet<String>,
}

/// Loads a site message by message, so that a message with unsupported syntax is reported and
/// skipped instead of failing the whole site.
fn load(site: &Site, cfg: &SiteConfig) -> Result<Loaded, I18nError> {
    let mut builder =
        TranslationsBuilder::new(&cfg.default_lang).missing_placeholders(cfg.placeholders);
    let mut unsupported = BTreeSet::new();
    let empty = BTreeMap::new();
    for (path, content) in i18n_files(site.files.as_ref().unwrap_or(&empty), cfg) {
        let path = Path::new(path);
        let file = MessageFile::read(path, content)?;
        for m in file.messages {
            let one = MessageFile {
                lang: file.lang.clone(),
                messages: vec![m],
            };
            match builder.add(path, one) {
                Ok(()) => {}
                Err(I18nError::Message {
                    key,
                    problem: MessageProblem::Syntax { .. },
                    ..
                }) => {
                    unsupported.insert(key);
                }
                Err(e) => return Err(e),
            }
        }
    }
    let translations = builder.build(cfg.languages.iter().map(String::as_str));
    Ok(Loaded {
        translations,
        unsupported,
    })
}

#[test]
fn translate_oracle() {
    let fixture: Fixture = ssg_testkit::fixture::oracle("oracle/i18n/translate/translate.json.gz");
    let diffs = expected_diffs().translate;
    let (mut compared, mut agreed, mut accepted) = (0usize, 0usize, 0usize);
    let mut skipped: BTreeMap<&str, usize> = BTreeMap::new();
    let mut failures = Vec::new();
    let mut unused: BTreeSet<&String> = diffs.accepted.keys().collect();
    let mut by_rule: BTreeMap<&str, usize> = BTreeMap::new();
    for site in &fixture.cases {
        let cfg = site_config(&site.toml);
        let loaded = match (load(site, &cfg), &site.err) {
            (Ok(l), None) => l,
            (Err(_), Some(_)) => continue,
            (Ok(_), Some(e)) => {
                // Go rejects `zh.Hans` as a language tag; we reject it as a file name too.
                failures.push(format!("{}: loads, Go fails with {e}", site.name));
                continue;
            }
            (Err(e), None) => {
                failures.push(format!("{}: load error {e}", site.name));
                continue;
            }
        };
        for key in &loaded.unsupported {
            assert!(
                diffs.unsupported_syntax.contains(key),
                "{}: `{key}` rejected as unsupported syntax but not listed",
                site.name
            );
        }
        for call in &site.calls {
            let J::String(want) = &call.out else {
                *skipped.entry("go panic").or_default() += 1;
                continue;
            };
            if diffs.go_only_argtypes.contains(&call.argtype) {
                *skipped.entry("go-only argument type").or_default() += 1;
                continue;
            }
            if loaded.unsupported.contains(&call.id) {
                *skipped.entry("unsupported message syntax").or_default() += 1;
                continue;
            }
            let lang = cfg
                .languages
                .iter()
                .position(|l| *l == call.lang)
                .map_or_else(
                    || panic!("{}: language {}", site.name, call.lang),
                    LangIdx::from_index,
                );

            let arg = value_of(&call.arg, &call.argtype);
            let got = loaded
                .translations
                .translate(lang, &call.id, &Args::from_value(&arg))
                .unwrap_or_default();
            compared += 1;
            if got == *want {
                agreed += 1;
                continue;
            }
            let label = format!(
                "{} {} {} {} {}",
                site.name, call.lang, call.id, call.argtype, call.arg
            );
            if let Some(rule) = classify(&arg, &call.argtype, &got, want) {
                assert!(
                    diffs.rules.contains_key(rule),
                    "rule {rule} not in expected_diffs"
                );
                *by_rule.entry(rule).or_default() += 1;
                accepted += 1;
            } else if diffs.accepted.contains_key(&label) {
                accepted += 1;
                unused.remove(&label);
            } else {
                failures.push(format!("{label:?}: got {got:?}, Go {want:?}"));
            }
        }
    }
    let rate = agreed as f64 / compared as f64;
    let with_accepted = (agreed + accepted) as f64 / compared as f64;
    println!(
        "translate: {agreed}/{compared} agree ({:.2}%); with {accepted} accepted deviations \
         {:.2}% {by_rule:?}; not compared {skipped:?}",
        rate * 100.0,
        with_accepted * 100.0
    );
    assert!(
        failures.is_empty(),
        "{} unlisted differences:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(unused.is_empty(), "stale accepted entries: {unused:?}");
    assert!(
        with_accepted >= 0.99,
        "agreement {with_accepted:.4} (raw {rate:.4}) below 99%"
    );
}

/// The accepted class of a difference, if any (rules documented in `expected_diffs.toml`).
fn classify(arg: &Value, argtype: &str, got: &str, want: &str) -> Option<&'static str> {
    let is_bool_count = |v: &Value| match v {
        Value::Bool(_) => true,
        Value::Map(m) => m.values().any(|v| matches!(v, Value::Bool(_))),
        _ => false,
    };
    match arg {
        Value::Map(_) | Value::Array(_) if got.is_empty() && !want.is_empty() => {
            Some("print-collection")
        }
        v if is_bool_count(v) => Some("bool-count"),
        Value::String(s) if s.contains('e') && s.chars().any(|c| c.is_ascii_digit()) => {
            Some("exponent-count")
        }
        Value::Float(_) | Value::String(_) if want.is_empty() && !got.is_empty() => {
            Some("scalar-count-field")
        }
        Value::Int(_) if argtype == "uint" && want.is_empty() && !got.is_empty() => {
            Some("scalar-count-field")
        }
        _ => None,
    }
}

#[test]
fn go_load_errors_are_load_errors() {
    let fixture: Fixture = ssg_testkit::fixture::oracle("oracle/i18n/translate/translate.json.gz");
    for site in fixture.cases.iter().filter(|s| s.err.is_some()) {
        let cfg = site_config(&site.toml);
        let err = load(site, &cfg).err();
        let err = err.unwrap_or_else(|| panic!("{} loads; Go: {:?}", site.name, site.err));
        let files = site.files.as_ref().unwrap();
        let named = files.keys().any(|p| err.to_string().contains(p.as_str()));
        assert!(named, "{}: error does not name the file: {err}", site.name);
    }
}

#[test]
fn value_conversion_follows_argtype() {
    assert_eq!(
        value_of(&serde_json::json!(1.0), "float64"),
        Value::Float(1.0)
    );
    assert_eq!(value_of(&serde_json::json!(1), "int"), Value::Int(1));
}
