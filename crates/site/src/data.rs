//! `.Site.Data`: the files of the data component merged into one case-preserving tree.
//!
//! - `data/**/*.{yaml,yml,toml,json,csv,xml}`: each directory is a nested map, each file's name
//!   without its extension (`a.b.json` → `a.b`, `site.th.yaml` → `site.th`) is its key. Keys
//!   are never folded and may contain `/` or `.`.
//! - **Precedence.** Within a directory, subdirectories come first (depth first), then the
//!   files: the project's before the themes', then by extension (descending: `yaml`, `toml`,
//!   `json`), then by name. The first value at a key wins: a later map only adds the keys the
//!   earlier map lacks (one level); a later array, or any value meeting a non-map, is dropped
//!   with a warning.
//! - A file's value must be a map or an array; an empty file is an empty map (an empty CSV an
//!   empty array), `null` is skipped, and other scalars are errors.
//! - CSV is a list of rows of strings; XML is the root element's content with attributes as
//!   `-name`, text beside children or attributes as `#text`, repeated elements as arrays.

use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use ssg_base::diag::Diagnostic;
use ssg_base::{Map, Value};
use ssg_vfs::{Component, FileRef, Module, Vfs, VfsError};

/// A data file that could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum DataError {
    #[error(transparent)]
    Vfs(#[from] VfsError),
    #[error("data file {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("data file {path}: {message}")]
    Decode { path: PathBuf, message: String },
    #[error("data file {path}: the format {ext:?} is not supported")]
    UnsupportedFormat { path: PathBuf, ext: String },
}

/// The data tree and what loading it reported.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Data {
    /// `.Site.Data`.
    pub map: Map,
    /// Values dropped for a higher-precedence value, and files that are not maps or arrays.
    pub diagnostics: Vec<Diagnostic>,
}

/// A directory of the union data view.
#[derive(Default)]
struct Dir<'a> {
    dirs: BTreeMap<&'a str, Dir<'a>>,
    files: Vec<(&'a str, &'a FileRef, Module)>,
}

/// Loads the data component of `vfs`.
///
/// # Errors
/// A walk or read error, a file that does not decode, or an unsupported extension.
pub fn load(vfs: &Vfs) -> Result<Data, DataError> {
    let files = vfs.walk(Component::Data)?;
    let module = |f: &FileRef| {
        vfs.mounts()
            .get(usize::from(f.mount_idx))
            .map_or(Module::Project, |m| m.module)
    };
    let mut root = Dir::default();
    for f in &files {
        let mut dir = &mut root;
        let mut parts = f.rel.split('/').filter(|s| !s.is_empty()).peekable();
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                dir.files.push((part, f, module(f)));
            } else {
                dir = dir.dirs.entry(part).or_default();
            }
        }
    }
    let mut out = Data::default();
    load_dir(&root, &mut Vec::new(), &mut out)?;
    Ok(out)
}

fn load_dir<'a>(dir: &Dir<'a>, path: &mut Vec<&'a str>, out: &mut Data) -> Result<(), DataError> {
    for (name, sub) in &dir.dirs {
        path.push(name);
        load_dir(sub, path, out)?;
        path.pop();
    }
    let mut files: Vec<_> = dir.files.iter().collect();
    files.sort_by_key(|(name, _, module)| {
        let (stem, ext) = split_ext(name);
        (
            *module,
            Reverse(ext.to_lowercase()),
            stem.to_lowercase(),
            *name,
        )
    });
    for (name, f, _) in files {
        let (stem, ext) = split_ext(name);
        let Some(value) = decode(f, ext)? else {
            continue;
        };
        if !matches!(value, Value::Map(_) | Value::Array(_)) {
            out.diagnostics.push(Diagnostic::error(format!(
                "data file {}: expected a map or an array, found {}",
                f.rel,
                kind(&value)
            )));
            continue;
        }
        insert(&mut out.map, path, stem, value, f, &mut out.diagnostics);
    }
    Ok(())
}

