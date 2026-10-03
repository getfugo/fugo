//! Phase B5: names, the tree relations, node dates and the default-sorted lists.
//!
//! Every ancestor lookup is segment-wise (`/docs/ab` is not below `/docs/a`), where Go's
//! radix tree matches characters (an accepted deviation).
//!
//! - **Names**: `.Title` (front matter; pages without a file get their kind's default title),
//!   `.LinkTitle`, `.Section` (first path segment), `.Type`.
//! - **Tree**: `.Parent` is the nearest branch page above (a bundled page's is its bundle),
//!   the home page at the root; `.CurrentSection` is a branch page itself, else the nearest
//!   branch page at or above its directory; `.FirstSection` the branch page at the root of that
//!   path (the home page for root pages); `.Sections` the section pages directly below a node.
//! - **Node dates**: a home, section or taxonomy page without dates takes the latest date,
//!   lastmod and (already passed) publish date of the pages below it (terms excepted, pages the
//!   build filter removed included); a term or taxonomy page without dates then takes those of
//!   the terms and members below it. `.Site.Lastmod` is the latest lastmod below the home page.
//! - **Lists** (default order): a home or section page lists the pages and sections directly
//!   in it; a taxonomy page its terms (all levels); a term page its members (weighted); a
//!   standalone page the site's pages.

use std::cmp::Ordering;
use std::ops::Index;

use jiff::{Timestamp, Zoned};
use ssg_base::paths::ContentKey;
use ssg_base::{IdVec, LangIdx, PageId, PageKind};
use ssg_config::Config;
use ssg_locale::Collator;
use ssg_page::{Dates, SortKey, default_title};

use crate::tree::PageRole;
use crate::{ListScope, LoadModelOptions, Model, Removed};

/// The collator of each language.
pub(crate) struct Collators(IdVec<LangIdx, Collator>);

impl Collators {
    pub fn new(cfg: &Config) -> Self {
        Self(
            cfg.sites
                .iter()
                .map(|s| Collator::for_language(&s.language.key))
                .collect(),
        )
    }
}

impl Index<LangIdx> for Collators {
    type Output = Collator;
    fn index(&self, lang: LangIdx) -> &Collator {
        &self.0[lang]
    }
}

/// Go's default page order between `a` and `b` (with their taxonomy weights in a term list).
pub(crate) fn default_order(
    m: &Model,
    c: &Collator,
    a: PageId,
    a_weight0: Option<i32>,
    b: PageId,
    b_weight0: Option<i32>,
) -> Ordering {
    let key = |id: PageId, weight0| {
        let p = &m.pages[id];
        SortKey {
            weight: p.meta.weight,
            date: p.meta.dates.date.as_ref(),
            link_title: &p.link_title,
            path: &p.path_info.path,
            ordinal: None,
            weight0,
        }
    };
    ssg_page::default_order(&key(a, a_weight0), &key(b, b_weight0), c)
}

/// Sorts page ids in the default order.
pub(crate) fn sort_default(m: &Model, c: &Collator, ids: &mut [PageId]) {
    ids.sort_by(|a, b| default_order(m, c, *a, None, *b, None));
}

/// `.Title`, `.LinkTitle`, `.Section` and `.Type` of every page.
pub(crate) fn names(m: &mut Model) {
    let cfg = m.config.clone();
    for lang in cfg.sites.ids() {
        let site = &cfg.sites[lang];
        let terms: Vec<(PageId, String)> = m.sites[lang]
            .taxonomies
            .iter()
            .flat_map(|t| t.terms.iter().map(|term| (term.page, term.term.clone())))
            .collect();
        for (page, term) in terms {
            if m.pages[page]
                .meta
                .title
                .as_deref()
                .unwrap_or_default()
                .is_empty()
                && m.pages[page].source.is_none()
            {
                m.pages[page].title = default_title(PageKind::Term, &term, &site.titles);
            }
        }
    }
    for p in m.pages.iter_mut() {
        let site = &cfg.sites[p.lang];
        let own = p.meta.title.clone().unwrap_or_default();
        if !own.is_empty() || p.source.is_some() {
            p.title = own;
        } else if p.kind != PageKind::Term {
            let raw = match p.kind {
                PageKind::Home => site.title.as_str(),
                PageKind::Section | PageKind::Taxonomy => p.path_info.original.name.as_str(),
                _ => "",
            };
            p.title = default_title(p.kind, raw, &site.titles);
        }
        p.link_title = match p.meta.link_title.as_deref() {
            Some(l) if !l.is_empty() => l.to_owned(),
            _ => p.title.clone(),
        };
        p.section.clone_from(&p.path_info.section);
        p.r#type = match p.meta.r#type.as_deref() {
            Some(t) if !t.is_empty() => t.to_owned(),
            _ if !p.section.is_empty() => p.section.clone(),
            _ => "page".to_owned(),
        };
    }
}

