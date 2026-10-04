//! A generation of the views: the site and page values of one build, made on demand and kept.

use super::*;

impl ViewGeneration {
    /// A generation: the Meta one (`contents: None`) or the Full one of a variant.
    pub(super) fn build(
        shared: &Arc<Shared>,
        contents: Option<&Contents>,
        memo: &ContentMemo,
    ) -> Self {
        let model = &shared.model;
        let summaries: IdVec<PageId, tera::Value> = match contents {
            None => shared.base.clone(),
            Some(c) => model
                .pages
                .as_slice()
                .par_iter()
                .map(|p| {
                    let content = memo.content(c.get(p.id).and_then(Option::as_ref));
                    merged(&shared.base[p.id], content)
                })
                .collect::<Vec<_>>()
                .into(),
        };
        let full = model.pages.iter().map(|_| OnceLock::new()).collect();
        let mut g = Self {
            shared: Arc::clone(shared),
            summaries,
            links: shared.links.clone(),
            full,
            terms: IdVec::new(),
            sites: IdVec::new(),
        };
        g.terms = model
            .sites
            .iter()
            .map(|site| {
                site.taxonomies
                    .iter()
                    .map(|t| {
                        t.listed_terms(model)
                            .map(|(_, term)| {
                                let key = t.key_of(term);
                                let pages: Vec<PageId> =
                                    term.members.iter().map(|w| w.page).collect();
                                let v = TermEntryView {
                                    name: term.term.clone(),
                                    key: key.clone(),
                                    count: pages.len(),
                                    page: g.summaries[term.page].clone(),
                                    pages: g.list(&pages),
                                };
                                (key, tera::Value::from_serializable(&v))
                            })
                            .collect()
                    })
                    .collect()
            })
            .collect();
        g.sites = model.sites.ids().map(|l| g.site_value(l)).collect();
        g
    }

    /// A list value of the summaries of `ids`.
    #[must_use]
    pub fn list(&self, ids: &[PageId]) -> tera::Value {
        if ids.is_empty() {
            return self.shared.empty.clone();
        }
        tera::Value::from(
            ids.iter()
                .map(|&p| self.summaries[p].clone())
                .collect::<Vec<_>>(),
        )
    }

    /// The summary of `id`, or none.
    #[must_use]
    pub fn opt(&self, id: Option<PageId>) -> tera::Value {
        id.map_or_else(tera::Value::none, |p| self.summaries[p].clone())
    }

    pub(super) fn site_value(&self, lang: LangIdx) -> tera::Value {
        let sh = &self.shared;
        let model = &sh.model;
        let cfg = &model.config;
        let site_cfg = &cfg.sites[lang];
        let site = &model.sites[lang];
        let mut taxonomies = tera::Map::new();
        for (t, terms) in site.taxonomies.iter().zip(&self.terms[lang]) {
            let mut m = tera::Map::new();
            for (key, v) in terms {
                m.insert(key.clone().into(), v.clone());
            }
            taxonomies.insert(t.def.plural.clone().into(), tera::Value::from(m));
        }
        tera::Value::from_serializable(&SiteView {
            title: site_cfg.title.clone(),
            base_url: site_cfg.base_url.as_str().to_owned(),
            lang: site_cfg.language.key.clone(),
            language_code: site_cfg.language.code.clone(),
            language: sh.languages[lang].clone(),
            languages: sh.all_languages.clone(),
            is_multilingual: cfg.sites.len() > 1,
            copyright: site_cfg.copyright.clone(),
            params: params_value(&site_cfg.params),
            data: sh.data.clone(),
            home: self.summaries[site.home].clone(),
            pages: self.list(&site.pages),
            regular_pages: self.list(&site.regular_pages),
            all_pages: self.list(&sh.all_pages),
            sections: self.list(&model.pages[site.home].sections),
            main_sections: site.main_sections.clone(),
            taxonomies: tera::Value::from(taxonomies),
            menus: sh.menu_values[lang].clone(),
            last_mod: site.last_mod.as_ref().map(DateView::new),
            config: sh.configs[lang].clone(),
            sitemap_abs_url: sh.sitemap_abs_urls[lang].clone(),
            server_port: site_cfg.base_url.port().unwrap_or(0),
        })
    }

    /// The full value of page `id`: its summary plus its relations (built on first use).
    #[must_use]
    pub fn full(&self, id: PageId) -> tera::Value {
        self.full[id].get_or_init(|| self.full_value(id)).clone()
    }

