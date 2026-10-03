//! The model against the recorded page and site method calls of the Go oracle
//! `oracle/sitebuild/site/<site>.json.gz`: relations (`.Parent`, `.Ancestors`, sections),
//! lists, translations, node dates, names, links and output formats, `.GetTerms`, `.GetPage`
//! (page-relative), `ref`/`relref` from every page, `.IsAncestor`/`.IsDescendant`/
//! `.InSection`/`.Eq`, and the site's `.Pages`, `.RegularPages`, `.Sections`, `.Home`,
//! `.AllPages`, `.Taxonomies`, `.Lastmod`, `.MainSections` and `.GetPage`.

use std::collections::BTreeMap;

use serde_json::{Value as J, json};
use ssg_base::{Idx, LangIdx, PageId, PageKind, text};
use ssg_site::{ListScope, Model, RefArgs, RefLink};
use ssg_testkit::fixture::oracle;

use crate::structure::Tally;
use crate::support::{Site, diff, time_json};

const SITES: [&str; 18] = [
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
    "site-menus",
    "site-refs",
    "synthetic",
    "testsite",
];

/// Paths of deviations (`expected_diffs.toml`) per site.
fn dev(name: &str) -> std::collections::BTreeSet<String> {
    crate::expected::assemble(name)
}

struct Ix {
    to_ours: Vec<Option<PageId>>,
    to_go: BTreeMap<PageId, usize>,
}

impl Ix {
    fn new(m: &Model, list: &[J]) -> Self {
        let mut ours: BTreeMap<(usize, String, String), PageId> = BTreeMap::new();
        for p in &m.pages {
            ours.insert((p.lang.index(), p.path(), p.kind.as_str().to_owned()), p.id);
        }
        let mut to_ours = Vec::new();
        let mut to_go = BTreeMap::new();
        for (i, g) in list.iter().enumerate() {
            let key = (
                usize::try_from(g["site"].as_u64().unwrap()).unwrap(),
                s(&g["path"]).to_owned(),
                s(&g["kind"]).to_owned(),
            );
            let id = ours.get(&key).copied();
            if let Some(id) = id {
                to_go.insert(id, i);
            }
            to_ours.push(id);
        }
        Self { to_ours, to_go }
    }

    fn page(&self, id: Option<PageId>) -> J {
        match id {
            None => J::Null,
            Some(id) => self.to_go.get(&id).map_or(json!("?"), |i| json!({"p": i})),
        }
    }

    fn pages(&self, ids: &[PageId]) -> J {
        if ids.is_empty() {
            return J::Null;
        }
        J::Array(ids.iter().map(|&id| self.page(Some(id))).collect())
    }

    /// The page an oracle argument names (`None` for nil pages and other values).
    fn arg(&self, v: &J) -> Option<PageId> {
        let i = usize::try_from(v.get("p")?.as_u64()?).ok()?;
        self.to_ours.get(i).copied().flatten()
    }
}

fn s(v: &J) -> &str {
    v.as_str().unwrap_or_default()
}

/// A recorded result with the type tags dropped (`{"p": 3, "t": …}` → `{"p": 3}`; a nil page
/// is `null`).
fn clean(v: &J) -> J {
    match v {
        J::Object(o) if o.get("nil") == Some(&J::Bool(true)) => J::Null,
        J::Object(o) if o.contains_key("p") && o.contains_key("t") => json!({"p": o["p"]}),
        J::Object(o) => J::Object(o.iter().map(|(k, v)| (k.clone(), clean(v))).collect()),
        J::Array(a) => J::Array(a.iter().map(clean).collect()),
        other => other.clone(),
    }
}

/// The dates Go writes for a date (`0001-01-01` for none).
fn date(d: Option<&jiff::Zoned>) -> J {
    time_json(d)
}

