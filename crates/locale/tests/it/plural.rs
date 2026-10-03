//! CLDR plural rules (ICU4X) against go-i18n's generated rules (`oracle/i18n/plural`): a grid of
//! counts × 212 locales, and go-i18n's own CLDR sample tests. Locales whose rules changed in
//! CLDR since go-i18n's tables, or that ICU4X ships no rules for, are listed in
//! `expected_diffs.toml`.

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use serde_json::Value as J;
use ssg_locale::{PluralCount, PluralForm, PluralRules};

use crate::common::expected_diffs;

#[derive(Deserialize)]
struct Fixture {
    counts: Vec<J>,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    kind: String,
    locale: String,
    #[serde(default)]
    results: Vec<J>,
    #[serde(default)]
    tests: Vec<Sample>,
}

#[derive(Deserialize)]
struct Sample {
    form: String,
    v: J,
}

fn count_of(v: &J) -> Option<PluralCount> {
    match v {
        J::Number(n) => n.as_i64().map(PluralCount::from_int),
        J::String(s) => PluralCount::parse(s),
        _ => None,
    }
}

#[test]
fn plural_rules_oracle() {
    let fixture: Fixture = ssg_testkit::fixture::oracle("oracle/i18n/plural/plural.json.gz");
    let listed = expected_diffs().plural;
    let (mut compared, mut agreed) = (0usize, 0usize);
    let mut differing: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let mut not_other: BTreeSet<&str> = BTreeSet::new();
    for case in &fixture.cases {
        let rules = PluralRules::for_language(&case.locale);
        let pairs: Vec<(&J, &str)> = match case.kind.as_str() {
            "grid" => fixture
                .counts
                .iter()
                .zip(&case.results)
                .filter_map(|(c, r)| Some((c, r.get("s")?.as_str()?)))
                .filter(|(_, s)| !s.is_empty())
                .collect(),
            _ => case.tests.iter().map(|t| (&t.v, t.form.as_str())).collect(),
        };
        for (count, want) in pairs {
            let Some(count) = count_of(count) else {
                continue;
            };
            let got = rules.form(&count);
            let counted = !listed.no_icu_data.contains(&case.locale);
            compared += usize::from(counted);
            if got != PluralForm::Other {
                not_other.insert(&case.locale);
            }
            if got.as_str() == want {
                agreed += usize::from(counted);
            } else {
                differing
                    .entry(&case.locale)
                    .or_default()
                    .push(format!("{count}: {got} (Go {want})"));
            }
        }
    }
    let changed_or_missing = differing
        .keys()
        .filter(|l| listed.changed.contains_key(**l) || listed.no_icu_data.iter().any(|m| m == *l))
        .count();
    println!(
        "plural (locales with ICU data): {agreed}/{compared} agree ({:.2}%); {} locales differ, \
         all listed: {} CLDR changes, {} without ICU data",
        agreed as f64 * 100.0 / compared as f64,
        differing.len(),
        listed.changed.len(),
        listed.no_icu_data.len(),
    );
    let unlisted: Vec<_> = differing
        .iter()
        .filter(|(l, _)| {
            !listed.changed.contains_key(**l) && !listed.no_icu_data.iter().any(|m| m == *l)
        })
        .map(|(l, d)| format!("{l}: {}", d.join(", ")))
        .collect();
    assert!(unlisted.is_empty(), "unlisted:\n{}", unlisted.join("\n"));
    assert_eq!(changed_or_missing, differing.len());
    for l in &listed.no_icu_data {
        assert!(
            !not_other.contains(l.as_str()),
            "{l} has ICU plural rules after all"
        );
        assert!(
            differing.contains_key(l.as_str()),
            "stale no_icu_data entry {l}"
        );
    }
    for l in listed.changed.keys() {
        assert!(
            differing.contains_key(l.as_str()),
            "stale changed entry {l}"
        );
    }
    assert!(agreed * 100 >= compared * 95, "agreement below 95%");
}

#[test]
fn cldr_plural_basics() {
    let form = |lang: &str, n: &str| {
        PluralRules::for_language(lang)
            .form(&PluralCount::parse(n).unwrap())
            .as_str()
    };
    assert_eq!(form("en", "1"), "one");
    assert_eq!(form("en", "1.0"), "other");
    assert_eq!(form("en", "-1"), "one");
    assert_eq!(form("th", "1"), "other");
    assert_eq!(form("ja", "1"), "other");
    assert_eq!(form("fr", "0"), "one");
    assert_eq!(form("fr", "1.5"), "one");
    assert_eq!(form("pl", "22"), "few");
    assert_eq!(form("pl", "25"), "many");
    assert_eq!(form("ar", "0"), "zero");
    assert_eq!(form("ru", "21"), "one");
    assert_eq!(form("klingon", "1"), "other");
}