/// Whether `id` is the taxonomy page or a term page of a hierarchical taxonomy.
fn hierarchical(m: &Model, id: PageId) -> bool {
    let p = &m.pages[id];
    matches!(p.kind, PageKind::Taxonomy | PageKind::Term)
        && p.taxonomy
            .is_some_and(|t| m.sites[p.lang].taxonomies[t].def.hierarchical)
}

/// The first page at or above `key` (segment-wise) in `lang`'s tree that satisfies `pred`.
fn at_or_above(
    m: &Model,
    lang: LangIdx,
    key: &ContentKey,
    pred: impl Fn(PageId) -> bool,
) -> Option<PageId> {
    let tree = &m.sites[lang].tree;
    let mut k = Some(key.clone());
    while let Some(cur) = k {
        if let Some(id) = tree.get(&cur)
            && pred(id)
        {
            return Some(id);
        }
        k = cur.parent();
    }
    None
}

/// `.Parent`, `.Ancestors`, `.CurrentSection`, `.FirstSection` and `.Sections`.
pub(crate) fn tree(m: &mut Model) {
    let ids: Vec<PageId> = m.pages.ids().collect();
    let is_branch = |m: &Model, id: PageId| m.pages[id].kind.is_branch();
    let mut rel = Vec::with_capacity(ids.len());
    for &id in &ids {
        let p = &m.pages[id];
        let home = m.sites[p.lang].home;
        let parent = if p.kind == PageKind::Home && p.role == PageRole::Standalone {
            None
        } else if p.role == PageRole::Standalone {
            let above = p.key.parent().unwrap_or_default();
            Some(at_or_above(m, p.lang, &above, |q| is_branch(m, q)).unwrap_or(home))
        } else {
            Some(m.bundle_owner(id).unwrap_or(home))
        };
        let current = if p.kind.is_branch() {
            id
        } else {
            at_or_above(m, p.lang, &p.dir_key(), |q| is_branch(m, q)).unwrap_or(home)
        };
        let first = at_or_above(m, p.lang, &p.dir_key(), |q| {
            is_branch(m, q) && m.pages[q].key.segments().count() <= 1
        })
        .unwrap_or(home);
        rel.push((id, parent, current, first));
    }
    for (id, parent, current, first) in rel {
        let p = &mut m.pages[id];
        p.parent = parent;
        p.current_section = current;
        p.first_section = first;
    }
    for &id in &ids {
        let mut ancestors = Vec::new();
        let mut cur = m.pages[id].parent;
        while let Some(a) = cur {
            if ancestors.contains(&a) {
                break;
            }
            ancestors.push(a);
            cur = m.pages[a].parent;
        }
        m.pages[id].ancestors = ancestors;
    }
    for &id in &ids {
        let p = &m.pages[id];
        if p.kind == PageKind::Page || p.role != PageRole::Standalone {
            continue;
        }
        // The nodes of a hierarchical taxonomy have their child terms.
        let child_kind = if hierarchical(m, id) {
            PageKind::Term
        } else {
            PageKind::Section
        };
        let sections: Vec<PageId> = in_section(m, p.lang, &p.key)
            .into_iter()
            .filter(|&c| {
                let c = &m.pages[c];
                c.kind == child_kind && c.listed(ListScope::Local) && c.parent == Some(id)
            })
            .collect();
        m.pages[id].sections = sections;
    }
}

/// Takes the later dates of `from` (a publish date only when it has passed).
fn absorb(d: &mut Dates, from: &Dates, now: Timestamp) {
    fn later(a: Option<&Zoned>, b: Option<&Zoned>) -> bool {
        match (a, b) {
            (Some(a), Some(b)) => a.timestamp() > b.timestamp(),
            (Some(_), None) => true,
            (None, _) => false,
        }
    }
    if later(from.date.as_ref(), d.date.as_ref()) {
        d.date.clone_from(&from.date);
    }
    if later(from.lastmod.as_ref(), d.lastmod.as_ref()) {
        d.lastmod.clone_from(&from.lastmod);
    }
    if later(from.publish_date.as_ref(), d.publish_date.as_ref())
        && from
            .publish_date
            .as_ref()
            .is_some_and(|p| p.timestamp() < now)
    {
        d.publish_date.clone_from(&from.publish_date);
    }
}

