//! Phase B3: the pages Go makes itself, and the order of the structure phases.
//!
//! Per language, in this order (Go's): a taxonomy page for every configured taxonomy that
//! has none; a section page for every root section that has none, and the home page if there
//! is none; the standalone pages (404; robots.txt and the sitemap index once per build, on the
//! default language, unless multihost; a sitemap per language); then the term pages
//! (`taxonomy`). Made pages get the cascade in force at their key and go through the build
//! filter like content pages (a switched-off node stays for the structure).

use ssg_base::paths::ContentKey;
use ssg_base::{FormatId, LangIdx, PageId, PageKind};
use ssg_config::Config;
use ssg_page::{Cjk, DateResolver, MetaCtx, meta_from_params};
use ssg_vfs::{Component, Parsed, PathInfo, PathParser};

use crate::filter::{self, Verdict};
use crate::refs::RefIndex;
use crate::tree::PageRole;
use crate::{
    LoadModelOptions, Model, ModelError, Page, Removed, meta, relations, resources, taxonomy,
    translations, urls,
};

/// Phases B3–B5 over a model whose content pages are loaded (see the crate docs).
pub(crate) fn assemble(
    m: &mut Model,
    o: &LoadModelOptions,
    removed: &[Removed],
) -> Result<(), ModelError> {
    let cfg = m.config.clone();
    let parser = m.refs.parser().clone();
    let maker = Maker {
        cfg: &cfg,
        parser: &parser,
        o,
    };
    for lang in cfg.sites.ids() {
        maker.missing_taxonomies(m, lang)?;
        maker.missing_root_sections(m, lang)?;
        maker.standalone_pages(m, lang)?;
        taxonomy::assemble_terms(m, &maker, lang, removed)?;
    }
    relations::names(m);
    relations::tree(m);
    relations::node_dates(m, o, removed);
    taxonomy::sort_members(m);
    relations::lists(m);
    translations::assign(m);
    urls::assign(m)?;
    resources::assign(m);
    m.refs = RefIndex::build(m);
    Ok(())
}

/// Makes the pages of one build.
pub(crate) struct Maker<'a> {
    pub cfg: &'a Config,
    pub parser: &'a PathParser,
    pub o: &'a LoadModelOptions,
}

