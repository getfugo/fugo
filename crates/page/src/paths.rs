//! Output file paths and links of a page in one output format.
//!
//! [`target_paths`] decides, for a page and format, the file written under `publishDir`, the
//! link to it, and where its bundle resources go. [`links`] turns the link into
//! `.RelPermalink` and `.Permalink`.

use ssg_base::PageKind;
use ssg_base::paths::{self, ContentKey, OutputPath, Permalink, UrlPath};
use ssg_base::url::{self, Component, PathCase, SiteUrls};
use ssg_config::OutputFormat;
use ssg_config::output::{Placement, UglyPolicy};
use ssg_vfs::PathInfo;

use crate::PageError;

/// Whether a source path is a bundle (a directory with an index file) or a single file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PathShape {
    Bundle,
    #[default]
    File,
}

/// The parts of a page's source path that its URL is made of.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourcePath {
    /// The directory holding the page: `/posts` for `/posts/a.md` and `/posts/a/index.md`.
    pub dir: String,
    /// The page's name: the file name without identifiers, or the bundle directory's name.
    pub name: String,
    pub shape: PathShape,
}

impl SourcePath {
    /// The source path of a parsed content file, in normalised spelling or, when paths keep
    /// their case (`disablePathToLower`), as written.
    #[must_use]
    pub fn from_path_info(pi: &PathInfo, case: PathCase) -> Self {
        let (path, name) = match case {
            PathCase::Lower => (pi.path.as_str(), pi.name.as_str()),
            PathCase::Preserve => (pi.original.path.as_str(), pi.original.name.as_str()),
        };
        let shape = if pi.kind.is_bundle() {
            PathShape::Bundle
        } else {
            PathShape::File
        };
        let file_dir = paths::dir(path);
        let dir = match shape {
            PathShape::Bundle => paths::dir(file_dir),
            PathShape::File => file_dir,
        };
        Self {
            dir: dir.to_owned(),
            name: name.to_owned(),
            shape,
        }
    }

    /// The source path of a page without a file (taxonomy and term pages made from front
    /// matter values): a bundle at `key`.
    #[must_use]
    pub fn from_key(key: &ContentKey) -> Self {
        let path = key.to_path();
        Self {
            dir: paths::dir(&path).to_owned(),
            name: paths::base(&path).trim_start_matches('/').to_owned(),
            shape: PathShape::Bundle,
        }
    }
}

/// The language prefix of file paths and links (`th`; empty for none).
#[derive(Clone, Copy, Debug, Default)]
pub struct LangPrefix<'a> {
    pub file: &'a str,
    pub link: &'a str,
    /// Also for formats placed at the root (sitemaps of a multilingual site, multihost).
    pub force: bool,
}

/// Everything [`target_paths`] reads.
#[derive(Clone, Copy, Debug)]
pub struct UrlInputs<'a> {
    pub kind: PageKind,
    pub format: &'a OutputFormat,
    /// The format's media type suffix with its delimiter (`.html`; empty for none).
    pub suffix: &'a str,
    pub source: &'a SourcePath,
    /// The page's current section as a path (`/posts`; `/` at the root). For branch pages
    /// this is the page itself.
    pub section: &'a str,
    /// The slug, else the standalone format's base name (`404`), else the source name.
    pub base_name: &'a str,
    pub prefix: LangPrefix<'a>,
    /// Front matter `url`.
    pub url: Option<&'a str>,
    /// The pager path (`/page/2`).
    pub pager: Option<&'a str>,
    /// The expanded `[permalinks]` pattern.
    pub permalink: Option<&'a str>,
    /// `uglyURLs` for the page's section.
    pub site_ugly: bool,
    pub urls: &'a SiteUrls,
}

/// Where bundle resources of a page go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceBase {
    /// The directory resources are written to (`/posts/one`).
    pub target: OutputPath,
    /// The link directory resources are linked from.
    pub link: UrlPath,
}

/// The output file and link of a page in one format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetPaths {
    /// The file under `publishDir` (`/posts/one/index.html`).
    pub target: OutputPath,
    /// The link, relative to the site root and unescaped (`/posts/olé/`).
    pub link: UrlPath,
    /// `None` for formats that have no resources (404, sitemap, robots.txt).
    pub resources: Option<ResourceBase>,
}

