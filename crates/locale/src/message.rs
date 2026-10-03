//! The message evaluator: text with `{{ . }}` and `{{ .Field }}` placeholders.
//!
//! i18n files keep Go's placeholder syntax, but a message is not a template: the only actions
//! are the dot (`{{ . }}`) and a field path (`{{ .Count }}`, `{{ .Page.Title }}`), optionally
//! with the trim markers `{{-` and `-}}`. Anything else (`if`, `with`, pipes, functions,
//! comments) is a [`SyntaxError`] when the file is loaded.

use std::fmt::{self, Write as _};

use ssg_base::Value;

use crate::plural::PluralCount;

/// What a placeholder prints when its value is missing (Go prints the same).
pub const NO_VALUE: &str = "<no value>";

/// One piece of a parsed message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece {
    /// Literal text.
    Text(String),
    /// `{{ . }}`: the whole argument.
    Dot,
    /// `{{ .A.B }}`: a field path into the argument.
    Field(Vec<String>),
}

/// An action the evaluator does not support, or an unterminated one.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{problem} at byte {offset}: `{action}`")]
pub struct SyntaxError {
    /// Byte offset of the action in the message.
    pub offset: usize,
    /// The action as written, delimiters included.
    pub action: String,
    problem: &'static str,
}

/// A value that cannot be printed into a message.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EvalError {
    /// `.Field` on a value that has no fields (a number, a string, …).
    #[error("`.{field}` on a {kind} value, which has no fields")]
    NoFields { field: String, kind: &'static str },
    /// A list or map printed as a whole.
    #[error("a {kind} value cannot be printed into a message")]
    NotPrintable { kind: &'static str },
}

/// A parsed message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Template {
    pieces: Vec<Piece>,
}

impl Template {
    /// Parses a message with the default delimiters `{{` and `}}`.
    ///
    /// # Errors
    /// An unsupported or unterminated action.
    pub fn parse(src: &str) -> Result<Self, SyntaxError> {
        Self::parse_with(src, "{{", "}}")
    }

    /// Parses a message with custom delimiters (a message's `leftDelim`/`rightDelim`).
    ///
    /// # Errors
    /// An unsupported or unterminated action.
    pub fn parse_with(src: &str, left: &str, right: &str) -> Result<Self, SyntaxError> {
        let mut pieces = Vec::new();
        let mut text = String::new();
        let mut rest = src;
        let mut trim_next = false;
        while let Some(start) = rest.find(left) {
            let offset = src.len() - rest.len() + start;
            push_text(&mut text, &rest[..start], trim_next);
            let after = &rest[start + left.len()..];
            let Some(end) = after.find(right) else {
                return Err(syntax(offset, &rest[start..], "unterminated action"));
            };
            let action = &after[..end];
            let whole = &rest[start..start + left.len() + end + right.len()];
            let (inner, trim_before, trim_after) = trim_markers(action);
            if trim_before {
                text.truncate(text.trim_end().len());
            }
            let piece = parse_action(inner.trim()).ok_or_else(|| {
                syntax(
                    offset,
                    whole,
                    "only `{{ . }}` and `{{ .Field }}` are supported",
                )
            })?;
            if !text.is_empty() {
                pieces.push(Piece::Text(std::mem::take(&mut text)));
            }
            pieces.push(piece);
            trim_next = trim_after;
            rest = &after[end + right.len()..];
        }
        push_text(&mut text, rest, trim_next);
        if !text.is_empty() {
            pieces.push(Piece::Text(text));
        }
        Ok(Self { pieces })
    }

    /// The pieces, in order.
    #[must_use]
    pub fn pieces(&self) -> &[Piece] {
        &self.pieces
    }

    /// Renders the message for `data` (the template argument). Without data, `count` is both
    /// `.` and `.Count`; a number or numeric string argument is its own `.Count`. A map key
    /// that is missing prints [`NO_VALUE`].
    ///
    /// # Errors
    /// A field of a value without fields, or a list or map printed whole.
    pub fn render(&self, data: &Value, count: Option<&PluralCount>) -> Result<String, EvalError> {
        let mut out = String::new();
        for piece in &self.pieces {
            match piece {
                Piece::Text(t) => out.push_str(t),
                Piece::Dot => match (data, count) {
                    (Value::Null, Some(c)) => out.push_str(c.as_str()),
                    (v, _) => print(&mut out, v)?,
                },
                Piece::Field(path) => match lookup(data, path, count)? {
                    Some(v) => print(&mut out, &v)?,
                    None => out.push_str(NO_VALUE),
                },
            }
        }
        Ok(out)
    }
}

fn syntax(offset: usize, action: &str, problem: &'static str) -> SyntaxError {
    SyntaxError {
        offset,
        action: action.to_owned(),
        problem,
    }
}

