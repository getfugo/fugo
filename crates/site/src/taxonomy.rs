//! Taxonomies and terms (part of phase B3).
//!
//! Every linked page of a language, in tree order, names its terms in front matter: `tags:
//! [a, b]` (a single value is one term; numbers and booleans are written out; a list holding a
//! list or a map names none). A term's key is `/<plural>/<value>` read as a content path
//! (lower case, spaces as `-`, a `/` nests); the page at that key is the term page (made when
//! missing). A page's weight in a taxonomy is its `<plural>_weight`. The term's `.Data.Term`
//! is the value last written for it; a term page nobody names keeps its path's name.
//!
//! A hierarchical taxonomy (`hierarchical = true`) makes its terms a tree. A value without a
//! `/` that is no top-level term names the one term whose last segment it is, among the term
//! pages and the paths pages write (`snacks/chips`); two such terms are ambiguous (a warning,
//! the value stays top-level). Every term above a term exists (made when missing), a term
//! lists the pages of the terms below it (the smallest weight counts), and `.Data.Term` and
//! the made pages' titles are the last segment as written.

use std::collections::BTreeMap;

use ssg_base::diag::Diagnostic;
use ssg_base::paths::ContentKey;
use ssg_base::{IdVec, LangIdx, PageId, PageKind, TaxonomyIdx, TermIdx, Value};
use ssg_config::SiteConfig;
use ssg_config::sections::TaxonomyDef;
use ssg_vfs::PathInfo;

use crate::nodes::Maker;
use crate::relations::{self, Collators};
use crate::{ListScope, Model, ModelError, Removed};

/// A configured taxonomy of one language.
#[derive(Clone, Debug)]
pub struct Taxonomy {
    pub def: TaxonomyDef,
    /// The taxonomy page (`None` when the build removed it).
    pub page: Option<PageId>,
    /// The term pages below the taxonomy page, by key.
    pub terms: IdVec<TermIdx, Term>,
}

/// A term of a taxonomy.
#[derive(Clone, Debug)]
pub struct Term {
    /// The term page's key (`tags/blue-sky`).
    pub key: ContentKey,
    /// `.Data.Term`: the value last written for the term (`Blue Sky`), else its path's name.
    pub term: String,
    pub page: PageId,
    /// The pages that name the term (in a hierarchical taxonomy also those of the terms below
    /// it): by weight, then in the default order.
    pub members: Vec<WeightedPage>,
    /// In a hierarchical taxonomy: the nearest term above (none at the top) and the terms
    /// directly below, in key order.
    pub parent: Option<TermIdx>,
    pub children: Vec<TermIdx>,
}

/// A page in a term with its weight (`tags_weight`) and position among the page's values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WeightedPage {
    pub page: PageId,
    pub weight: i32,
    pub ordinal: u32,
}

impl Taxonomy {
    /// The terms listed in `.Site.Taxonomies`: listed site-wide and named by a page.
    pub fn listed_terms<'a>(&'a self, m: &'a Model) -> impl Iterator<Item = (TermIdx, &'a Term)> {
        self.terms.iter_enumerated().filter(move |(_, t)| {
            !t.members.is_empty() && m.pages[t.page].listed(ListScope::Global)
        })
    }

    /// A term's key in templates: its last segment (`blue-sky`); in a hierarchical taxonomy
    /// its path below the taxonomy (`snacks/chips`).
    #[must_use]
    pub fn key_of(&self, term: &Term) -> String {
        if self.def.hierarchical {
            let depth = ContentKey::from_source(&self.def.plural).segments().count();
            term.key
                .segments()
                .skip(depth)
                .collect::<Vec<_>>()
                .join("/")
        } else {
            term.key.segments().last().unwrap_or_default().to_owned()
        }
    }
}

