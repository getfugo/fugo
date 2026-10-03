//! The structure-oracle gate of T23b (REWRITE_PLAN.md §8.2): per (page, format) the target file,
//! `.RelPermalink` and `.Permalink`, and per (page, resource) the resource's URL and file, of
//! the Go build of a real site (`testdata/golden/<site>/structure.json[.gz]`, written by
//! T01's `structure` oracle) against the model of the same site.
//!
//! The site itself comes from `tools/rust-port/i01/sites.py make <site> <dir>`: set
//! `FUGO_SITES=<dir>[:<dir>…]` (the directory's name is the site's name). Without the
//! dump or the site the test says why it skips.
//!
//! **Schema** (the reader of T30, `crates/layouts/tests/it/structure.rs`, reads the same file;
//! these are its `records` plus the fields below, see the crate README):
//!
//! ```json
//! { "config": { … },
//!   "records": [ { "path": "/posts/p1", "kind": "page", "layout": "", "lang": "en",
//!                  "format": "html", "template": "single.html", "baseof": "baseof.html",
//!                  "target": "/posts/p1/index.html",
//!                  "relPermalink": "/posts/p1/",
//!                  "permalink": "https://example.org/posts/p1/" } ],
//!   "aliases": [ { "path": "/posts/p1", "lang": "en", "format": "html",
//!                  "alias": "/old/", "target": "/old/index.html" } ],
//!   "resources": [ { "path": "/posts/p1", "lang": "en", "name": "cover.jpg",
//!                    "relPermalink": "/posts/p1/cover.jpg",
//!                    "target": "/posts/p1/cover.jpg" } ] }
//! ```

use std::path::{Path, PathBuf};

use serde_json::Value as J;
use ssg_base::UrlPath;
use ssg_site::{Model, Page};
use ssg_testkit::fixture::{read_json, repo_dir};

use crate::structure::Tally;
use crate::support::Site;

fn s(v: &J) -> &str {
    v.as_str().unwrap_or_default()
}

fn dump_path(site: &str) -> Option<PathBuf> {
    let golden = repo_dir().join("testdata/golden").join(site);
    ["structure.json.gz", "structure.json"]
        .iter()
        .map(|f| golden.join(f))
        .find(|p| p.is_file())
}

fn page<'m>(m: &'m Model, r: &J) -> Option<&'m Page> {
    m.pages.iter().find(|p| {
        p.path() == s(&r["path"])
            && m.config.sites[p.lang].language.key == s(&r["lang"])
            && r.get("kind").is_none_or(|k| p.kind.as_str() == s(k))
    })
}

/// Compares one site's model with its dump.
pub fn check(label: &str, dir: &Path, dump: &Path, t: &mut Tally) {
    let doc: J = read_json(dump).unwrap_or_else(|e| panic!("{e}"));
    let tmp = tempfile::tempdir().unwrap();
    let site = Site::at(tmp, dir.to_owned());
    let m = site.model().unwrap_or_else(|e| panic!("{label}: {e}"));
    for r in doc["records"].as_array().into_iter().flatten() {
        let Some(want_target) = r.get("target") else {
            continue;
        };
        let at = || {
            format!(
                "{label} {} ({}) {}",
                s(&r["path"]),
                s(&r["lang"]),
                s(&r["format"])
            )
        };
        let Some(p) = page(&m, r) else {
            t.check("golden pages", false, || format!("{}: no such page", at()));
            continue;
        };
        let url = m
            .config
            .output_formats
            .by_name(s(&r["format"]))
            .and_then(|f| p.url(f));
        let Some(url) = url else {
            t.check("golden pages", false, || {
                format!("{}: no such output", at())
            });
            continue;
        };
        let target = url.paths.target.as_str();
        t.check("golden targets", target == s(want_target), || {
            format!("{}: target {target} != {want_target}", at())
        });
        let rel = url
            .links
            .as_ref()
            .map(|l| l.rel_permalink.escaped())
            .unwrap_or_default();
        let perma = url
            .links
            .as_ref()
            .map(|l| l.permalink.to_string())
            .unwrap_or_default();
        t.check(
            "golden permalinks",
            rel == s(&r["relPermalink"]) && perma == s(&r["permalink"]),
            || {
                format!(
                    "{}: {rel} {perma} != {} {}",
                    at(),
                    r["relPermalink"],
                    r["permalink"]
                )
            },
        );
    }
    for r in doc["resources"].as_array().into_iter().flatten() {
        let at = || {
            format!(
                "{label} {} ({}) {}",
                s(&r["path"]),
                s(&r["lang"]),
                s(&r["name"])
            )
        };
        let Some(p) = page(&m, r) else {
            t.check("golden resources", false, || {
                format!("{}: no such page", at())
            });
            continue;
        };
        let urls = m.config.sites[p.lang].site_urls();
        let found = p
            .resources
            .iter()
            .map(|&rid| &m.bundle_resources[rid])
            .find(|b| b.page.is_none() && b.name_normalized == s(&r["name"]));
        let got = found.map(|b| {
            (
                b.link()
                    .map(|l| UrlPath::new(&urls.prepend_base_path(l.as_str())).escaped())
                    .unwrap_or_default(),
                b.target()
                    .map(|t| t.as_str().to_owned())
                    .unwrap_or_default(),
            )
        });
        let want = (s(&r["relPermalink"]).to_owned(), s(&r["target"]).to_owned());
        t.check("golden resources", got.as_ref() == Some(&want), || {
            format!("{}: {got:?} != {want:?}", at())
        });
    }
}

