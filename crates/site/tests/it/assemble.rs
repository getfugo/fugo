//! Assembly against the Go oracle `oracle/sitebuild/assemble/<site>.json.gz` (every page after
//! `setMetaPost` and the removal of drafts, future and expired content). Compared for every page
//! with a content file: the page set per language with kinds and bundle roles, the params after
//! the cascade, build options, draft, the four dates (branch pages without dates of their own
//! after node-date aggregation), the typed front matter fields, and the cascade in force.
//!
//! Pages without a file (missing home, root sections, taxonomies, standalone pages) are checked
//! by `structure`.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value as J, json};
use ssg_base::Idx;
use ssg_base::diag::Severity;
use ssg_page::{ListMode, Markup, PageMeta, RenderMode};
use ssg_site::{Model, Page, PageRole};
use ssg_testkit::fixture::oracle;

use crate::expected;
use crate::support::{Site, diff, time_json, to_json};

fn build_json(m: &PageMeta) -> J {
    let b = m.build;
    json!({
        "list": match b.list { ListMode::Always => "always", ListMode::Never => "never", ListMode::Local => "local" },
        "render": match b.render { RenderMode::Always => "always", RenderMode::Never => "never", RenderMode::Link => "link" },
        "publishResources": b.publish_resources,
    })
}

fn dates_json(m: &PageMeta) -> J {
    let d = &m.dates;
    json!({
        "date": time_json(d.date.as_ref()),
        "lastmod": time_json(d.lastmod.as_ref()),
        "publishDate": time_json(d.publish_date.as_ref()),
        "expiryDate": time_json(d.expiry_date.as_ref()),
    })
}

fn cascade_json(m: &Model, p: &Page) -> J {
    let c = m.sites[p.lang].cascade.in_force(&p.key, &p.meta.cascade);
    J::Array(
        c.rules()
            .iter()
            .map(|r| {
                let [kind, path, lang, environment] = r.target.sources();
                json!({
                    "fields": to_json(&ssg_base::Value::map(r.fields.as_map().clone())),
                    "params": to_json(&ssg_base::Value::map(r.params.as_map().clone())),
                    "target": {"environment": environment, "kind": kind, "lang": lang, "path": path},
                })
            })
            .collect(),
    )
}

/// The fields compared for one page: (name, got, want).
fn fields(m: &Model, p: &Page, w: &J) -> Vec<(&'static str, J, J)> {
    let meta = &p.meta;
    let s = |v: &Option<String>| json!(v.as_deref().unwrap_or_default());
    let mut out = vec![
        (
            "params",
            to_json(&ssg_base::Value::map(meta.params.as_map().clone())),
            w["params"].clone(),
        ),
        ("build", build_json(meta), w["build"].clone()),
        ("draft", json!(meta.draft), w["draft"].clone()),
        ("weight", json!(meta.weight), w["weight"].clone()),
        (
            "description",
            json!(meta.description),
            w["description"].clone(),
        ),
        ("layout", s(&meta.layout), w["layout"].clone()),
        ("slug", s(&meta.slug), w["slug"].clone()),
        ("url", s(&meta.url), w["url"].clone()),
        ("keywords", json!(meta.keywords), w["keywords"].clone()),
        ("aliases", json!(meta.aliases), w["aliases"].clone()),
        (
            "translationKey",
            s(&meta.translation_key),
            w["translationKey"].clone(),
        ),
        (
            "mediaType",
            json!(match meta.markup {
                Markup::Markdown => "text/markdown",
                Markup::Html => "text/html",
            }),
            w["mediaType"].clone(),
        ),
        (
            "sitemap",
            json!({
                "changeFreq": meta.sitemap.change_freq,
                "disable": meta.sitemap.disable,
                "filename": meta.sitemap.filename,
                "priority": meta.sitemap.priority,
            }),
            w["sitemap"].clone(),
        ),
        ("cascade", cascade_json(m, p), w["cascade"].clone()),
    ];
    if meta.title.is_some() {
        out.push(("title", s(&meta.title), w["title"].clone()));
    }
    if !p.kind.is_branch() || !meta.dates.is_empty() {
        out.push(("dates", dates_json(meta), w["dates"].clone()));
    }
    out
}

