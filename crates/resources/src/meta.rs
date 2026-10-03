//! Front matter `resources` metadata, and the lookups of a `Resources` list (`Get`, `GetMatch`,
//! `Match`, `ByType`).
//!
//! Metadata entries are applied in order: the first matching entry that sets `name` (or
//! `title`) wins, and every matching entry's `params` are merged over the resource's (later
//! entries override earlier ones). `src` is a Go glob matched case-insensitively against the
//! resource's name. `:counter` in a name or title is the resource's number among the resources
//! the entry matched; like Go, which applies the metadata to one resource at a time, it is
//! always `1`.

use std::sync::Arc;

use ssg_base::glob::{self, Glob, GlobError, GlobOpts};
use ssg_base::{Params, ResourceId, Value, text};

use crate::store::{Resource, ResourceError, ResourceStore};

const COUNTER: &str = ":counter";

/// One entry of the `resources` front matter list.
#[derive(Clone, Debug)]
pub struct MetaEntry {
    /// The `src` pattern as written.
    pub src: String,
    glob: Glob,
    pub name: Option<String>,
    pub title: Option<String>,
    pub params: Option<Params>,
}

/// The `resources` front matter of a page.
#[derive(Clone, Debug, Default)]
pub struct ResourceMeta {
    pub entries: Vec<MetaEntry>,
}

/// A resource's name, title and params after the metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct Applied {
    pub name: String,
    pub title: String,
    pub params: Params,
}

fn scalar_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.to_string()),
        Value::Int(i) => Some(i.to_string()),
        Value::Float(f) => Some(f.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Date(d) => Some(d.to_string()),
        Value::Null | Value::Array(_) | Value::Map(_) => None,
    }
}

impl ResourceMeta {
    /// Parses the `resources` front matter value: a list of maps with `src` and any of
    /// `name`, `title` and `params` (keys ignore case).
    ///
    /// # Errors
    /// A value that is not a list of maps, an entry without `src`, a `src` that is not a
    /// glob, or a `params` that is not a map.
    pub fn parse(v: &Value) -> Result<Self, ResourceError> {
        let invalid = |index: usize, reason: &str| ResourceError::Metadata {
            index,
            reason: reason.to_owned(),
        };
        let Some(items) = v.as_array() else {
            return Err(invalid(0, "`resources` must be a list of tables"));
        };
        let mut entries = Vec::with_capacity(items.len());
        for (index, item) in items.iter().enumerate() {
            let Some(m) = item.as_map() else {
                return Err(invalid(index, "not a table"));
            };
            let m = Params::fold(m);
            let src = m
                .get("src")
                .and_then(scalar_string)
                .ok_or_else(|| invalid(index, "missing `src`"))?;
            let glob = glob::compile(&text::to_lower(&src), GlobOpts::default())
                .map_err(|e| invalid(index, &e.to_string()))?;
            let params = match m.get("params") {
                None | Some(Value::Null) => None,
                Some(Value::Map(p)) => Some(Params::fold(p)),
                Some(_) => return Err(invalid(index, "`params` must be a table")),
            };
            entries.push(MetaEntry {
                src,
                glob,
                // Bundle names are relative with forward slashes; renames follow that form.
                name: m
                    .get("name")
                    .and_then(scalar_string)
                    .map(|n| n.replace('\\', "/").trim_start_matches('/').to_owned()),
                title: m.get("title").and_then(scalar_string),
                params,
            });
        }
        Ok(Self { entries })
    }

    /// Applies the entries to a resource named `name` with `title` and `params`.
    #[must_use]
    pub fn apply(&self, name: &str, title: &str, params: &Params) -> Applied {
        let key = text::to_lower(name);
        let mut out = Applied {
            name: name.to_owned(),
            title: title.to_owned(),
            params: params.clone(),
        };
        let (mut name_set, mut title_set) = (false, false);
        for e in self.entries.iter().filter(|e| e.glob.is_match(&key)) {
            if !name_set && let Some(n) = &e.name {
                out.name = n.replace(COUNTER, "1");
                name_set = true;
            }
            if !title_set && let Some(t) = &e.title {
                out.title = t.replace(COUNTER, "1");
                title_set = true;
            }
            if let Some(p) = &e.params {
                for (k, v) in p.iter() {
                    out.params.insert(k, v);
                }
            }
        }
        out
    }
}