/// The terms of a hierarchical taxonomy known before its values are read: the term pages and
/// the paths pages write, with all the terms above them.
#[derive(Default)]
struct Known {
    /// The key of a path a page writes → that path as first written (`/snacks/Chips`).
    written: BTreeMap<ContentKey, String>,
    /// Last key segment → the known terms that end in it.
    leaves: BTreeMap<String, Vec<ContentKey>>,
}

impl Known {
    fn collect(
        m: &Model,
        maker: &Maker<'_>,
        lang: LangIdx,
        plural: &str,
        pages: &[PageId],
    ) -> Self {
        let mut k = Self::default();
        let root = ContentKey::from_source(plural);
        let mut keys: Vec<ContentKey> = m.sites[lang]
            .tree
            .descendants(&root)
            .filter(|&(_, id)| m.pages[id].kind == PageKind::Term)
            .map(|(key, _)| key.clone())
            .collect();
        for &id in pages {
            let Some(values) = m.pages[id].params().get(plural).and_then(term_values) else {
                continue;
            };
            for v in values {
                let segments: Vec<&str> = v.split('/').filter(|s| !s.is_empty()).collect();
                if segments.len() < 2 {
                    continue;
                }
                for n in 1..=segments.len() {
                    let path = format!("/{plural}/{}", segments[..n].join("/"));
                    if let Some(info) = maker.parse(&format!("{path}/_index.md")) {
                        k.written.entry(info.key.clone()).or_insert(path);
                        keys.push(info.key);
                    }
                }
            }
        }
        keys.sort();
        keys.dedup();
        for key in keys {
            let leaf = key.segments().last().unwrap_or_default().to_owned();
            k.leaves.entry(leaf).or_default().push(key);
        }
        k
    }

    /// The term `value` names when its path is `top`: `top` itself unless the value has no
    /// `/`, no term is at `top` and one known term ends in its last segment. `Err` with the
    /// candidates when several do.
    fn resolve(
        &self,
        m: &Model,
        lang: LangIdx,
        value: &str,
        top: &ContentKey,
    ) -> Result<ContentKey, Vec<ContentKey>> {
        if value.trim_matches('/').contains('/')
            || m.sites[lang].tree.get(top).is_some()
            || self.written.contains_key(top)
        {
            return Ok(top.clone());
        }
        let leaf = top.segments().last().unwrap_or_default();
        match self.leaves.get(leaf).map(Vec::as_slice) {
            Some([one]) => Ok(one.clone()),
            Some(many) if many.len() > 1 => Err(many.to_vec()),
            _ => Ok(top.clone()),
        }
    }

    /// The path info of the known term `key` for making its page.
    fn info(
        &self,
        m: &Model,
        maker: &Maker<'_>,
        lang: LangIdx,
        key: &ContentKey,
    ) -> Option<PathInfo> {
        match m.sites[lang].tree.get(key) {
            Some(id) => Some(m.pages[id].path_info.clone()),
            None => maker.parse(&format!("{}/_index.md", self.written.get(key)?)),
        }
    }
}

/// The last segment of a term value as written (`Chips` of `snacks/Chips`).
fn last_segment(v: &str) -> &str {
    v.trim_matches('/').rsplit('/').next().unwrap_or_default()
}

/// A site's taxonomies in the order Go walks them: by plural, each plural once.
pub(crate) fn views(site: &SiteConfig) -> Vec<TaxonomyIdx> {
    let mut v: Vec<TaxonomyIdx> = site.taxonomies.ids().collect();
    v.sort_by(|a, b| site.taxonomies[*a].plural.cmp(&site.taxonomies[*b].plural));
    v.dedup_by(|a, b| site.taxonomies[*a].plural == site.taxonomies[*b].plural);
    v
}

