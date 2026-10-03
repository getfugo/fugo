//! Highlighting options: the site's `[markup.highlight]` defaults, overridden per call by a
//! fence's `{…}` options or by the `highlight` function's option string or map.

use ssg_base::{Map, Value};
use ssg_config::markup::HighlightConfig;

/// How token colours reach the page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Styling {
    /// Chroma class names (`class="k"`); the page needs a stylesheet ([`crate::Highlight::css`]).
    Classes,
    /// Inline `style` attributes (`noClasses = true`, Go's default).
    #[default]
    Inline,
}

/// Where line numbers go when they are on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineNumberLayout {
    /// A two-column table, so that copying the code leaves the numbers out (Go's default).
    #[default]
    Table,
    /// A span at the start of each line.
    Inline,
}

/// Whether the code is a block or inline code.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CodeLayout {
    /// `<div class="highlight"><pre><code>…`.
    #[default]
    Block,
    /// `<code class="code-inline language-x">…</code>` with no line structure (`hl_inline`).
    Inline,
}

/// The options of one highlighting call.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    /// The Chroma style name.
    pub style: String,
    pub styling: Styling,
    /// Whether lines are numbered (`linenos`).
    pub line_nos: bool,
    pub line_number_layout: LineNumberLayout,
    /// Whether line numbers are links to themselves (`anchorlinenos`).
    pub anchor_line_nos: bool,
    /// The prefix of line number ids (`lineanchors`); `-` is appended when not empty.
    pub line_anchors: String,
    /// The number of the first line (`linenostart`).
    pub line_no_start: i64,
    /// Lines to highlight as written: `"3 6-8"`, relative to line 1 of the code
    /// (`hl_lines`).
    pub hl_lines: String,
    /// Lines to highlight as absolute, inclusive line numbers; set by a fence's parsed
    /// `hl_lines`, and taking precedence over [`Options::hl_lines`].
    pub hl_ranges: Option<Vec<[i64; 2]>>,
    pub layout: CodeLayout,
    /// The CSS `tab-size` (0 or 8 write none).
    pub tab_width: i64,
    /// Whether an unknown or empty language is guessed from the code.
    pub guess_syntax: bool,
    /// The class of the wrapping `<div>`.
    pub wrapper_class: String,
}

impl Default for Options {
    fn default() -> Self {
        Self::from_config(&HighlightConfig::default())
    }
}

/// Invalid highlighting options.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum OptionsError {
    /// An option string item without exactly one `=`.
    #[error("invalid highlight option {0:?} (expected key=value)")]
    Syntax(String),
    /// A value that cannot be converted to the option's type.
    #[error("highlight option {key}: cannot use {value} as {expected}")]
    Value {
        key: String,
        value: String,
        expected: &'static str,
    },
}

/// A value of the option map, with the key it came from.
struct Opt<'a> {
    key: &'a str,
    value: &'a Value,
}

impl Opt<'_> {
    fn error(&self, expected: &'static str) -> OptionsError {
        OptionsError::Value {
            key: self.key.to_owned(),
            value: describe(self.value),
            expected,
        }
    }

    /// A weakly typed boolean (numbers are true when not zero, strings as Go's `ParseBool`).
    fn bool(&self) -> Result<bool, OptionsError> {
        match self.value {
            Value::Bool(b) => Ok(*b),
            Value::Int(i) => Ok(*i != 0),
            Value::Float(f) => Ok(*f != 0.0),
            Value::String(s) => match &**s {
                "1" | "t" | "T" | "true" | "TRUE" | "True" => Ok(true),
                "" | "0" | "f" | "F" | "false" | "FALSE" | "False" => Ok(false),
                _ => Err(self.error("a boolean")),
            },
            _ => Err(self.error("a boolean")),
        }
    }

    /// A weakly typed integer.
    fn int(&self) -> Result<i64, OptionsError> {
        match self.value {
            Value::Int(i) => Ok(*i),
            Value::Bool(b) => Ok(i64::from(*b)),
            #[expect(clippy::cast_possible_truncation, reason = "Go truncates the same way")]
            Value::Float(f) => Ok(*f as i64),
            Value::String(s) if s.is_empty() => Ok(0),
            Value::String(s) => parse_int(s).ok_or_else(|| self.error("an integer")),
            _ => Err(self.error("an integer")),
        }
    }

    /// A weakly typed string.
    fn string(&self) -> Result<String, OptionsError> {
        match self.value {
            Value::String(s) => Ok(s.to_string()),
            Value::Bool(b) => Ok(if *b { "1" } else { "0" }.to_owned()),
            Value::Int(i) => Ok(i.to_string()),
            Value::Float(f) => Ok(f.to_string()),
            _ => Err(self.error("a string")),
        }
    }
}