/// Node dates and `.Site.Lastmod` (see the module docs).
pub(crate) fn node_dates(m: &mut Model, o: &LoadModelOptions, removed: &[Removed]) {
    let now = o.clock.0;
    for lang in m.config.sites.ids() {
        // Home, sections and taxonomies: the pages below them (terms excepted).
        let branches: Vec<PageId> = m.sites[lang]
            .tree
            .iter()
            .map(|(_, id)| id)
            .filter(|&id| {
                matches!(
                    m.pages[id].kind,
                    PageKind::Home | PageKind::Section | PageKind::Taxonomy
                )
            })
            .collect();
        let mut updates = Vec::new();
        let mut last_mod: Option<Zoned> = None;
        for b in branches {
            let bp = &m.pages[b];
            let zero = bp.meta.dates.is_empty();
            let home = bp.kind == PageKind::Home;
            if !zero && !home {
                continue;
            }
            let mut dates = bp.meta.dates.clone();
            let mut below_lastmod = Dates::default();
            let below = m.sites[lang]
                .tree
                .descendants(&bp.key)
                .map(|(_, id)| &m.pages[id])
                .filter(|p| p.kind != PageKind::Term)
                .map(|p| &p.meta.dates)
                .chain(
                    removed
                        .iter()
                        .filter(|r| {
                            r.lang == lang
                                && r.kind != PageKind::Term
                                && r.key != bp.key
                                && r.key.starts_with_segments(&bp.key)
                        })
                        .map(|r| &r.dates),
                );
            let mut any = false;
            for d in below {
                any = true;
                if zero {
                    absorb(&mut dates, d, now);
                }
                absorb(&mut below_lastmod, d, now);
            }
            if home && any {
                let mut lm = Dates {
                    lastmod: dates.lastmod.clone(),
                    ..Dates::default()
                };
                absorb(&mut lm, &below_lastmod, now);
                last_mod = lm.lastmod;
            }
            if zero {
                updates.push((b, dates));
            }
        }
        for (b, dates) in updates {
            m.pages[b].meta.dates = dates;
        }
        m.sites[lang].last_mod = last_mod;

        // Taxonomies and terms: the terms and members below them.
        let site = &m.config.sites[lang];
        for idx in crate::taxonomy::views(site) {
            let plural = ContentKey::from_source(&site.taxonomies[idx].plural);
            if plural.is_home() {
                continue;
            }
            let nodes: Vec<PageId> = m.sites[lang]
                .tree
                .get(&plural)
                .into_iter()
                .chain(m.sites[lang].tree.descendants(&plural).map(|(_, id)| id))
                .filter(|&id| matches!(m.pages[id].kind, PageKind::Taxonomy | PageKind::Term))
                .collect();
            let mut updates = Vec::new();
            for &n in &nodes {
                let np = &m.pages[n];
                if !np.meta.dates.is_empty() {
                    continue;
                }
                let mut dates = Dates::default();
                for &t in &nodes {
                    let tp = &m.pages[t];
                    if tp.kind != PageKind::Term || !tp.key.starts_with_segments(&np.key) {
                        continue;
                    }
                    if t != n {
                        absorb(&mut dates, &tp.meta.dates, now);
                    }
                    if let Some(term) = tp.term.and_then(|ti| {
                        tp.taxonomy
                            .map(|tx| &m.sites[lang].taxonomies[tx].terms[ti])
                    }) {
                        for w in &term.members {
                            absorb(&mut dates, &m.pages[w.page].meta.dates, now);
                        }
                    }
                }
                // Content term pages the build filter removed still pass their own dates up.
                for r in removed {
                    if r.lang == lang
                        && r.kind == PageKind::Term
                        && r.key != np.key
                        && r.key.starts_with_segments(&np.key)
                    {
                        absorb(&mut dates, &r.dates, now);
                    }
                }
                updates.push((n, dates));
            }
            for (n, dates) in updates {
                m.pages[n].meta.dates = dates;
            }
        }
    }
}

/// The pages directly in the section at `key`: pages and branch pages below it with no branch
/// page in between.
fn in_section(m: &Model, lang: LangIdx, key: &ContentKey) -> Vec<PageId> {
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