impl Maker<'_> {
    /// The content path `rel` as the path parser reads it.
    pub fn parse(&self, rel: &str) -> Option<PathInfo> {
        match self.parser.parse(Component::Content, rel) {
            Parsed::File(info) => Some(*info),
            Parsed::DisabledLanguage => None,
        }
    }

    /// Adds a page without a file of `kind` at `key` to `lang`'s tree, with the cascade in force
    /// there; `None` when the build filter removes it.
    pub fn make(
        &self,
        m: &mut Model,
        lang: LangIdx,
        kind: PageKind,
        key: ContentKey,
        path_info: PathInfo,
    ) -> Result<Option<PageId>, ModelError> {
        let site = &self.cfg.sites[lang];
        let params = meta::cascaded_params(self.cfg, &m.sites[lang].cascade, lang, kind, &key);
        let resolver = DateResolver::from_site(site);
        let ctx = MetaCtx {
            kind,
            formats: &self.cfg.output_formats,
            media_types: &self.cfg.media_types,
            sitemap: &site.sitemap,
            cjk_default: Cjk::No,
            ext: "",
            dates: &resolver,
            file: None,
            time_zone: &site.language.time_zone,
        };
        let mut meta = meta_from_params(params, &ctx).map_err(|source| ModelError::Node {
            path: key.to_path(),
            lang: site.language.key.clone(),
            source,
        })?;
        if meta.cjk == Cjk::Detect {
            meta.cjk = Cjk::No;
        }
        match self
            .o
            .verdict(kind, !site.disable_kinds.contains(kind), &meta)
        {
            Verdict::Build => {}
            Verdict::Disable => meta.build = filter::DISABLED,
            Verdict::Remove => return Ok(None),
        }
        let id = m.pages.next_id();
        m.pages.push(Page::new(
            id,
            lang,
            kind,
            PageRole::Standalone,
            key.clone(),
            None,
            path_info,
            meta,
        ));
        m.sites[lang].tree.insert(key, id);
        Ok(Some(id))
    }

    /// A branch page of `kind` for the content path `dir` (`/Posts`) unless its key is taken.
    fn make_branch(
        &self,
        m: &mut Model,
        lang: LangIdx,
        kind: PageKind,
        dir: &str,
    ) -> Result<Option<PageId>, ModelError> {
        let Some(info) = self.parse(&format!("{}/_index.md", dir.trim_end_matches('/'))) else {
            return Ok(None);
        };
        let key = info.key.clone();
        if m.sites[lang].tree.get(&key).is_some() {
            return Ok(None);
        }
        self.make(m, lang, kind, key, info)
    }

    /// A taxonomy page for each configured taxonomy without one (unless taxonomy and term
    /// pages are both disabled).
    fn missing_taxonomies(&self, m: &mut Model, lang: LangIdx) -> Result<(), ModelError> {
        let site = &self.cfg.sites[lang];
        if site.disable_kinds.contains(PageKind::Taxonomy)
            && site.disable_kinds.contains(PageKind::Term)
        {
            return Ok(());
        }
        for idx in taxonomy::views(site) {
            let plural = site.taxonomies[idx].plural.clone();
            if let Some(id) =
                self.make_branch(m, lang, PageKind::Taxonomy, &format!("/{plural}"))?
            {
                m.pages[id].taxonomy = Some(idx);
            }
        }
        Ok(())
    }

    /// A section page for each root section that has pages but no page of its own (named as
    /// its first page writes it), and the home page if there is none.
    fn missing_root_sections(&self, m: &mut Model, lang: LangIdx) -> Result<(), ModelError> {
        let mut sections: Vec<String> = Vec::new();
        for (_, id) in m.sites[lang].tree.iter() {
            let p = &m.pages[id];
            if !matches!(p.kind, PageKind::Page | PageKind::Section)
                || p.path_info.section.is_empty()
                || sections.iter().any(|s| {
                    ContentKey::from_source(s) == ContentKey::from_source(&p.path_info.section)
                })
            {
                continue;
            }
            sections.push(p.path_info.original.section.clone());
        }
        for s in sections {
            self.make_branch(m, lang, PageKind::Section, &format!("/{s}"))?;
        }
        let home = match m.sites[lang].tree.get(&ContentKey::home()) {
            Some(id) => Some(id),
            None => {
                let info = self.parse("/_index.md").expect("the home path is content");
                self.make(m, lang, PageKind::Home, ContentKey::home(), info)?
            }
        };
        if let Some(home) = home {
            m.sites[lang].home = home;
        }
        Ok(())
    }

    /// The pages with one fixed output: 404, robots.txt, the sitemap and the sitemap index.
    fn standalone_pages(&self, m: &mut Model, lang: LangIdx) -> Result<(), ModelError> {
        let cfg = self.cfg;
        let site = &cfg.sites[lang];
        let first = lang == LangIdx::from_raw(0) || cfg.multihost;
        let sitemaps = cfg
            .sites
            .iter()
            .any(|s| !s.disable_kinds.contains(PageKind::Sitemap));
        let index = !cfg.multihost && (cfg.default_language_in_subdir || cfg.sites.len() > 1);
        let pages = [
            (PageKind::NotFound, "404", "404", true),
            (
                PageKind::RobotsTxt,
                "_robots",
                "robots",
                first && site.robots_txt == ssg_config::site::RobotsPolicy::Enabled,
            ),
            (PageKind::Sitemap, "_sitemap", "sitemap", sitemaps),
            (
                PageKind::SitemapIndex,
                "_sitemapindex",
                "sitemapindex",
                sitemaps && index && first,
            ),
        ];
        for (kind, key, format, wanted) in pages {
            let key = ContentKey::from_source(key);
            if !wanted
                || site.disable_kinds.contains(kind)
                || m.sites[lang].tree.get(&key).is_some()
            {
                continue;
            }
            let Some(format) = cfg.output_formats.by_name(format) else {
                continue;
            };
            let suffix = cfg
                .media_types
                .get(cfg.output_formats.get(format).media_type)
                .full_suffix();
            let Some(info) = self.parse(&format!("/{key}{suffix}")) else {
                continue;
            };
            if let Some(id) = self.make(m, lang, kind, key, info)? {
                m.pages[id].standalone = Some(format);
            }
        }
        Ok(())
    }
}

/// The base name of a standalone page's file (`404`; the sitemap's configured file name).
pub(crate) fn standalone_base_name(
    cfg: &Config,
    lang: LangIdx,
    kind: PageKind,
    format: FormatId,
) -> String {
    let f = cfg.output_formats.get(format);
    match kind {
        PageKind::Sitemap | PageKind::SitemapIndex => {
            let name = &cfg.sites[lang].sitemap.filename;
            if name.is_empty() {
                f.base_name.clone()
            } else {
                ssg_base::paths::filename(name).to_owned()
            }
        }
        _ => f.base_name.clone(),
    }
}
