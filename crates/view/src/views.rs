//! The view structs (REWRITE_PLAN.md §2.5), serialised once into `tera::Value`s by the
//! [`ViewCache`](crate::ViewCache).
//!
//! Every documented key is always present (`none` for an absent value), because Tera raises an
//! error when an undefined value is printed. The key lists ([`PAGE_SUMMARY_KEYS`],
//! [`CONTENT_KEYS`], [`PAGE_RELATION_KEYS`], [`SITE_KEYS`], …) are what templates may read;
//! the tests print every one of them for every kind.
//!
//! Beyond the plan's list, a page has `name` (`.Name`) and a site `main_sections`
//! (`.Site.MainSections`); menu entries have `key_name` and `parent`.

use jiff::Zoned;
use serde::Serialize;
use ssg_base::{PageKind, Params, Value};
use ssg_config::Config;
use ssg_config::media::MediaType;
use ssg_config::site::SiteConfig;

use crate::content::RenderedContent;

/// The keys of a summary page value (every generation).
pub const PAGE_SUMMARY_KEYS: &[&str] = &[
    "id",
    "kind",
    "lang",
    "path",
    "section",
    "type",
    "layout",
    "bundle_type",
    "name",
    "title",
    "link_title",
    "description",
    "date",
    "lastmod",
    "publish_date",
    "expiry_date",
    "weight",
    "draft",
    "params",
    "keywords",
    "aliases",
    "permalink",
    "rel_permalink",
    "is_home",
    "is_section",
    "is_page",
    "is_node",
    "is_translated",
    "file",
    "git_info",
    "sitemap",
    "language",
    "output_formats",
    "resources",
    "terms",
    "raw_content",
];

/// The content keys a page value has in the Full generations only.
pub const CONTENT_KEYS: &[&str] = &[
    "content",
    "summary",
    "truncated",
    "plain",
    "word_count",
    "fuzzy_word_count",
    "reading_time",
    "table_of_contents",
    "fragments",
    "len",
];

/// The keys a full page value adds to its summary (the rendered page, `deref`, `get_page`).
pub const PAGE_RELATION_KEYS: &[&str] = &[
    "parent",
    "current_section",
    "first_section",
    "ancestors",
    "pages",
    "regular_pages",
    "regular_pages_recursive",
    "sections",
    "prev",
    "next",
    "prev_in_section",
    "next_in_section",
    "translations",
    "all_translations",
    "alternative_output_formats",
    "taxonomy",
    "term",
];

/// The keys of `site`.
pub const SITE_KEYS: &[&str] = &[
    "title",
    "base_url",
    "lang",
    "language_code",
    "language",
    "languages",
    "is_multilingual",
    "copyright",
    "params",
    "data",
    "home",
    "pages",
    "regular_pages",
    "all_pages",
    "sections",
    "main_sections",
    "taxonomies",
    "menus",
    "last_mod",
    "config",
    "sitemap_abs_url",
    "server_port",
];

/// The keys of a page link (`page.terms.<plural>[i]`, alias pages, menu entries' `page`).
pub const PAGE_LINK_KEYS: &[&str] = &[
    "id",
    "kind",
    "path",
    "lang",
    "title",
    "link_title",
    "permalink",
    "rel_permalink",
];

/// The keys of a resource value.
pub const RESOURCE_KEYS: &[&str] = &[
    "__rid",
    "name",
    "title",
    "params",
    "resource_type",
    "media_type",
    "rel_permalink",
    "permalink",
    "width",
    "height",
    "data",
    "page_id",
];

/// The keys of a menu entry.
pub const MENU_ENTRY_KEYS: &[&str] = &[
    "identifier",
    "key_name",
    "name",
    "title",
    "url",
    "weight",
    "parent",
    "pre",
    "post",
    "params",
    "page",
    "children",
    "has_children",
];

