//! The ratchet of the comparison (tools/dev/changes/README.md): each (key, level) is compared
//! with the site's baseline (testdata/baselines/<site>.json). A change must be listed in the
//! changes file of the running task (tools/dev/changes/<task>.md); an unlisted new or changed
//! difference fails the run. `--update` writes the baseline with the listed changes applied (plus
//! new keys that are `ok`, and keys gone from both sides); unlisted changes keep their old entry.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use globset::{Glob, GlobMatcher};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::structdiff::{Comparison, Level, Outcome, Status};
use crate::{Fail, fail, json};

mod changes;

pub use changes::*;

pub const BASELINE_SCHEMA: &str = "ssg-baseline/1";
/// The triage classes of a change.
pub const CLASSES: [&str; 3] = ["engine-difference", "bug-fixed", "accepted-deviation"];
/// A baseline above this size is stored gzipped.
const GZIP_OVER: usize = 256 * 1024;

/// A baseline entry: `"ok"`, or an accepted difference.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Accepted {
    Status(Status),
    Difference {
        status: Status,
        fp: String,
        class: String,
        task: String,
        reason: String,
    },
}

impl Accepted {
    /// `ok`, or `<status>:<fingerprint>`.
    #[must_use]
    pub fn signature(&self) -> String {
        match self {
            Accepted::Status(s) => s.name().to_owned(),
            Accepted::Difference { status, fp, .. } => format!("{}:{fp}", status.name()),
        }
    }
}

/// `ok`, or `<status>:<fingerprint>`.
fn signature(o: &Outcome) -> String {
    if o.status == Status::Ok {
        "ok".into()
    } else {
        format!("{}:{}", o.status.name(), o.fp.as_deref().unwrap_or(""))
    }
}

type Entries = BTreeMap<String, BTreeMap<Level, Accepted>>;

/// The accepted state of a site: per file and per structure fact, per level.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Baseline {
    pub schema: String,
    pub site: String,
    pub files: Entries,
    pub structure: Entries,
}

impl Baseline {
    /// A baseline, or `None` when there is none.
    ///
    /// # Errors
    /// An unreadable baseline, or one of another schema.
    pub fn read(path: &Path) -> Result<Option<Baseline>, Fail> {
        let path = crate::structdiff::existing(path);
        if !path.exists() {
            return Ok(None);
        }
        let b: Baseline = json::read(&path)?;
        if b.schema != BASELINE_SCHEMA {
            return Err(fail!(
                "{}: not a {BASELINE_SCHEMA} baseline",
                path.display()
            ));
        }
        Ok(Some(b))
    }

    /// The baseline as text: one key per line.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut lines = vec![
            format!("\"schema\": {}", json::string(&self.schema)),
            format!("\"site\": {}", json::string(&self.site)),
        ];
        for (name, entries) in [("files", &self.files), ("structure", &self.structure)] {
            let body: Vec<String> = entries
                .iter()
                .map(|(k, v)| format!("{}: {}", json::string(k), json::line(v)))
                .collect();
            lines.push(if body.is_empty() {
                format!("\"{name}\": {{}}")
            } else {
                format!("\"{name}\": {{\n{}\n}}", body.join(",\n"))
            });
        }
        lines.sort();
        format!("{{\n{}\n}}\n", lines.join(",\n"))
    }

    /// Writes the baseline (gzipped when it is over 256 KiB) and returns its path; a file that
    /// already holds the same baseline is left as it is.
    ///
    /// # Errors
    /// The file cannot be written.
    pub fn write(&self, path: &Path) -> Result<PathBuf, Fail> {
        let text = self.to_text();
        let s = path.to_string_lossy();
        let plain = PathBuf::from(s.strip_suffix(".gz").unwrap_or(&s));
        let gz = PathBuf::from(format!("{}.gz", plain.display()));
        let (target, other) = if text.len() > GZIP_OVER {
            (gz, plain)
        } else {
            (plain, gz)
        };
        if other.exists() {
            std::fs::remove_file(&other).map_err(|e| fail!("{}: {e}", other.display()))?;
        }
        if json::read_bytes(&target).ok().as_deref() != Some(text.as_bytes()) {
            if let Some(dir) = target.parent() {
                std::fs::create_dir_all(dir).map_err(|e| fail!("{}: {e}", dir.display()))?;
            }
            json::write(&target, &text)?;
        }
        Ok(target)
    }
}