/// An integer with Go's base prefixes (`0x`, `0o`, `0b`, a leading `0` for octal).
fn parse_int(s: &str) -> Option<i64> {
    let (neg, digits) = match s.strip_prefix('-') {
        Some(d) => (true, d),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let digits = digits.replace('_', "");
    let lower = digits.to_ascii_lowercase();
    let n = if let Some(h) = lower.strip_prefix("0x") {
        i64::from_str_radix(h, 16).ok()?
    } else if let Some(b) = lower.strip_prefix("0b") {
        i64::from_str_radix(b, 2).ok()?
    } else if let Some(o) = lower.strip_prefix("0o") {
        i64::from_str_radix(o, 8).ok()?
    } else if lower.len() > 1 && lower.starts_with('0') {
        i64::from_str_radix(&lower[1..], 8).ok()?
    } else {
        lower.parse().ok()?
    };
    Some(if neg { -n } else { n })
}

fn describe(v: &Value) -> String {
    match v {
        Value::Null => "null".to_owned(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::String(s) => format!("{s:?}"),
        Value::Date(_) => "a date".to_owned(),
        Value::Array(_) => "an array".to_owned(),
        Value::Map(_) => "a map".to_owned(),
    }
}

/// `[[from, to], …]` (0-based, as a fence's parsed `hl_lines`), or `None` for any other shape.
fn line_ranges(v: &Value) -> Option<Vec<[i64; 2]>> {
    v.as_array()?
        .iter()
        .map(|r| match r.as_array()? {
            [from, to] => Some([from.as_i64()?, to.as_i64()?]),
            _ => None,
        })
        .collect()
}

impl Options {
    /// The site's defaults.
    #[must_use]
    pub fn from_config(c: &HighlightConfig) -> Self {
        Self {
            style: c.style.clone(),
            styling: if c.no_classes {
                Styling::Inline
            } else {
                Styling::Classes
            },
            line_nos: c.line_nos,
            line_number_layout: if c.line_numbers_in_table {
                LineNumberLayout::Table
            } else {
                LineNumberLayout::Inline
            },
            anchor_line_nos: c.anchor_line_nos,
            line_anchors: c.line_anchors.clone(),
            line_no_start: c.line_no_start,
            hl_lines: c.hl_lines.clone(),
            hl_ranges: None,
            layout: if c.hl_inline {
                CodeLayout::Inline
            } else {
                CodeLayout::Block
            },
            tab_width: c.tab_width,
            guess_syntax: c.guess_syntax,
            wrapper_class: c.wrapper_class.clone(),
        }
    }

    /// Applies an option string such as `linenos=table,hl_lines=3 6-8,style=emacs` (keys are
    /// trimmed and case-insensitive, values are taken as written).
    ///
    /// # Errors
    /// An item without exactly one `=`, or a value of the wrong type.
    pub fn apply_str(&mut self, s: &str) -> Result<(), OptionsError> {
        let s = s.trim_matches(' ');
        if s.is_empty() {
            return Ok(());
        }
        let mut map = Map::new();
        for item in s.split(',') {
            let mut kv = item.split('=');
            match (kv.next(), kv.next(), kv.next()) {
                (Some(k), Some(v), None) => {
                    map.insert(k.trim_matches(' '), Value::string(v));
                }
                _ => return Err(OptionsError::Syntax(item.trim_matches(' ').to_owned())),
            }
        }
        self.apply_map(&map)
    }

    /// Applies an option map: a fence's options or the `highlight` function's map. Keys are
    /// case-insensitive; unknown keys are ignored; `hl_lines` may be a string (`"3 6-8"`) or
    /// 0-based `[[from, to], …]` ranges (shifted by this map's `linenostart`).
    ///
    /// # Errors
    /// A value of the wrong type.
    pub fn apply_map(&mut self, map: &Map) -> Result<(), OptionsError> {
        let opts: Vec<(String, Opt<'_>)> = map
            .iter()
            .map(|(key, value)| (key.to_lowercase(), Opt { key, value }))
            .collect();
        let base = opts
            .iter()
            .find(|(k, _)| k == "linenostart")
            .map_or(1, |(_, o)| o.int().unwrap_or(0));
        for (key, o) in &opts {
            if o.value.is_null() {
                continue;
            }
            match key.as_str() {
                "style" => self.style = o.string()?,
                "noclasses" => {
                    self.styling = if o.bool()? {
                        Styling::Inline
                    } else {
                        Styling::Classes
                    };
                }
                "linenos" => {
                    match o.value.as_str() {
                        Some("table") => self.line_number_layout = LineNumberLayout::Table,
                        Some("inline") => self.line_number_layout = LineNumberLayout::Inline,
                        _ => {}
                    }
                    self.line_nos = match o.value.as_str() {
                        Some(s) => s != "false",
                        None => o.bool()?,
                    };
                }
                "linenumbersintable" => {
                    self.line_number_layout = if o.bool()? {
                        LineNumberLayout::Table
                    } else {
                        LineNumberLayout::Inline
                    };
                }
                "anchorlinenos" => self.anchor_line_nos = o.bool()?,
                "lineanchors" => self.line_anchors = o.string()?,
                "linenostart" => self.line_no_start = o.int()?,
                "hl_lines" => match line_ranges(o.value) {
                    Some(ranges) => {
                        let shifted = ranges.into_iter().map(|[a, b]| [a + base, b + base]);
                        self.hl_ranges = Some(shifted.collect());
                    }
                    None => self.hl_lines = o.string()?,
                },
                "hl_inline" => {
                    self.layout = if o.bool()? {
                        CodeLayout::Inline
                    } else {
                        CodeLayout::Block
                    };
                }
                "tabwidth" => self.tab_width = o.int()?,
                "guesssyntax" => self.guess_syntax = o.bool()?,
                "wrapperclass" => self.wrapper_class = o.string()?,
                _ => {}
            }
        }
        // `linenos=table|inline` also sets the layout when given before `linenumbersintable`
        // in the map; Go applies both in one decode, where the explicit key wins.
        if let Some((_, o)) = opts.iter().find(|(k, _)| k == "linenumbersintable")
            && !o.value.is_null()
        {
            self.line_number_layout = if o.bool()? {
                LineNumberLayout::Table
            } else {
                LineNumberLayout::Inline
            };
        }
        Ok(())
    }

    /// The highlighted line ranges (absolute, inclusive, sorted by start); an invalid
    /// `hl_lines` string highlights nothing.
    #[must_use]
    pub fn highlight_ranges(&self) -> Vec<[i64; 2]> {
        let mut ranges = match &self.hl_ranges {
            Some(r) => r.clone(),
            None => parse_hl_lines(&self.hl_lines, self.line_no_start).unwrap_or_default(),
        };
        ranges.sort_by_key(|r| r[0]);
        ranges
    }
}

/// Go's `hlLinesToRanges`: space-separated lines and ranges relative to `start`.
fn parse_hl_lines(s: &str, start: i64) -> Option<Vec<[i64; 2]>> {
    s.split(' ')
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .map(|field| {
            let mut parts = field.split('-');
            let first = parts.next()?.parse::<i64>().ok()? + start - 1;
            let last = match parts.next() {
                Some(p) => p.parse::<i64>().ok()? + start - 1,
                None => first,
            };
            Some([first, last])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_strings() {
        let mut o = Options::default();
        o.apply_str("linenos=inline, hl_Lines=3 6-8, noClasses=false")
            .expect("options");
        assert!(o.line_nos);
        assert_eq!(o.line_number_layout, LineNumberLayout::Inline);
        assert_eq!(o.styling, Styling::Classes);
        assert_eq!(o.highlight_ranges(), vec![[3, 3], [6, 8]]);
        assert!(o.apply_str("linenos").is_err());
        assert!(o.apply_str("tabwidth=x").is_err());
    }

    #[test]
    fn fence_ranges_shift_by_linenostart() {
        let mut o = Options::default();
        let map: Map = [
            (
                "hl_lines",
                Value::array(vec![Value::array(vec![Value::Int(1), Value::Int(2)])]),
            ),
            ("linenostart", Value::Int(10)),
        ]
        .into_iter()
        .collect();
        o.apply_map(&map).expect("options");
        assert_eq!(o.highlight_ranges(), vec![[11, 12]]);
        assert_eq!(o.line_no_start, 10);
    }
}
