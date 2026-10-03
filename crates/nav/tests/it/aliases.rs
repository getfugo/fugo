//! Oracle: the alias plan against the alias files of Go's builds
//! (`oracle/sitebuild/build/*`: every redirect Go wrote, with its page, language and format).
//!
//! The site is loaded with `ssg-site` (T23a: pages, front matter `aliases`, build options,
//! drafts); the pages' output formats and links, which T23b adds to the model, come from the
//! recorded renders of the same build. `page/1` redirects depend on what templates paginate
//! and are checked in `pagination.rs`.

use std::collections::BTreeMap;

use serde_json::Value as J;
use ssg_base::paths::ContentKey;
use ssg_base::{Clock, FormatId, Idx, LangIdx, OutputPath, PageId, PageKind, UrlPath};
use ssg_config::Config;
use ssg_nav::{AliasKind, AliasPlan, NavModel, PageFacts, Rendering, alias_plan, alias_target};
use ssg_page::{Dates, ListMode, RenderMode, ResourceBase, TargetPaths};
use ssg_site::{LoadModelOptions, Model, PageRole, load_model};

use crate::support::{Project, Tally, family, fixture, s};

/// The model plus, per page, the output formats and links Go rendered.
struct BuildSite {
    model: Model,
    outputs: Vec<Vec<(FormatId, TargetPaths)>>,
    empty: Dates,
    no_str: Vec<String>,
}

/// The link of a permalink relative to the site root, and the directory relative aliases are
/// resolved from (the link without `/` or file extension).
fn target_paths(permalink: &str, base: &str) -> TargetPaths {
    let link = format!(
        "/{}",
        permalink
            .strip_prefix(base)
            .unwrap_or(permalink)
            .trim_start_matches('/')
    );
    let dir = if let Some(d) = link.strip_suffix('/') {
        d.to_owned()
    } else {
        ssg_base::paths::trim_ext(&link).to_owned()
    };
    TargetPaths {
        target: OutputPath::new(""),
        link: UrlPath::new(&link),
        resources: Some(ResourceBase {
            target: OutputPath::new(&dir),
            link: UrlPath::new(&dir),
        }),
    }
}

/// A format name and the page's permalink in it (when an alias recorded it).
type FormatLink = (String, Option<String>);

/// The planned rows and Go's.
type PlanRows = (Vec<Row>, Vec<Row>);

impl BuildSite {
    fn new(fx: &J, cfg: &Config, model: Model) -> Self {
        // (lang, path) → format → permalink, in render order.
        let mut links: BTreeMap<(String, String), Vec<FormatLink>> = BTreeMap::new();
        for r in fx["renders"].as_array().expect("renders") {
            let key = (s(&r["lang"]).to_owned(), s(&r["path"]).to_owned());
            let format = s(&r["format"]).to_owned();
            let list = links.entry(key).or_default();
            let permalink = r["permalink"].as_str().map(str::to_owned);
            match list.iter_mut().find(|(f, _)| *f == format) {
                Some(slot) => {
                    if slot.1.is_none() {
                        slot.1 = permalink;
                    }
                }
                None if s(&r["kind"]) == "page" || permalink.is_some() => {
                    list.push((format, permalink));
                }
                None => {}
            }
        }
        let outputs = model
            .pages
            .iter()
            .map(|p| {
                let site = &cfg.sites[p.lang];
                let key = (site.language.key.clone(), p.key.to_path());
                links.get(&key).map_or_else(Vec::new, |list| {
                    list.iter()
                        .filter_map(|(f, permalink)| {
                            let id = cfg.output_formats.by_name(f)?;
                            let tp = target_paths(permalink.as_deref()?, site.base_url.as_str());
                            Some((id, tp))
                        })
                        .collect()
                })
            })
            .collect();
        Self {
            model,
            outputs,
            empty: Dates::default(),
            no_str: Vec::new(),
        }
    }
}

impl NavModel for BuildSite {
    fn page(&self, id: PageId) -> PageFacts<'_> {
        let p = self.model.page(id);
        let rendering =
            if p.role == PageRole::Standalone && p.meta.build.render == RenderMode::Always {
                Rendering::Rendered
            } else {
                Rendering::NotRendered
            };
        PageFacts {
            id,
            lang: p.lang,
            kind: p.kind,
            lang_key: "",
            section: p.key.first_segment(),
            title: "",
            link_title: "",
            name: "",
            slug: "",
            description: "",
            page_type: "",
            layout: "",
            bundle_type: "",
            draft: p.meta.draft,
            weight: p.meta.weight,
            keywords: &self.no_str,
            aliases: &p.meta.aliases,
            dates: &self.empty,
            params: &p.meta.params,
            menus: &[],
            rel_permalink: "",
            fragments: &self.no_str,
            outputs: &self.outputs[id.index()],
            rendering,
            list: ListMode::Always,
        }
    }

    fn tree_pages(&self, lang: LangIdx) -> Vec<PageId> {
        self.model.sites[lang]
            .tree
            .iter()
            .map(|(_, id)| id)
            .collect()
    }

    fn resolve_page_ref(&self, _: LangIdx, _: &str) -> Option<PageId> {
        None
    }

    fn is_ancestor(&self, _: PageId, _: PageId) -> bool {
        false
    }

    fn pages(&self, _: PageId) -> &[PageId] {
        &[]
    }

    fn regular_pages(&self, _: PageId) -> &[PageId] {
        &[]
    }

    fn site_regular_pages(&self, _: LangIdx) -> &[PageId] {
        &[]
    }

    fn home(&self, lang: LangIdx) -> Option<PageId> {
        // Auto nodes (a home page without `_index.md`) are T23b's: stand in with any page
        // (the redirect rows do not name their target page).
        self.model.sites[lang]
            .tree
            .get(&ContentKey::home())
            .or_else(|| {
                self.model
                    .pages
                    .iter()
                    .find(|p| p.kind == PageKind::Home)
                    .map(|p| p.id)
            })
            .or_else(|| self.model.pages.ids().next())
    }
}

