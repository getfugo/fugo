//! Oracle: pagination (`oracle/page/pagination/*`): every paginator Go's builds of the
//! reference sites made (the list, its size, every pager's pages and URL), the same lists
//! with other pager sizes and as page groups, the error cases, and the pager-size option.

use std::sync::Arc;

use serde_json::{Value as J, json};
use ssg_base::url::SiteUrls;
use ssg_base::{PageId, PageKind, Value};
use ssg_config::OutputFormat;
use ssg_config::sections::PaginationConfig;
use ssg_nav::{
    PageGroup, PagerSlice, Pagination, PaginationItems, pager_alias, pager_paths,
    resolve_pager_size,
};
use ssg_page::{LangPrefix, UrlInputs, links};

use crate::support::{
    OraclePath, Tally, family, fixture, idx, oracle_paths, output_format, page_id, s, site_urls,
    value,
};

struct Ctx {
    urls: Vec<(SiteUrls, PaginationConfig)>,
    formats: Vec<(OutputFormat, String)>,
    paths: Vec<OraclePath>,
}

fn pagination_config(ps: &J) -> PaginationConfig {
    let p = &ps["pagination"];
    PaginationConfig {
        pager_size: usize::try_from(p["pagerSize"].as_u64().expect("pagerSize")).expect("size"),
        path: s(&p["path"]).to_owned(),
        disable_aliases: p["disableAliases"].as_bool().expect("disableAliases"),
    }
}

fn ids(v: &J) -> Arc<[PageId]> {
    v.as_array()
        .map_or_else(Vec::new, |a| a.iter().map(|i| page_id(idx(i))).collect())
        .into()
}

/// The paginated items of a case; `None` for inputs only a template argument conversion can
/// reject (a string, a mixed list).
fn items(seq: &J) -> Option<PaginationItems> {
    match seq {
        J::Null => Some(PaginationItems::Pages(Arc::from([]))),
        J::Object(o) => match o.get("t").and_then(J::as_str) {
            Some("pages") => Some(PaginationItems::Pages(ids(&seq["pages"]))),
            Some("any") if seq["items"].is_null() => Some(PaginationItems::Pages(Arc::from([]))),
            Some("groups") => Some(PaginationItems::Groups(
                seq["groups"]
                    .as_array()
                    .expect("groups")
                    .iter()
                    .map(|g| PageGroup {
                        key: value(&g["key"]),
                        pages: ids(&g["pages"]),
                    })
                    .collect(),
            )),
            _ => None,
        },
        _ => None,
    }
}

/// A pagination as the oracle writes it.
fn render(p: &Pagination, url: &dyn Fn(usize) -> String) -> J {
    let count = p.pager_count();
    let pagers: Vec<J> = p
        .pagers()
        .iter()
        .map(|pg| {
            let n = pg.number;
            let (pages, groups) = match (&pg.slice, p.items()) {
                (PagerSlice::Pages(_), _) => {
                    let pages: Vec<usize> = p
                        .pages_of(pg)
                        .iter()
                        .map(|id| ssg_base::Idx::index(*id))
                        .collect();
                    (
                        if pages.is_empty() {
                            J::Null
                        } else {
                            json!(pages)
                        },
                        J::Null,
                    )
                }
                (PagerSlice::Groups(slices), PaginationItems::Groups(groups)) => {
                    let gs: Vec<J> = slices
                        .iter()
                        .map(|sl| {
                            let g = &groups[sl.group];
                            let pages: Vec<usize> = g.pages[sl.range.clone()]
                                .iter()
                                .map(|id| ssg_base::Idx::index(*id))
                                .collect();
                            json!({ "key": crate::support::to_json(&g.key), "pages": pages })
                        })
                        .collect();
                    (J::Null, if gs.is_empty() { J::Null } else { json!(gs) })
                }
                _ => unreachable!("group slices of a page list"),
            };
            let num = |k: usize| i64::try_from(k).expect("number");
            json!({
                "first": 1,
                "groups": groups,
                "hasNext": n < count,
                "hasPrev": n > 1,
                "last": count,
                "next": if n < count { num(n + 1) } else { -1 },
                "number": n,
                "numberOfElements": pg.len(),
                "pages": pages,
                "prev": if n > 1 { num(n - 1) } else { -1 },
                "string": format!("Pager {n}"),
                "url": url(n),
            })
        })
        .collect();
    json!({ "ok": {
        "pagerSize": p.size(),
        "pagers": pagers,
        "totalNumberOfElements": p.total_items(),
        "totalPages": p.total_pages(),
    }})
}

