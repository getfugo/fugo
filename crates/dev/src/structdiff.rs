//! Structural comparison of two builds of one site (docs/rust-port/REWRITE_PLAN.md §7.2): a
//! reference (the Go build's committed golden data) against a candidate (the Rust build), level
//! by level and per file, plus the structure oracle; the ratchet is [`crate::ratchet`].
//!
//! A side is two manifests ([`crate::manifest`]; files, or publish directories extracted like
//! the golden ones) and a structure dump (testdata/golden/README.md). The minified pass gives L1
//! and L4, the unminified pass L1, L2 and L3:
//!
//! - L1: the multisets of normalised output paths, with the known collision directories
//!   collapsed (the directory of every target that two (page, format) records of either
//!   structure dump claim: whose term wins it is an allowed difference, so only the directory's
//!   presence is compared, `dir/**`). Both passes.
//! - L2: the links and URLs of each file. Link integrity: an internal link (or alias target) of
//!   the candidate that resolves to none of its files is a difference unless the reference's is
//!   dangling too.
//! - L3: the visible text (by hash) and the heading ids of each page.
//! - L4: images' size and format, the bytes of static/ files, CSS/JS non-empty and referenced.
//!   From the minified pass; when neither side has one and both unminified manifests were
//!   extracted with L4 (a site published unminified), from those.
//! - S: per (lang, page, kind, format): target, relPermalink, permalink, template and baseof (an
//!   embedded template marked as such), written, pagers; per alias file and per page/1 alias:
//!   kind and permalink; per (lang, page, resource name): relPermalink, targets, publish; per
//!   page: its output formats.
//!
//! A7 (tracked metric): the share of the reference's HTML pages whose visible text the
//! candidate has exactly; the worst 20 pages are ranked by a similarity (the Dice coefficient
//! of the two word multisets when both manifests carry the text, else the length ratio).
//!
//! Every (file or structure key, level) gets a status (`ok`, `diff`, `missing`, `extra`) and,
//! when not ok, a fingerprint of both sides' values (12 hex digits) and difference classes.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::manifest::{self, Entry, Kind, L2, L4, Levels, Manifest, norm_path};
use crate::ratchet::{self, Report};
use crate::{Fail, fail};

mod a7;
mod compare;
mod report;
mod structure;
mod values;

pub use a7::*;
pub use compare::*;
pub use report::*;
pub use structure::*;
pub use values::*;

pub const SCHEMA: &str = "ssg-structdiff/1";
const WORST: usize = 20;

/// A level of the comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Level {
    L1,
    L2,
    L3,
    L4,
    S,
}

impl Level {
    pub const ALL: [Level; 5] = [Level::L1, Level::L2, Level::L3, Level::L4, Level::S];

    /// A level by its name (`L1` … `S`).
    #[must_use]
    pub fn parse(s: &str) -> Option<Level> {
        Level::ALL.into_iter().find(|l| l.name() == s)
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Level::L1 => "L1",
            Level::L2 => "L2",
            Level::L3 => "L3",
            Level::L4 => "L4",
            Level::S => "S",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Ok,
    Diff,
    Missing,
    Extra,
}

impl Status {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Diff => "diff",
            Status::Missing => "missing",
            Status::Extra => "extra",
        }
    }
}

/// The result of one (key, level).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Outcome {
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fp: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub classes: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
}

impl Outcome {
    #[must_use]
    pub fn ok() -> Outcome {
        Outcome {
            status: Status::Ok,
            fp: None,
            classes: Vec::new(),
            detail: String::new(),
        }
    }

    /// A difference: its classes, a readable detail, and the fingerprint of `values` (both
    /// sides' compared values).
    #[must_use]
    pub fn new(
        status: Status,
        classes: impl IntoIterator<Item = String>,
        detail: String,
        values: &Value,
    ) -> Outcome {
        let classes: BTreeSet<String> = classes.into_iter().collect();
        Outcome {
            status,
            fp: Some(fingerprint(values)),
            classes: classes.into_iter().collect(),
            detail,
        }
    }
}

