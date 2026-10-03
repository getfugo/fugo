//! The configuration of one language of the site ([`SiteConfig`]).

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use ssg_base::url::{Accents, BaseUrl, LinkStyle, PathCase, SiteUrls};
use ssg_base::{IdVec, KindSet, LangIdx, Map, PageKind, Params, TaxonomyIdx, Value, title};

use crate::error::ConfigError;
use crate::markup::{MarkupConfig, UseEmbedded};
use crate::output::OutputFormats;
use crate::sections::{
    CascadeConfig, DateField, DateSource, KindOutputs, MenuEntryConfig, PaginationConfig,
    Permalinks, RelatedConfig, Services, SitemapConfig, TaxonomyDef,
};

/// Text direction of a language.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}

/// A content language.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Language {
    pub idx: LangIdx,
    /// The key in `[languages]` (lower case: `en`, `zh-cn`).
    pub key: String,
    /// `languageName`.
    pub name: String,
    /// `languageCode` (`en-US`); the key when not set.
    pub code: String,
    pub direction: Direction,
    pub weight: i32,
    /// `timeZone` (UTC when not set): local front matter dates are read in it.
    #[serde(serialize_with = "ser_tz")]
    pub time_zone: jiff::tz::TimeZone,
    /// `""` for the default language outside a subdirectory, else the key.
    pub url_prefix: String,
    /// `title` set in the language's own table.
    pub title: String,
}

fn ser_tz<S: serde::Serializer>(tz: &jiff::tz::TimeZone, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(tz.iana_name().unwrap_or("UTC"))
}

/// Whether ugly URLs (`/a.html`) are used: everywhere, nowhere, or per section.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UglyUrls {
    #[default]
    Never,
    Always,
    Sections(BTreeMap<String, bool>),
}

impl UglyUrls {
    /// Whether pages of `section` get ugly URLs.
    #[must_use]
    pub fn in_section(&self, section: &str) -> bool {
        match self {
            Self::Never => false,
            Self::Always => true,
            Self::Sections(m) => m.get(section).copied().unwrap_or(false),
        }
    }

    fn decode(v: Option<&Value>) -> Self {
        match v {
            None | Some(Value::Null) => Self::Never,
            Some(Value::Map(m)) => Self::Sections(
                m.iter()
                    .map(|(k, v)| (k.to_owned(), crate::de::weak_bool(v).unwrap_or(false)))
                    .filter(|(_, on)| *on)
                    .collect(),
            ),
            Some(v) => {
                if crate::de::weak_bool(v).unwrap_or(false) {
                    Self::Always
                } else {
                    Self::Never
                }
            }
        }
    }
}

/// Whether published links are rewritten relative to the page (`relativeURLs`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LinkOutput {
    #[default]
    AsRendered,
    Relative,
}

/// How URLs are made and written.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct UrlPolicy {
    #[serde(skip)]
    pub link_style: LinkStyle,
    pub output: LinkOutput,
    pub ugly: UglyUrls,
    #[serde(skip)]
    pub path_case: PathCase,
    #[serde(skip)]
    pub accents: Accents,
}

/// Whether `/old/` alias pages are written (`disableAliases`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AliasPolicy {
    #[default]
    Write,
    Disabled,
}

/// Whether the redirect to the default language's home page is written
/// (`disableDefaultLanguageRedirect`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RedirectPolicy {
    #[default]
    Write,
    Disabled,
}

/// Whether `robots.txt` is rendered (`enableRobotsTXT`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RobotsPolicy {
    #[default]
    Disabled,
    Enabled,
}

/// Whether `:emoji:` shortcodes are replaced in content (`enableEmoji`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EmojiPolicy {
    #[default]
    Disabled,
    Enabled,
}

/// Automatic titles of list pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct TitleConfig {
    /// `titleCaseStyle`.
    #[serde(skip)]
    pub case_style: title::Style,
    /// `pluralizeListTitles`.
    pub pluralize: bool,
    /// `capitalizeListTitles`.
    pub capitalize: bool,
}

impl Default for TitleConfig {
    fn default() -> Self {
        Self {
            case_style: title::Style::Ap,
            pluralize: true,
            capitalize: true,
        }
    }
}

/// How `ref`/`relref` failures are reported.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RefLinksLevel {
    #[default]
    Error,
    Warning,
}

/// `refLinksErrorLevel`, `refLinksNotFoundURL`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct RefLinksConfig {
    pub level: RefLinksLevel,
    pub not_found_url: String,
}

