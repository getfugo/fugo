//! The structure-oracle gate of T30 (REWRITE_PLAN.md §8.2): for every (page, format) of the
//! target sites, the layout template and base template chosen from the converted layouts under
//! `sites/<site>/` must equal the ones Go chose (with Go's legacy names normalised to
//! v0.146 names by T01's normaliser).
//!
//! The dumps come from T01 (`tools/go-oracle/structure/`, frozen at 44529028); without
//! `testdata/golden/<site>/structure.json[.gz]` the test prints why it skips.
//! **Schema** (of the frozen dumps):
//!
//! ```json
//! { "config": { /* as in oracle/tplimpl/store/*.json.gz: defaultContentLanguage,
//!               languageIndex, disabledLanguages, mediaTypes, outputFormats,
//!               defaultOutputFormat */ },
//!   "records": [ { "path": "/posts/p1",   // the lookup path (first segment = type)
//!                  "kind": "page", "layout": "", "lang": "en", "format": "html",
//!                  "template": "single.html",   // normalised v0.146 name, "" = none
//!                  "baseof": "baseof.html" } ] } // "" = no base template
//! ```
//!
//! Docs patch variants (`golden/docs-<variant>/`) use `sites/docs/layouts` overlaid with
//! `sites/docs/patches/<variant>/layouts`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value as J;
use ssg_base::PageKind;
use ssg_base::paths::ContentKey;
use ssg_layouts::{LayoutQuery, LayoutSource, LayoutStore, Origin, TemplateName};

use crate::oracle::env_of;

fn dump_path(golden: &Path, label: &str) -> Option<PathBuf> {
    ["structure.json.gz", "structure.json"]
        .iter()
        .map(|f| golden.join(label).join(f))
        .find(|p| p.is_file())
}

fn read_layouts(dir: &Path, below: &str, out: &mut Vec<(String, PathBuf)>) {
    let Ok(rd) = fs::read_dir(dir.join(below)) else {
        return;
    };
    for e in rd {
        let e = e.unwrap();
        let name = e.file_name().into_string().unwrap();
        let rel = if below.is_empty() {
            name
        } else {
            format!("{below}/{name}")
        };
        if e.file_type().unwrap().is_dir() {
            read_layouts(dir, &rel, out);
        } else {
            out.push((rel, e.path()));
        }
    }
}

/// The plain name of a template, without the theme or embedded prefix.
fn plain(n: &TemplateName) -> String {
    let s = n.as_str();
    let s = s.strip_prefix(ssg_layouts::EMBEDDED_PREFIX).unwrap_or(s);
    match s.strip_prefix("_theme") {
        Some(rest) => rest.split_once('/').map_or(s, |(_, r)| r).to_owned(),
        None => s.to_owned(),
    }
}

fn check(label: &str, dump: &Path, layout_dirs: &[PathBuf]) {
    let doc: J = ssg_testkit::fixture::read_json(dump).unwrap_or_else(|e| panic!("{e}"));
    let (env, langs) = env_of(&doc["config"]);
    let mut files = std::collections::BTreeMap::new();
    for d in layout_dirs {
        let mut v = Vec::new();
        read_layouts(d, "", &mut v);
        for (rel, p) in v {
            files.insert(rel, p);
        }
    }
    let mut sources: Vec<LayoutSource> = files
        .into_iter()
        .map(|(rel, p)| LayoutSource {
            source: fs::read_to_string(&p).unwrap(),
            rel,
            origin: Origin::User(p),
        })
        .collect();
    sources.extend(
        ssg_layouts::embedded::TEMPLATES
            .iter()
            .map(|(rel, src)| LayoutSource {
                rel: (*rel).to_owned(),
                origin: Origin::Embedded,
                source: (*src).to_owned(),
            }),
    );
    let store = Arc::new(
        LayoutStore::from_sources(env.clone(), sources).unwrap_or_else(|e| panic!("{label}: {e}")),
    );
    let records = doc["records"].as_array().expect("records");
    let mut bad = Vec::new();
    for r in records {
        let f = |k: &str| r[k].as_str().unwrap_or_default();
        let Some(format) = env.formats().by_name(f("format")) else {
            bad.push(format!("unknown format in {r}"));
            continue;
        };
        let path = ContentKey::from_source(f("path"));
        let sel = store.select(&LayoutQuery {
            path: &path,
            kind: PageKind::parse(f("kind")),
            layout: Some(f("layout")).filter(|l| !l.is_empty()),
            exact_layout: false,
            lang: langs.get(f("lang")).copied(),
            format,
        });
        let got_t = sel.as_ref().map(|s| plain(&s.layout)).unwrap_or_default();
        let got_b = sel
            .as_ref()
            .and_then(|s| s.base.as_ref())
            .map(plain)
            .unwrap_or_default();
        if got_t != f("template") || got_b != f("baseof") {
            bad.push(format!(
                "{} {} {} {}: go {} + {}, rust {got_t} + {got_b}",
                f("path"),
                f("kind"),
                f("lang"),
                f("format"),
                f("template"),
                f("baseof")
            ));
        }
    }
    eprintln!(
        "{label}: {}/{} (page, format) template+baseof equal",
        records.len() - bad.len(),
        records.len()
    );
    assert!(
        bad.is_empty(),
        "{label}: {} differ:\n{}",
        bad.len(),
        bad[..bad.len().min(30)].join("\n")
    );
}

