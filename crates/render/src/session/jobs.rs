//! Render jobs: a page in one output format, deferred and adapter renders, layouts and aliases.

use super::*;

impl Session {
    /// Phase E: renders one job. Pure: no I/O; the outputs go to the publisher.
    ///
    /// # Errors
    /// A template error ([`RenderError::Render`]) or an unknown page or format.
    pub fn render_job(&self, job: &Job) -> Result<Vec<Output>, RenderError> {
        if !self.views.is_frozen() {
            return Err(RenderError::Phase("render_job before freeze_views"));
        }
        let model = &self.model;
        match *job {
            Job::Page { page, format } | Job::Standalone { page, format } => {
                let p = &model.pages[page];
                let Some(out) = outputs(p).find(|o| o.format == format) else {
                    return Err(RenderError::NoOutput { page, format });
                };
                self.render_layout(job, page, format, None, out.paths.target.clone())
            }
            Job::Pager {
                page,
                format,
                number,
            } => {
                let (target, _) = page_target(model, page, format, Some(number))?;
                self.render_layout(job, page, format, Some(number), target.target)
            }
            Job::Alias(ref a) => {
                let to = self.permalink(a.to, a.format)?;
                self.render_alias(job, &a.from, &to, Some(a.to), a.format)
            }
            Job::PagerAlias { page, format } => {
                let (target, _) = page_target(model, page, format, Some(1))?;
                let to = self.permalink(page, format)?;
                self.render_alias(job, &target.target, &to, Some(page), format)
            }
            Job::LanguageRedirect => {
                let Some(a) = self.language_redirect() else {
                    return Ok(Vec::new());
                };
                let to = self.permalink(a.to, a.format)?;
                self.render_alias(job, &a.from, &to, None, a.format)
            }
        }
    }

    /// The file `job` writes, without rendering it: `None` when it writes nothing (a page
    /// without a layout, an alias without an alias template, a language redirect without a
    /// home output). Phase E resolves target collisions with it before rendering.
    ///
    /// # Errors
    /// An unknown page or format, or a pager whose file cannot be made.
    pub fn target(&self, job: &Job) -> Result<Option<OutputPath>, RenderError> {
        let model = &self.model;
        let has_layout =
            |page: PageId, format: FormatId| self.selections.contains_key(&(page, format));
        Ok(match *job {
            Job::Page { page, format } | Job::Standalone { page, format } => {
                if !has_layout(page, format) {
                    return Ok(None);
                }
                let out = outputs(&model.pages[page])
                    .find(|o| o.format == format)
                    .ok_or(RenderError::NoOutput { page, format })?;
                Some(out.paths.target.clone())
            }
            Job::Pager {
                page,
                format,
                number,
            } => {
                if !has_layout(page, format) {
                    return Ok(None);
                }
                Some(page_target(model, page, format, Some(number))?.0.target)
            }
            Job::Alias(ref a) => self.alias_template().map(|_| a.from.clone()),
            Job::PagerAlias { page, format } => match self.alias_template() {
                Some(_) => Some(page_target(model, page, format, Some(1))?.0.target),
                None => None,
            },
            Job::LanguageRedirect => match (self.alias_template(), self.language_redirect()) {
                (Some(_), Some(a)) => Some(a.from),
                _ => None,
            },
        })
    }

    /// Phase E5: renders the template of a `defer(...)` call registered under `key`, with
    /// `data`, `site` (the default language's), `build` and a scope in phase `Deferred`.
    ///
    /// # Errors
    /// [`RenderError::Phase`] before [`freeze_views`](Self::freeze_views), or
    /// [`RenderError::Deferred`] when the template fails.
    pub fn render_deferred(&self, key: &str, d: &Deferred) -> Result<String, RenderError> {
        if !self.views.is_frozen() {
            return Err(RenderError::Phase("render_deferred before freeze_views"));
        }
        let lang = LangIdx::from_index(0);
        let home = self.model.sites[lang].home;
        let scope = RenderScope {
            phase: Phase::Deferred,
            ..RenderScope::layout(home, lang, self.html_format, None)
        };
        let generation = self.views.generation(Phase::Deferred, HookVariant::Html);
        let mut ctx = tera::Context::new();
        ctx.insert_value("data", d.data.clone());
        ctx.insert_value("site", generation.sites[lang].clone());
        ctx.insert_value("build", self.build_info.clone());
        ctx.insert_value(SCOPE_KEY, scope.to_value());
        self.templates
            .tera()
            .render(&d.template, &ctx)
            .map_err(|source| RenderError::Deferred {
                key: key.to_owned(),
                template: d.template.to_string(),
                source: Box::new(source),
            })
    }

    /// Runs a content adapter: renders `source`, the Tera template of the `_content.html` at
    /// `path`, for language `lang` as run `run` of [`Handles::adapters`] (which collects what
    /// it adds); the output is discarded. The context has `site` (the language's, without the
    /// page lists: Go's site is not built yet either), `build`, `lang` and a scope in phase
    /// `Adapter` on the language's home page, so site functions and partials work on the
    /// session's model (the content files).
    ///
    /// # Errors
    /// [`RenderError::Adapter`] when the template does not parse or fails.
    pub fn render_adapter(
        &self,
        path: &str,
        source: &str,
        lang: LangIdx,
        run: u32,
    ) -> Result<(), RenderError> {
        let home = self.model.sites[lang].home;
        let scope = RenderScope {
            phase: Phase::Adapter,
            adapter: Some(run),
            ..RenderScope::layout(home, lang, self.html_format, None)
        };
        let generation = self.views.generation(Phase::Adapter, HookVariant::Html);
        let site: tera::Map = generation.sites[lang]
            .clone()
            .into_map()
            .unwrap_or_default()
            .into_iter()
            .filter(|(k, _)| {
                !k.as_str()
                    .is_some_and(|k| ADAPTER_HIDDEN_SITE_KEYS.contains(&k))
            })
            .collect();
        let mut ctx = tera::Context::new();
        ctx.insert_value("site", tera::Value::from(site));
        ctx.insert_value("build", self.build_info.clone());
        ctx.insert("lang", &self.model.config.sites[lang].language.key);
        ctx.insert_value(SCOPE_KEY, scope.to_value());
        self.templates
            .tera()
            .render_str(source, &ctx, false)
            .map(drop)
            .map_err(|source| RenderError::Adapter {
                path: path.to_owned(),
                source: Box::new(source),
            })
    }