/// Places a file's value at `path` + `stem`, below what is already there.
fn insert(
    root: &mut Map,
    path: &[&str],
    stem: &str,
    value: Value,
    f: &FileRef,
    diags: &mut Vec<Diagnostic>,
) {
    let mut current = root;
    for part in path {
        if !current.contains_key(part) {
            current.insert(*part, Value::map(Map::new()));
        }
        match current.get_mut(part) {
            Some(Value::Map(m)) => current = Arc::make_mut(m),
            _ => {
                diags.push(overridden(f, &value));
                return;
            }
        }
    }
    match (current.get_mut(stem), value) {
        (None, v) => {
            current.insert(stem, v);
        }
        (Some(Value::Map(existing)), Value::Map(new)) => {
            let existing = Arc::make_mut(existing);
            for (k, v) in new.iter() {
                if !existing.contains_key(k) {
                    existing.insert(k, v.clone());
                }
            }
        }
        (Some(_), v) => diags.push(overridden(f, &v)),
    }
}

fn overridden(f: &FileRef, v: &Value) -> Diagnostic {
    Diagnostic::warning(format!(
        "data file {}: its {} is ignored, a higher-precedence value has the same key",
        f.rel,
        kind(v)
    ))
    .with_id("data-overridden")
}

fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Int(_) | Value::Float(_) => "number",
        Value::String(_) => "string",
        Value::Date(_) => "date",
        Value::Array(_) => "array",
        Value::Map(_) => "map",
    }
}

/// `a.b.json` → (`a.b`, `json`); no dot → (name, "").
fn split_ext(name: &str) -> (&str, &str) {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, ext),
        _ => (name, ""),
    }
}

/// Decodes a data file; `None` for a `null` document.
fn decode(f: &FileRef, ext: &str) -> Result<Option<Value>, DataError> {
    let ext = ext.to_lowercase();
    let text = std::fs::read_to_string(&f.abs).map_err(|source| DataError::Read {
        path: f.abs.clone(),
        source,
    })?;
    let err = |message: String| DataError::Decode {
        path: f.abs.clone(),
        message,
    };
    if !matches!(
        ext.as_str(),
        "yaml" | "yml" | "toml" | "json" | "csv" | "xml"
    ) {
        return Err(DataError::UnsupportedFormat {
            path: f.abs.clone(),
            ext,
        });
    }
    if text.trim().is_empty() {
        return Ok(Some(if ext == "csv" {
            Value::array(Vec::new())
        } else {
            Value::map(Map::new())
        }));
    }
    let v = match ext.as_str() {
        "yaml" | "yml" => Value::from_yaml_str(&text).map_err(|e| err(e.to_string()))?,
        "toml" => Value::from_toml_str(&text).map_err(|e| err(e.to_string()))?,
        "json" => Value::from_json_str(&text).map_err(|e| err(e.to_string()))?,
        "csv" => csv_rows(&text).map_err(|e| err(e.to_string()))?,
        _ => xml_root(&text).map_err(|e| err(e.to_string()))?,
    };
    Ok((!v.is_null()).then_some(v))
}

/// CSV (comma separated, rows of any length) as an array of arrays of strings.
fn csv_rows(text: &str) -> Result<Value, csv::Error> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes());
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record?;
        rows.push(Value::array(record.iter().map(Value::string).collect()));
    }
    Ok(Value::array(rows))
}

/// An XML document as the value of its root element.
fn xml_root(text: &str) -> Result<Value, roxmltree::Error> {
    let doc = roxmltree::Document::parse(text)?;
    Ok(xml_element(doc.root_element()))
}

fn xml_element(e: roxmltree::Node<'_, '_>) -> Value {
    let mut m = Map::new();
    for a in e.attributes() {
        m.insert(format!("-{}", a.name()), Value::string(a.value()));
    }
    let mut text = String::new();
    for child in e.children() {
        if child.is_element() {
            let name = child.tag_name().name();
            let v = xml_element(child);
            match m.get_mut(name) {
                None => {
                    m.insert(name, v);
                }
                Some(Value::Array(items)) => Arc::make_mut(items).push(v),
                Some(existing) => {
                    let first = std::mem::take(existing);
                    *existing = Value::array(vec![first, v]);
                }
            }
        } else if let Some(t) = child.text() {
            text.push_str(t);
        }
    }
    let text = text.trim();
    if m.is_empty() {
        return Value::string(text);
    }
    if !text.is_empty() {
        m.insert("#text", Value::string(text));
    }
    Value::map(m)
}
