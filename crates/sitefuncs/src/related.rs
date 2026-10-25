//! `related`: the pages of a list related to a page or to keyword lists, through one
//! `[related]` index per (language, candidate list) ([`RelatedCache`]). Fragments indices read
//! the pages' heading ids from the render session.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, Weak};

use ssg_base::{LangIdx, PageId};
use ssg_config::sections::{RelatedConfig, RelatedIndexKind};
use ssg_markup::Fragments;
use ssg_nav::{RelatedIndex, RelatedQuery, WithFragments};
use ssg_view::views::FragmentsView;
use ssg_view::{ContentError, ContentRenderer, Phase, RenderScope, ViewCache};
use tera::value::Key;
use tera::{Kwargs, State, TeraResult, Value};

use crate::Handles;
use crate::call::{
    Registrar, SiteFunction, chain, generation, list, msg, need_scope, page_ids, page_scope,
    renderer, scope, text, to_data,
};

pub(crate) fn register(r: &mut Registrar<'_>, h: &Handles) {
    r.function(
        "related",
        Related {
            views: Arc::clone(&h.views),
            cache: Arc::clone(&h.related),
            renderer: Arc::clone(&h.renderer),
        },
    );
}

/// How many list values [`RelatedCache`] knows by their allocation.
const RECENT: usize = 32;

/// Where an index is used: its language and phase. A shortcode's search can build an index
/// while a page expands, when that page's headings come without its shortcodes; layouts build
/// their own, from all the headings.
type Place = (LangIdx, Phase);

/// A related-content index's key: its place and the candidate list.
type RelatedKey = (Place, Vec<PageId>);

/// A list value searched lately: holding the value keeps its allocation alive, so that the
/// allocation names the list.
#[derive(Debug)]
struct Recent {
    place: Place,
    list: Value,
    index: Arc<RelatedIndex>,
}