/// `.RelPermalink` and `.Permalink` of a page in one format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Links {
    /// Unescaped, with the base URL's path in front (unless URLs are canonified).
    pub rel_permalink: UrlPath,
    pub permalink: Permalink,
}

/// Formats written as one file per site, without resources.
fn is_standalone_format(f: &OutputFormat) -> bool {
    matches!(f.name.as_str(), "404" | "sitemap" | "robots")
}

/// The path elements of an output file, with the state that shapes its link.
#[derive(Default)]
struct Builder {
    elements: Vec<String>,
    suffix: String,
    prefix_path: String,
    prefix_link: String,
    /// The link drops the last element (`index.html`).
    link_drops_index: bool,
    no_resources: bool,
}

impl Builder {
    fn add(&mut self, e: &str) {
        if !e.is_empty() && e != "/" {
            self.elements.push(e.to_owned());
        }
    }

    fn concat_last(&mut self, s: &str) {
        match self.elements.last_mut() {
            None => self.add(s),
            Some(last) if last.is_empty() => s.clone_into(last),
            Some(last) => {
                if last.ends_with('/') {
                    last.pop();
                }
                last.push_str(s);
            }
        }
    }

    fn last(&self) -> &str {
        self.elements.last().map_or("", String::as_str)
    }

    fn is_html_index(&self) -> bool {
        self.last() == "index.html"
    }

    fn joined(&self, drop_last: bool) -> String {
        let n = self.elements.len() - usize::from(drop_last && !self.elements.is_empty());
        let parts: Vec<&str> = self.elements[..n].iter().map(String::as_str).collect();
        let p = paths::join(&parts);
        if p.starts_with('/') {
            p
        } else {
            format!("/{p}")
        }
    }

    fn with_prefix(prefix: &str, p: String) -> String {
        if prefix.is_empty() {
            p
        } else {
            format!("/{prefix}{p}")
        }
    }

    fn dir_base(&self, index_name: &str) -> String {
        let mut dir = self.joined(false);
        if self.last().starts_with(&format!("{index_name}.")) {
            dir = paths::dir(&dir).to_owned();
        } else if let Some(d) = dir.strip_suffix(self.suffix.as_str()) {
            dir = d.to_owned();
        }
        if dir == "/" { String::new() } else { dir }
    }
}

