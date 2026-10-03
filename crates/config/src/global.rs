//! Project-wide sections: directories, `[build]`, `[caches]`, `[security]`, `[privacy]`,
//! `[imaging]`, `[minify]` and `[module.mounts]`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use ssg_base::{Map, Value};

use crate::duration::{self, SignedDuration};
use crate::error::ConfigError;

/// The project's component directories, as configured (relative to the project directory
/// unless absolute).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Dirs {
    pub content: PathBuf,
    pub data: PathBuf,
    pub layouts: PathBuf,
    pub i18n: PathBuf,
    pub archetypes: PathBuf,
    pub assets: PathBuf,
    pub resources: PathBuf,
    pub publish: PathBuf,
    pub themes: PathBuf,
    /// `staticDir`, then `staticDir0` … `staticDir10`, without duplicates.
    pub static_dirs: Vec<PathBuf>,
}

impl Default for Dirs {
    fn default() -> Self {
        Self {
            content: "content".into(),
            data: "data".into(),
            layouts: "layouts".into(),
            i18n: "i18n".into(),
            archetypes: "archetypes".into(),
            assets: "assets".into(),
            resources: "resources".into(),
            publish: "public".into(),
            themes: "themes".into(),
            static_dirs: vec!["static".into()],
        }
    }
}

impl Dirs {
    pub(crate) fn from_tree(m: &Map) -> Self {
        let d = Self::default();
        let get = |k: &str, dflt: PathBuf| {
            m.get(k)
                .and_then(crate::de::weak_string)
                .filter(|s| !s.is_empty())
                .map_or(dflt, PathBuf::from)
        };
        let mut static_dirs: Vec<PathBuf> = Vec::new();
        let mut push_static = |v: &Value| {
            let items: Vec<String> = match v {
                Value::Array(a) => a.iter().filter_map(crate::de::weak_string).collect(),
                other => crate::de::weak_string(other).into_iter().collect(),
            };
            for s in items {
                let p = PathBuf::from(s);
                if !static_dirs.contains(&p) {
                    static_dirs.push(p);
                }
            }
        };
        match m.get("staticdir") {
            Some(v) => push_static(v),
            None => push_static(&Value::string("static")),
        }
        for i in 0..=10 {
            if let Some(v) = m.get(&format!("staticdir{i}")) {
                push_static(v);
            }
        }
        Self {
            content: get("contentdir", d.content),
            data: get("datadir", d.data),
            layouts: get("layoutdir", d.layouts),
            i18n: get("i18ndir", d.i18n),
            archetypes: get("archetypedir", d.archetypes),
            assets: get("assetdir", d.assets),
            resources: get("resourcedir", d.resources),
            publish: get("publishdir", d.publish),
            themes: get("themesdir", d.themes),
            static_dirs,
        }
    }
}

/// `[build]`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BuildConfig {
    pub cache_busters: Vec<CacheBuster>,
    /// `fallback` (default), `always` or `never`.
    pub use_resource_cache_when: String,
    #[serde(rename = "noJSConfigInAssets")]
    pub no_js_config_in_assets: bool,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            cache_busters: Vec::new(),
            use_resource_cache_when: "fallback".to_owned(),
            no_js_config_in_assets: false,
        }
    }
}

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

/// An allow-list of regular expressions (`[security]`).
#[derive(Clone, Debug)]
pub struct Whitelist {
    patterns: Vec<String>,
    compiled: Vec<regex::Regex>,
}

impl PartialEq for Whitelist {
    fn eq(&self, other: &Self) -> bool {
        self.patterns == other.patterns
    }
}

impl Eq for Whitelist {}

impl Serialize for Whitelist {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.patterns.serialize(s)
    }
}

impl Whitelist {
    fn new(patterns: &[&str]) -> Self {
        Self::compile(patterns.iter().map(|&p| p.to_owned()).collect())
            .expect("built-in patterns compile")
    }

    fn compile(patterns: Vec<String>) -> Result<Self, regex::Error> {
        let compiled = patterns
            .iter()
            .filter(|p| !p.eq_ignore_ascii_case("none") && !p.is_empty())
            .map(|p| regex::Regex::new(p))
            .collect::<Result<_, _>>()?;
        Ok(Self { patterns, compiled })
    }

    /// Whether nothing is allowed (`"none"` or an empty list).
    #[must_use]
    pub fn is_none(&self) -> bool {
        self.compiled.is_empty()
    }

