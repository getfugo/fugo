//! Structural comparison of two builds of one site (docs/rust-port/REWRITE_PLAN.md §7.2): a
//! reference (the Go build's committed golden data) against a candidate (the Rust build), level
//! by level and per file, plus the structure oracle; then the ratchet
//! (testdata/baselines/<site>.json, tools/dev/changes/<task>.md).
//!
//! A manifest is a manifest file ([`crate::manifest`]; `.json` or `.json.gz`) or a publish
//! directory, which is extracted like the golden ones (its project directory, for static/ and
//! the base URLs, given with it); the `project:` entries of a manifest file (the Go build's
//! stats file, written next to its configuration) are left out, as this port writes no such
//! file. The minified pass gives L1 and L4, the unminified pass L1, L2 and L3; the structure
//! dump is described in testdata/golden/README.md. Every comparison reads both sides through
//! the same extractor, with the §7.2 normalisations:
//!
//! - L1: the multisets of output paths: `_hu_<hex>` -> `_hu_H`, fingerprints `.<16-64 hex>.` ->
//!   `.H.`, and the known collision directories collapsed (the directory of every target that
//!   two (page, format) records of either structure dump claim: whose term wins it is an
//!   allowed difference, so only the directory's presence is compared, `dir/**`). Both passes.
//! - L2: HTML: `<title>`, `<link rel=canonical|alternate>`, the set of internal href/src/srcset
//!   URLs; alias targets; RSS/sitemap link/loc/guid lists; JSON key paths and URL leaves; the
//!   line sets of _redirects/_headers/robots.txt. URLs are percent-decoded and NFC-normalised
//!   by the extractor. Link integrity: an internal link (or alias target) of the candidate that
//!   resolves to none of its files is a difference unless the reference's is dangling too.
//! - L3: HTML: the visible text (entities decoded, typographic characters mapped to ASCII,
//!   whitespace collapsed; compared by hash) and the heading-ID list.
//! - L4: images: (width, height, format); files of static/: bytes (sha256); CSS/JS: non-empty
//!   and referenced (as the reference's are). From the minified pass; when neither side has one
//!   and both unminified manifests were extracted with L4 (a site published unminified), from
//!   those.
//! - S: per (lang, page, kind, format): target, relPermalink, permalink, template and baseof
//!   (the v0.146 names; an embedded template is marked as such), written, pagers; per alias
//!   file (front matter and the language redirect) and per page/1 alias: kind and permalink;
//!   per (lang, page, resource name): relPermalink, target(s), publish; per page: its output
//!   formats.
//!
//! A7 (tracked metric): the share of the reference's HTML pages whose visible text the
//! candidate has exactly; the worst 20 pages are ranked by a per-page similarity (the Dice
//! coefficient of the two word multisets when both manifests carry the text, `--full-text`;
//! else the length ratio, marked `len`).
//!
//! Every (file or structure key, level) gets a status (`ok`, `diff`, `missing`, `extra`) and,
//! when not ok, a diff fingerprint (12 hex digits of the hash of both sides' normalised values)
//! and the difference classes. The report has the per-level numbers, the top difference
//! classes, the worst pages and the differences; the JSON output has all of it.
//!
//! Ratchet (with a baseline): each (key, level) is compared with the baseline's status and
//! fingerprint. A change must be listed in the changes file of the running task
//! (tools/dev/changes/<task>.md; format in its README.md); an unlisted new or changed
//! difference fails the run. `--update` writes the baseline with the listed changes applied
//! (plus new keys that are `ok`, and keys gone from both sides); unlisted changes keep their old
//! baseline entry. `--report-only` never fails. Without a baseline the run fails on any
//! difference.
//!
//! The fingerprints in the committed baselines were computed by the harness's first
//! implementation: payloads are serialized exactly as it did ([`crate::py`]).

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use indexmap::IndexMap;
use regex::Regex;
use sha2::{Digest, Sha256};

use crate::manifest::{self, norm_path};
use crate::py::{self, Dict, Py};
use crate::{Fail, dict, fail};

pub const SCHEMA: &str = "ssg-structdiff/1";
pub const BASELINE_SCHEMA: &str = "ssg-baseline/1";
pub const LEVELS: [&str; 5] = ["L1", "L2", "L3", "L4", "S"];
pub const CLASSES: [&str; 3] = ["engine-difference", "bug-fixed", "accepted-deviation"];
const GZIP_OVER: usize = 256 * 1024;
const WORST: usize = 20;

/// tools/dev/changes, the changes files of the tasks.
#[must_use]
pub fn changes_dir() -> PathBuf {
    crate::root().join("tools/dev/changes")
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
    src: Option<&Path>,
    project: Option<&Path>,
    site: &str,
    pass: &str,
) -> Result<Option<Py>, Fail> {
    let Some(src) = src else { return Ok(None) };
    if src.is_dir() {
        let levels: Vec<String> = if pass == "minified" {
            vec!["L1".into(), "L4".into()]
        } else {
            vec!["L1".into(), "L2".into(), "L3".into()]
        };
        let bases = match project {
            Some(p) => manifest::site_config(p)?,
            None => Vec::new(),
        };
        let files = manifest::extract(src, project, &bases, &levels, true)?;
        return Ok(Some(Py::Dict(manifest::document(
            site, pass, &levels, &bases, files,
        ))));
    }
    let mut doc = manifest::read_json(&existing(src))?;
    if let Some(d) = doc.as_dict_mut()
        && let Some(Py::Dict(files)) = d.get_mut("files")
    {
        let before = files.len();
        files.retain(|k, _| !k.starts_with(manifest::PROJECT_PREFIX));
        let removed = i64::try_from(before - files.len()).unwrap_or(0);
        if let Some(Py::Int(count)) = d.get_mut("count") {
            *count -= removed;
        }
    }
    Ok(Some(doc))
}

/// One side of a comparison.
pub struct Side {
    pub name: String,
    pub min: Option<Py>,
    pub unmin: Option<Py>,
    pub structure: Option<Py>,
}

fn files_of(manifest: &Py) -> &Dict {
    static EMPTY: std::sync::OnceLock<Dict> = std::sync::OnceLock::new();
    manifest
        .get("files")
        .and_then(Py::as_dict)
        .unwrap_or_else(|| EMPTY.get_or_init(Dict::new))
}

// ---------------------------------------------------------------------------------------------
// Values, as the first implementation compared, sorted and printed them

