//! The configuration of a project (docs/rust-port/REWRITE_PLAN.md §2.4, §3.1 A1).
//!
//! One `Value`-tree pipeline, then typed structs:
//!
//! 1. **Bootstrap**: the environment and the config directory come from [`CliOverrides`]
//!    (default environment `production`).
//! 2. **Sources**: the project file (the first of `config.toml`, `config.yaml`,
//!    `config.yml`, `config.json`, then `config.*`; the Go program's configuration file name is
//!    not read; a warning names the others when several exist; or the explicit list), then
//!    `config/_default/**` and `config/<environment>/**` (file names place their content:
//!    `config.*` at the root, `params.toml` under `params`, `menus.en.toml` under
//!    `languages.en.menus`).
//! 3. **Normalise** each tree ([`tree::normalize_keys`]) and migrate legacy keys
//!    ([`tree::migrate_legacy_keys`]).
//! 4. **Merge once**: file < directory < CLI (settings never come from the environment,
//!    [`env`]); then the themes
//!    ([`theme`]: `theme`, `[[module.imports]]` and their themes) are read and their
//!    configuration merged below the project's by Go's `_merge` rules ([`merge`]).
//! 5. **Per language**: `languages.X` over the root (`params` merge deeply, `menus`,
//!    `taxonomies` and `permalinks` replace).
//! 6. **Typed decode** with serde ([`de`]) into structs whose `Default` holds Go's defaults.
//!
//! Beside the configuration, never in it: the project's `.env` file ([`env_file`]), the
//! variables templates read with `get_env`.
//!
//! Errors point at the file, line and column the offending value was written on.

#![forbid(unsafe_code)]

pub mod de;
pub mod duration;
pub mod env;
pub mod env_file;
mod error;
pub mod global;
pub mod markup;
pub mod media;
pub mod merge;
pub mod output;
pub mod sections;
pub mod site;
mod source;
pub mod theme;
pub mod tree;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use ssg_base::diag::Diagnostic;
use ssg_base::{IdVec, Idx, LangIdx, Map, Params, Value};

pub use env_file::EnvFile;
pub use error::ConfigError;
pub use global::{
    BuildConfig, CachesConfig, ContentFilter, Dirs, ImagingConfig, MinifyConfig, MountConfig,
    PrivacyConfig, SecurityPolicy,
};
pub use markup::MarkupConfig;
pub use media::{ContentTypes, MediaType, MediaTypes};
pub use output::{OutputFormat, OutputFormats};
pub use sections::{
    CascadeConfig, CascadeTarget, DateField, DateSource, KindOutputs, Permalinks, SitemapConfig,
    TaxonomyDef, decode_cascade, decode_front_matter,
};
pub use site::{Direction, Language, RedirectPolicy, SiteConfig, TitleConfig};
pub use source::{CONFIG_BASE_NAMES, CONFIG_EXTENSIONS, config_file_names};
pub use theme::{Theme, ThemeMounts};

mod loader;

use loader::*;

/// Settings from the command line; they override the configuration files (the environment
/// overrides them in turn).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct CliOverrides {
    /// `--baseURL`.
    pub base_url: Option<String>,
    /// `--environment` (default `production`).
    pub environment: Option<String>,
    /// `--destination`: the publish directory.
    pub destination: Option<PathBuf>,
    /// `--minify`.
    pub minify: Option<bool>,
    /// `--buildDrafts`.
    pub build_drafts: Option<bool>,
    /// `--buildFuture`.
    pub build_future: Option<bool>,
    /// `--buildExpired`.
    pub build_expired: Option<bool>,
    /// `--cacheDir`.
    pub cache_dir: Option<PathBuf>,
    /// `--themesDir`.
    pub themes_dir: Option<PathBuf>,
    /// `--theme` (comma-separated in the CLI).
    pub theme: Option<Vec<String>>,
    /// `--ignoreCache`.
    pub ignore_cache: Option<bool>,
    /// `--configDir` (default `config`).
    pub config_dir: Option<PathBuf>,
    /// `--noTimes`: the static copy does not copy modification times.
    pub no_times: Option<bool>,
    /// `--noChmod`: the static copy does not copy permissions.
    pub no_chmod: Option<bool>,
}