/// (language, kind, path, file, bundled).
type Rec = (usize, String, String, String, bool);

/// Checks one site; returns the number of pages compared.
fn check(name: &str) -> usize {
    let f: J = oracle(&format!("oracle/sitebuild/assemble/{name}.json.gz"));
    let site = Site::new(&f["site"]);
    let m = site.model().unwrap_or_else(|e| panic!("{name}: {e}"));
    let dev = expected::assemble(name);

    let want: BTreeMap<Rec, &J> = f["dump"]["pages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| !p["file"].as_str().unwrap().is_empty())
        .filter(|p| !dev.contains(p["path"].as_str().unwrap()))
        .map(|p| {
            let rec = (
                usize::try_from(p["site"].as_u64().unwrap()).unwrap(),
                p["kind"].as_str().unwrap().to_owned(),
                p["path"].as_str().unwrap().to_owned(),
                p["file"].as_str().unwrap().to_owned(),
                p["bundled"].as_bool().unwrap(),
            );
            (rec, p)
        })
        .collect();
    let got: BTreeMap<Rec, &Page> = m
        .pages
        .iter()
        .filter(|p| p.source.is_some() && !dev.contains(&p.key.to_path()))
        .map(|p| {
            let file = site.norm(&p.source.as_ref().unwrap().file.abs);
            let rec = (
                p.lang.index(),
                p.kind.as_str().to_owned(),
                p.key.to_path(),
                file,
                matches!(p.role, PageRole::Bundled { .. }),
            );
            (rec, p)
        })
        .collect();
    let wk: BTreeSet<_> = want.keys().collect();
    let gk: BTreeSet<_> = got.keys().collect();
    let missing: Vec<_> = wk.difference(&gk).collect();
    let extra: Vec<_> = gk.difference(&wk).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "{name}: pages: Go only {missing:#?}\nRust only {extra:#?}"
    );

    let mut compared = 0;
    for (rec, p) in &got {
        let w = want[rec];
        for (field, g, wv) in fields(&m, p, w) {
            if let Some(d) = diff(field, &g, &wv) {
                panic!("{name}: {} ({}): {d}", rec.2, rec.3);
            }
            compared += 1;
        }
    }

    let log = f["log"].as_array().unwrap();
    let count = |s: &str| {
        log.iter()
            .filter(|l| l.as_str().unwrap().contains(s))
            .count()
    };
    let ours = |id: &str| {
        m.diagnostics
            .iter()
            .filter(|d| d.id.as_deref() == Some(id))
            .count()
    };
    assert_eq!(
        ours("duplicate-content-path"),
        count("Duplicate content path"),
        "{name}: duplicate warnings"
    );
    assert_eq!(
        ours("front-matter-date"),
        count("is not a parsable date"),
        "{name}: date errors"
    );
    assert!(
        m.diagnostics
            .iter()
            .filter(|d| d.id.as_deref() == Some("front-matter-date"))
            .all(|d| d.severity == Severity::Error)
    );
    assert_eq!(
        ours("warning-home-page-is-leaf-bundle"),
        count("Using index.md in your content's root"),
        "{name}: home leaf bundle warnings"
    );
    eprintln!(
        "assemble {name}: {} pages with a file equal ({compared} fields)",
        got.len()
    );
    got.len()
}

#[test]
fn assemble_testsite() {
    assert_eq!(check("testsite"), 2);
}

#[test]
fn assemble_docs() {
    assert!(check("docs") >= 900);
}

#[test]
fn assemble_other_oracle_sites() {
    for name in [
        "asm-build",
        "asm-cascade",
        "asm-flags",
        "asm-i18n",
        "asm-multihost",
        "asm-taxo",
        "asm-ugly",
        "content",
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