/// Python's ordering of the values that are sorted here (strings, numbers, sequences).
fn py_cmp(a: &Py, b: &Py) -> Ordering {
    match (a, b) {
        (Py::Str(x), Py::Str(y)) => x.cmp(y),
        (Py::List(x) | Py::Tuple(x), Py::List(y) | Py::Tuple(y)) => {
            for (p, q) in x.iter().zip(y) {
                if p != q {
                    return py_cmp(p, q);
                }
            }
            x.len().cmp(&y.len())
        }
        _ => match (a.as_f64(), b.as_f64()) {
            (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
            _ => Ordering::Equal,
        },
    }
}

fn canon(v: &Py) -> String {
    py::dumps(v, false)
}

fn sorted(mut v: Vec<Py>) -> Vec<Py> {
    v.sort_by(py_cmp);
    v
}

/// `sorted(set(a) - set(b))`.
fn set_minus(a: &[Py], b: &[Py]) -> Vec<Py> {
    let drop: HashSet<String> = b.iter().map(canon).collect();
    let keep: BTreeMap<String, &Py> = a
        .iter()
        .filter(|x| !drop.contains(&canon(x)))
        .map(|x| (canon(x), x))
        .collect();
    sorted(keep.into_values().cloned().collect())
}

/// `sorted((Counter(a) - Counter(b)).elements())`.
fn multiset_minus(a: &[Py], b: &[Py]) -> Vec<Py> {
    let mut count: IndexMap<String, (i64, &Py)> = IndexMap::new();
    for x in a {
        count.entry(canon(x)).or_insert((0, x)).0 += 1;
    }
    for x in b {
        if let Some(c) = count.get_mut(&canon(x)) {
            c.0 -= 1;
        }
    }
    let mut out = Vec::new();
    for (n, x) in count.into_values() {
        for _ in 0..n.max(0) {
            out.push(x.clone());
        }
    }
    sorted(out)
}

/// `{a, b, …}` of at most `n` values (`str()` of each) and the number of the others.
fn short(values: &[Py], n: usize) -> String {
    let shown: Vec<String> = values.iter().take(n).map(py::str_of).collect();
    let more = if values.len() > n {
        format!(", … {} more", values.len() - n)
    } else {
        String::new()
    };
    format!("{{{}{more}}}", shown.join(", "))
}

fn list(v: Option<&Py>) -> &[Py] {
    v.and_then(Py::as_list).unwrap_or(&[])
}

/// `{"a": …}.get(k)` printed with `str()`.
fn show(v: &Py, k: &str) -> String {
    py::str_of(v.get_or_none(k))
}

// ---------------------------------------------------------------------------------------------
// Results

/// The fingerprint of a difference: 12 hex digits of the hash of its parts.
#[must_use]
pub fn fingerprint(parts: &[Py]) -> String {
    let data = py::dumps(&Py::List(parts.to_vec()), false);
    let d = Sha256::digest(data.as_bytes());
    d.iter().map(|x| format!("{x:02x}")).collect::<String>()[..12].to_owned()
}

/// A result entry: its status and, when not ok, its fingerprint, classes and detail.
fn entry(status: &str, classes: &[String], detail: &str, fp_parts: &[Py]) -> Py {
    let mut e = Dict::new();
    e.insert("status".into(), status.into());
    if status != "ok" {
        e.insert("fp".into(), fingerprint(fp_parts).into());
        let classes: BTreeSet<&String> = classes.iter().collect();
        e.insert(
            "classes".into(),
            Py::List(classes.into_iter().map(Into::into).collect()),
        );
        if !detail.is_empty() {
            e.insert("detail".into(), detail.into());
        }
    }
    Py::Dict(e)
}

fn ok() -> Py {
    entry("ok", &[], "", &[])
}

fn status_of(e: &Py) -> &str {
    e.get("status").and_then(Py::as_str).unwrap_or("")
}

// ---------------------------------------------------------------------------------------------
// L1

/// The directories of targets that more than one (page, format) record claims.
fn collision_dirs(structure: Option<&Py>) -> BTreeSet<String> {
    let mut count: BTreeMap<String, usize> = BTreeMap::new();
    for r in list(structure.and_then(|s| s.get("records"))) {
        *count
            .entry(
                r.get("target")
                    .and_then(Py::as_str)
                    .unwrap_or("")
                    .to_owned(),
            )
            .or_default() += 1;
    }
    count
        .into_iter()
        .filter(|(_, n)| *n > 1)
        .filter_map(|(t, _)| {
            let d = t
                .trim_matches('/')
                .rsplit_once('/')
                .map_or("", |(d, _)| d)
                .to_owned();
            (!d.is_empty()).then(|| d + "/")
        })
        .collect()
}

fn l1_key(rel: &str, cdirs: &[String]) -> String {
    let n = norm_path(rel);
    cdirs
        .iter()
        .find(|d| n.starts_with(d.as_str()))
        .map_or(n, |d| format!("{d}**"))
}

/// The number of files per L1 key (a collapsed collision directory counts all its files).
fn l1_counts(manifest: &Py, cdirs: &[String]) -> BTreeMap<String, i64> {
    let mut out = BTreeMap::new();
    for rel in files_of(manifest).keys() {
        *out.entry(l1_key(rel, cdirs)).or_default() += 1;
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Per-file payloads

/// The entries of a manifest by normalised path, each group in the order of the paths.
fn by_norm(manifest: &Py) -> BTreeMap<String, Vec<&Py>> {
    let files = files_of(manifest);
    let mut rels: Vec<&String> = files.keys().collect();
    rels.sort();
    let mut g: BTreeMap<String, Vec<&Py>> = BTreeMap::new();
    for rel in rels {
        g.entry(norm_path(rel)).or_default().push(&files[rel]);
    }
    g
}

fn with_type(e: &Py, l: &Py) -> Py {
    let mut d = Dict::new();
    d.insert("type".into(), e.get_or_none("type").clone());
    if let Some(l) = l.as_dict() {
        for (k, v) in l {
            d.insert(k.clone(), v.clone());
        }
    }
    Py::Dict(d)
}

fn l2_payload(e: &Py) -> Option<Py> {
    e.get("L2")
        .filter(|v| !v.is_none())
        .map(|l2| with_type(e, l2))
}

fn l3_payload(e: &Py) -> Option<Py> {
    let l3 = e.get("L3").filter(|v| !v.is_none())?;
    if e.get("type").and_then(Py::as_str) == Some("html") {
        let text = l3
            .get("text")
            .map_or(Py::None, |t| t.get_or_none("sha256").clone());
        return Some(dict! { "type" => "html", "text" => text, "ids" => l3.get_or_none("ids") });
    }
    Some(with_type(e, l3))
}

fn l4_payload(e: &Py, is_static: bool) -> Option<Py> {
    let mut out = Dict::new();
    let t = e.get("type").and_then(Py::as_str).unwrap_or("");
    let l4 = e
        .get("L4")
        .filter(|v| v.truthy())
        .cloned()
        .unwrap_or_else(|| dict! {});
    if t == "image" {
        out.insert("image".into(), l4.get_or_none("image").clone());
    } else if t == "css" || t == "js" {
        out.insert("nonEmpty".into(), l4.get_or_none("nonEmpty").clone());
        out.insert("referenced".into(), l4.get_or_none("referenced").clone());
    }
    if is_static {
        out.insert("sha256".into(), e.get_or_none("sha256").clone());
    }
    (!out.is_empty()).then_some(Py::Dict(out))
}

/// The payloads of a group, sorted by their canonical JSON.
fn payloads(items: &[&Py], f: impl Fn(&Py) -> Option<Py>) -> Vec<Py> {
    let mut v: Vec<(String, Py)> = items
        .iter()
        .filter_map(|e| f(e))
        .map(|p| (canon(&p), p))
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v.into_iter().map(|(_, p)| p).collect()
}

fn same(a: &[Py], b: &[Py]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| canon(x) == canon(y))
}

/// Classes and a readable detail of two L2 payloads of one file.
fn l2_diff(r: &Py, c: &Py) -> (Vec<String>, String) {
    if r.get_or_none("type") != c.get_or_none("type") {
        return (
            vec!["L2 type".into()],
            format!("type {} -> {}", show(r, "type"), show(c, "type")),
        );
    }
    let t = show(r, "type");
    let (mut cls, mut parts): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    let get = |v: &Py, k: &str| v.get_or_none(k).clone();
    match t.as_str() {
        "html" => {
            if get(r, "title") != get(c, "title") {
                cls.push("L2 html title".into());
                parts.push(format!(
                    "title {} -> {}",
                    show(r, "title"),
                    show(c, "title")
                ));
            }
            if get(r, "rel") != get(c, "rel") {
                cls.push("L2 html rel".into());
                let tuples = |v: &Py| -> Vec<Py> {
                    list(v.get("rel"))
                        .iter()
                        .map(|x| Py::Tuple(x.as_list().unwrap_or(&[]).to_vec()))
                        .collect()
                };
                let (rs, cs) = (tuples(r), tuples(c));
                let (gone, new) = (set_minus(&rs, &cs), set_minus(&cs, &rs));
                parts.push(if gone.is_empty() && new.is_empty() {
                    "rel order".into()
                } else {
                    format!("rel -{} +{}", short(&gone, 6), short(&new, 6))
                });
            }
            if get(r, "links") != get(c, "links") {
                cls.push("L2 html links".into());
                let (rs, cs) = (list(r.get("links")), list(c.get("links")));
                parts.push(format!(
                    "links -{} +{}",
                    short(&set_minus(rs, cs), 6),
                    short(&set_minus(cs, rs), 6)
                ));
            }
        }
        "alias" => {
            cls.push("L2 alias target".into());
            parts.push(format!(
                "alias {} -> {}",
                show(r, "alias"),
                show(c, "alias")
            ));
        }
        "xml" => {
            cls.push("L2 xml items".into());
            let (ri, ci) = (list(r.get("items")), list(c.get("items")));
            if same(&sorted(ri.to_vec()), &sorted(ci.to_vec())) {
                parts.push("items in another order".into());
            } else {
                parts.push(format!(
                    "items -{} +{}",
                    short(&multiset_minus(ri, ci), 6),
                    short(&multiset_minus(ci, ri), 6)
                ));
            }
        }
        "json" => {
            if get(r, "error") != get(c, "error") {
                cls.push("L2 json error".into());
                parts.push(format!(
                    "error {} -> {}",
                    show(r, "error"),
                    show(c, "error")
                ));
            }
            if get(r, "keys") != get(c, "keys") {
                cls.push("L2 json keys".into());
                let (rs, cs) = (list(r.get("keys")), list(c.get("keys")));
                parts.push(format!(
                    "keys -{} +{}",
                    short(&set_minus(rs, cs), 6),
                    short(&set_minus(cs, rs), 6)
                ));
            }
            if get(r, "urls") != get(c, "urls") {
                cls.push("L2 json urls".into());
                let (ru, cu) = (list(r.get("urls")), list(c.get("urls")));
                parts.push(format!(
                    "urls -{} +{}",
                    short(&multiset_minus(ru, cu), 6),
                    short(&multiset_minus(cu, ru), 6)
                ));
            }
        }
        "lines" => {
            cls.push("L2 lines".into());
            let (rs, cs) = (list(r.get("lines")), list(c.get("lines")));
            parts.push(format!(
                "lines -{} +{}",
                short(&set_minus(rs, cs), 6),
                short(&set_minus(cs, rs), 6)
            ));
        }
        _ => cls.push(format!("L2 {t}")),
    }
    (cls, parts.join("; "))
}

fn l3_diff(r: &Py, c: &Py, rtext: &Py, ctext: &Py) -> (Vec<String>, String) {
    if r.get_or_none("type") != c.get_or_none("type") {
        return (
            vec!["L3 type".into()],
            format!("type {} -> {}", show(r, "type"), show(c, "type")),
        );
    }
    let (mut cls, mut parts): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    if r.get("type").and_then(Py::as_str) == Some("html") {
        if r.get_or_none("text") != c.get_or_none("text") {
            cls.push("L3 text".into());
            let or_q = |t: &Py, k: &str| t.get(k).map_or_else(|| "?".to_owned(), py::str_of);
            parts.push(format!(
                "text {} -> {} words, {} -> {} chars",
                or_q(rtext, "words"),
                or_q(ctext, "words"),
                or_q(rtext, "len"),
                or_q(ctext, "len"),
            ));
        }
        if r.get_or_none("ids") != c.get_or_none("ids") {
            cls.push("L3 heading ids".into());
            let (rs, cs) = (list(r.get("ids")), list(c.get("ids")));
            let (gone, new) = (set_minus(rs, cs), set_minus(cs, rs));
            parts.push(if gone.is_empty() && new.is_empty() {
                "ids in another order".into()
            } else {
                format!("ids -{} +{}", short(&gone, 6), short(&new, 6))
            });
        }
    }
    (cls, parts.join("; "))
}

fn l4_diff(r: &Py, c: &Py) -> (Vec<String>, String) {
    let (mut cls, mut parts): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
    if r.get_or_none("image") != c.get_or_none("image") {
        cls.push("L4 image".into());
        parts.push(format!(
            "image {} -> {}",
            show(r, "image"),
            show(c, "image")
        ));
    }
    if r.get_or_none("sha256") != c.get_or_none("sha256") {
        cls.push("L4 static bytes".into());
        parts.push("static file bytes differ".into());
    }
    for k in ["nonEmpty", "referenced"] {
        if r.get_or_none(k) != c.get_or_none(k) {
            cls.push(format!("L4 css/js {k}"));
            parts.push(format!("{k} {} -> {}", show(r, k), show(c, k)));
        }
    }
    (cls, parts.join("; "))
}

/// The difference of two multisets of payloads (files that share a normalised path).
fn group_detail(rp: &[Py], cp: &[Py]) -> String {
    let rc: Vec<Py> = rp.iter().map(|x| Py::Str(canon(x))).collect();
    let cc: Vec<Py> = cp.iter().map(|x| Py::Str(canon(x))).collect();
    format!(
        "{} vs {} files: -{} +{}",
        rp.len(),
        cp.len(),
        short(&multiset_minus(&rc, &cc), 3),
        short(&multiset_minus(&cc, &rc), 3)
    )
}

/// Whether an internal site path resolves to one of `files` (normalised publish paths).
#[must_use]
pub fn resolves(link: &str, files: &HashSet<String>) -> bool {
    let p = link.split('#').next().unwrap_or("");
    let p = p.split('?').next().unwrap_or("");
    let Some(rel) = p.strip_prefix('/') else {
        return true;
    };
    if rel.is_empty() {
        return files.contains("index.html");
    }
    if rel.ends_with('/') {
        return files.contains(&format!("{rel}index.html"));
    }
    files.contains(rel) || files.contains(&format!("{rel}/index.html"))
}

fn dangling(e: &Py, files: &HashSet<String>) -> BTreeSet<String> {
    let l2 = e
        .get("L2")
        .filter(|v| v.truthy())
        .cloned()
        .unwrap_or_else(|| dict! {});
    let mut links: Vec<String> = list(l2.get("links")).iter().map(py::str_of).collect();
    let alias = l2.get("alias").map_or_else(String::new, py::str_of);
    if e.get("type").and_then(Py::as_str) == Some("alias") && alias.starts_with('/') {
        links.push(alias);
    }
    links
        .into_iter()
        .filter(|x| x.starts_with('/') && !resolves(x, files))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// A7

fn word_counts(s: &str) -> BTreeMap<&str, i64> {
    let mut c = BTreeMap::new();
    for w in py::split_ws(s) {
        *c.entry(w).or_default() += 1;
    }
    c
}

/// The Dice coefficient of the word multisets of two texts.
#[must_use]
pub fn words_similarity(a: &str, b: &str) -> f64 {
    let (ca, cb) = (word_counts(a), word_counts(b));
    let total: i64 = ca.values().sum::<i64>() + cb.values().sum::<i64>();
    if total == 0 {
        return 1.0;
    }
    let common: i64 = ca
        .iter()
        .map(|(w, n)| (*n).min(cb.get(w).copied().unwrap_or(0)))
        .sum();
    #[allow(clippy::cast_precision_loss)]
    let r = (2 * common) as f64 / total as f64;
    r
}

/// The differing stretches of two texts, word by word: `[-removed-] [+added+]`, each side cut
/// to `width` words. The common prefix and suffix are trimmed first, so a page with one
/// difference costs no alignment.
#[must_use]
pub fn text_hunks(a: &str, b: &str, width: usize, limit: usize) -> Vec<String> {
    let (wa, wb) = (py::split_ws(a), py::split_ws(b));
    let mut i = 0;
    while i < wa.len().min(wb.len()) && wa[i] == wb[i] {
        i += 1;
    }
    let (mut ja, mut jb) = (wa.len(), wb.len());
    while ja > i && jb > i && wa[ja - 1] == wb[jb - 1] {
        ja -= 1;
        jb -= 1;
    }
    let cut = |ws: &[&str]| {
        let head = ws.iter().take(width).copied().collect::<Vec<_>>().join(" ");
        if ws.len() > width {
            format!("{head} …")
        } else {
            head
        }
    };
    let (ma, mb) = (&wa[i..ja], &wb[i..jb]);
    let mut out = Vec::new();
    for (tag, i1, i2, j1, j2) in crate::difflib::opcodes(ma, mb) {
        if tag != "equal" {
            out.push(format!("[-{}-] [+{}+]", cut(&ma[i1..i2]), cut(&mb[j1..j2])));
            if out.len() == limit {
                break;
            }
        }
    }
    out
}

/// A7, the worst pages, the text hunks shared by most pages, and each page's hunks (`byFile`).
fn a7(ref_un: &Py, cand_un: &Py) -> Dict {
    let (rg, cg) = (by_norm(ref_un), by_norm(cand_un));
    let mut pages: Vec<Dict> = Vec::new();
    let mut hunks: IndexMap<String, Vec<String>> = IndexMap::new();
    let is_html_l3 =
        |x: &Py| x.get("type").and_then(Py::as_str) == Some("html") && x.get("L3").is_some();
    for (n, items) in &rg {
        let e = items[0];
        if !is_html_l3(e) {
            continue;
        }
        let rt = e.get_or_none("L3").get_or_none("text");
        let Some(cand) = cg.get(n).and_then(|c| c.iter().find(|x| is_html_l3(x))) else {
            pages.push(dict! { "file" => n, "ratio" => 0.0, "method" => "missing" }.into_dict());
            continue;
        };
        let ct = cand.get_or_none("L3").get_or_none("text");
        let page = if rt.get_or_none("sha256") == ct.get_or_none("sha256") {
            dict! { "file" => n, "ratio" => 1.0, "method" => "equal" }
        } else if let (Some(Py::Str(ta)), Some(Py::Str(tb))) = (rt.get("t"), ct.get("t")) {
            let h = text_hunks(ta, tb, 8, 20);
            let mut seen = HashSet::new();
            for x in &h {
                if seen.insert(x.clone()) {
                    hunks.entry(x.clone()).or_default().push(n.clone());
                }
            }
            dict! {
                "file" => n,
                "ratio" => py::round(words_similarity(ta, tb), 4),
                "method" => "words",
                "hunks" => h.iter().take(3).cloned().collect::<Vec<String>>(),
                "hunkCount" => h.len(),
            }
        } else {
            let la = rt.get("len").and_then(Py::as_f64).unwrap_or(0.0);
            let lb = ct.get("len").and_then(Py::as_f64).unwrap_or(0.0);
            let ratio = if la.max(lb) != 0.0 {
                la.min(lb) / la.max(lb)
            } else {
                1.0
            };
            dict! { "file" => n, "ratio" => py::round(ratio.min(0.9999), 4), "method" => "len" }
        };
        pages.push(page.into_dict());
    }
    let method = |p: &Dict| p["method"].as_str().unwrap_or("").to_owned();
    let ratio = |p: &Dict| p["ratio"].as_f64().unwrap_or(0.0);
    let equal = pages.iter().filter(|p| method(p) == "equal").count();
    let mut worst: Vec<&Dict> = pages.iter().filter(|p| method(p) != "equal").collect();
    worst.sort_by(|a, b| {
        ratio(a)
            .partial_cmp(&ratio(b))
            .unwrap_or(Ordering::Equal)
            .then_with(|| a["file"].as_str().cmp(&b["file"].as_str()))
    });
    let mut top: Vec<(&String, &Vec<String>)> = hunks.iter().collect();
    top.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(b.0)));
    #[allow(clippy::cast_precision_loss)]
    let count = pages.len() as f64;
    let mut out = Dict::new();
    out.insert("pages".into(), pages.len().into());
    out.insert("equal".into(), equal.into());
    #[allow(clippy::cast_precision_loss)]
    let share = if pages.is_empty() {
        1.0
    } else {
        py::round(equal as f64 / count, 4)
    };
    out.insert("ratio".into(), share.into());
    let mean = if pages.is_empty() {
        1.0
    } else {
        py::round(py::sum_floats(pages.iter().map(ratio)) / count, 4)
    };
    out.insert("meanSimilarity".into(), mean.into());
    out.insert(
        "worst".into(),
        Py::List(
            worst
                .iter()
                .take(WORST)
                .map(|p| Py::Dict((*p).clone()))
                .collect(),
        ),
    );
    let mut by_file = Dict::new();
    for p in &pages {
        if p.get("hunks").is_some_and(Py::truthy) {
            by_file.insert(
                p["file"].as_str().unwrap_or("").to_owned(),
                Py::Dict(p.clone()),
            );
        }
    }
    out.insert("byFile".into(), Py::Dict(by_file));
    out.insert(
        "topHunks".into(),
        Py::List(
            top.into_iter()
                .take(WORST)
                .map(|(h, fs)| {
                    dict! { "hunk" => h, "pages" => fs.len(), "examples" => fs.iter().take(3).cloned().collect::<Vec<String>>() }
                })
                .collect(),
        ),
    );
    out
}

// ---------------------------------------------------------------------------------------------
// S: the structure oracle

fn template_id(name: &Py, file: &Py) -> Py {
    if !name.truthy() {
        return "".into();
    }
    let embedded = file.as_str().unwrap_or("").starts_with("_embedded/");
    Py::Str(format!(
        "{}{}",
        py::str_of(name),
        if embedded { " (embedded)" } else { "" }
    ))
}

fn get_or<'a>(v: &'a Py, k: &str, default: &'a Py) -> &'a Py {
    v.get(k).unwrap_or(default)
}

/// Every compared structure fact of a dump, by key.
fn structure_items(doc: &Py) -> IndexMap<String, Py> {
    let (empty, yes, zero) = (Py::Str(String::new()), Py::Bool(true), Py::Int(0));
    let s = |v: &Py, k: &str| py::str_of(get_or(v, k, &empty));
    let mut out = IndexMap::new();
    for r in list(doc.get("records")) {
        let k = format!(
            "record {} {} {} {}",
            s(r, "lang"),
            s(r, "path"),
            s(r, "kind"),
            s(r, "format")
        );
        let template = get_or(r, "template", &empty);
        let baseof = get_or(r, "baseof", &empty);
        out.insert(
            k,
            dict! {
                "target" => get_or(r, "target", &empty),
                "relPermalink" => get_or(r, "relPermalink", &empty),
                "permalink" => get_or(r, "permalink", &empty),
                "template" => template_id(template, get_or(r, "templateFile", template)),
                "baseof" => template_id(baseof, get_or(r, "baseofFile", baseof)),
                "written" => get_or(r, "written", &yes),
                "pagers" => get_or(r, "pagers", &zero),
            },
        );
    }
    for (name, section) in [("alias", "aliases"), ("pager", "pagerAliases")] {
        for a in list(doc.get(section)) {
            let k = format!(
                "{name} {} {} {} {}",
                s(a, "from"),
                s(a, "lang"),
                s(a, "path"),
                s(a, "format")
            );
            let mut v = dict! { "permalink" => get_or(a, "permalink", &empty) };
            if name == "alias" {
                v.as_dict_mut()
                    .expect("a dict")
                    .insert("kind".into(), get_or(a, "kind", &empty).clone());
            }
            out.insert(k, v);
        }
    }
    for r in list(doc.get("resources")) {
        let k = format!(
            "resource {} {} {}",
            s(r, "lang"),
            s(r, "path"),
            s(r, "name")
        );
        let targets = match r.get("targets") {
            Some(t) if t.truthy() => t.clone(),
            _ => match r.get("target") {
                Some(t) if t.truthy() => Py::List(vec![t.clone()]),
                _ => Py::List(Vec::new()),
            },
        };
        out.insert(
            k,
            dict! {
                "relPermalink" => get_or(r, "relPermalink", &empty),
                "targets" => targets,
                "publish" => get_or(r, "publish", &yes),
            },
        );
    }
    for p in list(doc.get("pages")) {
        let outputs = p
            .get("outputs")
            .filter(|o| o.truthy())
            .cloned()
            .unwrap_or(Py::List(Vec::new()));
        out.insert(
            format!("page {} {} {}", s(p, "lang"), s(p, "path"), s(p, "kind")),
            dict! { "outputs" => outputs },
        );
    }
    out
}

fn compare_structure(r: &Py, c: &Py) -> IndexMap<String, Py> {
    let (ri, ci) = (structure_items(r), structure_items(c));
    let keys: BTreeSet<&String> = ri.keys().chain(ci.keys()).collect();
    let mut out = IndexMap::new();
    for k in keys {
        let what = k.split(' ').next().unwrap_or("");
        let e = match (ri.get(k), ci.get(k)) {
            (Some(rv), None) => entry(
                "missing",
                &[format!("S {what} missing")],
                "only in the reference",
                &["missing".into(), rv.clone()],
            ),
            (None, Some(cv)) => entry(
                "extra",
                &[format!("S {what} extra")],
                "only in the candidate",
                &["extra".into(), cv.clone()],
            ),
            (Some(rv), Some(cv)) if rv != cv => {
                let names: BTreeSet<&String> = rv
                    .as_dict()
                    .into_iter()
                    .flat_map(|d| d.keys())
                    .chain(cv.as_dict().into_iter().flat_map(|d| d.keys()))
                    .collect();
                let fields: Vec<&String> = names
                    .into_iter()
                    .filter(|f| rv.get_or_none(f) != cv.get_or_none(f))
                    .collect();
                let detail: Vec<String> = fields
                    .iter()
                    .map(|f| {
                        format!(
                            "{f} {} -> {}",
                            py::repr(rv.get_or_none(f)),
                            py::repr(cv.get_or_none(f))
                        )
                    })
                    .collect();
                let classes: Vec<String> = fields.iter().map(|f| format!("S {what} {f}")).collect();
                entry(
                    "diff",
                    &classes,
                    &detail.join("; "),
                    &["diff".into(), rv.clone(), cv.clone()],
                )
            }
            _ => ok(),
        };
        out.insert(k.clone(), e);
    }
    out
}

// ---------------------------------------------------------------------------------------------
// The comparison

/// Compares two sides of `site`; `extra_collision_dirs` are collapsed at L1 too.
#[must_use]
pub fn compare(site: &str, r: &Side, c: &Side, extra_collision_dirs: &[String]) -> Dict {
    let mut files: BTreeMap<String, BTreeMap<String, Py>> = BTreeMap::new();
    let mut notes: Vec<String> = Vec::new();
    let mut cdirs: BTreeSet<String> = collision_dirs(r.structure.as_ref());
    cdirs.extend(collision_dirs(c.structure.as_ref()));
    cdirs.extend(
        extra_collision_dirs
            .iter()
            .map(|d| format!("{}/", d.trim_matches('/'))),
    );
    let cdirs: Vec<String> = cdirs.into_iter().collect();
    let mut passes = Dict::new();

    // L1, both passes; a key's status is the worse of the two.
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
        let (rc, cc) = (l1_counts(rm, &cdirs), l1_counts(cm, &cdirs));
        let (mut n_ok, mut missing, mut extra, mut matched) = (0, 0, 0, 0);
        let keys: BTreeSet<&String> = rc.keys().chain(cc.keys()).collect();
        for k in keys {
            let (mut a, mut b) = (
                rc.get(k).copied().unwrap_or(0),
                cc.get(k).copied().unwrap_or(0),
            );
            if k.ends_with("/**") && a != 0 && b != 0 {
                matched += a;
                a = a.min(b);
                b = a;
            } else {
                matched += a.min(b);
            }
            let e = match a.cmp(&b) {
                Ordering::Equal => {
                    n_ok += 1;
                    ok()
                }
                Ordering::Greater => {
                    missing += 1;
                    entry(
                        "missing",
                        &["L1 missing".into()],
                        &format!("{pass}: reference {a}, candidate {b}"),
                        &["missing".into(), a.into(), b.into()],
                    )
                }
                Ordering::Less => {
                    extra += 1;
                    entry(
                        "extra",
                        &["L1 extra".into()],
                        &format!("{pass}: reference {a}, candidate {b}"),
                        &["extra".into(), a.into(), b.into()],
                    )
                }
            };
            let slot = files.entry(k.clone()).or_default();
            let replace = slot
                .get("L1")
                .is_none_or(|prev| status_of(prev) == "ok" && status_of(&e) != "ok");
            if replace {
                slot.insert("L1".into(), e);
            }
        }
        passes.insert(
            pass.into(),
            dict! {
                "matched" => matched,
                "refFiles" => files_of(rm).len(),
                "candFiles" => files_of(cm).len(),
                "ok" => n_ok,
                "missing" => missing,
                "extra" => extra,
            },
        );
    }

    // L2, L3: the unminified pass.
    if let (Some(ru), Some(cu)) = (&r.unmin, &c.unmin) {
        let (rg, cg) = (by_norm(ru), by_norm(cu));
        let rfiles: HashSet<String> = files_of(ru).keys().map(|x| norm_path(x)).collect();
        let cfiles: HashSet<String> = files_of(cu).keys().map(|x| norm_path(x)).collect();
        for (n, ritems) in &rg {
            let Some(citems) = cg.get(n) else { continue };
            // L2
            let (rp, cp) = (payloads(ritems, l2_payload), payloads(citems, l2_payload));
            let mut new_dangling: BTreeSet<String> = BTreeSet::new();
            for e in citems {
                new_dangling.extend(dangling(e, &cfiles));
            }
            for e in ritems {
                for x in dangling(e, &rfiles) {
                    new_dangling.remove(&x);
                }
            }
            if !rp.is_empty() || !cp.is_empty() {
                let (mut cls, mut detail): (Vec<String>, Vec<String>) = (Vec::new(), Vec::new());
                if !same(&rp, &cp) {
                    let (cl, d) = if rp.len() == 1 && cp.len() == 1 {
                        l2_diff(&rp[0], &cp[0])
                    } else {
                        (
                            vec![format!("L2 {}", show(ritems[0], "type"))],
                            group_detail(&rp, &cp),
                        )
                    };
                    cls.extend(cl);
                    detail.push(d);
                }
                let dangling: Vec<Py> = new_dangling.iter().map(Into::into).collect();
                if !dangling.is_empty() {
                    cls.push("L2 dangling links".into());
                    detail.push(format!(
                        "dangling only in the candidate {}",
                        short(&dangling, 6)
                    ));
                }
                let e = if cls.is_empty() {
                    ok()
                } else {
                    entry(
                        "diff",
                        &cls,
                        &detail.join("; "),
                        &["L2".into(), rp.into(), cp.into(), dangling.into()],
                    )
                };
                files.entry(n.clone()).or_default().insert("L2".into(), e);
            }
            // L3
            let (rp, cp) = (payloads(ritems, l3_payload), payloads(citems, l3_payload));
            if !rp.is_empty() || !cp.is_empty() {
                let e = if same(&rp, &cp) {
                    ok()
                } else {
                    let (cl, d) = if rp.len() == 1 && cp.len() == 1 {
                        let text = |e: &Py| {
                            e.get("L3")
                                .filter(|v| v.truthy())
                                .and_then(|l| l.get("text"))
                                .filter(|t| t.truthy())
                                .cloned()
                                .unwrap_or_else(|| dict! {})
                        };
                        l3_diff(&rp[0], &cp[0], &text(ritems[0]), &text(citems[0]))
                    } else {
                        (
                            vec![format!("L3 {}", show(ritems[0], "type"))],
                            group_detail(&rp, &cp),
                        )
                    };
                    entry("diff", &cl, &d, &["L3".into(), rp.into(), cp.into()])
                };
                files.entry(n.clone()).or_default().insert("L3".into(), e);
            }
        }
    } else {
        notes.push("L2, L3: no unminified pass on both sides".into());
    }

    // L4: the minified pass; a site published unminified (docs-live: one pass, its manifests
    // extracted with L4) has it in the unminified pass.
    let has_l4 = |m: &Py| {
        list(m.get("levels"))
            .iter()
            .any(|l| l.as_str() == Some("L4"))
    };
    let l4 = match (&r.min, &c.min, &r.unmin, &c.unmin) {
        (Some(rm), Some(cm), _, _) => Some((rm, cm)),
        (None, None, Some(ru), Some(cu)) if has_l4(ru) && has_l4(cu) => {
            notes.push("L4: from the unminified pass (no minified pass on both sides)".into());
            Some((ru, cu))
        }
        _ => None,
    };
    if let Some((rm, cm)) = l4 {
        let (rg, cg) = (by_norm(rm), by_norm(cm));
        for (n, ritems) in &rg {
            let Some(citems) = cg.get(n) else { continue };
            let is_static = ritems
                .iter()
                .any(|e| e.get("static").is_some_and(Py::truthy));
            let (rp, cp) = (
                payloads(ritems, |e| l4_payload(e, is_static)),
                payloads(citems, |e| l4_payload(e, is_static)),
            );
            if rp.is_empty() && cp.is_empty() {
                continue;
            }
            let e = if same(&rp, &cp) {
                ok()
            } else if rp.len() == 1 && cp.len() == 1 {
                let (cl, d) = l4_diff(&rp[0], &cp[0]);
                entry("diff", &cl, &d, &["L4".into(), rp.into(), cp.into()])
            } else {
                let d = group_detail(&rp, &cp);
                entry(
                    "diff",
                    &[format!("L4 {}", show(ritems[0], "type"))],
                    &d,
                    &["L4".into(), rp.into(), cp.into()],
                )
            };
            files.entry(n.clone()).or_default().insert("L4".into(), e);
        }
    } else {
        notes.push("L4: no minified pass on both sides".into());
    }

    // S
    let structure: Dict = match (&r.structure, &c.structure) {
        (Some(rs), Some(cs)) => compare_structure(rs, cs)
            .into_iter()
            .map(|(k, e)| (k, dict! { "S" => e }))
            .collect(),
        (rs, cs) => {
            let sides = if rs.is_none() && cs.is_none() {
                "both sides"
            } else {
                "one side"
            };
            notes.push(format!("S: no structure dump on {sides}"));
            Dict::new()
        }
    };

    let mut text = match (&r.unmin, &c.unmin) {
        (Some(ru), Some(cu)) => Some(a7(ru, cu)),
        _ => None,
    };
    if let Some(t) = &mut text
        && let Some(Py::Dict(by_file)) = t.shift_remove("byFile")
    {
        for (n, p) in by_file {
            if let Some(Py::Dict(e)) = files.get_mut(&n).and_then(|f| f.get_mut("L3"))
                && e.get("status").and_then(Py::as_str) != Some("ok")
            {
                let count = p.get("hunkCount").and_then(Py::as_i64).unwrap_or(0);
                let more = if count > 1 {
                    format!(" (+{} more)", count - 1)
                } else {
                    String::new()
                };
                let first = list(p.get("hunks"))
                    .first()
                    .map(py::str_of)
                    .unwrap_or_default();
                let before = e.get("detail").map_or_else(String::new, py::str_of);
                let detail = format!("{before}; {first}{more}");
                e.insert(
                    "detail".into(),
                    detail.trim_start_matches([';', ' ']).into(),
                );
            }
        }
    }
    let mut res = Dict::new();
    res.insert("schema".into(), SCHEMA.into());
    res.insert("site".into(), site.into());
    res.insert("ref".into(), r.name.clone().into());
    res.insert("cand".into(), c.name.clone().into());
    res.insert("collisionDirs".into(), cdirs.into());
    res.insert("notes".into(), notes.into());
    res.insert("passes".into(), Py::Dict(passes));
    res.insert(
        "files".into(),
        Py::Dict(
            files
                .into_iter()
                .map(|(k, v)| (k, Py::Dict(v.into_iter().collect())))
                .collect(),
        ),
    );
    res.insert("structure".into(), Py::Dict(structure));
    res.insert("a7".into(), text.map_or(Py::None, Py::Dict));
    res
}

