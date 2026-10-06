//! The field of every front matter key, and of every key inside its tables (`nutrition.fat`,
//! below `nutrition`; the keys of a list's tables are below the list's key), as the build works
//! it out from the content: a label from the key, a widget from the kind of its values and from
//! its name, the short values pages share (to suggest), and whether a page's languages all give
//! it the same value (to edit once for all of them). The settings of `[cms.fields]` go over it,
//! setting by setting.

use std::hash::{DefaultHasher, Hash, Hasher};

use super::*;
use crate::fields::{Hint, Widget};

/// Longer values are not suggested: they are sentences, not names.
const SHORT: usize = 80;
/// The most values suggested for a key: those the most pages use.
const MAX_SUGGESTIONS: usize = 200;
/// Past this many different values a key's values are not counted any more, nor suggested.
const MAX_TRACKED: usize = 5000;
/// How deep into tables keys are followed, and how many keys inside tables are counted at most.
const MAX_DEPTH: usize = 6;
const MAX_NESTED: usize = 2000;
/// Keys whose values name one page (translations share theirs): nothing to suggest, and no
/// value for new pages.
pub(super) const OWN: [&str; 6] = [
    "title",
    "linktitle",
    "slug",
    "url",
    "aliases",
    "translationkey",
];
/// Keys whose value each language keeps its own of, even where they agree.
const PER_LANGUAGE: [&str; 4] = ["description", "summary", "keywords", "draft"];

/// What the content says about its keys, gathered file by file: by path, the key lower-cased
/// after the keys of the tables it is in.
#[derive(Default)]
pub(super) struct Keys {
    stats: BTreeMap<String, KeyStats>,
    /// Keys inside tables counted so far.
    nested: usize,
}

#[derive(Default)]
struct KeyStats {
    /// The key as first written.
    name: String,
    /// The kinds of its values, with how many there are; empty values have none.
    kinds: BTreeMap<&'static str, usize>,
    /// Its short text values (and the items of its lists), each with the pages that give it.
    values: BTreeMap<String, BTreeSet<String>>,
    /// More different values than are counted.
    many: bool,
    /// For a key of the front matter (not inside a table), per page: how the page's language
    /// files agree on its value.
    pages: BTreeMap<String, Agreement>,
}

/// The values the language files of one page give a key.
struct Agreement {
    /// The hash of the first file's value.
    hash: u64,
    files: usize,
    differ: bool,
}

impl Keys {
    /// Counts the front matter `fm` of a file of the page `entry`.
    pub(super) fn add(&mut self, entry: &str, fm: &Map) {
        for (k, v) in fm.iter() {
            let path = k.to_lowercase();
            self.value(entry, k, &path, v, 0);
            if let Some(s) = self.stats.get_mut(&path) {
                s.agree(entry, v);
            }
        }
    }

    /// Counts a value of the key `name` at `path`, and the keys of its tables below it.
    fn value(&mut self, entry: &str, name: &str, path: &str, v: &Value, depth: usize) {
        if depth > 0 && !self.stats.contains_key(path) {
            if self.nested == MAX_NESTED {
                return;
            }
            self.nested += 1;
        }
        let s = self.stats.entry(path.to_owned()).or_default();
        if s.name.is_empty() {
            name.clone_into(&mut s.name);
        }
        if !matches!(v, Value::Null) && v.as_str() != Some("") {
            *s.kinds.entry(kind_of(v)).or_default() += 1;
        }
        match v {
            Value::String(t) => s.text(entry, t),
            Value::Array(items) => {
                for item in items.iter() {
                    if let Value::String(t) = item {
                        s.text(entry, t);
                    }
                }
            }
            _ => {}
        }
        if depth == MAX_DEPTH {
            return;
        }
        let tables: Vec<&Map> = match v {
            Value::Array(items) => items.iter().filter_map(Value::as_map).collect(),
            _ => v.as_map().into_iter().collect(),
        };
        for table in tables {
            for (k, child) in table.iter() {
                let below = format!("{path}.{}", k.to_lowercase());
                self.value(entry, k, &below, child, depth + 1);
            }
        }
    }

