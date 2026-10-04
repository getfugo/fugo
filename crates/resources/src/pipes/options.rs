//! Decoding the options of a pipe: booleans, strings and lists, with Go's errors.

use super::*;

/// The entries of an options map, keys lower-cased (options are matched case-insensitively);
/// `null` is no options.
pub(super) fn option_entries(v: &Json) -> Result<Vec<(String, &Json)>, PipeError> {
    match v {
        Json::Null => Ok(Vec::new()),
        Json::Object(m) => Ok(m
            .iter()
            .filter(|(_, v)| !v.is_null())
            .map(|(k, v)| (k.to_ascii_lowercase(), v))
            .collect()),
        other => Err(PipeError::Option {
            option: String::new(),
            reason: format!("the options must be a map, got {other}"),
        }),
    }
}

pub(super) fn bad(option: &str, reason: impl Into<String>) -> PipeError {
    PipeError::Option {
        option: option.to_owned(),
        reason: reason.into(),
    }
}

/// A boolean option; `"true"`/`"false"` and numbers are accepted, as templates pass them.
pub(super) fn opt_bool(option: &str, v: &Json) -> Result<bool, PipeError> {
    match v {
        Json::Bool(b) => Ok(*b),
        Json::String(s) => match s.to_ascii_lowercase().as_str() {
            "true" | "1" => Ok(true),
            "false" | "0" | "" => Ok(false),
            _ => Err(bad(option, format!("expected a boolean, got {s:?}"))),
        },
        Json::Number(n) => Ok(n.as_f64().is_some_and(|f| f != 0.0)),
        other => Err(bad(option, format!("expected a boolean, got {other}"))),
    }
}

/// A string option; numbers and booleans are accepted as their text.
pub(super) fn opt_string(option: &str, v: &Json) -> Result<String, PipeError> {
    match v {
        Json::String(s) => Ok(s.clone()),
        Json::Number(n) => Ok(n.to_string()),
        Json::Bool(b) => Ok(b.to_string()),
        other => Err(bad(option, format!("expected a string, got {other}"))),
    }
}

/// A list of strings; a single string is a one-item list.
pub(super) fn opt_strings(option: &str, v: &Json) -> Result<Vec<String>, PipeError> {
    match v {
        Json::Array(a) => a.iter().map(|x| opt_string(option, x)).collect(),
        other => Ok(vec![opt_string(option, other)?]),
    }
}