/// Adds the per-level numbers (`summary`), the difference classes (`classes`) and the total
/// (`diffs`).
pub fn summarize(res: &mut Dict) {
    let mut levels = Dict::new();
    let mut diffs = 0;
    for lv in LEVELS {
        let section = if lv == "S" {
            &res["structure"]
        } else {
            &res["files"]
        };
        let mut counts: BTreeMap<&str, i64> = BTreeMap::new();
        for v in section.as_dict().into_iter().flat_map(|d| d.values()) {
            if let Some(e) = v.get(lv) {
                *counts.entry(status_of(e)).or_default() += 1;
            }
        }
        let compared: i64 = counts.values().sum();
        let get = |s: &str| counts.get(s).copied().unwrap_or(0);
        diffs += compared - get("ok");
        levels.insert(
            lv.into(),
            dict! { "compared" => compared, "ok" => get("ok"), "diff" => get("diff"), "missing" => get("missing"), "extra" => get("extra") },
        );
    }
    let mut kinds: BTreeMap<String, IndexMap<String, i64>> = BTreeMap::new();
    for (k, v) in res["structure"].as_dict().into_iter().flatten() {
        let kind = k.split(' ').next().unwrap_or("").to_owned();
        *kinds
            .entry(kind)
            .or_default()
            .entry(status_of(v.get_or_none("S")).to_owned())
            .or_default() += 1;
    }
    let by_kind: Dict = kinds
        .into_iter()
        .map(|(k, c)| {
            (
                k,
                Py::Dict(c.into_iter().map(|(s, n)| (s, Py::Int(n))).collect()),
            )
        })
        .collect();
    levels["S"]
        .as_dict_mut()
        .expect("S")
        .insert("byKind".into(), Py::Dict(by_kind));
    let mut classes: IndexMap<String, Vec<String>> = IndexMap::new();
    for section in ["files", "structure"] {
        for (k, v) in res[section].as_dict().into_iter().flatten() {
            for e in v.as_dict().into_iter().flat_map(|d| d.values()) {
                for c in list(e.get("classes")) {
                    classes.entry(py::str_of(c)).or_default().push(k.clone());
                }
            }
        }
    }
    let mut ranked: Vec<(String, Vec<String>)> = classes.into_iter().collect();
    ranked.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
    res.insert("summary".into(), Py::Dict(levels));
    res.insert(
        "classes".into(),
        Py::List(
            ranked
                .into_iter()
                .map(|(c, ks)| {
                    let n = ks.len();
                    dict! { "class" => c, "count" => n, "examples" => ks.into_iter().take(5).collect::<Vec<String>>() }
                })
                .collect(),
        ),
    );
    res.insert("diffs".into(), diffs.into());
}

