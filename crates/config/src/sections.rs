//! Per-language sections: taxonomies, outputs, permalinks, pagination, front matter dates,
//! related content, sitemap, services, menus and cascades.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ssg_base::{FormatId, KindSet, Map, PageKind, Params, Value};

use crate::error::ConfigError;
use crate::output::OutputFormats;

mod menus;
mod related;

pub use menus::*;
pub use related::*;

/// A taxonomy: `tag = "tags"`, or the table `[taxonomies.tag]` with `plural` and
/// `hierarchical`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TaxonomyDef {
    pub singular: String,
    pub plural: String,
    /// The terms form a tree: a `/` in a term nests it, every term above a term exists, and a
    /// term lists the pages of the terms below it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub hierarchical: bool,
}

/// The output formats each page kind is rendered in (disabled kinds have none).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct KindOutputs(BTreeMap<PageKind, Vec<FormatId>>);

impl KindOutputs {
    /// The formats of `kind`, in the configured order.
    #[must_use]
    pub fn get(&self, kind: PageKind) -> &[FormatId] {
        self.0.get(&kind).map_or(&[], Vec::as_slice)
    }

    /// Every kind with its formats.
    pub fn iter(&self) -> impl Iterator<Item = (PageKind, &[FormatId])> {
        self.0.iter().map(|(k, v)| (*k, v.as_slice()))
    }

    /// Decodes `[outputs]` (kind → format names) over Go's defaults, removing disabled kinds
    /// and, when the `rss` kind is disabled, the `rss` format.
    pub(crate) fn decode(
        config: &Map,
        formats: &OutputFormats,
        disabled: KindSet,
        rss_disabled: bool,
    ) -> Result<Self, ConfigError> {
        let mut names: BTreeMap<PageKind, Vec<String>> = BTreeMap::new();
        for kind in PageKind::ALL {
            let dflt: &[&str] = match kind {
                PageKind::Home | PageKind::Section | PageKind::Taxonomy | PageKind::Term => {
                    &["html", "rss"]
                }
                PageKind::Page => &["html"],
                PageKind::NotFound => &["404"],
                PageKind::Sitemap => &["sitemap"],
                PageKind::SitemapIndex => &["sitemapindex"],
                PageKind::RobotsTxt => &["robots"],
            };
            names.insert(kind, dflt.iter().map(|&s| s.to_owned()).collect());
        }
        for (k, v) in config.iter() {
            let Some(kind) = PageKind::parse(k).filter(|k| k.is_content()) else {
                // `rss` and unknown kinds: nothing to configure.
                continue;
            };
            let list: Vec<String> = match v {
                Value::Array(a) => a.iter().filter_map(crate::de::weak_string).collect(),
                other => crate::de::weak_string(other).into_iter().collect(),
            };
            names.insert(kind, list.into_iter().map(|s| s.to_lowercase()).collect());
        }
        let mut out = BTreeMap::new();
        for (kind, list) in names {
            if disabled.contains(kind) {
                continue;
            }
            let mut ids = Vec::with_capacity(list.len());
            for name in list {
                if rss_disabled && name == "rss" {
                    continue;
                }
                let id = formats.by_name(&name).ok_or_else(|| {
                    ConfigError::invalid(
                        format!("outputs.{kind}"),
                        format_args!("unknown output format {name:?}"),
                    )
                })?;
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
            out.insert(kind, ids);
        }
        Ok(Self(out))
    }
}

/// The kinds that can have permalink patterns.
pub const PERMALINK_KINDS: [PageKind; 4] = [
    PageKind::Page,
    PageKind::Section,
    PageKind::Taxonomy,
    PageKind::Term,
];

/// `[permalinks]`: per kind, section (or taxonomy) → pattern.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Permalinks(BTreeMap<PageKind, BTreeMap<String, String>>);

impl Permalinks {
    /// The pattern for pages of `kind` in `section`.
    #[must_use]
    pub fn get(&self, kind: PageKind, section: &str) -> Option<&str> {
        self.0.get(&kind)?.get(section).map(String::as_str)
    }

    /// The patterns of `kind`.
    #[must_use]
    pub fn of_kind(&self, kind: PageKind) -> Option<&BTreeMap<String, String>> {
        self.0.get(&kind)
    }

