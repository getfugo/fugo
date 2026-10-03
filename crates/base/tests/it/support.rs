//! Shared helpers of the oracle tests.

use serde_json::Value as J;

/// Reads a fixture under `testdata` as raw JSON.
pub fn fixture(rel: &str) -> J {
    ssg_testkit::fixture::oracle(rel)
}

/// A fixture string: `None` for a `{"$nh:bytes": …}` value (not UTF-8, cannot be a `&str`).
pub fn text(v: &J) -> Option<&str> {
    v.as_str()
}

/// Tallies checks and collects mismatches and accepted deviations.
#[derive(Default)]
pub struct Tally {
    pub name: &'static str,
    pub checks: usize,
    pub passed: usize,
    pub deviations: Vec<String>,
    pub skipped: Vec<String>,
    pub failures: Vec<String>,
}

impl Tally {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            ..Self::default()
        }
    }

    /// Records one comparison.
    pub fn check(&mut self, ok: bool, what: impl FnOnce() -> String) {
        self.checks += 1;
        if ok {
            self.passed += 1;
        } else {
            self.failures.push(what());
        }
    }

    /// Records a comparison that differs for a documented reason (see the crate README).
    pub fn deviation(&mut self, what: impl FnOnce() -> String) {
        self.checks += 1;
        self.deviations.push(what());
    }

    /// Records a case that cannot be expressed through the API (a Go string that is not UTF-8,
    /// a configuration the Go implementation never uses, an argument outside the parameter type).
    pub fn skip(&mut self, what: impl FnOnce() -> String) {
        self.skipped.push(what());
    }

    /// Prints the counts and fails on any mismatch.
    #[track_caller]
    pub fn finish(self) {
        eprintln!(
            "{}: {} checks, {} passed, {} accepted deviations, {} failures ({} not applicable)",
            self.name,
            self.checks,
            self.passed,
            self.deviations.len(),
            self.failures.len(),
            self.skipped.len()
        );
        for d in self.deviations.iter().take(10) {
            eprintln!("  deviation: {d}");
        }
        // NH_TALLY_DIR=<dir> writes every deviation and failure to <dir>/<name>.txt.
        if let Some(dir) = std::env::var_os("NH_TALLY_DIR") {
            let path =
                std::path::Path::new(&dir).join(format!("{}.txt", self.name.replace('/', "_")));
            let body: Vec<String> = self
                .failures
                .iter()
                .map(|f| format!("FAIL {f}"))
                .chain(self.deviations.iter().map(|d| format!("DEV {d}")))
                .chain(self.skipped.iter().map(|d| format!("SKIP {d}")))
                .collect();
            std::fs::write(path, body.join("\n")).expect("write tally");
        }
        assert!(
            self.failures.is_empty(),
            "{}: {} mismatches:\n{}",
            self.name,
            self.failures.len(),
            self.failures[..self.failures.len().min(40)].join("\n")
        );
    }
}