// ---------------------------------------------------------------------------------------------
// The ratchet

fn entry_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"^- (?P<site>[A-Za-z0-9_.-]+) (?P<levels>(?:L[1-4]|S)(?:,(?:L[1-4]|S))*) `(?P<key>[^`]+)` (?P<cls>[a-z-]+): (?P<reason>\S.*)$",
        )
        .expect("a valid expression")
    })
}

fn entry_start_re() -> &'static Regex {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^- [A-Za-z0-9_.-]+ (?:L[1-4]|S)[ ,]").expect("a valid expression")
    })
}

/// An entry of a changes file.
#[derive(Clone, Debug)]
pub struct Change {
    pub task: String,
    pub site: String,
    pub levels: Vec<String>,
    pub pattern: String,
    pub cls: String,
    pub reason: String,
    pub at: String,
    pub used: usize,
}

impl Change {
    #[must_use]
    pub fn matches(&self, site: &str, level: &str, key: &str) -> bool {
        site == self.site
            && self.levels.iter().any(|l| l == level)
            && crate::fnmatch::fnmatchcase(key, &self.pattern)
    }
}

/// The text of a file read with universal newlines (`\r\n` and `\r` as `\n`).
fn read_text(path: &Path) -> Result<String, Fail> {
    let s = std::fs::read_to_string(path).map_err(|e| fail!("{}: {e}", path.display()))?;
    Ok(s.replace("\r\n", "\n").replace('\r', "\n"))
}

