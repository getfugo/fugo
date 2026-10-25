//! Related content: an inverted index from keywords to pages over the `[related]` indices of
//! the configuration, built for an explicit candidate list (`site.pages` and
//! `site.regular_pages` give different indices), and searched with a page, fragments or named
//! keyword lists.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use ssg_base::{PageId, Value};
use ssg_config::sections::{RelatedConfig, RelatedIndexKind};

use crate::NavError;
use crate::model::NavModel;

mod keywords;

use keywords::*;

/// A search: what to find related pages for.
#[derive(Clone, Debug, Default)]
pub struct RelatedQuery<'a> {
    /// The page whose keywords are searched (it is never in the result, and pages published
    /// after it are left out unless `includeNewer` is set).
    pub document: Option<PageId>,
    /// The indices to search (all configured indices when empty).
    pub indices: &'a [String],
    /// Heading ids searched in the fragments indices.
    pub fragments: &'a [String],
    /// Extra keyword lists: index name → values.
    pub named: &'a [(String, Vec<Value>)],
}

/// The result of a search.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Related {
    /// Best match first: summed index weight, then newest publish date, then name.
    pub pages: Vec<PageId>,
    /// The matched heading ids of fragments indices with `applyFilter`, sorted: the pages'
    /// headings are filtered to these (`None`: no filter).
    pub heading_filter: Option<Vec<String>>,
}

/// A candidate of an index, with what the order of results reads about it (kept when it is
/// added, so that a search reads no page).
#[derive(Clone, Debug)]
struct Candidate {
    page: PageId,
    /// The publish date (Unix seconds and nanoseconds).
    published: (i64, i32),
    name: String,
}

/// An inverted index over a candidate list.
#[derive(Clone, Debug)]
pub struct RelatedIndex {
    cfg: RelatedConfig,
    /// Per configured index: keyword → candidates (slots, in candidate order).
    postings: Vec<BTreeMap<Keyword, Vec<usize>>>,
    /// Per configured index: how many pages have a keyword in it.
    doc_counts: Vec<usize>,
    /// The candidates by slot, in the order they were first added (the last tie-break of
    /// equal matches).
    candidates: Vec<Candidate>,
    slots: HashMap<PageId, usize>,
    min_weight: i32,
    max_weight: i32,
}

impl RelatedIndex {
    /// An empty index for `cfg`.
    #[must_use]
    pub fn new(cfg: &RelatedConfig) -> Self {
        let min_weight = cfg
            .indices
            .iter()
            .map(|i| i.weight)
            .min()
            .unwrap_or(0)
            .min(0);
        let max_weight = cfg
            .indices
            .iter()
            .map(|i| i.weight)
            .max()
            .unwrap_or(0)
            .max(0);
        Self {
            postings: vec![BTreeMap::new(); cfg.indices.len()],
            doc_counts: vec![0; cfg.indices.len()],
            candidates: Vec::new(),
            slots: HashMap::new(),
            min_weight,
            max_weight,
            cfg: cfg.clone(),
        }
    }

    /// Builds the index over `candidates` (in their order).
    ///
    /// # Errors
    /// The first candidate with a value that cannot be a keyword (see [`add`](Self::add)).
    pub fn build(
        m: &impl NavModel,
        cfg: &RelatedConfig,
        candidates: &[PageId],
    ) -> Result<Self, NavError> {
        let mut idx = Self::new(cfg);
        for &p in candidates {
            idx.add(m, p)?;
        }
        idx.finalize();
        Ok(idx)
    }

    /// Adds a candidate to every enabled index (weight ≠ 0). An index whose value cannot be a
    /// keyword (a boolean or number) is skipped for this page; the first such error is
    /// returned after the other indices got the page.
    ///
    /// # Errors
    /// A page value that cannot be a keyword.
    pub fn add(&mut self, m: &impl NavModel, page: PageId) -> Result<(), NavError> {
        let p = m.page(page);
        let slot = *self.slots.entry(page).or_insert_with(|| {
            self.candidates.push(Candidate {
                page,
                published: instant(p.dates.publish_date.as_ref()),
                name: p.name.to_owned(),
            });
            self.candidates.len() - 1
        });
        let mut first_err = None;
        for (i, cfg) in self.cfg.indices.iter().enumerate() {
            if cfg.weight == 0 {
                continue;
            }
            let mut words = match page_keywords(cfg, &p) {
                Ok(w) => w,
                Err(e) => {
                    first_err.get_or_insert(e);
                    continue;
                }
            };
            if cfg.kind == RelatedIndexKind::Fragments {
                words.extend(p.fragments.iter().cloned().map(Keyword::Fragment));
            }
            if words.is_empty() {
                continue;
            }
            self.doc_counts[i] += 1;
            for w in words {
                self.postings[i].entry(w).or_default().push(slot);
            }
        }
        first_err.map_or(Ok(()), Err)
    }

    /// Drops the keywords that more than `cardinalityThreshold` percent of an index's pages
    /// have.
    pub fn finalize(&mut self) {
        for (i, cfg) in self.cfg.indices.iter().enumerate() {
            let docs = self.doc_counts[i];
            let Ok(limit) = usize::try_from(cfg.cardinality_threshold) else {
                continue;
            };
            if limit == 0 || docs == 0 {
                continue;
            }
            self.postings[i].retain(|_, slots| (slots.len() * 100).div_ceil(docs) <= limit);
        }
    }

