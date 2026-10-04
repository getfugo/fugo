//! Collecting the themes a site imports, depth first.

use super::*;

pub(super) struct Collector<'a> {
    pub(super) themes_dir: PathBuf,
    pub(super) environment: &'a str,
    /// `module.replacements`: old path → new path.
    pub(super) replacements: BTreeMap<String, String>,
    pub(super) vendor: Vendor,
    /// [`path_key`]s of the imports taken.
    pub(super) seen: BTreeSet<String>,
    pub(super) out: Collected,
    pub(super) diagnostics: &'a mut Vec<Diagnostic>,
}

impl Collector<'_> {
    /// The imports of a configuration tree: `[[module.imports]]` (renamed by
    /// `module.replacements`, disabled ones left out), then `theme`. `owner` is the theme the
    /// tree belongs to.
    pub(super) fn imports(
        &self,
        tree: &Map,
        owner: Option<usize>,
    ) -> Result<Vec<Import>, ConfigError> {
        let raw: Vec<RawImport> = match tree::get_path(tree, "module.imports") {
            None | Some(Value::Null) => Vec::new(),
            Some(v) => de::from_value(v)
                .map_err(|e| self.locate(decode_error("module.imports", &e), owner))?,
        };
        let mut out = Vec::with_capacity(raw.len());
        for (i, r) in raw.into_iter().enumerate() {
            let Some(mut imp) = r.into_import() else {
                continue;
            };
            if let ImportMounts::These(m) = &imp.mounts {
                check_mounts(m, &format!("module.imports[{i}].mounts"))
                    .map_err(|e| self.locate(e, owner))?;
            }
            if let Some(new) = self.replacements.get(&imp.path) {
                imp.path.clone_from(new);
                imp.replaced = true;
            }
            out.push(imp);
        }
        let names: Vec<String> = match tree.get("theme") {
            Some(Value::Array(a)) => a.iter().filter_map(de::weak_string).collect(),
            Some(v) => de::weak_string(v).into_iter().collect(),
            None => Vec::new(),
        };
        out.extend(names.into_iter().map(Import::theme));
        Ok(out)
    }

    /// Adds `imports` of `owner` (whose directory is `owner_dir`) and, depth first, their
    /// imports.
    pub(super) fn visit(
        &mut self,
        owner: Option<usize>,
        owner_dir: &Path,
        imports: Vec<Import>,
    ) -> Result<(), ConfigError> {
        for imp in imports {
            if imp.path.is_empty() || !self.seen.insert(path_key(&imp.path)) {
                continue;
            }
            let idx = self.add(owner, owner_dir, &imp)?;
            if imp.reads != Reads::All {
                continue;
            }
            let nested = self.imports(&self.out.trees[idx], Some(idx))?;
            let dir = self.out.themes[idx].dir.clone();
            self.visit(Some(idx), &dir, nested)?;
        }
        Ok(())
    }

    /// Finds the theme `imp` of `owner`, reads its configuration and decides its mounts.
    pub(super) fn add(
        &mut self,
        owner: Option<usize>,
        owner_dir: &Path,
        imp: &Import,
    ) -> Result<usize, ConfigError> {
        let owner_path = owner.map(|i| self.out.themes[i].path.clone());
        let (dir, vendored) = self.find(owner_path.as_deref(), owner_dir, imp)?;
        let sources = match imp.reads {
            Reads::Nothing => Sources::default(),
            Reads::All | Reads::ConfigOnly => {
                read_config(&dir, self.environment, self.diagnostics)?
            }
        };
        let tree = sources.merged();
        let own_mounts: Vec<MountConfig> = match tree::get_path(&tree, "module.mounts") {
            None | Some(Value::Null) => Vec::new(),
            Some(v) => de::from_value(v)
                .map_err(|e| locate_in(&sources, decode_error("module.mounts", &e)))?,
        };
        let mounts = match &imp.mounts {
            ImportMounts::None => ThemeMounts::None,
            ImportMounts::These(m) => ThemeMounts::Configured(m.clone()),
            ImportMounts::Own if own_mounts.is_empty() => ThemeMounts::Components,
            ImportMounts::Own => {
                check_mounts(&own_mounts, "module.mounts").map_err(|e| locate_in(&sources, e))?;
                ThemeMounts::Configured(own_mounts)
            }
        };
        self.out.themes.push(Theme {
            path: imp.path.clone(),
            dir,
            owner: owner_path,
            config_files: sources.files.iter().map(|s| s.path.to_path_buf()).collect(),
            mounts,
            vendored,
        });
        self.out.sources.push(sources);
        self.out.trees.push(tree);
        Ok(self.out.themes.len() - 1)
    }

    /// The directory of the theme `imp` imported by `owner` (`None`: the project), and its
    /// `_vendor` version.
    pub(super) fn find(
        &mut self,
        owner: Option<&str>,
        owner_dir: &Path,
        imp: &Import,
    ) -> Result<(PathBuf, Option<String>), ConfigError> {
        if !self.vendor.ignores(&imp.path) {
            self.vendor.read(owner_dir)?;
            if let Some((dir, version)) = self.vendor.listed.get(&imp.path) {
                if !dir.is_dir() {
                    return Err(ConfigError::ThemeNotFound {
                        name: imp.path.clone(),
                        dir: dir.clone(),
                    });
                }
                return Ok((dir.clone(), Some(version.clone())));
            }
        }
        let path = Path::new(&imp.path);
        let anywhere = owner.is_none() || imp.replaced;
        let dir = if path.is_absolute() {
            clean(path)
        } else {
            clean(&self.themes_dir.join(path))
        };
        if !anywhere && !dir.starts_with(&self.themes_dir) {
            return Err(ConfigError::ThemeOutsideThemesDir {
                name: imp.path.clone(),
                owner: owner.unwrap_or_default().to_owned(),
                themes_dir: self.themes_dir.clone(),
            });
        }
        if !dir.is_dir() {
            return Err(ConfigError::ThemeNotFound {
                name: imp.path.clone(),
                dir,
            });
        }
        Ok((dir, None))
    }

    /// Adds the position of the key of a value error in `owner`'s configuration (`None`: the
    /// project's, which the caller locates).
    pub(super) fn locate(&self, e: ConfigError, owner: Option<usize>) -> ConfigError {
        match owner {
            Some(i) => locate_in(&self.out.sources[i], e),
            None => e,
        }
    }
}