/// The term values of a front matter value: `None` when it names no terms.
fn term_values(v: &Value) -> Option<Vec<String>> {
    fn scalar(v: &Value) -> Option<String> {
        match v {
            Value::Null => Some(String::new()),
            Value::String(s) => Some(s.to_string()),
            Value::Int(i) => Some(i.to_string()),
            Value::Float(f) => Some(f.to_string()),
            Value::Bool(b) => Some(b.to_string()),
            Value::Date(_) | Value::Array(_) | Value::Map(_) => None,
        }
    }
    match v {
        Value::Null => None,
        Value::Array(items) => items.iter().map(scalar).collect(),
        other => scalar(other).map(|s| vec![s]),
    }
}

/// A `<plural>_weight` value as an integer; `Err` for a value that is not one.
fn weight_of(v: Option<&Value>) -> Result<i32, ()> {
    let w = match v {
        None | Some(Value::Null) => 0,
        Some(Value::Int(i)) => *i,
        #[allow(clippy::cast_possible_truncation)]
        Some(Value::Float(f)) => f.trunc() as i64,
        Some(Value::Bool(b)) => i64::from(*b),
        Some(Value::String(s)) => {
            let s = s.trim();
            let s = s.strip_suffix(".0").unwrap_or(s);
            s.parse::<i64>().map_err(|_| ())?
        }
        Some(Value::Date(_) | Value::Array(_) | Value::Map(_)) => return Err(()),
    };
    i32::try_from(w).map_err(|_| ())
}

