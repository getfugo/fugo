//! The editor's index of the site (inside the Worker, which serves it as `GET api/site`):
//! languages, taxonomies and their terms, the sections with the front matter keys their pages
//! use, and every page with its files.
//!
//! The index is read from the content directories, not from the built model, so it lists what
//! a build leaves out too (drafts, future and expired pages). A page ("entry") is a leaf bundle
//! (`dir/index.<lang>.md`, key `dir`), a branch page (`dir/_index.<lang>.md`, key
//! `dir/_index`) or a single file (`dir/name.<lang>.md`, key `dir/name`); its files are its
//! translations, keyed by the path relative to their content directory, so translations in
//! per-language content directories meet in one entry. A leaf bundle's other files are its
//! resources.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Serialize;
use ssg_base::{Map, Value};
use ssg_config::Config;
use ssg_pageparser::{FrontMatterFormat, decode_front_matter_map, split_front_matter};

use crate::config::{CmsConfig, Field, Workflow};
use crate::fields::Hint;
use crate::paths::{self, MEDIA_EXT};

mod content;
use content::*;
mod hints;
use hints::{Keys, OWN};

/// The schema version of the index.
const VERSION: u32 = 1;

#[derive(Debug, Serialize)]
pub struct Index {
    pub version: u32,
    pub title: String,
    /// The editor's URL path (`/admin/`).
    pub path: String,
    /// The API's URL path (`/admin/api/`).
    pub api: String,
    pub site_url: String,
    pub workflow: Workflow,
    pub languages: Vec<Lang>,
    pub default_language: String,
    pub taxonomies: Vec<Taxonomy>,
    /// The field of every front matter key (lower-cased) the content or the settings have.
    pub fields: BTreeMap<String, Hint>,
    /// The content directory (project-relative).
    pub content_dir: Option<String>,
    /// The media directory (project-relative).
    pub media: Option<String>,
    /// How front matter names a file of the media directory: the prefix of its path after the
    /// directory (`/images/uploads/` for `static/images/uploads`: the file's URL path).
    pub media_ref: Option<String>,
    pub upload_types: Vec<&'static str>,
    pub max_upload: u64,
    pub sections: Vec<Section>,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Serialize)]