    /// The full value of page `id` without keeping it: what a layout job renders its own page
    /// with (the value is the job's; [`full`](Self::full) keeps the values `deref` and
    /// `get_page` share). Equal to `full(id)`; the cached one is reused when it exists.
    #[must_use]
    pub fn page_value(&self, id: PageId) -> tera::Value {
        if let Some(v) = self.full[id].get() {
            return v.clone();
        }
        self.full_value(id)
    }

    /// `.RegularPagesRecursive`, as Go: for the home page and a section, the regular pages
    /// below it that are listed locally (so `build.list = "local"` pages of nested sections
    /// too, which `site.regular_pages` leaves out); else `.RegularPages`.
    pub(super) fn regular_pages_recursive(&self, id: PageId) -> Vec<PageId> {
        let model = &self.shared.model;
        let p = &model.pages[id];
        let local = &model.sites[p.lang].regular_pages_local;
        match p.kind {
            PageKind::Home => local.clone(),
            PageKind::Section => local
                .iter()
                .copied()
                .filter(|&q| model.is_ancestor(id, q))
                .collect(),
            _ => p.regular_pages.clone(),
        }
    }

    pub(super) fn full_value(&self, id: PageId) -> tera::Value {
        let sh = &self.shared;
        let model = &sh.model;
        let p = &model.pages[id];
        let site = &model.sites[p.lang];
        let summary = &self.summaries[id];
        let taxonomy = p
            .taxonomy
            .filter(|_| p.kind == PageKind::Taxonomy)
            .map(|t| {
                let def = &site.taxonomies[t].def;
                TaxonomyView {
                    singular: def.singular.clone(),
                    plural: def.plural.clone(),
                    terms: tera::Value::from(
                        self.terms[p.lang][t]
                            .iter()
                            .map(|(_, v)| v.clone())
                            .collect::<Vec<_>>(),
                    ),
                }
            });
        let term = match (p.kind, p.taxonomy, p.term) {
            (PageKind::Term, Some(t), Some(term)) => {
                let tx = &site.taxonomies[t];
                let term = &tx.terms[term];
                Some(TermView {
                    name: p.name().to_owned(),
                    term: term.term.clone(),
                    key: tx.key_of(term),
                    singular: tx.def.singular.clone(),
                    plural: tx.def.plural.clone(),
                })
            }
            _ => None,
        };
        // Every output format of the page but the first, less the `notAlternative` ones.
        let formats = summary
            .as_map()
            .and_then(|m| m.get(&tera::value::Key::Str("output_formats")))
            .and_then(tera::Value::as_map);
        let alternative = p
            .urls
            .iter()
            .filter(|u| u.links.is_some())
            .skip(1)
            .map(|u| model.config.output_formats.get(u.format))
            .filter(|f| f.listing != Listing::NotAlternative)
            .filter_map(|f| formats?.get(&tera::value::Key::Str(&f.name)).cloned())
            .collect::<Vec<_>>();
        let translations: Vec<PageId> = p
            .translations
            .iter()
            .copied()
            .filter(|&q| q != id)
            .collect();
        let (prev, next) = sh.prev_next[id];
        let (prev_in_section, next_in_section) = sh.prev_next_in_section[id];
        let r = PageRelations {
            parent: p.parent.map(|q| self.summaries[q].clone()),
            current_section: self.summaries[p.current_section].clone(),
            first_section: self.summaries[p.first_section].clone(),
            ancestors: self.list(&p.ancestors),
            pages: self.list(&p.pages),
            regular_pages: self.list(&p.regular_pages),
            regular_pages_recursive: self.list(&self.regular_pages_recursive(id)),
            sections: self.list(&p.sections),
            prev: prev.map(|q| self.summaries[q].clone()),
            next: next.map(|q| self.summaries[q].clone()),
            prev_in_section: prev_in_section.map(|q| self.summaries[q].clone()),
            next_in_section: next_in_section.map(|q| self.summaries[q].clone()),
            translations: self.list(&translations),
            all_translations: self.list(&p.translations),
            alternative_output_formats: tera::Value::from(alternative),
            taxonomy,
            term,
        };
        merged(summary, to_map(&r))
    }

    /// The model the values were built from.
    #[must_use]
    pub fn model(&self) -> &Arc<Model> {
        &self.shared.model
    }
}
