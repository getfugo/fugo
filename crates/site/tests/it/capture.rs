//! Capture against the Go oracle `oracle/sitebuild/capture/<site>.json.gz` (Go's content trees
//! right after the Go build creates its sites, before drafts are removed): per language the
//! page tree (key → file, kind), the resource tree (key → file, bundled page or not), and for
//! every page the capture overrides (`kind`, `lang`, `path`, their normalised params) and its
//! own cascade. The model is built with drafts, future and expired content included.
//!
//! The `sc-err-*` fixtures record shortcode errors of the content phase (T34), not capture.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value as J, json};
use ssg_base::{Idx, LangIdx, Value};
use ssg_site::{Model, PageRole};
use ssg_testkit::fixture::oracle;

use crate::expected;
use crate::support::{Site, diff, to_json};

/// (language, key, file, kind).
type PageRec = (usize, String, String, String);
/// (language, key, file, bundled page).
type ResRec = (usize, String, String, bool);

fn nodes(n: &J) -> Vec<&J> {
    match n["nodes"].as_array() {
        Some(v) => v.iter().filter(|x| !x.is_null()).collect(),
        None => vec![n],
    }
}

fn key_path(k: &str) -> String {
    if k.is_empty() {
        "/".to_owned()
    } else {
        k.to_owned()
    }
}

fn idx(v: &J) -> usize {
    usize::try_from(v.as_u64().unwrap()).unwrap()
}

fn want(dump: &J) -> (BTreeSet<PageRec>, BTreeSet<ResRec>) {
    let pages = dump["pages"].as_array().unwrap();
    let file = |p: &J| {
        p["file"]["filename"]
            .as_str()
            .unwrap_or_default()
            .to_owned()
    };
    let mut pr = BTreeSet::new();
    for e in dump["trees"]["pages"].as_array().unwrap() {
        for n in nodes(&e["node"]) {
            let p = &pages[idx(&n["page"])];
            pr.insert((
                idx(&n["lang"]),
                key_path(e["key"].as_str().unwrap()),
                file(p),
                p["kind"].as_str().unwrap().to_owned(),
            ));
        }
    }
    let mut rr = BTreeSet::new();
    for e in dump["trees"]["resources"].as_array().unwrap() {
        for n in nodes(&e["node"]) {
            let (f, bundled) = match n.get("page") {
                Some(i) => (file(&pages[idx(i)]), true),
                None => (n["filename"].as_str().unwrap().to_owned(), false),
            };
            rr.insert((
                idx(&n["lang"]),
                key_path(e["key"].as_str().unwrap()),
                f,
                bundled,
            ));
        }
    }
    (pr, rr)
}

fn got(site: &Site, m: &Model) -> (BTreeSet<PageRec>, BTreeSet<ResRec>) {
    let mut pr = BTreeSet::new();
    let mut rr = BTreeSet::new();
    for (lang, s) in m.sites.iter_enumerated() {
        for (key, id) in s.tree.iter() {
            let p = m.page(id);
            if p.source.is_none() {
                continue;
            }
            let file = p
                .source
                .as_ref()
                .map(|s| site.norm(&s.file.abs))
                .unwrap_or_default();
            pr.insert((
                lang.index(),
                key.to_path(),
                file,
                p.kind.as_str().to_owned(),
            ));
        }
        for (key, rid) in &s.resources {
            let r = &m.bundle_resources[*rid];
            if r.copy_of.is_some() {
                continue;
            }
            rr.insert((
                lang.index(),
                key.to_path(),
                site.norm(&r.file.abs),
                r.page.is_some(),
            ));
        }
    }
    (pr, rr)
}

/// A page's own cascade as the oracle writes it.
fn cascade_json(c: &ssg_page::Cascade) -> J {
    J::Array(
        c.rules()
            .iter()
            .map(|r| {
                let [kind, path, lang, environment] = r.target.sources();
                json!({
                    "fields": to_json(&Value::map(r.fields.as_map().clone())),
                    "params": to_json(&Value::map(r.params.as_map().clone())),
                    "target": {"environment": environment, "kind": kind, "lang": lang, "path": path},
                })
            })
            .collect(),
    )
}

