//! `cascade`: front matter a page hands down to itself and its descendants, filtered by
//! kind, path, language and environment globs.

use ssg_base::glob::{self, Glob, GlobOpts};
use ssg_base::{PageKind, Params, Value};
use ssg_config::sections::CascadeConfig;

use crate::PageError;
use crate::value;

/// A glob with its source, compared by source.
#[derive(Clone, Debug)]
struct Pattern {
    source: String,
    glob: Glob,
}

impl PartialEq for Pattern {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
    }
}

impl Eq for Pattern {}

/// Which pages a cascade entry applies to. Every set glob must match (Go's globs: case
/// folded, `/` separated); an empty target matches every page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CascadeTarget {
    kind: Option<Pattern>,
    path: Option<Pattern>,
    lang: Option<Pattern>,
    environment: Option<Pattern>,
}

/// The page a cascade target is matched against.
#[derive(Clone, Copy, Debug)]
pub struct MatchCtx<'a> {
    pub kind: PageKind,
    /// `.Path` (`/blog/post`; `/` for the home page).
    pub path: &'a str,
    /// The language key (`en`).
    pub lang: &'a str,
    /// The build environment (`production`).
    pub environment: &'a str,
}

/// The kinds a cascade `kind` glob must match at least one of.
const CASCADE_KINDS: [PageKind; 5] = [
    PageKind::Page,
    PageKind::Home,
    PageKind::Section,
    PageKind::Taxonomy,
    PageKind::Term,
];

impl CascadeTarget {
    /// A target from its globs (empty strings are unset). The path glob is lower-cased.
    ///
    /// # Errors
    /// A glob that does not compile, or a kind glob that matches no content kind.
    pub fn new(kind: &str, path: &str, lang: &str, environment: &str) -> Result<Self, PageError> {
        let compile = |field: &'static str, s: &str| -> Result<Option<Pattern>, PageError> {
            if s.is_empty() {
                return Ok(None);
            }
            let glob = glob::compile(s, GlobOpts::default())
                .map_err(|source| PageError::CascadeGlob { field, source })?;
            Ok(Some(Pattern {
                source: s.to_owned(),
                glob,
            }))
        };
        let target = Self {
            kind: compile("kind", kind)?,
            path: compile("path", &path.to_lowercase())?,
            lang: compile("lang", lang)?,
            environment: compile("environment", environment)?,
        };
        if let Some(k) = &target.kind
            && !CASCADE_KINDS.iter().any(|c| k.glob.is_match(c.as_str()))
        {
            return Err(PageError::CascadeKind(kind.to_owned()));
        }
        Ok(target)
    }

    /// Whether the target applies to the page.
    #[must_use]
    pub fn matches(&self, m: &MatchCtx<'_>) -> bool {
        let ok = |p: &Option<Pattern>, s: &str| p.as_ref().is_none_or(|p| p.glob.is_match(s));
        let path = if m.path.starts_with('/') {
            m.path.to_owned()
        } else {
            format!("/{}", m.path)
        };
        ok(&self.kind, m.kind.as_str())
            && ok(&self.path, &path)
            && ok(&self.lang, m.lang)
            && ok(&self.environment, m.environment)
    }

    /// Whether the path glob ends in a file extension (`/blog/*.md`): paths have no
    /// extensions, so it matches nothing; worth a warning.
    #[must_use]
    pub fn path_looks_like_file(&self) -> bool {
        self.path
            .as_ref()
            .is_some_and(|p| !ssg_base::paths::ext(&p.source).is_empty())
    }

    /// The globs as written: kind, path (lower case), lang, environment (empty when unset).
    #[must_use]
    pub fn sources(&self) -> [&str; 4] {
        fn s(p: &Option<Pattern>) -> &str {
            p.as_ref().map_or("", |p| p.source.as_str())
        }
        [
            s(&self.kind),
            s(&self.path),
            s(&self.lang),
            s(&self.environment),
        ]
    }
}

/// One cascade entry: front matter fields and params for the pages its target matches.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CascadeRule {
    pub target: CascadeTarget,
    /// Front matter fields (`title`, `build`, `outputs`, …).
    pub fields: Params,
    /// `params`.
    pub params: Params,
}

impl CascadeRule {
    /// A site-level `[[cascade]]` entry.
    ///
    /// # Errors
    /// See [`CascadeTarget::new`].
    pub fn from_config(c: &CascadeConfig) -> Result<Self, PageError> {
        let t = &c.target;
        Ok(Self {
            target: CascadeTarget::new(&t.kind, &t.path, &t.lang, &t.environment)?,
            fields: c.fields.clone(),
            params: c.params.clone(),
        })
    }
}

/// Keys that choose a page's identity; they cannot be cascaded.
const DISALLOWED: [&str; 3] = ["kind", "path", "lang"];

