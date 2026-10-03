//! Typed front matter: the reserved keys of a page, decoded once from its folded params.

use jiff::tz::TimeZone;
use ssg_base::glob::{self, Glob, GlobOpts};
use ssg_base::paths::ContentKey;
use ssg_base::{FormatId, PageKind, Params, Value};
use ssg_config::sections::SitemapConfig;
use ssg_config::{MediaTypes, OutputFormats};

use crate::build::BuildPolicy;
use crate::cascade::Cascade;
use crate::dates::{DateResolver, Dates, FileCtx};
use crate::markup::{Markup, MarkupSource};
use crate::{PageError, value};

/// Front matter that decides where a page goes, applied before the page enters its content
/// tree (Go's front matter `kind`, `lang` and `path`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CaptureOverrides {
    pub kind: Option<PageKind>,
    /// The language key, lower case.
    pub lang: Option<String>,
    pub path: Option<ContentKey>,
}

/// Reads the capture overrides of a page's folded front matter.
///
/// # Errors
/// A `kind` that is not a content kind, or `kind`/`lang`/`path` values that are not strings.
pub fn capture_overrides(p: &Params) -> Result<CaptureOverrides, PageError> {
    let string = |key: &str| -> Result<Option<String>, PageError> {
        match p.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) if s.trim().is_empty() => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.trim().to_owned())),
            Some(other) => Err(PageError::field(
                key,
                format!("expected a string, found {}", value::kind(other)),
            )),
        }
    };
    let kind = string("kind")?
        .map(|k| {
            PageKind::parse(&k)
                .filter(|k| k.is_content())
                .ok_or(PageError::Kind(k))
        })
        .transpose()?;
    Ok(CaptureOverrides {
        kind,
        lang: string("lang")?.map(|l| l.to_lowercase()),
        path: string("path")?.map(|p| ContentKey::from_source(&p)),
    })
}

/// Whether a page's content is checked for CJK text (word counts and summaries count
/// characters instead of words).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cjk {
    Yes,
    #[default]
    No,
    /// `hasCJKLanguage`: detect from the content.
    Detect,
}

/// A front matter `resources` entry: metadata for the bundle resources `src` matches.
#[derive(Clone, Debug)]
pub struct ResourceMetaRule {
    /// The glob as written.
    pub src: String,
    pub matcher: Glob,
    /// The resource name; `:counter` is replaced by the match count.
    pub name: Option<String>,
    pub title: Option<String>,
    pub params: Params,
}

/// A front matter `menu`/`menus` entry: the page in a menu.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageMenuEntry {
    pub menu: String,
    /// Empty: the page's own identity.
    pub identifier: String,
    /// Empty: the page's link title.
    pub name: String,
    pub pre: String,
    pub post: String,
    pub parent: String,
    pub title: String,
    pub weight: i32,
    pub params: Params,
}

/// A page's front matter, typed. The reserved keys also stay in `params` with their
/// normalised values (templates read `.Params.title`).
#[derive(Clone, Debug)]
pub struct PageMeta {
    pub title: Option<String>,
    pub link_title: Option<String>,
    pub description: String,
    pub summary: Option<String>,
    /// Trimmed of `-`; `:filename` may supply one.
    pub slug: Option<String>,
    pub url: Option<String>,
    pub r#type: Option<String>,
    pub layout: Option<String>,
    pub weight: i32,
    pub draft: bool,
    pub keywords: Vec<String>,
    pub aliases: Vec<String>,
    pub outputs: Option<Vec<FormatId>>,
    pub sitemap: SitemapConfig,
    /// `build`, with `headless: true` folded in.
    pub build: BuildPolicy,
    pub translation_key: Option<String>,
    pub markup: Markup,
    pub cjk: Cjk,
    pub resources: Vec<ResourceMetaRule>,
    pub menus: Vec<PageMenuEntry>,
    /// The page's own `cascade` (branch pages only).
    pub cascade: Cascade,
    pub dates: Dates,
    /// Front matter date keys whose values are not dates (the caller warns).
    pub unparsable_dates: Vec<String>,
    pub params: Params,
}

/// What [`meta_from_params`] needs besides the front matter.
#[derive(Clone, Copy, Debug)]
pub struct MetaCtx<'a> {
    /// The page's kind (after capture overrides).
    pub kind: PageKind,
    pub formats: &'a OutputFormats,
    pub media_types: &'a MediaTypes,
    /// The site's sitemap defaults.
    pub sitemap: &'a SitemapConfig,
    /// `hasCJKLanguage`.
    pub cjk_default: Cjk,
    /// The content file's extension (empty for pages without a file).
    pub ext: &'a str,
    pub dates: &'a DateResolver,
    pub file: Option<FileCtx<'a>>,
    /// The language's time zone, for dates without one.
    pub time_zone: &'a TimeZone,
}