/// The entries of one changes file, and its format errors.
///
/// # Errors
/// An unreadable file.
pub fn parse_changes(path: &Path, task: &str) -> Result<(Vec<Change>, Vec<String>), Fail> {
    let (mut changes, mut errors) = (Vec::new(), Vec::new());
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let text = read_text(path)?;
    let mut lines: Vec<&str> = text.split('\n').collect();
    if text.ends_with('\n') {
        lines.pop();
    }
    for (i, line) in lines.into_iter().enumerate() {
        let at = format!("{name}:{}", i + 1);
        let Some(m) = entry_re().captures(line) else {
            if entry_start_re().is_match(line) {
                errors.push(format!(
                    "{at}: not `- <site> <level>[,<level>] `<key>` <class>: <reason>`: {line}"
                ));
            }
            continue;
        };
        if !CLASSES.contains(&&m["cls"]) {
            errors.push(format!(
                "{at}: triage class {} is not one of {}",
                py::repr_str(&m["cls"]),
                CLASSES.join(", ")
            ));
            continue;
        }
        changes.push(Change {
            task: task.to_owned(),
            site: m["site"].to_owned(),
            levels: m["levels"].split(',').map(str::to_owned).collect(),
            pattern: m["key"].to_owned(),
            cls: m["cls"].to_owned(),
            reason: py::strip(&m["reason"]).to_owned(),
            at,
            used: 0,
        });
    }
    Ok((changes, errors))
}

