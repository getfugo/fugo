//! Durations written in configuration: Go-style strings (`1h30m`, `2m30s`, `500ms`) or numbers
//! of seconds.

use std::time::Duration;

use ssg_base::Value;

/// A duration string that could not be parsed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} is not a duration (use a number of seconds or units such as \"90s\", \"1h30m\")")]
pub struct DurationError(pub String);

/// A signed duration: `-1` means "forever" for cache ages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SignedDuration {
    pub negative: bool,
    pub duration: Duration,
}

/// Parses `s` as a sequence of `<number><unit>` (units `ns`, `us`, `µs`, `ms`, `s`, `m`, `h`),
/// with an optional sign; `"0"` is zero.
///
/// # Errors
/// Anything else.
pub fn parse(s: &str) -> Result<SignedDuration, DurationError> {
    let err = || DurationError(s.to_owned());
    let t = s.trim();
    let (negative, mut rest) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    if rest == "0" {
        return Ok(SignedDuration {
            negative,
            duration: Duration::ZERO,
        });
    }
    if rest.is_empty() {
        return Err(err());
    }
    let mut total = 0f64;
    while !rest.is_empty() {
        let num_len = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .ok_or_else(err)?;
        if num_len == 0 {
            return Err(err());
        }
        let n: f64 = rest[..num_len].parse().map_err(|_| err())?;
        rest = &rest[num_len..];
        let unit_len = rest
            .find(|c: char| c.is_ascii_digit() || c == '.')
            .unwrap_or(rest.len());
        let scale = match &rest[..unit_len] {
            "ns" => 1e-9,
            "us" | "µs" | "μs" => 1e-6,
            "ms" => 1e-3,
            "s" => 1.0,
            "m" => 60.0,
            "h" => 3600.0,
            _ => return Err(err()),
        };
        rest = &rest[unit_len..];
        total += n * scale;
    }
    Ok(SignedDuration {
        negative,
        duration: Duration::try_from_secs_f64(total).map_err(|_| err())?,
    })
}

/// A duration from a configuration value: a string as in [`parse`], or a number of seconds.
///
/// # Errors
/// A value that is neither.
pub fn from_value(v: &Value) -> Result<SignedDuration, DurationError> {
    match v {
        Value::Int(i) => Ok(SignedDuration {
            negative: *i < 0,
            duration: Duration::from_secs(i.unsigned_abs()),
        }),
        Value::Float(f) => Ok(SignedDuration {
            negative: *f < 0.0,
            duration: Duration::try_from_secs_f64(f.abs())
                .map_err(|_| DurationError(f.to_string()))?,
        }),
        Value::String(s) => match s.trim().parse::<i64>() {
            Ok(i) => from_value(&Value::Int(i)),
            Err(_) => parse(s),
        },
        other => Err(DurationError(format!("{other:?}"))),
    }
}

/// Deserializes an optional [`SignedDuration`] with [`from_value`].
pub(crate) fn de_opt<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<SignedDuration>, D::Error> {
    let v = <Value as serde::Deserialize>::deserialize(d)?;
    if v.is_null() {
        return Ok(None);
    }
    from_value(&v).map(Some).map_err(serde::de::Error::custom)
}