pub struct Lang {
    pub key: String,
    pub name: String,
    /// The language's own content directory, when it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_dir: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Taxonomy {
    pub plural: String,
    pub singular: String,
    pub hierarchical: bool,
    /// The terms the content uses, sorted.
    pub terms: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Section {
    /// The first path segment (`""` for pages at the root).
    pub key: String,
    pub title: String,
    /// Regular pages in the section.
    pub count: usize,
    /// Its folders: the section pages below it (`<key>/…/_index`), such as the terms of a
    /// taxonomy (`brands/lays/_index`).
    pub folders: usize,
    /// How the section's pages (its folders', when it has no pages) are written, for new pages.
    pub style: Style,
    /// Front matter keys its pages (its folders', when it has no pages) use, with the kind of
    /// their values.
    pub keys: Vec<KeyKind>,
}

/// How new pages of a section are written: as bundles or single files, with language suffixes
/// or not, in which front matter format and extension.
#[derive(Debug, Serialize)]
pub struct Style {
    pub bundle: bool,
    pub lang_suffix: bool,
    pub format: &'static str,
    pub ext: String,
}

#[derive(Debug, Serialize)]
pub struct KeyKind {
    pub key: String,
    pub kind: &'static str,
    /// The value every page of the section that has the key gives it, for new pages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct Entry {
    pub key: String,
    pub section: String,
    /// `home`, `section` or `page`.
    pub kind: &'static str,
    /// A leaf bundle (its files are `index.*` in its own directory).
    pub bundle: bool,
    pub title: String,
    pub files: Vec<File>,
    /// A leaf bundle's other files (project-relative), sorted (always present, maybe empty).
    pub resources: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct File {
    pub lang: String,
    /// Project-relative.
    pub path: String,
    /// `yaml`, `toml`, `json`, `org` or `none`.
    pub format: &'static str,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub title: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub draft: bool,
    /// The URL path of its page as the build made it (`/posts/hello/`), for pages the build
    /// rendered: a page the editor moves keeps it as an alias.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The name carries the language (`index.th.md`).
    #[serde(skip)]
    suffixed: bool,
    /// Its front matter keys, the kind of their values, and the values that are not tables.
    #[serde(skip)]
    keys: Vec<(String, &'static str, Option<Value>)>,
}

/// One content file before it is grouped into an entry.
struct Found {
    key: String,
    kind: &'static str,
    bundle: bool,
    file: File,
    order: usize,
}

/// Reads the index of a site; `urls` are the URLs of the pages of the content files the build
/// rendered, by the files' paths in the project.
///
/// # Errors
/// Unreadable content directories.
pub fn read(
    cfg: &Config,
    cms: &CmsConfig,
    urls: &BTreeMap<String, String>,
) -> std::io::Result<Index> {
    let default_lang = cfg.default_site().language.key.clone();
    let lang_order: BTreeMap<&str, usize> = cfg
        .sites
        .iter()
        .enumerate()
        .map(|(i, s)| (s.language.key.as_str(), i))
        .collect();
    let content_suffixes: BTreeSet<String> = cfg
        .content_types
        .0
        .iter()
        .flat_map(|&id| cfg.media_types.get(id).suffixes.iter().cloned())
        .collect();
    let taxonomies = &cfg.default_site().taxonomies;
    let mut terms: Vec<BTreeSet<String>> = taxonomies.iter().map(|_| BTreeSet::new()).collect();
    let mut keys = Keys::default();

    // Content directories with the language their files default to.
    let root = paths::project_rel(cfg, &cfg.dirs.content);
    let mut roots: Vec<(String, Option<String>)> = Vec::new();
    if let Some(r) = &root {
        roots.push((r.clone(), None));
    }
    for s in &cfg.sites {
        if let Some(d) = s
            .content_dir
            .as_ref()
            .and_then(|d| paths::project_rel(cfg, d))
            && Some(&d) != root.as_ref()
            && !roots.iter().any(|(r, _)| r == &d)
        {
            roots.push((d, Some(s.language.key.clone())));
        }
    }

    let mut found: Vec<Found> = Vec::new();
    let mut resources: Vec<(String, String)> = Vec::new();
    for (dir, lang) in &roots {
        let mut files = Vec::new();
        walk(&cfg.project_dir.join(dir), "", &mut files)?;
        for rel in files {
            let (parent, name) = rel
                .rsplit_once('/')
                .map_or(("", rel.as_str()), |(p, n)| (p, n));
            let Some((stem, ext)) = name.rsplit_once('.') else {
                continue;
            };
            let ext_lc = ext.to_ascii_lowercase();
            if stem.eq_ignore_ascii_case("_content")
                || stem
                    .split('.')
                    .next()
                    .is_some_and(|s| s.eq_ignore_ascii_case("_content"))
            {
                continue; // a content adapter (a template)
            }
            let path = format!("{dir}/{rel}");
            if !content_suffixes.contains(&ext_lc) {
                resources.push((parent.to_owned(), path));
                continue;
            }
            let (base, file_lang, suffixed) = match stem.rsplit_once('.') {
                Some((b, l)) if lang_order.contains_key(l) => (b, l.to_owned(), true),
                _ => (
                    stem,
                    lang.clone().unwrap_or_else(|| default_lang.clone()),
                    false,
                ),
            };
            let join = |a: &str, b: &str| {
                if a.is_empty() {
                    b.to_owned()
                } else {
                    format!("{a}/{b}")
                }
            };
            let (key, kind, bundle) = match base {
                "index" if !parent.is_empty() => (parent.to_owned(), "page", true),
                "_index" if parent.is_empty() => ("_index".to_owned(), "home", false),
                "_index" => (join(parent, "_index"), "section", false),
                _ => (join(parent, base), "page", false),
            };
            let src = std::fs::read_to_string(cfg.project_dir.join(&path)).unwrap_or_default();
            let (format, fm) = front_matter(&src);
            keys.add(&key, &fm);
            for (i, t) in taxonomies.iter().enumerate() {
                if let Some(v) = get_ci(&fm, &t.plural) {
                    match v {
                        Value::String(s) if !s.trim().is_empty() => {
                            terms[i].insert(s.trim().to_owned());
                        }
                        Value::Array(items) => {
                            for item in items.iter() {
                                if let Value::String(s) = item
                                    && !s.trim().is_empty()
                                {
                                    terms[i].insert(s.trim().to_owned());
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            let title = get_ci(&fm, "title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let draft = get_ci(&fm, "draft").and_then(Value::as_bool) == Some(true);
            let keys = fm
                .iter()
                .map(|(k, v)| (k.to_owned(), kind_of(v), small(v).then(|| v.clone())))
                .collect();
            let url = urls.get(&path).cloned();
            found.push(Found {
                order: lang_order
                    .get(file_lang.as_str())
                    .copied()
                    .unwrap_or(usize::MAX),
                key,
                kind,
                bundle,
                file: File {
                    lang: file_lang,
                    path,
                    format,
                    title,
                    draft,
                    url,
                    suffixed,
                    keys,
                },
            });
        }
    }

    // Group the files into entries.
    let mut entries: BTreeMap<String, Entry> = BTreeMap::new();
    found.sort_by(|a, b| a.key.cmp(&b.key).then(a.order.cmp(&b.order)));
    for f in found {
        let e = entries.entry(f.key.clone()).or_insert_with(|| Entry {
            section: section_of(&f.key, f.kind),
            key: f.key,
            kind: f.kind,
            bundle: f.bundle,
            title: String::new(),
            files: Vec::new(),
            resources: Vec::new(),
        });
        if !e.files.iter().any(|x| x.lang == f.file.lang) {
            e.files.push(f.file);
        }
    }
    // A bundle's files belong to the nearest leaf bundle above them; a branch page owns the
    // files directly in its directory.
    for (dir, path) in resources {
        let mut d = dir.as_str();
        let owner = loop {
            if entries.get(d).is_some_and(|e| e.bundle) {
                break Some(d.to_owned());
            }
            if d.is_empty() {
                break None;
            }
            d = d.rsplit_once('/').map_or("", |(p, _)| p);
        };
        let branch = if dir.is_empty() {
            "_index".to_owned()
        } else {
            format!("{dir}/_index")
        };
        let owner = owner.or_else(|| entries.contains_key(&branch).then_some(branch));
        if let Some(e) = owner.and_then(|o| entries.get_mut(&o)) {
            e.resources.push(path);
        }
    }
    for e in entries.values_mut() {
        e.resources.sort();
        let title = e
            .files
            .iter()
            .find(|f| f.lang == default_lang && !f.title.is_empty())
            .or_else(|| e.files.iter().find(|f| !f.title.is_empty()))
            .map(|f| f.title.clone());
        e.title = title.unwrap_or_else(|| {
            let name = e.key.trim_end_matches("/_index");
            name.rsplit('/').next().unwrap_or(name).to_owned()
        });
    }

    let sections = sections(&entries, &default_lang);
    let base = cfg.default_site().base_url.base_path();
    let path = format!("{base}{}/", cms.path);
    Ok(Index {
        version: VERSION,
        title: cms
            .title
            .clone()
            .unwrap_or_else(|| cfg.default_site().title.clone()),
        api: format!("{path}api/"),
        path,
        site_url: cfg.default_site().base_url.as_str().to_owned(),
        workflow: cms.workflow,
        languages: cfg
            .sites
            .iter()
            .map(|s| Lang {
                key: s.language.key.clone(),
                name: if s.language.name.is_empty() {
                    s.language.key.clone()
                } else {
                    s.language.name.clone()
                },
                content_dir: s
                    .content_dir
                    .as_ref()
                    .and_then(|d| paths::project_rel(cfg, d))
                    .filter(|d| Some(d) != root.as_ref()),
            })
            .collect(),
        default_language: default_lang,
        taxonomies: taxonomies
            .iter()
            .zip(terms)
            .map(|(t, terms)| Taxonomy {
                plural: t.plural.clone(),
                singular: t.singular.clone(),
                hierarchical: t.hierarchical,
                terms: terms.into_iter().collect(),
            })
            .collect(),
        fields: keys.hints(
            &cms.fields,
            &taxonomies.iter().map(|t| t.plural.to_lowercase()).collect(),
        ),
        content_dir: root.clone(),
        media_ref: cms.media.as_deref().and_then(|m| media_ref(cfg, m)),
        media: cms.media.clone(),
        upload_types: MEDIA_EXT.to_vec(),
        max_upload: cms.max_upload,
        sections,
        entries: entries.into_values().collect(),
    })
}