/// The entries of the changes files of `tasks`, and the errors.
///
/// # Errors
/// An unreadable file.
pub fn load_changes(dir: &Path, tasks: &[String]) -> Result<(Vec<Change>, Vec<String>), Fail> {
    let (mut changes, mut errors) = (Vec::new(), Vec::new());
    for task in tasks {
        let path = dir.join(format!("{task}.md"));
        if !path.is_file() {
            errors.push(format!(
                "{}: no changes file for task {task}",
                path.display()
            ));
            continue;
        }
        let (c, e) = parse_changes(&path, task)?;
        changes.extend(c);
        errors.extend(e);
    }
    Ok((changes, errors))
}

/// `ok`, `<status>:<fp>`, or `None` (no entry).
fn sig(v: Option<&Py>) -> Option<String> {
    let v = v?;
    if let Py::Str(s) = v {
        return Some(s.clone());
    }
    if status_of(v) == "ok" {
        return Some("ok".into());
    }
    Some(format!(
        "{}:{}",
        status_of(v),
        v.get("fp").map_or_else(String::new, py::str_of)
    ))
}

type Key = (String, String, String);

fn flatten(sections: &Dict) -> BTreeMap<Key, Py> {
    let mut out = BTreeMap::new();
    for section in ["files", "structure"] {
        for (key, levels) in sections
            .get(section)
            .and_then(Py::as_dict)
            .into_iter()
            .flatten()
        {
            for (lv, v) in levels.as_dict().into_iter().flatten() {
                out.insert((section.to_owned(), key.clone(), lv.clone()), v.clone());
            }
        }
    }
    out
}

/// A baseline, or `None` when there is none.
///
/// # Errors
/// An unreadable baseline, or one of another schema.
pub fn read_baseline(path: &Path) -> Result<Option<Dict>, Fail> {
    let path = existing(path);
    if !path.exists() {
        return Ok(None);
    }
    let doc = manifest::read_json(&path)?;
    if doc.get("schema").and_then(Py::as_str) != Some(BASELINE_SCHEMA) {
        return Err(fail!(
            "{}: not a {BASELINE_SCHEMA} baseline",
            path.display()
        ));
    }
    Ok(doc.as_dict().cloned())
}

