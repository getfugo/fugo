//! Which pages are written in which formats, and to which files, against the Go build oracle
//! `oracle/sitebuild/build/<site>.json.gz` (the renders and the files of a full build): the
//! (language, page, format) triples Go rendered are among the rendered pages' formats here,
//! and their target files are files Go wrote. (Go skips a format without a template; which
//! templates exist is the layouts' concern, so formats rendered only here are not counted.)

use std::collections::BTreeSet;

use serde_json::Value as J;
use ssg_testkit::fixture::{oracle, testdata};

use crate::structure::Tally;
use crate::support::Site;

fn s(v: &J) -> &str {
    v.as_str().unwrap_or_default()
}

fn check(name: &str, t: &mut Tally) {
    let f: J = oracle(&format!("oracle/sitebuild/build/{name}.json.gz"));
    let site = Site::new(&f["site"]);
    let m = site.model().unwrap_or_else(|e| panic!("{name}: {e}"));
    let dev = crate::expected::assemble(name);

    let want: BTreeSet<(String, String, String)> = f["renders"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == "page" && !dev.contains(s(&r["path"])))
        .map(|r| {
            (
                s(&r["lang"]).to_owned(),
                s(&r["path"]).to_owned(),
                s(&r["format"]).to_owned(),
            )
        })
        .collect();
    // Empty outputs are not written.
    let empty: BTreeSet<(String, String, String)> = f["renders"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == "page" && r["empty"] == true)
        .map(|r| {
            (
                s(&r["lang"]).to_owned(),
                s(&r["path"]).to_owned(),
                s(&r["format"]).to_owned(),
            )
        })
        .collect();
    let written: BTreeSet<&str> = f["events"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| s(&e["path"]))
        .collect();
    let mut got = BTreeSet::new();
    for p in &m.pages {
        if !p.rendered() || p.role != ssg_site::PageRole::Standalone || dev.contains(&p.path()) {
            continue;
        }
        let lang = &m.config.sites[p.lang].language.key;
        for u in &p.urls {
            let format = &m.config.output_formats.get(u.format).name;
            let triple = (lang.clone(), p.path(), format.clone());
            got.insert(triple.clone());
            if !want.contains(&triple) || empty.contains(&triple) {
                continue;
            }
            let file = u.paths.target.as_str().trim_start_matches('/');
            t.check("target written", written.contains(file), || {
                format!(
                    "{name}: {} ({lang}) {format}: {file} is not a file of Go's build",
                    p.path()
                )
            });
        }
    }
    // Pagers 2..N of every paginated (page, format): `Model::pager_paths`.
    for pg in f["pagers"].as_array().unwrap() {
        let path = s(&pg["path"]);
        let Some(p) = m
            .pages
            .iter()
            .find(|p| p.path() == path && m.config.sites[p.lang].language.key == s(&pg["lang"]))
        else {
            t.check("pager targets", false, || {
                format!("{name}: no page for {pg}")
            });
            continue;
        };
        let Some(format) = m.config.output_formats.by_name(s(&pg["format"])) else {
            continue;
        };
        let segment = &m.config.sites[p.lang].pagination.path;
        for n in 2..=pg["total"].as_u64().unwrap() {
            let file = m
                .pager_paths(p.id, format, &format!("/{segment}/{n}"))
                .map(|tp| tp.target.as_str().trim_start_matches('/').to_owned())
                .unwrap_or_default();
            t.check("pager targets", written.contains(file.as_str()), || {
                format!(
                    "{name}: {path} {} pager {n}: {file} is not a file of Go's build",
                    s(&pg["format"])
                )
            });
        }
    }
    for w in want.difference(&got) {
        t.check("rendered", false, || {
            format!("{name}: rendered by Go only {w:?}")
        });
    }
    for _ in want.intersection(&got) {
        t.check("rendered", true, String::new);
    }
}

#[test]
fn rendered_pages_and_targets_match_go_build() {
    let dir = testdata("oracle/sitebuild/build");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter_map(|n| n.strip_suffix(".json.gz").map(str::to_owned))
        .collect();
    names.sort();
    let mut t = Tally::default();
    for name in &names {
        check(name, &mut t);
    }
    eprintln!("build: {} sites", names.len());
    t.finish("build");
}
