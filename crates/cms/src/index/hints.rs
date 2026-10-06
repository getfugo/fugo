//! The field of every front matter key, as the build works it out from the content: a label from
//! the key, a widget from the kind of its values and from its name, and the short values pages
//! share, to suggest. The settings of `[cms.fields]` go over it, setting by setting.

use super::*;
use crate::fields::{Hint, Widget};

/// Longer values are not suggested: they are sentences, not names.
const SHORT: usize = 80;
/// The most values suggested for a key: those the most pages use.
const MAX_SUGGESTIONS: usize = 200;
/// Past this many different values a key's values are not counted any more, nor suggested.
const MAX_TRACKED: usize = 5000;
/// Keys whose values name one page (translations share theirs): nothing to suggest.
const OWN: [&str; 6] = [
    "title",
    "linktitle",
    "slug",
    "url",
    "aliases",
    "translationkey",
];

/// What the content says about its keys (lower-cased), gathered file by file.
#[derive(Default)]
pub(super) struct Keys(BTreeMap<String, KeyStats>);

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
}

impl Keys {
    /// Counts the front matter `fm` of a file of the page `entry`.
    pub(super) fn add(&mut self, entry: &str, fm: &Map) {
        for (k, v) in fm.iter() {
            let s = self.0.entry(k.to_lowercase()).or_default();
            if s.name.is_empty() {
                k.clone_into(&mut s.name);
            }
            if !matches!(v, Value::Null) && v.as_str() != Some("") {
                *s.kinds.entry(kind_of(v)).or_default() += 1;
            }
            match v {
                Value::String(t) => s.value(entry, t),
                Value::Array(items) => {
                    for item in items.iter() {
                        if let Value::String(t) = item {
                            s.value(entry, t);
                        }
                    }
                }
                _ => {}
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
        for (key, s) in self.0 {
            let taxonomy = taxonomies.contains(&key);
            let kind = s.kind();
            let field = with_settings(inferred(&s.name, kind, taxonomy), fields.get(&key), kind);
            // Fixed options, a taxonomy's terms and the values of one page leave nothing to suggest.
            let suggests = !taxonomy
                && field.options.is_empty()
                && !OWN.contains(&key.as_str())
                && matches!(kind, "string" | "list" | "mixed");
            let suggestions = if suggests { s.shared() } else { Vec::new() };
            let hint = Hint {
                field,
                kind,
                suggestions,
                unused: false,
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
                label: Some(label_of(&key)),
                ..Field::default()
            };
            let hint = Hint {
                field: with_settings(named, Some(set), kind),
                kind,
                suggestions: Vec::new(),
                unused: true,
            };
            out.insert(key, hint);
        }
        out
    }
}

impl KeyStats {
    fn value(&mut self, entry: &str, text: &str) {
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
        help: set.help.clone().or(inferred.help),
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