/// 12 hex digits of the hash of a value's canonical JSON.
#[must_use]
pub fn fingerprint(values: &Value) -> String {
    let digest = Sha256::digest(crate::json::line(values).as_bytes());
    digest.iter().take(6).map(|b| format!("{b:02x}")).collect()
}

/// Results by key and level.
pub type Results = BTreeMap<String, BTreeMap<Level, Outcome>>;

// ---------------------------------------------------------------------------------------------
// Inputs

/// `path`, or `path.gz` when only that exists.
#[must_use]
pub fn existing(path: &Path) -> PathBuf {
    let gz = PathBuf::from(format!("{}.gz", path.display()));
    if !path.exists() && gz.exists() {
        gz
    } else {
        path.to_owned()
    }
}

/// A manifest file, or the manifest of a publish directory (extracted like the golden ones,
/// with the full text).
///
/// # Errors
/// An unreadable manifest or a failed extraction.
pub fn load_manifest(
    src: &Path,
    project: Option<&Path>,
    site: &str,
    pass: &str,
) -> Result<Manifest, Fail> {
    if !src.is_dir() {
        return crate::json::read(&existing(src));
    }
    let names: Vec<String> = if pass == "minified" {
        vec!["L1".into(), "L4".into()]
    } else {
        vec!["L1".into(), "L2".into(), "L3".into()]
    };
    let bases = match project {
        Some(p) => manifest::site_config(p)?,
        None => Vec::new(),
    };
    let files = manifest::extract(src, project, &bases, Levels::parse(&names)?, true)?;
    Ok(Manifest::new(site, pass, &names, &bases, files))
}

/// One side of a comparison.
#[derive(Clone, Debug)]
pub struct Side {
    pub name: String,
    pub min: Option<Manifest>,
    pub unmin: Option<Manifest>,
    pub structure: Option<Structure>,
}