#[test]
fn structure_oracle_template_and_baseof() {
    let root = ssg_testkit::fixture::repo_dir();
    let golden = root.join("testdata/golden");
    let sites = root.join("sites");
    let mut checked = 0;
    let mut site_dirs: Vec<PathBuf> = fs::read_dir(&sites)
        .map(|rd| rd.map(|e| e.unwrap().path()).collect())
        .unwrap_or_default();
    site_dirs.sort();
    for site in site_dirs {
        let name = site.file_name().unwrap().to_string_lossy().into_owned();
        let base = site.join("layouts");
        let mut labels = vec![(name.clone(), vec![base.clone()])];
        if let Ok(rd) = fs::read_dir(site.join("patches")) {
            for v in rd {
                let v = v.unwrap().path();
                let variant = v.file_name().unwrap().to_string_lossy().into_owned();
                labels.push((
                    format!("{name}-{variant}"),
                    vec![base.clone(), v.join("layouts")],
                ));
            }
        }
        for (label, dirs) in labels {
            match dump_path(&golden, &label) {
                Some(dump) => {
                    check(&label, &dump, &dirs);
                    checked += 1;
                }
                None => eprintln!(
                    "structure oracle: skipping {label}: {} has no structure.json[.gz] \
                     (the golden data, frozen at 44529028, has none for this label)",
                    golden.join(&label).display()
                ),
            }
        }
    }
    eprintln!("structure oracle: {checked} site(s) checked");
}

/// The reader above on a hand-written dump (the converted testsite layouts, the testsite
/// configuration of the tplimpl oracle).
#[test]
fn structure_reader_self_test() {
    let root = ssg_testkit::fixture::repo_dir();
    let store_fx: J = crate::oracle::fixture("store", "testsite");
    let dump = serde_json::json!({
        "config": store_fx["config"],
        "records": [
            {"path": "/posts/p1", "kind": "page", "layout": "", "lang": "en", "format": "html",
             "template": "single.html", "baseof": ""},
            {"path": "/posts", "kind": "section", "layout": "", "lang": "en", "format": "html",
             "template": "list.html", "baseof": ""},
            {"path": "", "kind": "home", "layout": "", "lang": "en", "format": "json",
             "template": "home.json", "baseof": ""},
            {"path": "", "kind": "404", "layout": "", "lang": "en", "format": "404",
             "template": "404.html", "baseof": ""},
            // The embedded rss.xml (T32) serves every kind, as Go's does.
            {"path": "", "kind": "page", "layout": "", "lang": "en", "format": "rss",
             "template": "rss.xml", "baseof": ""},
        ],
    });
    let tmp = tempfile::tempdir().unwrap();
    let p = tmp.path().join("structure.json");
    fs::write(&p, dump.to_string()).unwrap();
    check("self-test", &p, &[root.join("sites/testsite/layouts")]);
}