#[test]
fn structure_oracle_targets_permalinks_resources() {
    let dirs = std::env::var("FUGO_SITES").unwrap_or_default();
    let mut t = Tally::default();
    let mut checked = 0;
    for dir in dirs.split(':').filter(|d| !d.is_empty()) {
        let dir = Path::new(dir);
        let label = dir.file_name().unwrap().to_string_lossy().into_owned();
        match dump_path(&label) {
            Some(dump) => {
                check(&label, dir, &dump, &mut t);
                checked += 1;
            }
            None => eprintln!(
                "structure oracle: skipping {label}: testdata/golden/{label} has no \
                 structure.json[.gz] (the golden data, frozen at 44529028, has none for this label)"
            ),
        }
    }
    if checked == 0 {
        eprintln!(
            "structure oracle: no site checked (needs FUGO_SITES=<dir made by sites.py> and \
             testdata/golden/<site>/structure.json[.gz])"
        );
    }
    t.finish("golden");
}

/// The reader on a hand-written dump of a small site.
#[test]
fn structure_oracle_reader_self_test() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("mini");
    let site = serde_json::json!({
        "name": "mini",
        "toml": "baseURL = \"https://example.org/sub/\"\n",
        "files": [
            {"path": "content/posts/p1/index.md", "content": "---\ntitle: P1\n---\n"},
            {"path": "content/posts/p1/Cover.JPG", "content": "jpg"},
        ],
    });
    crate::support::write_site(&site, &dir);
    let dump = serde_json::json!({
        "config": {},
        "records": [
            {"path": "/posts/p1", "kind": "page", "layout": "", "lang": "en", "format": "html",
             "template": "single.html", "baseof": "", "target": "/posts/p1/index.html",
             "relPermalink": "/sub/posts/p1/", "permalink": "https://example.org/sub/posts/p1/"},
            {"path": "/posts", "kind": "section", "layout": "", "lang": "en", "format": "rss",
             "template": "rss.xml", "baseof": "", "target": "/posts/index.xml",
             "relPermalink": "/sub/posts/index.xml",
             "permalink": "https://example.org/sub/posts/index.xml"},
        ],
        "resources": [
            {"path": "/posts/p1", "lang": "en", "name": "cover.jpg",
             "relPermalink": "/sub/posts/p1/Cover.JPG", "target": "/posts/p1/Cover.JPG"},
        ],
    });
    let p = tmp.path().join("structure.json");
    std::fs::write(&p, dump.to_string()).unwrap();
    let mut t = Tally::default();
    check("self-test", &dir, &p, &mut t);
    t.finish("golden-self-test");
}
