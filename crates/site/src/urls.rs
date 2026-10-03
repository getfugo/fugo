//! Phase B4: every page's output formats, target paths and links (parallel over pages).
//!
//! The formats are the front matter `outputs`, else the site's formats of the page's kind (a
//! standalone page has its own format; a page that is not rendered keeps only the first). For
//! each format, [`ssg_page::target_paths`] gets the page's path, current section, name
//! (slug, else the standalone format's base name, else the path's name), language prefixes,
//! front matter `url` (expanded when it holds a `:` attribute) and expanded `[permalinks]`
//! pattern; [`ssg_page::links`] makes `.RelPermalink` and `.Permalink` of pages that have
//! a link. [`Model::pager_paths`] gives the same for a pager (`/page/2`).

use rayon::prelude::*;
use ssg_base::url::{self, Component, PathCase, SiteUrls};
use ssg_base::{FormatId, PageId, PageKind, paths};
use ssg_config::Config;
use ssg_page::{
    LangPrefix, Links, PageError, PermalinkCtx, PermalinkFile, PermalinkPattern, PermalinkPatterns,
    SourcePath, TargetPaths, UrlInputs, links, target_paths,
};

use crate::nodes::standalone_base_name;
use crate::tree::PageRole;
use crate::{Model, ModelError, Page, PageUrl};

/// Compiles every language's `[permalinks]`.
pub(crate) fn compile_permalinks(m: &mut Model) -> Result<(), ModelError> {
    let cfg = m.config.clone();
    for (lang, site) in cfg.sites.iter_enumerated() {
        m.sites[lang].permalinks =
            PermalinkPatterns::compile(&site.permalinks).map_err(|source| {
                ModelError::Permalinks {
                    lang: site.language.key.clone(),
                    source,
                }
            })?;
    }
    Ok(())
}

/// Sets `formats` and `urls` of every page.
pub(crate) fn assign(m: &mut Model) -> Result<(), ModelError> {
    compile_permalinks(m)?;
    let results: Vec<Result<Outputs, ModelError>> = m
        .pages
        .as_slice()
        .par_iter()
        .map(|p| page_urls(m, p).map_err(|source| node_error(m, p, source)))
        .collect();
    let ids: Vec<_> = m.pages.ids().collect();
    for (id, r) in ids.into_iter().zip(results) {
        let (formats, urls) = r?;
        let p = &mut m.pages[id];
        p.formats = formats;
        p.urls = urls;
    }
    Ok(())
}

fn node_error(m: &Model, p: &Page, source: PageError) -> ModelError {
    ModelError::Node {
        path: p.path(),
        lang: m.config.sites[p.lang].language.key.clone(),
        source,
    }
}

impl Model {
    /// The output file and link of pager `pager` (`/page/2`, as `pagination.path` makes it) of
    /// page `id` in `format`.
    ///
    /// # Errors
    /// The page's URL cannot be made (as when the model was loaded).
    pub fn pager_paths(
        &self,
        id: PageId,
        format: FormatId,
        pager: &str,
    ) -> Result<TargetPaths, ModelError> {
        let p = &self.pages[id];
        let inputs = PageInputs::new(self, p).map_err(|e| node_error(self, p, e))?;
        inputs
            .paths(self, p, format, Some(pager))
            .map_err(|e| node_error(self, p, e))
    }
}

/// Go's `url.QueryUnescape`, keeping the input when it is not a valid escape.
fn query_unescape(s: &str) -> String {
    url::unescape(s, Component::QueryComponent)
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .unwrap_or_else(|| s.to_owned())
}

/// The name of a file without extension and language (`index`, `_index`, `my-post`).
fn translation_base_name(p: &Page) -> Option<&str> {
    let s = p.source.as_ref()?;
    let stem = paths::trim_ext(paths::base(&s.file.rel));
    Some(if s.file_info.lang.is_some() {
        paths::trim_ext(stem)
    } else {
        stem
    })
}

/// The formats a page is written in.
fn formats(cfg: &Config, p: &Page) -> Vec<FormatId> {
    if let Some(f) = p.standalone {
        return vec![f];
    }
    let mut formats = match &p.meta.outputs {
        Some(o) if !o.is_empty() => o.clone(),
        _ => cfg.sites[p.lang].outputs.get(p.kind).to_vec(),
    };
    if !p.rendered() {
        formats.truncate(1);
    }
    formats
}

/// What a page's target paths are made of, in every format.
struct PageInputs {
    urls: SiteUrls,
    source: SourcePath,
    section: String,
    base_name: String,
    prefix_file: String,
    prefix_link: String,
    force_prefix: bool,
    url: Option<String>,
    permalink: Option<String>,
    site_ugly: bool,
}

