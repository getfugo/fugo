//! The global configuration: what the site and its languages share.

use super::*;

impl<'a> Loader<'a> {
    pub(super) fn global(
        &self,
        t: &Map,
        root: &Map,
        project: &Path,
    ) -> Result<Global, ConfigError> {
        let dirs = Dirs::from_tree(root);
        let cache_dir = self.cache_dir(t);
        let build: BuildConfig =
            de::from_map(&section(t, "build")).map_err(|e| decode_error("build", &e))?;
        for (i, cb) in build.cache_busters.iter().enumerate() {
            for (field, pattern) in [("source", &cb.source), ("target", &cb.target)] {
                regex::Regex::new(pattern).map_err(|e| {
                    ConfigError::invalid(format!("build.cacheBusters[{i}].{field}"), e)
                })?;
            }
        }
        let ignore_cache = t
            .get("ignorecache")
            .and_then(de::weak_bool)
            .unwrap_or(false);
        let caches = CachesConfig::decode(
            &section(t, "caches"),
            &cache_dir,
            project,
            &dirs.resources,
            ignore_cache,
        )?;
        let security = SecurityPolicy::decode(&section(t, "security"))
            .map_err(|e| decode_error("security", &e))?;
        let privacy: PrivacyConfig =
            de::from_map(&section(t, "privacy")).map_err(|e| decode_error("privacy", &e))?;
        let imaging: ImagingConfig =
            de::from_map(&section(t, "imaging")).map_err(|e| decode_error("imaging", &e))?;
        if !(1..=100).contains(&imaging.quality) {
            return Err(ConfigError::invalid(
                "imaging.quality",
                "must be between 1 and 100",
            ));
        }
        check_notice(&imaging)?;
        let minify =
            MinifyConfig::decode(&section(t, "minify")).map_err(|e| decode_error("minify", &e))?;
        let mounts: Vec<MountConfig> = match tree::get_path(t, "module.mounts") {
            None | Some(Value::Null) => Vec::new(),
            Some(v) => de::from_value(v).map_err(|e| decode_error("module.mounts", &e))?,
        };
        for (i, m) in mounts.iter().enumerate() {
            let component = m
                .target
                .trim_start_matches(['/', '\\'])
                .split(['/', '\\'])
                .next();
            if !component.is_some_and(|c| global::COMPONENTS.contains(&c)) {
                return Err(ConfigError::invalid(
                    format!("module.mounts[{i}].target"),
                    format_args!(
                        "{:?} is not under a component directory ({})",
                        m.target,
                        global::COMPONENTS.join(", ")
                    ),
                ));
            }
        }
        let timeout = match t.get("timeout") {
            None => Duration::from_secs(60),
            Some(v) => duration::from_value(v)
                .map(|d| {
                    if d.negative {
                        Duration::ZERO
                    } else {
                        d.duration
                    }
                })
                .map_err(|e| ConfigError::invalid("timeout", e))?,
        };
        let strings = |k: &str| -> Vec<String> {
            match t.get(k) {
                Some(Value::Array(a)) => a.iter().filter_map(de::weak_string).collect(),
                Some(v) => de::weak_string(v).into_iter().collect(),
                None => Vec::new(),
            }
        };
        for (i, p) in strings("ignorefiles").iter().enumerate() {
            regex::Regex::new(p)
                .map_err(|e| ConfigError::invalid(format!("ignoreFiles[{i}]"), e))?;
        }
        let flag = |k: &str| t.get(k).and_then(de::weak_bool).unwrap_or(false);
        let default_output_format = t
            .get("defaultoutputformat")
            .and_then(de::weak_string)
            .filter(|s| !s.is_empty())
            .map_or_else(|| "html".to_owned(), |s| s.to_lowercase());
        Ok(Global {
            default_output_format,
            dirs,
            cache_dir,
            mounts,
            build,
            caches,
            security,
            privacy,
            imaging,
            minify,
            content: ContentFilter {
                drafts: flag("builddrafts"),
                future: flag("buildfuture"),
                expired: flag("buildexpired"),
            },
            timeout,
            ignore_files: strings("ignorefiles"),
            ignore_logs: strings("ignorelogs")
                .iter()
                .map(|s| s.to_lowercase())
                .collect(),
            enable_git_info: flag("enablegitinfo"),
            default_language_redirect: if flag("disabledefaultlanguageredirect") {
                RedirectPolicy::Disabled
            } else {
                RedirectPolicy::Write
            },
        })
    }
}

/// The longest copyright notice or author `[imaging]` takes, in bytes: they go into every
/// published image.
const NOTICE_MAX: usize = 1000;

/// `[imaging]`'s copyright notice, author and title: text without control characters, braces
/// only in the copyright's `{year}`, and only with `stripMetadata` (an image's own metadata is
/// replaced to write them).
fn check_notice(imaging: &ImagingConfig) -> Result<(), ConfigError> {
    for (key, text) in [
        ("imaging.copyright", &imaging.copyright),
        ("imaging.author", &imaging.author),
    ] {
        if text.len() > NOTICE_MAX || text.chars().any(char::is_control) {
            return Err(ConfigError::invalid(
                key,
                format!("must be at most {NOTICE_MAX} bytes of text without control characters"),
            ));
        }
    }
    // A typo such as `{yaer}` would otherwise go into every image as it is.
    if let Some(brace) = stray_brace(&imaging.copyright) {
        return Err(ConfigError::invalid(
            "imaging.copyright",
            format!(
                "{brace:?} is not a placeholder: the only one is {}",
                ImagingConfig::YEAR
            ),
        ));
    }
    let notice =
        !imaging.copyright.is_empty() || !imaging.author.is_empty() || imaging.title_from_file_name;
    if notice && !imaging.strip_metadata {
        return Err(ConfigError::invalid(
            "imaging.stripMetadata",
            "must be true to write copyright, author or titleFromFileName into images: their \
             own metadata is replaced",
        ));
    }
    Ok(())
}

/// The first brace of `text` outside an [`ImagingConfig::YEAR`], with what it encloses: `{yaer}`,
/// or the lone `{` or `}`.
fn stray_brace(text: &str) -> Option<&str> {
    let mut rest = text;
    while let Some(i) = rest.find(['{', '}']) {
        let tail = &rest[i..];
        if let Some(after) = tail.strip_prefix(ImagingConfig::YEAR) {
            rest = after;
            continue;
        }
        return Some(match tail.find('}') {
            Some(end) if tail.starts_with('{') => &tail[..=end],
            _ => &tail[..1],
        });
    }
    None
}
