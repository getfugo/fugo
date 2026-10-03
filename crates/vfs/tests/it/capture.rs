//! Content discovery against the Go capture oracle `oracle/sitebuild/capture/<site>.json.gz`: for
//! every file that became a page or a resource in the Go implementation's content trees
//! (`treePages`, `treeResources` once its sites are created), the same (file, key, language,
//! bundle kind), and for pages the same name, section, extension and original base.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use serde_json::Value as J;
use ssg_base::{Idx, LangIdx, PageKind};
use ssg_config::{Config, LoadOptions, load};
use ssg_testkit::fixture::{oracle, repo_file};
use ssg_vfs::{BundleKind, PathParser, Vfs};

/// FNV-1a 64 as 16 hex digits (the oracle's hash of repository files).
fn fnv(b: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &c in b {
        h ^= u64::from(c);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// Writes the recorded site (`config.toml` and its files; repository files must still have their
/// recorded hash) into `dir`.
fn write_site(site: &J, dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join("config.toml"), site["toml"].as_str().unwrap()).unwrap();
    for f in site["files"].as_array().unwrap() {
        let p = f["path"].as_str().unwrap();
        let content = if let Some(r) = f["repo"].as_str() {
            let b = fs::read(repo_file(r)).unwrap_or_else(|e| panic!("{r}: {e}"));
            assert_eq!(fnv(&b), f["fnv"].as_str().unwrap(), "{r} changed");
            b
        } else {
            f["content"].as_str().unwrap().as_bytes().to_vec()
        };
        let fp = dir.join(p);
        fs::create_dir_all(fp.parent().unwrap()).unwrap();
        fs::write(fp, content).unwrap();
    }
}

/// Go's `paths.Type` number.
fn go_type(k: BundleKind) -> i64 {
    match k {
        BundleKind::Resource => 0,
        BundleKind::ContentResource => 1,
        BundleKind::Single => 2,
        BundleKind::Leaf => 3,
        BundleKind::Branch => 4,
        BundleKind::ContentAdapter => 5,
    }
}

/// (file below the site, key with a leading slash, language, Go type).
type Rec = (String, String, String, i64);
/// (file, name, section, extension, original base) of a page.
type Names = (String, String, String, String, String);

#[derive(Default)]
struct Want {
    recs: BTreeSet<Rec>,
    names: BTreeSet<Names>,
    /// Files whose front matter sets `path`: the page moves; not a file-system matter.
    moved: BTreeSet<String>,
}

fn site_file(filename: &str) -> String {
    filename
        .strip_prefix("/SITE/")
        .unwrap_or(filename)
        .to_owned()
}

fn nodes(n: &J) -> Vec<&J> {
    match n["nodes"].as_array() {
        Some(v) => v.iter().filter(|x| !x.is_null()).collect(),
        None => vec![n],
    }
}

fn want(dump: &J, cfg: &Config) -> Want {
    let pages = dump["pages"].as_array().unwrap();
    let lang_key = |i: &J| {
        let i = usize::try_from(i.as_u64().unwrap()).unwrap();
        cfg.sites[LangIdx::from_index(i)].language.key.clone()
    };
    let mut w = Want::default();
    let page = |w: &mut Want, i: &J| {
        let p = &pages[usize::try_from(i.as_u64().unwrap()).unwrap()];
        let Some(file) = p["file"]["filename"].as_str() else {
            return;
        };
        let file = site_file(file);
        if p["pageConfig"]["path"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
        {
            w.moved.insert(file);
            return;
        }
        let pi = &p["pathInfo"];
        let s = |k: &str| pi[k].as_str().unwrap().to_owned();
        w.recs.insert((
            file.clone(),
            s("base"),
            p["file"]["lang"].as_str().unwrap().to_owned(),
            pi["type"].as_i64().unwrap(),
        ));
        w.names.insert((
            file,
            s("baseNameNoIdentifier"),
            s("section"),
            s("ext"),
            s("unnormalizedBase"),
        ));
    };
    for e in dump["trees"]["pages"].as_array().unwrap() {
        for n in nodes(&e["node"]) {
            page(&mut w, &n["page"]);
        }
    }
    for e in dump["trees"]["resources"].as_array().unwrap() {
        for n in nodes(&e["node"]) {
            if n.get("page").is_some() {
                page(&mut w, &n["page"]);
            } else {
                w.recs.insert((
                    site_file(n["filename"].as_str().unwrap()),
                    e["key"].as_str().unwrap().to_owned(),
                    lang_key(&n["lang"]),
                    0,
                ));
            }
        }
    }
    w
}

/// Checks one site; returns the number of files compared.
fn check(name: &str) -> usize {
    let f: J = oracle(&format!("oracle/sitebuild/capture/{name}.json.gz"));
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join(f["site"]["name"].as_str().unwrap());
    write_site(&f["site"], &dir);
    let root = tmp.path().to_str().unwrap();
    let cfg = load(&LoadOptions {
        source: dir.clone(),
        env: vec![("HOME".into(), format!("{root}/home"))],
        ..LoadOptions::default()
    })
    .unwrap_or_else(|e| panic!("{name}: {e}"));
    let vfs = Vfs::new(&cfg).unwrap();
    let parser = PathParser::from_config(&cfg);
    let found = vfs.discover_content(&parser).unwrap();

    let w = want(&f["dump"], &cfg);
    let mut recs = BTreeSet::new();
    let mut names = BTreeSet::new();
    let disabled_pages = cfg.sites[LangIdx::from_index(0)]
        .disable_kinds
        .contains(PageKind::Page);
    let mut dropped_kind = 0;
    for c in &found.files {
        let file = c
            .file
            .abs
            .strip_prefix(&dir)
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let i = &c.info;
        if w.moved.contains(&file) {
            continue;
        }
        if disabled_pages && matches!(i.kind, BundleKind::Single | BundleKind::Leaf) {
            // Pages of a disabled kind are dropped when kinds are assigned (the site's job).
            dropped_kind += 1;
            continue;
        }
        recs.insert((
            file.clone(),
            i.key.to_path(),
            cfg.sites[c.lang].language.key.clone(),
            go_type(i.kind),
        ));
        if i.kind.is_content() {
            names.insert((
                file,
                i.name.clone(),
                i.section.clone(),
                i.ext.clone(),
                i.original.base.clone(),
            ));
        }
    }
    let go_duplicates = f["log"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l.as_str().unwrap().contains("Duplicate content path"))
        .count();
    assert_eq!(found.duplicates.len(), go_duplicates, "{name}: duplicates");
    let missing: Vec<_> = w.recs.difference(&recs).collect();
    let extra: Vec<_> = recs.difference(&w.recs).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "{name}: missing (Go only) {missing:#?}\nextra (Rust only) {extra:#?}"
    );
    let missing: Vec<_> = w.names.difference(&names).collect();
    let extra: Vec<_> = names.difference(&w.names).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "{name}: names: Go {missing:#?}\nRust {extra:#?}"
    );
    eprintln!(
        "capture {name}: {} files equal ({} pages), {} moved by front matter, {} pages of a \
         disabled kind, {} duplicates",
        recs.len(),
        names.len(),
        w.moved.len(),
        dropped_kind,
        found.duplicates.len()
    );
    recs.len()
}

#[test]
fn capture_testsite() {
    assert!(check("testsite") >= 2);
}

#[test]
fn capture_docs() {
    assert!(check("docs") >= 900);
}

#[test]
fn capture_other_oracle_sites() {
    for name in [
        "contentdir",
        "edge-tree",
        "homeleaf",
        "nokinds",
        "shortcodes",
        "synthetic",
    ] {
        check(name);
    }
}
