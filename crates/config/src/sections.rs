//! Per-language sections: taxonomies, outputs, permalinks, pagination, front matter dates,
//! related content, sitemap, services, menus and cascades.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ssg_base::{FormatId, KindSet, Map, PageKind, Params, Value};

use crate::error::ConfigError;
use crate::output::OutputFormats;

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

/// `[related]`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RelatedConfig {
    /// Minimum score (0–100).
    pub threshold: u8,
    pub include_newer: bool,
    pub to_lower: bool,
    pub indices: Vec<RelatedIndex>,
}

/// One index of `[[related.indices]]`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RelatedIndex {
    /// Front matter key or taxonomy (lower case).
    pub name: String,
    pub kind: RelatedIndexKind,
    pub weight: i32,
    pub cardinality_threshold: i32,
    /// A Go-layout date pattern for date indices.
    pub pattern: String,
    pub to_lower: bool,
    pub apply_filter: bool,
}

/// What a related-content index matches on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RelatedIndexKind {
    /// Front matter values.
    Basic,
    /// Heading ids (`.Fragments`).
    Fragments,
}

impl RelatedConfig {
    /// Go's default: `keywords` (100), `date` (10) and, when there is a `tag` taxonomy,
    /// `tags` (80); threshold 80.
    #[must_use]
    pub fn default_for(has_tags: bool) -> Self {
        let idx = |name: &str, weight| RelatedIndex {
            name: name.to_owned(),
            kind: RelatedIndexKind::Basic,
            weight,
            cardinality_threshold: 0,
            pattern: String::new(),
            to_lower: false,
            apply_filter: false,
        };
        let mut indices = vec![idx("keywords", 100), idx("date", 10)];
        if has_tags {
            indices.push(idx("tags", 80));
        }
        Self {
            threshold: 80,
            include_newer: false,
            to_lower: false,
            indices,
        }
    }

    pub(crate) fn decode(config: &Map) -> Result<Self, crate::de::DeError> {
        #[derive(Deserialize, Default)]
        #[serde(default, rename_all = "camelCase")]
        struct Raw {
            threshold: i64,
            include_newer: bool,
            to_lower: bool,
            indices: Vec<RawIndex>,
        }
        #[derive(Deserialize, Default)]
        #[serde(default, rename_all = "camelCase")]
        struct RawIndex {
            name: String,
            #[serde(rename = "type")]
            kind: String,
            weight: i32,
            cardinality_threshold: i32,
            pattern: String,
            to_lower: bool,
            apply_filter: bool,
        }
        let r: Raw = crate::de::from_map(config)?;
        let threshold = u8::try_from(r.threshold)
            .ok()
            .filter(|t| *t <= 100)
            .ok_or_else(|| crate::de::DeError {
                path: vec!["threshold".to_owned()],
                message: "must be between 0 and 100".to_owned(),
            })?;
        let mut indices = Vec::with_capacity(r.indices.len());
        for (i, ix) in r.indices.into_iter().enumerate() {
            let kind = match ix.kind.to_ascii_lowercase().as_str() {
                "" | "basic" => RelatedIndexKind::Basic,
                "fragments" => RelatedIndexKind::Fragments,
                other => {
                    return Err(crate::de::DeError {
                        path: vec!["indices".to_owned(), i.to_string(), "type".to_owned()],
                        message: format!("unknown index type {other:?} (basic or fragments)"),
                    });
                }
            };
            if !(0..=100).contains(&ix.cardinality_threshold) {
                return Err(crate::de::DeError {
                    path: vec![
                        "indices".to_owned(),
                        i.to_string(),
                        "cardinalityThreshold".to_owned(),
                    ],
                    message: "must be between 0 and 100".to_owned(),
                });
            }
            indices.push(RelatedIndex {
                name: ix.name.to_lowercase(),
                kind,
                weight: ix.weight,
                cardinality_threshold: ix.cardinality_threshold,
                pattern: ix.pattern,
                to_lower: ix.to_lower || r.to_lower,
                apply_filter: ix.apply_filter,
            });
        }
        Ok(Self {
            threshold,
            include_newer: r.include_newer,
            to_lower: r.to_lower,
            indices,
        })
    }
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

/// A menu entry defined in the configuration (`[[menus.main]]`).
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct MenuEntryConfig {
    /// The menu (`main`).
    pub menu: String,
    pub identifier: String,
    pub name: String,
    pub pre: String,
    pub post: String,
    pub url: String,
    pub page_ref: String,
    pub weight: i32,
    pub parent: String,
    pub title: String,
    pub params: Params,
}

pub(crate) fn decode_menus(config: &Map) -> Result<Vec<MenuEntryConfig>, crate::de::DeError> {
    #[derive(Deserialize, Default)]
    #[serde(default, rename_all = "camelCase")]
    struct Raw {
        identifier: String,
        name: String,
        pre: Value,
        post: Value,
        #[serde(rename = "URL")]
        url: String,
        page_ref: String,
        weight: i32,
        parent: String,
        title: String,
        params: Value,
    }
    let mut out = Vec::new();
    for (menu, entries) in config.iter().filter(|(k, _)| *k != "_merge") {
        let items = match entries {
            Value::Array(a) => a.as_slice(),
            other => {
                return Err(crate::de::DeError {
                    path: vec![menu.to_owned()],
                    message: format!("expected a list of menu entries, found {other:?}"),
                });
            }
        };
        for (i, item) in items.iter().enumerate() {
            let r: Raw = crate::de::from_value(item).map_err(|mut e| {
                e.path.splice(0..0, [menu.to_owned(), i.to_string()]);
                e
            })?;
            let html = |field: &str, v: &Value| {
                menu_html(v).ok_or_else(|| crate::de::DeError {
                    path: vec![menu.to_owned(), i.to_string(), field.to_owned()],
                    message: format!("expected a string, found {v:?}"),
                })
            };
            out.push(MenuEntryConfig {
                menu: menu.to_owned(),
                identifier: r.identifier,
                name: r.name,
                pre: html("pre", &r.pre)?,
                post: html("post", &r.post)?,
                url: r.url,
                page_ref: r.page_ref,
                weight: r.weight,
                parent: r.parent,
                title: r.title,
                params: r.params.as_map().map(Params::fold).unwrap_or_default(),
            });
        }
    }
    Ok(out)
}

/// The text of a menu entry's `pre`/`post`: a scalar as written, a boolean as `1`/`0` (how
/// Go reads a boolean into these string fields), nothing when unset.
fn menu_html(v: &Value) -> Option<String> {
    match v {
        Value::Null => Some(String::new()),
        Value::Bool(b) => Some(if *b { "1" } else { "0" }.to_owned()),
        other => crate::de::weak_string(other),
    }
}

/// A `[[cascade]]` entry: front matter applied to the pages below the defining page.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct CascadeConfig {
    /// `params` for the matched pages.
    pub params: Params,
    /// Other front matter fields (`title`, `build`, …), keys lower case.
    pub fields: Params,
    pub target: CascadeTarget,
}

