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

mod terms;
pub(crate) use terms::*;

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