/// A (key, level) whose status changed.
#[derive(Clone, Debug, Serialize)]
pub struct Item {
    pub section: &'static str,
    pub key: String,
    pub level: Level,
    pub was: String,
    pub now: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub classes: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub detail: String,
    /// The changes entry that lists it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// The ratchet's verdict.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// Whether there was a baseline.
    pub baseline: bool,
    /// New or changed differences no changes entry lists (they fail the run).
    pub unlisted: Vec<Item>,
    pub listed: Vec<Item>,
    /// Differences gone without an entry (kept in the baseline).
    pub improved: Vec<Item>,
    /// Keys that became `ok` or went away and were `ok`.
    pub benign: usize,
    /// New differences listed as `bug-fixed` (they fail the run).
    pub wrong_class: Vec<Item>,
    pub unused: Vec<String>,
    pub errors: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub written: Option<String>,
    /// `pass` or `FAIL`.
    pub verdict: String,
}

impl Report {
    #[must_use]
    pub fn passed(&self) -> bool {
        self.verdict == "pass"
    }

    /// The report's lines.
    #[must_use]
    pub fn lines(&self, show: usize) -> Vec<String> {
        let mut out = vec!["Ratchet:".to_owned()];
        if !self.baseline {
            out.push("  no baseline yet: every difference is a new one".into());
        }
        for (items, what) in [
            (&self.unlisted, "UNLISTED (fails)"),
            (&self.wrong_class, "WRONG CLASS (fails)"),
            (&self.listed, "listed"),
            (
                &self.improved,
                "improved, not listed (kept in the baseline)",
            ),
        ] {
            if items.is_empty() {
                continue;
            }
            out.push(format!("  {what}: {}", items.len()));
            for it in items.iter().take(show) {
                let entry = match (&it.class, &it.task, &it.reason) {
                    (Some(c), Some(t), Some(r)) => format!(" [{c} {t}: {r}]"),
                    _ => String::new(),
                };
                out.push(format!(
                    "    {} {}: {} -> {}{entry}",
                    it.level.name(),
                    it.key,
                    it.was,
                    it.now
                ));
            }
            if items.len() > show {
                out.push(format!("    … {} more", items.len() - show));
            }
        }
        out.extend(self.unused.iter().map(|u| format!("  warning: {u}")));
        out.extend(self.errors.iter().map(|e| format!("  error: {e}")));
        if let Some(w) = &self.written {
            out.push(format!("  baseline written: {w}"));
        }
        out.push(format!("  verdict: {}", self.verdict));
        out
    }
}