#[allow(clippy::too_many_lines)]
fn page_call(m: &Model, ix: &Ix, id: PageId, method: &str, args: &J) -> Option<J> {
    let p = m.page(id);
    let lang = p.lang;
    let d = &p.meta.dates;
    let links = p.links();
    let refs = |ids: &[PageId]| ix.pages(ids);
    Some(match method {
        "AllTranslations" => refs(&p.translations),
        "Translations" => {
            let t: Vec<PageId> = p
                .translations
                .iter()
                .copied()
                .filter(|&t| t != id)
                .collect();
            refs(&t)
        }
        "IsTranslated" => json!(p.translations.iter().any(|&t| t != id)),
        "TranslationKey" => json!(
            p.meta
                .translation_key
                .clone()
                .filter(|k| !k.is_empty())
                .unwrap_or_else(|| p.path())
        ),
        "Ancestors" => refs(&p.ancestors),
        "Parent" => ix.page(p.parent),
        "CurrentSection" => ix.page(Some(p.current_section)),
        "FirstSection" => ix.page(Some(p.first_section)),
        "Pages" => refs(&p.pages),
        "RegularPages" => refs(&p.regular_pages),
        "Sections" => refs(&p.sections),
        "SectionsPath" => json!(m.page(p.current_section).path()),
        "Permalink" => json!(links.map(|l| l.permalink.to_string()).unwrap_or_default()),
        "RelPermalink" => json!(links.map(|l| l.rel_permalink.escaped()).unwrap_or_default()),
        "Date" => date(d.date.as_ref()),
        "Lastmod" => date(d.lastmod.as_ref()),
        "PublishDate" => date(d.publish_date.as_ref()),
        "ExpiryDate" => date(d.expiry_date.as_ref()),
        "Title" => json!(p.title),
        "LinkTitle" => json!(p.link_title),
        "Name" => json!(m.page_name(id)),
        "Kind" => json!(p.kind.as_str()),
        "Path" => json!(p.path()),
        "Section" => json!(p.section),
        "Type" => json!(p.r#type),
        "IsHome" => json!(p.kind == PageKind::Home),
        "IsNode" => json!(p.kind != PageKind::Page),
        "IsPage" => json!(p.kind == PageKind::Page),
        "IsSection" => json!(p.kind == PageKind::Section),
        "OutputFormats" => {
            let v: Vec<J> = p
                .urls
                .iter()
                .filter_map(|u| {
                    let l = u.links.as_ref()?;
                    Some(json!({
                        "name": m.config.output_formats.get(u.format).name,
                        "relPermalink": l.rel_permalink.escaped(),
                        "permalink": l.permalink.to_string(),
                    }))
                })
                .collect();
            if v.is_empty() { J::Null } else { J::Array(v) }
        }
        "Eq" | "IsAncestor" | "IsDescendant" | "InSection" => {
            let Some(other) = ix.arg(&args[0]) else {
                return Some(json!(false));
            };
            let o = m.page(other);
            json!(match method {
                "Eq" => other == id,
                "IsAncestor" => m.is_ancestor(id, other),
                "IsDescendant" => m.is_ancestor(other, id),
                _ => p.current_section == o.current_section,
            })
        }
        "GetPage" => match m.get_page(lang, s(&args[0]), Some(id)) {
            Ok(r) => ix.page(r),
            Err(_) => json!({"err": true}),
        },
        "GetTerms" => {
            let plural = s(&args[0]);
            let ids: Vec<PageId> = p
                .terms
                .iter()
                .filter(|(tx, _)| m.sites[lang].taxonomies[*tx].def.plural == plural)
                .map(|(tx, tm)| m.sites[lang].taxonomies[*tx].terms[*tm].page)
                .collect();
            refs(&ids)
        }
        "Ref" | "RelRef" | "RefFrom" | "RelRefFrom" => {
            let a = args[0].as_object()?;
            let get = |k: &str| {
                a.iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case(k))
                    .and_then(|(_, v)| v.as_str())
                    .map(str::to_owned)
            };
            let from = if method.ends_with("From") {
                Some(ix.arg(&args[1])?)
            } else {
                Some(id)
            };
            let not_found = &m.config.sites[lang].ref_links.not_found_url;
            // Arguments are decoded weakly: a number is a path, a list is no path.
            let path = match a
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case("path"))
                .map(|(_, v)| v)
            {
                None => String::new(),
                Some(J::String(p)) => p.clone(),
                Some(J::Number(n)) => n.to_string(),
                Some(_) => return Some(json!(not_found)),
            };
            let ra = RefArgs {
                path,
                lang: get("lang"),
                output_format: get("outputFormat"),
            };
            let kind = if method.starts_with("Rel") {
                RefLink::RelPermalink
            } else {
                RefLink::Permalink
            };
            json!(
                m.ref_link(lang, &ra, from, kind)
                    .unwrap_or_else(|_| not_found.clone())
            )
        }
        _ => return None,
    })
}

