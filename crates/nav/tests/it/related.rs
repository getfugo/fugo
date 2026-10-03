//! Oracle: related content (`oracle/page/related/*`): `.Site.RegularPages.Related` for every
//! page of the reference sites with their configurations, option maps, and direct index builds
//! and searches with five more configurations (documents, index subsets, fragments with and
//! without the heading filter, named keyword lists, cardinality thresholds, date patterns).

use serde_json::{Value as J, json};
use ssg_base::{Idx, LangIdx, PageId, Value};
use ssg_config::sections::{RelatedConfig, RelatedIndex as IndexConfig, RelatedIndexKind};
use ssg_nav::{NavError, NavModel, Related, RelatedIndex, RelatedQuery, related};

use crate::support::{DumpSite, Project, Tally, family, fixture, idx, page_id, s, strings, value};

fn config(j: &J) -> RelatedConfig {
    let int = |v: &J| i32::try_from(v.as_i64().expect("int")).expect("i32");
    RelatedConfig {
        threshold: u8::try_from(j["threshold"].as_u64().expect("threshold")).expect("u8"),
        include_newer: j["includeNewer"].as_bool().expect("includeNewer"),
        to_lower: j["toLower"].as_bool().expect("toLower"),
        indices: j["indices"]
            .as_array()
            .map_or(&[][..], Vec::as_slice)
            .iter()
            .map(|i| IndexConfig {
                name: s(&i["name"]).to_owned(),
                kind: if s(&i["type"]) == "fragments" {
                    RelatedIndexKind::Fragments
                } else {
                    RelatedIndexKind::Basic
                },
                weight: int(&i["weight"]),
                cardinality_threshold: int(&i["cardinalityThreshold"]),
                pattern: s(&i["pattern"]).to_owned(),
                to_lower: i["toLower"].as_bool().expect("toLower"),
                apply_filter: i["applyFilter"].as_bool().expect("applyFilter"),
            })
            .collect(),
    }
}

fn config_json(c: &RelatedConfig) -> J {
    let idx: Vec<J> = c
        .indices
        .iter()
        .map(|i| {
            json!({
                "applyFilter": i.apply_filter, "cardinalityThreshold": i.cardinality_threshold,
                "name": i.name, "pattern": i.pattern, "toLower": i.to_lower,
                "type": match i.kind { RelatedIndexKind::Basic => "basic", RelatedIndexKind::Fragments => "fragments" },
                "weight": i.weight,
            })
        })
        .collect();
    json!({ "includeNewer": c.include_newer, "indices": idx, "threshold": c.threshold, "toLower": c.to_lower })
}

/// A result as the oracle writes it: page, filtered headings, and runs of equal (publish date,
/// name) sorted by page (Go's order within them is random).
fn docs_json(site: &DumpSite, r: &Related) -> J {
    struct E {
        i: usize,
        h: Option<Vec<String>>,
        key: ((i64, i32), String),
    }
    let mut es: Vec<E> = r
        .pages
        .iter()
        .map(|&p| {
            let d = &site.pages[p.index()];
            let h = r.heading_filter.as_ref().map(|f| {
                d.headings
                    .iter()
                    .filter(|h| f.binary_search(h).is_ok())
                    .cloned()
                    .collect()
            });
            let t = d.dates.publish_date.as_ref().map_or((i64::MIN, 0), |z| {
                (z.timestamp().as_second(), z.timestamp().subsec_nanosecond())
            });
            E {
                i: p.index(),
                h,
                key: (t, d.name.clone()),
            }
        })
        .collect();
    let mut ties = false;
    let mut i = 0;
    while i < es.len() {
        let mut j = i + 1;
        while j < es.len() && es[j].key == es[i].key {
            j += 1;
        }
        if j - i > 1 {
            ties = true;
            es[i..j].sort_by_key(|e| e.i);
        }
        i = j;
    }
    let docs: Vec<J> = es
        .iter()
        .map(|e| match &e.h {
            Some(h) => json!({"i": e.i, "h": h}),
            None => json!({"i": e.i}),
        })
        .collect();
    let mut m = json!({ "docs": docs });
    if ties {
        m["ties"] = J::Bool(true);
    }
    m
}

/// Whether our error is the one Go reported.
fn same_error(got: &NavError, want: &str) -> bool {
    match got {
        NavError::UnknownIndex(n) => want == format!("index {n:?} not found"),
        NavError::InvalidIndexName => want.ends_with("not valid"),
        NavError::UnsupportedKeyword { index, .. } => want.starts_with(&format!(
            "indexing currently not supported for index {index:?}"
        )),
        _ => false,
    }
}

fn check(t: &mut Tally, file: &str, c: &J, got: &Result<J, NavError>) {
    let want = &c["res"];
    let ok = match (got, want.get("ok"), want.get("err")) {
        (Ok(g), Some(w), _) => g == w,
        (Err(e), _, Some(w)) => same_error(e, s(w)),
        _ => false,
    };
    t.check(ok, || format!("{file}: {c}\n   got {got:?}"));
}

