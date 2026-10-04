//! The YAML and TOML writers of `remarshal`: the output of Go's encoders (yaml.v2's emitter
//! and go-toml v2 with indented tables), so a configuration sample reads the same.
//!
//! - Scalars: YAML strings are plain when that reads back as the same string, else single-
//!   quoted, else double-quoted (yaml.v2); a multi-line string is a literal block. TOML strings
//!   are literal (`'…'`) unless they hold a `'`, a line break or a control character.
//! - Keys: YAML maps in yaml.v2's natural order (digit runs by value, letters after other
//!   characters); TOML keys bare when they can be, sorted bytewise, plain values before tables.
//! - Dates of a TOML document stay dates: TOML writes them bare, YAML a local date as a quoted
//!   string and an offset date-time plain, JSON as strings.

use std::cmp::Ordering;
use std::fmt::Write as _;
use std::sync::LazyLock;

use regex::Regex;
use tera::{Map, TeraResult, Value};

use super::value::entries;

mod toml_v2;
mod yaml_v2;

pub(super) use toml_v2::*;
pub(super) use yaml_v2::*;

/// The prefix that marks a TOML local date, date-time or time (as text) inside `remarshal`.
const LOCAL_DATE: &str = "\u{0}toml-local-date\u{0}";
/// The prefix that marks a TOML offset date-time (as RFC 3339 text) inside `remarshal`.
const OFFSET_DATE: &str = "\u{0}toml-offset-date\u{0}";

/// A marked TOML date: the text of a local (`false`) or offset (`true`) date.
pub(super) fn date_marker(text: &str, offset: bool) -> String {
    format!("{}{text}", if offset { OFFSET_DATE } else { LOCAL_DATE })
}

/// The date of a marked string: its text and whether it has an offset.
fn marked_date(s: &str) -> Option<(&str, bool)> {
    s.strip_prefix(LOCAL_DATE)
        .map(|d| (d, false))
        .or_else(|| s.strip_prefix(OFFSET_DATE).map(|d| (d, true)))
}

/// The data as Go's `remarshal` encodes it: integral floats of maps (not of arrays) become
/// integers (`applyMarshalTypes`); `plain_dates` turns marked dates back into strings (JSON).
pub(super) fn prepare(v: &Value, plain_dates: bool) -> Value {
    fn walk(v: &Value, in_map: bool, plain_dates: bool) -> Value {
        if let Some(m) = v.as_map() {
            let mut out = Map::new();
            for (k, item) in entries(m) {
                out.insert(k.into_owned().into(), walk(item, true, plain_dates));
            }
            Value::from(out)
        } else if let Some(a) = v.as_array() {
            Value::from(
                a.iter()
                    .map(|item| walk(item, false, plain_dates))
                    .collect::<Vec<_>>(),
            )
        } else if in_map && v.is_f64() {
            let f = v.as_f64().unwrap_or_default();
            #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
            let i = f as i64;
            #[allow(clippy::cast_precision_loss)]
            if f.is_finite() && i as f64 == f {
                Value::from(i)
            } else {
                v.clone()
            }
        } else if plain_dates && let Some((date, _)) = v.as_str().and_then(marked_date) {
            Value::from(date)
        } else {
            v.clone()
        }
    }
    walk(v, false, plain_dates)
}

#[cfg(test)]
mod tests;
