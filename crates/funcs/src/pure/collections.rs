//! Collections and maps: `default_if_empty`, `get_path`, `sort_keys`, `from_pairs`, `append`,
//! `concat`, `merge`, `delimit`, the set operations, `sort_by`, `querify`, `max`, `min`.

use std::cmp::Ordering;
use std::sync::Arc;

use tera::{Kwargs, State, TeraResult, Value};

use super::value::{self, array, compare, entries, same, sorted_map, text};
use super::{PureEnv, Registrar};

pub(super) fn register(r: &mut Registrar<'_>, env: &Arc<PureEnv>) {
    r.filter("default_if_empty", |v, kw, _| default_if_empty(v, kw));
    r.filter("get_path", |v, kw, _| get_path(&v, kw));
    r.filter("sort_keys", |v, _, _| sort_keys(&v));
    r.filter("from_pairs", |v, _, _| from_pairs(&v));
    r.filter("append", |v, kw, _| append(&v, kw));
    r.filter("concat", |v, kw, _| concat(&v, kw));
    r.filter("merge", |v, kw, _| merge_filter(&v, kw));
    r.filter("delimit", |v, kw, _| delimit(&v, kw));
    r.filter("complement", |v, kw, _| set_op(&v, kw, SetOp::Complement));
    r.filter("union", |v, kw, _| set_op(&v, kw, SetOp::Union));
    r.filter("intersect", |v, kw, _| set_op(&v, kw, SetOp::Intersect));
    r.filter("symdiff", |v, kw, _| set_op(&v, kw, SetOp::SymDiff));
    let env = Arc::clone(env);
    r.filter("sort_by", move |v, kw, st| sort_by(&v, kw, st, &env));
    r.function("querify", |kw, _| querify(kw));
    r.function("max", |kw, _| extreme(kw, Ordering::Greater));
    r.function("min", |kw, _| extreme(kw, Ordering::Less));
}

/// Whether Go's `default` treats `v` as unset: undefined, none, zero numbers, empty strings,
/// arrays and maps, and zero dates. `false` is set.
pub(super) fn is_empty(v: &Value) -> bool {
    if v.is_undefined() || v.is_none() {
        return true;
    }
    if let Some(s) = v.as_str() {
        return s.is_empty();
    }
    if let Some(f) = v.as_f64()
        && v.is_number()
    {
        return f == 0.0;
    }
    if let Some(a) = v.as_array() {
        return a.is_empty();
    }
    if let Some(m) = v.as_map() {
        return m.is_empty();
    }
    false
}

fn default_if_empty(v: Value, kw: &Kwargs) -> TeraResult<Value> {
    let fallback = kw.must_get::<Value>("value")?;
    Ok(if is_empty(&v) { fallback } else { v })
}

fn get_path(v: &Value, kw: &Kwargs) -> TeraResult<Value> {
    let path = kw.must_get::<Value>("path")?;
    let steps = array(&path, "get_path(path=)")?;
    let mut cur = v;
    for s in steps {
        let seg = text(s, "get_path(path=)")?;
        match value::step(cur, &seg) {
            Some(next) => cur = next,
            None => return Ok(Value::none()),
        }
    }
    Ok(cur.clone())
}

fn sort_keys(v: &Value) -> TeraResult<Value> {
    let m = v.as_map().ok_or_else(|| {
        tera::Error::message(format!("sort_keys expects a map, got {}", v.name()))
    })?;
    Ok(sorted_map(entries(m).map(|(k, v)| (k, v.clone()))))
}

/// A map from `[key, value]` pairs (the inverse of Tera's `pairs`): keys are strings (numbers
/// and bools are stringified), a later pair wins, the keys are sorted.
fn from_pairs(v: &Value) -> TeraResult<Value> {
    let mut out = Vec::new();
    if v.is_none() || v.is_undefined() {
        return Ok(sorted_map::<&str>([]));
    }
    for p in array(v, "from_pairs")? {
        let pair = array(p, "from_pairs (each pair)")?;
        let [k, v] = pair else {
            return Err(tera::Error::message(format!(
                "from_pairs expects [key, value] pairs, got an array of {}",
                pair.len()
            )));
        };
        out.push((text(k, "from_pairs (a key)")?.into_owned(), v.clone()));
    }
    Ok(sorted_map(out))
}

fn append(v: &Value, kw: &Kwargs) -> TeraResult<Value> {
    let mut items = if v.is_none() || v.is_undefined() {
        Vec::new()
    } else {
        array(v, "append")?.to_vec()
    };
    items.push(kw.must_get::<Value>("value")?);
    Ok(Value::from(items))
}

fn concat(v: &Value, kw: &Kwargs) -> TeraResult<Value> {
    let with = kw.must_get::<Value>("with")?;
    let mut items = if v.is_none() || v.is_undefined() {
        Vec::new()
    } else {
        array(v, "concat")?.to_vec()
    };
    if !with.is_none() {
        items.extend_from_slice(array(&with, "concat(with=)")?);
    }
    Ok(Value::from(items))
}

