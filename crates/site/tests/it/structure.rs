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

/// Checks the pages of one site.
#[allow(clippy::too_many_lines)]
fn check_pages(name: &str, m: &Model, ix: &Index<'_>, dev: &BTreeSet<String>, t: &mut Tally) {
    let cfg = &m.config;
    for (i, g) in ix.go.iter().enumerate() {
        let Some(id) = ix.to_ours[i] else {
            continue;
        };
        let p = m.page(id);
        let at = || format!("{name} {} ({})", s(&g["path"]), s(&g["lang"]));
        let eq = |t: &mut Tally, check: &'static str, got: J, want: &J| {
            let d = diff(check, &got, want);
            t.check(check, d.is_none(), || {
                format!("{}: {}", at(), d.unwrap_or_default())
            });
        };
        eq(t, "title", json!(p.title), &g["title"]);
        eq(t, "linkTitle", json!(p.link_title), &g["linkTitle"]);
        eq(t, "type", json!(p.r#type), &g["type"]);
        eq(t, "section", json!(p.section), &g["section"]);
        if p.kind == PageKind::Term {
            let term = p
                .taxonomy
                .zip(p.term)
                .map(|(tx, tm)| m.sites[p.lang].taxonomies[tx].terms[tm].term.clone());
            eq(t, "term", json!(term.unwrap_or_default()), &g["term"]);
        }
        let d = &p.meta.dates;
        eq(
            t,
            "dates",
            json!({
                "date": time_json(d.date.as_ref()),
                "lastmod": time_json(d.lastmod.as_ref()),
                "publishDate": time_json(d.publish_date.as_ref()),
                "expiryDate": time_json(d.expiry_date.as_ref()),
            }),
            &g["dates"],
        );
        if p.source.is_none() {
            eq(
                t,
                "params (made pages)",
                to_json(&ssg_base::Value::map(p.meta.params.as_map().clone())),
                &g["params"],
            );
            eq(t, "build (made pages)", build_json(p), &g["build"]);
        }
        if p.role != PageRole::Standalone {
            continue;
        }
        eq(t, "parent", ix.opt(p.parent), &g["parent"]);
        eq(
            t,
            "currentSection",
            ix.go_ref(p.current_section),
            &g["currentSection"],
        );
        eq(
            t,
            "firstSection",
            ix.go_ref(p.first_section),
            &g["firstSection"],
        );
        if p.kind != PageKind::Page {
            let (got, want) = (ix.list(&p.sections), plain_list(&g["sections"]));
            let without_dev = |l: &J| {
                J::Array(
                    l.as_array()
                        .unwrap()
                        .iter()
                        .filter(|e| {
                            e["p"].as_u64().is_none_or(|i| {
                                !dev.contains(s(&ix.go[usize::try_from(i).unwrap()]["path"]))
                            })
                        })
                        .cloned()
                        .collect(),
                )
            };
            if got != want && without_dev(&got) == without_dev(&want) {
                t.accept("sections", "segment-wise-taxonomy-prefix");
            } else {
                eq(t, "sections", got, &want);
            }
        }

        // Outputs: per rendered format, the file, link and resource directory.
        let want_out: BTreeMap<&str, &J> = g["outputs"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|o| o["render"] == true)
            .map(|o| (s(&o["name"]), &o["target"]))
            .collect();
        let got_out: BTreeMap<&str, J> = p
            .urls
            .iter()
            .filter(|_| p.rendered())
            .map(|u| {
                let res = u.paths.resources.as_ref();
                (
                    cfg.output_formats.get(u.format).name.as_str(),
                    json!({
                        "filename": u.paths.target.as_str(),
                        "link": u.paths.link.escaped(),
                        "subTarget": res.map_or("", |r| dir_str(r.target.as_str())),
                        "subLink": res.map_or("", |r| dir_str(r.link.as_str())),
                    }),
                )
            })
            .collect();
        let got_names: Vec<&str> = got_out.keys().copied().collect();
        let want_names: Vec<&str> = want_out.keys().copied().collect();
        eq(t, "rendered formats", json!(got_names), &json!(want_names));
        for (f, w) in &want_out {
            if let Some(g2) = got_out.get(f) {
                let mut w = (*w).clone();
                // Output paths are clean: Go's `/th/section/` resource directory is
                // `/th/section` (ssg-page's accepted deviation).
                if let Some(J::String(d)) = w.get_mut("subTarget")
                    && d.len() > 1
                    && d.ends_with('/')
                {
                    d.pop();
                }
                eq(t, "target paths", g2.clone(), &w);
            }
        }
        // `.OutputFormats`: name, rel and links (none without a link).
        let got_of: Vec<J> = p
            .urls
            .iter()
            .filter_map(|u| {
                let l = u.links.as_ref()?;
                let f = cfg.output_formats.get(u.format);
                // Go's `rel` (a view detail): `canonical` for a page's only format when it is
                // a built-in one.
                let builtin = ssg_config::OutputFormats::builtin(&cfg.media_types)
                    .by_name(&f.name)
                    .is_some();
                let rel = if p.urls.len() == 1 && builtin {
                    "canonical"
                } else {
                    f.rel.as_str()
                };
                Some(json!({
                    "name": f.name,
                    "rel": rel,
                    "relPermalink": l.rel_permalink.escaped(),
                    "permalink": l.permalink.to_string(),
                }))
            })
            .collect();
        eq(
            t,
            "output formats",
            J::Array(got_of),
            &g["pageOutputFormats"],
        );
    }
}

/// Our term lists and taxonomies as the oracle writes them.
#[allow(clippy::too_many_lines)]
fn check_site(
    name: &str,
    m: &Model,
    ix: &Index<'_>,
    lang: LangIdx,
    w: &J,
    dev: &BTreeSet<String>,
    t: &mut Tally,
) {
    // A list without the pages of `expected_diffs.toml` deviations.
    let without_dev = |l: &J| {
        J::Array(
            l.as_array()
                .unwrap()
                .iter()
                .filter(|e| {
                    e["p"].as_u64().is_none_or(|i| {
                        !dev.contains(s(&ix.go[usize::try_from(i).unwrap()]["path"]))
                    })
                })
                .cloned()
                .collect(),
        )
    };
    let site = &m.sites[lang];
    let at = |what: &str| format!("{name} [{}] {what}", lang.index());
    let eq = |t: &mut Tally, check: &'static str, what: &str, got: J, want: &J| {
        let d = diff(check, &got, want);
        t.check(check, d.is_none(), || {
            format!("{}: {}", at(what), d.unwrap_or_default())
        });
    };
    eq(t, "home", "", ix.go_ref(site.home), &w["home"]);
    eq(
        t,
        "site lastmod",
        "",
        time_json(site.last_mod.as_ref()),
        &w["lastmod"],
    );
    let tie = &w["mainSectionsTie"];
    if tie.is_array() && w["mainSections"].is_null() {
        let ok = site.main_sections.len() == 1
            && tie
                .as_array()
                .unwrap()
                .contains(&json!(site.main_sections[0]));
        t.check("main sections", ok, || at("main sections (tie)"));
    } else {
        eq(
            t,
            "main sections",
            "",
            json!(site.main_sections),
            &w["mainSections"],
        );
    }
    eq(
        t,
        "site pages",
        "",
        ix.list(&site.pages),
        &plain_list(&w["sitePages"]),
    );
    eq(
        t,
        "site regular pages",
        "",
        ix.list(&site.regular_pages),
        &plain_list(&w["siteRegularPages"]),
    );

    for c in w["collections"].as_array().unwrap() {
        let gi = usize::try_from(c["p"].as_u64().unwrap()).unwrap();
        let Some(id) = ix.to_ours[gi] else {
            continue;
        };
        let p = m.page(id);
        let what = s(&ix.go[gi]["path"]).to_owned();
        if dev.contains(&what) {
            continue;
        }
        let (gp, gr) = (ix.list(&p.pages), ix.list(&p.regular_pages));
        let (wp, wr) = (plain_list(&c["pages"]), plain_list(&c["regularPages"]));
        if (gp != wp || gr != wr)
            && without_dev(&gp) == without_dev(&wp)
            && without_dev(&gr) == without_dev(&wr)
        {
            t.accept("pages", "segment-wise-taxonomy-prefix");
            continue;
        }
        if p.kind == PageKind::Term && wp == wr && (gp != wp || gr != wr) && (gp == wp || gr == wr)
        {
            // Go caches a term's `.Pages` and `.RegularPages` under one key: whichever is
            // asked first answers both.
            t.accept("pages", "term-lists-share-cache");
            continue;
        }
        eq(t, "pages", &what, gp, &wp);
        eq(t, "regular pages", &what, gr, &wr);
    }

    // `.Site.Taxonomies`: plural → lower-cased term → weighted pages.
    let mut got_tax: BTreeMap<String, BTreeMap<String, J>> = BTreeMap::new();
    for tx in &site.taxonomies {
        let e = got_tax.entry(tx.def.plural.clone()).or_default();
        for (_, term) in tx.listed_terms(m) {
            let list = J::Array(
                term.members
                    .iter()
                    .map(|wp| json!({"p": ix.go_ref(wp.page), "w": wp.weight}))
                    .collect(),
            );
            e.insert(ssg_base::text::to_lower(&term.term), list);
        }
    }
    let mut want_tax: BTreeMap<String, BTreeMap<String, J>> = BTreeMap::new();
    for e in w["taxonomies"].as_array().unwrap() {
        let terms = want_tax.entry(s(&e[0]).to_owned()).or_default();
        for te in e[1].as_array().into_iter().flatten() {
            terms.insert(s(&te[0]).to_owned(), te[1].clone());
        }
    }
    for (plural, want_terms) in &want_tax {
        let got_terms = got_tax.get(plural).cloned().unwrap_or_default();
        let keys = |m: &BTreeMap<String, J>| m.keys().cloned().collect::<Vec<_>>();
        eq(
            t,
            "taxonomy terms",
            plural,
            json!(keys(&got_terms)),
            &json!(keys(want_terms)),
        );
        for (k, wl) in want_terms {
            if let Some(gl) = got_terms.get(k) {
                eq(t, "term members", &format!("{plural}/{k}"), gl.clone(), wl);
            }
        }
    }

    // `.GetTerms`.
    let mut want_terms: BTreeMap<(usize, String), J> = BTreeMap::new();
    for e in w["terms"].as_array().unwrap() {
        let gi = usize::try_from(e["p"].as_u64().unwrap()).unwrap();
        want_terms.insert((gi, s(&e["taxonomy"]).to_owned()), plain_list(&e["terms"]));
    }
    let mut got_terms: BTreeMap<(usize, String), J> = BTreeMap::new();
    for (_, id) in site.tree.iter() {
        let p = m.page(id);
        let Some(&gi) = ix.to_go.get(&id) else {
            continue;
        };
        let mut per: BTreeMap<String, Vec<PageId>> = BTreeMap::new();
        for &(tx, tm) in &p.terms {
            let taxonomy = &site.taxonomies[tx];
            per.entry(taxonomy.def.plural.clone())
                .or_default()
                .push(taxonomy.terms[tm].page);
        }
        for (plural, ids) in per {
            got_terms.insert((gi, plural), ix.list(&ids));
        }
    }
    let keys: BTreeSet<_> = want_terms.keys().chain(got_terms.keys()).cloned().collect();
    for k in keys {
        eq(
            t,
            "get terms",
            &format!("{} {}", s(&ix.go[k.0]["path"]), k.1),
            got_terms.get(&k).cloned().unwrap_or(J::Null),
            want_terms.get(&k).unwrap_or(&J::Null),
        );
    }

    // GetPage.
    let result = |r: Result<Option<PageId>, RefError>| match r {
        Ok(p) => json!({"p": ix.opt(p)}),
        Err(RefError::Ambiguous(_)) => json!({"err": "ambiguous"}),
        Err(_) => json!({"err": "other"}),
    };
    // Error texts are ours; the kind of error must match.
    let norm = |v: &J| match v.get("err") {
        Some(e) if s(e).contains("ambiguous") => json!({"err": "ambiguous"}),
        Some(_) => json!({"err": "other"}),
        None => v.clone(),
    };
    for e in w["getPage"].as_array().unwrap() {
        let r = s(&e[0]);
        let got = if r.contains('|') {
            let args: Vec<&str> = r.split('|').collect();
            result(m.site_get_page(lang, &args))
        } else {
            result(m.site_get_page(lang, &[r]))
        };
        eq(t, "get page", r, got, &norm(&e[1]));
    }
    for e in w["getPageCtx"].as_array().unwrap() {
        let gi = usize::try_from(e[0].as_u64().unwrap()).unwrap();
        let Some(from) = ix.to_ours[gi] else {
            continue;
        };
        let r = s(&e[1]);
        let what = format!("{} {r}", s(&ix.go[gi]["path"]));
        eq(
            t,
            "get page (from)",
            &what,
            result(m.get_page(lang, r, Some(from))),
            &norm(&e[2]),
        );
        eq(
            t,
            "ref page (from)",
            &what,
            result(m.ref_page(lang, r, Some(from))),
            &norm(&e[3]),
        );
    }

    // Resources: bundle files by normalised name → relative permalink; bundled pages.
    let urls = m.config.sites[lang].site_urls();
    let mut seen = BTreeSet::new();
    for e in w["resources"].as_array().unwrap() {
        let gi = usize::try_from(e["p"].as_u64().unwrap()).unwrap();
        let Some(id) = ix.to_ours[gi] else {
            continue;
        };
        let want: BTreeSet<(String, String)> = e["resources"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| match r.get("page") {
                Some(p) => ("page".to_owned(), p.to_string()),
                None => (
                    s(&r["nameNorm"]).to_owned(),
                    s(&r["relPermalink"]).to_owned(),
                ),
            })
            .collect();
        let got: BTreeSet<(String, String)> = m
            .page(id)
            .resources
            .iter()
            .map(|&rid| {
                let r = &m.bundle_resources[rid];
                match r.page {
                    Some(p) => ("page".to_owned(), ix.go_ref(p).to_string()),
                    None => (
                        r.name_normalized.clone(),
                        r.link().map_or_else(String::new, |l| {
                            ssg_base::UrlPath::new(&urls.prepend_base_path(l.as_str())).escaped()
                        }),
                    ),
                }
            })
            .collect();
        let what = s(&ix.go[gi]["path"]).to_owned();
        seen.insert(id);
        let p = m.page(id);
        if got != want && got.is_subset(&want) && !p.path_info.kind.is_bundle() {
            // Go gives a single-file page (`leafy.md`) the files of a bundle in the
            // directory of the same name too (it does not check their owner).
            t.accept("resources", "single-page-takes-sibling-bundle");
            continue;
        }
        let names = |set: &BTreeSet<(String, String)>| -> BTreeSet<String> {
            set.iter().map(|(_, l)| l.clone()).collect()
        };
        if got != want && names(&got) == names(&want) {
            // Go names a file after the first page that walks it (`b/img.jpg` from
            // `leafy.md`); here after its owner (`img.jpg`).
            t.accept("resources", "resource-named-by-owner");
            continue;
        }
        eq(
            t,
            "resources",
            &what,
            json!(got.iter().collect::<Vec<_>>()),
            &json!(want.iter().collect::<Vec<_>>()),
        );
    }
    for (_, id) in site.tree.iter() {
        let p = m.page(id);
        if !seen.contains(&id) && !p.resources.is_empty() && !dev.contains(&p.path()) {
            t.check("resources", false, || {
                at(&format!("{}: resources only here", p.path()))
            });
        }
    }
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