fn site_call(m: &Model, ix: &Ix, lang: LangIdx, method: &str, args: &J) -> Option<J> {
    let site = &m.sites[lang];
    Some(match method {
        "Pages" => ix.pages(&site.pages),
        "RegularPages" => ix.pages(&site.regular_pages),
        "Sections" => ix.pages(&m.page(site.home).sections),
        "Home" => ix.page(Some(site.home)),
        "Lastmod" | "LastChange" => date(site.last_mod.as_ref()),
        "MainSections" => json!(site.main_sections),
        "AllPages" => {
            let mut all: Vec<PageId> = m.sites.iter().flat_map(|s| s.pages.clone()).collect();
            let c = ssg_locale::Collator::for_language(&m.config.sites[lang].language.key);
            let key = |id: PageId| {
                let p = m.page(id);
                ssg_page::SortKey {
                    weight: p.meta.weight,
                    date: p.meta.dates.date.as_ref(),
                    link_title: &p.link_title,
                    path: &p.path_info.path,
                    ordinal: None,
                    weight0: None,
                }
            };
            all.sort_by(|a, b| ssg_page::default_order(&key(*a), &key(*b), &c));
            ix.pages(&all)
        }
        "Taxonomies" => {
            let mut out = serde_json::Map::new();
            for t in &site.taxonomies {
                let mut terms = serde_json::Map::new();
                for (_, term) in t.listed_terms(m) {
                    terms.insert(
                        text::to_lower(&term.term),
                        J::Array(
                            term.members
                                .iter()
                                .map(|w| json!({"p": ix.page(Some(w.page)), "w": w.weight}))
                                .collect(),
                        ),
                    );
                }
                out.insert(t.def.plural.clone(), J::Object(terms));
            }
            J::Object(out)
        }
        "GetPage" => {
            let r = s(args);
            let res = if r.contains('|') {
                let a: Vec<&str> = r.split('|').collect();
                m.site_get_page(lang, &a)
            } else {
                m.site_get_page(lang, &[r])
            };
            match res {
                Ok(p) => ix.page(p),
                Err(_) => json!({"err": true}),
            }
        }
        _ => return None,
    })
}

/// The recorded result as compared: `ok` values (an empty list is none); errors only as "an
/// error".
fn want(v: &J) -> J {
    match v.get("ok") {
        Some(J::Array(a)) if a.is_empty() => J::Null,
        Some(ok) => clean(ok),
        None => json!({"err": true}),
    }
}