/// Compares a result with a baseline; returns the report (no verdict yet) and the new baseline.
#[must_use]
pub fn ratchet(
    res: &Comparison,
    baseline: Option<&Baseline>,
    changes: &mut [Change],
    site: &str,
) -> (Report, Baseline) {
    let mut report = Report {
        baseline: baseline.is_some(),
        ..Report::default()
    };
    let mut next = baseline.cloned().unwrap_or_default();
    next.schema = BASELINE_SCHEMA.into();
    next.site = site.into();
    for (section, current) in [("files", &res.files), ("structure", &res.structure)] {
        let empty = Entries::new();
        let accepted = baseline.map_or(&empty, |b| {
            if section == "files" {
                &b.files
            } else {
                &b.structure
            }
        });
        let target = if section == "files" {
            &mut next.files
        } else {
            &mut next.structure
        };
        let keys: BTreeSet<(&String, Level)> = current
            .iter()
            .flat_map(|(k, v)| v.keys().map(move |l| (k, *l)))
            .chain(
                accepted
                    .iter()
                    .flat_map(|(k, v)| v.keys().map(move |l| (k, *l))),
            )
            .collect();
        for (key, level) in keys {
            let now = current.get(key).and_then(|v| v.get(&level));
            let was = accepted.get(key).and_then(|v| v.get(&level));
            let (now_sig, was_sig) = (now.map(signature), was.map(Accepted::signature));
            if now_sig == was_sig {
                continue;
            }
            let good = now.is_none_or(|o| o.status == Status::Ok);
            let mut item = Item {
                section,
                key: key.clone(),
                level,
                was: was_sig.clone().unwrap_or_else(|| "none".into()),
                now: now_sig.unwrap_or_else(|| "none".into()),
                classes: now
                    .filter(|_| !good)
                    .map(|o| o.classes.clone())
                    .unwrap_or_default(),
                detail: now
                    .filter(|_| !good)
                    .map(|o| o.detail.clone())
                    .unwrap_or_default(),
                task: None,
                class: None,
                reason: None,
            };
            let set = |target: &mut Entries, value: Option<Accepted>| match value {
                Some(v) => {
                    target.entry(key.clone()).or_default().insert(level, v);
                }
                None => {
                    if let Some(levels) = target.get_mut(key) {
                        levels.remove(&level);
                        if levels.is_empty() {
                            target.remove(key);
                        }
                    }
                }
            };
            if good && was_sig.as_deref().is_none_or(|w| w == "ok") {
                report.benign += 1;
                set(target, now.map(|_| Accepted::Status(Status::Ok)));
                continue;
            }
            let Some(change) = changes.iter_mut().find(|c| c.matches(site, level, key)) else {
                if good {
                    report.improved.push(item);
                } else {
                    report.unlisted.push(item);
                }
                continue;
            };
            change.used += 1;
            item.task = Some(change.task.clone());
            item.class = Some(change.class.clone());
            item.reason = Some(change.reason.clone());
            if !good && change.class == "bug-fixed" {
                report.wrong_class.push(item);
                continue;
            }
            report.listed.push(item);
            let value = now.map(|o| {
                if good {
                    Accepted::Status(Status::Ok)
                } else {
                    Accepted::Difference {
                        status: o.status,
                        fp: o.fp.clone().unwrap_or_default(),
                        class: change.class.clone(),
                        task: change.task.clone(),
                        reason: change.reason.clone(),
                    }
                }
            });
            set(target, value);
        }
    }
    report.unused = changes
        .iter()
        .filter(|c| c.used == 0 && c.site == site)
        .map(|c| {
            let levels: Vec<&str> = c.levels.iter().map(|l| l.name()).collect();
            format!(
                "{}: {} {} `{}` matched no change",
                c.at,
                c.site,
                levels.join(","),
                c.pattern
            )
        })
        .collect();
    (report, next)
}

/// The ratchet of a comparison against the baseline at `path`, with the changes of `tasks`;
/// with `update`, the baseline is written.
///
/// # Errors
/// An unreadable baseline or changes file, or a baseline that cannot be written.
pub fn run(
    res: &Comparison,
    path: &Path,
    dir: &Path,
    tasks: &[String],
    update: bool,
) -> Result<Report, Fail> {
    let baseline = Baseline::read(path)?;
    let (mut changes, errors) = load_changes(dir, tasks)?;
    let (mut report, next) = ratchet(res, baseline.as_ref(), &mut changes, &res.site);
    report.errors = errors;
    if update {
        if tasks.is_empty() {
            report
                .errors
                .push("--update needs --task (the changes file that lists the changes)".into());
        } else {
            report.written = Some(next.write(path)?.display().to_string());
        }
    }
    let failed =
        !report.unlisted.is_empty() || !report.wrong_class.is_empty() || !report.errors.is_empty();
    report.verdict = if failed { "FAIL" } else { "pass" }.into();
    Ok(report)
}