/// Compares the result with the baseline; returns the ratchet report and the new baseline.
pub fn ratchet(
    res: &Dict,
    baseline: Option<&Dict>,
    changes: &mut [Change],
    site: &str,
) -> (Dict, Dict) {
    let cur = flatten(res);
    let base = baseline.map(flatten).unwrap_or_default();
    let (mut unlisted, mut listed, mut improved, mut wrong_class) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut benign = 0;
    let mut new = base.clone();
    let keys: BTreeSet<&Key> = cur.keys().chain(base.keys()).collect();
    for k in keys {
        let (c, b) = (cur.get(k), base.get(k));
        let (cs, bs) = (sig(c), sig(b));
        if cs == bs {
            continue;
        }
        let (section, key, lv) = k;
        let mut item = dict! {
            "section" => section,
            "key" => key,
            "level" => lv,
            "was" => bs.clone().unwrap_or_else(|| "none".into()),
            "now" => cs.clone().unwrap_or_else(|| "none".into()),
        };
        let item_d = item.as_dict_mut().expect("a dict");
        if let Some(c) = c
            && cs.as_deref() != Some("ok")
        {
            item_d.insert(
                "classes".into(),
                c.get("classes").cloned().unwrap_or(Py::List(Vec::new())),
            );
            if let Some(d) = c.get("detail").filter(|d| d.truthy()) {
                item_d.insert("detail".into(), d.clone());
            }
        }
        let good = matches!(cs.as_deref(), None | Some("ok"));
        if good && matches!(bs.as_deref(), None | Some("ok")) {
            benign += 1;
            if c.is_none() {
                new.remove(k);
            } else {
                new.insert(k.clone(), "ok".into());
            }
            continue;
        }
        let Some(m) = changes.iter_mut().find(|ch| ch.matches(site, lv, key)) else {
            if good {
                improved.push(item);
            } else {
                unlisted.push(item);
            }
            continue;
        };
        m.used += 1;
        item_d.insert("task".into(), m.task.clone().into());
        item_d.insert("class".into(), m.cls.clone().into());
        item_d.insert("reason".into(), m.reason.clone().into());
        if !good && m.cls == "bug-fixed" {
            wrong_class.push(item);
            continue;
        }
        listed.push(item);
        match c {
            None => {
                new.remove(k);
            }
            Some(_) if good => {
                new.insert(k.clone(), "ok".into());
            }
            Some(c) => {
                new.insert(
                    k.clone(),
                    dict! {
                        "status" => c.get_or_none("status"),
                        "fp" => c.get_or_none("fp"),
                        "class" => m.cls.clone(),
                        "task" => m.task.clone(),
                        "reason" => m.reason.clone(),
                    },
                );
            }
        }
    }
    let unused: Vec<String> = changes
        .iter()
        .filter(|ch| ch.used == 0 && ch.site == site)
        .map(|ch| {
            format!(
                "{}: {} {} `{}` matched no change",
                ch.at,
                ch.site,
                ch.levels.join(","),
                ch.pattern
            )
        })
        .collect();
    let mut sections: BTreeMap<&str, BTreeMap<String, BTreeMap<String, Py>>> = BTreeMap::new();
    sections.insert("files", BTreeMap::new());
    sections.insert("structure", BTreeMap::new());
    for ((section, key, lv), v) in new {
        sections
            .get_mut(section.as_str())
            .expect("a section")
            .entry(key)
            .or_default()
            .insert(lv, v);
    }
    let mut doc = Dict::new();
    doc.insert("schema".into(), BASELINE_SCHEMA.into());
    doc.insert("site".into(), site.into());
    for (section, keys) in sections {
        doc.insert(
            section.into(),
            Py::Dict(
                keys.into_iter()
                    .map(|(k, v)| (k, Py::Dict(v.into_iter().collect())))
                    .collect(),
            ),
        );
    }
    let report = dict! {
        "baseline" => baseline.is_some(),
        "unlisted" => unlisted,
        "listed" => listed,
        "improved" => improved,
        "benign" => benign,
        "wrongClass" => wrong_class,
        "unused" => unused,
    };
    (report.into_dict(), doc)
}

/// A baseline as text: one key per line.
#[must_use]
pub fn dump_baseline(doc: &Dict) -> String {
    let mut lines = vec![
        format!(
            "\"schema\": {}",
            py::json_str(doc["schema"].as_str().unwrap_or(""), true)
        ),
        format!(
            "\"site\": {}",
            py::json_str(doc["site"].as_str().unwrap_or(""), true)
        ),
    ];
    for section in ["files", "structure"] {
        let entries = doc
            .get(section)
            .and_then(Py::as_dict)
            .cloned()
            .unwrap_or_default();
        let body: Vec<String> = entries
            .iter()
            .map(|(k, v)| format!("{}: {}", py::json_str(k, false), py::dumps(v, false)))
            .collect();
        lines.push(if body.is_empty() {
            format!("\"{section}\": {{}}")
        } else {
            format!("\"{section}\": {{\n{}\n}}", body.join(",\n"))
        });
    }
    lines.sort();
    format!("{{\n{}\n}}\n", lines.join(",\n"))
}

/// Writes a baseline (gzipped when it is over 256 KiB) and returns its path. A file that
/// already holds the same baseline is left as it is.
///
/// # Errors
/// The file cannot be written.
pub fn write_baseline(path: &Path, doc: &Dict) -> Result<PathBuf, Fail> {
    let text = dump_baseline(doc);
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
    let gzipped = target.extension().is_some_and(|e| e == "gz");
    let current = std::fs::read(&target).ok().map(|raw| {
        if !gzipped {
            return raw;
        }
        let mut out = Vec::new();
        std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(&raw[..]), &mut out)
            .map_or_else(|_| Vec::new(), |_| out)
    });
    if current.as_deref() != Some(text.as_bytes()) {
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir).map_err(|e| fail!("{}: {e}", dir.display()))?;
        }
        let data = if gzipped {
            manifest::gzip(text.as_bytes())
        } else {
            text.into_bytes()
        };
        std::fs::write(&target, data).map_err(|e| fail!("{}: {e}", target.display()))?;
    }
    Ok(target)
}

// ---------------------------------------------------------------------------------------------
// The report

fn n(v: &Py, k: &str) -> i64 {
    v.get(k).and_then(Py::as_i64).unwrap_or(0)
}

