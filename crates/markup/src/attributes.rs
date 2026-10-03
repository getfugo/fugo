//! The Go implementation's `{…}` attribute syntax (goldmark's attribute grammar), used after
//! headings, on a line after a block (`attribute.block`) and in code-fence info strings.
//!
//! ```text
//! attributes := "{" (attribute ","?)* "}"
//! attribute  := "#" id | "." class | name "=" value
//! value      := string | number | "true" | "false" | "null" | word | "[" value ("," value)* "]"
//! ```
//! Repeated classes are joined with a space. Values become [`Value`]s the way Go's
//! attribute holder converts them: names lower-cased, `on*` event handlers dropped, arrays
//! turned into 0-based `[from, to]` line ranges (`hl_lines=[2, "4-5"]`).

use ssg_base::{Map, Value};

/// A parsed attribute value.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum AttrValue {
    Str(String),
    /// A number; `int` when the literal has no fraction or exponent.
    Num {
        value: f64,
        int: Option<i64>,
    },
    Bool(bool),
    Null,
    Array(Vec<AttrValue>),
}

/// One `name=value` (`id` for `#…`, `class` for `.…`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Attr {
    pub name: String,
    pub value: AttrValue,
}

/// An attribute value the Go implementation cannot represent (`null`).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("attribute {name:?}: null values are not supported")]
pub(crate) struct NullValue {
    pub name: String,
}

/// Parses attributes at the start of `s` (leading spaces allowed). Returns them and the
/// number of bytes consumed through the closing `}`.
pub(crate) fn parse(s: &str) -> Option<(Vec<Attr>, usize)> {
    let mut p = Parser { s, i: 0 };
    p.skip_spaces();
    if !p.eat(b'{') {
        return None;
    }
    let mut attrs: Vec<Attr> = Vec::new();
    loop {
        p.skip_spaces();
        if p.eat(b'}') {
            return Some((attrs, p.i));
        }
        let attr = p.attribute()?;
        if attr.name == "class"
            && let Some(prev) = attrs.iter_mut().find(|a| a.name == "class")
            && let (AttrValue::Str(a), AttrValue::Str(b)) = (&mut prev.value, &attr.value)
        {
            a.push(' ');
            a.push_str(b);
        } else {
            attrs.push(attr);
        }
        p.skip_spaces();
        p.eat(b',');
    }
}

/// Parses `s` when it is exactly one attribute block (surrounding spaces allowed).
pub(crate) fn parse_all(s: &str) -> Option<Vec<Attr>> {
    let (attrs, n) = parse(s)?;
    s[n..].trim().is_empty().then_some(attrs)
}

struct Parser<'s> {
    s: &'s str,
    i: usize,
}

fn is_name_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c == b':'
}