    /// The fields of the keys, with the settings `fields` over them, and the fields of the keys
    /// only the settings name; `taxonomies` are the taxonomies' plurals, lower-cased.
    pub(super) fn hints(
        self,
        fields: &BTreeMap<String, Field>,
        taxonomies: &BTreeSet<String>,
    ) -> BTreeMap<String, Hint> {
        let mut out: BTreeMap<String, Hint> = BTreeMap::new();
        for (key, s) in self.stats {
            let top = !key.contains('.');
            let taxonomy = top && taxonomies.contains(&key);
            let kind = s.kind();
            let mut field = inferred(&s.name, kind, taxonomy);
            let own = OWN.contains(&key.as_str()) || PER_LANGUAGE.contains(&key.as_str());
            if top && !own && s.same_in_every_language() {
                field.shared = Some(true);
            }
            let field = with_settings(field, fields.get(&key), kind);
            // Fixed options, a taxonomy's terms and the values of one page leave nothing to suggest.
            let suggests = !taxonomy
                && field.options.is_empty()
                && !OWN.contains(&key.as_str())
                && matches!(kind, "string" | "list" | "mixed");
            let suggestions = if suggests { s.shared() } else { Vec::new() };
            let name = (s.name != s.name.to_lowercase()).then_some(s.name);
            let hint = Hint {
                field,
                kind,
                suggestions,
                unused: false,
                name,
            };
            out.insert(key, hint);
        }
        for (key, set) in fields {
            let key = key.to_lowercase();
            if out.contains_key(&key) {
                continue;
            }
            let kind = kind_by_settings(set);
            let named = Field {
                label: Some(label_of(key.rsplit('.').next().unwrap_or(&key))),
                ..Field::default()
            };
            let hint = Hint {
                field: with_settings(named, Some(set), kind),
                kind,
                suggestions: Vec::new(),
                unused: true,
                name: None,
            };
            out.insert(key, hint);
        }
        out
    }
}

impl KeyStats {
    fn text(&mut self, entry: &str, text: &str) {
        let t = text.trim();
        if self.many || t.is_empty() || t.contains('\n') || t.chars().count() > SHORT {
            return;
        }
        if self.values.len() == MAX_TRACKED && !self.values.contains_key(t) {
            self.many = true;
            self.values.clear();
            return;
        }
        self.values
            .entry(t.to_owned())
            .or_default()
            .insert(entry.to_owned());
    }

    /// Notes the value `v` one of the page's language files gives the key (an empty one says
    /// nothing).
    fn agree(&mut self, entry: &str, v: &Value) {
        let Some(text) = canonical(v) else {
            return;
        };
        let mut h = DefaultHasher::new();
        text.hash(&mut h);
        let hash = h.finish();
        let a = self.pages.entry(entry.to_owned()).or_insert(Agreement {
            hash,
            files: 0,
            differ: false,
        });
        a.files += 1;
        a.differ |= a.hash != hash;
    }

    /// Whether the key's value is one for every language: two pages or more give it in several
    /// languages, and nine in ten of them, at least, give it the same value in all.
    fn same_in_every_language(&self) -> bool {
        let several: Vec<&Agreement> = self.pages.values().filter(|a| a.files > 1).collect();
        let agree = several.iter().filter(|a| !a.differ).count();
        several.len() >= 2 && agree * 10 >= several.len() * 9
    }

    /// The kind of the key's values: `mixed` when they have more than one, `string` without any.
    fn kind(&self) -> &'static str {
        let mut kinds = self.kinds.keys();
        match (kinds.next(), kinds.next()) {
            (Some(kind), None) => kind,
            (Some(_), Some(_)) => "mixed",
            (None, _) => "string",
        }
    }

    /// The values pages share (each, on the average, on two pages or more), at most
    /// [`MAX_SUGGESTIONS`] of them (those the most pages use), in alphabetical order.
    fn shared(&self) -> Vec<String> {
        let uses: usize = self.values.values().map(BTreeSet::len).sum();
        if self.many || self.values.is_empty() || self.values.len() * 2 > uses {
            return Vec::new();
        }
        let mut by_use: Vec<(&String, usize)> = self
            .values
            .iter()
            .map(|(v, pages)| (v, pages.len()))
            .collect();
        by_use.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        let mut out: Vec<String> = by_use
            .into_iter()
            .take(MAX_SUGGESTIONS)
            .map(|(v, _)| v.clone())
            .collect();
        out.sort();
        out
    }
}

