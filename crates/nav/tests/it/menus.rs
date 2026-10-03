//! Oracle: menus (`oracle/page/menus/*`, Go's `menu_test.go` sites, docs and a nested
//! multilingual site). The configured menus are decoded by `ssg-config` from the recorded
//! `menus` tables, the pages' own entries by `ssg-page` from their front matter; then the
//! assembled menus (tree, order, names, titles, URLs, weights, parents, params), the pages' own
//! entries, `IsMenuCurrent`/`HasMenuCurrent` for every page and entry, and the menu sorts must
//! equal Go's.

use std::collections::BTreeMap;

use serde_json::{Value as J, json};
use ssg_base::url::{Accents, BaseUrl, LinkStyle, PathCase, SiteUrls};
use ssg_base::{Idx, LangIdx, PageId, Params};
use ssg_config::sections::MenuEntryConfig;
use ssg_nav::{
    MenuEntry, MenuOptions, NavModel, SiteMenus, build_site_menus, compare_names, menu_order,
    page_menu_entries,
};

use crate::support::{
    DumpSite, Project, Tally, family, fixture, idx, page_id, page_menus, s, value,
};

/// The configured entries of a `menus` table, as `ssg-config` decodes them.
fn config_entries(menus: &J) -> Result<Vec<MenuEntryConfig>, String> {
    let menus = if menus.is_null() {
        json!({})
    } else {
        menus.clone()
    };
    let p = Project::json(&json!({ "menus": menus }))?;
    Ok(p.cfg.sites[LangIdx::from_index(0)].menus.clone())
}

fn params_eq(mine: &Params, want: &J) -> bool {
    match want.as_object() {
        None => mine.is_empty(),
        Some(_) => *mine == Params::fold(value(want).as_map().expect("map")),
    }
}

/// A decoded configuration entry against the oracle's (in the menu's sorted order).
fn config_entry_eq(c: &MenuEntryConfig, w: &J) -> bool {
    c.menu == s(&w["menu"])
        && c.identifier == s(&w["identifier"])
        && c.name == s(&w["name"])
        && c.pre == s(&w["pre"])
        && c.post == s(&w["post"])
        && c.url == s(&w["url"])
        && c.page_ref == s(&w["pageRef"])
        && i64::from(c.weight) == w["weight"].as_i64().expect("weight")
        && c.parent == s(&w["parent"])
        && c.title == s(&w["title"])
        && params_eq(&c.params, &w["params"])
}

/// Checks a decode case: the entries of each menu, sorted.
fn check_decode(file: &str, c: &J, entries: &[MenuEntryConfig], t: &mut Tally) {
    let want = &c["res"]["ok"]["menus"];
    let mut by_menu: BTreeMap<&str, Vec<MenuEntry>> = BTreeMap::new();
    let mut cfgs: BTreeMap<&str, Vec<&MenuEntryConfig>> = BTreeMap::new();
    for e in entries {
        cfgs.entry(e.menu.as_str()).or_default().push(e);
    }
    let mut ok = cfgs.len() == want.as_object().map_or(0, serde_json::Map::len);
    for (menu, list) in &cfgs {
        // Sort the configured entries the way menus are sorted.
        let mut order: Vec<usize> = (0..list.len()).collect();
        let as_entry = |c: &MenuEntryConfig| MenuEntry {
            identifier: c.identifier.clone(),
            name: c.name.clone(),
            weight: c.weight,
            ..MenuEntry::default()
        };
        let entries: Vec<MenuEntry> = list.iter().map(|c| as_entry(c)).collect();
        order.sort_by(|&a, &b| menu_order(&entries[a], &entries[b]));
        by_menu.insert(menu, entries);
        let w = want[*menu].as_array().map_or(&[][..], Vec::as_slice);
        ok &= w.len() == order.len()
            && order
                .iter()
                .zip(w)
                .all(|(&i, w)| config_entry_eq(list[i], w));
    }
    t.check(ok, || {
        format!(
            "{file}: decode {}\n   got {entries:?}\n  want {want}",
            c["in"]
        )
    });
}

