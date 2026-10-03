//! The minifier options and their mapping from the Go implementation's `[minify.tdewolff]` table.
//!
//! The Go implementation configures its minifier (tdewolff/minify) per output type. This port
//! minifies with minify-html, lightningcss, oxc and its own JSON and XML minifiers, so only the
//! options with an equivalent are honoured. Keys are matched case-insensitively, as Go's config
//! loader does.
//!
//! | `[minify.tdewolff]` key | default | this port |
//! |---|---|---|
//! | `html.keepComments` | `false` | honoured: keep every comment |
//! | `html.keepSpecialComments` | `true` | honoured: keep SSI comments (`<!--#…-->`); minify-html does not keep conditional comments |
//! | `html.keepConditionalComments` | – | legacy alias of `keepSpecialComments` (used when that is not set) |
//! | `html.keepEndTags` | `true` | honoured: `false` lets minify-html omit optional closing tags |
//! | `html.keepDocumentTags` | `true` | honoured: `false` lets minify-html omit attribute-less `<html>`/`<head>` opening tags |
//! | `html.keepDefaultAttrVals` | `true` | honoured for `<input type=text>` (minify-html's only switch) |
//! | `html.templateDelims` | `["", ""]` | `["{{", "}}"]` preserves `{{ }}`, `{% %}` and `{# #}` spans; `["<%", "%>"]` preserves `<% %>` spans; other pairs are ignored |
//! | `html.keepQuotes`, `html.keepWhitespace` | `false` | ignored: minify-html always unquotes where the spec allows and collapses whitespace by context |
//! | `css.keepCSS2` | `true` | honoured: keeps comma `rgb()`/`rgba()` notation instead of hex-alpha and space-separated colours in stand-alone CSS |
//! | `css.precision` (`css.decimal`), `css.inline` | `0` | ignored: lightningcss prints numbers exactly; inline CSS is handled inside HTML |
//! | `js.keepVarNames` | `false` | honoured: `true` disables oxc's name mangling |
//! | `js.precision`, `js.version` | `0` | ignored: oxc keeps number values exactly and prints the input's syntax level |
//! | `json.precision`, `json.keepNumbers` | `0`, `false` | ignored: numbers are always copied verbatim |
//! | `svg.keepComments` | `false` | honoured (stand-alone SVG) |
//! | `svg.precision` (`svg.decimal`), `svg.inline` | `0` | ignored: path data and numbers are not rewritten |
//! | `xml.keepWhitespace` | `false` | honoured |
//!
//! `disableCSS`, `disableHTML`, `disableJS`, `disableJSON`, `disableSVG` and `disableXML` are
//! decoded by `ssg-config` ([`ssg_config::MinifyConfig::disabled`]); a disabled type is
//! passed through untouched, and HTML then also leaves its inline `<style>`/`<script>` alone.

use ssg_base::{Map, Value};

use crate::MinifyError;

/// Which HTML comments survive.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HtmlComments {
    /// Remove every comment.
    Remove,
    /// Keep server-side-include comments (`<!--# … -->`).
    #[default]
    KeepSpecial,
    /// Keep every comment.
    KeepAll,
}

/// Template syntax that the HTML minifier passes through untouched.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TemplateSyntax {
    /// No template syntax.
    #[default]
    None,
    /// `{{ … }}`, `{% … %}` and `{# … #}`.
    Braces,
    /// `<% … %>`.
    ChevronPercent,
}

/// HTML options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HtmlOptions {
    pub comments: HtmlComments,
    /// Keep optional closing tags.
    pub keep_end_tags: bool,
    /// Keep attribute-less `<html>` and `<head>` opening tags.
    pub keep_document_tags: bool,
    /// Keep default attribute values (`<input type=text>`).
    pub keep_default_attr_vals: bool,
    pub templates: TemplateSyntax,
}

impl Default for HtmlOptions {
    fn default() -> Self {
        Self {
            comments: HtmlComments::KeepSpecial,
            keep_end_tags: true,
            keep_document_tags: true,
            keep_default_attr_vals: true,
            templates: TemplateSyntax::None,
        }
    }
}

/// CSS options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CssOptions {
    /// Avoid colour syntax newer than CSS 2/3 (hex alpha, space-separated `rgb()`).
    pub keep_css2: bool,
    /// The browsers to prefix and lower for (the project's browserslist configuration,
    /// [`crate::project_browsers`]); `None`: declarations are kept as written.
    pub browsers: Option<lightningcss::targets::Browsers>,
}

