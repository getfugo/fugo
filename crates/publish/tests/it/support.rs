//! Tallies of oracle comparisons, checked against `crates/publish/expected_diffs.toml`.

use std::collections::BTreeMap;

use ssg_testkit::fixture::GoString;

/// Pass/fail counts of one fixture family.
#[derive(Default)]
pub struct Tally {
    pub total: usize,
    pub failed: usize,
    /// Differences accepted as documented deviations, by class.
    pub accepted: BTreeMap<&'static str, usize>,
    samples: Vec<String>,
}

impl Tally {
    pub fn pass(&mut self) {
        self.total += 1;
    }

    pub fn fail(&mut self, detail: impl FnOnce() -> String) {
        self.total += 1;
        self.failed += 1;
        if self.samples.len() < 25 {
            self.samples.push(detail());
        }
    }

    /// A difference that `expected_diffs.toml` documents.
    pub fn accept(&mut self, class: &'static str) {
        self.total += 1;
        *self.accepted.entry(class).or_default() += 1;
    }

    pub fn check(&mut self, ok: bool, detail: impl FnOnce() -> String) {
        if ok {
            self.pass();
        } else {
            self.fail(detail);
        }
    }

    pub fn exact(&self) -> usize {
        self.total - self.failed - self.accepted.values().sum::<usize>()
    }

    /// Prints the tally and asserts that every difference is a reviewed one, with the reviewed
    /// count.
    pub fn finish(&self, family: &str) {
        self.print(family);
        self.verify(family);
    }

    pub fn print(&self, family: &str) {
        eprintln!(
            "{family}: {} checks, {} exact, accepted deviations {:?}, unexplained {}",
            self.total,
            self.exact(),
            self.accepted,
            self.failed
        );
        for s in &self.samples {
            eprintln!("  {s}");
        }
    }

    pub fn verify(&self, family: &str) {
        assert_eq!(self.failed, 0, "{family}: unexplained differences");
        let got: BTreeMap<String, usize> = self
            .accepted
            .iter()
            .map(|(k, v)| ((*k).to_owned(), *v))
            .collect();
        assert_eq!(
            got,
            expected_diffs(family),
            "{family}: accepted deviations differ from expected_diffs.toml"
        );
    }
}

/// The reviewed deviation counts of a family.
pub fn expected_diffs(family: &str) -> BTreeMap<String, usize> {
    let path = ssg_testkit::fixture::repo_dir().join("crates/publish/expected_diffs.toml");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let doc: toml::Table = toml::from_str(&text).expect("expected_diffs.toml");
    let Some(classes) = doc.get(family).and_then(toml::Value::as_table) else {
        return BTreeMap::new();
    };
    classes
        .iter()
        .map(|(class, entry)| {
            let count = entry
                .get("count")
                .and_then(toml::Value::as_integer)
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or_else(|| panic!("{family}.{class}: count"));
            (class.clone(), count)
        })
        .collect()
}

/// A Go string for messages.
pub fn show(s: &GoString) -> String {
    let text = String::from_utf8_lossy(&s.0);
    let mut t: String = text.chars().take(160).collect();
    if text.chars().count() > 160 {
        t.push('…');
    }
    format!("{t:?}")
}