/// The site's URL helpers, from its home page link (`/foo/`, `/en/`).
fn urls_of(site: &DumpSite, lang: LangIdx) -> SiteUrls {
    let home = site
        .home(lang)
        .map(|h| site.page(h).rel_permalink.to_owned());
    let mut base = home.unwrap_or_else(|| "/".to_owned());
    let key = site
        .pages
        .iter()
        .find(|p| p.lang == lang)
        .map(|p| p.lang_key.clone())
        .unwrap_or_default();
    if site.langs > 1
        && let Some(b) = base.strip_suffix(&format!("{key}/"))
    {
        base = b.to_owned();
    }
    SiteUrls {
        base_url: BaseUrl::parse(&format!("https://example.org{base}")).expect("base URL"),
        language_prefix: String::new(),
        link_style: LinkStyle::Relative,
        path_case: PathCase::Lower,
        accents: Accents::Keep,
    }
}

/// The `sectionPagesMenu` of a site: the menu whose entries Go made from sections (they have
/// no menu name).
fn section_pages_menu(fx: &J, site: usize) -> Option<String> {
    let entries = fx["entries"].as_array().expect("entries");
    let menus = fx["siteMenus"][site].as_object()?;
    menus.iter().find_map(|(name, list)| {
        list.as_array()?
            .iter()
            .any(|i| {
                let e = &entries[idx(i)];
                s(&e["menu"]).is_empty() && e["page"].as_i64().is_some_and(|p| p >= 0)
            })
            .then(|| name.clone())
    })
}

/// Compares an assembled entry with oracle entry `i`; records where each oracle entry is.
fn entry_eq(mine: &MenuEntry, fx: &J, i: usize, at: &mut BTreeMap<usize, MenuEntry>) -> bool {
    let w = &fx["entries"][i];
    at.insert(i, mine.clone());
    let page = w["page"].as_i64().expect("page");
    let want_page = usize::try_from(page).ok().map(page_id);
    let children = w["children"].as_array().map_or(&[][..], Vec::as_slice);
    let mut ok = mine.identifier == s(&w["identifier"])
        && mine.name == s(&w["name"])
        && mine.title == s(&w["title"])
        && mine.pre == s(&w["pre"])
        && mine.post == s(&w["post"])
        && mine.url == s(&w["urlResult"])
        && i64::from(mine.weight) == w["weight"].as_i64().expect("weight")
        && mine.parent.as_deref().unwrap_or_default() == s(&w["parent"])
        && mine.page == want_page
        && mine.key_name() == s(&w["keyName"])
        && mine.has_children() == w["hasChildren"].as_bool().expect("hasChildren")
        && params_eq(&mine.params, &w["params"])
        && mine.children.len() == children.len();
    for (c, wc) in mine.children.iter().zip(children) {
        ok &= entry_eq(c, fx, idx(wc), at);
    }
    ok
}

fn describe(e: &MenuEntry) -> String {
    format!(
        "{}({}) url={} w={} parent={:?} page={:?} children={:?}",
        e.name,
        e.identifier,
        e.url,
        e.weight,
        e.parent,
        e.page.map(Idx::index),
        e.children
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>()
    )
}