/// The configuration of one language of the site.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SiteConfig {
    pub lang: LangIdx,
    pub language: Language,
    #[serde(serialize_with = "ser_base_url")]
    pub base_url: BaseUrl,
    pub title: String,
    pub copyright: String,
    pub params: Params,
    pub taxonomies: IdVec<TaxonomyIdx, TaxonomyDef>,
    pub outputs: KindOutputs,
    pub permalinks: Permalinks,
    pub pagination: PaginationConfig,
    pub markup: MarkupConfig,
    pub front_matter: Vec<(DateField, Vec<DateSource>)>,
    pub related: RelatedConfig,
    pub sitemap: SitemapConfig,
    pub services: Services,
    pub menus: Vec<MenuEntryConfig>,
    /// `sectionPagesMenu`: the menu the top-level sections are added to, when set.
    pub section_pages_menu: Option<String>,
    pub cascade: Vec<CascadeConfig>,
    pub urls: UrlPolicy,
    #[serde(serialize_with = "ser_kinds")]
    pub disable_kinds: KindSet,
    pub aliases: AliasPolicy,
    pub robots_txt: RobotsPolicy,
    pub emoji: EmojiPolicy,
    pub titles: TitleConfig,
    pub summary_length: usize,
    /// The language's own content directory (`languages.X.contentDir`).
    pub content_dir: Option<PathBuf>,
    /// The language's own static directories (`languages.X.staticDir`, multihost sites).
    pub static_dirs: Option<Vec<PathBuf>>,
    pub ref_links: RefLinksConfig,
    /// `mainSections` (root or in `params`), when configured.
    pub main_sections: Option<Vec<String>>,
    /// `hasCJKLanguage`: word counts and summaries treat text as CJK.
    pub has_cjk_language: bool,
}

fn ser_base_url<S: serde::Serializer>(u: &BaseUrl, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(u.as_str())
}

fn ser_kinds<S: serde::Serializer>(k: &KindSet, s: S) -> Result<S::Ok, S::Error> {
    s.collect_seq(k.iter().map(PageKind::as_str))
}

impl SiteConfig {
    /// The URL helpers of this language.
    #[must_use]
    pub fn site_urls(&self) -> SiteUrls {
        SiteUrls {
            base_url: self.base_url.clone(),
            language_prefix: self.language.url_prefix.clone(),
            link_style: self.urls.link_style,
            path_case: self.urls.path_case,
            accents: self.urls.accents,
        }
    }

    /// The taxonomy whose plural is `plural`.
    #[must_use]
    pub fn taxonomy(&self, plural: &str) -> Option<TaxonomyIdx> {
        self.taxonomies
            .iter_enumerated()
            .find(|(_, t)| t.plural == plural)
            .map(|(i, _)| i)
    }

    /// The sources of a date field.
    #[must_use]
    pub fn date_sources(&self, field: DateField) -> &[DateSource] {
        self.front_matter
            .iter()
            .find(|(f, _)| *f == field)
            .map_or(&[], |(_, s)| s.as_slice())
    }
}

/// The scalar keys of one language, as configured.
#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "mirrors the configuration keys; converted to enums"
)]
struct Raw {
    #[serde(rename = "baseURL")]
    base_url: String,
    title: String,
    copyright: String,
    language_code: String,
    language_name: String,
    language_direction: String,
    weight: i32,
    time_zone: String,
    #[serde(rename = "canonifyURLs")]
    canonify_urls: bool,
    #[serde(rename = "relativeURLs")]
    relative_urls: bool,
    remove_path_accents: bool,
    disable_path_to_lower: bool,
    disable_aliases: bool,
    #[serde(rename = "enableRobotsTXT")]
    enable_robots_txt: bool,
    enable_emoji: bool,
    title_case_style: String,
    pluralize_list_titles: Option<bool>,
    capitalize_list_titles: Option<bool>,
    summary_length: Option<i64>,
    ref_links_error_level: String,
    #[serde(rename = "refLinksNotFoundURL")]
    ref_links_not_found_url: String,
    #[serde(rename = "hasCJKLanguage")]
    has_cjk_language: bool,
    section_pages_menu: String,
    pagination: PaginationConfig,
    markup: MarkupConfig,
    sitemap: SitemapConfig,
    services: Services,
}

/// What [`decode_site`] needs besides the language's merged tree.
pub(crate) struct SiteContext<'a> {
    pub lang: LangIdx,
    pub key: &'a str,
    /// The language's own table (`languages.X`), for keys that only count there.
    pub own: &'a Map,
    pub url_prefix: String,
    pub output_formats: &'a OutputFormats,
    /// The embedded render hooks' default (`fallback` or `auto`).
    pub use_embedded: UseEmbedded,
    /// The default related-content indices include `tags` (the project has a `tag` taxonomy).
    pub default_has_tags: bool,
    pub diagnostics: &'a mut Vec<ssg_base::diag::Diagnostic>,
}