/// Related-content indices, one per (language, phase, candidate list):
/// `related(pages=site.pages)` and `related(pages=site.regular_pages)` search different indices
/// (REWRITE_PLAN.md §4.6).
///
/// A layout passes the same list value (`site.regular_pages`) on every page: the lists searched
/// last are found by their allocation, without reading their pages again.
#[derive(Debug, Default)]
pub struct RelatedCache {
    indices: Mutex<HashMap<RelatedKey, Arc<RelatedIndex>>>,
    recent: Mutex<VecDeque<Recent>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Whether `r` is list value `list` (the same allocation) searched at `place`.
fn is_list(r: &Recent, place: Place, list: &[Value]) -> bool {
    r.place == place && r.list.as_array().is_some_and(|l| std::ptr::eq(l, list))
}

impl RelatedCache {
    /// The index used at `place` over the pages of list value `list`: `candidates` reads
    /// them, and `build` builds the index from them on first use.
    fn index(
        &self,
        place: Place,
        list: &Value,
        candidates: impl FnOnce() -> TeraResult<Vec<PageId>>,
        build: impl FnOnce(&[PageId]) -> TeraResult<RelatedIndex>,
    ) -> TeraResult<Arc<RelatedIndex>> {
        let items = list.as_array().unwrap_or_default();
        if let Some(r) = lock(&self.recent).iter().find(|r| is_list(r, place, items)) {
            return Ok(Arc::clone(&r.index));
        }
        let key = (place, candidates()?);
        let found = lock(&self.indices).get(&key).cloned();
        let index = match found {
            Some(i) => i,
            None => {
                let built = Arc::new(build(&key.1)?);
                Arc::clone(lock(&self.indices).entry(key).or_insert(built))
            }
        };
        let mut recent = lock(&self.recent);
        if !recent.iter().any(|r| is_list(r, place, items)) {
            if recent.len() == RECENT {
                recent.pop_front();
            }
            recent.push_back(Recent {
                place,
                list: list.clone(),
                index: Arc::clone(&index),
            });
        }
        Ok(index)
    }
}

/// Whether some index of `cfg` matches heading ids.
fn has_fragments(cfg: &RelatedConfig) -> bool {
    cfg.indices
        .iter()
        .any(|i| i.kind == RelatedIndexKind::Fragments && i.weight != 0)
}

/// The headings of page `p` (none for a page without content).
fn fragments(
    r: &dyn ContentRenderer,
    views: &ViewCache,
    p: PageId,
    s: &RenderScope,
) -> TeraResult<Arc<Fragments>> {
    match r.fragments(p, s) {
        Ok(f) => Ok(f),
        Err(ContentError::NoContent(_)) => Ok(Arc::default()),
        Err(e) => Err(chain(
            format!("related: the headings of {}", views.model().pages[p].path()),
            e,
        )),
    }
}

/// The strings of list kwarg `key` (`[]` when absent or none).
fn strings(kw: &Kwargs, key: &str) -> TeraResult<Vec<String>> {
    let what = format!("related({key}=)");
    match kw.get::<Value>(key)? {
        Some(v) if !v.is_none() => list(&v, &what)?.iter().map(|i| text(i, &what)).collect(),
        _ => Ok(Vec::new()),
    }
}

/// `keywords=`: index name → a value or a list of values (read as a front matter value).
fn keywords(kw: &Kwargs) -> TeraResult<Vec<(String, Vec<ssg_base::Value>)>> {
    let Some(v) = kw.get::<Value>("keywords")?.filter(|v| !v.is_none()) else {
        return Ok(Vec::new());
    };
    let m = v.as_map().ok_or_else(|| {
        msg(format!(
            "related(keywords=): expected a map of index names to keywords, got {}",
            v.name()
        ))
    })?;
    Ok(m.iter()
        .map(|(k, v)| {
            let name = k.as_str().unwrap_or_default().to_lowercase();
            (name, vec![to_data(v)])
        })
        .collect())
}

/// Search result `v` (a summary) with `fragments` in place of its own.
fn with_fragments(v: &Value, f: &FragmentsView) -> Value {
    let mut m = v.as_map().cloned().unwrap_or_default();
    m.insert(Key::Str("fragments"), Value::from_serializable(f));
    Value::from(m)
}

/// `related(pages=, page=?, indices=?, fragments=?, keywords=?, limit=?)`: the pages of `pages`
/// related to `page` (`page=none`: to `fragments` and `keywords` alone), best first, by the
/// `[related]` configuration of the page's language. When a fragments index with
/// `applyFilter` matched, each result's `fragments` holds only the matched headings.
struct Related {
    views: Arc<ViewCache>,
    cache: Arc<RelatedCache>,
    renderer: Arc<OnceLock<Weak<dyn ContentRenderer>>>,
}

impl SiteFunction for Related {
    fn call(&self, kw: &Kwargs, st: &State) -> TeraResult<Value> {
        let (s, document) = match kw.get::<Value>("page")? {
            Some(p) if p.is_none() => (need_scope(st, "related")?, None),
            _ => {
                let s = page_scope(&self.views, st, kw, "related")?;
                let page = s.page;
                (s, Some(page))
            }
        };
        // The caller's scope travels with the requests for headings (its chain detects cycles).
        let req = scope(st)?.unwrap_or_else(|| s.clone());
        let g = generation(&self.views, Some(&s));
        let pages = kw.must_get::<Value>("pages")?;
        if pages.is_none() || pages.is_undefined() || list(&pages, "related(pages=)")?.is_empty() {
            return Ok(g.list(&[]));
        }
        let indices: Vec<String> = strings(kw, "indices")?
            .iter()
            .map(|i| i.to_lowercase())
            .collect();
        let headings = strings(kw, "fragments")?;
        let named = keywords(kw)?;
        let cfg = &self.views.model().config.sites[s.lang].related;
        let session = || renderer(&self.renderer, "related");
        let index = self.cache.index(
            (s.lang, req.phase),
            &pages,
            || page_ids(&self.views, &pages, "related(pages=)"),
            |candidates| {
                let mut found = Vec::new();
                if has_fragments(cfg) {
                    let r = session()?;
                    for &p in candidates {
                        found.push((p, fragments(r.as_ref(), &self.views, p, &req)?));
                    }
                }
                let m = WithFragments::new(
                    self.views.nav(),
                    found.iter().map(|(p, f)| (*p, f.identifiers.as_slice())),
                );
                RelatedIndex::build(&m, cfg, candidates).map_err(|e| chain("related", e))
            },
        )?;
        let own = match document {
            Some(d) if has_fragments(cfg) => {
                Some((d, fragments(session()?.as_ref(), &self.views, d, &req)?))
            }
            _ => None,
        };
        let m = WithFragments::new(
            self.views.nav(),
            own.iter().map(|(p, f)| (*p, f.identifiers.as_slice())),
        );
        let found = index
            .search(
                &m,
                &RelatedQuery {
                    document,
                    indices: &indices,
                    fragments: &headings,
                    named: &named,
                },
            )
            .map_err(|e| chain("related", e))?;
        let mut pages = found.pages;
        if let Some(limit) = kw.get::<i64>("limit")? {
            pages.truncate(usize::try_from(limit).unwrap_or(0));
        }
        let Some(matched) = found.heading_filter else {
            return Ok(g.list(&pages));
        };
        let keep = |id: &str| matched.binary_search_by(|m| m.as_str().cmp(id)).is_ok();
        let r = session()?;
        let values = pages
            .iter()
            .map(|&p| {
                let f = fragments(r.as_ref(), &self.views, p, &req)?;
                Ok(with_fragments(
                    &g.summaries[p],
                    &FragmentsView::filtered(&f, keep),
                ))
            })
            .collect::<TeraResult<Vec<_>>>()?;
        Ok(Value::from(values))
    }
}