fn merge_filter(v: &Value, kw: &Kwargs) -> TeraResult<Value> {
    let with = kw.must_get::<Value>("with")?;
    let as_map = |x: &Value, what: &str| -> TeraResult<Option<Value>> {
        if x.is_none() || x.is_undefined() {
            Ok(None)
        } else if x.is_map() {
            Ok(Some(x.clone()))
        } else {
            Err(tera::Error::message(format!(
                "merge: {what} must be a map, got {}",
                x.name()
            )))
        }
    };
    let base = as_map(v, "the input")?;
    let with = as_map(&with, "`with`")?;
    Ok(match (base, with) {
        (Some(a), Some(b)) => merge(&a, &b),
        (Some(a), None) => sort_deep(&a),
        (None, Some(b)) => sort_deep(&b),
        (None, None) => sorted_map::<&str>([]),
    })
}

/// Deep merge of two maps: keys match ignoring ASCII case, `b`'s key and value win, nested maps
/// merge; the result's keys are sorted.
pub(super) fn merge(a: &Value, b: &Value) -> Value {
    let (Some(am), Some(bm)) = (a.as_map(), b.as_map()) else {
        return b.clone();
    };
    let mut out: Vec<(String, Value)> = Vec::new();
    for (k, v) in entries(am) {
        let other = entries(bm).find(|(bk, _)| bk.eq_ignore_ascii_case(&k));
        match other {
            Some((bk, bv)) => {
                let merged = if v.is_map() && bv.is_map() {
                    merge(v, bv)
                } else {
                    sort_deep(bv)
                };
                out.push((bk.into_owned(), merged));
            }
            None => out.push((k.into_owned(), sort_deep(v))),
        }
    }
    for (bk, bv) in entries(bm) {
        if !out.iter().any(|(k, _)| k.eq_ignore_ascii_case(&bk)) {
            out.push((bk.into_owned(), sort_deep(bv)));
        }
    }
    sorted_map(out)
}

/// `v` with the keys of every nested map sorted.
fn sort_deep(v: &Value) -> Value {
    if let Some(m) = v.as_map() {
        return sorted_map(entries(m).map(|(k, v)| (k, sort_deep(v))));
    }
    if let Some(a) = v.as_array() {
        return Value::from(a.iter().map(sort_deep).collect::<Vec<_>>());
    }
    v.clone()
}

fn delimit(v: &Value, kw: &Kwargs) -> TeraResult<Value> {
    let sep = kw.must_get::<Value>("sep")?;
    let sep = text(&sep, "delimit(sep=)")?;
    let last = kw.get::<Value>("last")?;
    let last = last
        .as_ref()
        .map(|l| text(l, "delimit(last=)"))
        .transpose()?;
    let items: Vec<Value> = if let Some(a) = v.as_array() {
        a.to_vec()
    } else if let Some(m) = v.as_map() {
        // Go ranges maps in key order
        let mut pairs: Vec<_> = entries(m).collect();
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        pairs.into_iter().map(|(_, v)| v.clone()).collect()
    } else {
        return Err(tera::Error::message(format!(
            "delimit expects an array or a map, got {}",
            v.name()
        )));
    };
    let mut out = String::new();
    let n = items.len();
    for (i, item) in items.iter().enumerate() {
        out.push_str(&text(item, "delimit")?);
        if i + 2 == n
            && let Some(last) = &last
        {
            out.push_str(last);
        } else if i + 1 < n {
            out.push_str(&sep);
        }
    }
    Ok(Value::from(out))
}

#[derive(Clone, Copy)]
enum SetOp {
    Complement,
    Union,
    Intersect,
    SymDiff,
}

fn set_op(v: &Value, kw: &Kwargs, op: SetOp) -> TeraResult<Value> {
    let (name, arg) = match op {
        SetOp::Complement => ("complement", "without"),
        SetOp::Union => ("union", "with"),
        SetOp::Intersect => ("intersect", "with"),
        SetOp::SymDiff => ("symdiff", "with"),
    };
    let other = kw.must_get::<Value>(arg)?;
    // union and intersect read none as an empty list; complement and symdiff need arrays
    let lenient = matches!(op, SetOp::Union | SetOp::Intersect);
    let list = |x: &Value, what: &str| -> TeraResult<Vec<Value>> {
        if lenient && (x.is_none() || x.is_undefined()) {
            Ok(Vec::new())
        } else {
            Ok(array(x, what)?.to_vec())
        }
    };
    let a = list(v, name)?;
    let b = list(&other, &format!("{name}({arg}=)"))?;
    let contains = |l: &[Value], x: &Value| l.iter().any(|y| same(x, y));
    let mut out: Vec<Value> = Vec::new();
    let push_unique = |x: &Value, out: &mut Vec<Value>| {
        if !contains(out, x) {
            out.push(x.clone());
        }
    };
    match op {
        SetOp::Complement => a
            .iter()
            .filter(|x| !contains(&b, x))
            .for_each(|x| out.push(x.clone())),
        SetOp::Union => a.iter().chain(&b).for_each(|x| push_unique(x, &mut out)),
        SetOp::Intersect => a
            .iter()
            .filter(|x| contains(&b, x))
            .for_each(|x| push_unique(x, &mut out)),
        SetOp::SymDiff => {
            a.iter()
                .filter(|x| !contains(&b, x))
                .chain(b.iter().filter(|x| !contains(&a, x)))
                .for_each(|x| push_unique(x, &mut out));
        }
    }
    Ok(Value::from(out))
}