/// The term pages of one language, their members and the language's taxonomy tables.
pub(crate) fn assemble_terms(
    m: &mut Model,
    maker: &Maker<'_>,
    lang: LangIdx,
    removed: &[Removed],
) -> Result<(), ModelError> {
    let cfg = m.config.clone();
    let site = &cfg.sites[lang];
    // A content term page the build filter removed takes its values with it.
    let gone = |key: &ContentKey| {
        removed
            .iter()
            .any(|r| r.lang == lang && r.kind == PageKind::Term && r.key == *key)
    };
    // (term page, member) → (weight, ordinal); the last value naming the term.
    let mut entries: BTreeMap<(PageId, PageId), (i32, u32)> = BTreeMap::new();
    let mut last_value: BTreeMap<PageId, String> = BTreeMap::new();
    if !site.disable_kinds.contains(PageKind::Term) {
        let pages: Vec<PageId> = m.sites[lang]
            .tree
            .iter()
            .map(|(_, id)| id)
            .filter(|&id| m.pages[id].linked())
            .collect();
        let views = views(site);
        let known: BTreeMap<TaxonomyIdx, Known> = views
            .iter()
            .filter(|&&idx| site.taxonomies[idx].hierarchical)
            .map(|&idx| {
                let plural = &site.taxonomies[idx].plural;
                (idx, Known::collect(m, maker, lang, plural, &pages))
            })
            .collect();
        let mut ambiguous: BTreeMap<(TaxonomyIdx, String), Vec<ContentKey>> = BTreeMap::new();
        for id in pages {
            for &idx in &views {
                let plural = &site.taxonomies[idx].plural;
                let Some(values) = m.pages[id].params().get(plural).and_then(term_values) else {
                    continue;
                };
                let weight = match weight_of(m.pages[id].params().get(&format!("{plural}_weight")))
                {
                    Ok(w) => w,
                    Err(()) => {
                        m.diagnostics.push(
                            Diagnostic::warning(format!(
                                "{}: {plural}_weight is not an integer",
                                m.pages[id].path()
                            ))
                            .with_id("taxonomy-weight"),
                        );
                        0
                    }
                };
                for (i, v) in values.into_iter().enumerate() {
                    if v.is_empty() {
                        continue;
                    }
                    let Some(mut info) = maker.parse(&format!("/{plural}/{v}/_index.md")) else {
                        continue;
                    };
                    if let Some(k) = known.get(&idx) {
                        match k.resolve(m, lang, &v, &info.key) {
                            Ok(key) if key == info.key => {}
                            Ok(key) => match k.info(m, maker, lang, &key) {
                                Some(found) => info = found,
                                None => continue,
                            },
                            Err(candidates) => {
                                ambiguous.entry((idx, v.clone())).or_insert(candidates);
                            }
                        }
                    }
                    let key = info.key.clone();
                    let term = match m.sites[lang].tree.get(&key) {
                        Some(t) => t,
                        None if gone(&key) => continue,
                        None => match maker.make(m, lang, PageKind::Term, key, info)? {
                            Some(t) => t,
                            None => continue,
                        },
                    };
                    let name = if known.contains_key(&idx) {
                        last_segment(&v).to_owned()
                    } else {
                        v
                    };
                    last_value.insert(term, name);
                    let ordinal = u32::try_from(i).unwrap_or(u32::MAX);
                    entries.insert((term, id), (weight, ordinal));
                }
            }
        }
        for ((idx, v), candidates) in ambiguous {
            let paths: Vec<String> = candidates.iter().map(ContentKey::to_path).collect();
            m.diagnostics.push(
                Diagnostic::warning(format!(
                    "{} term {v:?} is ambiguous: it ends {}; write its path",
                    site.taxonomies[idx].plural,
                    paths.join(" and ")
                ))
                .with_id("taxonomy-ambiguous-term"),
            );
        }
        // Every term above a term of a hierarchical taxonomy, named as its path is written.
        for (idx, k) in &known {
            let root = ContentKey::from_source(&site.taxonomies[*idx].plural);
            let below: Vec<(ContentKey, String)> = m.sites[lang]
                .tree
                .descendants(&root)
                .filter(|&(_, id)| m.pages[id].kind == PageKind::Term)
                .map(|(key, id)| (key.clone(), m.pages[id].path_info.original.base.clone()))
                .collect();
            for (key, base) in below {
                let mut above = Vec::new();
                let mut up = key.parent();
                while let Some(cur) = up.filter(|c| *c != root && c.starts_with_segments(&root)) {
                    up = cur.parent();
                    above.push(cur);
                }
                for cur in above.into_iter().rev() {
                    if m.sites[lang].tree.get(&cur).is_some() || gone(&cur) {
                        continue;
                    }
                    let depth = cur.segments().count();
                    let path = k.written.get(&cur).cloned().unwrap_or_else(|| {
                        let segments: Vec<&str> = base
                            .split('/')
                            .filter(|s| !s.is_empty())
                            .take(depth)
                            .collect();
                        format!("/{}", segments.join("/"))
                    });
                    let info = match maker.parse(&format!("{path}/_index.md")) {
                        Some(info) if info.key == cur => info,
                        _ => match maker.parse(&format!("/{}/_index.md", cur.as_str())) {
                            Some(info) => info,
                            None => continue,
                        },
                    };
                    maker.make(m, lang, PageKind::Term, cur, info)?;
                }
            }
        }
    }

    // The tables, with the term pages in key order.
    let mut taxonomies: IdVec<TaxonomyIdx, Taxonomy> = IdVec::new();
    for (idx, def) in site.taxonomies.iter_enumerated() {
        let plural = ContentKey::from_source(&def.plural);
        let page = m.sites[lang].tree.get(&plural);
        if let Some(p) = page {
            m.pages[p].taxonomy = Some(idx);
        }
        let mut terms: IdVec<TermIdx, Term> = IdVec::new();
        let below: Vec<(ContentKey, PageId)> = m.sites[lang]
            .tree
            .descendants(&plural)
            .filter(|_| !plural.is_home())
            .map(|(k, id)| (k.clone(), id))
            .collect();
        for (key, id) in below {
            if m.pages[id].kind != PageKind::Term {
                continue;
            }
            let term = last_value.get(&id).cloned().unwrap_or_else(|| {
                let base = &m.pages[id].path_info.original.base;
                let rel = base
                    .strip_prefix(&format!("/{}", def.plural))
                    .unwrap_or(base)
                    .trim_start_matches('/');
                if def.hierarchical {
                    last_segment(rel).to_owned()
                } else {
                    rel.to_owned()
                }
            });
            let members = entries
                .range((id, PageId::from_raw(0))..=(id, PageId::from_raw(u32::MAX)))
                .map(|(&(_, page), &(weight, ordinal))| WeightedPage {
                    page,
                    weight,
                    ordinal,
                })
                .collect();
            let tidx = terms.push(Term {
                key,
                term,
                page: id,
                members,
                parent: None,
                children: Vec::new(),
            });
            let p = &mut m.pages[id];
            p.taxonomy = Some(idx);
            p.term = Some(tidx);
        }
        taxonomies.push(Taxonomy {
            def: def.clone(),
            page,
            terms,
        });
    }

    // `.GetTerms` of every member: the terms the page names.
    let mut by_member: BTreeMap<PageId, Vec<(TaxonomyIdx, u32, TermIdx)>> = BTreeMap::new();
    for (idx, t) in taxonomies.iter_enumerated() {
        for (tidx, term) in t.terms.iter_enumerated() {
            for w in &term.members {
                by_member
                    .entry(w.page)
                    .or_default()
                    .push((idx, w.ordinal, tidx));
            }
        }
    }
    for (page, mut terms) in by_member {
        terms.sort_unstable();
        m.pages[page].terms = terms.into_iter().map(|(t, _, term)| (t, term)).collect();
    }
    for t in taxonomies.iter_mut().filter(|t| t.def.hierarchical) {
        nest(t);
    }
    m.sites[lang].taxonomies = taxonomies;
    Ok(())
}

