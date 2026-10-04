//! The site's template functions and filters, registered for a session (`with_functions`).

use super::*;

impl Session {
    /// [`new`](Self::new), with `extra` registering more template functions after the site
    /// functions (tests; a function registered here replaces one of the same name).
    ///
    /// # Errors
    /// As [`new`](Self::new).
    pub fn with_functions(
        project: Project,
        model: Arc<Model>,
        o: &RenderOptions,
        extra: &dyn Fn(&mut tera::Tera, &Handles),
    ) -> Result<Arc<Self>, RenderError> {
        let diagnostics = Arc::new(Diagnostics::new(model.config.ignore_logs.iter()));
        let cfg = &model.config;
        let imaging = Imaging::from_config(&cfg.imaging).unwrap_or_else(|e| {
            diagnostics.push(Diagnostic::error(format!("[imaging]: {e}")));
            Imaging::default()
        });
        // One image queue: the store registers processed images in it, `resize` & co. enqueue
        // into it, and phase E6 processes it into `[caches.images]`.
        let images = Arc::new(ImageQueue::new(
            imaging,
            cfg.caches.get("images").map(ImageCache::from_config),
        ));
        let store = Arc::new(ResourceStore::new(StoreConfig::from_config(
            cfg,
            Some(Arc::clone(&project.vfs)),
            Some(Arc::clone(&images)),
        )));
        let translations = Arc::new(i18n::load(&project.vfs, cfg)?);
        let (menus, menu_diagnostics) =
            ssg_nav::build_menus(&NavSite::new(Arc::clone(&model)), &model.config);
        for d in menu_diagnostics {
            diagnostics.push(d);
        }
        let views = Arc::new(ViewCache::new(ViewInputs {
            model: Arc::clone(&model),
            store: Arc::clone(&store),
            menus: Arc::new(menus),
        })?);
        let pagination = Arc::new(PaginationRecorder::default());
        let aliases = model
            .config
            .sites
            .ids()
            .map(|l| ssg_nav::page_aliases(views.nav(), &model.config, l))
            .collect::<Result<_, _>>()?;

        let mut selections = BTreeMap::new();
        for p in &model.pages {
            let path = lookup_path(p);
            for out in outputs(p) {
                match project.layouts.select(&layout_query(p, &path, out.format)) {
                    Some(s) => {
                        selections.insert((p.id, out.format), s);
                    }
                    None if is_standalone(p.kind) => {}
                    None => diagnostics.push(Diagnostic::warning(format!(
                        "no layout for {} page {} in format {}",
                        p.kind.as_str(),
                        p.key.to_path(),
                        model.config.output_formats.get(out.format).name
                    ))),
                }
            }
        }
        let sel: Selections = selections.values().cloned().collect();

        let html_format = cfg
            .output_formats
            .by_name("html")
            .unwrap_or(FormatId::from_raw(0));
        let variants = hook_variants(&project.layouts, html_format);
        let embedded = project
            .layouts
            .templates()
            .filter(|t| t.origin == Origin::Embedded)
            .map(|t| t.render_name().clone())
            .collect();
        let highlight = Arc::new(Highlight::new(&cfg.default_site().markup.highlight));
        let renderer: Arc<OnceLock<Weak<dyn ContentRenderer>>> = Arc::new(OnceLock::new());
        let templates_slot: Arc<OnceLock<Weak<Templates>>> = Arc::new(OnceLock::new());
        let handles = Handles {
            model: Arc::clone(&model),
            views: Arc::clone(&views),
            store,
            images,
            stores: Arc::new(PageStores::new(model.pages.len())),
            pagination: Arc::clone(&pagination),
            deferred: Arc::new(DeferredRegistry::default()),
            css_purges: Arc::default(),
            menus: Arc::clone(views.menus()),
            related: Arc::new(ssg_sitefuncs::RelatedCache::default()),
            i18n: translations,
            diagnostics: Arc::clone(&diagnostics),
            highlight: Arc::clone(&highlight),
            renderer: Arc::clone(&renderer),
            templates: Arc::clone(&templates_slot),
            frames: Arc::default(),
            partial_cache: Arc::default(),
            adapters: Arc::default(),
        };

        let pure = Arc::new(pure_env(&model, o, &diagnostics));
        let templates = ssg_layouts::load(Arc::clone(&project.layouts), &sel, &|t| {
            register_placeholders(t);
            register_pure(t, &pure);
            ssg_sitefuncs::register(t, &handles);
            extra(t, &handles);
        })?;
        let session = Arc::new(Self {
            build_info: tera::Value::from_serializable(&BuildView::new(&model.config, o.server)),
            cells: ContentStore::new(model.pages.len(), &variants),
            markdown: cfg.sites.iter().map(content::markdown_options).collect(),
            embedded_hooks: cfg
                .sites
                .iter()
                .map(|s| EmbeddedHooks::of(s, cfg))
                .collect(),
            // A language whose `[markup.highlight]` is the default site's shares the `highlight`
            // filter's highlighter (the syntax sets are loaded once).
            highlighters: cfg
                .sites
                .iter()
                .map(|s| {
                    let cell = OnceLock::new();
                    if s.markup.highlight == cfg.default_site().markup.highlight {
                        let _ = cell.set(Arc::clone(&highlight));
                    }
                    cell
                })
                .collect(),
            model,
            project,
            templates: {
                let templates = Arc::new(templates);
                let _ = templates_slot.set(Arc::downgrade(&templates));
                templates
            },
            views,
            pagination,
            handles,
            selections,
            aliases,
            variants,
            html_format,
            embedded,
            inclusions: Inclusions::default(),
            contents: OnceLock::new(),
            diagnostics,
        });
        let weak: Weak<Self> = Arc::downgrade(&session);
        let weak: Weak<dyn ContentRenderer> = weak;
        let _ = renderer.set(weak);
        Ok(session)
    }
}