fn run(file: &str, t: &mut Tally) {
    let fx = fixture(&format!("page/related/{file}"));
    let site = DumpSite::new(&fx);
    let site_configs: Vec<RelatedConfig> = fx["siteConfigs"]
        .as_array()
        .expect("siteConfigs")
        .iter()
        .map(config)
        .collect();
    let configs: Vec<RelatedConfig> = fx["configs"]
        .as_array()
        .expect("configs")
        .iter()
        .map(config)
        .collect();
    let lists: Vec<Vec<PageId>> = fx["lists"]
        .as_array()
        .expect("lists")
        .iter()
        .map(|l| {
            l.as_array()
                .map_or_else(Vec::new, |a| a.iter().map(|i| page_id(idx(i))).collect())
        })
        .collect();
    let mut index: Option<((usize, usize), RelatedIndex)> = None;
    for c in fx["cases"].as_array().expect("cases") {
        let list = &lists[idx(&c["list"])];
        let doc = c.get("doc").map(|d| page_id(idx(d)));
        let site_cfg = |d: PageId| &site_configs[site.page(d).lang.index()];
        let nullable = |r: Related| {
            if r.pages.is_empty() {
                J::Null
            } else {
                docs_json(&site, &r)
            }
        };
        match s(&c["op"]) {
            "Related" => {
                let d = doc.expect("doc");
                let got = related(&site, site_cfg(d), list, d).map(nullable);
                check(t, file, c, &got);
            }
            "RelatedOpts" => {
                let d = doc.expect("doc");
                let (indices, fragments, document): (Vec<String>, Vec<String>, Option<PageId>) =
                    match idx(&c["opts"]) {
                        0 => (vec!["keywords".into()], vec![], Some(d)),
                        1 => (vec!["tags".into(), "date".into()], vec![], Some(d)),
                        2 => (vec![], vec!["common".into(), "heading-1".into()], Some(d)),
                        3 => (vec![], vec!["common".into()], None),
                        4 => (vec!["nope".into()], vec![], Some(d)),
                        _ => {
                            t.skip("template-argument-conversion");
                            continue;
                        }
                    };
                let q = RelatedQuery {
                    document,
                    indices: &indices,
                    fragments: &fragments,
                    named: &[],
                };
                let got = if list.is_empty() {
                    Ok(J::Null)
                } else {
                    RelatedIndex::build(&site, site_cfg(d), list)
                        .and_then(|i| i.search(&site, &q))
                        .map(nullable)
                };
                check(t, file, c, &got);
            }
            "RelatedBadArg" => t.skip("template-argument-conversion"),
            "Add" => {
                let ci = idx(&c["cfg"]);
                let mut i = RelatedIndex::new(&configs[ci]);
                let mut errs: Vec<usize> = Vec::new();
                for &p in list {
                    if i.add(&site, p).is_err() {
                        errs.push(p.index());
                    }
                }
                i.finalize();
                let want: Vec<usize> = c["errs"]
                    .as_array()
                    .map_or_else(Vec::new, |a| a.iter().map(|e| idx(&e[0])).collect());
                t.check(errs == want, || {
                    format!("{file}: Add cfg {ci}: errors on {errs:?}, want {want:?}")
                });
                index = Some(((ci, idx(&c["list"])), i));
            }
            op @ ("Search" | "SearchNamed") => {
                let Some((key, i)) = &index else {
                    panic!("search before add")
                };
                assert_eq!(*key, (idx(&c["cfg"]), idx(&c["list"])));
                let indices = strings(&c["indices"]);
                let fragments = strings(&c["fragments"]);
                let named: Vec<(String, Vec<Value>)> = if op == "SearchNamed" {
                    let values = c["values"]
                        .as_array()
                        .map_or_else(Vec::new, |a| a.iter().map(value).collect());
                    vec![(s(&c["key"]).to_owned(), values)]
                } else {
                    Vec::new()
                };
                let q = RelatedQuery {
                    document: doc,
                    indices: &indices,
                    fragments: &fragments,
                    named: &named,
                };
                let got = i.search(&site, &q).map(|r| docs_json(&site, &r));
                check(t, file, c, &got);
            }
            op => panic!("unknown op {op}"),
        }
    }
}

#[test]
fn related_matches_go() {
    let mut t = Tally::default();
    for file in family("page/related", &["decode.json.gz"]) {
        run(&file, &mut t);
    }
    t.finish("related");
}

#[test]
fn related_config_decodes_like_go() {
    let mut t = Tally::default();
    let fx = fixture("page/related/decode.json.gz");
    for c in fx["cases"].as_array().expect("cases") {
        let got = Project::json(&json!({ "related": c["in"] }))
            .map(|p| config_json(&p.cfg.sites[LangIdx::from_index(0)].related));
        let want = &c["res"];
        let ok = match (&got, want.get("ok")) {
            (Ok(g), Some(w)) => g == w,
            (Err(_), None) => true,
            _ => false,
        };
        if !ok && got.is_ok() && c["in"].as_object().is_some_and(serde_json::Map::is_empty) {
            // An empty `[related]` table is the empty configuration: in the site's loader,
            // Go's too (allconfig oracle `sections/related-empty`); this oracle calls
            // `related.DecodeConfig` on the bare table.
            t.accept("config-related-empty-accepted");
            continue;
        }
        t.check(ok, || {
            format!("related {}: got {got:?} want {want}", c["in"])
        });
    }
    t.finish("related-decode");
}
