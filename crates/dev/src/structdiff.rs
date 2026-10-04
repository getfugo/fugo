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
// The structure dump

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Structure {
    pub records: Vec<Record>,
    pub aliases: Vec<AliasRecord>,
    pub pager_aliases: Vec<AliasRecord>,
    pub resources: Vec<ResourceRecord>,
    pub pages: Vec<PageRecord>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Record {
    pub lang: String,
    pub path: String,
    pub kind: String,
    pub format: String,
    pub target: String,
    pub rel_permalink: String,
    pub permalink: String,
    pub template: String,
    pub template_file: Option<String>,
    pub baseof: String,
    pub baseof_file: Option<String>,
    pub written: Option<bool>,
    pub pagers: i64,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AliasRecord {
    pub from: String,
    pub lang: String,
    pub path: String,
    pub format: String,
    pub permalink: String,
    pub kind: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ResourceRecord {
    pub lang: String,
    pub path: String,
    pub name: String,
    pub rel_permalink: String,
    pub target: Option<String>,
    pub targets: Option<Vec<String>>,
    pub publish: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PageRecord {
    pub lang: String,
    pub path: String,
    pub kind: String,
    pub outputs: Vec<String>,
}

/// A template's name, marked when it is one of the embedded templates.
fn template_id(name: &str, file: Option<&str>) -> String {
    if name.is_empty() {
        return String::new();
    }
    if file.unwrap_or(name).starts_with("_embedded/") {
        format!("{name} (embedded)")
    } else {
        name.to_owned()
    }
}

impl Structure {
    /// Every compared fact, by key.
    #[must_use]
    pub fn facts(&self) -> BTreeMap<String, Value> {
        let mut out = BTreeMap::new();
        for r in &self.records {
            out.insert(
                format!("record {} {} {} {}", r.lang, r.path, r.kind, r.format),
                json!({
                    "target": r.target,
                    "relPermalink": r.rel_permalink,
                    "permalink": r.permalink,
                    "template": template_id(&r.template, r.template_file.as_deref()),
                    "baseof": template_id(&r.baseof, r.baseof_file.as_deref()),
                    "written": r.written.unwrap_or(true),
                    "pagers": r.pagers,
                }),
            );
        }
        for (name, aliases) in [("alias", &self.aliases), ("pager", &self.pager_aliases)] {
            for a in aliases {
                let mut v = json!({ "permalink": a.permalink });
                if name == "alias" {
                    v["kind"] = json!(a.kind);
                }
                out.insert(
                    format!("{name} {} {} {} {}", a.from, a.lang, a.path, a.format),
                    v,
                );
            }
        }
        for r in &self.resources {
            let targets = match (&r.targets, &r.target) {
                (Some(t), _) if !t.is_empty() => t.clone(),
                (_, Some(t)) if !t.is_empty() => vec![t.clone()],
                _ => Vec::new(),
            };
            out.insert(
                format!("resource {} {} {}", r.lang, r.path, r.name),
                json!({ "relPermalink": r.rel_permalink, "targets": targets, "publish": r.publish.unwrap_or(true) }),
            );
        }
        for p in &self.pages {
            out.insert(
                format!("page {} {} {}", p.lang, p.path, p.kind),
                json!({ "outputs": p.outputs }),
            );
        }
        out
    }

    /// The directories of targets that more than one (page, format) record claims.
    #[must_use]
    pub fn collision_dirs(&self) -> BTreeSet<String> {
        let mut count: HashMap<&str, usize> = HashMap::new();
        for r in &self.records {
            *count.entry(r.target.as_str()).or_default() += 1;
        }
        count
            .into_iter()
            .filter(|(_, n)| *n > 1)
            .filter_map(|(t, _)| {
                t.trim_matches('/')
                    .rsplit_once('/')
                    .map(|(d, _)| format!("{d}/"))
            })
            .collect()
    }
}

fn compare_structure(r: &Structure, c: &Structure) -> Results {
    let (rf, cf) = (r.facts(), c.facts());
    let keys: BTreeSet<&String> = rf.keys().chain(cf.keys()).collect();
    let mut out = Results::new();
    for key in keys {
        let what = key.split(' ').next().unwrap_or("");
        let outcome = match (rf.get(key), cf.get(key)) {
            (Some(rv), None) => Outcome::new(
                Status::Missing,
                [format!("S {what} missing")],
                "only in the reference".into(),
                &json!(["missing", rv]),
            ),
            (None, Some(cv)) => Outcome::new(
                Status::Extra,
                [format!("S {what} extra")],
                "only in the candidate".into(),
                &json!(["extra", cv]),
            ),
            (Some(rv), Some(cv)) if rv != cv => {
                let (ro, co) = (
                    rv.as_object().cloned().unwrap_or_default(),
                    cv.as_object().cloned().unwrap_or_default(),
                );
                let fields: BTreeSet<&String> = ro
                    .keys()
                    .chain(co.keys())
                    .filter(|f| ro.get(*f) != co.get(*f))
                    .collect();
                let detail: Vec<String> = fields
                    .iter()
                    .map(|f| {
                        format!(
                            "{f} {} -> {}",
                            ro.get(*f).unwrap_or(&Value::Null),
                            co.get(*f).unwrap_or(&Value::Null)
                        )
                    })
                    .collect();
                Outcome::new(
                    Status::Diff,
                    fields.iter().map(|f| format!("S {what} {f}")),
                    detail.join("; "),
                    &json!(["diff", rv, cv]),
                )
            }
            _ => Outcome::ok(),
        };
        out.entry(key.clone())
            .or_default()
            .insert(Level::S, outcome);
    }
    out
}

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
// Per-file values

/// The entries of a manifest by normalised path (in the order of their paths).
fn by_norm(m: &Manifest) -> BTreeMap<String, Vec<&Entry>> {
    let mut groups: BTreeMap<String, Vec<&Entry>> = BTreeMap::new();
    for (rel, e) in &m.files {
        groups.entry(norm_path(rel)).or_default().push(e);
    }
    groups
}

fn sorted<T: Ord>(mut v: Vec<T>) -> Vec<T> {
    v.sort();
    v
}

/// `{a, b, …}`: at most `n` items and the number of the others.
fn short<T: std::fmt::Display>(items: &[T], n: usize) -> String {
    let shown: Vec<String> = items.iter().take(n).map(ToString::to_string).collect();
    let more = if items.len() > n {
        format!(", … {} more", items.len() - n)
    } else {
        String::new()
    };
    format!("{{{}{more}}}", shown.join(", "))
}

/// The items of `a` that are not in `b`, and those of `b` not in `a`.
fn set_changes<T: Ord + Clone>(a: &[T], b: &[T]) -> (Vec<T>, Vec<T>) {
    let (sa, sb): (BTreeSet<&T>, BTreeSet<&T>) = (a.iter().collect(), b.iter().collect());
    (
        sa.difference(&sb).map(|x| (*x).clone()).collect(),
        sb.difference(&sa).map(|x| (*x).clone()).collect(),
    )
}

/// The items of `a` beyond their count in `b`, and those of `b` beyond `a`.
fn multiset_changes<T: Ord + Clone>(a: &[T], b: &[T]) -> (Vec<T>, Vec<T>) {
    let count = |xs: &[T]| {
        let mut m: BTreeMap<T, usize> = BTreeMap::new();
        for x in xs {
            *m.entry(x.clone()).or_default() += 1;
        }
        m
    };
    let (ca, cb) = (count(a), count(b));
    let extra = |x: &BTreeMap<T, usize>, y: &BTreeMap<T, usize>| -> Vec<T> {
        x.iter()
            .flat_map(|(k, n)| {
                std::iter::repeat_n(k.clone(), n.saturating_sub(y.get(k).copied().unwrap_or(0)))
            })
            .collect()
    };
    (extra(&ca, &cb), extra(&cb, &ca))
}

fn changes<T: std::fmt::Display>(what: &str, gone: &[T], new: &[T]) -> String {
    format!("{what} -{} +{}", short(gone, 6), short(new, 6))
}

/// The classes and detail of two different L2 values of one file.
fn l2_diff(r: (Kind, &L2), c: (Kind, &L2)) -> (Vec<String>, String) {
    if r.0 != c.0 {
        return (
            vec!["L2 type".into()],
            format!("type {} -> {}", r.0.name(), c.0.name()),
        );
    }
    let (mut classes, mut parts): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    match (r.1, c.1) {
        (
            L2::Html {
                title: rt,
                rel: rr,
                links: rl,
            },
            L2::Html {
                title: ct,
                rel: cr,
                links: cl,
            },
        ) => {
            if rt != ct {
                classes.push("L2 html title".into());
                parts.push(format!("title {rt:?} -> {ct:?}"));
            }
            if rr != cr {
                classes.push("L2 html rel".into());
                let show =
                    |v: &[[String; 4]]| v.iter().map(|x| format!("{x:?}")).collect::<Vec<_>>();
                let (gone, new) = set_changes(&show(rr), &show(cr));
                parts.push(if gone.is_empty() && new.is_empty() {
                    "rel order".into()
                } else {
                    changes("rel", &gone, &new)
                });
            }
            if rl != cl {
                classes.push("L2 html links".into());
                let (gone, new) = set_changes(rl, cl);
                parts.push(changes("links", &gone, &new));
            }
        }
        (L2::Alias { alias: ra }, L2::Alias { alias: ca }) => {
            classes.push("L2 alias target".into());
            parts.push(format!("alias {ra} -> {ca}"));
        }
        (L2::Xml { items: ri }, L2::Xml { items: ci }) => {
            classes.push("L2 xml items".into());
            if sorted(ri.clone()) == sorted(ci.clone()) {
                parts.push("items in another order".into());
            } else {
                let (gone, new) = multiset_changes(ri, ci);
                parts.push(changes("items", &gone, &new));
            }
        }
        (L2::Json { keys: rk, urls: ru }, L2::Json { keys: ck, urls: cu }) => {
            if rk != ck {
                classes.push("L2 json keys".into());
                let (gone, new) = set_changes(rk, ck);
                parts.push(changes("keys", &gone, &new));
            }
            if ru != cu {
                classes.push("L2 json urls".into());
                let (gone, new) = multiset_changes(ru, cu);
                parts.push(changes("urls", &gone, &new));
            }
        }
        (L2::Lines { lines: rl }, L2::Lines { lines: cl }) => {
            classes.push("L2 lines".into());
            let (gone, new) = set_changes(rl, cl);
            parts.push(changes("lines", &gone, &new));
        }
        (rv, cv) => {
            classes.push(format!("L2 {}", r.0.name()));
            parts.push(format!(
                "{} -> {}",
                crate::json::line(rv),
                crate::json::line(cv)
            ));
        }
    }
    (classes, parts.join("; "))
}

/// The detail of two groups of values (files that share a normalised path).
fn group_detail<T: Serialize>(r: &[T], c: &[T]) -> String {
    let canon = |xs: &[T]| xs.iter().map(crate::json::line).collect::<Vec<_>>();
    let (gone, new) = multiset_changes(&canon(r), &canon(c));
    format!(
        "{} vs {} files: -{} +{}",
        r.len(),
        c.len(),
        short(&gone, 3),
        short(&new, 3)
    )
}

/// Whether an internal site path resolves to one of `files` (normalised publish paths).
#[must_use]
pub fn resolves(link: &str, files: &HashSet<String>) -> bool {
    let path = link.split(['#', '?']).next().unwrap_or("");
    let Some(rel) = path.strip_prefix('/') else {
        return true;
    };
    if rel.is_empty() {
        files.contains("index.html")
    } else if rel.ends_with('/') {
        files.contains(&format!("{rel}index.html"))
    } else {
        files.contains(rel) || files.contains(&format!("{rel}/index.html"))
    }
}

/// The internal links (and alias target) of an entry.
#[must_use]
pub fn links_of(e: &Entry) -> Vec<&String> {
    match &e.l2 {
        Some(L2::Html { links, .. }) => links.iter().collect(),
        Some(L2::Alias { alias }) => vec![alias],
        _ => Vec::new(),
    }
}

/// The internal links (and alias target) of an entry that resolve to none of `files`.
fn dangling(e: &Entry, files: &HashSet<String>) -> BTreeSet<String> {
    links_of(e)
        .into_iter()
        .filter(|x| x.starts_with('/') && !resolves(x, files))
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------------------------
// A7

/// The Dice coefficient of the word multisets of two texts.
#[must_use]
pub fn words_similarity(a: &str, b: &str) -> f64 {
    fn count(s: &str) -> HashMap<&str, usize> {
        let mut m: HashMap<&str, usize> = HashMap::new();
        for w in s.split_whitespace() {
            *m.entry(w).or_default() += 1;
        }
        m
    }
    let (ca, cb) = (count(a), count(b));
    let total: usize = ca.values().sum::<usize>() + cb.values().sum::<usize>();
    if total == 0 {
        return 1.0;
    }
    let common: usize = ca
        .iter()
        .map(|(w, n)| (*n).min(cb.get(w).copied().unwrap_or(0)))
        .sum();
    ratio(2 * common, total)
}

#[allow(clippy::cast_precision_loss)]
fn ratio(a: usize, b: usize) -> f64 {
    a as f64 / b as f64
}

fn round4(x: f64) -> f64 {
    (x * 10_000.0).round() / 10_000.0
}

/// The differing stretches of two texts, word by word (`[-removed-] [+added+]`, each side cut
/// to `width` words), at most `limit`.
#[must_use]
pub fn text_hunks(a: &str, b: &str, width: usize, limit: usize) -> Vec<String> {
    let (wa, wb): (Vec<&str>, Vec<&str>) = (
        a.split_whitespace().collect(),
        b.split_whitespace().collect(),
    );
    let cut = |ws: &[&str]| {
        let head = ws.iter().take(width).copied().collect::<Vec<_>>().join(" ");
        if ws.len() > width {
            format!("{head} …")
        } else {
            head
        }
    };
    similar::capture_diff_slices(similar::Algorithm::Myers, &wa, &wb)
        .iter()
        .filter_map(|op| match op.as_tag_tuple() {
            (similar::DiffTag::Equal, ..) => None,
            (_, ra, rb) => Some(format!("[-{}-] [+{}+]", cut(&wa[ra]), cut(&wb[rb]))),
        })
        .take(limit)
        .collect()
}

/// A7, and the first hunk (and the number of hunks) of each page with differing text.
fn a7(r: &Manifest, c: &Manifest) -> (A7, BTreeMap<String, (String, usize)>) {
    let (rg, cg) = (by_norm(r), by_norm(c));
    fn page_text<'e>(e: &&'e Entry) -> Option<&'e manifest::Text> {
        if e.kind == Kind::Html {
            e.l3.as_ref().map(|l| &l.text)
        } else {
            None
        }
    }
    let mut pages = Vec::new();
    let mut hunks: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (n, items) in &rg {
        let Some(rt) = items.first().and_then(page_text) else {
            continue;
        };
        let score = |ratio: f64, method| PageScore {
            file: n.clone(),
            ratio,
            method,
            hunks: Vec::new(),
            hunk_count: None,
        };
        let Some(ct) = cg.get(n).and_then(|cs| cs.iter().find_map(page_text)) else {
            pages.push(score(0.0, "missing"));
            continue;
        };
        if rt.sha256 == ct.sha256 {
            pages.push(score(1.0, "equal"));
        } else if let (Some(ta), Some(tb)) = (&rt.t, &ct.t) {
            let h = text_hunks(ta, tb, 8, 20);
            for x in h.iter().collect::<BTreeSet<_>>() {
                hunks.entry(x.clone()).or_default().push(n.clone());
            }
            pages.push(PageScore {
                hunks: h.iter().take(3).cloned().collect(),
                hunk_count: Some(h.len()),
                ..score(round4(words_similarity(ta, tb)), "words")
            });
        } else {
            let (la, lb) = (rt.len, ct.len);
            let r = if la.max(lb) == 0 {
                1.0
            } else {
                ratio(la.min(lb), la.max(lb))
            };
            pages.push(score(round4(r.min(0.9999)), "len"));
        }
    }
    let equal = pages.iter().filter(|p| p.method == "equal").count();
    let firsts = pages
        .iter()
        .filter_map(|p| {
            p.hunks
                .first()
                .map(|h| (p.file.clone(), (h.clone(), p.hunk_count.unwrap_or(0))))
        })
        .collect();
    let mut worst: Vec<PageScore> = pages
        .iter()
        .filter(|p| p.method != "equal")
        .cloned()
        .collect();
    worst.sort_by(|a, b| {
        a.ratio
            .total_cmp(&b.ratio)
            .then_with(|| a.file.cmp(&b.file))
    });
    worst.truncate(WORST);
    let mut top: Vec<(String, Vec<String>)> = hunks.into_iter().collect();
    top.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
    let n = pages.len();
    let a7 = A7 {
        pages: n,
        equal,
        ratio: if n == 0 { 1.0 } else { round4(ratio(equal, n)) },
        mean_similarity: if n == 0 {
            1.0
        } else {
            round4(pages.iter().map(|p| p.ratio).sum::<f64>() / ratio(n, 1))
        },
        worst,
        top_hunks: top
            .into_iter()
            .take(WORST)
            .map(|(hunk, files)| HunkCount {
                hunk,
                pages: files.len(),
                examples: files.into_iter().take(3).collect(),
            })
            .collect(),
    };
    (a7, firsts)
}

// ---------------------------------------------------------------------------------------------
// The comparison

/// L1 of both passes: a key's status is the worse of the two.
fn compare_l1(
    r: &Side,
    c: &Side,
    cdirs: &[String],
    files: &mut Results,
    notes: &mut Vec<String>,
) -> BTreeMap<String, PassCount> {
    let l1_key = |rel: &str| {
        let n = norm_path(rel);
        cdirs
            .iter()
            .find(|d| n.starts_with(d.as_str()))
            .map_or(n, |d| format!("{d}**"))
    };
    let count = |m: &Manifest| {
        let mut out: BTreeMap<String, usize> = BTreeMap::new();
        for rel in m.files.keys() {
            *out.entry(l1_key(rel)).or_default() += 1;
        }
        out
    };
    let mut passes = BTreeMap::new();
    for (pass, rm, cm) in [
        ("minified", &r.min, &c.min),
        ("unminified", &r.unmin, &c.unmin),
    ] {
        let (Some(rm), Some(cm)) = (rm, cm) else {
            let sides = if rm.is_none() && cm.is_none() {
                "both sides"
            } else {
                "one side"
            };
            notes.push(format!("L1: no {pass} pass on {sides}"));
            continue;
        };
        let (rc, cc) = (count(rm), count(cm));
        let mut tally = PassCount {
            matched: 0,
            ref_files: rm.files.len(),
            cand_files: cm.files.len(),
            ok: 0,
            missing: 0,
            extra: 0,
        };
        for key in rc.keys().chain(cc.keys()).collect::<BTreeSet<_>>() {
            let (mut a, mut b) = (
                rc.get(key).copied().unwrap_or(0),
                cc.get(key).copied().unwrap_or(0),
            );
            if key.ends_with("/**") && a > 0 && b > 0 {
                // A collision directory: only its presence is compared.
                tally.matched += a;
                a = a.min(b);
                b = a;
            } else {
                tally.matched += a.min(b);
            }
            let detail = format!("{pass}: reference {a}, candidate {b}");
            let outcome = match a.cmp(&b) {
                std::cmp::Ordering::Equal => {
                    tally.ok += 1;
                    Outcome::ok()
                }
                std::cmp::Ordering::Greater => {
                    tally.missing += 1;
                    Outcome::new(
                        Status::Missing,
                        ["L1 missing".into()],
                        detail,
                        &json!(["missing", a, b]),
                    )
                }
                std::cmp::Ordering::Less => {
                    tally.extra += 1;
                    Outcome::new(
                        Status::Extra,
                        ["L1 extra".into()],
                        detail,
                        &json!(["extra", a, b]),
                    )
                }
            };
            let slot = files.entry(key.clone()).or_default();
            if slot
                .get(&Level::L1)
                .is_none_or(|prev| prev.status == Status::Ok && outcome.status != Status::Ok)
            {
                slot.insert(Level::L1, outcome);
            }
        }
        passes.insert(pass.to_owned(), tally);
    }
    passes
}

/// L2 and L3 of the unminified pass.
fn compare_l2_l3(ru: &Manifest, cu: &Manifest, files: &mut Results) {
    let (rg, cg) = (by_norm(ru), by_norm(cu));
    let rfiles: HashSet<String> = ru.files.keys().map(|x| norm_path(x)).collect();
    let cfiles: HashSet<String> = cu.files.keys().map(|x| norm_path(x)).collect();
    for (n, ritems) in &rg {
        let Some(citems) = cg.get(n) else { continue };
        let l2 = |items: &[&Entry]| {
            sorted(
                items
                    .iter()
                    .filter_map(|e| e.l2.clone().map(|v| (e.kind, v)))
                    .collect(),
            )
        };
        let (rp, cp) = (l2(ritems), l2(citems));
        let mut new_dangling: BTreeSet<String> =
            citems.iter().flat_map(|e| dangling(e, &cfiles)).collect();
        for e in ritems {
            for x in dangling(e, &rfiles) {
                new_dangling.remove(&x);
            }
        }
        if !rp.is_empty() || !cp.is_empty() {
            let (mut classes, mut detail) = (Vec::new(), Vec::new());
            if rp != cp {
                let (cl, d) = match (&rp[..], &cp[..]) {
                    ([(rk, rv)], [(ck, cv)]) => l2_diff((*rk, rv), (*ck, cv)),
                    _ => (
                        vec![format!("L2 {}", ritems[0].kind.name())],
                        group_detail(&rp, &cp),
                    ),
                };
                classes.extend(cl);
                detail.push(d);
            }
            let dangling: Vec<String> = new_dangling.into_iter().collect();
            if !dangling.is_empty() {
                classes.push("L2 dangling links".into());
                detail.push(format!(
                    "dangling only in the candidate {}",
                    short(&dangling, 6)
                ));
            }
            let outcome = if classes.is_empty() {
                Outcome::ok()
            } else {
                Outcome::new(
                    Status::Diff,
                    classes,
                    detail.join("; "),
                    &json!(["L2", rp, cp, dangling]),
                )
            };
            files
                .entry(n.clone())
                .or_default()
                .insert(Level::L2, outcome);
        }
        let l3 = |items: &[&Entry]| {
            sorted(
                items
                    .iter()
                    .filter_map(|e| {
                        e.l3.as_ref()
                            .map(|l| (e.kind, l.text.sha256.clone(), l.ids.clone()))
                    })
                    .collect(),
            )
        };
        let (rp, cp) = (l3(ritems), l3(citems));
        if rp.is_empty() && cp.is_empty() {
            continue;
        }
        let outcome = if rp == cp {
            Outcome::ok()
        } else if let ([(_, rs, ri)], [(_, cs, ci)], Some(rl), Some(cl)) =
            (&rp[..], &cp[..], &ritems[0].l3, &citems[0].l3)
        {
            let (mut classes, mut parts): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
            if rs != cs {
                classes.push("L3 text".into());
                parts.push(format!(
                    "text {} -> {} words, {} -> {} chars",
                    rl.text.words, cl.text.words, rl.text.len, cl.text.len
                ));
            }
            if ri != ci {
                classes.push("L3 heading ids".into());
                let (gone, new) = set_changes(ri, ci);
                parts.push(if gone.is_empty() && new.is_empty() {
                    "ids in another order".into()
                } else {
                    changes("ids", &gone, &new)
                });
            }
            Outcome::new(
                Status::Diff,
                classes,
                parts.join("; "),
                &json!(["L3", rp, cp]),
            )
        } else {
            Outcome::new(
                Status::Diff,
                [format!("L3 {}", ritems[0].kind.name())],
                group_detail(&rp, &cp),
                &json!(["L3", rp, cp]),
            )
        };
        files
            .entry(n.clone())
            .or_default()
            .insert(Level::L3, outcome);
    }
}

/// The classes and detail of two different L4 values of one file.
fn l4_diff(
    r: &(Option<L4>, Option<String>),
    c: &(Option<L4>, Option<String>),
) -> (Vec<String>, String) {
    let (mut classes, mut parts): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    match (&r.0, &c.0) {
        (Some(L4::Image { image: ri }), Some(L4::Image { image: ci })) if ri != ci => {
            classes.push("L4 image".into());
            parts.push(format!("image {ri:?} -> {ci:?}"));
        }
        (
            Some(L4::Asset {
                non_empty: rn,
                referenced: rr,
            }),
            Some(L4::Asset {
                non_empty: cn,
                referenced: cr,
            }),
        ) => {
            if rn != cn {
                classes.push("L4 css/js nonEmpty".into());
                parts.push(format!("nonEmpty {rn} -> {cn}"));
            }
            if rr != cr {
                classes.push("L4 css/js referenced".into());
                parts.push(format!("referenced {rr:?} -> {cr:?}"));
            }
        }
        (rl, cl) if rl != cl => {
            classes.push("L4 type".into());
            parts.push(format!(
                "{} -> {}",
                crate::json::line(rl),
                crate::json::line(cl)
            ));
        }
        _ => {}
    }
    if r.1 != c.1 {
        classes.push("L4 static bytes".into());
        parts.push("static file bytes differ".into());
    }
    (classes, parts.join("; "))
}

/// L4 of one pass.
fn compare_l4(rm: &Manifest, cm: &Manifest, files: &mut Results) {
    let (rg, cg) = (by_norm(rm), by_norm(cm));
    for (n, ritems) in &rg {
        let Some(citems) = cg.get(n) else { continue };
        let is_static = ritems.iter().any(|e| e.is_static);
        let values = |items: &[&Entry]| {
            sorted(
                items
                    .iter()
                    .filter(|e| e.l4.is_some() || is_static)
                    .map(|e| {
                        (
                            e.l4.clone(),
                            if is_static { e.sha256.clone() } else { None },
                        )
                    })
                    .collect(),
            )
        };
        let (rp, cp) = (values(ritems), values(citems));
        if rp.is_empty() && cp.is_empty() {
            continue;
        }
        let outcome = if rp == cp {
            Outcome::ok()
        } else if let ([r], [c]) = (&rp[..], &cp[..]) {
            let (classes, detail) = l4_diff(r, c);
            Outcome::new(Status::Diff, classes, detail, &json!(["L4", rp, cp]))
        } else {
            Outcome::new(
                Status::Diff,
                [format!("L4 {}", ritems[0].kind.name())],
                group_detail(&rp, &cp),
                &json!(["L4", rp, cp]),
            )
        };
        files
            .entry(n.clone())
            .or_default()
            .insert(Level::L4, outcome);
    }
}

/// Compares two sides of `site`; `extra_collision_dirs` are collapsed at L1 too.
#[must_use]
pub fn compare(site: &str, r: &Side, c: &Side, extra_collision_dirs: &[String]) -> Comparison {
    let mut files = Results::new();
    let mut notes = Vec::new();
    let mut cdirs: BTreeSet<String> = BTreeSet::new();
    for s in [&r.structure, &c.structure].into_iter().flatten() {
        cdirs.extend(s.collision_dirs());
    }
    cdirs.extend(
        extra_collision_dirs
            .iter()
            .map(|d| format!("{}/", d.trim_matches('/'))),
    );
    let cdirs: Vec<String> = cdirs.into_iter().collect();
    let passes = compare_l1(r, c, &cdirs, &mut files, &mut notes);
    if let (Some(ru), Some(cu)) = (&r.unmin, &c.unmin) {
        compare_l2_l3(ru, cu, &mut files);
    } else {
        notes.push("L2, L3: no unminified pass on both sides".into());
    }
    match (&r.min, &c.min, &r.unmin, &c.unmin) {
        (Some(rm), Some(cm), ..) => compare_l4(rm, cm, &mut files),
        (None, None, Some(ru), Some(cu)) if ru.has("L4") && cu.has("L4") => {
            notes.push("L4: from the unminified pass (no minified pass on both sides)".into());
            compare_l4(ru, cu, &mut files);
        }
        _ => notes.push("L4: no minified pass on both sides".into()),
    }
    let structure = match (&r.structure, &c.structure) {
        (Some(rs), Some(cs)) => compare_structure(rs, cs),
        (rs, cs) => {
            let sides = if rs.is_none() && cs.is_none() {
                "both sides"
            } else {
                "one side"
            };
            notes.push(format!("S: no structure dump on {sides}"));
            Results::new()
        }
    };
    let a7 = match (&r.unmin, &c.unmin) {
        (Some(ru), Some(cu)) => {
            let (a7, firsts) = a7(ru, cu);
            for (n, (hunk, count)) in firsts {
                if let Some(o) = files.get_mut(&n).and_then(|f| f.get_mut(&Level::L3))
                    && o.status != Status::Ok
                {
                    let more = if count > 1 {
                        format!(" (+{} more)", count - 1)
                    } else {
                        String::new()
                    };
                    let sep = if o.detail.is_empty() { "" } else { "; " };
                    o.detail = format!("{}{sep}{hunk}{more}", o.detail);
                }
            }
            Some(a7)
        }
        _ => None,
    };
    let mut res = Comparison {
        schema: SCHEMA,
        site: site.into(),
        reference: r.name.clone(),
        cand: c.name.clone(),
        collision_dirs: cdirs,
        notes,
        passes,
        files,
        structure,
        a7,
        summary: BTreeMap::new(),
        classes: Vec::new(),
        diffs: 0,
        ratchet: None,
    };
    summarize(&mut res);
    res
}

/// Fills the per-level numbers, the difference classes and the total.
fn summarize(res: &mut Comparison) {
    for level in Level::ALL {
        let section = if level == Level::S {
            &res.structure
        } else {
            &res.files
        };
        let mut count = LevelCount::default();
        for o in section.values().filter_map(|v| v.get(&level)) {
            count.compared += 1;
            match o.status {
                Status::Ok => count.ok += 1,
                Status::Diff => count.diff += 1,
                Status::Missing => count.missing += 1,
                Status::Extra => count.extra += 1,
            }
        }
        if level == Level::S {
            let mut kinds: BTreeMap<String, BTreeMap<Status, usize>> = BTreeMap::new();
            for (k, v) in &res.structure {
                if let Some(o) = v.get(&Level::S) {
                    let kind = k.split(' ').next().unwrap_or("").to_owned();
                    *kinds.entry(kind).or_default().entry(o.status).or_default() += 1;
                }
            }
            count.by_kind = Some(kinds);
        }
        res.diffs += count.compared - count.ok;
        res.summary.insert(level, count);
    }
    let mut classes: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for section in [&res.files, &res.structure] {
        for (k, levels) in section {
            for o in levels.values() {
                for c in &o.classes {
                    classes.entry(c.clone()).or_default().push(k.clone());
                }
            }
        }
    }
    let mut ranked: Vec<(String, Vec<String>)> = classes.into_iter().collect();
    ranked.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
    res.classes = ranked
        .into_iter()
        .map(|(class, keys)| ClassCount {
            class,
            count: keys.len(),
            examples: keys.into_iter().take(5).collect(),
        })
        .collect();
}

// ---------------------------------------------------------------------------------------------
// The report

/// The first `n` characters of `s`.
#[must_use]
pub fn cut(s: &str, n: usize) -> &str {
    s.char_indices().nth(n).map_or(s, |(i, _)| &s[..i])
}

/// The report of a comparison.
#[must_use]
pub fn report_text(res: &Comparison, show: usize) -> String {
    let mut out = vec![format!(
        "structdiff {}: {} (reference) vs {} (candidate)",
        res.site, res.reference, res.cand
    )];
    for (pass, p) in &res.passes {
        out.push(format!(
            "  L1 {pass:<10} {}/{} files matched (candidate {} files; paths {} missing, {} extra)",
            p.matched, p.ref_files, p.cand_files, p.missing, p.extra
        ));
    }
    if !res.collision_dirs.is_empty() {
        out.push(format!(
            "     collision directories (collapsed): {}",
            res.collision_dirs.join(", ")
        ));
    }
    let level = |l| res.summary.get(&l).cloned().unwrap_or_default();
    for (l, what) in [
        (Level::L2, "files with equal links"),
        (Level::L3, "files with equal text"),
        (Level::L4, "files with equal assets"),
    ] {
        let x = level(l);
        out.push(format!("  {} {}/{} {what}", l.name(), x.ok, x.compared));
    }
    let s = level(Level::S);
    let kinds: Vec<String> = s
        .by_kind
        .iter()
        .flatten()
        .map(|(k, c)| {
            format!(
                "{k} {}/{}",
                c.get(&Status::Ok).copied().unwrap_or(0),
                c.values().sum::<usize>()
            )
        })
        .collect();
    out.push(format!(
        "  S  {}/{} structure facts equal ({})",
        s.ok,
        s.compared,
        kinds.join(", ")
    ));
    if let Some(a) = &res.a7 {
        out.push(format!(
            "  A7 {:.4} ({}/{} pages with equal visible text; mean similarity {:.4})",
            a.ratio, a.equal, a.pages, a.mean_similarity
        ));
    }
    out.extend(res.notes.iter().map(|n| format!("  note: {n}")));
    out.push(format!("  total: {} differences", res.diffs));
    if !res.classes.is_empty() {
        out.push("Top difference classes:".into());
        for c in res.classes.iter().take(25) {
            let examples = c
                .examples
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            out.push(format!("  {:>5}  {:<28} e.g. {examples}", c.count, c.class));
        }
    }
    if let Some(a) = &res.a7 {
        if !a.top_hunks.is_empty() {
            out.push("Top visible-text differences (word hunks, pages that have them):".into());
            for h in &a.top_hunks {
                let examples = h
                    .examples
                    .iter()
                    .take(2)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push(format!(
                    "  {:>5}  {}  e.g. {examples}",
                    h.pages,
                    cut(&h.hunk, 150)
                ));
            }
        }
        if !a.worst.is_empty() {
            out.push(format!(
                "Worst {} pages (A7 similarity: `words` = Dice of the word multisets, `len` = length ratio):",
                a.worst.len()
            ));
            for p in &a.worst {
                let h = p.hunks.first().map_or_else(String::new, |h| {
                    format!(
                        "  {} hunks, first {}",
                        p.hunk_count.unwrap_or(0),
                        cut(h, 120)
                    )
                });
                out.push(format!("  {:.4} {:<7} {}{h}", p.ratio, p.method, p.file));
            }
        }
    }
    for level in Level::ALL {
        let section = if level == Level::S {
            &res.structure
        } else {
            &res.files
        };
        let bad: Vec<(&String, &Outcome)> = section
            .iter()
            .filter_map(|(k, v)| {
                v.get(&level)
                    .filter(|o| o.status != Status::Ok)
                    .map(|o| (k, o))
            })
            .collect();
        if bad.is_empty() {
            continue;
        }
        out.push(format!("{} differences ({}):", level.name(), bad.len()));
        for (k, o) in bad.iter().take(show) {
            let detail = if o.detail.is_empty() {
                String::new()
            } else {
                format!(": {}", cut(&o.detail, 300))
            };
            out.push(format!("  {:<7} {k}{detail}", o.status.name()));
        }
        if bad.len() > show {
            out.push(format!("  … {} more", bad.len() - show));
        }
    }
    if let Some(r) = &res.ratchet {
        out.extend(r.lines(show));
    }
    let mut text = out.join("\n");
    text.push('\n');
    text
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