    /// Decodes a `[permalinks]` table: `[permalinks.page]` style tables per kind (`page`,
    /// `section`, `taxonomy`, `term`), or the legacy flat `section = pattern`, which applies to
    /// pages and terms. Section keys keep their case; `null` is no patterns.
    ///
    /// # Errors
    /// A table for a kind that cannot have permalinks (`home`), or a pattern that is not a
    /// string.
    pub fn decode(config: &Value) -> Result<Self, ConfigError> {
        let mut out: BTreeMap<PageKind, BTreeMap<String, String>> = PERMALINK_KINDS
            .iter()
            .map(|&k| (k, BTreeMap::new()))
            .collect();
        let table = match config {
            Value::Null => return Ok(Self(out)),
            Value::Map(m) => m,
            _ => return Err(ConfigError::invalid("permalinks", "expected a table")),
        };
        let pattern = |key: String, v: &Value| match v {
            Value::String(p) => Ok(p.to_string()),
            _ => Err(ConfigError::invalid(
                key,
                "expected a permalink pattern (a string)",
            )),
        };
        for (k, v) in table.iter() {
            if let Value::Map(m) = v {
                let kind = PageKind::parse(k)
                    .filter(|k| PERMALINK_KINDS.contains(k))
                    .ok_or_else(|| {
                        ConfigError::invalid(
                            format!("permalinks.{k}"),
                            "only page, section, taxonomy and term can have permalinks",
                        )
                    })?;
                let entry = out.entry(kind).or_default();
                for (section, p) in m.iter() {
                    let p = pattern(format!("permalinks.{k}.{section}"), p)?;
                    entry.insert(section.to_owned(), p);
                }
            } else {
                let p = pattern(format!("permalinks.{k}"), v)?;
                for kind in [PageKind::Page, PageKind::Term] {
                    out.entry(kind).or_default().insert(k.to_owned(), p.clone());
                }
            }
        }
        Ok(Self(out))
    }
}

/// `[pagination]`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PaginationConfig {
    pub pager_size: usize,
    /// The path segment before the pager number (`page` → `/page/2/`).
    pub path: String,
    /// No `page/1/` alias.
    pub disable_aliases: bool,
}

impl Default for PaginationConfig {
    fn default() -> Self {
        Self {
            pager_size: 10,
            path: "page".to_owned(),
            disable_aliases: false,
        }
    }
}

/// The date fields of a page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DateField {
    Date,
    Lastmod,
    PublishDate,
    ExpiryDate,
}

impl DateField {
    /// Every date field.
    pub const ALL: [Self; 4] = [
        Self::Date,
        Self::Lastmod,
        Self::PublishDate,
        Self::ExpiryDate,
    ];

    /// The configuration and front matter key (lower case).
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Date => "date",
            Self::Lastmod => "lastmod",
            Self::PublishDate => "publishdate",
            Self::ExpiryDate => "expirydate",
        }
    }

    fn defaults(self) -> &'static [&'static str] {
        match self {
            Self::Date => &[
                "date",
                "publishdate",
                "pubdate",
                "published",
                "lastmod",
                "modified",
            ],
            Self::Lastmod => &[
                ":git",
                "lastmod",
                "modified",
                "date",
                "publishdate",
                "pubdate",
                "published",
            ],
            Self::PublishDate => &["publishdate", "pubdate", "published", "date"],
            Self::ExpiryDate => &["expirydate", "unpublishdate"],
        }
    }
}

/// Where a date comes from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DateSource {
    /// A front matter field (lower case).
    Field(String),
    /// A date at the start of the file name (`:filename`).
    Filename,
    /// The file's modification time (`:fileModTime`).
    FileModTime,
    /// The last Git commit of the file (`:git`).
    Git,
}

impl DateSource {
    /// A source as configured, ignoring case: `:filename`, `:fileModTime`, `:git`, or a front
    /// matter field (kept lower case).
    #[must_use]
    pub fn parse(s: &str) -> Self {
        let s = s.to_lowercase();
        match s.as_str() {
            ":filename" => Self::Filename,
            ":filemodtime" => Self::FileModTime,
            ":git" => Self::Git,
            _ => Self::Field(s),
        }
    }

