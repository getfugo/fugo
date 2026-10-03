//! The reviewed deviations from the Go oracles (`crates/site/expected_diffs.toml`).

use std::collections::BTreeSet;

use ssg_base::Value;
use ssg_testkit::fixture::repo_dir;

/// The `path`s listed for `fixture` (`capture/edge-tree`, `assemble/edge-tree`, `data/…`).
fn paths(fixture: &str) -> BTreeSet<String> {
    let file = repo_dir().join("crates/site/expected_diffs.toml");
    let text = std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    let doc = Value::from_toml_str(&text).unwrap();
    let mut out = BTreeSet::new();
    let entries = doc
        .as_map()
        .and_then(|m| m.get("deviation"))
        .and_then(Value::as_array)
        .unwrap_or_default();
    for d in entries {
        let m = d.as_map().expect("a table");
        let s = |k: &str| m.get(k).and_then(Value::as_str).unwrap_or_default();
        assert!(
            !s("reason").is_empty(),
            "{file:?}: a deviation without a reason"
        );
        assert_eq!(s("class"), "accepted-deviation", "{file:?}");
        let fixtures = m
            .get("fixtures")
            .and_then(Value::as_array)
            .unwrap_or_default();
        if fixtures.iter().any(|f| f.as_str() == Some(fixture)) {
            out.insert(s("path").to_owned());
        }
    }
    out
}

/// Page paths whose placement differs from the capture oracle.
pub fn capture(name: &str) -> BTreeSet<String> {
    paths(&format!("capture/{name}"))
}

/// Page paths whose assembly differs from the assemble oracle.
pub fn assemble(name: &str) -> BTreeSet<String> {
    paths(&format!("assemble/{name}"))
}

/// The accepted difference classes of the test `test` (`[[class]]` entries): id → exact count.
pub fn classes(test: &str) -> std::collections::BTreeMap<String, usize> {
    let file = repo_dir().join("crates/site/expected_diffs.toml");
    let text = std::fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    let doc = Value::from_toml_str(&text).unwrap();
    let mut out = std::collections::BTreeMap::new();
    let entries = doc
        .as_map()
        .and_then(|m| m.get("class"))
        .and_then(Value::as_array)
        .unwrap_or_default();
    for c in entries {
        let m = c.as_map().expect("a table");
        let s = |k: &str| m.get(k).and_then(Value::as_str).unwrap_or_default();
        assert!(
            !s("reason").is_empty(),
            "{file:?}: a class without a reason"
        );
        if s("test") != test {
            continue;
        }
        let count = m.get("count").and_then(Value::as_i64).expect("count");
        out.insert(s("id").to_owned(), usize::try_from(count).unwrap());
    }
    out
}

/// Value paths (`/numbers/octal`) whose data differs from the data oracle.
pub fn data(name: &str) -> BTreeSet<String> {
    paths(&format!("data/{name}"))
}
