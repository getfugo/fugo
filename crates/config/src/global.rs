//! Project-wide sections: directories, `[build]`, `[caches]`, `[security]`, `[privacy]`,
//! `[imaging]`, `[minify]` and `[module.mounts]`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use ssg_base::{Map, Value};

use crate::duration::{self, SignedDuration};
use crate::error::ConfigError;

mod caches;
mod security;

pub use caches::*;
pub use security::*;

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
    /// The minifier options as configured, a table per type (`[minify.html]`, `[minify.css]`,
    /// …; `[minify.tdewolff]` is migrated to them); the minify crate maps them.
    pub options: Map,
}

/// The keys of `[minify]` that hold a type's minifier options.
const OPTION_TABLES: [&str; 6] = ["css", "html", "js", "json", "svg", "xml"];

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
        // Checked by the minify crate, which names a value that is not a table.
        let options = m
            .iter()
            .filter(|(k, _)| OPTION_TABLES.iter().any(|t| t.eq_ignore_ascii_case(k)))
            .map(|(k, v)| (k, v.clone()))
            .collect();
        Ok(Self {
            minify_output: r.minify_output,
            disabled,
            options,
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