/// The report of a comparison.
#[must_use]
pub fn report_text(res: &Dict, show_n: usize) -> String {
    let mut out: Vec<String> = Vec::new();
    let s = &res["summary"];
    let text = |k: &str| res.get(k).map_or_else(String::new, py::str_of);
    out.push(format!(
        "structdiff {}: {} (reference) vs {} (candidate)",
        text("site"),
        text("ref"),
        text("cand")
    ));
    for (pass, p) in res["passes"].as_dict().into_iter().flatten() {
        out.push(format!(
            "  L1 {pass:<10} {}/{} files matched (candidate {} files; paths {} missing, {} extra)",
            n(p, "matched"),
            n(p, "refFiles"),
            n(p, "candFiles"),
            n(p, "missing"),
            n(p, "extra"),
        ));
    }
    let cdirs: Vec<String> = list(res.get("collisionDirs"))
        .iter()
        .map(py::str_of)
        .collect();
    if !cdirs.is_empty() {
        out.push(format!(
            "     collision directories (collapsed): {}",
            cdirs.join(", ")
        ));
    }
    for (lv, what) in [
        ("L2", "files with equal links"),
        ("L3", "files with equal text"),
        ("L4", "files with equal assets"),
    ] {
        let x = s.get_or_none(lv);
        out.push(format!("  {lv} {}/{} {what}", n(x, "ok"), n(x, "compared")));
    }
    let x = s.get_or_none("S");
    let kinds: Vec<String> = x
        .get("byKind")
        .and_then(Py::as_dict)
        .into_iter()
        .flatten()
        .map(|(k, c)| {
            let total: i64 = c
                .as_dict()
                .into_iter()
                .flat_map(|d| d.values())
                .filter_map(Py::as_i64)
                .sum();
            format!("{k} {}/{total}", n(c, "ok"))
        })
        .collect();
    out.push(format!(
        "  S  {}/{} structure facts equal ({})",
        n(x, "ok"),
        n(x, "compared"),
        kinds.join(", ")
    ));
    let a = res.get("a7").filter(|a| a.truthy());
    if let Some(a) = a {
        let f = |k: &str| a.get(k).and_then(Py::as_f64).unwrap_or(0.0);
        out.push(format!(
            "  A7 {:.4} ({}/{} pages with equal visible text; mean similarity {:.4})",
            f("ratio"),
            n(a, "equal"),
            n(a, "pages"),
            f("meanSimilarity"),
        ));
    }
    for note in list(res.get("notes")) {
        out.push(format!("  note: {}", py::str_of(note)));
    }
    out.push(format!(
        "  total: {} differences",
        res.get("diffs").and_then(Py::as_i64).unwrap_or(0)
    ));
    let classes = list(res.get("classes"));
    if !classes.is_empty() {
        out.push("Top difference classes:".into());
        for c in classes.iter().take(25) {
            let examples: Vec<String> = list(c.get("examples"))
                .iter()
                .take(3)
                .map(py::str_of)
                .collect();
            out.push(format!(
                "  {:>5}  {:<28} e.g. {}",
                n(c, "count"),
                show(c, "class"),
                examples.join(", ")
            ));
        }
    }
    if let Some(a) = a {
        let top = list(a.get("topHunks"));
        if !top.is_empty() {
            out.push("Top visible-text differences (word hunks, pages that have them):".into());
            for h in top {
                let examples: Vec<String> = list(h.get("examples"))
                    .iter()
                    .take(2)
                    .map(py::str_of)
                    .collect();
                out.push(format!(
                    "  {:>5}  {}  e.g. {}",
                    n(h, "pages"),
                    py::prefix(&show(h, "hunk"), 150),
                    examples.join(", ")
                ));
            }
        }
        let worst = list(a.get("worst"));
        if !worst.is_empty() {
            out.push(format!(
                "Worst {} pages (A7 similarity: `words` = Dice of the word multisets, `len` = length ratio):",
                worst.len()
            ));
            for p in worst {
                let hunks = list(p.get("hunks"));
                let h = match hunks.first() {
                    Some(first) => {
                        format!(
                            "  {} hunks, first {}",
                            n(p, "hunkCount"),
                            py::prefix(&py::str_of(first), 120)
                        )
                    }
                    None => String::new(),
                };
                let ratio = p.get("ratio").and_then(Py::as_f64).unwrap_or(0.0);
                out.push(format!(
                    "  {ratio:.4} {:<7} {}{h}",
                    show(p, "method"),
                    show(p, "file")
                ));
            }
        }
    }
    for lv in LEVELS {
        let section = if lv == "S" {
            &res["structure"]
        } else {
            &res["files"]
        };
        let bad: Vec<(&String, &Py)> = section
            .as_dict()
            .into_iter()
            .flatten()
            .filter_map(|(k, v)| v.get(lv).filter(|e| status_of(e) != "ok").map(|e| (k, e)))
            .collect();
        if bad.is_empty() {
            continue;
        }
        out.push(format!("{lv} differences ({}):", bad.len()));
        for (k, e) in bad.iter().take(show_n) {
            let detail = e
                .get("detail")
                .filter(|d| d.truthy())
                .map_or_else(String::new, |d| {
                    format!(": {}", py::prefix(&py::str_of(d), 300))
                });
            out.push(format!("  {:<7} {k}{detail}", status_of(e)));
        }
        if bad.len() > show_n {
            out.push(format!("  … {} more", bad.len() - show_n));
        }
    }
    if let Some(r) = res.get("ratchet").filter(|r| r.truthy()) {
        out.push("Ratchet:".into());
        if !r.get("baseline").is_some_and(Py::truthy) {
            out.push("  no baseline yet: every difference is a new one".into());
        }
        for (name, what) in [
            ("unlisted", "UNLISTED (fails)"),
            ("wrongClass", "WRONG CLASS (fails)"),
            ("listed", "listed"),
            ("improved", "improved, not listed (kept in the baseline)"),
        ] {
            let items = list(r.get(name));
            if items.is_empty() {
                continue;
            }
            out.push(format!("  {what}: {}", items.len()));
            for it in items.iter().take(show_n) {
                let extra = if it.get("class").is_some() {
                    format!(
                        " [{} {}: {}]",
                        show(it, "class"),
                        show(it, "task"),
                        show(it, "reason")
                    )
                } else {
                    String::new()
                };
                out.push(format!(
                    "    {} {}: {} -> {}{extra}",
                    show(it, "level"),
                    show(it, "key"),
                    show(it, "was"),
                    show(it, "now")
                ));
            }
            if items.len() > show_n {
                out.push(format!("    … {} more", items.len() - show_n));
            }
        }
        for u in list(r.get("unused")) {
            out.push(format!("  warning: {}", py::str_of(u)));
        }
        for e in list(r.get("errors")) {
            out.push(format!("  error: {}", py::str_of(e)));
        }
        if let Some(w) = r.get("written").filter(|w| w.truthy()) {
            out.push(format!("  baseline written: {}", py::str_of(w)));
        }
        out.push(format!("  verdict: {}", show(r, "verdict")));
    }
    let mut text = out.join("\n");
    text.push('\n');
    text
}

// ---------------------------------------------------------------------------------------------

/// The arguments of `structdiff compare`.
#[derive(Default)]
pub struct CompareArgs {
    pub site: String,
    pub ref_min: Option<PathBuf>,
    pub ref_unmin: Option<PathBuf>,
    pub ref_structure: Option<PathBuf>,
    pub ref_project: Option<PathBuf>,
    pub ref_name: String,
    pub cand_min: Option<PathBuf>,
    pub cand_unmin: Option<PathBuf>,
    pub cand_structure: Option<PathBuf>,
    pub cand_project: Option<PathBuf>,
    pub cand_name: String,
    pub collision_dir: Vec<String>,
    pub json: Option<PathBuf>,
    pub report: Option<PathBuf>,
    pub show: usize,
    pub baseline: Option<PathBuf>,
    pub task: Vec<String>,
    pub changes: PathBuf,
    pub update: bool,
    pub report_only: bool,
}

/// `structdiff compare`: the exit status.
///
/// # Errors
/// Unreadable inputs or outputs that cannot be written.
pub fn cmd_compare(a: &CompareArgs) -> Result<i32, Fail> {
    let structure = |p: &Option<PathBuf>| {
        p.as_ref()
            .map(|p| manifest::read_json(&existing(p)))
            .transpose()
    };
    let r = Side {
        name: a.ref_name.clone(),
        min: load_manifest(
            a.ref_min.as_deref(),
            a.ref_project.as_deref(),
            &a.site,
            "minified",
        )?,
        unmin: load_manifest(
            a.ref_unmin.as_deref(),
            a.ref_project.as_deref(),
            &a.site,
            "unminified",
        )?,
        structure: structure(&a.ref_structure)?,
    };
    let c = Side {
        name: a.cand_name.clone(),
        min: load_manifest(
            a.cand_min.as_deref(),
            a.cand_project.as_deref(),
            &a.site,
            "minified",
        )?,
        unmin: load_manifest(
            a.cand_unmin.as_deref(),
            a.cand_project.as_deref(),
            &a.site,
            "unminified",
        )?,
        structure: structure(&a.cand_structure)?,
    };
    let mut res = compare(&a.site, &r, &c, &a.collision_dir);
    summarize(&mut res);
    let mut status = 0;
    if let Some(path) = &a.baseline {
        let baseline = read_baseline(path)?;
        let (mut changes, errors) = load_changes(&a.changes, &a.task)?;
        let (mut rep, doc) = ratchet(&res, baseline.as_ref(), &mut changes, &a.site);
        let mut errors: Vec<Py> = errors.into_iter().map(Py::Str).collect();
        let mut failed = !list(rep.get("unlisted")).is_empty()
            || !list(rep.get("wrongClass")).is_empty()
            || !errors.is_empty();
        if a.update {
            if a.task.is_empty() {
                errors
                    .push("--update needs --task (the changes file that lists the changes)".into());
                failed = true;
            } else {
                let written = write_baseline(path, &doc)?;
                rep.insert("written".into(), written.display().to_string().into());
            }
        }
        rep.insert("errors".into(), Py::List(errors));
        rep.insert(
            "verdict".into(),
            if failed { "FAIL" } else { "pass" }.into(),
        );
        res.insert("ratchet".into(), Py::Dict(rep));
        status = i32::from(failed);
    } else if res.get("diffs").and_then(Py::as_i64).unwrap_or(0) != 0 {
        status = 1;
    }
    let text = report_text(&res, a.show);
    print!("{text}");
    if let Some(p) = &a.report {
        std::fs::write(p, &text).map_err(|e| fail!("{}: {e}", p.display()))?;
    }
    if let Some(p) = &a.json {
        let mut json = py::dumps_indent0(&Py::Dict(res));
        json.push('\n');
        std::fs::write(p, json).map_err(|e| fail!("{}: {e}", p.display()))?;
    }
    Ok(if a.report_only { 0 } else { status })
}

/// `structdiff changes`: validates every changes file; the exit status.
///
/// # Errors
/// An unreadable directory or file.
pub fn cmd_changes(dir: &Path) -> Result<i32, Fail> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map_err(|e| fail!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".md") && n != "README.md")
        .collect();
    names.sort();
    let (mut count, mut errors) = (0, Vec::new());
    for name in names {
        let (c, e) = parse_changes(&dir.join(&name), &name[..name.len() - 3])?;
        count += c.len();
        errors.extend(e);
    }
    for e in &errors {
        eprintln!("structdiff changes: {e}");
    }
    let tail = if errors.is_empty() {
        String::new()
    } else {
        format!(", {} errors", errors.len())
    };
    println!("{count} change entries in {}{tail}", dir.display());
    Ok(i32::from(!errors.is_empty()))
}