fn push_text(text: &mut String, s: &str, trim_start: bool) {
    text.push_str(if trim_start { s.trim_start() } else { s });
}

/// Splits the trim markers off an action: `- x -` → (`x`, true, true). A marker is a `-`
/// separated from the action by white space, as in Go templates.
fn trim_markers(action: &str) -> (&str, bool, bool) {
    let mut inner = action;
    let before = inner
        .strip_prefix('-')
        .filter(|r| r.starts_with(char::is_whitespace));
    if let Some(r) = before {
        inner = r;
    }
    let after = inner
        .strip_suffix('-')
        .filter(|r| r.ends_with(char::is_whitespace));
    if let Some(r) = after {
        inner = r;
    }
    (inner, before.is_some(), after.is_some())
}

fn parse_action(action: &str) -> Option<Piece> {
    if action == "." {
        return Some(Piece::Dot);
    }
    let path = action.strip_prefix('.')?;
    let fields: Vec<String> = path.split('.').map(str::to_owned).collect();
    fields
        .iter()
        .all(|f| is_identifier(f))
        .then_some(Piece::Field(fields))
}

fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    chars.next().is_some_and(|c| c == '_' || c.is_alphabetic())
        && chars.all(|c| c == '_' || c.is_alphanumeric())
}

/// The value at `path`, `None` when a map has no such key or the argument is missing.
fn lookup(
    data: &Value,
    path: &[String],
    count: Option<&PluralCount>,
) -> Result<Option<Value>, EvalError> {
    let mut current = data.clone();
    for (i, field) in path.iter().enumerate() {
        let is_count = i == 0 && field == "Count";
        current = match &current {
            Value::Null if is_count => return Ok(count.map(count_value)),
            Value::Null => return Ok(None),
            Value::Map(m) => match m.get(field) {
                Some(v) => v.clone(),
                None => return Ok(None),
            },
            // A number or numeric string passed as the argument is the count itself.
            v if is_count && count.is_some() && !matches!(v, Value::Array(_)) => v.clone(),
            v => {
                return Err(EvalError::NoFields {
                    field: field.clone(),
                    kind: kind(v),
                });
            }
        };
    }
    Ok(Some(current))
}

fn count_value(c: &PluralCount) -> Value {
    Value::string(c.as_str())
}

fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "missing",
        Value::Bool(_) => "boolean",
        Value::Int(_) | Value::Float(_) => "number",
        Value::String(_) => "string",
        Value::Date(_) => "date",
        Value::Array(_) => "list",
        Value::Map(_) => "map",
    }
}

fn print(out: &mut String, v: &Value) -> Result<(), EvalError> {
    match v {
        Value::Null => out.push_str(NO_VALUE),
        Value::Bool(b) => {
            let _ = write!(out, "{b}");
        }
        Value::Int(i) => {
            let _ = write!(out, "{i}");
        }
        Value::Float(f) => {
            let _ = write!(out, "{}", Float(*f));
        }
        Value::String(s) => out.push_str(s),
        Value::Date(d) => {
            let _ = write!(out, "{d}");
        }
        Value::Array(_) | Value::Map(_) => return Err(EvalError::NotPrintable { kind: kind(v) }),
    }
    Ok(())
}

/// A float as Go prints it: the shortest round-trip digits, in exponent form (`1e+06`,
/// `1.5e-07`) when the decimal exponent is below -4 or at least 6.
struct Float(f64);

impl fmt::Display for Float {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let x = self.0;
        if !x.is_finite() {
            return f.write_str(if x.is_nan() {
                "NaN"
            } else if x > 0.0 {
                "+Inf"
            } else {
                "-Inf"
            });
        }
        // `{:e}` gives the shortest round-trip mantissa: `-1.2345e6`.
        let sci = format!("{x:e}");
        let (mantissa, exp) = sci.split_once('e').expect("`{:e}` has an exponent");
        let exp: i32 = exp.parse().expect("`{:e}` exponent is an integer");
        let (sign, mantissa) = mantissa
            .strip_prefix('-')
            .map_or(("", mantissa), |m| ("-", m));
        let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
        if !(-4..6).contains(&exp) {
            let (head, tail) = digits.split_at(1);
            let dot = if tail.is_empty() { "" } else { "." };
            let esign = if exp < 0 { '-' } else { '+' };
            return write!(f, "{sign}{head}{dot}{tail}e{esign}{:02}", exp.abs());
        }
        let point = exp + 1; // digits before the decimal point
        let s = if point <= 0 {
            let zeros = "0".repeat(point.unsigned_abs() as usize);
            format!("0.{zeros}{digits}")
        } else {
            let point = point.unsigned_abs() as usize;
            if digits.len() <= point {
                format!("{digits}{}", "0".repeat(point - digits.len()))
            } else {
                format!("{}.{}", &digits[..point], &digits[point..])
            }
        };
        write!(f, "{sign}{s}")
    }
}
