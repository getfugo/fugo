//! The editor's fields: the settings of `[cms.fields.<key>]` (`<key>` may be a key inside tables,
//! `nutrition.fat`), the field of every key as the index gives it to the editor (what the build
//! works out from the content, `index::hints`, with the settings over it), and `fugo cms fields`,
//! which prints those fields as settings to change.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use ssg_config::Config;

use crate::CmsError;
use crate::config::CmsConfig;
use crate::index;

/// How the editor shows a front matter key (`[cms.fields.<key>]`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct Field {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub widget: Option<Widget>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    /// A select of any number of its options (the value is a list).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub multiple: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
    /// The key's value on a new page, and when it is added to a page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    /// The smallest and the largest number, and the step between numbers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<serde_json::Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<serde_json::Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<serde_json::Number>,
    /// The key needs a value: the editor does not save a page without one (a page of a section
    /// whose pages use the key, or that has it).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub required: bool,
    /// One value for every language: the editor writes it to each language's file of the page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shared: Option<bool>,
    /// A table, or each table of a list, shown closed with a summary of its values.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub collapsed: bool,
    /// The summary of a closed table: its values in place of `{key}` (`{name}: {share}%`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// How the editor shows a front matter value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Widget {
    Text,
    Textarea,
    Number,
    Boolean,
    Date,
    Select,
    List,
    Image,
    Hidden,
}

/// A key's field as the index gives it to the editor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Hint {
    #[serde(flatten)]
    pub field: Field,
    /// The kind of the key's values: `string`, `number`, `boolean`, `date`, `list`, `objects`,
    /// `map`, or `mixed` when pages give it values of different kinds.
    pub kind: &'static str,
    /// Short values that pages share, for the editor to suggest as you type.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub suggestions: Vec<String>,
    /// No page has the key yet, only the settings: the editor offers it to add to any page.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub unused: bool,
    /// The key as pages write it, when that is not in lower case (the index's keys are).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// The `[cms.fields]` settings, checked: a multiple select needs options to choose from, and
/// `min` is not above `max`.
pub(crate) fn check(fields: BTreeMap<String, Field>) -> Result<BTreeMap<String, Field>, CmsError> {
    for (key, f) in &fields {
        if f.multiple && f.options.is_empty() {
            return Err(CmsError::config(
                format!("cms.fields.{key}.multiple"),
                "a select of several options needs `options`",
            ));
        }
        if let (Some(min), Some(max)) = (f.min.as_ref(), f.max.as_ref())
            && min.as_f64() > max.as_f64()
        {
            return Err(CmsError::config(
                format!("cms.fields.{key}.min"),
                format!("{min} is above max ({max})"),
            ));
        }
    }
    Ok(fields)
}

const HEADER: &str = "\
# The editor's fields (fugo cms fields): every front matter key of the site's content, and every
# key inside its tables (\"nutrition.fat\"), as the build gives it to the editor: a label from the
# key, a widget from its values and its name, whether its value is the same in every language,
# with the settings of [cms.fields] over them. Copy the ones you want to change into the
# configuration (as [cms.fields.<key>] in a configuration file, as [fields.<key>] in the cms.toml
# of a configuration directory); a setting you leave out stays as the build works it out.
#
# widget: text, textarea, number, boolean, date, select (with options; multiple = true for a list
# of them), list, image (a file of the page or an upload) or hidden (not shown, kept). A table (a
# key holding keys) has none: the form shows the keys inside it. Also: default (on new pages),
# min, max and step (numbers), required, shared (one value for every language), collapsed and
# summary (\"{name}: {share}%\", for tables).
";

/// The fields of a site as `[cms.fields]` settings (TOML).
///
/// # Errors
/// Unreadable content directories.
pub fn toml(cfg: &Config, cms: &CmsConfig) -> std::io::Result<String> {
    let index = index::read(cfg, cms, &BTreeMap::new())?;
    let mut out = String::from(HEADER);
    for (key, hint) in &index.fields {
        let top = key.split('.').next().unwrap_or(key);
        let using = |s: &&index::Section| s.keys.iter().any(|k| k.key.eq_ignore_ascii_case(top));
        let sections: Vec<&str> = index
            .sections
            .iter()
            .filter(using)
            .map(section_name)
            .collect();
        let taxonomy = index
            .taxonomies
            .iter()
            .any(|t| t.plural.eq_ignore_ascii_case(key));
        let what = match hint.kind {
            _ if taxonomy => "a taxonomy (the editor suggests its terms)",
            "objects" => "a list of tables",
            "map" => "a table",
            "list" => "a list",
            "date" => "a date",
            "number" => "a number",
            "boolean" => "yes or no",
            "mixed" => "values of different kinds",
            _ => "text",
        };
        let place = if hint.unused {
            "; no page has it yet".to_owned()
        } else if sections.is_empty() {
            "; on section pages".to_owned()
        } else {
            format!("; in {}", sections.join(", "))
        };
        let _ = write!(out, "\n[cms.fields.{}]\n# {what}{place}\n", toml_key(key));
        match hint.suggestions.len() {
            0 => {}
            1 => out.push_str("# 1 value that pages share, suggested as you type\n"),
            n => {
                let _ = writeln!(out, "# {n} values that pages share, suggested as you type");
            }
        }
        for s in &index.sections {
            let default = s.keys.iter().find(|k| k.key.eq_ignore_ascii_case(key));
            if let Some(v) = default.and_then(|k| k.default.as_ref())
                && let Ok(v) = serde_json::to_value(v)
            {
                let _ = writeln!(
                    out,
                    "# new pages in {} start with {}",
                    section_name(s),
                    toml_value(&v)
                );
            }
        }
        settings(&mut out, &hint.field);
    }
    Ok(out)
}

fn section_name(s: &index::Section) -> &str {
    if s.key.is_empty() { "/" } else { &s.key }
}

/// The settings of a field, one per line.
fn settings(out: &mut String, field: &Field) {
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
    let flags = [
        ("multiple", field.multiple),
        ("required", field.required),
        ("collapsed", field.collapsed),
    ];
    for (name, on) in flags {
        if on {
            let _ = writeln!(out, "{name} = true");
        }
    }
    if let Some(shared) = field.shared {
        let _ = writeln!(out, "shared = {shared}");
    }
    for (name, n) in [
        ("min", &field.min),
        ("max", &field.max),
        ("step", &field.step),
    ] {
        if let Some(n) = n {
            let _ = writeln!(out, "{name} = {n}");
        }
    }
    if let Some(default) = &field.default {
        let _ = writeln!(out, "default = {}", toml_value(default));
    }
    if let Some(summary) = &field.summary {
        let _ = writeln!(out, "summary = {}", toml_string(summary));
    }
    if let Some(help) = &field.help {
        let _ = writeln!(out, "help = {}", toml_string(help));
    }
}

/// A TOML value: strings, numbers, booleans, arrays and inline tables.
fn toml_value(v: &serde_json::Value) -> String {
    use serde_json::Value as J;
    match v {
        J::Null => "\"\"".to_owned(),
        J::Bool(b) => b.to_string(),
        J::Number(n) => n.to_string(),
        J::String(s) => toml_string(s),
        J::Array(items) => format!(
            "[{}]",
            items.iter().map(toml_value).collect::<Vec<_>>().join(", ")
        ),
        J::Object(m) => format!(
            "{{ {} }}",
            m.iter()
                .map(|(k, v)| format!("{} = {}", toml_key(k), toml_value(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
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