    /// Whether `s` matches one of the patterns.
    #[must_use]
    pub fn accepts(&self, s: &str) -> bool {
        self.compiled.iter().any(|r| r.is_match(s))
    }

    /// The patterns as configured (`["none"]` when configured so).
    #[must_use]
    pub fn patterns(&self) -> &[String] {
        &self.patterns
    }
}

fn whitelist<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Whitelist>, D::Error> {
    let v = <Vec<String> as Deserialize>::deserialize(d)?;
    Whitelist::compile(v)
        .map(Some)
        .map_err(serde::de::Error::custom)
}

/// `[security]`: what templates and resource pipelines may execute, read or fetch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SecurityPolicy {
    /// External programs (`exec.allow`).
    pub exec_allow: Whitelist,
    /// Environment variables passed to them (`exec.osEnv`).
    pub exec_os_env: Whitelist,
    /// Environment variables `getenv` may read (`funcs.getenv`).
    pub getenv: Whitelist,
    /// URLs `resources.GetRemote` may fetch (`http.urls`).
    pub http_urls: Whitelist,
    /// HTTP methods (`http.methods`).
    pub http_methods: Whitelist,
    /// Media types of remote resources (`http.mediaTypes`); empty allows any.
    pub http_media_types: Whitelist,
    pub inline_shortcodes: InlineShortcodes,
}

/// Whether content may define inline shortcodes (`enableInlineShortcodes`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InlineShortcodes {
    #[default]
    Disabled,
    Enabled,
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        Self {
            exec_allow: Whitelist::new(&["^(dart-)?sass(-embedded)?$", "^go$", "^git$", "^npx$"]),
            exec_os_env: Whitelist::new(&[
                r"(?i)^((HTTPS?|NO)_PROXY|PATH(EXT)?|APPDATA|TE?MP|TERM|GO\w+|(XDG_CONFIG_)?HOME|USERPROFILE|SSH_AUTH_SOCK|DISPLAY|LANG|SYSTEMDRIVE)$",
            ]),
            getenv: Whitelist::new(&[concat!("^", ssg_base::env_var!("")), "^CI$"]),
            http_urls: Whitelist::new(&[".*"]),
            http_methods: Whitelist::new(&["(?i)GET|POST"]),
            http_media_types: Whitelist::new(&[]),
            inline_shortcodes: InlineShortcodes::Disabled,
        }
    }
}

impl SecurityPolicy {
    pub(crate) fn decode(m: &Map) -> Result<Self, crate::de::DeError> {
        #[derive(Deserialize, Default)]
        #[serde(default, rename_all = "camelCase")]
        struct Raw {
            exec: Exec,
            funcs: Funcs,
            http: Http,
            enable_inline_shortcodes: bool,
        }
        #[derive(Deserialize, Default)]
        #[serde(default, rename_all = "camelCase")]
        struct Exec {
            #[serde(deserialize_with = "whitelist")]
            allow: Option<Whitelist>,
            #[serde(deserialize_with = "whitelist")]
            os_env: Option<Whitelist>,
        }
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Funcs {
            #[serde(deserialize_with = "whitelist")]
            getenv: Option<Whitelist>,
        }
        #[derive(Deserialize, Default)]
        #[serde(default, rename_all = "camelCase")]
        struct Http {
            #[serde(deserialize_with = "whitelist")]
            urls: Option<Whitelist>,
            #[serde(deserialize_with = "whitelist")]
            methods: Option<Whitelist>,
            #[serde(deserialize_with = "whitelist")]
            media_types: Option<Whitelist>,
        }
        let r: Raw = crate::de::from_map(m)?;
        let d = Self::default();
        Ok(Self {
            exec_allow: r.exec.allow.unwrap_or(d.exec_allow),
            exec_os_env: r.exec.os_env.unwrap_or(d.exec_os_env),
            getenv: r.funcs.getenv.unwrap_or(d.getenv),
            http_urls: r.http.urls.unwrap_or(d.http_urls),
            http_methods: r.http.methods.unwrap_or(d.http_methods),
            http_media_types: r.http.media_types.unwrap_or(d.http_media_types),
            inline_shortcodes: if r.enable_inline_shortcodes {
                InlineShortcodes::Enabled
            } else {
                InlineShortcodes::Disabled
            },
        })
    }
}