/// Checks one site; returns the number of pages and resources compared.
fn check(name: &str) -> usize {
    let f: J = oracle(&format!("oracle/sitebuild/capture/{name}.json.gz"));
    let site = Site::new(&f["site"]);
    let m = site.model_all().unwrap_or_else(|e| panic!("{name}: {e}"));
    let dev = expected::capture(name);

    let (wp, wr) = want(&f["dump"]);
    let (gp, gr) = got(&site, &m);
    let missing: BTreeSet<_> = wp.difference(&gp).filter(|r| !dev.contains(&r.1)).collect();
    let extra: BTreeSet<_> = gp.difference(&wp).filter(|r| !dev.contains(&r.1)).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "{name}: pages: Go only {missing:#?}\nRust only {extra:#?}"
    );
    let missing: Vec<_> = wr.difference(&gr).collect();
    let extra: Vec<_> = gr.difference(&wr).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "{name}: resources: Go only {missing:#?}\nRust only {extra:#?}"
    );

    // Overrides and own cascades, per page with a file.
    let by_file: BTreeMap<(usize, String), &ssg_site::Page> = m
        .pages
        .iter()
        .filter_map(|p| {
            let s = p.source.as_ref()?;
            Some(((p.lang.index(), site.norm(&s.file.abs)), p))
        })
        .collect();
    let mut checked = 0;
    for p in f["dump"]["pages"].as_array().unwrap() {
        let Some(file) = p["file"]["filename"].as_str() else {
            continue;
        };
        let lang = idx(&p["siteIdx"]);
        let Some(page) = by_file.get(&(lang, file.to_owned())) else {
            if dev.contains(&key_path(
                p["pathInfo"]["base"].as_str().unwrap_or_default(),
            )) {
                continue;
            }
            panic!("{name}: no page for {file} in language {lang}");
        };
        let pc = &p["pageConfig"];
        assert_eq!(
            p["bundled"].as_bool().unwrap(),
            matches!(page.role, PageRole::Bundled { .. }),
            "{name}: {file}: bundled"
        );
        if !dev.contains(&page.key.to_path()) {
            assert_eq!(
                pc["kind"].as_str().unwrap(),
                page.kind.as_str(),
                "{name}: {file}"
            );
        }
        let key = |k: &str| page.params().get(k).map_or(J::Null, to_json);
        for k in ["kind", "lang", "path"] {
            let want = pc["params"].get(k).cloned().unwrap_or(J::Null);
            assert_eq!(key(k), want, "{name}: {file}: params.{k}");
        }
        if let Some(l) = pc["lang"].as_str().filter(|l| !l.is_empty()) {
            assert_eq!(
                m.config.sites[LangIdx::from_index(lang)].language.key,
                l,
                "{name}: {file}: lang"
            );
        }
        if let Some(path) = pc["path"].as_str().filter(|l| !l.is_empty()) {
            assert_eq!(page.key.to_path(), path, "{name}: {file}: path");
        }
        let want_cascade = if pc["cascade"].is_null() {
            json!([])
        } else {
            pc["cascade"].clone()
        };
        if let Some(d) = diff("cascade", &cascade_json(&page.meta.cascade), &want_cascade) {
            panic!("{name}: {file}: {d}");
        }
        checked += 1;
    }

    let go_dups = f["log"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|l| l.as_str().unwrap().contains("Duplicate content path"))
        .count();
    let dups = m
        .diagnostics
        .iter()
        .filter(|d| d.id.as_deref() == Some("duplicate-content-path"))
        .count();
    assert_eq!(dups, go_dups, "{name}: duplicate warnings");
    eprintln!(
        "capture {name}: {} tree pages, {} resources, {checked} page configs equal, {dups} \
         duplicates",
        gp.len(),
        gr.len()
    );
    gp.len() + gr.len()
}

#[test]
fn capture_testsite() {
    assert_eq!(check("testsite"), 2);
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