impl CliOverrides {
    /// The overrides as a configuration tree (lower-case keys).
    #[must_use]
    pub fn to_tree(&self) -> Map {
        let mut m = Map::new();
        let path_str = |p: &Path| Value::string(&p.to_string_lossy());
        let mut set = |k: &str, v: Option<Value>| {
            if let Some(v) = v {
                tree::set_path(&mut m, k, v);
            }
        };
        set("baseurl", self.base_url.as_deref().map(Value::string));
        set(
            "environment",
            self.environment.as_deref().map(Value::string),
        );
        set("publishdir", self.destination.as_deref().map(path_str));
        set("minify.minifyoutput", self.minify.map(Value::Bool));
        set("builddrafts", self.build_drafts.map(Value::Bool));
        set("buildfuture", self.build_future.map(Value::Bool));
        set("buildexpired", self.build_expired.map(Value::Bool));
        set("cachedir", self.cache_dir.as_deref().map(path_str));
        set("themesdir", self.themes_dir.as_deref().map(path_str));
        set(
            "theme",
            self.theme
                .as_ref()
                .map(|t| Value::array(t.iter().map(|s| Value::string(s)).collect())),
        );
        set("ignorecache", self.ignore_cache.map(Value::Bool));
        set("notimes", self.no_times.map(Value::Bool));
        set("nochmod", self.no_chmod.map(Value::Bool));
        m
    }
}

/// What to load.
#[derive(Clone, Debug, Default)]
pub struct LoadOptions {
    /// The project directory (`--source`).
    pub source: PathBuf,
    /// `--config` files, relative to `source`; the first has the highest precedence. Empty:
    /// the first of `config.toml`, `config.yaml`, `config.yml`, `config.json`, `config.*`
    /// ([`config_file_names`]).
    pub config_files: Vec<PathBuf>,
    pub cli: CliOverrides,
    /// The process environment: `HOME`, `XDG_CACHE_HOME`, `TMPDIR` and `USER` for the default
    /// cache directory ([`env::is_read`]; other variables are not read).
    pub env: Vec<(String, String)>,
}

/// The configuration of a project: settings shared by every language, and one
/// [`SiteConfig`] per enabled language.
#[derive(Clone, Debug, Serialize)]
pub struct Config {
    pub project_dir: PathBuf,
    /// `production`, `development`, …
    pub environment: String,
    /// The files the configuration was read from, lowest precedence first: the themes' (the
    /// last theme first), then the project's.
    pub config_files: Vec<PathBuf>,
    /// Enabled languages: the default language first, then by (weight, key).
    pub sites: IdVec<LangIdx, SiteConfig>,
    /// Languages switched off with `disabled` or `disableLanguages`.
    pub disabled_languages: Vec<String>,
    /// Every language has its own `baseURL`.
    pub multihost: bool,
    /// The default language's content is under `/<lang>/` too.
    pub default_language_in_subdir: bool,
    /// Whether the redirect to the default language's home page is written
    /// (`disableDefaultLanguageRedirect`).
    pub default_language_redirect: RedirectPolicy,
    pub output_formats: Arc<OutputFormats>,
    pub media_types: Arc<MediaTypes>,
    pub content_types: ContentTypes,
    /// The format of `.Permalink` and friends outside a page context (`defaultOutputFormat`).
    pub default_output_format: String,
    pub dirs: Dirs,
    /// The resolved `cacheDir`.
    pub cache_dir: PathBuf,
    /// `[[module.mounts]]` as configured (default mounts are added by the file system layer).
    pub mounts: Vec<MountConfig>,
    /// The themes (`theme`, `[[module.imports]]` and their themes), in precedence order.
    pub themes: Vec<Theme>,
    pub build: BuildConfig,
    pub caches: CachesConfig,
    pub security: SecurityPolicy,
    pub privacy: PrivacyConfig,
    pub imaging: ImagingConfig,
    pub minify: MinifyConfig,
    pub content: ContentFilter,
    /// Maximum time for one template execution.
    pub timeout: Duration,
    /// `ignoreFiles`: regular expressions of content paths to skip.
    pub ignore_files: Vec<String>,
    /// `ignoreLogs`: diagnostic ids to drop (lower case).
    pub ignore_logs: Vec<String>,
    pub enable_git_info: bool,
    /// The merged configuration tree (root level, keys lower case).
    pub raw: Params,
    /// The project's `.env` variables (for `get_env`; never printed).
    #[serde(skip)]
    pub env_file: EnvFile,
    /// Deprecations and other notices found while loading.
    #[serde(skip)]
    pub diagnostics: Vec<Diagnostic>,
}