/// The text of a value with the keys of its tables in order and its empty values left out
/// (`None` for an empty value), so that files that write one value differently compare equal.
fn canonical(v: &Value) -> Option<String> {
    match v {
        Value::Null => None,
        Value::String(s) if s.is_empty() => None,
        Value::Array(items) => {
            let inner: Vec<String> = items.iter().filter_map(canonical).collect();
            (!inner.is_empty()).then(|| format!("[{}]", inner.join(",")))
        }
        _ => match v.as_map() {
            Some(m) => {
                let mut inner: Vec<String> = m
                    .iter()
                    .filter_map(|(k, v)| Some(format!("{:?}:{}", k.to_lowercase(), canonical(v)?)))
                    .collect();
                inner.sort();
                (!inner.is_empty()).then(|| format!("{{{}}}", inner.join(",")))
            }
            None => serde_json::to_string(v).ok(),
        },
    }
}

/// The field a key's name and the kind of its values suggest.
fn inferred(key: &str, kind: &str, taxonomy: bool) -> Field {
    let label = label_of(key);
    let lower = label.to_lowercase();
    let words: Vec<&str> = lower.split(' ').collect();
    let any = |names: &[&str]| words.iter().any(|w| names.contains(w));
    let widget = match kind {
        // The editor suggests a taxonomy's terms, for one value or a list of them.
        _ if taxonomy => None,
        "boolean" => Some(Widget::Boolean),
        "number" => Some(Widget::Number),
        "date" => Some(Widget::Date),
        "list" => Some(Widget::List),
        // A table shows the keys inside it; values of different kinds each show as they are.
        "objects" | "map" | "mixed" => None,
        _ if any(&[
            "image",
            "img",
            "cover",
            "thumbnail",
            "photo",
            "picture",
            "banner",
            "logo",
        ]) && !any(&[
            "alt", "caption", "title", "text", "credit", "width", "height",
        ]) =>
        {
            Some(Widget::Image)
        }
        _ if any(&["description", "summary", "excerpt", "abstract", "bio"]) => {
            Some(Widget::Textarea)
        }
        _ => Some(Widget::Text),
    };
    Field {
        label: Some(label),
        widget,
        ..Field::default()
    }
}

/// The settings `set` over the field `inferred`, setting by setting. Options without a widget
/// make a select, and a select of a key whose values are lists takes any number of options.
fn with_settings(inferred: Field, set: Option<&Field>, kind: &str) -> Field {
    let Some(set) = set else {
        return inferred;
    };
    let widget = set
        .widget
        .or_else(|| (!set.options.is_empty()).then_some(Widget::Select))
        .or(inferred.widget);
    Field {
        label: set.label.clone().or(inferred.label),
        multiple: set.multiple || (widget == Some(Widget::Select) && kind == "list"),
        widget,
        options: set.options.clone(),
        shared: set.shared.or(inferred.shared),
        ..set.clone()
    }
}

/// The kind of value a key that no page has yet takes, by its settings.
fn kind_by_settings(f: &Field) -> &'static str {
    match f.widget {
        Some(Widget::Number) => "number",
        Some(Widget::Boolean) => "boolean",
        Some(Widget::Date) => "date",
        Some(Widget::List) => "list",
        _ if f.multiple => "list",
        _ => "string",
    }
}

/// `image_preview` → `Image preview`, `whenSeen` → `When seen`.
fn label_of(key: &str) -> String {
    let mut words = String::new();
    let mut prev_lower = false;
    for c in key.chars() {
        if c == '_' || c == '-' || c == ' ' {
            words.push(' ');
            prev_lower = false;
        } else if c.is_uppercase() && prev_lower {
            words.push(' ');
            words.extend(c.to_lowercase());
            prev_lower = false;
        } else {
            words.push(c);
            prev_lower = c.is_lowercase();
        }
    }
    let words = words.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = words.chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}
