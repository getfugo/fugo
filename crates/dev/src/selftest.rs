//! Self-test of structdiff (docs/rust-port/REWRITE_PLAN.md §7.2): synthetic perturbations of a
//! Go build's output, each compared with the unperturbed output, must be classified exactly as
//! expected (the right file, level and difference class, and nothing else):
//!
//! 1. drop a file: L1 missing
//! 2. add a file: L1 extra
//! 3. change an internal link: L2 html links
//! 4. reorder attributes: ignored (no difference)
//! 5. percent-encode a Thai href: ignored (no difference; a percent-encoded one is decoded)
//! 6. split code into token spans: ignored (no difference: a highlighter's span structure)
//! 7. change visible text: L3 text
//! 8. change image dimensions: L4 image
//! 9. change an RSS item link: L2 xml items
//! 10. drop the most linked page: L1 missing, and L2 dangling links in every file that links to
//!     it (link integrity)
//!
//! then the ratchet: against a baseline of the unperturbed output, perturbation 7 unlisted
//! fails; listed in a changes file it passes, and `--update` writes it into the baseline; the
//! unperturbed output against that baseline is an unlisted improvement, which does not fail.
//!
//! The Go output is a publish directory (with its site directory) or, by default, Go's testsite
//! output (crates/build/tests/it/testsite-go.txtar) with a Thai page and a PNG added (the
//! testsite has neither). Everything happens in a temporary directory.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use regex::{Captures, Regex};

use crate::manifest::{self, Kind, L2, Manifest, norm_path, page_url};
use crate::ratchet::{self, Accepted, Baseline};
use crate::structdiff::{self as sd, Comparison, Level, Side, Status};
use crate::urls::SiteUrls;
use crate::{Fail, fail, json, txtar};

mod content;
mod perturb;

use content::*;
use perturb::*;

fn re(cell: &'static OnceLock<Regex>, pattern: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pattern).expect("a valid expression"))
}

// ---------------------------------------------------------------------------------------------
// The Go output

fn write(path: &Path, text: &[u8]) -> Result<(), Fail> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| fail!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, text).map_err(|e| fail!("{}: {e}", path.display()))
}

fn read(root: &Path, rel: &str) -> Result<String, Fail> {
    let p = root.join(rel);
    std::fs::read_to_string(&p).map_err(|e| fail!("{}: {e}", p.display()))
}

/// A PNG of the given size, one colour.
fn png(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbImage::from_pixel(width, height, image::Rgb([0x80, 0x40, 0x20]));
    let mut out = Cursor::new(Vec::new());
    image
        .write_to(&mut out, image::ImageFormat::Png)
        .expect("encode to memory");
    out.into_inner()
}

/// Go's testsite output plus a Thai page and an image (what the testsite lacks).
fn testsite_output(dest: &Path) -> Result<(), Fail> {
    let archive = crate::root().join("crates/build/tests/it/testsite-go.txtar");
    let text =
        std::fs::read_to_string(&archive).map_err(|e| fail!("{}: {e}", archive.display()))?;
    for (name, content) in txtar::parse(&text) {
        write(&dest.join(name), content.as_bytes())?;
    }
    let th = "/th/%E0%B8%82%E0%B8%99%E0%B8%A1/";
    write(
        &dest.join("th/ขนม/index.html"),
        format!(
            "<!DOCTYPE html><html lang=\"th\"><head><title>ขนม</title></head><body><h1 id=\"khanom\">ขนม</h1><p>หน้าขนมไทย with a picture</p><a href=\"{th}\">ขนม</a> <a href=\"/\">Home</a><img src=\"/img/selftest.png\" alt=\"\"></body></html>\n"
        )
        .as_bytes(),
    )?;
    write(&dest.join("img/selftest.png"), &png(3, 2))
}

// ---------------------------------------------------------------------------------------------
// Comparison

struct Run {
    site: String,
    project: Option<PathBuf>,
}

impl Run {
    fn manifest(&self, out: &Path, pass: &str) -> Result<Manifest, Fail> {
        sd::load_manifest(out, self.project.as_deref(), &self.site, pass)
    }

    fn side(&self, name: &str, out: &Path) -> Result<Side, Fail> {
        Ok(Side {
            name: name.to_owned(),
            min: Some(self.manifest(out, "minified")?),
            unmin: Some(self.manifest(out, "unminified")?),
            structure: None,
        })
    }

    fn compare(&self, r: &Path, c: &Path) -> Result<Comparison, Fail> {
        Ok(sd::compare(
            &self.site,
            &self.side("go", r)?,
            &self.side("perturbed", c)?,
            &[],
        ))
    }
}

