//! Menus and cascades declared in the configuration.

use super::*;

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
pub(super) fn menu_html(v: &Value) -> Option<String> {
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