impl Config {
    /// The default language's configuration.
    #[must_use]
    pub fn default_site(&self) -> &SiteConfig {
        &self.sites[LangIdx::from_index(0)]
    }

    /// The configuration of the language with key `key`.
    #[must_use]
    pub fn site(&self, key: &str) -> Option<&SiteConfig> {
        self.sites.iter().find(|s| s.language.key == key)
    }
}

/// Loads the configuration of `o.source`.
///
/// # Errors
/// Unreadable or invalid files, values of the wrong type, or inconsistent languages.
pub fn load(o: &LoadOptions) -> Result<Config, ConfigError> {
    Loader::new(o).run()
}

struct Languages {
    /// Enabled language keys, default first.
    keys: Vec<String>,
    /// The merged tree of each enabled language.
    trees: Vec<Map>,
    /// Each language's own table.
    own: std::collections::BTreeMap<String, Map>,
    disabled: Vec<String>,
    /// `[languages]` is configured.
    configured: bool,
    multihost: bool,
    in_subdir: bool,
}

struct Global {
    default_output_format: String,
    dirs: Dirs,
    cache_dir: PathBuf,
    mounts: Vec<MountConfig>,
    build: BuildConfig,
    caches: CachesConfig,
    security: SecurityPolicy,
    privacy: PrivacyConfig,
    imaging: ImagingConfig,
    minify: MinifyConfig,
    content: ContentFilter,
    timeout: Duration,
    ignore_files: Vec<String>,
    ignore_logs: Vec<String>,
    enable_git_info: bool,
    default_language_redirect: RedirectPolicy,
}

/// Merges a language table over the root tree: tables merge deeply, except `menus`,
/// `taxonomies` and `permalinks`, which the language replaces as a whole.
fn merge_language(tree: &mut Map, lang: &Map) {
    for (k, v) in lang.iter() {
        match k {
            "menus" | "taxonomies" | "permalinks" => {
                tree.insert(k, tree::strip_merge(v));
            }
            _ => {
                let mut single = Map::new();
                single.insert(k, v.clone());
                tree::merge_deep(tree, &single);
            }
        }
    }
}

fn strip_map(m: &Map) -> Map {
    match tree::strip_merge(&Value::map(m.clone())) {
        Value::Map(m) => Arc::unwrap_or_clone(m),
        _ => unreachable!("a table stays a table"),
    }
}

fn section(t: &Map, key: &str) -> Map {
    t.get(key)
        .and_then(Value::as_map)
        .cloned()
        .unwrap_or_default()
}

pub(crate) fn key_segments(key: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in key.split('.') {
        let (name, idx) = match part.find('[') {
            Some(i) => (&part[..i], Some(&part[i..])),
            None => (part, None),
        };
        if !name.is_empty() {
            out.push(name.to_lowercase());
        }
        if let Some(idx) = idx {
            out.extend(
                idx.split(['[', ']'])
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned),
            );
        }
    }
    out
}

/// A typed-decode error under `prefix`, as a [`ConfigError::Invalid`] (position added later).
pub(crate) fn decode_error(prefix: &str, e: &de::DeError) -> ConfigError {
    let path = e.dotted_path();
    let key = match (prefix.is_empty(), path.is_empty()) {
        (true, _) => path,
        (false, true) => prefix.to_owned(),
        (false, false) if path.starts_with('[') => format!("{prefix}{path}"),
        (false, false) => format!("{prefix}.{path}"),
    };
    ConfigError::invalid(key, &e.message)
}

/// A table, with `null` (an unset section) as the empty table.
pub(crate) fn de_map<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Map, D::Error> {
    match <Value as serde::Deserialize>::deserialize(d)? {
        Value::Map(m) => Ok(Arc::unwrap_or_clone(m)),
        Value::Null => Ok(Map::new()),
        other => Err(serde::de::Error::custom(format_args!(
            "expected a table, found {other:?}"
        ))),
    }
}
