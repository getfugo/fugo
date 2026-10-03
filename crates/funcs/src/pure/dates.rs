//! Dates and numbers: `now`, `to_date`, `date`, `format_number`.
//!
//! A date value is a map `{rfc3339, unix}` (the shape of the views' `DateView`): templates
//! compare instants with `.unix` and format with `date`.

use std::sync::Arc;

use jiff::Zoned;
use jiff::tz::TimeZone;
use ssg_base::parse_date;
use ssg_locale::{DatePattern, DateStyle, format_date, format_number};
use tera::value::Key;
use tera::{Kwargs, State, TeraResult, Value};

use super::value::{chain, sorted_map, text};
use super::{PureEnv, Registrar};

pub(super) fn register(r: &mut Registrar<'_>, env: &Arc<PureEnv>) {
    let e = Arc::clone(env);
    r.function("now", move |_, _| {
        Ok(date_value(&e.clock.now().to_zoned(e.time_zone.clone())))
    });
    let e = Arc::clone(env);
    r.filter("to_date", move |v, _, _| {
        Ok(date_value(&to_zoned(&v, &e.time_zone)?))
    });
    let e = Arc::clone(env);
    r.filter("date", move |v, kw, st| date(&v, kw, st, &e));
    let e = Arc::clone(env);
    r.filter("format_number", move |v, kw, st| {
        let precision = kw.get::<u8>("precision")?.unwrap_or(0);
        let n = number(&v)?;
        Ok(Value::from(format_number(
            n,
            precision,
            &e.locales.current(st),
        )))
    });
}

/// The template value of a date: `{rfc3339, unix}`, RFC 3339 in the date's own offset.
#[must_use]
pub fn date_value(d: &Zoned) -> Value {
    let rfc3339 = if d.timestamp().subsec_nanosecond() == 0 {
        d.strftime("%Y-%m-%dT%H:%M:%S%:z").to_string()
    } else {
        d.strftime("%Y-%m-%dT%H:%M:%S%.f%:z").to_string()
    };
    sorted_map([
        ("rfc3339", Value::from(rfc3339)),
        ("unix", Value::from(d.timestamp().as_second())),
    ])
}

fn number(v: &Value) -> TeraResult<f64> {
    if let Some(f) = v.as_f64() {
        return Ok(f);
    }
    let s = text(v, "format_number")?;
    s.trim()
        .parse::<f64>()
        .map_err(|_| tera::Error::message(format!("format_number expects a number, got `{s}`")))
}

/// A date from a date value, a date string (parsed in `tz` when it has no offset) or Unix
/// seconds.
fn to_zoned(v: &Value, tz: &TimeZone) -> TeraResult<Zoned> {
    if let Some(m) = v.as_map() {
        if let Some(s) = m.get(&Key::Str("rfc3339")).and_then(Value::as_str) {
            return parse(s, tz);
        }
        return Err(tera::Error::message("a date map needs an `rfc3339` field"));
    }
    if let Some(secs) = v.as_i64() {
        let ts = jiff::Timestamp::from_second(secs)
            .map_err(|e| chain(format!("{secs} is not a Unix time"), e))?;
        return Ok(ts.to_zoned(tz.clone()));
    }
    if let Some(s) = v.as_str() {
        return parse(s, tz);
    }
    Err(tera::Error::message(format!(
        "expected a date, a date string or Unix seconds, got {}",
        v.name()
    )))
}

fn parse(s: &str, tz: &TimeZone) -> TeraResult<Zoned> {
    parse_date(s.trim(), tz).map_err(|e| chain(format!("`{s}` is not a date"), e))
}

/// `date(format=)` (strftime) or `date(style=)` (`short`, `medium`, `long`, `full`). A style is
/// always localized, in `locale` or the render's language; the names of a strftime `format`
/// (`%B %b %h %A %a`) are English, as Go's `Time.Format`, unless `locale` is given (as the Go
/// templates' `time.Format`). A none input prints nothing (zero dates are none).
fn date(v: &Value, kw: &Kwargs, st: &State, env: &PureEnv) -> TeraResult<Value> {
    if v.is_none() || v.is_undefined() {
        return Ok(Value::from(""));
    }
    let d = to_zoned(v, &env.time_zone)?;
    let locale = kw.get::<&str>("locale")?;
    let pattern = match (kw.get::<&str>("format")?, kw.get::<&str>("style")?) {
        (Some(f), None) if locale.is_none() => {
            return jiff::fmt::strtime::format(f.as_bytes(), &d)
                .map(Value::from)
                .map_err(|e| chain(format!("date: bad strftime format `{f}`"), e));
        }
        (Some(f), None) => DatePattern::Strftime(f),
        (None, Some(s)) => DatePattern::Style(style(s)?),
        (Some(_), Some(_)) => {
            return Err(tera::Error::message(
                "date takes `format` or `style`, not both",
            ));
        }
        (None, None) => return Err(tera::Error::message("date needs `format` or `style`")),
    };
    let locale = match locale {
        Some(key) => env.locales.get(key),
        None => env.locales.current(st),
    };
    format_date(&d, pattern, &locale)
        .map(Value::from)
        .map_err(|e| chain("date", e))
}

fn style(s: &str) -> TeraResult<DateStyle> {
    match s.to_ascii_lowercase().as_str() {
        "short" => Ok(DateStyle::Short),
        "medium" => Ok(DateStyle::Medium),
        "long" => Ok(DateStyle::Long),
        "full" => Ok(DateStyle::Full),
        other => Err(tera::Error::message(format!(
            "date(style=): unknown style `{other}`; expected short, medium, long or full"
        ))),
    }
}