/// Links the terms of a hierarchical taxonomy to the nearest term above them and gives every
/// term the pages of the terms below it (with the smallest weight and ordinal).
fn nest(t: &mut Taxonomy) {
    let by_key: BTreeMap<ContentKey, TermIdx> = t
        .terms
        .iter_enumerated()
        .map(|(i, term)| (term.key.clone(), i))
        .collect();
    let ids: Vec<TermIdx> = t.terms.ids().collect();
    for &i in &ids {
        let mut up = t.terms[i].key.parent();
        while let Some(cur) = up {
            if let Some(&p) = by_key.get(&cur) {
                t.terms[i].parent = Some(p);
                t.terms[p].children.push(i);
                break;
            }
            up = cur.parent();
        }
    }
    let mut deepest_first = ids;
    deepest_first.sort_by_key(|&i| std::cmp::Reverse(t.terms[i].key.segments().count()));
    let mut pages: IdVec<TermIdx, BTreeMap<PageId, (i32, u32)>> = t
        .terms
        .iter()
        .map(|term| {
            term.members
                .iter()
                .map(|w| (w.page, (w.weight, w.ordinal)))
                .collect()
        })
        .collect();
    for i in deepest_first {
        let Some(p) = t.terms[i].parent else {
            continue;
        };
        let below = std::mem::take(&mut pages[i]);
        for (&page, &wo) in &below {
            pages[p]
                .entry(page)
                .and_modify(|cur| *cur = (*cur).min(wo))
                .or_insert(wo);
        }
        pages[i] = below;
    }
    for (term, pages) in t.terms.iter_mut().zip(pages) {
        term.members = pages
            .into_iter()
            .map(|(page, (weight, ordinal))| WeightedPage {
                page,
                weight,
                ordinal,
            })
            .collect();
    }
}

/// Sorts every term's members: by weight, then in the default order.
pub(crate) fn sort_members(m: &mut Model) {
    let collators = Collators::new(&m.config);
    for lang in m.config.sites.ids() {
        let mut taxonomies = std::mem::take(&mut m.sites[lang].taxonomies);
        for t in taxonomies.iter_mut() {
            for term in t.terms.iter_mut() {
                term.members.sort_by(|a, b| {
                    a.weight.cmp(&b.weight).then_with(|| {
                        relations::default_order(m, &collators[lang], a.page, None, b.page, None)
                    })
                });
            }
        }
        m.sites[lang].taxonomies = taxonomies;
    }
}
