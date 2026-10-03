//! Shared helpers: fixture values and the reviewed expected differences.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value as J;
use ssg_base::{Map, Value};

/// `crates/locale/expected_diffs.toml`.
#[derive(Debug, Deserialize)]
pub struct ExpectedDiffs {
    pub translate: TranslateDiffs,
    pub plural: PluralDiffs,
    pub locales: LocalesDiffs,
    pub parse: ParseDiffs,
}

#[derive(Debug, Deserialize)]
pub struct TranslateDiffs {
    /// Go argument types with no native counterpart (not compared).
    pub go_only_argtypes: Vec<String>,
    /// Keys whose messages use syntax outside `{{ . }}` / `{{ .Field }}` (load errors here).
    pub unsupported_syntax: Vec<String>,
    /// Accepted classes of deviation (classified by the test) → reason.
    pub rules: BTreeMap<String, String>,
    /// Accepted single deviations: `case lang id argtype arg` → reason.
    pub accepted: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct PluralDiffs {
    /// Locales without plural rules in ICU4X's compiled data (root rules apply).
    pub no_icu_data: Vec<String>,
    /// Locales whose rules changed in CLDR since go-i18n's tables → reason.
    pub changed: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct LocalesDiffs {
    /// `<lang> <field>` → reason, for whole fields that differ from the Go locales library.
    pub fields: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct ParseDiffs {
    /// Fixture case index → reason.
    pub cases: BTreeMap<String, String>,
}

pub fn expected_diffs() -> ExpectedDiffs {
    let path = ssg_testkit::fixture::repo_dir().join("crates/locale/expected_diffs.toml");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    toml::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// A fixture argument as a native value: JSON numbers follow the Go type (`argtype`), Go
/// structs and `maps.Params` become maps, `template.HTML` a string.
pub fn value_of(arg: &J, argtype: &str) -> Value {
    match arg {
        J::Null => Value::Null,
        J::Bool(b) => Value::Bool(*b),
        J::Number(n) if argtype.starts_with("float") => Value::Float(n.as_f64().unwrap()),
        J::Number(n) => n
            .as_i64()
            .map_or_else(|| Value::Float(n.as_f64().unwrap()), Value::Int),
        J::String(s) => Value::string(s),
        J::Array(a) => Value::array(a.iter().map(|v| value_of(v, "")).collect()),
        J::Object(o) => match o.get("t").and_then(J::as_str) {
            Some("html") => value_of(&o["v"], ""),
            Some("params") => value_of(&o["v"], ""),
            Some("struct") => value_of(&o["fields"], ""),
            _ => Value::map(o.iter().map(|(k, v)| (k.as_str(), value_of(v, ""))).fold(
                Map::new(),
                |mut m, (k, v)| {
                    m.insert(k, v);
                    m
                },
            )),
        },
    }
}