/// A (key, level) of a comparison.
type Key = (String, Level);
type Diffs = BTreeMap<Key, Vec<String>>;

/// The classes of every (key, level) that is not ok.
fn diffs(res: &Comparison) -> Diffs {
    [&res.files, &res.structure]
        .into_iter()
        .flat_map(|section| section.iter())
        .flat_map(|(k, levels)| levels.iter().map(move |(lv, o)| ((k.clone(), *lv), o)))
        .filter(|(_, o)| o.status != Status::Ok)
        .map(|(key, o)| (key, o.classes.clone()))
        .collect()
}

fn html_files(root: &Path) -> Vec<String> {
    manifest::walk(root)
        .into_iter()
        .filter(|r| r.ends_with(".html"))
        .collect()
}

fn is_html(man: &Manifest, rel: &str) -> bool {
    man.files.get(rel).is_some_and(|e| e.kind == Kind::Html)
}

const PERTURBATIONS: [(&str, Perturbation); 10] = [
    ("drop a file", drop_file),
    ("add a file", add_file),
    ("change an internal link", change_link),
    ("reorder attributes (ignored)", reorder_attributes),
    ("percent-encode a Thai href (ignored)", thai_href),
    ("split code into token spans (ignored)", split_code),
    ("change visible text", change_text),
    ("change image dimensions", change_image),
    ("change an RSS item link", change_rss_link),
    // Beyond §7.2's eight: link integrity (the span split is T66's).
    ("drop a linked page (link integrity)", drop_linked_page),
];

/// Up to `n` of the items, as `<level> <key>: <class>`, sorted by level; "no difference" for none.
fn listing(items: impl Iterator<Item = (Key, String)>, n: usize) -> String {
    let mut lines: Vec<(Level, String, String)> = items.map(|((k, lv), c)| (lv, k, c)).collect();
    if lines.is_empty() {
        return "no difference".into();
    }
    lines.sort();
    let shown: Vec<String> = lines
        .iter()
        .take(n)
        .map(|(lv, k, c)| format!("{} {k}: {c}", lv.name()))
        .collect();
    let more = if lines.len() > n {
        format!("; … {} more", lines.len() - n)
    } else {
        String::new()
    };
    format!("{}{more}", shown.join("; "))
}

/// Whether the differences are exactly the expected ones (each with its class).
fn check(got: &Diffs, want: &Want) -> bool {
    got.len() == want.len()
        && want
            .iter()
            .all(|(k, c)| got.get(k).is_some_and(|g| g.contains(c)))
}

fn pass(ok: bool) -> &'static str {
    if ok { "PASS" } else { "FAIL" }
}

// ---------------------------------------------------------------------------------------------
// The ratchet

fn ratchet_checks(
    run: &Run,
    base_dir: &Path,
    text_dir: &Path,
    tmp: &Path,
) -> Result<Vec<(String, bool, String)>, Fail> {
    let mut results = Vec::new();
    let base_res = run.compare(base_dir, base_dir)?;
    let path = tmp.join("baseline.json");
    let (_, doc) = ratchet::ratchet(&base_res, None, &mut [], &run.site);
    doc.write(&path)?;
    let baseline = || Baseline::read(&path);
    let res = run.compare(base_dir, text_dir)?;
    let d = diffs(&res);
    let (key, level) = match d.keys().collect::<Vec<_>>()[..] {
        [k] => k.clone(),
        _ => return Err(fail!("the text change is not one difference")),
    };
    let (r, _) = ratchet::ratchet(&res, baseline()?.as_ref(), &mut [], &run.site);
    results.push((
        "an unlisted new diff fails".into(),
        !r.unlisted.is_empty() && r.listed.is_empty(),
        format!("{} unlisted: {} {key}", r.unlisted.len(), level.name()),
    ));

    let changes_dir = tmp.join("changes");
    let entry = format!(
        "- {} {} `{key}` accepted-deviation: the self-test's text change\n",
        run.site,
        level.name()
    );
    write(
        &changes_dir.join("SELFTEST.md"),
        format!("# SELFTEST\n\n{entry}").as_bytes(),
    )?;
    let (mut changes, errors) = ratchet::load_changes(&changes_dir, &["SELFTEST".into()])?;
    let (r, doc) = ratchet::ratchet(&res, baseline()?.as_ref(), &mut changes, &run.site);
    doc.write(&path)?;
    let stored = baseline()?.and_then(|b| b.files.get(&key)?.get(&level).cloned());
    let written = matches!(
        &stored,
        Some(Accepted::Difference { class, task, .. }) if class == "accepted-deviation" && task == "SELFTEST"
    );
    results.push((
        "a listed diff passes and --update writes it".into(),
        errors.is_empty() && r.unlisted.is_empty() && r.listed.len() == 1 && written,
        format!(
            "baseline entry {}",
            stored
                .as_ref()
                .map_or_else(|| "none".to_owned(), json::line)
        ),
    ));

    let (r, _) = ratchet::ratchet(&res, baseline()?.as_ref(), &mut [], &run.site);
    results.push((
        "the same diff against the updated baseline passes".into(),
        r.unlisted.is_empty() && r.listed.is_empty() && r.improved.is_empty(),
        "unchanged".into(),
    ));

    let (r, _) = ratchet::ratchet(&base_res, baseline()?.as_ref(), &mut [], &run.site);
    results.push((
        "an unlisted improvement does not fail".into(),
        r.unlisted.is_empty() && r.improved.len() == 1,
        format!("{} improved", r.improved.len()),
    ));

    write(
        &changes_dir.join("BAD.md"),
        format!(
            "- {0} L3 `x` fixed: no such class\n- {0} L3 `x` bug-fixed:\n",
            run.site
        )
        .as_bytes(),
    )?;
    let (_, errors) = ratchet::load_changes(&changes_dir, &["BAD".into()])?;
    results.push((
        "a changes entry without one triage class and a reason is an error".into(),
        errors.len() == 2,
        format!("{} errors", errors.len()),
    ));
    Ok(results)
}