fn is_name(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b':' | b'.' | b'-')
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.as_bytes().get(self.i).copied()
    }

    fn eat(&mut self, c: u8) -> bool {
        let hit = self.peek() == Some(c);
        if hit {
            self.i += 1;
        }
        hit
    }

    /// Spaces and tabs (attributes never span lines).
    fn skip_spaces(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t')) {
            self.i += 1;
        }
    }

    fn take_while(&mut self, f: impl Fn(u8) -> bool) -> &str {
        let start = self.i;
        while self.peek().is_some_and(&f) {
            self.i += 1;
        }
        &self.s[start..self.i]
    }

    fn attribute(&mut self) -> Option<Attr> {
        match self.peek()? {
            c @ (b'#' | b'.') => {
                self.i += 1;
                // Ids and classes: no spaces, and of ASCII punctuation only `_ - : .`.
                let v = self.take_while(|b| {
                    !b.is_ascii_whitespace()
                        && (!b.is_ascii_punctuation() || matches!(b, b'_' | b'-' | b':' | b'.'))
                });
                let name = if c == b'#' { "id" } else { "class" };
                Some(Attr {
                    name: name.to_owned(),
                    value: AttrValue::Str(v.to_owned()),
                })
            }
            c if is_name_start(c) => {
                let name = self.take_while(is_name).to_owned();
                self.skip_spaces();
                if !self.eat(b'=') {
                    return None;
                }
                let value = self.value()?;
                if name == "class" && !matches!(value, AttrValue::Str(_)) {
                    return None;
                }
                Some(Attr { name, value })
            }
            _ => None,
        }
    }

    fn value(&mut self) -> Option<AttrValue> {
        self.skip_spaces();
        match self.peek()? {
            b'[' => self.array(),
            b'"' => self.string().map(AttrValue::Str),
            b'-' | b'+' | b'0'..=b'9' => self.number(),
            c if is_name_start(c) => Some(match self.take_while(is_name) {
                "true" => AttrValue::Bool(true),
                "false" => AttrValue::Bool(false),
                "null" => AttrValue::Null,
                w => AttrValue::Str(w.to_owned()),
            }),
            // Objects (`{…}`) are not supported by Go's attribute holder either.
            _ => None,
        }
    }

    fn array(&mut self) -> Option<AttrValue> {
        self.i += 1;
        let mut items = Vec::new();
        loop {
            self.skip_spaces();
            if !items.is_empty() {
                if self.eat(b']') {
                    return Some(AttrValue::Array(items));
                }
                if !self.eat(b',') {
                    return None;
                }
            } else if self.eat(b']') {
                return Some(AttrValue::Array(items));
            }
            items.push(self.value()?);
        }
    }

    fn string(&mut self) -> Option<String> {
        self.i += 1;
        let mut out = String::new();
        let rest = &self.s[self.i..];
        let mut chars = rest.char_indices();
        while let Some((at, c)) = chars.next() {
            match c {
                '\n' => return None,
                '"' => {
                    self.i += at + 1;
                    return Some(out);
                }
                '\\' => match rest[at + 1..].chars().next() {
                    Some(e @ ('"' | '/' | '\\')) => {
                        out.push(e);
                        chars.next();
                    }
                    Some(e @ ('b' | 'f' | 'n' | 'r' | 't')) => {
                        out.push(match e {
                            'b' => '\u{8}',
                            'f' => '\u{c}',
                            'n' => '\n',
                            'r' => '\r',
                            _ => '\t',
                        });
                        chars.next();
                    }
                    _ => out.push('\\'),
                },
                c => out.push(c),
            }
        }
        None
    }

    fn number(&mut self) -> Option<AttrValue> {
        let start = self.i;
        if matches!(self.peek(), Some(b'-' | b'+')) {
            self.i += 1;
        }
        if self.take_while(|b| b.is_ascii_digit()).is_empty() {
            return None;
        }
        let mut integral = true;
        if self.eat(b'.') {
            integral = false;
            self.take_while(|b| b.is_ascii_digit());
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            integral = false;
            self.i += 1;
            if matches!(self.peek(), Some(b'-' | b'+')) {
                self.i += 1;
            }
            self.take_while(|b| b.is_ascii_digit());
        }
        let lit = self.s[start..self.i].trim_start_matches('+');
        let value: f64 = lit.parse().ok()?;
        let int = if integral { lit.parse().ok() } else { None };
        Some(AttrValue::Num { value, int })
    }
}

/// Highlighter options of a code fence (Chroma's names; matched case-insensitively).
const HIGHLIGHT_OPTIONS: &[&str] = &[
    "anchorlinenos",
    "guesssyntax",
    "hl_lines",
    "hl_inline",
    "lineanchors",
    "linenos",
    "linenostart",
    "linenumbersintable",
    "noclasses",
    "style",
    "tabwidth",
];

/// Where attributes are used: code fences split highlighter options from attributes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Owner {
    General,
    CodeBlock,
}

/// Attributes converted for templates: `(attributes, options)`, both in source order.
pub(crate) type Converted = (Vec<(String, Value)>, Vec<(String, Value)>);