/// The keys of a pager (`paginator()`, `paginate()`).
pub const PAGER_KEYS: &[&str] = &[
    "page_number",
    "url",
    "pages",
    "pager_size",
    "total_pages",
    "total_number_of_elements",
    "has_prev",
    "has_next",
    "prev",
    "next",
    "first",
    "last",
    "pagers",
];

/// A date: compare instants with `.unix`, format with the `date` filter.
#[derive(Clone, Debug, Serialize)]
pub struct DateView {
    pub rfc3339: String,
    pub unix: i64,
}

impl DateView {
    #[must_use]
    pub fn new(d: &Zoned) -> Self {
        let rfc3339 = if d.timestamp().subsec_nanosecond() == 0 {
            d.strftime("%Y-%m-%dT%H:%M:%S%:z").to_string()
        } else {
            d.strftime("%Y-%m-%dT%H:%M:%S%.f%:z").to_string()
        };
        Self {
            rfc3339,
            unix: d.timestamp().as_second(),
        }
    }
}

/// `.File` of a page with a content file.
#[derive(Clone, Debug, Serialize)]
pub struct FileView {
    /// The path inside the content directory as written (`posts/My Post.md`).
    pub path: String,
    /// Its directory with a trailing slash (`posts/`; empty at the root).
    pub dir: String,
    /// The file name without extension (`My Post`, `index`).
    pub base_file_name: String,
    /// The bundle directory's name for a bundle index, else the base file name.
    pub content_base_name: String,
    /// The MD5 hex digest of `path`.
    pub unique_id: String,
    pub is_content_adapter: bool,
}

/// `.GitInfo` (always none: `enableGitInfo` is a COULD).
#[derive(Clone, Debug, Serialize)]
pub struct GitInfoView {
    pub hash: String,
    pub abbreviated_hash: String,
    pub subject: String,
    pub author_name: String,
    pub author_email: String,
    pub author_date: DateView,
    pub commit_date: DateView,
}

#[derive(Clone, Debug, Serialize)]
pub struct SitemapView {
    pub change_freq: String,
    /// An integer when it is integral, so that it prints as Go prints a `float64` (`0`, `1`,
    /// `0.5`; Tera prints the float `0.0` as `0.0`).
    #[serde(serialize_with = "go_float")]
    pub priority: f64,
    pub disable: bool,
}

/// Serialises `f` as an integer when it is integral (and exactly representable), else as a
/// float: Tera then prints it as Go's `%v` does (`strconv.FormatFloat(f, 'g', -1, 64)` for the
/// values a template meets).
fn go_float<S: serde::Serializer>(f: &f64, s: S) -> Result<S::Ok, S::Error> {
    if f.fract() == 0.0 && f.abs() < 9.0e15 {
        s.serialize_i64(*f as i64)
    } else {
        s.serialize_f64(*f)
    }
}

/// The content fields of a page value in a Full generation (inserted into its summary map).
#[derive(Clone, Debug, Serialize)]
pub struct ContentView {
    /// HTML (safe).
    pub content: tera::Value,
    /// HTML (safe).
    pub summary: tera::Value,
    pub truncated: bool,
    pub plain: tera::Value,
    pub word_count: usize,
    pub fuzzy_word_count: usize,
    pub reading_time: usize,
    /// HTML (safe).
    pub table_of_contents: tera::Value,
    /// `FragmentsView`.
    pub fragments: tera::Value,
    pub len: usize,
}

impl ContentView {
    /// The content fields of `c` (`None`: a page without content, all fields empty).
    #[must_use]
    pub fn new(c: Option<&RenderedContent>) -> Self {
        let empty = RenderedContent::default();
        let c = c.unwrap_or(&empty);
        Self {
            content: tera::Value::safe_string(&c.html),
            summary: tera::Value::safe_string(&c.summary),
            truncated: c.truncated,
            plain: tera::Value::from(c.plain.as_str()),
            word_count: c.word_count,
            fuzzy_word_count: c.fuzzy_word_count,
            reading_time: c.reading_time,
            table_of_contents: tera::Value::safe_string(&c.table_of_contents),
            fragments: tera::Value::from_serializable(&FragmentsView::new(&c.fragments)),
            len: c.html.len(),
        }
    }
}