// ---------------------------------------------------------------------------------------------

/// Runs the self-test, printing a line per check; the number of failed checks.
///
/// # Errors
/// A perturbation that finds nothing to perturb, or a failed read or write.
pub fn run(go_out: Option<&Path>, project: Option<&Path>, keep: bool) -> Result<usize, Fail> {
    let tmp = tempfile::Builder::new()
        .prefix("ssg-selftest.")
        .tempdir()
        .map_err(|e| fail!("a temporary directory: {e}"))?;
    let result = run_in(tmp.path(), go_out, project);
    if keep {
        println!("selftest: kept {}", tmp.keep().display());
    }
    result
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), Fail> {
    for rel in manifest::walk(from) {
        let src = from.join(&rel);
        write(
            &to.join(&rel),
            &std::fs::read(&src).map_err(|e| fail!("{}: {e}", src.display()))?,
        )?;
    }
    Ok(())
}

fn run_in(tmp: &Path, go_out: Option<&Path>, project: Option<&Path>) -> Result<usize, Fail> {
    let (src, project, label) = if let Some(out) = go_out {
        (out.to_owned(), project.map(Path::to_owned), "go-out")
    } else {
        let src = tmp.join("testsite");
        testsite_output(&src)?;
        // The project directory only gives the extractor the base URL.
        let project = tmp.join("testsite-project");
        write(
            &project.join("config.toml"),
            b"baseURL = \"https://example.org/\"\n",
        )?;
        (src, Some(project), "testsite (Go output + Thai page + PNG)")
    };
    let base = tmp.join("base");
    copy_dir(&src, &base)?;
    let run = Run {
        site: "selftest".into(),
        project,
    };
    println!(
        "selftest: Go output of {label}: {} files",
        manifest::walk(&base).len()
    );
    let mut failures = 0;
    let ident = diffs(&run.compare(&base, &base)?);
    println!(
        "  {}  identity: {} differences",
        pass(ident.is_empty()),
        ident.len()
    );
    failures += usize::from(!ident.is_empty());
    let man = run.manifest(&base, "unminified")?;
    let mut text_dir = None;
    for (i, (name, perturb)) in PERTURBATIONS.iter().enumerate() {
        let d = tmp.join(format!("p{}", i + 1));
        copy_dir(&base, &d)?;
        let (what, want) = perturb(&d, &run, &man)?;
        let got = diffs(&run.compare(&base, &d)?);
        let ok = check(&got, &want);
        failures += usize::from(!ok);
        println!("  {}  {}. {name}: {what}", pass(ok), i + 1);
        println!("          expected {}", listing(want.into_iter(), 4));
        if !ok {
            println!(
                "          got      {}",
                listing(got.into_iter().map(|(k, c)| (k, c.join(", "))), 4)
            );
        }
        if *name == "change visible text" {
            text_dir = Some(d);
        }
    }
    let text_dir = text_dir.expect("the text perturbation ran");
    for (name, ok, detail) in ratchet_checks(&run, &base, &text_dir, tmp)? {
        failures += usize::from(!ok);
        println!("  {}  ratchet: {name} ({detail})", pass(ok));
    }
    let verdict = if failures == 0 {
        "all checks pass".to_owned()
    } else {
        format!("{failures} FAILED")
    };
    println!("selftest: {verdict}");
    Ok(failures)
}
