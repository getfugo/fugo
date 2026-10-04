//! The terms of each taxonomy: assembled from the pages' front matter, nested for hierarchical
//! taxonomies, and their members sorted.

use super::*;

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
pub(super) fn nest(t: &mut Taxonomy) {
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