impl Default for CssOptions {
    fn default() -> Self {
        Self {
            keep_css2: true,
            browsers: None,
        }
    }
}

/// JavaScript options.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct JsOptions {
    /// Do not rename local variables.
    pub keep_var_names: bool,
}

/// Which comments an XML-family document keeps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum XmlComments {
    #[default]
    Remove,
    Keep,
}

/// Whitespace handling in XML-family text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum XmlWhitespace {
    /// Drop whitespace-only text, trim text at tags and collapse whitespace runs.
    #[default]
    Collapse,
    /// Keep text exactly.
    Keep,
}

/// SVG options (stand-alone SVG documents).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SvgOptions {
    pub comments: XmlComments,
}

/// XML options.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct XmlOptions {
    pub whitespace: XmlWhitespace,
}

/// All minifier options. [`Default`] is the Go implementation's default configuration.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub html: HtmlOptions,
    pub css: CssOptions,
    pub js: JsOptions,
    pub svg: SvgOptions,
    pub xml: XmlOptions,
}

/// Why a configured `[minify.tdewolff]` key has no effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IgnoreReason {
    /// A tdewolff option with no equivalent here's minifiers.
    NoEquivalent,
    /// A key tdewolff does not know either.
    Unknown,
}

/// A configured key that is not honoured (for a warning).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IgnoredOption {
    /// `section.key` as configured, e.g. `html.keepQuotes`.
    pub key: String,
    pub reason: IgnoreReason,
}

/// Options decoded from `[minify.tdewolff]`, with the keys that have no effect.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DecodedOptions {
    pub options: Options,
    pub ignored: Vec<IgnoredOption>,
}

impl Options {
    /// Decodes the `[minify.tdewolff]` table (see the module docs for the mapping).
    ///
    /// # Errors
    /// [`MinifyError::Option`] for a section that is not a table or a value of the wrong type.
    pub fn from_tdewolff(table: &Map) -> Result<DecodedOptions, MinifyError> {
        let mut d = Decoder::default();
        for (section, value) in table.iter() {
            let Value::Map(entries) = value else {
                return Err(option_error(section, "a table"));
            };
            match section.to_ascii_lowercase().as_str() {
                "html" => d.html(section, entries)?,
                "css" => d.css(section, entries)?,
                "js" => d.js(section, entries)?,
                "json" => d.json(section, entries),
                "svg" => d.svg(section, entries)?,
                "xml" => d.xml(section, entries)?,
                _ => d.ignore(section, IgnoreReason::Unknown),
            }
        }
        Ok(DecodedOptions {
            options: d.options,
            ignored: d.ignored,
        })
    }
}

#[derive(Default)]
struct Decoder {
    options: Options,
    ignored: Vec<IgnoredOption>,
}

fn option_error(key: &str, expected: &'static str) -> MinifyError {
    MinifyError::Option {
        key: key.to_owned(),
        expected,
    }
}

fn boolean(section: &str, key: &str, v: &Value) -> Result<bool, MinifyError> {
    match v {
        Value::Bool(b) => Ok(*b),
        _ => Err(option_error(&format!("{section}.{key}"), "a boolean")),
    }
}

impl Decoder {
    fn ignore(&mut self, key: &str, reason: IgnoreReason) {
        self.ignored.push(IgnoredOption {
            key: key.to_owned(),
            reason,
        });
    }

    fn ignore_in(&mut self, section: &str, key: &str, reason: IgnoreReason) {
        self.ignore(&format!("{section}.{key}"), reason);
    }