// ---------------------------------------------------------------------------------------------
// The comparison's result

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PassCount {
    pub matched: usize,
    pub ref_files: usize,
    pub cand_files: usize,
    pub ok: usize,
    pub missing: usize,
    pub extra: usize,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LevelCount {
    pub compared: usize,
    pub ok: usize,
    pub diff: usize,
    pub missing: usize,
    pub extra: usize,
    /// The structure facts' statuses by kind of fact (S only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_kind: Option<BTreeMap<String, BTreeMap<Status, usize>>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ClassCount {
    pub class: String,
    pub count: usize,
    pub examples: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageScore {
    pub file: String,
    /// 1.0 for equal text.
    pub ratio: f64,
    /// `equal`, `words` (the Dice coefficient of the word multisets), `len` (the length ratio)
    /// or `missing`.
    pub method: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hunks: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hunk_count: Option<usize>,
}

#[derive(Clone, Debug, Serialize)]
pub struct HunkCount {
    pub hunk: String,
    pub pages: usize,
    pub examples: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct A7 {
    pub pages: usize,
    pub equal: usize,
    pub ratio: f64,
    pub mean_similarity: f64,
    pub worst: Vec<PageScore>,
    pub top_hunks: Vec<HunkCount>,
}

/// The result of a comparison.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Comparison {
    pub schema: &'static str,
    pub site: String,
    #[serde(rename = "ref")]
    pub reference: String,
    pub cand: String,
    pub collision_dirs: Vec<String>,
    pub notes: Vec<String>,
    pub passes: BTreeMap<String, PassCount>,
    pub files: Results,
    pub structure: Results,
    pub a7: Option<A7>,
    pub summary: BTreeMap<Level, LevelCount>,
    pub classes: Vec<ClassCount>,
    /// Compared (key, level)s that are not ok.
    pub diffs: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ratchet: Option<Report>,
}

// ---------------------------------------------------------------------------------------------

/// The arguments of `structdiff compare`: a reference and a candidate, each as manifest files
/// or publish directories.
#[derive(clap::Args)]
pub struct CompareArgs {
    #[arg(long)]
    pub site: String,
    #[arg(long)]
    pub ref_min: Option<PathBuf>,
    #[arg(long)]
    pub ref_unmin: Option<PathBuf>,
    #[arg(long)]
    pub ref_structure: Option<PathBuf>,
    #[arg(long)]
    pub ref_project: Option<PathBuf>,
    #[arg(long, default_value = "golden")]
    pub ref_name: String,
    #[arg(long)]
    pub cand_min: Option<PathBuf>,
    #[arg(long)]
    pub cand_unmin: Option<PathBuf>,
    #[arg(long)]
    pub cand_structure: Option<PathBuf>,
    #[arg(long)]
    pub cand_project: Option<PathBuf>,
    #[arg(long, default_value = "rust")]
    pub cand_name: String,
    /// A directory whose files are compared only by their presence at L1
    #[arg(long)]
    pub collision_dir: Vec<String>,
    /// Write the whole result as JSON
    #[arg(long)]
    pub json: Option<PathBuf>,
    /// Write the report
    #[arg(long)]
    pub report: Option<PathBuf>,
    /// The number of differences listed per level
    #[arg(long, default_value_t = 40)]
    pub show: usize,
    /// The ratchet's baseline (testdata/baselines/<site>.json)
    #[arg(long)]
    pub baseline: Option<PathBuf>,
    /// The task whose changes file lists the changes (tools/dev/changes/<task>.md)
    #[arg(long)]
    pub task: Vec<String>,
    #[arg(long, default_value_os_t = ratchet::changes_dir())]
    pub changes: PathBuf,
    /// Write the baseline with the listed changes applied
    #[arg(long)]
    pub update: bool,
    /// Never fail
    #[arg(long)]
    pub report_only: bool,
}

/// The inputs of one side.
pub struct SideArgs<'a> {
    pub name: &'a str,
    pub min: Option<&'a Path>,
    pub unmin: Option<&'a Path>,
    pub structure: Option<&'a Path>,
    pub project: Option<&'a Path>,
}

/// Reads one side.
///
/// # Errors
/// An unreadable manifest or structure dump.
pub fn read_side(site: &str, a: &SideArgs<'_>) -> Result<Side, Fail> {
    Ok(Side {
        name: a.name.to_owned(),
        min: a
            .min
            .map(|p| load_manifest(p, a.project, site, "minified"))
            .transpose()?,
        unmin: a
            .unmin
            .map(|p| load_manifest(p, a.project, site, "unminified"))
            .transpose()?,
        structure: a
            .structure
            .map(|p| crate::json::read(&existing(p)))
            .transpose()?,
    })
}

/// `structdiff compare`: the exit status.
///
/// # Errors
/// Unreadable inputs or outputs that cannot be written.
pub fn cmd_compare(a: &CompareArgs) -> Result<i32, Fail> {
    let r = read_side(
        &a.site,
        &SideArgs {
            name: &a.ref_name,
            min: a.ref_min.as_deref(),
            unmin: a.ref_unmin.as_deref(),
            structure: a.ref_structure.as_deref(),
            project: a.ref_project.as_deref(),
        },
    )?;
    let c = read_side(
        &a.site,
        &SideArgs {
            name: &a.cand_name,
            min: a.cand_min.as_deref(),
            unmin: a.cand_unmin.as_deref(),
            structure: a.cand_structure.as_deref(),
            project: a.cand_project.as_deref(),
        },
    )?;
    let mut res = compare(&a.site, &r, &c, &a.collision_dir);
    let mut status = i32::from(res.diffs > 0);
    if let Some(path) = &a.baseline {
        let report = ratchet::run(&res, path, &a.changes, &a.task, a.update)?;
        status = i32::from(!report.passed());
        res.ratchet = Some(report);
    }
    let text = report_text(&res, a.show);
    print!("{text}");
    if let Some(p) = &a.report {
        std::fs::write(p, &text).map_err(|e| fail!("{}: {e}", p.display()))?;
    }
    if let Some(p) = &a.json {
        let json = serde_json::to_string_pretty(&res).map_err(|e| fail!("{e}"))?;
        std::fs::write(p, json + "\n").map_err(|e| fail!("{}: {e}", p.display()))?;
    }
    Ok(if a.report_only { 0 } else { status })
}