impl ResourceStore {
    /// Resource `id` with the page's `resources` metadata applied: another resource (same
    /// target and content) when the name, title or params change, else `id`.
    #[must_use]
    pub fn apply_meta(&self, id: ResourceId, meta: &ResourceMeta) -> ResourceId {
        if meta.entries.is_empty() {
            return id;
        }
        let r = self.resource(id);
        let a = meta.apply(&r.name, &r.title, &r.params);
        self.with_meta(id, a.name, a.title, a.params)
    }
}

/// What the lookups of a resource list need: pages of a bundle implement it too.
pub trait Named {
    /// `.Name`.
    fn name(&self) -> &str;
    /// The normalized name, when there is one.
    fn name_normalized(&self) -> Option<&str>;
    /// `.ResourceType`.
    fn resource_type(&self) -> &str;
}

impl Named for Resource {
    fn name(&self) -> &str {
        &self.name
    }

    fn name_normalized(&self) -> Option<&str> {
        Some(&self.name_normalized)
    }

    fn resource_type(&self) -> &str {
        Resource::resource_type(self)
    }
}

impl<T: Named + ?Sized> Named for Arc<T> {
    fn name(&self) -> &str {
        (**self).name()
    }

    fn name_normalized(&self) -> Option<&str> {
        (**self).name_normalized()
    }

    fn resource_type(&self) -> &str {
        (**self).resource_type()
    }
}

fn with_leading_slash(s: &str) -> String {
    if s.starts_with('/') {
        s.to_owned()
    } else {
        format!("/{s}")
    }
}

fn equal_fold(a: &str, b: &str) -> bool {
    a == b || text::to_lower(a) == text::to_lower(b)
}

/// `.Resources.Get name`: the first resource whose name, then whose normalized name, equals
/// `name` ignoring case. Both sides are compared with a leading `/`, unless `name` starts with
/// `./`, which compares the rest as written.
pub fn get<'a, R: Named>(rs: &'a [R], name: &str) -> Option<&'a R> {
    let (want, dot) = match name.strip_prefix("./") {
        Some(rest) => (rest.to_owned(), true),
        None => (with_leading_slash(name), false),
    };
    let check = |n: &str| {
        if dot {
            equal_fold(&want, n)
        } else {
            equal_fold(&want, &with_leading_slash(n))
        }
    };
    rs.iter()
        .find(|r| check(r.name()))
        .or_else(|| rs.iter().find(|r| r.name_normalized().is_some_and(check)))
}

fn glob_of(pattern: &str) -> Result<Glob, GlobError> {
    glob::compile(&with_leading_slash(pattern), GlobOpts::default())
}

/// `.Resources.GetMatch pattern`: the first resource whose name (with a leading `/`), then
/// whose normalized name, matches the glob.
///
/// # Errors
/// A pattern that is not a glob.
pub fn get_match<'a, R: Named>(rs: &'a [R], pattern: &str) -> Result<Option<&'a R>, GlobError> {
    let g = glob_of(pattern)?;
    Ok(rs
        .iter()
        .find(|r| g.is_match(&with_leading_slash(r.name())))
        .or_else(|| {
            rs.iter().find(|r| {
                r.name_normalized()
                    .is_some_and(|n| g.is_match(&with_leading_slash(n)))
            })
        }))
}

/// `.Resources.Match pattern`: every resource whose name matches the glob; when none does,
/// every resource whose normalized name matches.
///
/// # Errors
/// A pattern that is not a glob.
pub fn matches<'a, R: Named>(rs: &'a [R], pattern: &str) -> Result<Vec<&'a R>, GlobError> {
    let g = glob_of(pattern)?;
    let by_name: Vec<&R> = rs
        .iter()
        .filter(|r| g.is_match(&with_leading_slash(r.name())))
        .collect();
    if !by_name.is_empty() {
        return Ok(by_name);
    }
    Ok(rs
        .iter()
        .filter(|r| {
            r.name_normalized()
                .is_some_and(|n| g.is_match(&with_leading_slash(n)))
        })
        .collect())
}

/// `.Resources.ByType typ`: the resources whose resource type is `typ`.
pub fn by_type<'a, R: Named>(rs: &'a [R], typ: &str) -> Vec<&'a R> {
    rs.iter().filter(|r| r.resource_type() == typ).collect()
}