/// Go's `sort`: by `attribute` (a dotted path; `""` or `value` sorts by the elements
/// themselves), collated in the render language, dates as instants, stable; a map's values are
/// sorted (by key when there is no attribute).
fn sort_by(v: &Value, kw: &Kwargs, state: &State, env: &PureEnv) -> TeraResult<Value> {
    let attribute = kw.must_get::<Value>("attribute")?;
    let attribute = text(&attribute, "sort_by(attribute=)")?;
    let attribute = attribute.trim_matches('.');
    let reverse = kw.get::<bool>("reverse")?.unwrap_or(false);
    let locale = env.locales.current(state);
    let collator = locale.collator();

    let by_value = attribute.is_empty() || attribute == "value";
    // a missing map key sorts as its zero value; an attribute of something that is not a map
    // is an error
    let key_of = |x: &Value| -> TeraResult<Option<Value>> {
        if !x.is_map() {
            return Err(tera::Error::message(format!(
                "sort_by(attribute=\"{attribute}\"): an element is a {}, not a map",
                x.name()
            )));
        }
        value::lookup(x, attribute).map(Option::<&Value>::cloned).map_err(|seg| {
            tera::Error::message(format!(
                "sort_by(attribute=\"{attribute}\"): `{seg}` is missing, or not a map, in an element"
            ))
        })
    };
    // (key, element)
    let mut pairs: Vec<(Option<Value>, Value)> = if let Some(a) = v.as_array() {
        a.iter()
            .map(|x| {
                let key = if by_value {
                    Some(x.clone())
                } else {
                    key_of(x)?
                };
                Ok((key, x.clone()))
            })
            .collect::<TeraResult<_>>()?
    } else if let Some(m) = v.as_map() {
        entries(m)
            .map(|(k, x)| {
                let key = if attribute.is_empty() {
                    Some(Value::from(k.as_ref()))
                } else if attribute == "value" {
                    Some(x.clone())
                } else {
                    key_of(x)?
                };
                Ok((key, x.clone()))
            })
            .collect::<TeraResult<_>>()?
    } else {
        return Err(tera::Error::message(format!(
            "sort_by expects an array or a map, got {}",
            v.name()
        )));
    };
    pairs.sort_by(|(a, _), (b, _)| {
        let ord = compare(a.as_ref(), b.as_ref(), collator);
        if reverse { ord.reverse() } else { ord }
    });
    Ok(Value::from(
        pairs.into_iter().map(|(_, x)| x).collect::<Vec<_>>(),
    ))
}

/// A URL query string from a map: keys sorted, keys and values query-escaped (`url.Values`).
fn querify(kw: &Kwargs) -> TeraResult<Value> {
    let params = kw.must_get::<Value>("params")?;
    let m = params.as_map().ok_or_else(|| {
        tera::Error::message(format!(
            "querify(params=) expects a map, got {}",
            params.name()
        ))
    })?;
    let mut pairs: Vec<(String, String)> = Vec::new();
    for (k, v) in entries(m) {
        pairs.push((k.into_owned(), text(v, "querify(params=)")?.into_owned()));
    }
    pairs.sort();
    let out: Vec<String> = pairs
        .iter()
        .map(|(k, v)| {
            format!(
                "{}={}",
                super::urls::query_escape(k),
                super::urls::query_escape(v)
            )
        })
        .collect();
    Ok(Value::from(out.join("&")))
}

fn extreme(kw: &Kwargs, want: Ordering) -> TeraResult<Value> {
    let values = kw.must_get::<Value>("values")?;
    let items = array(&values, "values")?;
    let mut best: Option<&Value> = None;
    for item in items {
        if !item.is_number() {
            return Err(tera::Error::message(format!(
                "max/min expect numbers, got {}",
                item.name()
            )));
        }
        if best.is_none_or(|b| item.cmp(b) == want) {
            best = Some(item);
        }
    }
    best.cloned()
        .ok_or_else(|| tera::Error::message("max/min need at least one value"))
}