pub(crate) fn decode_site(tree: &Map, cx: SiteContext<'_>) -> Result<SiteConfig, ConfigError> {
    let raw: Raw = crate::de::from_map(tree).map_err(|e| crate::decode_error("", &e))?;
    let section = |k: &str| {
        tree.get(k)
            .and_then(Value::as_map)
            .cloned()
            .unwrap_or_default()
    };

    let time_zone = if raw.time_zone.is_empty() {
        jiff::tz::TimeZone::UTC
    } else {
        jiff::tz::TimeZone::get(&raw.time_zone).map_err(|e| {
            ConfigError::invalid(
                "timeZone",
                format_args!("unknown time zone {:?}: {e}", raw.time_zone),
            )
        })?
    };
    let language = Language {
        idx: cx.lang,
        key: cx.key.to_owned(),
        name: raw.language_name,
        code: if raw.language_code.is_empty() {
            cx.key.to_owned()
        } else {
            raw.language_code
        },
        direction: if raw.language_direction.eq_ignore_ascii_case("rtl") {
            Direction::Rtl
        } else {
            Direction::Ltr
        },
        weight: raw.weight,
        time_zone,
        url_prefix: cx.url_prefix,
        title: cx
            .own
            .get("title")
            .and_then(crate::de::weak_string)
            .unwrap_or_default(),
    };
    let base_url = BaseUrl::parse(&raw.base_url).map_err(|e| ConfigError::invalid("baseURL", e))?;

    let taxonomies: IdVec<TaxonomyIdx, TaxonomyDef> = match tree.get("taxonomies") {
        None => vec![("category", "categories"), ("tag", "tags")]
            .into_iter()
            .map(|(s, p)| TaxonomyDef {
                singular: s.to_owned(),
                plural: p.to_owned(),
                hierarchical: false,
            })
            .collect(),
        Some(Value::Map(m)) => {
            let mut defs = IdVec::new();
            for (k, v) in m.iter().filter(|(k, _)| *k != "_merge") {
                if let Some(def) = taxonomy_def(k, v)? {
                    defs.push(def);
                }
            }
            defs
        }
        Some(_) => return Err(ConfigError::invalid("taxonomies", "expected a table")),
    };

    let (disable_kinds, rss_disabled) = disabled_kinds(tree.get("disablekinds"), cx.diagnostics);
    let outputs = KindOutputs::decode(
        &section("outputs"),
        cx.output_formats,
        disable_kinds,
        rss_disabled,
    )?;
    let permalinks = Permalinks::decode(tree.get("permalinks").unwrap_or(&Value::Null))?;

    let mut markup = raw.markup;
    for hook in [
        &mut markup.goldmark.render_hooks.image,
        &mut markup.goldmark.render_hooks.link,
    ] {
        hook.use_embedded.get_or_insert(cx.use_embedded);
    }

    let related = match tree.get("related") {
        None => RelatedConfig::default_for(cx.default_has_tags),
        Some(Value::Map(m)) => {
            RelatedConfig::decode(m).map_err(|e| crate::decode_error("related", &e))?
        }
        Some(_) => return Err(ConfigError::invalid("related", "expected a table")),
    };
    let menus = match tree.get("menus") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Map(m)) => {
            crate::sections::decode_menus(m).map_err(|e| crate::decode_error("menus", &e))?
        }
        Some(_) => return Err(ConfigError::invalid("menus", "expected a table")),
    };
    let cascade = crate::sections::decode_cascade(tree.get("cascade").unwrap_or(&Value::Null))
        .map_err(|e| crate::decode_error("cascade", &e))?;

    let params = tree
        .get("params")
        .and_then(Value::as_map)
        .map(Params::fold)
        .unwrap_or_default();
    let main_sections = tree
        .get("mainsections")
        .or_else(|| params.get("mainsections"))
        .map(|v| match v {
            Value::Array(a) => a.iter().filter_map(crate::de::weak_string).collect(),
            other => crate::de::weak_string(other).into_iter().collect(),
        });

    let summary_length = usize::try_from(raw.summary_length.unwrap_or(70))
        .map_err(|_| ConfigError::invalid("summaryLength", "must not be negative"))?;

    Ok(SiteConfig {
        lang: cx.lang,
        language,
        base_url,
        title: raw.title,
        copyright: raw.copyright,
        params,
        taxonomies,
        outputs,
        permalinks,
        pagination: raw.pagination,
        markup,
        front_matter: crate::sections::decode_front_matter(&section("frontmatter")),
        related,
        sitemap: raw.sitemap,
        services: raw.services,
        menus,
        section_pages_menu: Some(raw.section_pages_menu).filter(|s| !s.is_empty()),
        cascade,
        urls: UrlPolicy {
            link_style: if raw.canonify_urls {
                LinkStyle::Canonify
            } else {
                LinkStyle::Relative
            },
            output: if raw.relative_urls {
                LinkOutput::Relative
            } else {
                LinkOutput::AsRendered
            },
            ugly: UglyUrls::decode(tree.get("uglyurls")),
            path_case: if raw.disable_path_to_lower {
                PathCase::Preserve
            } else {
                PathCase::Lower
            },
            accents: if raw.remove_path_accents {
                Accents::Remove
            } else {
                Accents::Keep
            },
        },
        disable_kinds,
        aliases: if raw.disable_aliases {
            AliasPolicy::Disabled
        } else {
            AliasPolicy::Write
        },
        robots_txt: if raw.enable_robots_txt {
            RobotsPolicy::Enabled
        } else {
            RobotsPolicy::Disabled
        },
        emoji: if raw.enable_emoji {
            EmojiPolicy::Enabled
        } else {
            EmojiPolicy::Disabled
        },
        titles: TitleConfig {
            case_style: title::Style::parse(&raw.title_case_style),
            pluralize: raw.pluralize_list_titles.unwrap_or(true),
            capitalize: raw.capitalize_list_titles.unwrap_or(true),
        },
        summary_length,
        content_dir: cx
            .own
            .get("contentdir")
            .and_then(crate::de::weak_string)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from),
        static_dirs: cx.own.get("staticdir").map(|v| match v {
            Value::Array(a) => a
                .iter()
                .filter_map(crate::de::weak_string)
                .map(PathBuf::from)
                .collect(),
            other => crate::de::weak_string(other)
                .into_iter()
                .map(PathBuf::from)
                .collect(),
        }),
        ref_links: RefLinksConfig {
            level: if raw.ref_links_error_level.eq_ignore_ascii_case("warning") {
                RefLinksLevel::Warning
            } else {
                RefLinksLevel::Error
            },
            not_found_url: raw.ref_links_not_found_url,
        },
        main_sections,
        has_cjk_language: raw.has_cjk_language,
    })
}

