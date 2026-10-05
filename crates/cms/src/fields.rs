//! `fugo cms fields`: a starting point for `[cms.fields]`, from the front matter of the site's
//! content. One table per key the content uses: the settings the configuration has for it, else
//! a label and the widget its values suggest.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use ssg_config::Config;

use crate::config::{CmsConfig, Field, Widget};
use crate::index;

const HEADER: &str = "\
# The editor's fields (fugo cms fields): every front matter key of the site's content, with the
# settings [cms.fields] has for it, else a label and the widget its values suggest. Change them,
# and keep the ones you change in the configuration: as [cms.fields.<key>] in a configuration
# file, as [fields.<key>] in the cms.toml of a configuration directory.
#
# widget: text, textarea, number, boolean, date, select (with options), list, image (a file of
# the page or an upload) or hidden (not shown, kept). A table (a key holding keys) has none: the
# form shows the keys inside it.
";

/// The `[cms.fields]` tables of a site, as TOML.
///
/// # Errors
/// Unreadable content directories.
pub fn toml(cfg: &Config, cms: &CmsConfig) -> std::io::Result<String> {
    let index = index::read(cfg, cms, &BTreeMap::new())?;
    let taxonomies: BTreeSet<String> = index
        .taxonomies
        .iter()
        .map(|t| t.plural.to_lowercase())
        .collect();
    // Lower-cased key → the key as first written, the kind of its values, the sections using it.
    let mut keys: BTreeMap<String, (String, &str, Vec<&str>)> = BTreeMap::new();
    for s in &index.sections {
        for k in &s.keys {
            let entry = keys
                .entry(k.key.to_lowercase())
                .or_insert_with(|| (k.key.clone(), k.kind, Vec::new()));
            let section = if s.key.is_empty() {
                "/"
            } else {
                s.key.as_str()
            };
            if !entry.2.contains(&section) {
                entry.2.push(section);
            }
        }
    }
    let mut out = String::from(HEADER);
    for (lower, (key, kind, sections)) in &keys {
        let taxonomy = taxonomies.contains(lower);
        let field = cms
            .fields
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(lower))
            .map_or_else(|| inferred(key, kind, taxonomy), |(_, f)| f.clone());
        let what = match *kind {
            _ if taxonomy => "a taxonomy (the editor suggests its terms)",
            "objects" => "a list of tables",
            "map" => "a table",
            "list" => "a list",
            "date" => "a date",
            "number" => "a number",
            "boolean" => "yes or no",
            _ => "text",
        };
        let _ = write!(
            out,
            "\n[cms.fields.{}]\n# {what}; in {}\n",
            toml_key(lower),
            sections.join(", ")
        );
        if let Some(label) = &field.label {
            let _ = writeln!(out, "label = {}", toml_string(label));
        }
        if let Some(widget) = field.widget {
            let _ = writeln!(out, "widget = \"{}\"", widget_name(widget));
        }
        if !field.options.is_empty() {
            let options: Vec<String> = field.options.iter().map(|o| toml_string(o)).collect();
            let _ = writeln!(out, "options = [{}]", options.join(", "));
        }
        if let Some(help) = &field.help {
            let _ = writeln!(out, "help = {}", toml_string(help));
        }
    }
    Ok(out)
}

/// The settings a key's name and values suggest.
fn inferred(key: &str, kind: &str, taxonomy: bool) -> Field {
    let lower = key.to_lowercase();
    let named = |words: &[&str]| words.iter().any(|w| lower.contains(w));
    let widget = match kind {
        // The editor suggests a taxonomy's terms, for one value or a list of them.
        _ if taxonomy => None,
        "boolean" => Some(Widget::Boolean),
        "number" => Some(Widget::Number),
        "date" => Some(Widget::Date),
        "list" => Some(Widget::List),
        "objects" | "map" => None,
        _ if named(&["image", "cover", "thumbnail", "photo", "picture"]) => Some(Widget::Image),
        _ if named(&["description", "summary", "excerpt", "abstract"]) => Some(Widget::Textarea),
        _ => Some(Widget::Text),
    };
    Field {
        label: Some(label_of(key)),
        widget,
        options: Vec::new(),
        help: None,
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

fn widget_name(w: Widget) -> &'static str {
    match w {
        Widget::Text => "text",
        Widget::Textarea => "textarea",
        Widget::Number => "number",
        Widget::Boolean => "boolean",
        Widget::Date => "date",
        Widget::Select => "select",
        Widget::List => "list",
        Widget::Image => "image",
        Widget::Hidden => "hidden",
    }
}

/// A TOML key: bare when it may be, else quoted.
fn toml_key(k: &str) -> String {
    if !k.is_empty()
        && k.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        k.to_owned()
    } else {
        toml_string(k)
    }
}

/// A TOML basic string.
fn toml_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04X}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
