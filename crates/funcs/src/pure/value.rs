//! Value helpers shared by the pure functions.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Arc;

use ssg_base::Collate;
use tera::value::Key;
use tera::{Map, TeraResult, Value};

/// The text of a scalar: strings as they are, numbers and booleans printed, none and undefined
/// empty. Arrays and maps are an error naming `what`.
pub fn text<'v>(v: &'v Value, what: &str) -> TeraResult<Cow<'v, str>> {
    if let Some(s) = v.as_str() {
        return Ok(Cow::Borrowed(s));
    }
    if v.is_none() || v.is_undefined() {
        return Ok(Cow::Borrowed(""));
    }
    if v.is_f64() {
        // shortest round-trip digits, no exponent (`1.5`, `1`, `-0`)
        return Ok(Cow::Owned(format!("{}", v.as_f64().unwrap_or_default())));
    }
    if v.is_bool() || v.is_number() {
        return Ok(Cow::Owned(v.to_string()));
    }
    Err(tera::Error::message(format!(
        "{what} expects a string, got {}",
        v.name()
    )))
}

/// An error with a cause: the cause's text is in the message (Tera prints only the message,
/// and drops the cause when it reports a function's error).
pub fn chain<E>(what: impl std::fmt::Display, e: E) -> tera::Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    tera::Error::chain(format!("{what}: {e}"), e)
}

/// A string value with the safety of `like`.
pub fn same_safety(like: &Value, s: String) -> Value {
    if like.is_safe() {
        Value::safe_string(&s)
    } else {
        Value::from(s)
    }
}

/// A map with its keys in byte order (every map a pure function builds is sorted).
pub fn sorted_map<K: AsRef<str>>(entries: impl IntoIterator<Item = (K, Value)>) -> Value {
    let sorted: BTreeMap<String, Value> = entries
        .into_iter()
        .map(|(k, v)| (k.as_ref().to_owned(), v))
        .collect();
    let mut map = Map::with_capacity(sorted.len());
    for (k, v) in sorted {
        map.insert(Key::String(Arc::from(k)), v);
    }
    Value::from(map)
}

/// The string keys of a map with their values.
pub fn entries(m: &Map) -> impl Iterator<Item = (Cow<'_, str>, &Value)> {
    m.iter().map(|(k, v)| {
        let key = k
            .as_str()
            .map_or_else(|| Cow::Owned(k.as_value().to_string()), Cow::Borrowed);
        (key, v)
    })
}

/// The entry `key` of a map.
pub fn get<'m>(m: &'m Map, key: &str) -> Option<&'m Value> {
    m.get_index_of(&Key::Str(key)).map(|i| &m[i])
}

/// The list elements of an array value, or an error naming `what`.
pub fn array<'v>(v: &'v Value, what: &str) -> TeraResult<&'v [Value]> {
    v.as_array()
        .ok_or_else(|| tera::Error::message(format!("{what} expects an array, got {}", v.name())))
}

/// The identity of a page or resource value (`id` or `__rid`), which set operations and
/// comparisons use instead of the whole value.
fn identity(v: &Value) -> Option<(&'static str, Value)> {
    let m = v.as_map()?;
    if let Some(rid) = get(m, "__rid") {
        return Some(("__rid", rid.clone()));
    }
    let id = get(m, "id")?;
    get(m, "kind").map(|_| ("id", id.clone()))
}

/// Equality for set operations: pages by `id`, resources by `__rid`, numbers by value across
/// integer and float, everything else structurally.
pub fn same(a: &Value, b: &Value) -> bool {
    match (identity(a), identity(b)) {
        (Some(x), Some(y)) => x == y,
        (None, None) => a == b,
        _ => false,
    }
}

/// A date view (`{rfc3339, unix}`): its Unix seconds.
pub fn date_unix(v: &Value) -> Option<i64> {
    let m = v.as_map()?;
    get(m, "rfc3339")?;
    get(m, "unix")?.as_i64()
}

/// Walks a dotted path (`params.weight`) through maps; array steps take integer indices.
/// A missing last step is `Ok(None)`; like a Tera lookup, a missing earlier step, or a step into
/// something that is neither a map nor an array, is an error (the offending segment).
pub fn lookup<'v, 'p>(v: &'v Value, path: &'p str) -> Result<Option<&'v Value>, &'p str> {
    let segments: Vec<&str> = path.split('.').filter(|s| !s.is_empty()).collect();
    let mut cur = v;
    for (i, seg) in segments.iter().enumerate() {
        if !cur.is_map() && !cur.is_array() {
            return Err(seg);
        }
        match step(cur, seg) {
            Some(next) => cur = next,
            None if i + 1 == segments.len() => return Ok(None),
            None => return Err(seg),
        }
    }
    Ok(Some(cur))
}

/// One step of [`lookup`]: a map key (then its lower-cased form: params keys are lower case), or
/// an array index.
pub fn step<'v>(v: &'v Value, seg: &str) -> Option<&'v Value> {
    if let Some(m) = v.as_map() {
        return get(m, seg).or_else(|| get(m, &seg.to_lowercase()));
    }
    let idx: usize = seg.parse().ok()?;
    v.as_array()?.get(idx)
}

/// A sort key as Go compares it: text, or a number (dates by instant, booleans as 0/1, and
/// missing values, maps and arrays as 0).
enum SortKey<'v> {
    Text(&'v str),
    Number(f64),
}

fn sort_key(v: Option<&Value>) -> SortKey<'_> {
    let Some(v) = v else {
        return SortKey::Number(0.0);
    };
    if let Some(unix) = date_unix(v) {
        #[allow(clippy::cast_precision_loss)]
        return SortKey::Number(unix as f64);
    }
    if let Some(b) = v.as_bool() {
        return SortKey::Number(if b { 1.0 } else { 0.0 });
    }
    if let Some(s) = v.as_str() {
        return match s.trim().parse::<f64>() {
            Ok(f) if f.is_finite() && !s.trim().is_empty() => SortKey::Number(f),
            _ => SortKey::Text(s),
        };
    }
    SortKey::Number(v.as_f64().filter(|_| v.is_number()).unwrap_or(0.0))
}

/// Go's ordering of two sort keys: two texts by the language's collation; otherwise
/// numerically, where numeric strings are numbers, dates their instant, and other texts,
/// missing values and none count as 0 (so they keep their place: the sort is stable).
pub fn compare(a: Option<&Value>, b: Option<&Value>, collate: &dyn Collate) -> Ordering {
    match (sort_key(a), sort_key(b)) {
        (SortKey::Text(x), SortKey::Text(y)) => collate.compare(x, y),
        (x, y) => {
            let n = |k: SortKey<'_>| match k {
                SortKey::Number(f) => f,
                SortKey::Text(_) => 0.0,
            };
            n(x).partial_cmp(&n(y)).unwrap_or(Ordering::Equal)
        }
    }
}