    pub(super) fn permalink(&self, page: PageId, format: FormatId) -> Result<String, RenderError> {
        outputs(&self.model.pages[page])
            .find(|o| o.format == format)
            .and_then(|o| o.links.as_ref())
            .map(|l| l.permalink.to_string())
            .ok_or(RenderError::NoOutput { page, format })
    }

    /// The order of a job.
    #[must_use]
    pub fn order(&self, job: &Job) -> JobOrder {
        let rank = |f: FormatId| u8::try_from(f.index()).unwrap_or(u8::MAX);
        let of = |page: PageId, format: FormatId, sub: u8| {
            JobOrder::new(self.model.pages[page].lang, rank(format), page.raw(), sub)
        };
        match *job {
            Job::Alias(ref a) => of(a.to, a.format, 0),
            Job::Page { page, format } => of(page, format, 1),
            Job::PagerAlias { page, format } => of(page, format, 2),
            Job::Pager { page, format, .. } => of(page, format, 3),
            Job::Standalone { page, format } => of(page, format, 4),
            Job::LanguageRedirect => JobOrder::new(LangIdx::from_index(0), 0, 0, 5),
        }
    }

    pub(super) fn render_layout(
        &self,
        job: &Job,
        page: PageId,
        format: FormatId,
        pager: Option<u32>,
        target: OutputPath,
    ) -> Result<Vec<Output>, RenderError> {
        let Some(sel) = self.selections.get(&(page, format)) else {
            return Ok(Vec::new());
        };
        let cfg = &self.model.config;
        let p = &self.model.pages[page];
        let mut scope = RenderScope::layout(page, p.lang, format, pager);
        // A layout job for format F sees variant `Format(F)` if it exists, else `Html`.
        scope.variant = self.known_variant(HookVariant::Format(format));
        let generation = self.views.generation(Phase::Layout, scope.variant);
        let full = generation.page_value(page);
        let format_name = &cfg.output_formats.get(format).name;
        let output_format = full
            .as_map()
            .and_then(|m| m.get(&tera::value::Key::Str("output_formats")))
            .and_then(tera::Value::as_map)
            .and_then(|m| m.get(&tera::value::Key::String(format_name.clone().into())))
            .cloned()
            .unwrap_or_else(tera::Value::none);
        let mut ctx = tera::Context::new();
        ctx.insert_value("page", full);
        ctx.insert_value("site", generation.sites[p.lang].clone());
        ctx.insert_value("build", self.build_info.clone());
        ctx.insert("lang", &cfg.sites[p.lang].language.key);
        ctx.insert_value("output_format", output_format);
        if p.kind == PageKind::SitemapIndex {
            ctx.insert_value("sites", tera::Value::from(generation.sites.as_slice()));
        }
        ctx.insert_value(SCOPE_KEY, scope.to_value());
        let text = self
            .templates
            .tera()
            .render(sel.render_as.as_str(), &ctx)
            .map_err(|source| RenderError::Render {
                template: sel.render_as.to_string(),
                page: p.key.to_path(),
                source: Box::new(source),
            })?;
        Ok(vec![Output {
            path: target,
            text,
            format,
            lang: p.lang,
            is_html: cfg.output_formats.get(format).is_html,
            order: self.order(job),
        }])
    }

    /// The alias template (user, theme or embedded `alias.html`).
    pub(super) fn alias_template(&self) -> Option<TemplateName> {
        self.project
            .layouts
            .templates()
            .filter(|t| t.role == TemplateRole::Standalone(StandaloneKind::Alias))
            .min_by(|a, b| a.origin.cmp(&b.origin))
            .map(|t| t.render_name().clone())
    }

    pub(super) fn render_alias(
        &self,
        job: &Job,
        from: &OutputPath,
        to: &str,
        page: Option<PageId>,
        format: FormatId,
    ) -> Result<Vec<Output>, RenderError> {
        let Some(name) = self.alias_template() else {
            return Ok(Vec::new());
        };
        let lang = page.map_or(LangIdx::from_index(0), |p| self.model.pages[p].lang);
        let generation = self.views.generation(Phase::Layout, HookVariant::Html);
        let mut ctx = tera::Context::new();
        ctx.insert("permalink", to);
        ctx.insert_value(
            "page",
            page.map_or_else(tera::Value::none, |p| generation.links[p].clone()),
        );
        ctx.insert_value("site", generation.sites[lang].clone());
        ctx.insert_value("build", self.build_info.clone());
        let text = self
            .templates
            .tera()
            .render(name.as_str(), &ctx)
            .map_err(|source| RenderError::Render {
                template: name.to_string(),
                page: from.to_string(),
                source: Box::new(source),
            })?;
        Ok(vec![Output {
            path: from.clone(),
            text,
            format,
            lang,
            is_html: true,
            order: self.order(job),
        }])
    }
}
