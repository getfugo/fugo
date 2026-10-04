//! The loader: reads the configuration files, the environment and the languages, and merges them
//! (`load`).

use super::*;

mod globals;
mod languages;

pub(super) struct Loader<'a> {
    pub(super) o: &'a LoadOptions,
    /// The project's configuration files.
    pub(super) sources: source::Sources,
    /// Each theme's configuration files, in precedence order.
    pub(super) theme_sources: Vec<source::Sources>,
    pub(super) diagnostics: Vec<Diagnostic>,
}

impl<'a> Loader<'a> {
    pub(super) fn new(o: &'a LoadOptions) -> Self {
        Self {
            o,
            sources: source::Sources::default(),
            theme_sources: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    pub(super) fn env(&self, name: &str) -> Option<&str> {
        self.o
            .env
            .iter()
            .rev()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    pub(super) fn run(mut self) -> Result<Config, ConfigError> {
        let project = self.o.source.clone();
        let environment = self
            .o
            .cli
            .environment
            .clone()
            .filter(|e| !e.is_empty())
            .unwrap_or_else(|| "production".to_owned());

        // Step 2: sources.
        let config_dir = project.join(
            self.o
                .cli
                .config_dir
                .as_deref()
                .unwrap_or(Path::new("config")),
        );
        self.sources.files =
            source::project_files(&project, &self.o.config_files, &mut self.diagnostics)?;
        for sub in ["_default", environment.as_str()] {
            let dir = config_dir.join(sub);
            if dir.is_dir() {
                self.sources.files.extend(source::dir_files(&dir)?);
            }
        }
        if self.sources.files.is_empty() {
            return Err(ConfigError::NotFound { dir: project });
        }
        let env_file = EnvFile::load(&project, &environment, &mut self.diagnostics)?;

        // Steps 3 and 4: normalise, migrate, merge.
        let mut root = self.sources.merged();
        self.migrate(&mut root);
        tree::merge_deep(&mut root, &tree::normalize_keys(&self.o.cli.to_tree()));
        let mut root = tree::normalize_keys(&root);

        // Step 4, then: the themes (found and read) and their configuration below the
        // project's.
        let themes = theme::collect(&project, &root, &environment, &mut self.diagnostics)
            .map_err(|e| self.locate(e, ""))?;
        if !themes.trees.is_empty() {
            merge::merge_themes(&mut root, &themes.trees);
        }
        self.theme_sources = themes.sources;

        self.migrate(&mut root);
        for key in ["disablekinds", "disablelanguages"] {
            if let Some(v) = root.get(key) {
                let v = tree::split_list(v);
                root.insert(key, v);
            }
        }

        // Step 5: languages.
        let langs = self.languages(&root)?;
        let root = strip_map(&root);
        let default_tree = &langs.trees[0];

        // Step 6: typed decode. Project-wide settings come from the default language.
        let media_types = MediaTypes::decode(&section(default_tree, "mediatypes"))
            .map_err(|e| self.locate(e, &langs.keys[0]))?;
        let output_formats =
            OutputFormats::decode(&section(default_tree, "outputformats"), &media_types)
                .map_err(|e| self.locate(e, &langs.keys[0]))?;
        let content_types =
            ContentTypes::decode(&section(default_tree, "contenttypes"), &media_types)
                .map_err(|e| self.locate(e, &langs.keys[0]))?;
        let duplicate_resources =
            tree::get_path(default_tree, "markup.goldmark.duplicateresourcefiles")
                .and_then(de::weak_bool)
                .unwrap_or(false);
        let use_embedded = if langs.configured && !langs.multihost && !duplicate_resources {
            markup::UseEmbedded::Fallback
        } else {
            markup::UseEmbedded::Auto
        };

        let default_has_tags = match root.get("taxonomies") {
            Some(Value::Map(m)) => m.contains_key("tag"),
            Some(_) => false,
            None => true,
        };
        let mut sites = IdVec::with_capacity(langs.keys.len());
        for (i, (key, tree)) in langs.keys.iter().zip(&langs.trees).enumerate() {
            let lang = LangIdx::from_index(i);
            let url_prefix = if i == 0 && !langs.in_subdir {
                String::new()
            } else {
                key.clone()
            };
            let own = langs.own.get(key).cloned().unwrap_or_default();
            let cx = site::SiteContext {
                lang,
                key,
                own: &own,
                url_prefix,
                output_formats: &output_formats,
                use_embedded,
                default_has_tags,
                diagnostics: &mut self.diagnostics,
            };
            let site = site::decode_site(tree, cx).map_err(|e| self.locate(e, key))?;
            sites.push(site);
        }

        let g = self
            .global(default_tree, &root, &project)
            .map_err(|e| self.locate(e, &langs.keys[0]))?;
        if output_formats.by_name(&g.default_output_format).is_none() {
            return Err(self.locate(
                ConfigError::invalid(
                    "defaultOutputFormat",
                    format_args!("unknown output format {:?}", g.default_output_format),
                ),
                &langs.keys[0],
            ));
        }

        Ok(Config {
            project_dir: project,
            environment,
            config_files: self
                .theme_sources
                .iter()
                .rev()
                .chain(std::iter::once(&self.sources))
                .flat_map(|s| s.files.iter().map(|f| f.path.to_path_buf()))
                .collect(),
            sites,
            disabled_languages: langs.disabled,
            multihost: langs.multihost,
            default_language_in_subdir: langs.in_subdir,
            default_language_redirect: g.default_language_redirect,
            output_formats: Arc::new(output_formats),
            media_types: Arc::new(media_types),
            content_types,
            default_output_format: g.default_output_format,
            dirs: g.dirs,
            cache_dir: g.cache_dir,
            mounts: g.mounts,
            themes: themes.themes,
            build: g.build,
            caches: g.caches,
            security: g.security,
            privacy: g.privacy,
            imaging: g.imaging,
            minify: g.minify,
            content: g.content,
            timeout: g.timeout,
            ignore_files: g.ignore_files,
            ignore_logs: g.ignore_logs,
            enable_git_info: g.enable_git_info,
            raw: Params::fold(&root),
            env_file,
            diagnostics: self.diagnostics,
        })
    }

    /// Migrates legacy keys at the root and in each language table.
    pub(super) fn migrate(&mut self, root: &mut Map) {
        let mut done = tree::migrate_legacy_keys(root);
        if let Some(Value::Map(langs)) = root.get_mut("languages") {
            for (_, l) in Arc::make_mut(langs).iter_mut() {
                if let Value::Map(l) = l {
                    done.extend(tree::migrate_legacy_keys(Arc::make_mut(l)));
                }
            }
        }
        for m in done {
            let message = if m.to.is_empty() {
                format!("config: {} is no longer supported and is ignored", m.from)
            } else {
                format!("config: {} is deprecated; use {}", m.from, m.to)
            };
            self.diagnostics.push(
                Diagnostic::warning(message)
                    .with_id(format!("deprecated-config-{}", m.from.to_lowercase())),
            );
        }
    }

    /// Adds the file position of the offending key to a value error: the project's files
    /// first, then the themes' in precedence order.
    pub(super) fn locate(&self, e: ConfigError, lang: &str) -> ConfigError {
        match e {
            ConfigError::Invalid {
                key,
                position: None,
                message,
            } => {
                let segs = key_segments(&key);
                let mut in_lang = vec!["languages".to_owned(), lang.to_owned()];
                in_lang.extend(segs.iter().cloned());
                let found = std::iter::once(&self.sources)
                    .chain(&self.theme_sources)
                    .find_map(|s| s.locate(&in_lang).or_else(|| s.locate(&segs)));
                match found {
                    Some(found) => ConfigError::Invalid {
                        key: found.dotted_key(),
                        position: Some(found.position),
                        message,
                    },
                    None => ConfigError::Invalid {
                        key,
                        position: None,
                        message,
                    },
                }
            }
            other => other,
        }
    }

    /// `cacheDir`, else `$XDG_CACHE_HOME/<name>_cache` (or `$HOME/.cache/<name>_cache`) when it can
    /// exist, else `$TMPDIR/<name>_cache_$USER`.
    pub(super) fn cache_dir(&self, t: &Map) -> PathBuf {
        if let Some(dir) = t
            .get("cachedir")
            .and_then(de::weak_string)
            .filter(|s| !s.is_empty())
        {
            // A relative directory is rejected when the caches are resolved.
            return PathBuf::from(dir);
        }
        let user_cache = self
            .env("XDG_CACHE_HOME")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .or_else(|| self.env("HOME").map(|h| Path::new(h).join(".cache")));
        if let Some(base) = user_cache {
            let candidate = base.join(format!("{}_cache", ssg_base::APP_NAME));
            let creatable = candidate
                .ancestors()
                .find(|a| a.exists())
                .is_some_and(Path::is_dir);
            if creatable {
                return candidate;
            }
        }
        let tmp = self
            .env("TMPDIR")
            .filter(|s| !s.is_empty())
            .map_or_else(std::env::temp_dir, PathBuf::from);
        match self.env("USER").filter(|s| !s.is_empty()) {
            Some(user) => tmp.join(format!("{}_cache_{user}", ssg_base::APP_NAME)),
            None => tmp.join(format!("{}_cache", ssg_base::APP_NAME)),
        }
    }
}