/// The cascade entries in force for a page, strongest first. Entries with the same target
/// are merged (the stronger entry's keys win).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cascade(Vec<CascadeRule>);

impl Cascade {
    /// Decodes a front matter or configuration `cascade` value: a table or a list of tables.
    /// Keys ignore case; `target` (or the legacy `_target`) holds the globs, `params` the
    /// params, everything else is a front matter field.
    ///
    /// # Errors
    /// A value that is not a table or a list of tables, a target that is not a table, a
    /// disallowed key (`kind`, `path`, `lang`), or a bad glob.
    pub fn decode(v: &Value) -> Result<Self, PageError> {
        let items: &[Value] = match v {
            Value::Null => &[],
            Value::Map(_) => std::slice::from_ref(v),
            Value::Array(a) => a,
            other => {
                return Err(PageError::Cascade(format!(
                    "expected a table or a list of tables, found {}",
                    value::kind(other)
                )));
            }
        };
        let mut out = Self::default();
        for item in items {
            let Some(m) = item.as_map() else {
                return Err(PageError::Cascade(format!(
                    "expected a table, found {}",
                    value::kind(item)
                )));
            };
            let m = Params::fold(m);
            let mut rule = CascadeRule::default();
            for (k, v) in m.iter() {
                match k {
                    "target" | "_target" => rule.target = decode_target(v)?,
                    "params" => rule.params = v.as_map().map(Params::fold).unwrap_or_default(),
                    k if DISALLOWED.contains(&k) => {
                        return Err(PageError::Cascade(format!(
                            "the key {k:?} cannot be cascaded"
                        )));
                    }
                    _ => rule.fields.insert(k, v),
                }
            }
            out.push(rule);
        }
        Ok(out)
    }

    /// The cascade from site-level `[[cascade]]` entries.
    ///
    /// # Errors
    /// See [`CascadeTarget::new`].
    pub fn from_config(entries: &[CascadeConfig]) -> Result<Self, PageError> {
        let mut out = Self::default();
        for e in entries {
            out.push(CascadeRule::from_config(e)?);
        }
        Ok(out)
    }

    /// Adds a weaker rule; merged into an existing rule with the same target.
    pub fn push(&mut self, rule: CascadeRule) {
        if let Some(existing) = self.0.iter_mut().find(|r| r.target == rule.target) {
            fill_shallow(&mut existing.fields, &rule.fields);
            fill_shallow(&mut existing.params, &rule.params);
        } else {
            self.0.push(rule);
        }
    }

    /// The cascade in force below a page: its own entries, then its parent's.
    #[must_use]
    pub fn inherit(parent: &Self, own: &Self) -> Self {
        let mut out = own.clone();
        for rule in &parent.0 {
            out.push(rule.clone());
        }
        out
    }

    /// Fills the page's missing front matter keys from every matching rule, strongest
    /// first. Keys the page sets are never replaced.
    pub fn apply(&self, m: &MatchCtx<'_>, p: &mut Params) {
        for rule in self.0.iter().filter(|r| r.target.matches(m)) {
            fill_shallow(p, &rule.fields);
            fill_shallow(p, &rule.params);
        }
    }

    /// [`apply`](Self::apply) for a page a content adapter added, whose fields and params are
    /// apart (Go's `setMetaPost`): the rules' fields fill `fields` (the `add_page` map), their
    /// params fill `params`.
    pub fn apply_split(&self, m: &MatchCtx<'_>, fields: &mut Params, params: &mut Params) {
        for rule in self.0.iter().filter(|r| r.target.matches(m)) {
            fill_shallow(fields, &rule.fields);
            fill_shallow(params, &rule.params);
        }
    }

    /// The rules, strongest first.
    #[must_use]
    pub fn rules(&self) -> &[CascadeRule] {
        &self.0
    }

    /// Whether there are no rules.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Adds the top-level keys of `src` that `dst` lacks (a present key keeps its whole value).
fn fill_shallow(dst: &mut Params, src: &Params) {
    for (k, v) in src.iter() {
        if dst.get(k).is_none() {
            dst.insert(k, v);
        }
    }
}

fn decode_target(v: &Value) -> Result<CascadeTarget, PageError> {
    let Some(m) = v.as_map() else {
        return Err(PageError::Cascade(format!(
            "target: expected a table, found {}",
            value::kind(v)
        )));
    };
    let m = Params::fold(m);
    let field = |k: &str| -> Result<String, PageError> {
        m.get(k).map_or(Ok(String::new()), |v| {
            value::weak_string(v).ok_or_else(|| {
                PageError::Cascade(format!(
                    "target.{k}: expected a string, found {}",
                    value::kind(v)
                ))
            })
        })
    };
    CascadeTarget::new(
        &field("kind")?,
        &field("path")?,
        &field("lang")?,
        &field("environment")?,
    )
}
