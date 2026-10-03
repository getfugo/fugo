//! The alias plan: the redirect files of front matter `aliases` and the main-language redirect.
//! (The `page/1/` redirects of paginated pages depend on what templates paginate; see
//! [`crate::pager_alias`].)

use std::collections::BTreeSet;

use ssg_base::paths::{clean, join};
use ssg_base::{FormatId, LangIdx, OutputPath, PageId};
use ssg_config::Config;
use ssg_config::site::{AliasPolicy, RedirectPolicy};

use crate::NavError;
use crate::model::{NavModel, Rendering};

/// Why an alias file is planned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AliasKind {
    /// Front matter `aliases` of the page.
    FrontMatter,
    /// The redirect to the default language's home page (`/en/` or `/`).
    LanguageRedirect,
}

/// One redirect file: `from` redirects to the page `to` in output format `format`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AliasPlan {
    pub from: OutputPath,
    pub to: PageId,
    pub format: FormatId,
    pub kind: AliasKind,
}

/// Why an alias cannot be written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AliasProblem {
    Empty,
    /// It would replace the site's home page.
    Root,
    /// It climbs out of the publish directory.
    Traversal,
}

/// The redirect file of alias `alias`: the cleaned path, with `index.html` appended unless it
/// names an `.html` file. `allow_root` admits `/` (the main-language redirect).
///
/// # Errors
/// An empty alias, the root (unless allowed), or one climbing out of the publish directory.
pub fn alias_target(alias: &str, allow_root: bool) -> Result<OutputPath, NavError> {
    let err = |problem| NavError::Alias {
        alias: alias.to_owned(),
        problem,
    };
    if alias.is_empty() {
        return Err(err(AliasProblem::Empty));
    }
    let cleaned = clean(alias);
    if cleaned == "/" && !allow_root {
        return Err(err(AliasProblem::Root));
    }
    if cleaned.split('/').next() == Some("..") {
        return Err(err(AliasProblem::Traversal));
    }
    let mut path = cleaned.trim_start_matches('/').to_owned();
    if !path.ends_with(".html") {
        path.push_str("/index.html");
    }
    Ok(OutputPath::new(&path))
}

/// The alias files of language `lang`'s pages: for every rendered page with `aliases`, one
/// file per alias and HTML output format (formats sharing a `path` share the files). A
/// relative alias is resolved against the page's directory, an absolute one is placed below
/// the format's `path`; sections with ugly URLs get `.html` files, and multihost sites keep each
/// language's aliases in its directory.
///
/// # Errors
/// An alias that cannot be written ([`alias_target`]).
pub fn page_aliases(
    m: &impl NavModel,
    cfg: &Config,
    lang: LangIdx,
) -> Result<Vec<AliasPlan>, NavError> {
    let site = &cfg.sites[lang];
    if site.aliases == AliasPolicy::Disabled {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for id in m.tree_pages(lang) {
        let p = m.page(id);
        if p.aliases.is_empty() || p.rendering == Rendering::NotRendered {
            continue;
        }
        // Relative aliases are siblings of the page (its resource directory's parent).
        let base = p
            .outputs
            .first()
            .map(|(_, t)| {
                t.resources
                    .as_ref()
                    .map_or_else(|| t.link.to_string(), |r| r.link.to_string())
            })
            .unwrap_or_default();
        let parent = join(&[&base, ".."]);
        let mut seen = BTreeSet::new();
        for (fid, _) in p.outputs {
            let f = cfg.output_formats.get(*fid);
            if !f.is_html || !seen.insert(f.path.as_str()) {
                continue;
            }
            for a in p.aliases {
                let mut path = if a.starts_with('/') {
                    join(&[&f.path, a])
                } else {
                    join(&[&parent, a])
                };
                if site.urls.ugly.in_section(p.section) && !path.ends_with(".html") {
                    path.push_str(".html");
                }
                let key = &site.language.key;
                if cfg.multihost && !path.starts_with(&format!("/{key}")) {
                    path = join(&[key, &path]);
                }
                out.push(AliasPlan {
                    from: alias_target(&path, false)?,
                    to: id,
                    format: *fid,
                    kind: AliasKind::FrontMatter,
                });
            }
        }
    }
    Ok(out)
}

/// The redirect to the default language's home page of a multilingual (or
/// `defaultContentLanguageInSubdir`) single-host site: `/` → `/en/` when the default language
/// is in a subdirectory, else `/en/` → `/`. `None` for multihost sites, when
/// `disableDefaultLanguageRedirect` is set, or without an `html` format or home page.
#[must_use]
pub fn language_redirect(m: &impl NavModel, cfg: &Config) -> Option<AliasPlan> {
    let disabled = cfg.default_language_redirect == RedirectPolicy::Disabled;
    let multilingual = cfg.sites.len() > 1;
    if disabled || cfg.multihost || !(cfg.default_language_in_subdir || multilingual) {
        return None;
    }
    let lang = cfg.sites.ids().next()?;
    let from = if cfg.default_language_in_subdir {
        "/".to_owned()
    } else {
        cfg.sites[lang].language.key.clone()
    };
    Some(AliasPlan {
        from: alias_target(&from, true).ok()?,
        to: m.home(lang)?,
        format: cfg.output_formats.by_name("html")?,
        kind: AliasKind::LanguageRedirect,
    })
}

/// Every alias file of the site: the pages' aliases per language, then the main-language
/// redirect.
///
/// # Errors
/// As [`page_aliases`].
pub fn alias_plan(m: &impl NavModel, cfg: &Config) -> Result<Vec<AliasPlan>, NavError> {
    let mut out = Vec::new();
    for lang in cfg.sites.ids() {
        out.extend(page_aliases(m, cfg, lang)?);
    }
    out.extend(language_redirect(m, cfg));
    Ok(out)
}
