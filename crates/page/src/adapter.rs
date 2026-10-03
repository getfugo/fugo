//! Pages from content adapters: the map a `_content.html` template passes to `add_page`
//! (Go's `pagesfromdata/pagesfromgotmpl.go` with the `IsFromContentAdapter` paths of
//! `resources/page/pagemeta/page_frontmatter.go` and the site builder's `page__meta.go`).
//!
//! An adapter page is configured differently from a page read from a file:
//!
//! - **Placing** ([`AdapterPage::decode`], Go's `PageConfigEarly` with `Init(true)` and
//!   `CompileForPagesFromDataPre`): `kind` (default `page`; one of the five content kinds as
//!   written, in lower case), `path` (one leading `/` removed, then joined to the adapter's
//!   directory with `path.Join` and normalised: lower case, spaces → `-`, not trimmed; empty
//!   only for the home page),
//!   `content` (`mediaType`, default Markdown, and `value`), `cascade` (branch kinds only). `lang`
//!   and `content.markup` must not be set. The page is then parsed as the content file
//!   `<path>/index.<suffix>` (`_index.<suffix>` for branch kinds) of its media type.
//! - **Fields** ([`meta_from_adapter`], Go's `PageConfig.Compile` decoding the map): the
//!   reserved front matter fields are read from the map's top level, filled by the cascade's
//!   fields; `.Params` are the cascade's params with the map's `params` over them (top-level
//!   keys replaced) and nothing else (no reserved keys, no dates). No `_build`, `headless`,
//!   `published`, `menu`, `resources` or `markup`; the sitemap settings start from zero
//!   (priority 0), not from the site's `[sitemap]`.
//! - **Dates** ([`DateResolver::adapter_dates`]): `dates.{date, lastmod, publishDate,
//!   expiryDate}`, then for each date in turn the configured `[frontmatter]` chain over those
//!   four as the earlier chains left them (aliases, custom keys and `:` sources skipped).

use jiff::tz::TimeZone;
use ssg_base::paths::{self, ContentKey};
use ssg_base::{PageKind, Params, Value};
use ssg_config::MediaTypes;
use ssg_config::sections::SitemapConfig;
use ssg_config::{DateField, DateSource};

use crate::build::BuildPolicy;
use crate::cascade::Cascade;
use crate::dates::{DateResolver, Dates, to_date};
use crate::markup::{Markup, MarkupSource};
use crate::meta::{Cjk, MetaCtx, PageMeta, decode_menus, decode_sitemap};
use crate::{PageError, value};

/// The placing fields of an `add_page` map.
#[derive(Clone, Debug)]
pub struct AdapterPage {
    pub kind: PageKind,
    /// The page's logical path below the content root, normalised, without a leading slash
    /// (`news/v0-151-0`; empty for the home page).
    pub path: String,
    pub markup: Markup,
    /// The first suffix of the content's media type (`md`, `html`): the page's path is parsed
    /// as a content file with it.
    pub suffix: String,
    /// `content.value`.
    pub content: String,
    /// The page's own `cascade` (branch kinds only).
    pub cascade: Cascade,
    /// The whole map, folded: the page's front matter fields.
    pub fields: Params,
}

