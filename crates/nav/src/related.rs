//! Related content: an inverted index from keywords to pages over the `[related]` indices of
//! the configuration, built for an explicit candidate list (`site.pages` and
//! `site.regular_pages` give different indices), and searched with a page, fragments or named
//! keyword lists.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use jiff::Zoned;
use ssg_base::{PageId, Value, text};
use ssg_config::sections::{RelatedConfig, RelatedIndex as IndexConfig, RelatedIndexKind};

use crate::NavError;
use crate::model::{NavModel, PageFacts};

/// A keyword of an index; heading ids (fragments) never match plain keywords.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Keyword {
    Text(String),
    Fragment(String),
}

/// The page value an index reads, by the index's name.
enum MetaValue<'a> {
    None,
    Text(&'a str),
    List(&'a [String]),
    Date(Option<&'a Zoned>),
    Value(&'a Value),
    /// A value that cannot be a keyword (a boolean or a number field).
    Unsupported(&'static str),
}

fn meta_value<'a>(p: &PageFacts<'a>, name: &str) -> MetaValue<'a> {
    match name {
        "keywords" => MetaValue::List(p.keywords),
        "aliases" => MetaValue::List(p.aliases),
        "kind" => MetaValue::Text(p.kind.as_str()),
        "bundletype" => MetaValue::Text(p.bundle_type),
        "section" => MetaValue::Text(p.section),
        "lang" => MetaValue::Text(p.lang_key),
        "name" => MetaValue::Text(p.name),
        "description" => MetaValue::Text(p.description),
        "title" => MetaValue::Text(p.title),
        "linktitle" => MetaValue::Text(p.link_title),
        "slug" => MetaValue::Text(p.slug),
        "type" => MetaValue::Text(p.page_type),
        "layout" => MetaValue::Text(p.layout),
        "date" => MetaValue::Date(p.dates.date.as_ref()),
        "publishdate" => MetaValue::Date(p.dates.publish_date.as_ref()),
        "expirydate" => MetaValue::Date(p.dates.expiry_date.as_ref()),
        "lastmod" => MetaValue::Date(p.dates.lastmod.as_ref()),
        "draft" => MetaValue::Unsupported("bool"),
        "weight" => MetaValue::Unsupported("int"),
        "mediatype" => MetaValue::Unsupported("media type"),
        _ => p
            .params
            .get(name)
            .or_else(|| p.params.get_path(name))
            .map_or(MetaValue::None, MetaValue::Value),
    }
}

/// A date keyword: the date in the index's Go layout (`2006` by default); a page without the
/// date has Go's zero date (year 1).
fn date_keyword(d: Option<&Zoned>, pattern: &str) -> String {
    let layout = if pattern.is_empty() { "2006" } else { pattern };
    let zero;
    let d = if let Some(d) = d {
        d
    } else {
        zero = jiff::civil::date(1, 1, 1)
            .at(0, 0, 0, 0)
            .to_zoned(jiff::tz::TimeZone::UTC)
            .expect("year 1 is in range");
        &zero
    };
    ssg_page::format_go_layout(d, layout)
}

fn text_keyword(cfg: &IndexConfig, s: &str) -> Keyword {
    let s = if cfg.to_lower {
        text::to_lower(s)
    } else {
        s.to_owned()
    };
    match cfg.kind {
        RelatedIndexKind::Basic => Keyword::Text(s),
        RelatedIndexKind::Fragments => Keyword::Fragment(s),
    }
}

/// A scalar as a list element keyword (Go `cast.ToString`: nested lists and maps are empty).
fn scalar_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

fn value_keywords(cfg: &IndexConfig, v: &Value) -> Result<Vec<Keyword>, NavError> {
    let unsupported = |kind: &'static str| NavError::UnsupportedKeyword {
        index: cfg.name.clone(),
        kind,
    };
    Ok(match v {
        Value::Null => Vec::new(),
        Value::String(s) => vec![text_keyword(cfg, s)],
        Value::Array(items) => items
            .iter()
            .map(|i| text_keyword(cfg, &scalar_string(i)))
            .collect(),
        Value::Date(d) => {
            let z = match d {
                ssg_base::Date::Zoned(z) => z.clone(),
                ssg_base::Date::Local(dt) => dt
                    .to_zoned(jiff::tz::TimeZone::UTC)
                    .map_err(|_| unsupported("date"))?,
            };
            vec![Keyword::Text(date_keyword(Some(&z), &cfg.pattern))]
        }
        Value::Bool(_) => return Err(unsupported("bool")),
        Value::Int(_) => return Err(unsupported("int")),
        Value::Float(_) => return Err(unsupported("float")),
        Value::Map(_) => return Err(unsupported("map")),
    })
}

/// The keywords page `p` has in index `cfg`.
fn page_keywords(cfg: &IndexConfig, p: &PageFacts<'_>) -> Result<Vec<Keyword>, NavError> {
    match meta_value(p, &cfg.name) {
        MetaValue::None => Ok(Vec::new()),
        MetaValue::Text(s) => Ok(vec![text_keyword(cfg, s)]),
        MetaValue::List(l) => Ok(l.iter().map(|s| text_keyword(cfg, s)).collect()),
        MetaValue::Date(d) => Ok(vec![Keyword::Text(date_keyword(d, &cfg.pattern))]),
        MetaValue::Value(v) => value_keywords(cfg, v),
        MetaValue::Unsupported(kind) => Err(NavError::UnsupportedKeyword {
            index: cfg.name.clone(),
            kind,
        }),
    }
}

/// Unix seconds and nanoseconds of a publish date (Go's zero date when absent).
fn instant(d: Option<&Zoned>) -> (i64, i32) {
    const ZERO: i64 = -62_135_596_800;
    d.map_or((ZERO, 0), |z| {
        let t = z.timestamp();
        (t.as_second(), t.subsec_nanosecond())
    })
}

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

/// An inverted index over a candidate list.
#[derive(Clone, Debug)]
pub struct RelatedIndex {
    cfg: RelatedConfig,
    /// Per configured index: keyword → pages (candidate order).
    postings: Vec<BTreeMap<Keyword, Vec<PageId>>>,
    /// Per configured index: how many pages have a keyword in it.
    doc_counts: Vec<usize>,
    /// Position of each candidate (the tie-break of equal matches).
    rank: HashMap<PageId, usize>,
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
            rank: HashMap::new(),
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
        let n = self.rank.len();
        self.rank.entry(page).or_insert(n);
        let p = m.page(page);
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
                self.postings[i].entry(w).or_default().push(page);
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
            self.postings[i].retain(|_, pages| (pages.len() * 100).div_ceil(docs) <= limit);
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
        // Matches in first-match order: page → (summed weight, matches).
        let mut ranks: Vec<(PageId, i64, i64)> = Vec::new();
        let mut at: HashMap<PageId, usize> = HashMap::new();
        let mut filter: BTreeSet<String> = BTreeSet::new();
        for (i, words) in &query {
            let cfg = &self.cfg.indices[*i];
            for w in words {
                let Some(pages) = self.postings[*i].get(w) else {
                    continue;
                };
                for &p in pages {
                    if Some(p) == q.document {
                        continue;
                    }
                    if let Some(upper) = upper
                        && instant(m.page(p).dates.publish_date.as_ref()) > upper
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
                    match at.get(&p) {
                        Some(&k) => {
                            ranks[k].1 += weight;
                            ranks[k].2 += 1;
                        }
                        None => {
                            at.insert(p, ranks.len());
                            ranks.push((p, weight, 1));
                        }
                    }
                }
            }
        }

        let threshold = i64::from(self.cfg.threshold);
        let (min, max) = (i64::from(self.min_weight), i64::from(self.max_weight));
        let mut hits: Vec<(PageId, i64)> = ranks
            .into_iter()
            .filter(|&(_, weight, matches)| norm(weight / matches, min, max) >= threshold / matches)
            .map(|(p, weight, _)| (p, weight))
            .collect();
        hits.sort_by(|a, b| {
            b.1.cmp(&a.1).then_with(|| {
                let (pa, pb) = (m.page(a.0), m.page(b.0));
                instant(pb.dates.publish_date.as_ref())
                    .cmp(&instant(pa.dates.publish_date.as_ref()))
                    .then_with(|| pa.name.cmp(pb.name))
                    .then_with(|| self.order(a.0).cmp(&self.order(b.0)))
            })
        });
        Ok(Related {
            pages: hits.into_iter().map(|(p, _)| p).collect(),
            heading_filter: (!filter.is_empty()).then(|| filter.into_iter().collect()),
        })
    }

    fn order(&self, p: PageId) -> usize {
        self.rank.get(&p).copied().unwrap_or(usize::MAX)
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
