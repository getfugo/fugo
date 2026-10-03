//! Phase A4: read every content file of the discovery, split and decode its front matter, and
//! read the capture overrides (`kind`, `lang`, `path`) and the page's own `cascade`; list the
//! content adapters (`_content.html`).
//!
//! Files are read in parallel; the result keeps the discovery order (key, then language). The
//! pages content adapters add are captured from their `add_page` maps ([`adapter_page`]) and go
//! after the files.

use std::path::Path;
use std::sync::Arc;

use jiff::Timestamp;
use rayon::prelude::*;
use ssg_base::diag::Diagnostic;
use ssg_base::{LangIdx, PageKind, Params, Value};
use ssg_config::Config;
use ssg_page::{AdapterPage, Cascade, capture_overrides};
use ssg_pageparser::{FrontMatterFormat, decode_front_matter, split_front_matter};
use ssg_vfs::{BundleKind, Component, ContentFile, FileRef, Parsed, PathInfo, PathParser, Vfs};

use crate::{AddedResource, ModelError};

/// The content file a page was read from.
#[derive(Clone, Debug)]
pub struct SourceFile {
    pub file: FileRef,
    /// The file's path as discovered.
    pub file_info: PathInfo,
    /// The page's path: `file_info`, or the path front matter `path` names.
    pub info: PathInfo,
    /// The front matter's format (`None`: the file has none).
    pub front_matter: Option<FrontMatterFormat>,
    /// The whole file.
    pub text: Arc<str>,
    /// Where the body starts in `text` (after the front matter and any byte order mark).
    pub body_offset: usize,
    /// The file's modification time (`:fileModTime`).
    pub mod_time: Option<Timestamp>,
}

impl SourceFile {
    /// The content after the front matter.
    #[must_use]
    pub fn body(&self) -> &str {
        &self.text[self.body_offset..]
    }

    /// The file name `:filename` dates read: the bundle directory's name for a bundle index,
    /// else the file name without identifiers, in the file's own spelling.
    #[must_use]
    pub fn base_filename(&self) -> &str {
        &self.file_info.original.name
    }
}

/// A content file after phase A4: where it goes, and its folded front matter.
#[derive(Clone, Debug)]
pub(crate) struct CapturedPage {
    pub lang: LangIdx,
    /// Front matter `kind` (normalised); `None`: from the path.
    pub kind: Option<PageKind>,
    /// A content file inside a leaf bundle (`BundleKind::ContentResource`).
    pub bundled: bool,
    pub source: SourceFile,
    /// Folded front matter; `kind`, `lang` and `path` normalised as the overrides read them.
    pub params: Params,
    /// The page's own `cascade`.
    pub cascade: Cascade,
    /// A page a content adapter added: its `add_page` map (its fields are read with
    /// [`ssg_page::meta_from_adapter`], not as front matter).
    pub adapter: Option<Arc<AdapterPage>>,
}

/// A content adapter: a `_content.html` Tera template that adds pages and resources to its
/// directory when the model is built (Go's `_content.gotmpl`).
#[derive(Clone, Debug)]
pub struct ContentAdapter {
    pub file: FileRef,
    pub info: PathInfo,
    /// The language it runs for first: the file name's, else the mount's, else the default
    /// language.
    pub lang: LangIdx,
}

/// A bundle resource file that is not content (images, data, …), or a resource a content
/// adapter added (then `file` is the adapter).
#[derive(Clone, Debug)]
pub(crate) struct CapturedResource {
    pub lang: LangIdx,
    pub file: FileRef,
    pub info: PathInfo,
    pub adapter: Option<Arc<AddedResource>>,
}