/// One entry of `[taxonomies]`: `tag = "tags"` or `[taxonomies.tag]` with `plural` and
/// `hierarchical`; `None` for an empty plural (the taxonomy is switched off).
fn taxonomy_def(singular: &str, v: &Value) -> Result<Option<TaxonomyDef>, ConfigError> {
    let (plural, hierarchical) = match v {
        Value::Map(t) => {
            let key = |k: &str| format!("taxonomies.{singular}.{k}");
            if let Some(other) = t
                .keys()
                .find(|k| !matches!(*k, "plural" | "hierarchical" | "_merge"))
            {
                return Err(ConfigError::invalid(key(other), "unknown key"));
            }
            let plural = match t.get("plural") {
                None => String::new(),
                Some(p) => crate::de::weak_string(p)
                    .ok_or_else(|| ConfigError::invalid(key("plural"), "expected a string"))?,
            };
            let hierarchical = match t.get("hierarchical") {
                None => false,
                Some(h) => crate::de::weak_bool(h).ok_or_else(|| {
                    ConfigError::invalid(key("hierarchical"), "expected a boolean")
                })?,
            };
            (plural, hierarchical)
        }
        other => match crate::de::weak_string(other) {
            Some(p) => (p, false),
            None => return Ok(None),
        },
    };
    Ok((!plural.is_empty()).then(|| TaxonomyDef {
        singular: singular.to_owned(),
        plural,
        hierarchical,
    }))
}

/// `disableKinds`: the disabled page kinds, and whether `rss` is disabled. Unknown kinds are
/// reported and ignored.
fn disabled_kinds(
    v: Option<&Value>,
    diagnostics: &mut Vec<ssg_base::diag::Diagnostic>,
) -> (KindSet, bool) {
    let mut set = KindSet::EMPTY;
    let mut rss = false;
    let Some(Value::Array(items)) = v.map(crate::tree::split_list) else {
        return (set, rss);
    };
    for item in items.iter().filter_map(crate::de::weak_string) {
        let lower = item.to_ascii_lowercase();
        if lower == "rss" {
            rss = true;
            continue;
        }
        if lower == "taxonomyterm" {
            diagnostics.push(
                ssg_base::diag::Diagnostic::warning(
                    "disableKinds: the kind \"taxonomyTerm\" is deprecated; use \"taxonomy\"",
                )
                .with_id("deprecated-disablekinds-taxonomyterm"),
            );
        }
        match PageKind::parse(&lower) {
            Some(k) => set.insert(k),
            None => diagnostics.push(ssg_base::diag::Diagnostic::warning(format!(
                "disableKinds: unknown kind {item:?}"
            ))),
        }
    }
    (set, rss)
}
