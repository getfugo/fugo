//! Assets (the files of the assets directories) and the resources of page bundles and adapters.

use super::*;

impl ResourceStore {
    /// `resources.Get`: the asset at `path` (cleaned; a leading `/` is ignored) in `lang`'s
    /// view, or `None` when there is no such file. Bytes injected with
    /// [`inject_generated`](Self::inject_generated) count as a file.
    ///
    /// # Errors
    /// None today; reserved for asset sources that can fail.
    pub fn get_asset(
        &self,
        lang: LangIdx,
        path: &str,
    ) -> Result<Option<ResourceId>, ResourceError> {
        let rel = paths::clean(&format!("/{path}"));
        let rel = rel.trim_start_matches('/');
        if rel.is_empty() {
            return Ok(None);
        }
        let Some(file) = self
            .cfg
            .vfs
            .as_ref()
            .and_then(|v| v.open(Component::Assets, rel))
        else {
            return Ok(None);
        };
        let body = Body::File(file.abs);
        let lang = self.global_lang(lang);
        self.assets
            .get_or_try((lang, rel.to_owned()), || {
                Ok(self.push_asset(lang, rel, body))
            })
            .map(Some)
    }

    pub(super) fn push_asset(&self, lang: LangIdx, rel: &str, body: Body) -> ResourceId {
        let link = format!("/{rel}");
        self.push(NewResource {
            origin: Origin::Asset {
                path: rel.to_owned(),
            },
            media_type: self.media_type_of(rel),
            name: link.clone(),
            name_normalized: None,
            title: link.clone(),
            params: Params::default(),
            data: Map::new(),
            lang,
            target: self.global_target(lang, &link),
            link: UrlPath::new(&link),
            body,
            policy: PublishPolicy::OnReference,
            kind: None,
        })
    }

    pub(super) fn asset_files(&self) -> Result<Arc<Vec<String>>, ResourceError> {
        let mut cached = lock(&self.asset_files);
        if let Some(files) = &*cached {
            return Ok(Arc::clone(files));
        }
        let mut files: Vec<String> = match &self.cfg.vfs {
            Some(vfs) => vfs
                .walk(Component::Assets)?
                .into_iter()
                .map(|f| f.rel)
                .collect(),
            None => Vec::new(),
        };
        files.sort();
        files.dedup();
        let files = Arc::new(files);
        *cached = Some(Arc::clone(&files));
        Ok(files)
    }

    /// `resources.Match`: the assets whose path matches the glob `pattern` (Go's globs, case
    /// folded, a leading `/` ignored), sorted by path.
    ///
    /// # Errors
    /// An invalid pattern, or an assets directory that cannot be read.
    pub fn find_assets(
        &self,
        lang: LangIdx,
        pattern: &str,
    ) -> Result<Vec<ResourceId>, ResourceError> {
        let g = glob::compile(pattern.trim_start_matches('/'), GlobOpts::default())?;
        let mut paths: Vec<String> = self
            .asset_files()?
            .iter()
            .filter(|p| g.is_match(p))
            .cloned()
            .collect();
        paths.sort();
        paths.dedup();
        let mut ids = Vec::with_capacity(paths.len());
        for p in &paths {
            if let Some(id) = self.get_asset(lang, p)? {
                ids.push(id);
            }
        }
        Ok(ids)
    }

    /// `resources.GetMatch`: the first of [`find_assets`](Self::find_assets).
    ///
    /// # Errors
    /// As [`find_assets`](Self::find_assets).
    pub fn find_asset(
        &self,
        lang: LangIdx,
        pattern: &str,
    ) -> Result<Option<ResourceId>, ResourceError> {
        Ok(self.find_assets(lang, pattern)?.into_iter().next())
    }

    /// A bundle file of a page. Registering a second file for the same target returns the
    /// first registration (translations sharing a bundle directory share the file).
    pub fn register_bundle(&self, b: &BundleResource) -> ResourceId {
        let name = b.name.trim_start_matches('/');
        let link = paths::join(&["/", &b.dir, name]);
        let target = OutputPath::new(&format!("{}{link}", self.lang_target(b.lang).target_prefix));
        let result: Result<ResourceId, std::convert::Infallible> =
            self.bundles.get_or_try(target.clone(), || {
                Ok(self.push(NewResource {
                    origin: Origin::Bundle { lang: b.lang },
                    media_type: self.media_type_of(name),
                    name: name.to_owned(),
                    name_normalized: None,
                    title: name.to_owned(),
                    params: Params::default(),
                    data: Map::new(),
                    lang: b.lang,
                    target,
                    link: UrlPath::new(&link),
                    body: Body::File(b.file.clone()),
                    policy: b.policy,
                    kind: None,
                }))
            });
        match result {
            Ok(id) => id,
            Err(never) => match never {},
        }
    }

    /// A page resource a content adapter added: like [`register_bundle`](Self::register_bundle)
    /// (target and link below the page's directory, or the passed resource's own place), with
    /// the adapter's name, title and params. Every call makes a new resource (two adapter
    /// resources may share a passed resource's place).
    pub fn register_adapter_resource(&self, r: &AdapterResource) -> ResourceId {
        let name = r.name.trim_start_matches('/');
        let (target, link) = match &r.place {
            Some((target, link)) => (target.clone(), link.clone()),
            None => {
                let link = paths::join(&["/", &r.dir, name]);
                let target =
                    OutputPath::new(&format!("{}{link}", self.lang_target(r.lang).target_prefix));
                (target, UrlPath::new(&link))
            }
        };
        let types = &self.cfg.media_types;
        let media_type = r
            .media_type
            .as_deref()
            .and_then(|t| types.by_type(t))
            .map_or_else(|| self.media_type_of(name), |id| types.get(id).clone());
        let display = r.display_name.clone().unwrap_or_else(|| name.to_owned());
        self.push(NewResource {
            origin: Origin::Bundle { lang: r.lang },
            media_type,
            title: r.title.clone().unwrap_or_else(|| display.clone()),
            name_normalized: Some(normalize_name(name)),
            name: display,
            params: r.params.clone(),
            data: Map::new(),
            lang: r.lang,
            target,
            link,
            body: r.body.clone(),
            policy: r.policy,
            kind: None,
        })
    }
}
