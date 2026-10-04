//! The structure of the model against the Go oracle `oracle/sitebuild/assemble/<site>.json.gz`
//! (Go's pages after assembly, with their outputs, relations, lists, taxonomies, page
//! lookups and resources): every page Go makes, and for every page its names, dates,
//! relations, and per format its output file, link, resource directory and permalinks.
//!
//! Every difference is exact or falls in a reviewed class of `expected_diffs.toml`
//! (`[[class]]`, with its exact count over all sites).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value as J, json};
use ssg_base::{Idx, LangIdx, PageId, PageKind};
use ssg_page::{ListMode, RenderMode};
use ssg_site::{Model, Page, PageRole, RefError};
use ssg_testkit::fixture::oracle;

use crate::expected;
use crate::support::{Site, diff, time_json, to_json};

mod pages;
mod site;

use pages::*;
use site::*;

/// The oracle sites.
pub const SITES: [&str; 16] = [
    "asm-build",
    "asm-cascade",
    "asm-flags",
    "asm-i18n",
    "asm-multihost",
    "asm-taxo",
    "asm-ugly",
    "content",
    "contentdir",
    "docs",
    "edge-tree",
    "homeleaf",
    "nokinds",
    "shortcodes",
    "synthetic",
    "testsite",
];

/// Pass, accept and fail counts per check.
#[derive(Default)]
pub struct Tally {
    pub checks: BTreeMap<&'static str, (usize, usize)>,
    pub accepted: BTreeMap<&'static str, usize>,
    pub failures: Vec<String>,
}

impl Tally {
    pub fn check(&mut self, check: &'static str, ok: bool, why: impl FnOnce() -> String) {
        let e = self.checks.entry(check).or_default();
        e.0 += 1;
        if ok {
            e.1 += 1;
        } else {
            self.failures.push(format!("[{check}] {}", why()));
        }
    }

    pub fn accept(&mut self, check: &'static str, class: &'static str) {
        self.checks.entry(check).or_default().0 += 1;
        *self.accepted.entry(class).or_default() += 1;
    }

    /// Prints the table; fails on any failure or on accepted counts that differ from
    /// `expected_diffs.toml`.
    pub fn finish(&self, name: &str) {
        let (mut total, mut exact) = (0, 0);
        for (check, (n, ok)) in &self.checks {
            eprintln!("{name}: {check:<22} {ok:>6}/{n:<6} exact");
            total += n;
            exact += ok;
        }
        for (class, n) in &self.accepted {
            eprintln!("{name}: accepted {class}: {n}");
        }
        eprintln!(
            "{name}: {exact}/{total} exact, {} accepted",
            total - exact - self.failures.len()
        );
        assert!(
            self.failures.is_empty(),
            "{name}: {} unexplained differences:\n{}",
            self.failures.len(),
            self.failures[..self.failures.len().min(40)].join("\n")
        );
        let want = expected::classes(name);
        let got: BTreeMap<String, usize> = self
            .accepted
            .iter()
            .map(|(k, v)| ((*k).to_owned(), *v))
            .collect();
        assert_eq!(
            got, want,
            "{name}: accepted counts differ from expected_diffs.toml"
        );
    }
}

/// The oracle's pages mapped to the model's.
struct Index<'a> {
    go: &'a [J],
    to_ours: Vec<Option<PageId>>,
    to_go: BTreeMap<PageId, usize>,
}

impl<'a> Index<'a> {
    fn new(site: &Site, m: &Model, go: &'a [J]) -> Self {
        let mut by_file: BTreeMap<(usize, String), PageId> = BTreeMap::new();
        let mut by_path: BTreeMap<(usize, String, String), PageId> = BTreeMap::new();
        for p in &m.pages {
            if let Some(s) = &p.source {
                by_file.insert((p.lang.index(), site.norm(&s.file.abs)), p.id);
            } else {
                by_path.insert((p.lang.index(), p.path(), p.kind.as_str().to_owned()), p.id);
            }
        }
        let mut to_ours = Vec::new();
        let mut to_go = BTreeMap::new();
        for (i, g) in go.iter().enumerate() {
            let lang = usize::try_from(g["site"].as_u64().unwrap()).unwrap();
            let file = g["file"].as_str().unwrap();
            let id = if file.is_empty() {
                by_path
                    .get(&(lang, s(&g["path"]).to_owned(), s(&g["kind"]).to_owned()))
                    .copied()
            } else {
                by_file.get(&(lang, file.to_owned())).copied()
            };
            if let Some(id) = id {
                to_go.insert(id, i);
            }
            to_ours.push(id);
        }
        Self { go, to_ours, to_go }
    }