impl AdapterPage {
    /// Decodes an `add_page` map of the adapter whose directory is `base` (the adapter's key:
    /// `news` for `content/news/_content.html`).
    ///
    /// # Errors
    /// [`PageError::Adapter`] for a `kind` that is not a content kind in lower case, an empty
    /// path of a page that is not the home page, `lang` or `content.markup` set, or a map that
    /// is not a table; [`PageError::UnknownMarkup`] and [`PageError::UnsupportedMarkup`] for
    /// the media type; [`PageError::Cascade`] for a cascade on a regular page.
    pub fn decode(map: &Params, base: &ContentKey, types: &MediaTypes) -> Result<Self, PageError> {
        let string = |key: &str| -> Result<String, PageError> {
            map.get(key).map_or(Ok(String::new()), |v| {
                value::weak_string(v).ok_or_else(|| {
                    PageError::field(key, format!("expected a string, found {}", value::kind(v)))
                })
            })
        };
        // Go decodes `kind` as it is (no case folding, no legacy names, unlike front
        // matter's `kinds.GetKindMain`).
        let kind = match string("kind")?.as_str() {
            "" => PageKind::Page,
            k => PageKind::parse(k)
                .filter(|p| p.is_content() && p.as_str() == k)
                .ok_or_else(|| {
                    PageError::Adapter(format!(
                        "`kind` {k:?} is not one of page, home, section, taxonomy, term (in \
                         lower case; the Go implementation adds a page of another kind that is \
                         never rendered)"
                    ))
                })?,
        };
        // `Init(true)`: one leading slash is removed; the path is not trimmed.
        let path = string("path")?;
        let path = path.strip_prefix('/').unwrap_or(&path);
        if path.is_empty() && kind != PageKind::Home {
            return Err(PageError::Adapter(
                "`path` is empty: only the home page (`kind: home`) has an empty path".into(),
            ));
        }
        if !string("lang")?.is_empty() {
            return Err(PageError::Adapter(
                "`lang` cannot be set: an adapter adds its pages to the language it runs for \
                 (`enable_all_languages()` runs it for every language)"
                    .into(),
            ));
        }
        let content = match map.get("content") {
            None | Some(Value::Null) => Params::default(),
            Some(Value::Map(m)) => Params::fold(m),
            Some(other) => {
                return Err(PageError::field(
                    "content",
                    format!("expected a table, found {}", value::kind(other)),
                ));
            }
        };
        let content_string = |key: &str| {
            content
                .get(key)
                .map(|v| value::weak_string(v).unwrap_or_default())
                .unwrap_or_default()
        };
        if !content_string("markup").is_empty() {
            return Err(PageError::Adapter(
                "`content.markup` cannot be set: use `content.mediaType`".into(),
            ));
        }
        let media_type = content_string("mediatype");
        let (markup, suffix) = if media_type.is_empty() {
            (Markup::Markdown, markdown_suffix(types))
        } else {
            let id = types
                .by_type(&media_type)
                .ok_or_else(|| PageError::UnknownMarkup(media_type.clone()))?;
            let markup = Markup::detect(
                MarkupSource {
                    media_type: Some(&media_type),
                    ..MarkupSource::default()
                },
                types,
            )?;
            (markup, types.get(id).first_suffix().to_owned())
        };
        let cascade = match map.get("cascade") {
            None | Some(Value::Null) => Cascade::default(),
            Some(v) if kind.is_branch() => Cascade::decode(v)?,
            Some(_) => {
                return Err(PageError::Cascade(format!(
                    "only branch pages can cascade, not {kind} pages"
                )));
            }
        };
        let joined = paths::join(&["/", &base.to_path(), path]);
        Ok(Self {
            kind,
            path: paths::normalize_key(joined.trim_start_matches('/')),
            markup,
            suffix,
            content: content.get("value").map_or_else(String::new, content_text),
            cascade,
            fields: map.clone(),
        })
    }

    /// The content path the page is read as: `/<path>/index.<suffix>`, or `_index.<suffix>`
    /// for branch kinds (`/news/v0-151-0/index.md`, `/_index.md` for the home page).
    #[must_use]
    pub fn source_path(&self) -> String {
        let index = if self.kind.is_branch() {
            "_index"
        } else {
            "index"
        };
        let dir = if self.path.is_empty() {
            String::new()
        } else {
            format!("/{}", self.path)
        };
        format!("{dir}/{index}.{}", self.suffix)
    }
}

/// The first suffix of `text/markdown` (`md`).
fn markdown_suffix(types: &MediaTypes) -> String {
    types
        .by_type("text/markdown")
        .map_or("md", |id| types.get(id).first_suffix())
        .to_owned()
}

/// `content.value` as text: strings as they are, numbers and booleans written out.
fn content_text(v: &Value) -> String {
    value::weak_string(v).unwrap_or_default()
}

