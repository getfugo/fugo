//! Shared helpers of the oracle tests.

use std::collections::BTreeMap;

use serde_json::Value as J;

/// `crates/pageparser/expected_diffs.toml`: section → case id → reason.
pub fn expected_diffs() -> BTreeMap<String, BTreeMap<String, String>> {
    let path = ssg_testkit::fixture::repo_dir().join("crates/pageparser/expected_diffs.toml");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    toml::from_str(&text).unwrap_or_else(|e| panic!("{path:?}: {e}"))
}

/// The cases of `parser/pageparser/pages.json.gz` with unique ids: the input name, plus
/// `#<n>` (its ordinal among inputs of the same name) when the name repeats (`test`, `shape`).
pub fn page_cases() -> Vec<(String, J)> {
    let doc: J = ssg_testkit::fixture::oracle("oracle/parser/pageparser/pages.json.gz");
    let J::Array(cases) = doc["cases"].clone() else {
        panic!("pages.json.gz: no cases")
    };
    assert_eq!(Some(cases.len() as u64), doc["inputs"].as_u64());
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    cases
        .into_iter()
        .map(|c| {
            let name = c["name"].as_str().expect("case name").to_owned();
            let n = seen.entry(name.clone()).or_default();
            let id = if matches!(name.as_str(), "test" | "shape") {
                format!("{name}#{n}")
            } else {
                name
            };
            *n += 1;
            (id, c)
        })
        .collect()
}

/// Tallies checks, accepted deviations and failures.
pub struct Tally {
    name: &'static str,
    pub checks: usize,
    pub passed: usize,
    pub deviations: Vec<String>,
    pub skipped: usize,
    pub failures: Vec<String>,
}

impl Tally {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            checks: 0,
            passed: 0,
            deviations: Vec::new(),
            skipped: 0,
            failures: Vec::new(),
        }
    }

    pub fn check(&mut self, ok: bool, what: impl FnOnce() -> String) {
        self.checks += 1;
        if ok {
            self.passed += 1;
        } else {
            self.failures.push(what());
        }
    }

    pub fn deviation(&mut self, what: String) {
        self.checks += 1;
        self.deviations.push(what);
    }

    #[track_caller]
    pub fn finish(self) {
        eprintln!(
            "{}: {} checks, {} passed, {} accepted deviations, {} failures ({} not applicable)",
            self.name,
            self.checks,
            self.passed,
            self.deviations.len(),
            self.failures.len(),
            self.skipped,
        );
        for d in self.deviations.iter().take(10) {
            eprintln!("  deviation: {d}");
        }
        assert!(
            self.failures.is_empty(),
            "{}: {} failures:\n{}",
            self.name,
            self.failures.len(),
            self.failures[..self.failures.len().min(30)].join("\n")
        );
    }
}
