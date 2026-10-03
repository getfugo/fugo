//! Filters over view values: resource lists (`get_resource`, `find_resource`,
//! `find_resources`, `by_type`) and shortcode arguments (`arg`).

use ssg_base::glob::{self, GlobOpts};
use tera::{Kwargs, TeraResult, Value};

use super::Registrar;
use super::value::{array, chain, get};

pub(super) fn register(r: &mut Registrar<'_>) {
    r.filter("get_resource", |v, kw, _| {
        let name = kw.must_get::<&str>("name")?;
        Ok(resources(&v, "get_resource")?
            .iter()
            .find(|res| field(res, "name").is_some_and(|n| n.eq_ignore_ascii_case(name)))
            .cloned()
            .unwrap_or_else(Value::none))
    });
    r.filter("find_resource", |v, kw, _| {
        let g = compile(kw.must_get::<&str>("pattern")?)?;
        Ok(resources(&v, "find_resource")?
            .iter()
            .find(|res| field(res, "name").is_some_and(|n| g.is_match(n)))
            .cloned()
            .unwrap_or_else(Value::none))
    });
    r.filter("find_resources", |v, kw, _| {
        let g = compile(kw.must_get::<&str>("pattern")?)?;
        Ok(Value::from(
            resources(&v, "find_resources")?
                .iter()
                .filter(|res| field(res, "name").is_some_and(|n| g.is_match(n)))
                .cloned()
                .collect::<Vec<_>>(),
        ))
    });
    r.filter("by_type", |v, kw, _| {
        let ty = kw.must_get::<&str>("type")?;
        Ok(Value::from(
            resources(&v, "by_type")?
                .iter()
                .filter(|res| field(res, "resource_type").is_some_and(|t| t == ty))
                .cloned()
                .collect::<Vec<_>>(),
        ))
    });
    r.filter("arg", |v, kw, _| arg(&v, kw));
}

fn resources<'v>(v: &'v Value, what: &str) -> TeraResult<&'v [Value]> {
    if v.is_none() || v.is_undefined() {
        return Ok(&[]);
    }
    array(v, what)
}

fn field<'v>(v: &'v Value, name: &str) -> Option<&'v str> {
    get(v.as_map()?, name)?.as_str()
}

fn compile(pattern: &str) -> TeraResult<glob::Glob> {
    glob::compile(pattern, GlobOpts::default())
        .map_err(|e| chain(format!("invalid glob `{pattern}`"), e))
}

/// Go's `.Get` on a shortcode value: by position (`index`) for positional arguments, by
/// `name` for named ones. A missing argument is `default` (else `""`); asking a named shortcode
/// by position or a positional one by name is `default` (else none).
fn arg(v: &Value, kw: &Kwargs) -> TeraResult<Value> {
    let m = v.as_map().ok_or_else(|| {
        tera::Error::message(format!("arg expects a shortcode value, got {}", v.name()))
    })?;
    let named = get(m, "is_named_params")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let default = kw.get::<Value>("default")?;
    let found = match (kw.get::<i64>("index")?, kw.get::<&str>("name")?) {
        (Some(_), Some(_)) => {
            return Err(tera::Error::message(
                "arg takes `index` or `name`, not both",
            ));
        }
        (None, None) => return Err(tera::Error::message("arg needs `index` or `name`")),
        (Some(_), None) if named => Err(()),
        (None, Some(_)) if !named => Err(()),
        (Some(i), None) => Ok(usize::try_from(i).ok().and_then(|i| {
            get(m, "args")
                .and_then(Value::as_array)
                .and_then(|a| a.get(i))
                .cloned()
        })),
        (None, Some(name)) => Ok(get(m, "params")
            .and_then(|p| p.as_map())
            .and_then(|p| get(p, name))
            .cloned()),
    };
    Ok(match found {
        Ok(Some(v)) => v,
        Ok(None) => default.unwrap_or_else(|| Value::from("")),
        Err(()) => default.unwrap_or_else(Value::none),
    })
}