/// `[privacy]`: privacy switches of the embedded templates and shortcodes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PrivacyConfig {
    pub disqus: Disableable,
    pub google_analytics: GoogleAnalyticsPrivacy,
    pub instagram: SimplePrivacy,
    /// Deprecated spelling of [`PrivacyConfig::x`]; its keys are copied there.
    pub twitter: XPrivacy,
    pub vimeo: XPrivacy,
    pub x: XPrivacy,
    pub youtube: YouTubePrivacy,
}

/// A service that can be disabled.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct Disableable {
    pub disable: bool,
}

/// `[privacy.googleAnalytics]`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GoogleAnalyticsPrivacy {
    pub disable: bool,
    pub respect_do_not_track: bool,
}

/// `[privacy.instagram]`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct SimplePrivacy {
    pub disable: bool,
    pub simple: bool,
}

/// `[privacy.x]`, `[privacy.vimeo]`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct XPrivacy {
    pub disable: bool,
    #[serde(rename = "enableDNT")]
    pub enable_dnt: bool,
    pub simple: bool,
}

/// `[privacy.youtube]`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct YouTubePrivacy {
    pub disable: bool,
    pub privacy_enhanced: bool,
}

/// `[imaging]`, as configured (the images crate interprets it).
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ImagingConfig {
    pub resample_filter: String,
    pub quality: i32,
    pub anchor: String,
    pub hint: String,
    pub bg_color: String,
    pub compression: String,
    #[serde(deserialize_with = "crate::de_map")]
    pub exif: Map,
}

impl Default for ImagingConfig {
    fn default() -> Self {
        Self {
            resample_filter: "box".to_owned(),
            quality: 75,
            anchor: String::new(),
            hint: "photo".to_owned(),
            bg_color: "#ffffff".to_owned(),
            compression: String::new(),
            exif: Map::new(),
        }
    }
}

/// An output type the minifier handles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MinifyTarget {
    Css,
    Html,
    Js,
    Json,
    Svg,
    Xml,
}

/// `[minify]`.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct MinifyConfig {
    /// Minify every rendered output (`minifyOutput`, or `--minify`).
    pub minify_output: bool,
    /// Output types excluded (`disableHTML`, …).
    pub disabled: Vec<MinifyTarget>,
    /// Minifier options as configured (`[minify.tdewolff]`); the minify crate maps them.
    pub options: Map,
}

impl MinifyConfig {
    pub(crate) fn decode(m: &Map) -> Result<Self, crate::de::DeError> {
        #[derive(Deserialize, Default)]
        #[serde(default, rename_all = "camelCase")]
        #[expect(
            clippy::struct_excessive_bools,
            reason = "mirrors the configuration keys"
        )]
        struct Raw {
            minify_output: bool,
            #[serde(rename = "disableCSS")]
            disable_css: bool,
            #[serde(rename = "disableHTML")]
            disable_html: bool,
            #[serde(rename = "disableJS")]
            disable_js: bool,
            #[serde(rename = "disableJSON")]
            disable_json: bool,
            #[serde(rename = "disableSVG")]
            disable_svg: bool,
            #[serde(rename = "disableXML")]
            disable_xml: bool,
            #[serde(deserialize_with = "crate::de_map")]
            tdewolff: Map,
        }
        let r: Raw = crate::de::from_map(m)?;
        let disabled = [
            (r.disable_css, MinifyTarget::Css),
            (r.disable_html, MinifyTarget::Html),
            (r.disable_js, MinifyTarget::Js),
            (r.disable_json, MinifyTarget::Json),
            (r.disable_svg, MinifyTarget::Svg),
            (r.disable_xml, MinifyTarget::Xml),
        ]
        .into_iter()
        .filter_map(|(on, t)| on.then_some(t))
        .collect();
        Ok(Self {
            minify_output: r.minify_output,
            disabled,
            options: r.tdewolff,
        })
    }
}

/// A `[[module.mounts]]` entry.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MountConfig {
    pub source: String,
    pub target: String,
    /// The content language of the mounted files.
    pub lang: Option<String>,
    #[serde(deserialize_with = "string_or_list")]
    pub include_files: Vec<String>,
    #[serde(deserialize_with = "string_or_list")]
    pub exclude_files: Vec<String>,
    pub disable_watch: bool,
}

fn string_or_list<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<String>, D::Error> {
    <Vec<String> as Deserialize>::deserialize(d)
}

/// Whether published pages include drafts, future and expired content.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ContentFilter {
    /// `buildDrafts`.
    pub drafts: bool,
    /// `buildFuture`.
    pub future: bool,
    /// `buildExpired`.
    pub expired: bool,
}
