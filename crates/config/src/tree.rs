//! Operations on untyped configuration trees: key normalisation, the legacy-key table and the
//! deep merge (steps 3 and 4 of the A1 pipeline).

use std::sync::Arc;

use ssg_base::{Map, Params, Value};

/// Lower-cases keys at every map level except inside arrays, renames `menu` to `menus`
/// (at the root and in each language) and drops the reserved `internal` table.
#[must_use]
pub fn normalize_keys(tree: &Map) -> Map {
    let mut m = Params::fold(tree).into_map();
    m.remove("internal");
    rename_menu(&mut m);
    if let Some(Value::Map(langs)) = m.get_mut("languages") {
        let langs = Arc::make_mut(langs);
        for (_, lang) in langs.iter_mut() {
            if let Value::Map(l) = lang {
                rename_menu(Arc::make_mut(l));
            }
        }
    }
    m
}

fn rename_menu(m: &mut Map) {
    if let Some(menu) = m.remove("menu") {
        match m.get_mut("menus") {
            Some(existing) => {
                let mut merged = menu;
                merge_value(&mut merged, existing);
                *existing = merged;
            }
            None => {
                m.insert("menus", menu);
            }
        }
    }
}

/// A legacy key that was rewritten to its current name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Migration {
    /// The legacy key path (dotted, as documented).
    pub from: String,
    /// The current key path.
    pub to: String,
}

/// Where each legacy key lives now, spelled as documented (matching ignores case). `*` in
/// `from` matches any remaining key, which is appended to `to`; an empty `to` means the key
/// is no longer supported.
pub const LEGACY_KEYS: &[(&str, &str)] = &[
    ("indexes", "taxonomies"),
    ("paginate", "pagination.pagerSize"),
    ("paginatePath", "pagination.path"),
    ("rssLimit", "services.rss.limit"),
    // The Go build's stats file (`[build] writeStats`, then `[build.buildStats]`), which
    // external CSS tools read; `purge_css` scans each page itself.
    ("build.writeStats", ""),
    ("build.buildStats", ""),
    ("ignoreErrors", "ignoreLogs"),
    (
        "footnoteReturnLinkContents",
        "markup.goldmark.extensions.footnote.backlinkHTML",
    ),
    ("disqusShortname", "services.disqus.shortname"),
    ("googleAnalytics", "services.googleAnalytics.id"),
    ("logI18nWarnings", "printI18nWarnings"),
    ("logPathWarnings", "printPathWarnings"),
    ("pygmentsCodeFences", "markup.highlight.codeFences"),
    (
        "pygmentsCodefencesGuessSyntax",
        "markup.highlight.guessSyntax",
    ),
    ("pygmentsStyle", "markup.highlight.style"),
    ("pygmentsUseClasses", "markup.highlight.noClasses"),
    ("pygmentsOptions", "markup.highlight.options"),
    ("pygmentsUseClassic", ""),
    ("privacy.twitter.*", "privacy.x.*"),
    ("services.twitter.*", "services.x.*"),
];

/// Rewrites legacy keys of one tree (the root or a language; keys lower case) to their
/// current location. A current key wins over a legacy one. Returns what was migrated, for
/// deprecation notices.
pub fn migrate_legacy_keys(m: &mut Map) -> Vec<Migration> {
    let mut done = Vec::new();
    for &(from, to) in LEGACY_KEYS {
        let (from_l, to_l) = (from.to_lowercase(), to.to_lowercase());
        if let Some(prefix) = from_l.strip_suffix(".*") {
            let to_prefix = to_l.strip_suffix(".*").unwrap_or(&to_l);
            let Some(Value::Map(src)) = get_path(m, prefix).cloned() else {
                continue;
            };
            for (k, v) in src.iter() {
                let target = format!("{to_prefix}.{k}");
                if get_path(m, &target).is_none() {
                    set_path(m, &target, v.clone());
                }
                done.push(Migration {
                    from: format!("{}.{k}", &from[..from.len() - 2]),
                    to: format!("{}.{k}", &to[..to.len() - 2]),
                });
            }
            continue;
        }
        let Some(mut v) = remove_path(m, &from_l) else {
            continue;
        };
        if to.is_empty() {
            done.push(Migration {
                from: from.to_owned(),
                to: String::new(),
            });
            continue;
        }
        if from_l == "pygmentsuseclasses" {
            // `pygmentsUseClasses = true` means CSS classes, i.e. `noClasses = false`.
            v = Value::Bool(!crate::de::weak_bool(&v).unwrap_or(false));
        }
        if get_path(m, &to_l).is_none() {
            set_path(m, &to_l, v);
        }
        done.push(Migration {
            from: from.to_owned(),
            to: to.to_owned(),
        });
    }
    // A boolean `minify` is the old spelling of `minify.minifyOutput`.
    if let Some(v) = m.get("minify")
        && !matches!(v, Value::Map(_))
    {
        let v = m.remove("minify").expect("present");
        set_path(
            m,
            "minify.minifyoutput",
            Value::Bool(crate::de::weak_bool(&v).unwrap_or(false)),
        );
        done.push(Migration {
            from: "minify".to_owned(),
            to: "minify.minifyOutput".to_owned(),
        });
    }
    done
}

