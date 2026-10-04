//! The default-sorted lists of each node: its pages, regular pages and sections, and the lists of
//! the site.

use super::*;

/// The pages directly in the section at `key`: pages and branch pages below it with no branch
/// page in between.
pub(super) fn in_section(m: &Model, lang: LangIdx, key: &ContentKey) -> Vec<PageId> {
    let tree = &m.sites[lang].tree;
    tree.descendants(key)
        .filter(|(k, _)| {
            let mut up = k.parent();
            while let Some(cur) = up {
                if cur == *key {
                    return true;
                }
                if tree.get(&cur).is_some_and(|b| m.pages[b].kind.is_branch()) {
                    return false;
                }
                up = cur.parent();
            }
            true
        })
        .map(|(_, id)| id)
        .collect()
}

/// `.Pages`, `.RegularPages`, `.Site.Pages`, `.Site.RegularPages` and `.Site.MainSections`.
pub(crate) fn lists(m: &mut Model) {
    let collators = Collators::new(&m.config);
    for lang in m.config.sites.ids() {
        let c = &collators[lang];
        let tree = &m.sites[lang].tree;
        let mut site_pages: Vec<PageId> = tree
            .iter()
            .map(|(_, id)| id)
            .filter(|&id| m.pages[id].listed(ListScope::Global))
            .collect();
        sort_default(m, c, &mut site_pages);
        let site_regular: Vec<PageId> = site_pages
            .iter()
            .copied()
            .filter(|&id| m.pages[id].kind == PageKind::Page)
            .collect();
        let mut local_regular: Vec<PageId> = tree
            .iter()
            .map(|(_, id)| id)
            .filter(|&id| {
                let p = &m.pages[id];
                p.kind == PageKind::Page && p.listed(ListScope::Local)
            })
            .collect();
        sort_default(m, c, &mut local_regular);

        let mut lists = Vec::new();
        for (key, id) in tree.iter() {
            let p = &m.pages[id];
            let local = |q: &PageId| m.pages[*q].listed(ListScope::Local);
            let kind_is = |q: &PageId, k: PageKind| m.pages[*q].kind == k;
            let (mut pages, mut regular): (Vec<PageId>, Vec<PageId>) = match p.kind {
                PageKind::Page => continue,
                PageKind::Home | PageKind::Section => {
                    let direct = in_section(m, lang, key);
                    (
                        direct
                            .iter()
                            .copied()
                            .filter(|q| {
                                local(q)
                                    && (kind_is(q, PageKind::Page) || kind_is(q, PageKind::Section))
                            })
                            .collect(),
                        direct
                            .into_iter()
                            .filter(|q| local(q) && kind_is(q, PageKind::Page))
                            .collect(),
                    )
                }
                // A hierarchical taxonomy lists its top-level terms.
                PageKind::Taxonomy => (
                    tree.descendants(key)
                        .map(|(_, q)| q)
                        .filter(|q| local(q) && kind_is(q, PageKind::Term))
                        .filter(|q| !hierarchical(m, id) || m.pages[*q].parent == Some(id))
                        .collect(),
                    in_section(m, lang, key)
                        .into_iter()
                        .filter(|q| local(q) && kind_is(q, PageKind::Page))
                        .collect(),
                ),
                PageKind::Term => {
                    let members: Vec<(PageId, i32)> = match (p.taxonomy, p.term) {
                        (Some(t), Some(term)) => m.sites[lang].taxonomies[t].terms[term]
                            .members
                            .iter()
                            .filter(|w| local(&w.page))
                            .map(|w| (w.page, w.weight))
                            .collect(),
                        _ => Vec::new(),
                    };
                    let mut members = members;
                    members.sort_by(|a, b| default_order(m, c, a.0, Some(a.1), b.0, Some(b.1)));
                    let pages: Vec<PageId> = members.iter().map(|w| w.0).collect();
                    let regular = pages
                        .iter()
                        .copied()
                        .filter(|q| kind_is(q, PageKind::Page))
                        .collect();
                    lists.push((id, pages, regular));
                    continue;
                }
                PageKind::NotFound
                | PageKind::Sitemap
                | PageKind::SitemapIndex
                | PageKind::RobotsTxt => {
                    lists.push((id, site_pages.clone(), site_regular.clone()));
                    continue;
                }
            };
            sort_default(m, c, &mut pages);
            sort_default(m, c, &mut regular);
            lists.push((id, pages, regular));
        }
        for (id, pages, regular) in lists {
            let mut sections = std::mem::take(&mut m.pages[id].sections);
            sort_default(m, c, &mut sections);
            let p = &mut m.pages[id];
            p.pages = pages;
            p.regular_pages = regular;
            p.sections = sections;
        }

        let main = match &m.config.sites[lang].main_sections {
            Some(s) => s.clone(),
            None => {
                let mut counts: std::collections::BTreeMap<&str, usize> =
                    std::collections::BTreeMap::new();
                for (_, id) in m.sites[lang].tree.iter() {
                    let p = &m.pages[id];
                    if p.kind == PageKind::Page && !p.section.is_empty() {
                        *counts.entry(p.section.as_str()).or_default() += 1;
                    }
                }
                let best = counts
                    .iter()
                    .fold(None::<(&str, usize)>, |best, (&s, &n)| match best {
                        Some((_, b)) if b >= n => best,
                        _ => Some((s, n)),
                    });
                vec![best.map_or_else(String::new, |(s, _)| s.to_owned())]
            }
        };
        let site = &mut m.sites[lang];
        site.pages = site_pages;
        site.regular_pages = site_regular;
        site.regular_pages_local = local_regular;
        site.main_sections = main;
    }
}