/// `.Fragments`.
#[derive(Clone, Debug, Serialize)]
pub struct FragmentsView {
    pub headings: Vec<HeadingView>,
    pub identifiers: Vec<String>,
}

impl FragmentsView {
    #[must_use]
    pub fn new(f: &ssg_markup::Fragments) -> Self {
        Self {
            headings: f.headings.iter().map(HeadingView::new).collect(),
            identifiers: f.identifiers.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct HeadingView {
    pub id: String,
    pub level: u8,
    /// The heading as HTML (rendered without hooks).
    pub title: String,
    pub headings: Vec<HeadingView>,
}

impl HeadingView {
    fn new(h: &ssg_markup::Heading) -> Self {
        Self {
            id: h.id.clone(),
            level: h.level,
            title: h.html.clone(),
            headings: h.children.iter().map(Self::new).collect(),
        }
    }
}

/// The relation-free page value, Arc-shared in every list. The Full generations insert the
/// [`ContentView`] keys into it. Values that many pages share (dates, sitemap settings, media
/// types, empty lists) are pre-serialised once and passed through.
#[derive(Clone, Debug, Serialize)]
#[allow(clippy::struct_excessive_bools)] // template fields (`is_home`, `draft`, …), not state
pub struct PageSummaryView {
    pub id: u32,
    pub kind: PageKind,
    pub lang: String,
    pub path: String,
    pub section: String,
    pub r#type: String,
    pub layout: Option<String>,
    /// `leaf`, `branch`, or none.
    pub bundle_type: Option<&'static str>,
    /// `.Name`.
    pub name: String,
    pub title: String,
    pub link_title: String,
    pub description: String,
    /// `DateView` or none (as the three below).
    pub date: tera::Value,
    pub lastmod: tera::Value,
    pub publish_date: tera::Value,
    pub expiry_date: tera::Value,
    pub weight: i32,
    pub draft: bool,
    pub params: tera::Value,
    /// `[String]` (as `aliases`).
    pub keywords: tera::Value,
    pub aliases: tera::Value,
    /// The primary format's links (`""` when the page has no link).
    pub permalink: String,
    pub rel_permalink: String,
    pub is_home: bool,
    pub is_section: bool,
    pub is_page: bool,
    pub is_node: bool,
    pub is_translated: bool,
    pub file: Option<FileView>,
    pub git_info: Option<GitInfoView>,
    /// `SitemapView`.
    pub sitemap: tera::Value,
    /// `LanguageView`.
    pub language: tera::Value,
    /// `{name: OutputFormatView}` in format order.
    pub output_formats: tera::Value,
    /// `[ResourceView]`.
    pub resources: tera::Value,
    /// `{plural: [PageLink]}`, a key for every configured taxonomy.
    pub terms: tera::Value,
    /// `.RawContent`: the source after the front matter. Known after parsing, so it is in
    /// every generation (shortcodes and hooks read other pages' sources through it); one
    /// value per page, shared by the generations.
    pub raw_content: tera::Value,
}

/// The relations a full page value adds on top of its summary. Every list holds summary values
/// of the same generation (acyclic, Arc-shared).
#[derive(Clone, Debug, Serialize)]
pub struct PageRelations {
    pub parent: Option<tera::Value>,
    pub current_section: tera::Value,
    pub first_section: tera::Value,
    pub ancestors: tera::Value,
    pub pages: tera::Value,
    pub regular_pages: tera::Value,
    pub regular_pages_recursive: tera::Value,
    pub sections: tera::Value,
    pub prev: Option<tera::Value>,
    pub next: Option<tera::Value>,
    pub prev_in_section: Option<tera::Value>,
    pub next_in_section: Option<tera::Value>,
    pub translations: tera::Value,
    pub all_translations: tera::Value,
    /// The output formats other than the primary one.
    pub alternative_output_formats: tera::Value,
    /// Taxonomy pages.
    pub taxonomy: Option<TaxonomyView>,
    /// Term pages.
    pub term: Option<TermView>,
}

/// `page.taxonomy` of a taxonomy page (`.Data.Singular/Plural/Terms`).
#[derive(Clone, Debug, Serialize)]
pub struct TaxonomyView {
    pub singular: String,
    pub plural: String,
    /// `[TermEntryView]` by term key.
    pub terms: tera::Value,
}

/// `page.term` of a term page.
#[derive(Clone, Debug, Serialize)]
pub struct TermView {
    /// `.Name`: the term as first written.
    pub name: String,
    /// `.Data.Term`.
    pub term: String,
    /// The term's key (`blue-sky`).
    pub key: String,
    pub singular: String,
    pub plural: String,
}

/// A term with its pages (`site.taxonomies.<plural>.<key>`, `page.taxonomy.terms`).
#[derive(Clone, Debug, Serialize)]
pub struct TermEntryView {
    pub name: String,
    pub key: String,
    pub count: usize,
    /// The term page (summary).
    pub page: tera::Value,
    /// Its members (summaries): weight, then the default order.
    pub pages: tera::Value,
}

/// A reference to a page (terms, alias pages, menu entries).
#[derive(Clone, Debug, Serialize)]
pub struct PageLink {
    pub id: u32,
    pub kind: PageKind,
    pub path: String,
    pub lang: String,
    pub title: String,
    pub link_title: String,
    pub permalink: String,
    pub rel_permalink: String,
}

/// `site`, per language.
#[derive(Clone, Debug, Serialize)]
pub struct SiteView {
    pub title: String,
    pub base_url: String,
    pub lang: String,
    pub language_code: String,
    pub language: tera::Value,
    pub languages: tera::Value,
    pub is_multilingual: bool,
    pub copyright: String,
    pub params: tera::Value,
    /// `.Site.Data`: shared by every language and generation, keys as written.
    pub data: tera::Value,
    pub home: tera::Value,
    pub pages: tera::Value,
    pub regular_pages: tera::Value,
    pub all_pages: tera::Value,
    pub sections: tera::Value,
    pub main_sections: Vec<String>,
    /// `{plural: {term_key: TermEntryView}}`, terms by key.
    pub taxonomies: tera::Value,
    /// `{menu: [MenuEntryView]}`.
    pub menus: tera::Value,
    pub last_mod: Option<DateView>,
    pub config: tera::Value,
    pub sitemap_abs_url: Option<String>,
    /// The port of the language's base URL, 0 without one (Go's `.Site.ServerPort`; the
    /// server points the base URLs at its listeners).
    pub server_port: u16,
}

#[derive(Clone, Debug, Serialize)]
pub struct LanguageView {
    pub lang: String,
    pub name: String,
    pub code: String,
    pub direction: String,
    pub weight: i32,
    pub params: tera::Value,
}

impl LanguageView {
    #[must_use]
    pub fn new(site: &SiteConfig) -> Self {
        let l = &site.language;
        Self {
            lang: l.key.clone(),
            name: l.name.clone(),
            code: l.code.clone(),
            direction: match l.direction {
                ssg_config::site::Direction::Ltr => "ltr",
                ssg_config::site::Direction::Rtl => "rtl",
            }
            .to_owned(),
            weight: l.weight,
            params: params_value(&site.params),
        }
    }
}

/// A menu entry (`site.menus.<name>`).
#[derive(Clone, Debug, Serialize)]
pub struct MenuEntryView {
    pub identifier: String,
    /// `.KeyName`: the identifier, else the name.
    pub key_name: String,
    pub name: String,
    pub title: String,
    pub url: String,
    pub weight: i32,
    pub parent: Option<String>,
    /// HTML (safe).
    pub pre: tera::Value,
    /// HTML (safe).
    pub post: tera::Value,
    pub params: tera::Value,
    pub page: Option<PageLink>,
    pub children: Vec<MenuEntryView>,
    pub has_children: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct MediaTypeView {
    /// `text/html`: what `{{ .MediaType }}` printed.
    pub r#type: String,
    pub main_type: String,
    pub sub_type: String,
    pub suffixes: Vec<String>,
    pub delimiter: String,
}

impl MediaTypeView {
    #[must_use]
    pub fn new(mt: &MediaType) -> Self {
        Self {
            r#type: if mt.main.is_empty() {
                String::new()
            } else {
                mt.type_string()
            },
            main_type: mt.main.clone(),
            sub_type: mt.sub.clone(),
            suffixes: mt.suffixes.clone(),
            delimiter: mt.delimiter.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct OutputFormatView {
    pub name: String,
    pub rel: String,
    /// `MediaTypeView` (one value per format).
    pub media_type: tera::Value,
    pub permalink: String,
    pub rel_permalink: String,
    pub is_plain_text: bool,
    pub is_html: bool,
}

/// A pager (`paginator()`, `paginate()`).
#[derive(Clone, Debug, Serialize)]
pub struct PagerView {
    pub page_number: u32,
    pub url: String,
    /// Summaries, or `[{key, pages}]` groups.
    pub pages: tera::Value,
    pub pager_size: usize,
    pub total_pages: u32,
    pub total_number_of_elements: usize,
    pub has_prev: bool,
    pub has_next: bool,
    pub prev: Option<PagerLink>,
    pub next: Option<PagerLink>,
    pub first: PagerLink,
    pub last: PagerLink,
    pub pagers: Vec<PagerLink>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PagerLink {
    pub page_number: u32,
    pub url: String,
}

/// `.Data` of a resource.
#[derive(Clone, Debug, Serialize)]
pub struct ResourceDataView {
    /// `Integrity` of a fingerprinted resource (else none).
    pub integrity: Option<String>,
}

/// A resource (bundle files, assets, transform results).
#[derive(Clone, Debug, Serialize)]
pub struct ResourceView {
    /// The store id site functions read back (`ResourceArg`).
    #[serde(rename = "__rid")]
    pub rid: u32,
    pub name: String,
    pub title: String,
    pub params: tera::Value,
    /// `image`, `text`, `page`, …
    pub resource_type: String,
    pub media_type: MediaTypeView,
    /// Links; post-process placeholders when the value is only known in phase E5.
    pub rel_permalink: String,
    pub permalink: String,
    /// Images: the planned size of a processed image, else the size in the file's header
    /// (none for other resources and undecodable images).
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub data: ResourceDataView,
    /// Bundled content pages: `resource_content` gives that page's HTML.
    pub page_id: Option<u32>,
}

/// A shortcode call (`shortcode`); built by the content engine.
#[derive(Clone, Debug, Serialize)]
pub struct ShortcodeView {
    pub name: String,
    pub args: Vec<tera::Value>,
    pub params: tera::Value,
    pub is_named_params: bool,
    pub ordinal: u32,
    pub parent: Option<Box<ShortcodeView>>,
    pub position: String,
}

/// `build`: the program, its version and the build environment.
#[derive(Clone, Debug, Serialize)]
pub struct BuildView {
    /// The program's name (`build.name`), e.g. for a feed's `<generator>`.
    pub name: &'static str,
    /// The program's version (`build.version`).
    pub version: &'static str,
    pub environment: String,
    pub is_production: bool,
    pub is_development: bool,
    pub is_server: bool,
    pub generator: tera::Value,
}

impl BuildView {
    /// `server`: the build runs in the `server` command (`build.is_server`).
    #[must_use]
    pub fn new(cfg: &Config, server: bool) -> Self {
        Self {
            name: ssg_base::APP_NAME,
            version: ssg_base::VERSION,
            environment: cfg.environment.clone(),
            is_production: cfg.environment == "production",
            is_development: cfg.environment == "development",
            is_server: server,
            generator: tera::Value::safe_string(&format!(
                r#"<meta name="generator" content="{} {}">"#,
                ssg_base::APP_NAME,
                ssg_base::VERSION
            )),
        }
    }
}

/// `site.config`: the configuration the embedded templates read, in snake case.
#[derive(Clone, Debug, Serialize)]
pub struct SiteConfigView {
    pub services: ServicesView,
    /// Every service has every switch (false when the service has none).
    pub privacy: PrivacyView,
}

#[derive(Clone, Debug, Serialize)]
pub struct ServicesView {
    pub rss: RssView,
    pub google_analytics: GoogleAnalyticsView,
    pub disqus: DisqusView,
    pub instagram: InlineCssView,
    pub x: InlineCssView,
    pub twitter: InlineCssView,
}

#[derive(Clone, Debug, Serialize)]
pub struct RssView {
    /// `-1`: no limit.
    pub limit: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct GoogleAnalyticsView {
    pub id: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct DisqusView {
    pub shortname: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct InlineCssView {
    pub disable_inline_css: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct PrivacyView {
    pub disqus: PrivacyServiceView,
    pub google_analytics: PrivacyServiceView,
    pub instagram: PrivacyServiceView,
    pub twitter: PrivacyServiceView,
    pub vimeo: PrivacyServiceView,
    pub x: PrivacyServiceView,
    pub youtube: PrivacyServiceView,
}

/// The privacy switches of one service.
#[derive(Clone, Debug, Default, Serialize)]
#[allow(clippy::struct_excessive_bools)] // configuration switches, as Go names them
pub struct PrivacyServiceView {
    pub disable: bool,
    pub simple: bool,
    pub enable_dnt: bool,
    pub respect_do_not_track: bool,
    pub privacy_enhanced: bool,
}

impl SiteConfigView {
    #[must_use]
    pub fn new(cfg: &Config, site: &SiteConfig) -> Self {
        let s = &site.services;
        let p = &cfg.privacy;
        let x = |v: &ssg_config::global::XPrivacy| PrivacyServiceView {
            disable: v.disable,
            simple: v.simple,
            enable_dnt: v.enable_dnt,
            ..PrivacyServiceView::default()
        };
        Self {
            services: ServicesView {
                rss: RssView { limit: s.rss.limit },
                google_analytics: GoogleAnalyticsView {
                    id: s.google_analytics.id.clone(),
                },
                disqus: DisqusView {
                    shortname: s.disqus.shortname.clone(),
                },
                instagram: InlineCssView {
                    disable_inline_css: s.instagram.disable_inline_css,
                },
                x: InlineCssView {
                    disable_inline_css: s.x.disable_inline_css,
                },
                twitter: InlineCssView {
                    disable_inline_css: s.twitter.disable_inline_css,
                },
            },
            privacy: PrivacyView {
                disqus: PrivacyServiceView {
                    disable: p.disqus.disable,
                    ..PrivacyServiceView::default()
                },
                google_analytics: PrivacyServiceView {
                    disable: p.google_analytics.disable,
                    respect_do_not_track: p.google_analytics.respect_do_not_track,
                    ..PrivacyServiceView::default()
                },
                instagram: PrivacyServiceView {
                    disable: p.instagram.disable,
                    simple: p.instagram.simple,
                    ..PrivacyServiceView::default()
                },
                twitter: x(&p.twitter),
                vimeo: x(&p.vimeo),
                x: x(&p.x),
                youtube: PrivacyServiceView {
                    disable: p.youtube.disable,
                    privacy_enhanced: p.youtube.privacy_enhanced,
                    ..PrivacyServiceView::default()
                },
            },
        }
    }
}

/// The template value of front matter or configuration params.
#[must_use]
pub fn params_value(p: &Params) -> tera::Value {
    Value::Map(std::sync::Arc::new(p.as_map().clone())).to_tera()
}
