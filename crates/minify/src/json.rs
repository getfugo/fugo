//! JSON: removes insignificant whitespace, keeping member order, strings and numbers verbatim.
//!
//! The grammar is checked, with two leniencies the Go implementation's minifier also has: a
//! trailing comma before `}` or `]` is accepted (and dropped), and raw control characters inside
//! strings are copied.
//! Whitespace-only input minifies to the empty string.

use crate::MinifyError;

/// What is wrong with a JSON input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum JsonErrorKind {
    #[error("unexpected character {0:?}")]
    Unexpected(char),
    #[error("unexpected end of input")]
    Eof,
    #[error("invalid number")]
    Number,
    #[error("invalid escape sequence")]
    Escape,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Container {
    Object,
    Array,
}

/// What the next token may be.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Expect {
    /// A value (top level, or after `:`).
    Value,
    /// A value or `]` (after `[` or an array's `,`).
    ValueOrClose,
    /// A member name or `}` (after `{` or an object's `,`).
    KeyOrClose,
    /// `:` after a member name.
    Colon,
    /// `,` or the closing bracket, or the end at the top level.
    Next,
}

pub(crate) fn minify(input: &str) -> Result<String, MinifyError> {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut stack: Vec<Container> = Vec::new();
    let mut expect = Expect::Value;
    let mut i = 0;
    let fail = |offset: usize, kind: JsonErrorKind| MinifyError::Json { offset, kind };
    let unexpected = |i: usize| {
        let c = input[i..].chars().next().unwrap_or_default();
        fail(i, JsonErrorKind::Unexpected(c))
    };
    loop {
        while i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r') {
            i += 1;
        }
        let Some(&b) = bytes.get(i) else {
            return match (expect, stack.is_empty()) {
                (Expect::Next, true) => Ok(out),
                (Expect::Value, true) if out.is_empty() => Ok(out),
                _ => Err(fail(i, JsonErrorKind::Eof)),
            };
        };
        let top = stack.last().copied();
        match (expect, b) {
            (Expect::Value | Expect::ValueOrClose, _) if !matches!(b, b']') => {
                i = value(input, i, &mut out, &mut stack)?;
                expect = match b {
                    b'{' => Expect::KeyOrClose,
                    b'[' => Expect::ValueOrClose,
                    _ => Expect::Next,
                };
            }
            (Expect::KeyOrClose, b'"') => {
                let end = string_end(input, i)?;
                out.push_str(&input[i..end]);
                i = end;
                expect = Expect::Colon;
            }
            (Expect::Colon, b':') => {
                out.push(':');
                i += 1;
                expect = Expect::Value;
            }
            (Expect::Next, b',') if top.is_some() => {
                out.push(',');
                i += 1;
                expect = match top {
                    Some(Container::Object) => Expect::KeyOrClose,
                    _ => Expect::ValueOrClose,
                };
            }
            (Expect::Next | Expect::KeyOrClose, b'}') if top == Some(Container::Object) => {
                close(&mut out, &mut stack, '}');
                i += 1;
                expect = Expect::Next;
            }
            (Expect::Next | Expect::ValueOrClose, b']') if top == Some(Container::Array) => {
                close(&mut out, &mut stack, ']');
                i += 1;
                expect = Expect::Next;
            }
            _ => return Err(unexpected(i)),
        }
    }
}

/// Writes the closing bracket, dropping a trailing comma.
fn close(out: &mut String, stack: &mut Vec<Container>, bracket: char) {
    if out.ends_with(',') {
        out.pop();
    }
    stack.pop();
    out.push(bracket);
}

/// Copies the value starting at `i` (or opens its container); returns the offset after it.
fn value(
    input: &str,
    i: usize,
    out: &mut String,
    stack: &mut Vec<Container>,
) -> Result<usize, MinifyError> {
    let bytes = input.as_bytes();
    let end = match bytes[i] {
        b'{' => {
            stack.push(Container::Object);
            i + 1
        }
        b'[' => {
            stack.push(Container::Array);
            i + 1
        }
        b'"' => string_end(input, i)?,
        b'-' | b'0'..=b'9' => number_end(bytes, i)?,
        _ => {
            let rest = &input[i..];
            let len = ["true", "false", "null"]
                .into_iter()
                .find(|lit| rest.starts_with(lit))
                .map(str::len)
                .ok_or_else(|| MinifyError::Json {
                    offset: i,
                    kind: JsonErrorKind::Unexpected(rest.chars().next().unwrap_or_default()),
                })?;
            i + len
        }
    };
    out.push_str(&input[i..end]);
    Ok(end)
}

/// The offset after the string starting with the `"` at `start`.
fn string_end(input: &str, start: usize) -> Result<usize, MinifyError> {
    let bytes = input.as_bytes();
    let mut i = start + 1;
    loop {
        match bytes.get(i) {
            None => {
                return Err(MinifyError::Json {
                    offset: i,
                    kind: JsonErrorKind::Eof,
                });
            }
            Some(b'"') => return Ok(i + 1),
            Some(b'\\') => {
                let ok = match bytes.get(i + 1) {
                    Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => {
                        i += 2;
                        true
                    }
                    Some(b'u') => {
                        let hex = bytes.get(i + 2..i + 6);
                        i += 6;
                        hex.is_some_and(|h| h.iter().all(u8::is_ascii_hexdigit))
                    }
                    _ => false,
                };
                if !ok {
                    return Err(MinifyError::Json {
                        offset: i,
                        kind: JsonErrorKind::Escape,
                    });
                }
            }
            Some(_) => i += 1,
        }
    }
}

/// The offset after the number starting at `start`:
/// `-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?`.
fn number_end(bytes: &[u8], start: usize) -> Result<usize, MinifyError> {
    let digits = |from: usize| {
        let n = bytes[from..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count();
        (n > 0).then_some(from + n)
    };
    let bad = |offset| MinifyError::Json {
        offset,
        kind: JsonErrorKind::Number,
    };
    let mut i = start + usize::from(bytes[start] == b'-');
    i = match bytes.get(i) {
        Some(b'0') => i + 1,
        Some(b'1'..=b'9') => digits(i).unwrap_or(i),
        _ => return Err(bad(i)),
    };
    if bytes.get(i) == Some(&b'.') {
        i = digits(i + 1).ok_or_else(|| bad(i + 1))?;
    }
    if matches!(bytes.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(bytes.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        i = digits(i).ok_or_else(|| bad(i))?;
    }
    Ok(i)
}
