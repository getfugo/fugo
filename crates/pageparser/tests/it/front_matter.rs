//! Front matter decoding against the Go implementation's `ParseFrontMatterAndContent`
//! (`parser/pageparser/pages.json.gz`).

use serde_json::Value as J;
use ssg_base::{Date, Map, Value};
use ssg_pageparser::{
    FrontMatterFormat, decode_front_matter, decode_front_matter_map, split_front_matter,
};
use ssg_testkit::fixture::Tag;

use crate::support::{Tally, expected_diffs, page_cases};

/// How numbers compare.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Numbers {
    /// An integer only equals an integer, a float a float.
    Typed,
    /// An integer equals the float with its value (Go decodes JSON numbers as floats).
    ByValue,
}

/// Whether our value equals Go's (plain JSON with this port tags). Go integers beyond `i64`
/// are compared as floats (base's rule for them).
fn same(ours: &Value, go: &J, numbers: Numbers) -> bool {
    if let Some(tag) = Tag::of(go) {
        return match (tag, ours) {
            (Tag::Time(s), Value::Date(Date::Zoned(z))) => s
                .parse::<jiff::Timestamp>()
                .is_ok_and(|t| t == z.timestamp()),
            (Tag::Local(s), Value::Date(Date::Local(dt))) => {
                s.parse::<jiff::civil::DateTime>().is_ok_and(|g| g == *dt)
            }
            (Tag::Local(s), Value::String(o)) => s == &**o,
            (Tag::Float(g), Value::Float(f)) => g == *f || (g.is_nan() && f.is_nan()),
            _ => false,
        };
    }
    match (ours, go) {
        (Value::Null, J::Null) => true,
        (Value::Bool(a), J::Bool(b)) => a == b,
        (Value::String(a), J::String(b)) => **a == **b,
        (Value::Int(a), J::Number(n)) if numbers == Numbers::ByValue => {
            #[expect(clippy::cast_precision_loss, reason = "compared as Go's float")]
            let a = *a as f64;
            n.as_f64() == Some(a)
        }
        (Value::Int(a), J::Number(n)) => n.as_i64() == Some(*a) && !n.is_f64(),
        (Value::Float(a), J::Number(n)) => {
            (n.is_f64() || n.as_i64().is_none()) && n.as_f64() == Some(*a)
        }
        (Value::Array(a), J::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| same(x, y, numbers))
        }
        (Value::Map(a), J::Object(b)) => same_map(a, b, numbers),
        _ => false,
    }
}

fn same_map(ours: &Map, go: &serde_json::Map<String, J>, numbers: Numbers) -> bool {
    ours.len() == go.len()
        && go
            .iter()
            .all(|(k, v)| ours.get(k).is_some_and(|o| same(o, v, numbers)))
}

fn format_of(name: &str) -> Option<FrontMatterFormat> {
    match name {
        "yaml" => Some(FrontMatterFormat::Yaml),
        "toml" => Some(FrontMatterFormat::Toml),
        "json" => Some(FrontMatterFormat::Json),
        "org" => Some(FrontMatterFormat::Org),
        _ => None,
    }
}

#[test]
fn front_matter_decodes_like_go() {
    let diffs = expected_diffs();
    let accepted = &diffs["front_matter"];
    let json_numbers = &diffs["front_matter_rules"]["json_numbers"];
    let mut tally = Tally::new("pageparser/front matter (docs, tests, shapes, soups)");
    for (id, c) in page_cases() {
        let fm = &c["fm"];
        let Some(format) = fm["format"].as_str().and_then(format_of) else {
            continue;
        };
        let Some(src) = c["src"].as_str() else {
            tally.skipped += 1;
            continue;
        };
        if format == FrontMatterFormat::Org {
            // Org front matter is lexed, not decoded (COULD).
            tally.skipped += 1;
            continue;
        }
        let split = split_front_matter(src).expect("split");
        let (f, text) = split.front_matter.expect("front matter");
        assert_eq!(f, format, "{id}");
        let got = decode_front_matter_map(f, text);
        let outcome = |numbers| match (&got, fm.get("err")) {
            (Err(_), Some(_)) => true,
            (Ok(m), None) => match &fm["fm"] {
                J::Null => m.is_empty(),
                J::Object(o) => same_map(m, o, numbers),
                _ => false,
            },
            _ => false,
        };
        let ok = outcome(Numbers::Typed);
        // The folded form decodes too (keys lower-cased).
        if let Ok(m) = &got {
            let p = decode_front_matter(f, text).expect("fold");
            assert_eq!(p.len(), fold_len(m), "{id}");
        }
        if !ok && f == FrontMatterFormat::Json && outcome(Numbers::ByValue) {
            tally.deviation(format!("{id}: {json_numbers}"));
        } else if !ok && let Some(reason) = accepted.get(&id) {
            tally.deviation(format!("{id}: {reason}"));
        } else {
            tally.check(ok, || {
                format!(
                    "{id}: {got:?}\n  Go {}",
                    serde_json::to_string(fm).unwrap_or_default()
                )
            });
        }
    }
    tally.finish();
}

/// The number of distinct lower-cased top-level keys.
fn fold_len(m: &Map) -> usize {
    m.keys()
        .map(str::to_lowercase)
        .collect::<std::collections::BTreeSet<_>>()
        .len()
}
