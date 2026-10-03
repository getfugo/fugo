//! URLs and paths: `parse_url`, `join_url`, `urldecode`, the `path_*` filters and `path_join`.

use ssg_base::paths;
use ssg_base::url::{self, Component, UrlRef};
use tera::{Kwargs, TeraResult, Value};

use super::Registrar;
use super::value::{array, chain, sorted_map, text};

pub(super) fn register(r: &mut Registrar<'_>) {
    r.filter("parse_url", |v, _, _| parse_url(&text(&v, "parse_url")?));
    r.function("join_url", |kw, _| join_url(kw));
    r.filter("urldecode", |v, _, _| {
        let s = text(&v, "urldecode")?;
        let bytes = url::unescape(&s, Component::QueryComponent)
            .map_err(|e| chain(format!("urldecode `{s}`"), e))?;
        String::from_utf8(bytes)
            .map(Value::from)
            .map_err(|_| tera::Error::message(format!("urldecode: `{s}` is not UTF-8")))
    });
    r.filter("path_ext", |v, _, _| {
        Ok(Value::from(paths::ext(&text(&v, "path_ext")?)))
    });
    r.filter("path_base", |v, _, _| {
        Ok(Value::from(paths::base(&text(&v, "path_base")?)))
    });
    r.filter("path_base_name", |v, _, _| {
        let s = text(&v, "path_base_name")?;
        let base = paths::base(&s);
        Ok(Value::from(
            base.strip_suffix(paths::ext(&s)).unwrap_or(base),
        ))
    });
    r.filter("path_dir", |v, _, _| {
        Ok(Value::from(paths::parent(&text(&v, "path_dir")?)))
    });
    r.filter("path_clean", |v, _, _| {
        Ok(Value::from(paths::clean(&text(&v, "path_clean")?)))
    });
    r.function("path_join", |kw, _| {
        let parts = string_parts(kw, "path_join")?;
        let refs: Vec<&str> = parts.iter().map(String::as_str).collect();
        Ok(Value::from(paths::join(&refs)))
    });
}

/// `url.QueryEscape`: every reserved character escaped, space as `+`.
pub(super) fn query_escape(s: &str) -> String {
    url::escape(s.as_bytes(), Component::QueryComponent).into_owned()
}

fn string_parts(kw: &Kwargs, what: &str) -> TeraResult<Vec<String>> {
    let parts = kw.must_get::<Value>("parts")?;
    array(&parts, what)?
        .iter()
        .map(|p| text(p, what).map(std::borrow::Cow::into_owned))
        .collect()
}

fn parse_error(s: &str, e: url::UrlError) -> tera::Error {
    chain(format!("`{s}` is not a URL"), e)
}

/// `{scheme, host, path, fragment, query, is_absolute, string}` (path and fragment decoded).
fn parse_url(s: &str) -> TeraResult<Value> {
    let u = UrlRef::parse(s).map_err(|e| parse_error(s, e))?;
    let lossy = |b: &[u8]| Value::from(String::from_utf8_lossy(b).into_owned());
    Ok(sorted_map([
        ("scheme", Value::from(u.scheme())),
        ("host", lossy(u.host())),
        ("path", lossy(u.path())),
        ("fragment", lossy(u.fragment())),
        ("query", Value::from(u.raw_query())),
        ("is_absolute", Value::from(u.is_absolute())),
        ("string", Value::from(u.to_string())),
    ]))
}

/// `url.JoinPath`: the path elements joined onto the first URL with single slashes (a trailing
/// slash of the last element is kept; a relative URL stays relative; an opaque URL such as
/// `mailto:` is returned as it is).
fn join_url(kw: &Kwargs) -> TeraResult<Value> {
    let parts = string_parts(kw, "join_url")?;
    let Some((first, rest)) = parts.split_first() else {
        return Ok(Value::from(""));
    };
    let mut u = UrlRef::parse(first).map_err(|e| parse_error(first, e))?;
    if !u.opaque().is_empty() {
        // `mailto:…`, `data:…`: no path to join onto
        return Ok(Value::from(first.as_str()));
    }
    let base_path = u.escaped_path().into_owned();
    let relative = !base_path.starts_with('/');
    let rooted = if relative {
        format!("/{base_path}")
    } else {
        base_path.clone()
    };
    let mut elems: Vec<&str> = vec![&rooted];
    elems.extend(rest.iter().map(String::as_str));
    let joined = paths::join(&elems);
    let joined = if relative {
        joined
            .strip_prefix('/')
            .map(str::to_owned)
            .unwrap_or_default()
    } else {
        joined
    };
    let last = rest.last().unwrap_or(&rooted);
    let mut joined = if joined.is_empty() && !relative {
        "/".to_owned()
    } else {
        joined
    };
    if last.ends_with('/') && !joined.ends_with('/') {
        joined.push('/');
    }
    if u.has_host() && !joined.starts_with('/') {
        joined.insert(0, '/');
    }
    // the elements are path text: `?` and `#` in them are escaped when written back
    let decoded = url::unescape(&joined, Component::Path).map_err(|e| parse_error(&joined, e))?;
    u.set_path(decoded);
    Ok(Value::from(u.to_string()))
}
