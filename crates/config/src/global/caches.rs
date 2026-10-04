//! File caches: their names, directories and ages.

use super::*;

/// A cache buster: when a file matching `source` changes, resources matching `target` are
/// rebuilt.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct CacheBuster {
    pub source: String,
    pub target: String,
}

/// The component directories a mount can target.
pub const COMPONENTS: [&str; 7] = [
    "archetypes",
    "assets",
    "content",
    "data",
    "i18n",
    "layouts",
    "static",
];

/// The names of the file caches.
pub const CACHE_NAMES: [&str; 7] = [
    "assets",
    "getcsv",
    "getjson",
    "getresource",
    "images",
    "misc",
    "modules",
];

/// How long a file cache entry lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MaxAge {
    Forever,
    For(Duration),
}

/// One file cache of `[caches]`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FileCache {
    /// The directory as configured (`:cacheDir/:project`).
    pub dir: String,
    pub max_age: MaxAge,
    /// The resolved absolute directory of this cache's files.
    pub path: PathBuf,
    /// Whether the directory is under the resource directory (`:resourceDir`).
    pub in_resource_dir: bool,
}

/// `[caches]` with the placeholders resolved.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CachesConfig {
    pub caches: BTreeMap<String, FileCache>,
}

impl CachesConfig {
    /// The cache named `name` (see [`CACHE_NAMES`]).
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&FileCache> {
        self.caches.get(name)
    }

    /// Decodes `[caches]`: `dir` may use `:cacheDir`, `:project` (the project directory's
    /// name) and `:resourceDir`; `maxAge` is a duration, a number of seconds, or `-1`
    /// (forever). `ignore_cache` sets every age to zero.
    pub(crate) fn decode(
        m: &Map,
        cache_dir: &Path,
        project: &Path,
        resource_dir: &Path,
        ignore_cache: bool,
    ) -> Result<Self, ConfigError> {
        #[derive(Deserialize, Default)]
        #[serde(default, rename_all = "camelCase")]
        struct Entry {
            dir: Option<String>,
            #[serde(deserialize_with = "duration::de_opt")]
            max_age: Option<SignedDuration>,
        }
        let project_name = project
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut caches = BTreeMap::new();
        for name in CACHE_NAMES {
            let key = format!("caches.{name}");
            let (entry, configured): (Entry, bool) = match m.get(name) {
                Some(Value::Map(e)) => (
                    crate::de::from_map(e).map_err(|e| crate::decode_error(&key, &e))?,
                    true,
                ),
                None => (Entry::default(), false),
                Some(other) => {
                    return Err(ConfigError::invalid(
                        &key,
                        format_args!("expected a table, found {other:?}"),
                    ));
                }
            };
            // A configured cache without `dir` is a project cache.
            let default_dir = match name {
                _ if configured => ":cacheDir/:project",
                "assets" | "images" => ":resourceDir/_gen",
                "modules" => ":cacheDir/modules",
                _ => ":cacheDir/:project",
            };
            let dir = entry
                .dir
                .filter(|d| !d.is_empty())
                .unwrap_or_else(|| default_dir.to_owned());
            let max_age = match entry.max_age {
                _ if ignore_cache => MaxAge::For(Duration::ZERO),
                None => MaxAge::Forever,
                Some(d) if d.negative => MaxAge::Forever,
                Some(d) => MaxAge::For(d.duration),
            };
            let (path, in_resource_dir) = if let Some(rest) = dir.strip_prefix(":resourceDir") {
                let rel = rest.trim_start_matches(['/', '\\']);
                (project.join(resource_dir).join(rel).join(name), true)
            } else {
                let expanded = dir
                    .replace(":cacheDir", &cache_dir.to_string_lossy())
                    .replace(":project", &project_name);
                let p = PathBuf::from(expanded);
                if !p.is_absolute() {
                    return Err(ConfigError::invalid(
                        format!("{key}.dir"),
                        format_args!("{} must resolve to an absolute directory", p.display()),
                    ));
                }
                (p.join("filecache").join(name), false)
            };
            caches.insert(
                name.to_owned(),
                FileCache {
                    dir,
                    max_age,
                    path,
                    in_resource_dir,
                },
            );
        }
        Ok(Self { caches })
    }
}