/// The output of phase A4.
#[derive(Clone, Debug, Default)]
pub(crate) struct Capture {
    pub pages: Vec<CapturedPage>,
    pub resources: Vec<CapturedResource>,
    pub adapters: Vec<ContentAdapter>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Discovers and reads the content files of `vfs`.
pub(crate) fn capture(cfg: &Config, vfs: &Vfs) -> Result<Capture, ModelError> {
    let parser = PathParser::from_config(cfg);
    let discovery = vfs.discover_content(&parser)?;
    let mut out = Capture::default();
    for d in &discovery.duplicates {
        out.diagnostics.push(
            Diagnostic::warning(format!(
                "duplicate content path {:?}: {} is used, {} is ignored",
                d.key.to_path(),
                d.kept.display(),
                d.dropped.display()
            ))
            .with_id("duplicate-content-path"),
        );
    }

    let (content, resources): (Vec<ContentFile>, Vec<ContentFile>) = discovery
        .files
        .into_iter()
        .partition(|f| f.info.kind.is_content());
    for f in discovery.adapters {
        if f.info.ext != "html" {
            return Err(ModelError::GoContentAdapter(f.file.abs));
        }
        out.adapters.push(ContentAdapter {
            file: f.file,
            info: f.info,
            lang: f.lang,
        });
    }
    out.resources = resources
        .into_iter()
        .map(|f| CapturedResource {
            lang: f.lang,
            file: f.file,
            info: f.info,
            adapter: None,
        })
        .collect();

    let read: Vec<Result<CapturedPage, ModelError>> = content
        .into_par_iter()
        .map(|f| read_page(f, &parser))
        .collect();
    for r in read {
        out.pages.push(r?);
    }
    Ok(out)
}

fn read_page(f: ContentFile, parser: &PathParser) -> Result<CapturedPage, ModelError> {
    let abs = f.file.abs.clone();
    let text: Arc<str> = std::fs::read_to_string(&abs)
        .map_err(|source| ModelError::Read {
            path: abs.clone(),
            source,
        })?
        .into();
    let split = split_front_matter(&text).map_err(|e| ModelError::FrontMatter {
        path: abs.clone(),
        message: e.to_string(),
    })?;
    let front_matter = split.front_matter.map(|(format, _)| format);
    let mut params = match split.front_matter {
        Some((format, fm)) => {
            decode_front_matter(format, fm).map_err(|e| ModelError::FrontMatter {
                path: abs.clone(),
                message: e.to_string(),
            })?
        }
        None => Params::default(),
    };
    let body_offset = split.body_offset;
    let page_err = |source| ModelError::Page {
        path: abs.clone(),
        source,
    };
    let overrides = capture_overrides(&params).map_err(page_err)?;
    let cascade = params
        .get("cascade")
        .map_or(Ok(Cascade::default()), Cascade::decode)
        .map_err(page_err)?;

    let mut lang = f.lang;
    if let Some(l) = overrides.lang.as_deref()
        && let Some(idx) = parser.language(l)
    {
        lang = idx;
        params.insert("lang", &Value::string(l));
    }
    if let Some(k) = overrides.kind {
        params.insert("kind", &Value::string(k.as_str()));
    }
    let bundled = f.info.kind == BundleKind::ContentResource;
    let mut info = f.info.clone();
    if let Some(Value::String(p)) = params.get("path").cloned()
        && !p.trim().is_empty()
    {
        let path = p.trim().replace('\\', "/");
        params.insert("path", &Value::string(&path));
        info = moved_path(&path, overrides.kind, &f.info, parser, &abs)?;
    }
    let mod_time = std::fs::metadata(&abs)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| Timestamp::try_from(t).ok());
    Ok(CapturedPage {
        lang,
        kind: overrides.kind,
        bundled,
        source: SourceFile {
            file: f.file,
            file_info: f.info,
            info,
            front_matter,
            text,
            body_offset,
            mod_time,
        },
        params,
        cascade,
        adapter: None,
    })
}

/// The resource an adapter added with `add_resource`: a bundle resource at its path (the
/// page at the longest key above it owns it).
pub(crate) fn adapter_resource(
    adapter: &ContentAdapter,
    added: AddedResource,
    parser: &PathParser,
) -> Result<CapturedResource, ModelError> {
    let info = match parser.parse(Component::Content, &format!("/{}", added.path)) {
        Parsed::File(info) => info.into_bundled(),
        Parsed::DisabledLanguage => {
            return Err(ModelError::Adapter {
                path: adapter.file.abs.clone(),
                message: format!(
                    "resource {:?}: the path names a disabled language",
                    added.path
                ),
            });
        }
    };
    Ok(CapturedResource {
        lang: added.lang,
        file: adapter.file.clone(),
        info,
        adapter: Some(Arc::new(added)),
    })
}

/// The page `adapter` added in `lang` with an `add_page` map: Go gives it the adapter's file
/// (`.File` is the adapter) and the path `/<path>/index.<suffix>` (`_index` for branch kinds),
/// so it is a bundle named after its last path element.
pub(crate) fn adapter_page(
    adapter: &ContentAdapter,
    lang: LangIdx,
    page: Arc<AdapterPage>,
    parser: &PathParser,
) -> Result<CapturedPage, ModelError> {
    let path = page.source_path();
    let info = match parser.parse(Component::Content, &path) {
        Parsed::File(info) if info.kind.is_page() => *info,
        _ => {
            return Err(ModelError::Page {
                path: adapter.file.abs.clone(),
                source: ssg_page::PageError::Field {
                    key: "path".to_owned(),
                    message: format!("{:?} is not a content path", page.path),
                },
            });
        }
    };
    Ok(CapturedPage {
        lang,
        kind: Some(page.kind),
        bundled: false,
        source: SourceFile {
            file: adapter.file.clone(),
            file_info: adapter.info.clone(),
            info,
            front_matter: None,
            text: Arc::from(page.content.as_str()),
            body_offset: 0,
            mod_time: None,
        },
        params: page.fields.clone(),
        cascade: page.cascade.clone(),
        adapter: Some(page),
    })
}

/// The path of a page moved by front matter `path`: a path without an extension names a
/// bundle (`/custom/moved` → `/custom/moved/index.md`, `_index.md` for branch kinds).
fn moved_path(
    path: &str,
    kind: Option<PageKind>,
    file: &PathInfo,
    parser: &PathParser,
    abs: &Path,
) -> Result<PathInfo, ModelError> {
    let has_ext = !ssg_base::paths::ext(ssg_base::paths::base(path)).is_empty();
    let full = if has_ext {
        path.to_owned()
    } else {
        let branch = kind.map_or(file.kind == BundleKind::Branch, PageKind::is_branch);
        let ext = if file.ext.is_empty() { "md" } else { &file.ext };
        let index = if branch { "_index" } else { "index" };
        format!("{}/{index}.{ext}", path.trim_end_matches('/'))
    };
    match parser.parse(Component::Content, &full) {
        Parsed::File(info) if info.kind.is_page() => Ok(*info),
        _ => Err(ModelError::Page {
            path: abs.to_owned(),
            source: ssg_page::PageError::Field {
                key: "path".to_owned(),
                message: format!("{path:?} is not a content path"),
            },
        }),
    }
}
