//! Rendering a site in waves: content first, then the pages that wait on others, and the language
//! redirect.

use super::*;

impl Session {
    /// Phase C1: renders the content of every page with a content file (bundled content pages
    /// included) in every hook variant. A second call does nothing.
    ///
    /// # Errors
    /// [`RenderError::Content`] of the first page (in page order) whose content fails.
    pub fn render_content(&self) -> Result<(), RenderError> {
        if self.contents.get().is_some() {
            return Ok(());
        }
        let model = &self.model;
        let per_page: Vec<Vec<Option<Arc<RenderedContent>>>> = model
            .pages
            .as_slice()
            .par_iter()
            .map(|p| {
                self.variants
                    .iter()
                    .map(|&v| {
                        if p.source.is_none() {
                            return Ok(None);
                        }
                        let scope = self.root_scope(p, v);
                        self.content_of(p.id, v, &scope)
                            .map(Some)
                            .map_err(|source| RenderError::Content {
                                page: p.source.as_ref().map_or_else(
                                    || p.key.to_path(),
                                    |s| s.file.abs.display().to_string(),
                                ),
                                source: Box::new(source),
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .collect::<Result<_, _>>()?;
        let mut all = BTreeMap::new();
        for (i, &v) in self.variants.iter().enumerate() {
            let c: Contents = per_page.iter().map(|row| row[i].clone()).collect();
            all.insert(v, c);
        }
        let _ = self.contents.set(all);
        Ok(())
    }

    /// Wave 1 of language `lang` (phase E2):
    /// aliases, then every page × format, then the standalone pages (robots.txt and the
    /// sitemap index with the first language), in [`JobOrder`].
    #[must_use]
    pub fn wave1(&self, lang: LangIdx) -> Vec<Job> {
        let model = &self.model;
        let mut jobs: Vec<Job> = self.aliases[lang].iter().cloned().map(Job::Alias).collect();
        // robots.txt and the sitemap index are rendered once, with the first language.
        let root = |p: &Page| {
            matches!(p.kind, PageKind::RobotsTxt | PageKind::SitemapIndex) && p.lang.index() == 0
        };
        let mut standalone: Vec<PageId> = model
            .pages
            .iter()
            .filter(|p| p.lang == lang && p.standalone.is_some() && !root(p))
            .map(|p| p.id)
            .collect();
        if lang.index() == 0 {
            standalone.extend(model.pages.iter().filter(|p| root(p)).map(|p| p.id));
        }
        for p in &model.pages {
            if p.lang != lang || is_standalone(p.kind) {
                continue;
            }
            for o in outputs(p) {
                jobs.push(Job::Page {
                    page: p.id,
                    format: o.format,
                });
            }
        }
        for s in standalone {
            for o in outputs(&model.pages[s]) {
                jobs.push(Job::Standalone {
                    page: s,
                    format: o.format,
                });
            }
        }
        jobs.sort_by_key(|j| self.order(j));
        jobs
    }

    /// Wave 2 (phase E3): from the paginations recorded in wave 1, the `page/1/` aliases (HTML
    /// formats, unless `pagination.disableAliases`) and pagers 2..N; then the language
    /// redirect.
    #[must_use]
    pub fn wave2(&self) -> Vec<Job> {
        let cfg = &self.model.config;
        let mut jobs = Vec::new();
        for ((page, format), rec) in self.pagination.recorded() {
            let site = &cfg.sites[self.model.pages[page].lang];
            if cfg.output_formats.get(format).is_html && !site.pagination.disable_aliases {
                jobs.push(Job::PagerAlias { page, format });
            }
            for number in 2..=rec.total_pages() {
                jobs.push(Job::Pager {
                    page,
                    format,
                    number,
                });
            }
        }
        jobs.sort_by_key(|j| self.order(j));
        if self.language_redirect().is_some() {
            jobs.push(Job::LanguageRedirect);
        }
        jobs
    }

    /// The layout chosen for page `page` in `format` in [`new`](Self::new) (the template and
    /// base template its [`Job::Page`] or [`Job::Standalone`] renders); `None`: no layout, the
    /// job renders nothing. The structure dump of `ssg-build` records it.
    #[must_use]
    pub fn selection(&self, page: PageId, format: FormatId) -> Option<&Selection> {
        self.selections.get(&(page, format))
    }

    /// The redirect to the default language's home page (`ssg_nav::language_redirect`:
    /// `/en/` → `/`, or `/` → `/en/` with `defaultContentLanguageInSubdir`), when the home page
    /// has that output: what [`Job::LanguageRedirect`] renders.
    #[must_use]
    pub fn language_redirect(&self) -> Option<AliasPlan> {
        let a = ssg_nav::language_redirect(self.views.nav(), &self.model.config)?;
        outputs(&self.model.pages[a.to])
            .any(|o| o.format == a.format)
            .then_some(a)
    }
}
