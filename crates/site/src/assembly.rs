//! Capture and assembly: the content captured for content adapters, what they add, and the model
//! assembled from both (`load_model`, `capture_content`, `assemble`).

use super::*;

/// Why the model could not be built.
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error(transparent)]
    Vfs(#[from] VfsError),
    #[error("{path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("{path}: {message}")]
    FrontMatter { path: PathBuf, message: String },
    #[error("{path}: {source}")]
    Page {
        path: PathBuf,
        #[source]
        source: PageError,
    },
    /// A Go-template content adapter (`_content.gotmpl`).
    #[error(
        "{0}: content adapters are Tera templates: port this Go template to \
         `_content.html` in the same directory (`add_page(page={{…}})`, \
         `add_resource(resource={{…}})`, `store_set`, `enable_all_languages()`; \
         https://github.com/getfugo/fugo/blob/main/docs/rust-port/template-api.md gives \
         the Go template functions with their Tera names)"
    )]
    GoContentAdapter(PathBuf),
    /// A page or resource a content adapter added that cannot be placed.
    #[error("{path}: {message}")]
    Adapter { path: PathBuf, message: String },
    #[error("{path}: no taxonomy is configured for {key:?}")]
    NoTaxonomy { path: PathBuf, key: String },
    #[error("site cascade: {0}")]
    SiteCascade(#[source] PageError),
    #[error("[permalinks] of language {lang}: {source}")]
    Permalinks {
        lang: String,
        #[source]
        source: PageError,
    },
    /// A page made by the build (a missing section, a term page) whose cascaded front matter
    /// is invalid, or a page whose URL cannot be made.
    #[error("page {path} ({lang}): {source}")]
    Node {
        path: String,
        lang: String,
        #[source]
        source: PageError,
    },
    #[error(transparent)]
    Data(#[from] DataError),
}

/// A page the build filter removed: node dates still count its dates (Go aggregates dates
/// before it removes drafts, future and expired content).
#[derive(Clone, Debug)]
pub(crate) struct Removed {
    pub lang: LangIdx,
    pub key: ContentKey,
    pub kind: PageKind,
    pub dates: Dates,
}

/// The content and data of a project, read and decoded (phase A4), before the model is
/// assembled; content adapters are listed, not run.
#[derive(Clone, Debug)]
pub struct Captured {
    pub(super) capture: capture::Capture,
    pub(super) data: Data,
}

impl Captured {
    /// The content adapters (`_content.html`), by directory, then language.
    #[must_use]
    pub fn adapters(&self) -> &[ContentAdapter] {
        &self.capture.adapters
    }
}

/// What content adapters added: their pages and page resources, in the order they were
/// added.
#[derive(Clone, Debug, Default)]
pub struct Added {
    pub pages: Vec<AddedPage>,
    pub resources: Vec<AddedResource>,
}

/// A page an adapter added with `add_page`.
#[derive(Clone, Debug)]
pub struct AddedPage {
    /// The adapter (an index into [`Captured::adapters`]).
    pub adapter: usize,
    /// The language the adapter ran for.
    pub lang: LangIdx,
    pub page: Arc<AdapterPage>,
}

/// A page resource an adapter added with `add_resource` (Go's `ResourceConfig`).
#[derive(Clone, Debug)]
pub struct AddedResource {
    /// The adapter (an index into [`Captured::adapters`]).
    pub adapter: usize,
    /// The language the adapter ran for.
    pub lang: LangIdx,
    /// The resource's path below the content root, normalised, without a leading slash
    /// (`news/p1/cover.jpg`): it belongs to the page at the longest key above it.
    pub path: String,
    /// `name` (`None`: the path below the page).
    pub name: Option<String>,
    /// `title` (`None`: the name).
    pub title: Option<String>,
    pub params: Params,
    pub content: AddedContent,
}

/// The content of a resource an adapter added.
#[derive(Clone, Debug)]
pub enum AddedContent {
    /// A string `content.value`, of `content.mediaType` (`None`: the type of the path's
    /// extension); published below its page like a bundle file.
    Text {
        text: Arc<str>,
        media_type: Option<String>,
    },
    /// A resource the adapter got (`get_asset`, `get_remote`, …): Go uses the resource itself,
    /// so it keeps its own file and link (relative to the site root, not to the page).
    Resource {
        body: AddedBody,
        media_type: String,
        target: OutputPath,
        link: UrlPath,
    },
}

/// Where the bytes of an added resource are.
#[derive(Clone, Debug)]
pub enum AddedBody {
    File(PathBuf),
    Bytes(Arc<[u8]>),
}

/// Reads the content and data of `cfg`'s project (phase A4, see the crate docs).
///
/// # Errors
/// A content or data file that cannot be read or decoded, front matter capture overrides of
/// the wrong shape, or a Go-template content adapter (`_content.gotmpl`).
pub fn capture_content(cfg: &Config, vfs: &Vfs) -> Result<Captured, ModelError> {
    let (captured, data) = rayon::join(|| capture::capture(cfg, vfs), || data::load(vfs));
    Ok(Captured {
        capture: captured?,
        data: data?,
    })
}

/// Builds the model of `cfg`'s project from content without running its content adapters
/// ([`capture_content`], then [`assemble`] with nothing [`Added`]).
///
/// # Errors
/// See [`capture_content`] and [`assemble`].
pub fn load_model(cfg: Arc<Config>, vfs: &Vfs, o: &LoadModelOptions) -> Result<Model, ModelError> {
    let captured = capture_content(&cfg, vfs)?;
    assemble(cfg, captured, Added::default(), o)
}

/// Builds the model from captured content and the pages content adapters added: the added
/// pages go after the content files (a file wins a key both claim, with a warning).
///
/// A key (or resource path) both a content file and an adapter claim is the file's; one two
/// adapters claim is the later adapter's, in the place of the earlier one's (both with a
/// warning). This is Go's order with one collector worker (`pages_capture.go` `collectDirDir`
/// queues a directory's adapters before its files and its subdirectories, and `content_map.go`
/// `insertPageWithLock`/`insertResourceWithLock` keep the last insert); with several workers
/// Go's result depends on scheduling.
///
/// # Errors
/// Invalid front matter (reserved keys of the wrong shape, a bad cascade or date
/// configuration), a page an adapter added that cannot be placed, a taxonomy kind without a
/// taxonomy, a `[permalinks]` pattern that does not parse, or a URL that cannot be made.
pub fn assemble(
    cfg: Arc<Config>,
    captured: Captured,
    added: Added,
    o: &LoadModelOptions,
) -> Result<Model, ModelError> {
    let Captured {
        capture: mut captured,
        data,
    } = captured;
    if !added.pages.is_empty() || !added.resources.is_empty() {
        let parser = PathParser::from_config(&cfg);
        // (language, key) → place in `captured.pages` of the adapter pages added so far.
        let mut page_at: HashMap<(LangIdx, ContentKey), usize> = HashMap::new();
        for a in added.pages {
            let adapter = &captured.adapters[a.adapter];
            let page = capture::adapter_page(adapter, a.lang, a.page, &parser)?;
            // A file claiming the key wins in `tree::place`; an earlier adapter's page is
            // replaced (one adapter's repeats are already its last `add_page`).
            match page_at.entry((page.lang, page.source.info.key.clone())) {
                Entry::Occupied(at) => {
                    let earlier = &mut captured.pages[*at.get()];
                    captured.diagnostics.push(later_adapter_wins(
                        "content",
                        &page.source.info.key,
                        &page.source.file.abs,
                        &earlier.source.file.abs,
                    ));
                    *earlier = page;
                }
                Entry::Vacant(at) => {
                    at.insert(captured.pages.len());
                    captured.pages.push(page);
                }
            }
        }
        // (language, key) → place in `captured.resources`: the first file of a key, then the
        // adapter resources.
        let mut resource_at: HashMap<(LangIdx, ContentKey), usize> = HashMap::new();
        if !added.resources.is_empty() {
            for (i, f) in captured.resources.iter().enumerate() {
                resource_at.entry((f.lang, f.info.key.clone())).or_insert(i);
            }
        }
        for a in added.resources {
            let adapter = &captured.adapters[a.adapter];
            let r = capture::adapter_resource(adapter, a, &parser)?;
            let taken = match resource_at.entry((r.lang, r.info.key.clone())) {
                Entry::Occupied(at) => Some(&mut captured.resources[*at.get()]),
                Entry::Vacant(at) => {
                    at.insert(captured.resources.len());
                    None
                }
            };
            match taken {
                Some(f) if f.adapter.is_none() => {
                    captured.diagnostics.push(
                        Diagnostic::warning(format!(
                            "duplicate resource path {:?}: {} is used, the resource {} adds \
                             is ignored",
                            r.info.key.to_path(),
                            f.file.abs.display(),
                            r.file.abs.display()
                        ))
                        .with_id("duplicate-resource-path"),
                    );
                }
                Some(f) => {
                    captured.diagnostics.push(later_adapter_wins(
                        "resource",
                        &r.info.key,
                        &r.file.abs,
                        &f.file.abs,
                    ));
                    *f = r;
                }
                None => captured.resources.push(r),
            }
        }
    }
    let assembly = tree::place(&cfg, captured)?;
    let cascades = meta::cascade_indexes(&cfg, &assembly)?;
    let (metas, meta_diags) = meta::metas(&cfg, &assembly, &cascades)?;

    let mut diagnostics = assembly.diagnostics;
    diagnostics.extend(meta_diags);

    // Filter: removed pages take the bundle files below them (same language) with them.
    let mut removed: Vec<Removed> = Vec::new();
    let mut keep = vec![true; assembly.pages.len()];
    let mut metas: Vec<Option<PageMeta>> = metas.into_iter().map(Some).collect();
    for (i, p) in assembly.pages.iter().enumerate() {
        if p.role != PageRole::Standalone {
            continue;
        }
        let site = &cfg.sites[p.page.lang];
        let Some(meta) = metas[i].as_mut() else {
            continue;
        };
        match o.verdict(p.kind, !site.disable_kinds.contains(p.kind), meta) {
            Verdict::Build => {}
            Verdict::Disable => meta.build = filter::DISABLED,
            Verdict::Remove => {
                keep[i] = false;
                removed.push(Removed {
                    lang: p.page.lang,
                    key: p.key.clone(),
                    kind: p.kind,
                    dates: meta.dates.clone(),
                });
            }
        }
    }
    let below_removed = |lang: LangIdx, key: &ContentKey| {
        removed
            .iter()
            .any(|r| r.lang == lang && r.key != *key && key.starts_with_segments(&r.key))
    };

    let mut pages: IdVec<PageId, Page> = IdVec::new();
    let mut bundle_resources: IdVec<ResourceId, BundleResource> = IdVec::new();
    let mut sites: IdVec<LangIdx, SiteModel> = cascades
        .into_iter()
        .enumerate()
        .map(|(i, cascade)| SiteModel {
            lang: <LangIdx as ssg_base::Idx>::from_index(i),
            tree: SiteTree::default(),
            resources: BTreeMap::new(),
            cascade,
            home: PageId::from_raw(0),
            pages: Vec::new(),
            regular_pages: Vec::new(),
            regular_pages_local: Vec::new(),
            taxonomies: IdVec::new(),
            main_sections: Vec::new(),
            last_mod: None,
            permalinks: PermalinkPatterns::default(),
        })
        .collect();
    for (i, p) in assembly.pages.into_iter().enumerate() {
        let lang = p.page.lang;
        let bundled = p.role != PageRole::Standalone;
        if !keep[i] || (bundled && below_removed(lang, &p.key)) {
            continue;
        }
        let Some(meta) = metas[i].take() else {
            continue;
        };
        let id = pages.next_id();
        let source = p.page.source;
        if bundled {
            let mut r = BundleResource::new(
                p.key.clone(),
                lang,
                source.file.clone(),
                source.file_info.clone(),
            );
            r.page = Some(id);
            let rid = bundle_resources.push(r);
            sites[lang].resources.insert(p.key.clone(), rid);
        } else {
            sites[lang].tree.insert(p.key.clone(), id);
        }
        let path_info = source.info.clone();
        pages.push(Page::new(
            id,
            lang,
            p.kind,
            p.role,
            p.key,
            Some(source),
            path_info,
            meta,
        ));
    }
    for r in assembly.resources {
        if below_removed(r.lang, &r.info.key) {
            continue;
        }
        let key = r.info.key.clone();
        let mut br = BundleResource::new(key.clone(), r.lang, r.file, r.info);
        br.adapter = r.adapter;
        let rid = bundle_resources.push(br);
        sites[r.lang].resources.insert(key, rid);
    }

    diagnostics.extend(data.diagnostics);
    let parser = Arc::new(PathParser::from_config(&cfg));
    let mut model = Model {
        config: cfg,
        pages,
        sites,
        bundle_resources,
        data: Arc::new(data.map),
        diagnostics,
        refs: refs::RefIndex::new(parser),
    };
    nodes::assemble(&mut model, o, &removed)?;
    Ok(model)
}

/// The warning for a page or resource path (`what`: `content`, `resource`) two content
/// adapters add: the one that runs `later` replaces the `earlier` one's.
pub(super) fn later_adapter_wins(
    what: &str,
    key: &ContentKey,
    later: &std::path::Path,
    earlier: &std::path::Path,
) -> Diagnostic {
    Diagnostic::warning(format!(
        "duplicate {what} path {:?}: {} is used, {} is ignored (of two content adapters, the \
         one that runs later wins)",
        key.to_path(),
        later.display(),
        earlier.display()
    ))
    .with_id(format!("duplicate-{what}-path"))
}
