//! Front matter `build` (and the legacy `headless`): whether a page is listed, rendered and
//! publishes its resources.

use ssg_base::Value;

use crate::PageError;
use crate::value;

/// Whether a page appears in page collections.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ListMode {
    /// In every collection.
    #[default]
    Always,
    /// In no collection (it can still be fetched with `get_page`).
    Never,
    /// Only in the collections of its own section (`.Pages`, `.RegularPages`), not in site-wide
    /// ones.
    Local,
}

/// Whether a page gets output files.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RenderMode {
    #[default]
    Always,
    Never,
    /// No output files, but `.Permalink` and `.RelPermalink` work.
    Link,
}

/// The build options of a page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BuildPolicy {
    pub list: ListMode,
    pub render: RenderMode,
    /// Whether the page's bundle resources are published even when nothing links to them.
    pub publish_resources: bool,
}

impl Default for BuildPolicy {
    fn default() -> Self {
        Self {
            list: ListMode::Always,
            render: RenderMode::Always,
            publish_resources: true,
        }
    }
}

impl BuildPolicy {
    /// The policy of `headless: true`: neither listed nor rendered.
    #[must_use]
    pub fn headless(self) -> Self {
        Self {
            list: ListMode::Never,
            render: RenderMode::Never,
            ..self
        }
    }

    /// Decodes a front matter `build` value over the defaults. Keys ignore case; `list` and
    /// `render` take their names (exact spelling), or a boolean (`true` = always,
    /// `false` = never); unknown names fall back to `always`.
    ///
    /// # Errors
    /// A value that is not a table, `list`/`render` that are not scalars, or a
    /// `publishResources` that is not a boolean.
    pub fn decode(v: &Value) -> Result<Self, PageError> {
        let mut out = Self::default();
        let m = match v {
            Value::Null => return Ok(out),
            Value::Map(m) => m,
            other => {
                return Err(PageError::field(
                    "build",
                    format!("expected a table, found {}", value::kind(other)),
                ));
            }
        };
        for (k, v) in m.iter() {
            match k.to_ascii_lowercase().as_str() {
                "list" => {
                    out.list = match mode_name(v, "build.list")? {
                        Mode::Never => ListMode::Never,
                        Mode::Named("local") => ListMode::Local,
                        _ => ListMode::Always,
                    };
                }
                "render" => {
                    out.render = match mode_name(v, "build.render")? {
                        Mode::Never => RenderMode::Never,
                        Mode::Named("link") => RenderMode::Link,
                        _ => RenderMode::Always,
                    };
                }
                "publishresources" => {
                    out.publish_resources = value::weak_bool(v).ok_or_else(|| {
                        PageError::field("build.publishResources", "expected a boolean")
                    })?;
                }
                _ => {}
            }
        }
        Ok(out)
    }
}

enum Mode<'a> {
    Always,
    Never,
    Named(&'a str),
}

fn mode_name<'a>(v: &'a Value, key: &str) -> Result<Mode<'a>, PageError> {
    Ok(match v {
        Value::Bool(true) => Mode::Always,
        Value::Bool(false) => Mode::Never,
        Value::Int(0) => Mode::Never,
        Value::Int(1) => Mode::Always,
        Value::String(s) => match &**s {
            "never" | "0" => Mode::Never,
            "always" | "1" => Mode::Always,
            other => Mode::Named(other),
        },
        Value::Null | Value::Int(_) | Value::Float(_) => Mode::Always,
        other => {
            return Err(PageError::field(
                key,
                format!("expected a name, found {}", value::kind(other)),
            ));
        }
    })
}