impl PageInputs {
    fn new(m: &Model, p: &Page) -> Result<Self, PageError> {
        let cfg = &m.config;
        let site = &cfg.sites[p.lang];
        let urls = site.site_urls();
        let case = site.urls.path_case;
        let section = &m.pages[p.current_section];
        let section_path = match case {
            PathCase::Lower => section.key.to_path(),
            PathCase::Preserve => section.path_info.original.base.clone(),
        };
        let section_path = if section_path.is_empty() {
            "/".to_owned()
        } else {
            section_path
        };
        let sections: Vec<&str> = section.key.segments().collect();
        let dir = p
            .source
            .as_ref()
            .map(|s| format!("{}/", paths::dir(&s.file.rel)))
            .unwrap_or_default();
        let slug = p.meta.slug.as_deref().unwrap_or_default();
        let pctx = PermalinkCtx {
            date: p.meta.dates.date.as_ref(),
            title: &p.title,
            slug,
            section: &p.section,
            sections: &sections,
            file: translation_base_name(p).map(|name| PermalinkFile {
                translation_base_name: name,
                dir: if dir == "/" { "" } else { &dir },
            }),
            content_base_name: &p.path_info.original.name,
            urls: &urls,
        };
        let url = match p.meta.url.as_deref().filter(|u| !u.is_empty()) {
            Some(u) if u.contains(':') => {
                let expanded = query_unescape(&PermalinkPattern::parse(u)?.expand(&pctx)?);
                Some(if expanded.is_empty() {
                    u.to_owned()
                } else {
                    expanded
                })
            }
            Some(u) => Some(u.to_owned()),
            None => None,
        };
        let permalink = match m.sites[p.lang].permalinks.get(p.kind, &p.section) {
            Some(pattern) => {
                let mut s = query_unescape(&pattern.expand(&pctx)?);
                if s.ends_with("//") {
                    s.pop();
                }
                Some(s)
            }
            None => None,
        };
        let base_name = if !slug.is_empty() {
            slug.to_owned()
        } else if let Some(f) = p.standalone {
            standalone_base_name(cfg, p.lang, p.kind, f)
        } else {
            match case {
                PathCase::Lower => p.path_info.name.clone(),
                PathCase::Preserve => p.path_info.original.name.clone(),
            }
        };
        // Sitemaps of a multilingual site are always in their language's directory; multihost
        // sites write every language's files to its own directory, with root-relative links.
        let in_subdir = p.kind == PageKind::Sitemap;
        let lang_key = site.language.key.as_str();
        let prefix_link = if cfg.multihost {
            ""
        } else if cfg.sites.len() > 1 && in_subdir {
            lang_key
        } else {
            site.language.url_prefix.as_str()
        };
        let prefix_file = if cfg.multihost { lang_key } else { prefix_link };
        Ok(Self {
            source: SourcePath::from_path_info(&p.path_info, case),
            section: section_path,
            base_name,
            prefix_file: prefix_file.to_owned(),
            prefix_link: prefix_link.to_owned(),
            force_prefix: cfg.multihost || in_subdir,
            url,
            permalink,
            site_ugly: site.urls.ugly.in_section(&p.section),
            urls,
        })
    }

    fn paths(
        &self,
        m: &Model,
        p: &Page,
        format: FormatId,
        pager: Option<&str>,
    ) -> Result<TargetPaths, PageError> {
        let f = m.config.output_formats.get(format);
        let suffix = m.config.media_types.get(f.media_type).full_suffix();
        target_paths(&UrlInputs {
            kind: p.kind,
            format: f,
            suffix: &suffix,
            source: &self.source,
            section: &self.section,
            base_name: &self.base_name,
            prefix: LangPrefix {
                file: &self.prefix_file,
                link: &self.prefix_link,
                force: self.force_prefix,
            },
            url: self.url.as_deref(),
            pager,
            permalink: self.permalink.as_deref(),
            site_ugly: self.site_ugly,
            urls: &self.urls,
        })
    }

    fn links(&self, m: &Model, paths: &TargetPaths, format: FormatId) -> Result<Links, PageError> {
        links(paths, &self.urls, m.config.output_formats.get(format))
    }
}

/// A page's formats and its output in each.
type Outputs = (Vec<FormatId>, Vec<PageUrl>);

fn page_urls(m: &Model, p: &Page) -> Result<Outputs, PageError> {
    let formats = formats(&m.config, p);
    if formats.is_empty() {
        return Ok((formats, Vec::new()));
    }
    let inputs = PageInputs::new(m, p)?;
    let has_link = p.linked() && p.role == PageRole::Standalone;
    let mut urls = Vec::with_capacity(formats.len());
    for &format in &formats {
        let paths = inputs.paths(m, p, format, None)?;
        let links = if has_link {
            Some(inputs.links(m, &paths, format)?)
        } else {
            None
        };
        urls.push(PageUrl {
            format,
            paths,
            links,
        });
    }
    Ok((formats, urls))
}
