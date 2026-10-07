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

mod config;
mod keys;
mod page;

pub use config::*;
pub use keys::*;
pub use page::*;

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
    /// `[PagerLink]`, one value shared by every pager of the pagination.
    pub pagers: tera::Value,
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

/// The template value of front matter or configuration params.
#[must_use]
pub fn params_value(p: &Params) -> tera::Value {
    Value::Map(std::sync::Arc::new(p.as_map().clone())).to_tera()
}
