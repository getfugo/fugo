//! The keywords of an index: what a page has in it (its value under the index's name) and what
//! a named keyword list searches for.

use jiff::Zoned;
use ssg_base::{Value, text};
use ssg_config::sections::{RelatedIndex as IndexConfig, RelatedIndexKind};

use crate::NavError;
use crate::model::PageFacts;

/// A keyword of an index; heading ids (fragments) never match plain keywords.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) enum Keyword {
    Text(String),
    Fragment(String),
}

/// The page value an index reads, by the index's name.
enum MetaValue<'a> {
    None,
    Text(&'a str),
    List(&'a [String]),
    Date(Option<&'a Zoned>),
    Value(&'a Value),
    /// A value that cannot be a keyword (a boolean or a number field).
    Unsupported(&'static str),
}

fn meta_value<'a>(p: &PageFacts<'a>, name: &str) -> MetaValue<'a> {
    match name {
        "keywords" => MetaValue::List(p.keywords),
        "aliases" => MetaValue::List(p.aliases),
        "kind" => MetaValue::Text(p.kind.as_str()),
        "bundletype" => MetaValue::Text(p.bundle_type),
        "section" => MetaValue::Text(p.section),
        "lang" => MetaValue::Text(p.lang_key),
        "name" => MetaValue::Text(p.name),
        "description" => MetaValue::Text(p.description),
        "title" => MetaValue::Text(p.title),
        "linktitle" => MetaValue::Text(p.link_title),
        "slug" => MetaValue::Text(p.slug),
        "type" => MetaValue::Text(p.page_type),
        "layout" => MetaValue::Text(p.layout),
        "date" => MetaValue::Date(p.dates.date.as_ref()),
        "publishdate" => MetaValue::Date(p.dates.publish_date.as_ref()),
        "expirydate" => MetaValue::Date(p.dates.expiry_date.as_ref()),
        "lastmod" => MetaValue::Date(p.dates.lastmod.as_ref()),
        "draft" => MetaValue::Unsupported("bool"),
        "weight" => MetaValue::Unsupported("int"),
        "mediatype" => MetaValue::Unsupported("media type"),
        _ => p
            .params
            .get(name)
            .or_else(|| p.params.get_path(name))
            .map_or(MetaValue::None, MetaValue::Value),
    }
}

/// A date keyword: the date in the index's Go layout (`2006` by default); a page without the
/// date has Go's zero date (year 1).
fn date_keyword(d: Option<&Zoned>, pattern: &str) -> String {
    let layout = if pattern.is_empty() { "2006" } else { pattern };
    let zero;
    let d = if let Some(d) = d {
        d
    } else {
        zero = jiff::civil::date(1, 1, 1)
            .at(0, 0, 0, 0)
            .to_zoned(jiff::tz::TimeZone::UTC)
            .expect("year 1 is in range");
        &zero
    };
    ssg_page::format_go_layout(d, layout)
}

fn text_keyword(cfg: &IndexConfig, s: &str) -> Keyword {
    let s = if cfg.to_lower {
        text::to_lower(s)
    } else {
        s.to_owned()
    };
    match cfg.kind {
        RelatedIndexKind::Basic => Keyword::Text(s),
        RelatedIndexKind::Fragments => Keyword::Fragment(s),
    }
}

/// A scalar as a list element keyword (Go `cast.ToString`: nested lists and maps are empty).
fn scalar_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

/// The keywords value `v` gives in index `cfg`.
pub(super) fn value_keywords(cfg: &IndexConfig, v: &Value) -> Result<Vec<Keyword>, NavError> {
    let unsupported = |kind: &'static str| NavError::UnsupportedKeyword {
        index: cfg.name.clone(),
        kind,
    };
    Ok(match v {
        Value::Null => Vec::new(),
        Value::String(s) => vec![text_keyword(cfg, s)],
        Value::Array(items) => items
            .iter()
            .map(|i| text_keyword(cfg, &scalar_string(i)))
            .collect(),
        Value::Date(d) => {
            let z = match d {
                ssg_base::Date::Zoned(z) => z.clone(),
                ssg_base::Date::Local(dt) => dt
                    .to_zoned(jiff::tz::TimeZone::UTC)
                    .map_err(|_| unsupported("date"))?,
            };
            vec![Keyword::Text(date_keyword(Some(&z), &cfg.pattern))]
        }
        Value::Bool(_) => return Err(unsupported("bool")),
        Value::Int(_) => return Err(unsupported("int")),
        Value::Float(_) => return Err(unsupported("float")),
        Value::Map(_) => return Err(unsupported("map")),
    })
}

/// The keywords page `p` has in index `cfg`.
pub(super) fn page_keywords(
    cfg: &IndexConfig,
    p: &PageFacts<'_>,
) -> Result<Vec<Keyword>, NavError> {
    match meta_value(p, &cfg.name) {
        MetaValue::None => Ok(Vec::new()),
        MetaValue::Text(s) => Ok(vec![text_keyword(cfg, s)]),
        MetaValue::List(l) => Ok(l.iter().map(|s| text_keyword(cfg, s)).collect()),
        MetaValue::Date(d) => Ok(vec![Keyword::Text(date_keyword(d, &cfg.pattern))]),
        MetaValue::Value(v) => value_keywords(cfg, v),
        MetaValue::Unsupported(kind) => Err(NavError::UnsupportedKeyword {
            index: cfg.name.clone(),
            kind,
        }),
    }
}

/// Unix seconds and nanoseconds of a publish date (Go's zero date when absent).
pub(super) fn instant(d: Option<&Zoned>) -> (i64, i32) {
    const ZERO: i64 = -62_135_596_800;
    d.map_or((ZERO, 0), |z| {
        let t = z.timestamp();
        (t.as_second(), t.subsec_nanosecond())
    })
}
