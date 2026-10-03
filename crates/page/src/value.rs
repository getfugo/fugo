//! Lenient conversions of front matter values (Go's `cast` rules).

use std::sync::Arc;

use ssg_base::Value;

/// A description of the shape of `v`, for error messages.
pub(crate) fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "nothing",
        Value::Bool(_) => "a boolean",
        Value::Int(_) => "an integer",
        Value::Float(_) => "a number",
        Value::String(_) => "a string",
        Value::Date(_) => "a date",
        Value::Array(_) => "a list",
        Value::Map(_) => "a table",
    }
}

/// A scalar as a string: strings as they are, booleans and numbers written out, nothing as
/// `""`. Lists, tables and dates have no string form.
pub(crate) fn weak_string(v: &Value) -> Option<String> {
    Some(match v {
        Value::Null => String::new(),
        Value::String(s) => s.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => format_float(*f),
        Value::Date(d) => d.to_string(),
        Value::Array(_) | Value::Map(_) => return None,
    })
}

/// Go's shortest `%v` of a float (`1.5`, `2`, `1e+21`).
fn format_float(f: f64) -> String {
    if f.is_finite() && f == f.trunc() && f.abs() < 1e21 {
        format!("{f:.0}")
    } else {
        f.to_string()
    }
}

/// A boolean: `true`/`false`, the strings Go's `ParseBool` accepts, numbers (non-zero is
/// true), nothing (false).
pub(crate) fn weak_bool(v: &Value) -> Option<bool> {
    match v {
        Value::Null => Some(false),
        Value::Bool(b) => Some(*b),
        Value::Int(i) => Some(*i != 0),
        Value::Float(f) => Some(*f != 0.0),
        Value::String(s) => match &**s {
            "1" | "t" | "T" | "true" | "TRUE" | "True" => Some(true),
            "" | "0" | "f" | "F" | "false" | "FALSE" | "False" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// An integer: integers, floats (truncated), numeric strings, booleans (1/0), nothing (0).
pub(crate) fn weak_int(v: &Value) -> Option<i64> {
    match v {
        Value::Null => Some(0),
        Value::Bool(b) => Some(i64::from(*b)),
        Value::Int(i) => Some(*i),
        #[expect(
            clippy::cast_possible_truncation,
            reason = "Go truncates float weights to int"
        )]
        Value::Float(f) if f.is_finite() => Some(f.trunc() as i64),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// A list of strings: a list's scalar items (others dropped), or a string split at white
/// space.
pub(crate) fn string_list(v: &Value) -> Vec<String> {
    match v {
        Value::Array(a) => a.iter().filter_map(weak_string).collect(),
        Value::String(s) => s.split_whitespace().map(str::to_owned).collect(),
        Value::Null => Vec::new(),
        other => weak_string(other).into_iter().collect(),
    }
}

/// A list of strings as a value.
pub(crate) fn string_array(items: &[String]) -> Value {
    Value::Array(Arc::new(
        items.iter().map(|s| Value::string(s)).collect::<Vec<_>>(),
    ))
}