/// Decides the output file, link and resource directory of a page in one format (Go's
/// `CreateTargetPaths` semantics: pretty URLs `/a/index.html` → `/a/`, ugly URLs `/a.html`,
/// language prefixes, front matter `url` kept verbatim, every other element sanitised).
///
/// # Errors
/// A link that is not a URL reference (a front matter `url` with a broken `%` escape).
pub fn target_paths(i: &UrlInputs<'_>) -> Result<TargetPaths, PageError> {
    let f = i.format;
    let root = f.placement == Placement::Root;
    let mut url = i.url.filter(|u| !u.is_empty()).map(str::to_owned);
    let force_prefix =
        i.prefix.force || (!root && url.as_deref().is_some_and(|u| !u.starts_with('/')));
    if let Some(u) = &mut url
        && u.contains("..")
    {
        *u = paths::join(&["/", u]);
    }
    let (prefix_file, prefix_link) = if root && !force_prefix {
        ("", "")
    } else {
        (i.prefix.file, i.prefix.link)
    };
    let permalink = i.permalink.filter(|p| !p.is_empty());
    let pager = i.pager.unwrap_or_default();

    let mut b = Builder {
        suffix: i.suffix.to_owned(),
        ..Builder::default()
    };
    let mut needs_index = true;
    let mut ugly = match f.ugly {
        UglyPolicy::Always => true,
        UglyPolicy::Never => false,
        UglyPolicy::Inherit => i.site_ugly,
    };
    let base_is_format_name =
        i.source.shape == PathShape::File && !i.base_name.is_empty() && i.base_name == f.base_name;
    let ugly_index_kind = matches!(
        i.kind,
        PageKind::Home | PageKind::Section | PageKind::Taxonomy
    ) && ugly;
    if permalink.is_none() && base_is_format_name {
        ugly = true;
    }

    b.add(&f.path);
    if is_standalone_format(f) {
        b.no_resources = true;
    } else if i.kind != PageKind::Page && url.is_none() && i.section != "/" {
        b.add(permalink.unwrap_or(i.section));
        needs_index = false;
    }

    let index_file = format!("{}{}", f.base_name, i.suffix);
    match &url {
        Some(u) if i.kind != PageKind::Home => {
            for field in u.split('/') {
                b.add(field);
            }
            b.add(pager);
            if u.ends_with('/') || !u.contains('.') {
                b.add(&index_file);
            } else {
                paths::ext(u).clone_into(&mut b.suffix);
            }
            b.link_drops_index = b.is_html_index();
            if force_prefix {
                if !prefix_file.is_empty() && !u.starts_with(&format!("/{prefix_file}")) {
                    prefix_file.clone_into(&mut b.prefix_path);
                }
                if !prefix_link.is_empty() && !u.starts_with(&format!("/{prefix_link}")) {
                    prefix_link.clone_into(&mut b.prefix_link);
                }
            }
        }
        _ if !i.kind.is_branch() => {
            if let Some(p) = permalink {
                b.add(p);
            } else {
                b.add(&i.source.dir);
                b.add(if i.base_name.is_empty() {
                    &i.source.name
                } else {
                    i.base_name
                });
            }
            b.add(pager);
            if ugly {
                let s = b.suffix.clone();
                b.concat_last(&s);
            } else {
                b.add(&index_file);
            }
            b.link_drops_index = b.is_html_index();
            prefix_file.clone_into(&mut b.prefix_path);
            prefix_link.clone_into(&mut b.prefix_link);
        }
        _ => {
            b.add(pager);
            needs_index = needs_index && pager.is_empty();
            if needs_index || !ugly || ugly_index_kind {
                b.add(&index_file);
            } else {
                let s = b.suffix.clone();
                b.concat_last(&s);
            }
            b.link_drops_index = !ugly_index_kind && b.is_html_index();
            prefix_file.clone_into(&mut b.prefix_path);
            prefix_link.clone_into(&mut b.prefix_link);
        }
    }

    if url.is_none() {
        for e in &mut b.elements {
            *e = i.urls.make_path_sanitized(e);
        }
    }

    // The link.
    let mut link = b.joined(b.link_drops_index);
    if base_is_format_name && let Some(l) = link.strip_suffix(i.base_name) {
        link = l.to_owned();
    }
    link = Builder::with_prefix(&b.prefix_link, link);
    if b.link_drops_index && !link.ends_with('/') {
        link.push('/');
    }
    let link = decoded_link(&link, url.is_some())?;

    let target = Builder::with_prefix(&b.prefix_path, b.joined(false));
    let resources = (!b.no_resources).then(|| {
        let dir = b.dir_base(&f.base_name);
        ResourceBase {
            target: OutputPath::new(&Builder::with_prefix(&b.prefix_path, dir.clone())),
            link: UrlPath::new(&Builder::with_prefix(&b.prefix_link, dir)),
        }
    });
    Ok(TargetPaths {
        target: OutputPath::new(&target),
        link,
        resources,
    })
}

/// The link as a URL path: `#` is part of the path, and a front matter `url` keeps its
/// escapes and query (which are decoded here and re-escaped by [`UrlPath::escaped`]).
fn decoded_link(link: &str, from_url: bool) -> Result<UrlPath, PageError> {
    let link = link.replace('#', "%23");
    let err = |source| PageError::Link {
        link: link.clone(),
        source,
    };
    let escaped = if from_url {
        url::url_escape(&link)
    } else {
        url::path_escape(&link)
    }
    .map_err(err)?;
    let decoded = url::unescape(&escaped, Component::Path).map_err(err)?;
    let decoded = String::from_utf8(decoded)
        .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());
    Ok(UrlPath::new(if decoded.is_empty() {
        "/"
    } else {
        &decoded
    }))
}

/// `.RelPermalink` and `.Permalink` of a page in `format` (whose `protocol`, such as
/// `webcal://`, replaces the base URL's scheme).
///
/// # Errors
/// A protocol that cannot replace the base URL's scheme.
pub fn links(t: &TargetPaths, urls: &SiteUrls, format: &OutputFormat) -> Result<Links, PageError> {
    let rel = urls.prepend_base_path(t.link.as_str());
    let base = if format.protocol.is_empty() {
        urls.base_url.clone()
    } else {
        urls.base_url
            .with_protocol(&format.protocol)
            .map_err(|source| PageError::Link {
                link: t.link.to_string(),
                source,
            })?
    };
    Ok(Links {
        rel_permalink: UrlPath::new(&rel),
        permalink: Permalink::new(&base, &t.link),
    })
}