    /// The configuration spelling (`date`, `:git`).
    #[must_use]
    pub fn as_config_str(&self) -> &str {
        match self {
            Self::Field(f) => f,
            Self::Filename => ":filename",
            Self::FileModTime => ":filemodtime",
            Self::Git => ":git",
        }
    }
}

/// Decodes a `[frontmatter]` table: for each date field (all four, in [`DateField::ALL`]
/// order), its sources in priority order, without duplicates. Keys and sources ignore case. An
/// unconfigured field has Go's defaults; `:default` stands for them inside a list; naming
/// `lastmod`, `publishdate` or `expirydate` includes their aliases (`modified`; `pubdate`,
/// `published`; `unpublishdate`). A scalar is a one-element list; `null` or `[]` is no
/// sources; unknown keys are ignored.
#[must_use]
pub fn decode_front_matter(config: &Map) -> Vec<(DateField, Vec<DateSource>)> {
    let config = Params::fold(config);
    DateField::ALL
        .into_iter()
        .map(|field| {
            let configured: Option<Vec<String>> = config.get(field.key()).map(|v| match v {
                Value::Array(a) => a
                    .iter()
                    .filter_map(crate::de::weak_string)
                    .map(|s| s.to_lowercase())
                    .collect(),
                other => crate::de::weak_string(other)
                    .map(|s| s.to_lowercase())
                    .into_iter()
                    .collect(),
            });
            let mut names: Vec<String> = Vec::new();
            let mut push = |s: &str| {
                if !names.iter().any(|n| n == s) {
                    names.push(s.to_owned());
                }
            };
            match configured {
                None => field.defaults().iter().for_each(|s| push(s)),
                Some(list) => {
                    for s in &list {
                        if s == ":default" {
                            field.defaults().iter().for_each(|s| push(s));
                            continue;
                        }
                        push(s);
                        let aliases: &[&str] = match s.as_str() {
                            "lastmod" => &["modified"],
                            "publishdate" => &["pubdate", "published"],
                            "expirydate" => &["unpublishdate"],
                            _ => &[],
                        };
                        aliases.iter().for_each(|a| push(a));
                    }
                }
            }
            (field, names.iter().map(|s| DateSource::parse(s)).collect())
        })
        .collect()
}

/// `[sitemap]`: the defaults of the sitemap's page entries.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SitemapConfig {
    pub change_freq: String,
    /// `-1` means "not set".
    pub priority: f64,
    pub filename: String,
    pub disable: bool,
}

impl Default for SitemapConfig {
    fn default() -> Self {
        Self {
            change_freq: String::new(),
            priority: -1.0,
            filename: "sitemap.xml".to_owned(),
            disable: false,
        }
    }
}

/// `[services]`: third-party services the embedded templates use.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Services {
    pub disqus: Disqus,
    pub google_analytics: GoogleAnalytics,
    pub instagram: Instagram,
    /// Deprecated spelling of [`Services::x`]; its keys are copied there.
    pub twitter: InlineCss,
    pub x: InlineCss,
    pub rss: Rss,
}

impl Default for Services {
    fn default() -> Self {
        Self {
            disqus: Disqus::default(),
            google_analytics: GoogleAnalytics::default(),
            instagram: Instagram::default(),
            twitter: InlineCss::default(),
            x: InlineCss::default(),
            rss: Rss { limit: -1 },
        }
    }
}

/// `[services.disqus]`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct Disqus {
    pub shortname: String,
}

/// `[services.googleAnalytics]`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct GoogleAnalytics {
    #[serde(rename = "ID")]
    pub id: String,
}

/// `[services.instagram]`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Instagram {
    #[serde(rename = "disableInlineCSS")]
    pub disable_inline_css: bool,
    pub access_token: String,
}

/// `[services.x]`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct InlineCss {
    #[serde(rename = "disableInlineCSS")]
    pub disable_inline_css: bool,
}

/// `[services.rss]`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct Rss {
    /// Maximum items in a feed; `-1` for all.
    pub limit: i64,
}

impl Default for Rss {
    fn default() -> Self {
        Self { limit: -1 }
    }
}