/// Which pages a cascade applies to (`target` or the legacy `_target`); all globs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct CascadeTarget {
    pub kind: String,
    pub path: String,
    pub lang: String,
    pub environment: String,
}

/// Decodes a `cascade` value (a table, a list of tables, or `null` for none): keys fold to
/// lower case; `params` becomes [`CascadeConfig::params`], `target` (or the legacy `_target`)
/// the [`CascadeTarget`], every other key [`CascadeConfig::fields`].
///
/// # Errors
/// A value that is not a table or a list of tables, or a `target` that is not a table of
/// strings; the error's path names the entry.
pub fn decode_cascade(v: &Value) -> Result<Vec<CascadeConfig>, crate::de::DeError> {
    let items = match v {
        Value::Array(a) => a.as_slice(),
        Value::Map(_) => std::slice::from_ref(v),
        Value::Null => &[],
        other => {
            return Err(crate::de::DeError {
                path: Vec::new(),
                message: format!("expected a table or a list of tables, found {other:?}"),
            });
        }
    };
    let mut out = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let Some(m) = item.as_map() else {
            return Err(crate::de::DeError {
                path: vec![i.to_string()],
                message: "expected a table".to_owned(),
            });
        };
        let m = Params::fold(m).into_map();
        let mut entry = CascadeConfig::default();
        let mut fields = Map::new();
        for (k, v) in m.iter() {
            match k {
                "params" => entry.params = v.as_map().map(Params::fold).unwrap_or_default(),
                "target" | "_target" => {
                    entry.target = crate::de::from_value(v).map_err(|mut e| {
                        e.path.splice(0..0, [i.to_string(), k.to_owned()]);
                        e
                    })?;
                }
                _ => {
                    fields.insert(k, v.clone());
                }
            }
        }
        entry.fields = Params::fold(&fields);
        out.push(entry);
    }
    Ok(out)
}