/// Decodes the reserved front matter keys of a page (after its cascade was applied), then
/// resolves its dates. The `params:` table of the front matter is merged over the top-level
/// keys last.
///
/// # Errors
/// A reserved key with a value of the wrong shape, an absolute `url`, an unknown output
/// format or markup, a bad `resources` glob, or a `cascade` on a regular page.
pub fn meta_from_params(mut params: Params, ctx: &MetaCtx<'_>) -> Result<PageMeta, PageError> {
    let user = params.remove("params");
    let string = |p: &Params, key: &str| {
        p.get(key)
            .map(|v| value::weak_string(v).unwrap_or_default())
    };

    let title = string(&params, "title");
    let link_title = string(&params, "linktitle");
    let description = string(&params, "description").unwrap_or_default();
    let summary = string(&params, "summary");
    let slug = string(&params, "slug").map(|s| s.trim_matches('-').to_owned());
    let url = string(&params, "url");
    if let Some(u) = &url
        && (u.starts_with("http://") || u.starts_with("https://"))
    {
        return Err(PageError::AbsoluteUrl(u.clone()));
    }
    let r#type = string(&params, "type");
    let layout = string(&params, "layout");
    let weight = params.get("weight").map_or(Ok(0), |v| {
        value::weak_int(v)
            .and_then(|w| i32::try_from(w).ok())
            .ok_or_else(|| PageError::field("weight", "expected an integer"))
    })?;
    // `draft`, else Go's undocumented `published: <bool>` as its opposite (a `published`
    // date is not a bool and only a date source of `publishDate`).
    let draft = match params.get("draft") {
        Some(v) => value::weak_bool(v).unwrap_or(false),
        None => params
            .get("published")
            .and_then(value::weak_bool)
            .is_some_and(|published| !published),
    };
    let keywords = params
        .get("keywords")
        .map(value::string_list)
        .unwrap_or_default();
    let aliases = params
        .get("aliases")
        .map(value::string_list)
        .unwrap_or_default();
    let translation_key = string(&params, "translationkey");
    let cjk = match params.get("iscjklanguage").and_then(value::weak_bool) {
        Some(true) => Cjk::Yes,
        Some(false) => Cjk::No,
        None => ctx.cjk_default,
    };

    let outputs = params
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

    // The legacy `_build` wins over `build`, as in Go; params keep the key as written.
    let mut build = params
        .get("_build")
        .or_else(|| params.get("build"))
        .map_or(Ok(BuildPolicy::default()), BuildPolicy::decode)?;
    if params.get("headless").and_then(value::weak_bool) == Some(true) {
        build = build.headless();
    }

    let markup_name = string(&params, "markup");
    let media_type = string(&params, "mediatype");
    let markup = Markup::detect(
        MarkupSource {
            media_type: media_type.as_deref(),
            markup: markup_name.as_deref(),
            ext: ctx.ext,
        },
        ctx.media_types,
    )?;

    let sitemap = decode_sitemap(params.get("sitemap"), ctx.sitemap)?;
    let resources = params
        .get("resources")
        .map_or(Ok(Vec::new()), decode_resources)?;
    let menus = decode_menus(params.get("menus").or_else(|| params.get("menu")))?;
    let cascade = match params.get("cascade") {
        None | Some(Value::Null) => Cascade::default(),
        Some(v) if ctx.kind.is_branch() => Cascade::decode(v)?,
        Some(_) => {
            return Err(PageError::Cascade(format!(
                "only branch pages can cascade, not {} pages",
                ctx.kind
            )));
        }
    };

    // The normalised values of the reserved keys.
    let mut put = |key: &str, v: Option<Value>| {
        if let Some(v) = v {
            params.insert(key, &v);
        }
    };
    put("title", title.as_deref().map(Value::string));
    put("linktitle", link_title.as_deref().map(Value::string));
    put("summary", summary.as_deref().map(Value::string));
    put("slug", slug.as_deref().map(Value::string));
    put("url", url.as_deref().map(Value::string));
    put("type", r#type.as_deref().map(Value::string));
    put("layout", layout.as_deref().map(Value::string));
    put(
        "translationkey",
        translation_key.as_deref().map(Value::string),
    );
    put("markup", markup_name.as_deref().map(Value::string));
    if params.get("description").is_some() {
        params.insert("description", &Value::string(&description));
    }
    if params.get("weight").is_some() {
        params.insert("weight", &Value::Int(i64::from(weight)));
    }
    if params.get("keywords").is_some() {
        params.insert("keywords", &value::string_array(&keywords));
    }
    if params.get("aliases").is_some() {
        params.insert("aliases", &value::string_array(&aliases));
    }
    params.insert("draft", &Value::Bool(draft));
    params.insert("iscjklanguage", &Value::Bool(cjk == Cjk::Yes));

    let resolved = ctx
        .dates
        .resolve(&mut params, ctx.file.as_ref(), ctx.time_zone);
    let slug = slug.or(resolved.slug.filter(|s| !s.is_empty()));

    if let Some(Value::Map(m)) = &user {
        params.merge_deep(&Params::fold(m));
    }

    Ok(PageMeta {
        title,
        link_title,
        description,
        summary,
        slug,
        url,
        r#type,
        layout,
        weight,
        draft,
        keywords,
        aliases,
        outputs,
        sitemap,
        build,
        translation_key,
        markup,
        cjk,
        resources,
        menus,
        cascade,
        dates: resolved.dates,
        unparsable_dates: resolved.unparsable,
        params,
    })
}

pub(crate) fn decode_sitemap(
    v: Option<&Value>,
    site: &SitemapConfig,
) -> Result<SitemapConfig, PageError> {
    let mut out = site.clone();
    let Some(v) = v else {
        return Ok(out);
    };
    let Some(m) = v.as_map() else {
        return Err(PageError::field(
            "sitemap",
            format!("expected a table, found {}", value::kind(v)),
        ));
    };
    for (k, v) in Params::fold(m).iter() {
        match k {
            "changefreq" => out.change_freq = value::weak_string(v).unwrap_or_default(),
            "priority" => {
                out.priority = v
                    .as_f64()
                    .ok_or_else(|| PageError::field("sitemap.priority", "expected a number"))?;
            }
            "filename" => out.filename = value::weak_string(v).unwrap_or_default(),
            "disable" => out.disable = value::weak_bool(v).unwrap_or(false),
            _ => {}
        }
    }
    Ok(out)
}

fn decode_resources(v: &Value) -> Result<Vec<ResourceMetaRule>, PageError> {
    let Some(items) = v.as_array() else {
        return Err(PageError::field(
            "resources",
            format!("expected a list of tables, found {}", value::kind(v)),
        ));
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let Some(m) = item.as_map() else {
            return Err(PageError::field("resources", "expected a table"));
        };
        let m = Params::fold(m);
        let src = m
            .get("src")
            .and_then(value::weak_string)
            .ok_or_else(|| PageError::field("resources", "an entry has no src"))?;
        let matcher = glob::compile(&src, GlobOpts::default())
            .map_err(|e| PageError::field("resources.src", e.to_string()))?;
        out.push(ResourceMetaRule {
            src,
            matcher,
            name: m.get("name").and_then(value::weak_string),
            title: m.get("title").and_then(value::weak_string),
            params: m
                .get("params")
                .and_then(Value::as_map)
                .map(Params::fold)
                .unwrap_or_default(),
        });
    }
    Ok(out)
}

/// `menu: main` (or a number: `menu: 42`), `menus: [main, footer]`, or
/// `menus: {main: {weight: 10, parent: docs}}`.
pub(crate) fn decode_menus(v: Option<&Value>) -> Result<Vec<PageMenuEntry>, PageError> {
    let entry = |menu: &str| PageMenuEntry {
        menu: menu.to_owned(),
        ..PageMenuEntry::default()
    };
    Ok(match v {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::String(s)) => vec![entry(s)],
        // A number or boolean names a menu too (`menu: 42` is the menu "42").
        Some(v @ (Value::Int(_) | Value::Float(_) | Value::Bool(_))) => {
            vec![entry(&value::weak_string(v).unwrap_or_default())]
        }
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(value::weak_string)
            .map(|m| entry(&m))
            .collect(),
        Some(Value::Map(m)) => {
            let mut out = Vec::with_capacity(m.len());
            for (menu, props) in m.iter() {
                let mut e = entry(menu);
                match props {
                    Value::Null => {}
                    Value::Map(pm) => {
                        let pm = Params::fold(pm);
                        // A boolean reads as `1`/`0` in these text fields, as in Go.
                        let s = |k: &str| match pm.get(k) {
                            Some(Value::Bool(b)) => if *b { "1" } else { "0" }.to_owned(),
                            v => v.and_then(value::weak_string).unwrap_or_default(),
                        };
                        e.identifier = s("identifier");
                        e.name = s("name");
                        e.pre = s("pre");
                        e.post = s("post");
                        e.parent = s("parent");
                        e.title = s("title");
                        e.weight = pm
                            .get("weight")
                            .and_then(value::weak_int)
                            .and_then(|w| i32::try_from(w).ok())
                            .unwrap_or(0);
                        e.params = pm
                            .get("params")
                            .and_then(Value::as_map)
                            .map(Params::fold)
                            .unwrap_or_default();
                    }
                    other => {
                        return Err(PageError::field(
                            "menus",
                            format!(
                                "menu {menu:?}: expected a table, found {}",
                                value::kind(other)
                            ),
                        ));
                    }
                }
                out.push(e);
            }
            out
        }
        Some(other) => {
            return Err(PageError::field(
                "menus",
                format!(
                    "expected a menu name, list or table, found {}",
                    value::kind(other)
                ),
            ));
        }
    })
}