    fn index_of(&self, name: &str) -> Option<usize> {
        self.cfg.indices.iter().position(|i| i.name == name)
    }

    /// The pages related to `q`, best first.
    ///
    /// # Errors
    /// An unknown or empty index name, or a document or named value that cannot be a keyword.
    pub fn search(&self, m: &impl NavModel, q: &RelatedQuery<'_>) -> Result<Related, NavError> {
        let doc = q.document.map(|d| m.page(d));
        let selected: Vec<usize> = if q.indices.is_empty() {
            (0..self.cfg.indices.len()).collect()
        } else {
            q.indices
                .iter()
                .map(|n| {
                    self.index_of(n)
                        .ok_or_else(|| NavError::UnknownIndex(n.clone()))
                })
                .collect::<Result<_, _>>()?
        };
        let mut query: Vec<(usize, Vec<Keyword>)> = Vec::new();
        for i in selected {
            let cfg = &self.cfg.indices[i];
            let mut words = Vec::new();
            if let Some(d) = &doc {
                words.extend(page_keywords(cfg, d)?);
            }
            if cfg.kind == RelatedIndexKind::Fragments {
                words.extend(q.fragments.iter().cloned().map(Keyword::Fragment));
                if let Some(d) = &doc {
                    words.extend(d.fragments.iter().cloned().map(Keyword::Fragment));
                }
            }
            query.push((i, words));
        }
        for (name, values) in q.named {
            if name.is_empty() {
                return Err(NavError::InvalidIndexName);
            }
            let i = self
                .index_of(name)
                .ok_or_else(|| NavError::UnknownIndex(name.clone()))?;
            let mut words = Vec::new();
            for v in values {
                words.extend(value_keywords(&self.cfg.indices[i], v)?);
            }
            query.push((i, words));
        }

        let upper = doc
            .as_ref()
            .and_then(|d| d.dates.publish_date.as_ref())
            .filter(|_| !self.cfg.include_newer)
            .map(|d| instant(Some(d)));
        let own = q.document.and_then(|d| self.slots.get(&d).copied());
        // Matches in first-match order: (slot, summed weight, matches); `at` holds each
        // matched slot's position in `ranks`.
        let mut ranks: Vec<(usize, i64, i64)> = Vec::new();
        let mut at = vec![usize::MAX; self.candidates.len()];
        let mut filter: BTreeSet<String> = BTreeSet::new();
        for (i, words) in &query {
            let cfg = &self.cfg.indices[*i];
            for w in words {
                let Some(slots) = self.postings[*i].get(w) else {
                    continue;
                };
                for &p in slots {
                    if Some(p) == own {
                        continue;
                    }
                    if let Some(upper) = upper
                        && self.candidates[p].published > upper
                    {
                        continue;
                    }
                    if cfg.kind == RelatedIndexKind::Fragments
                        && cfg.apply_filter
                        && let Keyword::Fragment(f) = w
                    {
                        filter.insert(f.clone());
                    }
                    let weight = i64::from(cfg.weight);
                    match at[p] {
                        usize::MAX => {
                            at[p] = ranks.len();
                            ranks.push((p, weight, 1));
                        }
                        k => {
                            ranks[k].1 += weight;
                            ranks[k].2 += 1;
                        }
                    }
                }
            }
        }

        let threshold = i64::from(self.cfg.threshold);
        let (min, max) = (i64::from(self.min_weight), i64::from(self.max_weight));
        let mut hits: Vec<(usize, i64)> = ranks
            .into_iter()
            .filter(|&(_, weight, matches)| norm(weight / matches, min, max) >= threshold / matches)
            .map(|(p, weight, _)| (p, weight))
            .collect();
        hits.sort_by(|a, b| {
            b.1.cmp(&a.1).then_with(|| {
                let (ca, cb) = (&self.candidates[a.0], &self.candidates[b.0]);
                cb.published
                    .cmp(&ca.published)
                    .then_with(|| ca.name.cmp(&cb.name))
                    .then_with(|| a.0.cmp(&b.0))
            })
        });
        Ok(Related {
            pages: hits
                .into_iter()
                .map(|(p, _)| self.candidates[p].page)
                .collect(),
            heading_filter: (!filter.is_empty()).then(|| filter.into_iter().collect()),
        })
    }
}

/// `num` on a 0–100 scale between `min` and `max`, rounded half up.
fn norm(num: i64, min: i64, max: i64) -> i64 {
    let span = max - min;
    if span == 0 {
        return 0;
    }
    ((num - min) * 200 + span).div_euclid(2 * span)
}

/// The pages of `candidates` related to page `page` with the site's configuration
/// (`.Site.RegularPages.Related .`): builds the index and searches it.
///
/// # Errors
/// As [`RelatedIndex::build`] and [`RelatedIndex::search`].
pub fn related(
    m: &impl NavModel,
    cfg: &RelatedConfig,
    candidates: &[PageId],
    page: PageId,
) -> Result<Related, NavError> {
    if candidates.is_empty() {
        return Ok(Related::default());
    }
    RelatedIndex::build(m, cfg, candidates)?.search(
        m,
        &RelatedQuery {
            document: Some(page),
            ..RelatedQuery::default()
        },
    )
}