fn check(name: &str, t: &mut Tally) {
    let f: J = oracle(&format!("oracle/sitebuild/site/{name}.json.gz"));
    let site = Site::new(&f["site"]);
    let m = site.model().unwrap_or_else(|e| panic!("{name}: {e}"));
    let list = f["dump"]["pageList"].as_array().unwrap();
    let ix = Ix::new(&m, list);
    let dev = dev(name);
    for (i, g) in list.iter().enumerate() {
        if ix.to_ours[i].is_none() && !dev.contains(s(&g["path"])) {
            t.check("page list", false, || format!("{name}: Go only {g}"));
        }
    }
    for pg in f["dump"]["pages"].as_array().unwrap() {
        let gi = usize::try_from(pg["p"].as_u64().unwrap()).unwrap();
        let Some(id) = ix.to_ours[gi] else {
            continue;
        };
        let path = s(&list[gi]["path"]);
        if dev.contains(path) {
            continue;
        }
        for c in pg["calls"].as_array().unwrap() {
            let method = s(&c[0]);
            let args = &c[1];
            let Some(got) = page_call(&m, &ix, id, method, args) else {
                continue;
            };
            let mut w = want(&c[2]);
            if method == "OutputFormats"
                && let J::Array(a) = &mut w
            {
                for e in a {
                    if let J::Object(o) = e {
                        o.retain(|k, _| {
                            matches!(k.as_str(), "name" | "relPermalink" | "permalink")
                        });
                    }
                }
            }
            let check = check_name(method);
            let d = diff(method, &got, &w);
            if d.is_some()
                && check == "ref/relref"
                && w == json!("")
                && m.page(id).role != ssg_site::PageRole::Standalone
            {
                // Go's bundled pages have no working site: their refs are all empty.
                t.accept(check, "ref-from-bundled-page");
                continue;
            }
            if d.is_some()
                && (mentions_dev(&w, list, &dev)
                    || mentions_dev(args, list, &dev)
                    || got.to_string().contains(r#""?""#))
            {
                t.accept(check, "segment-wise-taxonomy-prefix");
                continue;
            }
            t.check(check, d.is_none(), || {
                format!(
                    "{name} {path} ({}) {method}{args}: {}",
                    s(&list[gi]["lang"]),
                    d.unwrap_or_default()
                )
            });
        }
    }
    for (i, sd) in f["dump"]["sites"].as_array().unwrap().iter().enumerate() {
        let lang = LangIdx::from_index(i);
        for c in sd["siteWrapper"].as_array().unwrap() {
            let method = s(&c[0]);
            let (args, result) = if c.as_array().unwrap().len() == 3 {
                (&c[1], &c[2])
            } else {
                (&J::Null, &c[1])
            };
            let Some(got) = site_call(&m, &ix, lang, method, args) else {
                continue;
            };
            let w = want(result);
            let check = if method == "GetPage" {
                "site GetPage"
            } else {
                "site lists"
            };
            let d = diff(method, &got, &w);
            if d.is_some() && (mentions_dev(&w, list, &dev) || got.to_string().contains(r#""?""#)) {
                t.accept(check, "segment-wise-taxonomy-prefix");
                continue;
            }
            t.check(check, d.is_none(), || {
                format!(
                    "{name} [{i}] .Site.{method} {args}: {}",
                    d.unwrap_or_default()
                )
            });
        }
    }
    // Listed pages exist in the page list.
    for p in &m.pages {
        if p.listed(ListScope::Global) && !ix.to_go.contains_key(&p.id) && !dev.contains(&p.path())
        {
            t.check("page list", false, || {
                format!("{name}: Rust only {} {}", p.path(), p.lang)
            });
        }
    }
}

/// Whether a recorded value names a page of a deviation.
fn mentions_dev(v: &J, list: &[J], dev: &std::collections::BTreeSet<String>) -> bool {
    match v {
        J::Object(o) => {
            if let Some(i) = o.get("p").and_then(J::as_u64) {
                return dev.contains(s(&list[usize::try_from(i).unwrap()]["path"]));
            }
            o.values().any(|v| mentions_dev(v, list, dev))
        }
        J::Array(a) => a.iter().any(|v| mentions_dev(v, list, dev)),
        _ => false,
    }
}

fn check_name(method: &str) -> &'static str {
    match method {
        "AllTranslations" | "Translations" | "IsTranslated" | "TranslationKey" => "translations",
        "Ancestors" | "Parent" | "CurrentSection" | "FirstSection" | "SectionsPath" => "relations",
        "Pages" | "RegularPages" | "Sections" => "lists",
        "Permalink" | "RelPermalink" | "OutputFormats" => "links",
        "Date" | "Lastmod" | "PublishDate" | "ExpiryDate" => "dates",
        "Eq" | "IsAncestor" | "IsDescendant" | "InSection" => "comparisons",
        "GetPage" => "GetPage",
        "GetTerms" => "GetTerms",
        "Ref" | "RelRef" | "RefFrom" | "RelRefFrom" => "ref/relref",
        _ => "names and kinds",
    }
}

#[test]
fn page_and_site_calls_match_go() {
    let mut t = Tally::default();
    for name in SITES {
        check(name, &mut t);
    }
    t.finish("calls");
}