fn inputs<'a>(c: &'a J, cx: &'a Ctx) -> UrlInputs<'a> {
    let td = &c["td"];
    let (format, suffix) = &cx.formats[idx(&td["type"])];
    let (urls, _) = &cx.urls[idx(&td["ps"])];
    let path = &cx.paths[idx(&td["path"])];
    let section = td["section"]
        .as_i64()
        .and_then(|i| usize::try_from(i).ok())
        .map_or("/", |i| cx.paths[i].base.as_str());
    let opt = |k: &str| Some(s(&td[k])).filter(|v| !v.is_empty());
    UrlInputs {
        kind: PageKind::parse(s(&td["kind"])).expect("kind"),
        format,
        suffix,
        source: &path.source,
        section,
        base_name: s(&td["baseName"]),
        prefix: LangPrefix {
            file: s(&td["prefixFilePath"]),
            link: s(&td["prefixLink"]),
            force: td["forcePrefix"].as_bool().expect("forcePrefix"),
        },
        url: opt("url"),
        pager: None,
        permalink: opt("expandedPermalink"),
        site_ugly: td["uglyURLs"].as_bool().expect("uglyURLs"),
        urls,
    }
}

#[test]
fn pagination_matches_go() {
    let mut t = Tally::default();
    let mut aliases = 0;
    for file in family("page/pagination", &["resolve.json.gz"]) {
        let fx = fixture(&format!("page/pagination/{file}"));
        let cx = Ctx {
            urls: fx["pathspecs"]
                .as_array()
                .expect("pathspecs")
                .iter()
                .map(|ps| (site_urls(ps), pagination_config(ps)))
                .collect(),
            formats: fx["formats"]
                .as_array()
                .expect("formats")
                .iter()
                .map(output_format)
                .collect(),
            paths: oracle_paths(&fx["paths"]),
        };
        for c in fx["cases"].as_array().expect("cases") {
            let Some(items) = items(&c["seq"]) else {
                t.skip("template-argument-conversion");
                continue;
            };
            let size = c["size"].as_i64().expect("size");
            let got = match usize::try_from(size)
                .ok()
                .map(|size| Pagination::new(items, size))
            {
                None | Some(Err(_)) => json!({ "err": "pager size" }),
                Some(Ok(p)) => {
                    let inp = inputs(c, &cx);
                    let (urls, cfg) = &cx.urls[idx(&c["td"]["ps"])];
                    let url = |n: usize| {
                        let tp = pager_paths(&inp, cfg, n).expect("pager paths");
                        links(&tp, urls, inp.format)
                            .expect("links")
                            .rel_permalink
                            .escaped()
                    };
                    // The `page/1` redirect of HTML formats is the pager path of page 1.
                    if pager_alias(&inp, inp.format, cfg).expect("alias").is_some() {
                        aliases += 1;
                    }
                    render(&p, &url)
                }
            };
            let want = &c["res"];
            let same = if want.get("err").is_some() {
                got.get("err").is_some()
            } else {
                got == *want
            };
            t.check(same, || {
                format!("{file}: {}\n   got {got}\n  want {want}", c["td"])
            });
        }
    }
    assert!(aliases > 1000, "{aliases} page/1 aliases");
    t.finish("pagination");
}

#[test]
fn pager_size_option_matches_go() {
    let mut t = Tally::default();
    let fx = fixture("page/pagination/resolve.json.gz");
    for c in fx["cases"].as_array().expect("cases") {
        let args: Vec<Value> = c["options"]
            .as_array()
            .expect("options")
            .iter()
            .map(value)
            .collect();
        let got = resolve_pager_size(&args, 10);
        let want = &c["res"];
        let same = match (&got, want.get("ok")) {
            (Ok(n), Some(w)) => w.as_u64() == u64::try_from(*n).ok(),
            (Err(_), None) => true,
            _ => false,
        };
        t.check(same, || {
            format!("{}: got {got:?} want {want}", c["options"])
        });
    }
    t.finish("pagination-size");
}

#[test]
fn pager_alias_targets() {
    // `page/1` of the home page, a section, and the 404 page (a file, not a directory).
    let fx = fixture("page/pagination/testsite.json.gz");
    let cx = Ctx {
        urls: fx["pathspecs"]
            .as_array()
            .expect("pathspecs")
            .iter()
            .map(|ps| (site_urls(ps), pagination_config(ps)))
            .collect(),
        formats: fx["formats"]
            .as_array()
            .expect("formats")
            .iter()
            .map(output_format)
            .collect(),
        paths: oracle_paths(&fx["paths"]),
    };
    let mut got: Vec<String> = Vec::new();
    for c in fx["cases"].as_array().expect("cases") {
        if c["src"] != "build" {
            continue;
        }
        let inp = inputs(c, &cx);
        let (_, cfg) = &cx.urls[idx(&c["td"]["ps"])];
        if let Some(a) = pager_alias(&inp, inp.format, cfg).expect("alias") {
            got.push(a.to_string());
        }
        let off = PaginationConfig {
            disable_aliases: true,
            ..cfg.clone()
        };
        assert_eq!(pager_alias(&inp, inp.format, &off).expect("alias"), None);
    }
    got.sort();
    got.dedup();
    for want in [
        "/page/1/index.html",
        "/404/page/1.html",
        "/nn/page/1/index.html",
    ] {
        assert!(got.iter().any(|g| g == want), "{want} not in {got:?}");
    }
}