/// Removes the value at a dotted path of lower-case keys.
fn remove_path(m: &mut Map, dotted: &str) -> Option<Value> {
    match dotted.split_once('.') {
        None => m.remove(dotted),
        Some((first, rest)) => match m.get_mut(first)? {
            Value::Map(child) => remove_path(Arc::make_mut(child), rest),
            _ => None,
        },
    }
}

/// The value at a dotted path of lower-case keys.
#[must_use]
pub fn get_path<'a>(m: &'a Map, dotted: &str) -> Option<&'a Value> {
    let mut segs = dotted.split('.');
    let mut cur = m.get(segs.next()?)?;
    for seg in segs {
        cur = cur.as_map()?.get(seg)?;
    }
    Some(cur)
}

/// Sets a dotted path, creating (or replacing non-map) intermediate tables.
pub fn set_path(m: &mut Map, dotted: &str, v: Value) {
    let segs: Vec<&str> = dotted.split('.').collect();
    set_segments(m, &segs, v);
}

pub(crate) fn set_segments(m: &mut Map, segs: &[&str], v: Value) {
    match segs {
        [] => {}
        [last] => {
            m.insert(*last, v);
        }
        [first, rest @ ..] => {
            let child = m.get_mut(first).filter(|c| matches!(c, Value::Map(_)));
            if let Some(Value::Map(child)) = child {
                set_segments(Arc::make_mut(child), rest, v);
            } else {
                let mut child = Map::new();
                set_segments(&mut child, rest, v);
                m.insert(*first, Value::map(child));
            }
        }
    }
}

/// Merges `over` into `base`: tables merge key by key, anything else in `over` replaces the
/// value in `base`. A table in `over` with `_merge = "none"` replaces instead of merging.
/// `_merge` keys are kept, `over`'s winning (the theme merge reads them, see [`crate::merge`]);
/// [`strip_merge`] removes them.
pub fn merge_deep(base: &mut Map, over: &Map) {
    for (k, v) in over.iter() {
        match base.get_mut(k) {
            Some(existing) => merge_value(existing, v),
            None => {
                base.insert(k, v.clone());
            }
        }
    }
}

fn merge_value(existing: &mut Value, over: &Value) {
    match (existing, over) {
        (Value::Map(a), Value::Map(b)) if !replaces(b) => merge_deep(Arc::make_mut(a), b),
        (slot, v) => *slot = v.clone(),
    }
}

fn replaces(m: &Map) -> bool {
    m.get("_merge")
        .and_then(Value::as_str)
        .is_some_and(|s| s.eq_ignore_ascii_case("none"))
}

/// `v` without `_merge` keys.
#[must_use]
pub fn strip_merge(v: &Value) -> Value {
    match v {
        Value::Map(m)
            if m.iter()
                .any(|(k, v)| k == "_merge" || matches!(v, Value::Map(_))) =>
        {
            Value::map(
                m.iter()
                    .filter(|(k, _)| *k != "_merge")
                    .map(|(k, v)| (k, strip_merge(v)))
                    .collect(),
            )
        }
        other => other.clone(),
    }
}

/// Splits a string list written as one string (`"taxonomy, term"`, `"fr de"`) on commas and
/// white space; a list is returned unchanged.
#[must_use]
pub fn split_list(v: &Value) -> Value {
    match v {
        Value::String(s) => Value::array(
            s.split(|c: char| c == ',' || c.is_whitespace())
                .filter(|p| !p.is_empty())
                .map(Value::string)
                .collect(),
        ),
        other => other.clone(),
    }
}