/// An alias file as compared: (file, language, page path, format, kind).
pub type Row = (String, String, String, String, &'static str);

fn expected(fx: &J, cfg: &Config) -> Vec<Row> {
    let pager_dirs: Vec<String> = cfg
        .sites
        .iter()
        .map(|s| format!("/{}/1", s.pagination.path))
        .collect();
    let mut out = Vec::new();
    for r in fx["renders"].as_array().expect("renders") {
        if s(&r["kind"]) != "alias" {
            continue;
        }
        let (lang, path, format, alias) = (
            s(&r["lang"]),
            s(&r["path"]),
            s(&r["format"]),
            s(&r["alias"]),
        );
        // `page/1` redirects (`/posts/page/1/index.html`, `/404/page/1.html`).
        let pager = pager_dirs
            .iter()
            .any(|d| alias.contains(&format!("{d}/")) || alias.ends_with(&format!("{d}.html")));
        if pager {
            continue;
        }
        let redirect = lang.is_empty() && path.is_empty();
        let file =
            alias_target(alias, redirect).map_or_else(|e| format!("error: {e}"), |p| p.to_string());
        let kind = if redirect { "redirect" } else { "front matter" };
        out.push((
            file,
            lang.to_owned(),
            path.to_owned(),
            format.to_owned(),
            kind,
        ));
    }
    out.sort();
    out
}

fn rows(plan: &[AliasPlan], site: &BuildSite, cfg: &Config) -> Vec<Row> {
    let mut out: Vec<Row> = plan
        .iter()
        .map(|a| {
            let p = site.model.page(a.to);
            let (lang, path, kind) = match a.kind {
                AliasKind::FrontMatter => (
                    cfg.sites[p.lang].language.key.clone(),
                    p.key.to_path(),
                    "front matter",
                ),
                AliasKind::LanguageRedirect => (String::new(), String::new(), "redirect"),
            };
            let format = cfg.output_formats.get(a.format).name.clone();
            (a.from.to_string(), lang, path, format, kind)
        })
        .collect();
    out.sort();
    out
}

/// The alias rows of the plan of a Go build fixture and the ones Go wrote; `None` when the
/// build wrote no alias or its site does not load.
fn plan_rows(fx: &J) -> Option<Result<PlanRows, String>> {
    let has = fx["renders"]
        .as_array()
        .is_some_and(|r| r.iter().any(|r| r["kind"] == "alias"));
    if !has {
        return None;
    }
    let project = Project::recorded(&fx["site"]);
    let cfg = project.cfg.clone();
    let want = expected(fx, &cfg);
    let vfs = ssg_vfs::Vfs::new(&cfg).expect("vfs");
    let clock = Clock("2026-09-01T00:00:00Z".parse().expect("clock"));
    let model = load_model(
        cfg.clone(),
        &vfs,
        &LoadModelOptions::from_config(&cfg, clock),
    )
    .ok()?;
    let site = BuildSite::new(fx, &cfg, model);
    Some(
        alias_plan(&site, &cfg)
            .map(|plan| (rows(&plan, &site, &cfg), want))
            .map_err(|e| e.to_string()),
    )
}

/// The planned and the Go alias rows of build fixture `file`.
pub fn build_rows(file: &str) -> (Vec<Row>, Vec<Row>) {
    let fx = fixture(&format!("sitebuild/build/{file}"));
    plan_rows(&fx).expect("aliases").expect("plan")
}

#[test]
fn alias_plan_matches_go_builds() {
    let mut t = Tally::default();
    let mut files = 0;
    for file in family("sitebuild/build", &[]) {
        let fx = fixture(&format!("sitebuild/build/{file}"));
        let (got, want) = match plan_rows(&fx) {
            None => continue,
            Some(Err(e)) => {
                t.fail(|| format!("{file}: {e}"));
                continue;
            }
            Some(Ok(rows)) => rows,
        };
        files += 1;
        // Row by row, as multisets.
        let mut rest = got.clone();
        for w in &want {
            match rest.iter().position(|g| g == w) {
                Some(i) => {
                    rest.remove(i);
                    t.pass();
                }
                None => t.fail(|| format!("{file}: missing {w:?}")),
            }
        }
        for g in rest {
            t.fail(|| format!("{file}: extra {g:?}"));
        }
    }
    eprintln!("aliases: {files} builds with alias files");
    assert!(files >= 5, "{files} builds with aliases");
    t.finish("aliases");
}

#[test]
fn alias_targets() {
    let ok = |a: &str, root: bool| alias_target(a, root).map(|p| p.to_string()).ok();
    assert_eq!(ok("/old/a/", false).as_deref(), Some("/old/a/index.html"));
    assert_eq!(ok("/home.html", false).as_deref(), Some("/home.html"));
    assert_eq!(
        ok("/dir/./x/../y/", false).as_deref(),
        Some("/dir/y/index.html")
    );
    assert_eq!(ok("en", true).as_deref(), Some("/en/index.html"));
    assert_eq!(ok("/", true).as_deref(), Some("/index.html"));
    assert_eq!(ok("/", false), None);
    assert_eq!(ok("", false), None);
    assert_eq!(ok("../up", false), None);
}