    fn html(&mut self, section: &str, entries: &Map) -> Result<(), MinifyError> {
        let (mut keep_comments, mut special, mut conditional) = (None, None, None);
        for (key, v) in entries.iter() {
            let o = &mut self.options.html;
            match key.to_ascii_lowercase().as_str() {
                "keepcomments" => keep_comments = Some(boolean(section, key, v)?),
                "keepspecialcomments" => special = Some(boolean(section, key, v)?),
                "keepconditionalcomments" => conditional = Some(boolean(section, key, v)?),
                "keependtags" => o.keep_end_tags = boolean(section, key, v)?,
                "keepdocumenttags" => o.keep_document_tags = boolean(section, key, v)?,
                "keepdefaultattrvals" => o.keep_default_attr_vals = boolean(section, key, v)?,
                "templatedelims" => match template_syntax(v) {
                    Some(t) => o.templates = t,
                    None if matches!(v, Value::Array(_)) => {
                        self.ignore_in(section, key, IgnoreReason::NoEquivalent);
                    }
                    None => {
                        return Err(option_error(
                            &format!("{section}.{key}"),
                            "a pair of strings",
                        ));
                    }
                },
                "keepquotes" | "keepwhitespace" => {
                    boolean(section, key, v)?;
                    self.ignore_in(section, key, IgnoreReason::NoEquivalent);
                }
                _ => self.ignore_in(section, key, IgnoreReason::Unknown),
            }
        }
        let special = special.or(conditional).unwrap_or(true);
        self.options.html.comments = match (keep_comments.unwrap_or(false), special) {
            (true, _) => HtmlComments::KeepAll,
            (false, true) => HtmlComments::KeepSpecial,
            (false, false) => HtmlComments::Remove,
        };
        Ok(())
    }

    fn css(&mut self, section: &str, entries: &Map) -> Result<(), MinifyError> {
        for (key, v) in entries.iter() {
            match key.to_ascii_lowercase().as_str() {
                "keepcss2" => self.options.css.keep_css2 = boolean(section, key, v)?,
                "precision" | "decimal" | "inline" => {
                    self.ignore_in(section, key, IgnoreReason::NoEquivalent);
                }
                _ => self.ignore_in(section, key, IgnoreReason::Unknown),
            }
        }
        Ok(())
    }

    fn js(&mut self, section: &str, entries: &Map) -> Result<(), MinifyError> {
        for (key, v) in entries.iter() {
            match key.to_ascii_lowercase().as_str() {
                "keepvarnames" => self.options.js.keep_var_names = boolean(section, key, v)?,
                "precision" | "version" | "usealphabetvarnames" => {
                    self.ignore_in(section, key, IgnoreReason::NoEquivalent);
                }
                _ => self.ignore_in(section, key, IgnoreReason::Unknown),
            }
        }
        Ok(())
    }

    fn json(&mut self, section: &str, entries: &Map) {
        for key in entries.keys() {
            let reason = match key.to_ascii_lowercase().as_str() {
                "precision" | "keepnumbers" => IgnoreReason::NoEquivalent,
                _ => IgnoreReason::Unknown,
            };
            self.ignore_in(section, key, reason);
        }
    }

    fn svg(&mut self, section: &str, entries: &Map) -> Result<(), MinifyError> {
        for (key, v) in entries.iter() {
            match key.to_ascii_lowercase().as_str() {
                "keepcomments" => {
                    self.options.svg.comments = if boolean(section, key, v)? {
                        XmlComments::Keep
                    } else {
                        XmlComments::Remove
                    };
                }
                "precision" | "decimal" | "inline" => {
                    self.ignore_in(section, key, IgnoreReason::NoEquivalent);
                }
                _ => self.ignore_in(section, key, IgnoreReason::Unknown),
            }
        }
        Ok(())
    }

    fn xml(&mut self, section: &str, entries: &Map) -> Result<(), MinifyError> {
        for (key, v) in entries.iter() {
            match key.to_ascii_lowercase().as_str() {
                "keepwhitespace" => {
                    self.options.xml.whitespace = if boolean(section, key, v)? {
                        XmlWhitespace::Keep
                    } else {
                        XmlWhitespace::Collapse
                    };
                }
                _ => self.ignore_in(section, key, IgnoreReason::Unknown),
            }
        }
        Ok(())
    }
}

/// `templateDelims = [open, close]`: `Some` for a pair minify-html can preserve (or the empty
/// pair, which means none).
fn template_syntax(v: &Value) -> Option<TemplateSyntax> {
    let Value::Array(items) = v else {
        return None;
    };
    match items.as_slice() {
        [] => Some(TemplateSyntax::None),
        [Value::String(open), Value::String(close)] => match (open.as_ref(), close.as_ref()) {
            ("", "") => Some(TemplateSyntax::None),
            ("{{", "}}") => Some(TemplateSyntax::Braces),
            ("<%", "%>") => Some(TemplateSyntax::ChevronPercent),
            _ => None,
        },
        _ => None,
    }
}