fn run(file: &str, t: &mut Tally) {
    let fx = fixture(&format!("page/menus/{file}"));
    let site = DumpSite::new(&fx);
    let cases = fx["cases"].as_array().expect("cases");

    // The configured entries of each language.
    let mut configs: Vec<Vec<MenuEntryConfig>> = vec![Vec::new(); site.langs];
    for c in cases.iter().filter(|c| c["op"] == "decode") {
        let entries = config_entries(&c["in"]).unwrap_or_else(|e| panic!("{file}: {e}"));
        check_decode(file, c, &entries, t);
        configs[idx(&c["site"])] = entries;
    }

    // The assembled menus.
    let mut built: Vec<SiteMenus> = Vec::new();
    let mut at: BTreeMap<usize, MenuEntry> = BTreeMap::new();
    for (si, entries) in configs.iter().enumerate() {
        let lang = LangIdx::from_index(si);
        let opts = MenuOptions {
            entries,
            section_pages_menu: section_pages_menu(&fx, si),
            urls: urls_of(&site, lang),
        };
        let (menus, _) = build_site_menus(&site, lang, &opts);
        let mut want = fx["siteMenus"][si].as_object().cloned().unwrap_or_default();
        want.retain(|_, l| l.as_array().is_some_and(|a| !a.is_empty()));
        let names_ok = menus.menus.keys().eq(want.keys());
        t.check(names_ok, || {
            format!(
                "{file}: site {si} menus {:?} want {:?}",
                menus.menus.keys(),
                want.keys()
            )
        });
        for (name, list) in &want {
            let list = list.as_array().expect("menu");
            let mine = menus.menu(name);
            let ok = mine.len() == list.len()
                && mine
                    .iter()
                    .zip(list)
                    .all(|(e, w)| entry_eq(e, &fx, idx(w), &mut at));
            t.check(ok, || {
                format!(
                    "{file}: site {si} menu {name}\n   got {:?}\n  want {:?}",
                    mine.iter().map(describe).collect::<Vec<_>>(),
                    list.iter()
                        .map(|i| {
                            let w = &fx["entries"][idx(i)];
                            format!(
                                "{}({}) url={} w={}",
                                w["name"], w["identifier"], w["urlResult"], w["weight"]
                            )
                        })
                        .collect::<Vec<_>>()
                )
            });
        }
        built.push(menus);
    }

    for c in cases {
        let si = idx(&c["site"]);
        match s(&c["op"]) {
            "decode" => {}
            "PageMenusFromPage" => {
                let id = page_id(idx(&c["page"]));
                let Ok(decoded) = page_menus(&Params::fold(
                    value(&json!({ "menus": c["in"] })).as_map().expect("map"),
                )) else {
                    t.check(c["res"].get("err").is_some(), || {
                        format!("{file}: PageMenusFromPage {c}: rejected")
                    });
                    continue;
                };
                let mut facts = site.page(id);
                facts.menus = &decoded;
                let got = page_menu_entries(&facts);
                let want = c["res"]["ok"].as_object().cloned().unwrap_or_default();
                let ok = got.keys().eq(want.keys())
                    && got.iter().all(|(menu, e)| {
                        let w = &want[menu];
                        e.identifier == s(&w["identifier"])
                            && e.name == s(&w["name"])
                            && e.title == s(&w["title"])
                            && e.pre == s(&w["pre"])
                            && e.post == s(&w["post"])
                            && e.parent.as_deref().unwrap_or_default() == s(&w["parent"])
                            && i64::from(e.weight) == w["weight"].as_i64().expect("weight")
                            && e.page.map(Idx::index) == w["page"].as_u64().map(|p| p as usize)
                            && params_eq(&e.params, &w["params"])
                    });
                t.check(ok, || {
                    format!("{file}: PageMenusFromPage {c}\n   got {got:?}")
                });
            }
            "current" => {
                let page: PageId = page_id(idx(&c["page"]));
                let menus = &built[si];
                // The page's own entries must be the recorded ones.
                let own = c["pageMenus"].as_object().cloned().unwrap_or_default();
                let mine_own = menus.page_menus.get(&page).cloned().unwrap_or_default();
                let own_ok = mine_own.keys().eq(own.keys())
                    && own.iter().all(|(menu, i)| {
                        let w = &fx["entries"][idx(i)];
                        mine_own[menu].name == s(&w["name"])
                            && mine_own[menu].identifier == s(&w["identifier"])
                    });
                t.check(own_ok, || {
                    format!(
                        "{file}: page {page:?} own entries {:?} want {own:?}",
                        mine_own.keys()
                    )
                });
                for r in c["res"].as_array().map_or(&[][..], Vec::as_slice) {
                    let menu = s(&r[0]);
                    let i = idx(&r[1]);
                    let entry = at.get(&i).cloned().or_else(|| {
                        own.iter()
                            .find(|(_, oi)| idx(oi) == i)
                            .and_then(|(m, _)| mine_own.get(m).cloned())
                    });
                    let Some(entry) = entry else {
                        t.fail(|| format!("{file}: entry {i} not assembled"));
                        continue;
                    };
                    let got = (
                        menus.is_menu_current(page, menu, &entry),
                        menus.has_menu_current(&site, page, menu, &entry),
                    );
                    let want = (r[2].as_bool().expect("is"), r[3].as_bool().expect("has"));
                    t.check(got == want, || {
                        format!(
                            "{file}: page {} {menu} entry {i} ({}): got {got:?} want {want:?}",
                            c["page"],
                            describe(&entry)
                        )
                    });
                }
            }
            "sort" => {
                let input: Vec<usize> = c["in"].as_array().expect("in").iter().map(idx).collect();
                let Some(entries) = input.iter().map(|i| at.get(i)).collect::<Option<Vec<_>>>()
                else {
                    t.fail(|| format!("{file}: sort input not assembled {input:?}"));
                    continue;
                };
                let order = |cmp: &dyn Fn(&MenuEntry, &MenuEntry) -> std::cmp::Ordering| {
                    let mut pos: Vec<usize> = (0..input.len()).collect();
                    pos.sort_by(|&a, &b| cmp(entries[a], entries[b]));
                    pos.into_iter().map(|p| input[p]).collect::<Vec<_>>()
                };
                let sorted = order(&menu_order);
                let by_name = order(&|a, b| compare_names(&a.name, &b.name));
                let reversed: Vec<usize> = input.iter().rev().copied().collect();
                let limit2: Vec<usize> = input.iter().take(2).copied().collect();
                let ids = |k: &str| -> Vec<usize> {
                    c[k].as_array()
                        .map_or_else(Vec::new, |a| a.iter().map(idx).collect())
                };
                for (k, got) in [
                    ("sort", &sorted),
                    ("byWeight", &sorted),
                    ("byName", &by_name),
                    ("reverse", &reversed),
                    ("limit2", &limit2),
                ] {
                    t.check(*got == ids(k), || {
                        format!("{file}: {k} {input:?}: got {got:?} want {:?}", ids(k))
                    });
                }
            }
            op => panic!("unknown op {op}"),
        }
    }
}

#[test]
fn menus_match_go() {
    let mut t = Tally::default();
    for file in family("page/menus", &["decode.json.gz"]) {
        run(&file, &mut t);
    }
    t.finish("menus");
}

#[test]
fn menu_config_decodes_like_go() {
    let mut t = Tally::default();
    let fx = fixture("page/menus/decode.json.gz");
    for c in fx["cases"].as_array().expect("cases") {
        let entries = config_entries(&c["in"]);
        let menus = c["in"].as_object();
        match (&entries, c["res"].get("ok").is_some()) {
            (Ok(e), true) => {
                // Menu names are configuration keys, which the site's configuration loader
                // lower-cases (Go's too; this oracle calls `navigation.DecodeConfig` on
                // keys as written).
                let folded = menus.is_some_and(|m| m.keys().any(|k| k.to_lowercase() != *k));
                if folded {
                    t.accept("config-menu-name-folded");
                } else {
                    check_decode("decode", c, e, &mut t);
                }
            }
            (Err(_), false) => t.pass(),
            (got, _) => t.fail(|| format!("decode {}: got {got:?} want {}", c["in"], c["res"])),
        }
    }
    t.finish("menus-decode");
}