/// Converts parsed attributes the way the Go implementation exposes them to templates.
pub(crate) fn convert(attrs: &[Attr], owner: Owner) -> Result<Converted, NullValue> {
    let mut plain = Vec::new();
    let mut options = Vec::new();
    for a in attrs {
        let lower = a.name.to_lowercase();
        if lower.starts_with("on") {
            continue;
        }
        let value = match &a.value {
            AttrValue::Str(s) => Value::string(s),
            AttrValue::Bool(b) => Value::Bool(*b),
            AttrValue::Num { value, int } => int.map_or(Value::Float(*value), Value::Int),
            AttrValue::Null => {
                return Err(NullValue {
                    name: a.name.clone(),
                });
            }
            AttrValue::Array(items) => line_ranges(items),
        };
        if owner == Owner::CodeBlock && HIGHLIGHT_OPTIONS.contains(&lower.as_str()) {
            options.push((a.name.clone(), value));
        } else {
            plain.push((lower, value));
        }
    }
    Ok((plain, options))
}

/// `[2, "4-5"]` → `[[1, 1], [3, 4]]`; `Null` when no item is a line or range.
fn line_ranges(items: &[AttrValue]) -> Value {
    let ranges: Vec<Value> = items
        .iter()
        .filter_map(|item| {
            let (from, to) = match item {
                AttrValue::Num { value, .. } => {
                    #[expect(clippy::cast_possible_truncation, reason = "line numbers")]
                    let n = *value as i64;
                    (n, n)
                }
                AttrValue::Str(s) => {
                    let mut parts = s.split('-');
                    let from: i64 = parts.next()?.trim().parse().ok()?;
                    let to = match parts.next() {
                        Some(t) => t.trim().parse().ok()?,
                        None => from,
                    };
                    (from, to)
                }
                _ => return None,
            };
            Some(Value::array(vec![Value::Int(from - 1), Value::Int(to - 1)]))
        })
        .collect();
    if ranges.is_empty() {
        Value::Null
    } else {
        Value::array(ranges)
    }
}

/// The map templates see (a later attribute of the same name wins).
pub(crate) fn to_map(attrs: &[(String, Value)]) -> Map {
    attrs.iter().map(|(k, v)| (k.as_str(), v.clone())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(s: &str) -> Vec<(String, AttrValue)> {
        parse_all(s)
            .expect("parses")
            .into_iter()
            .map(|a| (a.name, a.value))
            .collect()
    }

    fn st(s: &str) -> AttrValue {
        AttrValue::Str(s.to_owned())
    }

    #[test]
    fn grammar() {
        assert_eq!(
            names(r#"{#custom-id .cls1 .cls2 data-x="1" num=3.5 flag=true}"#),
            vec![
                ("id".into(), st("custom-id")),
                ("class".into(), st("cls1 cls2")),
                ("data-x".into(), st("1")),
                (
                    "num".into(),
                    AttrValue::Num {
                        value: 3.5,
                        int: None
                    }
                ),
                ("flag".into(), AttrValue::Bool(true)),
            ]
        );
        assert_eq!(
            names(r#"{.class linenos=table hl_lines=[2,"4-5"] style=monokai}"#)[2],
            (
                "hl_lines".into(),
                AttrValue::Array(vec![
                    AttrValue::Num {
                        value: 2.0,
                        int: Some(2)
                    },
                    st("4-5")
                ])
            )
        );
        assert!(parse_all("{=x}").is_none());
        assert!(parse_all("{a={b=1}}").is_none());
        assert!(parse_all("{bad attrs here").is_none());
        assert_eq!(names("{ #a, .b }").len(), 2);
    }

    #[test]
    fn conversion() {
        let a = parse_all(r#"{.class linenos=table hl_lines=[2,"4-5"] Style=monokai onclick="x"}"#)
            .expect("parses");
        let (plain, opts) = convert(&a, Owner::CodeBlock).expect("no null");
        assert_eq!(plain, vec![("class".to_owned(), Value::string("class"))]);
        assert_eq!(opts[0].0, "linenos");
        assert_eq!(
            opts[1].1,
            Value::array(vec![
                Value::array(vec![Value::Int(1), Value::Int(1)]),
                Value::array(vec![Value::Int(3), Value::Int(4)]),
            ])
        );
        assert_eq!(opts[2].0, "Style");
        let null = parse_all("{a=null}").expect("parses");
        assert!(convert(&null, Owner::General).is_err());
    }
}
