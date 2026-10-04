//! The site's side of a structure check: lists, sections, taxonomies and terms against the
//! oracle's.

use super::*;

/// Our term lists and taxonomies as the oracle writes them.
#[allow(clippy::too_many_lines)]
pub(super) fn check_site(
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