/// The typed meta of an adapter page. `fields` is the page's map with the cascade's fields
/// filled in, `params` the cascade's params (the map's `params` go over them).
///
/// # Errors
/// A reserved field of the wrong shape, an absolute `url`, or an unknown output format.
pub fn meta_from_adapter(
    page: &AdapterPage,
    fields: &Params,
    mut params: Params,
    ctx: &MetaCtx<'_>,
) -> Result<PageMeta, PageError> {
    let string = |key: &str| {
        fields
            .get(key)
            .map(|v| value::weak_string(v).unwrap_or_default())
    };
    let url = string("url");
    if let Some(u) = &url
        && (u.starts_with("http://") || u.starts_with("https://"))
    {
        return Err(PageError::AbsoluteUrl(u.clone()));
    }
    let weight = fields.get("weight").map_or(Ok(0), |v| {
        value::weak_int(v)
            .and_then(|w| i32::try_from(w).ok())
            .ok_or_else(|| PageError::field("weight", "expected an integer"))
    })?;
    let outputs = fields
        .get("outputs")
        .map(|v| {
            value::string_list(v)
                .iter()
                .map(|n| {
                    ctx.formats
                        .by_name(n)
                        .ok_or_else(|| PageError::OutputFormat(n.clone()))
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    let build = fields
        .get("build")
        .map_or(Ok(BuildPolicy::default()), BuildPolicy::decode)?;
    let zero_sitemap = SitemapConfig {
        change_freq: String::new(),
        priority: 0.0,
        filename: String::new(),
        disable: false,
    };
    let sitemap = decode_sitemap(fields.get("sitemap"), &zero_sitemap)?;
    let cjk = match fields.get("iscjklanguage").and_then(value::weak_bool) {
        Some(true) => Cjk::Yes,
        Some(false) => Cjk::No,
        None => ctx.cjk_default,
    };
    match fields.get("params") {
        None | Some(Value::Null) => {}
        Some(Value::Map(m)) => {
            for (k, v) in Params::fold(m).iter() {
                params.insert(k, v);
            }
        }
        Some(other) => {
            return Err(PageError::field(
                "params",
                format!("expected a table, found {}", value::kind(other)),
            ));
        }
    }
    let given = match fields.get("dates") {
        None | Some(Value::Null) => Dates::default(),
        Some(Value::Map(m)) => adapter_dates_given(&Params::fold(m), ctx.time_zone)?,
        Some(other) => {
            return Err(PageError::field(
                "dates",
                format!("expected a table, found {}", value::kind(other)),
            ));
        }
    };

    Ok(PageMeta {
        title: string("title"),
        link_title: string("linktitle"),
        description: string("description").unwrap_or_default(),
        summary: string("summary"),
        // As given: Go trims the `-` only from front matter slugs (`setMetaPostParams`
        // returns before that for adapter pages).
        slug: string("slug"),
        url,
        r#type: string("type"),
        layout: string("layout"),
        weight,
        draft: fields
            .get("draft")
            .and_then(value::weak_bool)
            .unwrap_or(false),
        keywords: fields
            .get("keywords")
            .map(value::string_list)
            .unwrap_or_default(),
        aliases: fields
            .get("aliases")
            .map(value::string_list)
            .unwrap_or_default(),
        outputs,
        sitemap,
        build,
        translation_key: string("translationkey"),
        markup: page.markup,
        cjk,
        resources: Vec::new(),
        menus: decode_menus(fields.get("menus"))?,
        cascade: page.cascade.clone(),
        dates: ctx.dates.adapter_dates(&given),
        unparsable_dates: Vec::new(),
        params,
    })
}

/// The `dates` table of an `add_page` map: date values (`to_date`'s `{rfc3339, unix}`),
/// dates, date strings (in the language's time zone when they have none) or Unix seconds.
fn adapter_dates_given(m: &Params, tz: &TimeZone) -> Result<Dates, PageError> {
    let mut out = Dates::default();
    for field in DateField::ALL {
        let Some(v) = m.get(field.key()) else {
            continue;
        };
        let date = match v {
            Value::Null => None,
            Value::String(s) if s.is_empty() => None,
            Value::Map(dm) => match dm.get("rfc3339") {
                Some(Value::String(s)) => Some(to_date(&Value::String(s.clone()), tz)),
                _ => Some(None),
            },
            other => Some(to_date(other, tz)),
        };
        match date {
            None => {}
            Some(Some(z)) => out.set(field, z),
            Some(None) => {
                return Err(PageError::field(
                    &format!("dates.{}", field.key()),
                    format!("expected a date, found {}", value::kind(v)),
                ));
            }
        }
    }
    Ok(out)
}

impl DateResolver {
    /// The dates of an adapter page from the dates its map gives (Go's
    /// `createContentAdapterDatesHandler`). The given dates are kept; then, one date after the
    /// other in the order date, lastmod, publishDate, expiryDate, the first of its configured
    /// `[frontmatter]` sources that is one of the four date fields and set replaces it. The
    /// chains read the dates as the earlier chains left them (with the default configuration
    /// a page with only `lastmod` gets it as its `date`, and then as its `publishDate`); a
    /// date whose chain finds nothing keeps its given value. Aliases such as `pubdate`, custom
    /// keys and the `:` sources do not apply.
    #[must_use]
    pub fn adapter_dates(&self, given: &Dates) -> Dates {
        let mut out = given.clone();
        for (field, sources) in self.chains() {
            let found = sources.iter().find_map(|s| match s {
                DateSource::Field(key) => DateField::ALL
                    .into_iter()
                    .find(|f| f.key() == key)
                    .and_then(|f| out.get(f))
                    .cloned(),
                _ => None,
            });
            if let Some(z) = found {
                out.set(*field, z);
            }
        }
        out
    }
}