    fn go_ref(&self, id: PageId) -> J {
        self.to_go.get(&id).map_or(json!("?"), |i| json!(i))
    }

    /// A list of our pages as the oracle writes it.
    fn list(&self, ids: &[PageId]) -> J {
        J::Array(
            ids.iter()
                .map(|&id| json!({"p": self.go_ref(id)}))
                .collect(),
        )
    }

    fn opt(&self, id: Option<PageId>) -> J {
        id.map_or(J::Null, |id| self.go_ref(id))
    }
}

fn s(v: &J) -> &str {
    v.as_str().unwrap_or_default()
}

/// A list without ordinals (`[{p}]`); `null` is empty.
fn plain_list(v: &J) -> J {
    J::Array(
        v.as_array()
            .map(|a| a.iter().map(|e| json!({"p": e["p"]})).collect())
            .unwrap_or_default(),
    )
}

fn build_json(p: &Page) -> J {
    let b = p.meta.build;
    json!({
        "list": match b.list { ListMode::Always => "always", ListMode::Never => "never", ListMode::Local => "local" },
        "render": match b.render { RenderMode::Always => "always", RenderMode::Never => "never", RenderMode::Link => "link" },
        "publishResources": b.publish_resources,
    })
}

/// `""` for the root and for "none" (as Go writes resource directories).
fn dir_str(p: &str) -> &str {
    if p == "/" { "" } else { p }
}

fn check(name: &str, t: &mut Tally) {
    let f: J = oracle(&format!("oracle/sitebuild/assemble/{name}.json.gz"));
    let site = Site::new(&f["site"]);
    let m = site.model().unwrap_or_else(|e| panic!("{name}: {e}"));
    let dev = expected::assemble(name);
    let go = f["dump"]["pages"].as_array().unwrap();
    let ix = Index::new(&site, &m, go);

    // The page set: every page Go made or read, per language.
    let want: BTreeSet<(usize, String, String)> = go
        .iter()
        .filter(|g| !dev.contains(s(&g["path"])))
        .map(|g| {
            (
                usize::try_from(g["site"].as_u64().unwrap()).unwrap(),
                s(&g["kind"]).to_owned(),
                format!("{} {}", s(&g["path"]), s(&g["file"])),
            )
        })
        .collect();
    let got: BTreeSet<(usize, String, String)> = m
        .pages
        .iter()
        .filter(|p| !dev.contains(&p.path()))
        .map(|p| {
            let (path, file) = match &p.source {
                Some(src) => {
                    let path = if p.role == PageRole::Standalone {
                        p.path()
                    } else {
                        // Bundled pages: Go's path is the file's key without extension.
                        ix.to_go
                            .get(&p.id)
                            .map_or_else(|| p.path(), |&i| s(&go[i]["path"]).to_owned())
                    };
                    (path, site.norm(&src.file.abs))
                }
                None => (p.path(), String::new()),
            };
            (
                p.lang.index(),
                p.kind.as_str().to_owned(),
                format!("{path} {file}"),
            )
        })
        .collect();
    for w in want.difference(&got) {
        t.check("page set", false, || format!("{name}: Go only {w:?}"));
    }
    for g in got.difference(&want) {
        t.check("page set", false, || format!("{name}: Rust only {g:?}"));
    }
    for _ in want.intersection(&got) {
        t.check("page set", true, String::new);
    }

    check_pages(name, &m, &ix, &dev, t);
    for (i, w) in f["dump"]["sites"].as_array().unwrap().iter().enumerate() {
        check_site(name, &m, &ix, LangIdx::from_index(i), w, &dev, t);
    }
}

#[test]
fn structure_matches_assemble_oracle() {
    let mut t = Tally::default();
    for name in SITES {
        check(name, &mut t);
    }
    t.finish("structure");
}
