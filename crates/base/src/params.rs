//! Case-folded params: front matter, configuration and language params.

use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::text;
use crate::value::{Map, Value};

/// A [`Map`] whose keys are lower case at every map level (but not inside arrays: the maps of
/// `ingredients_percentage: [{Name: …}]` keep their keys). Lookups ignore case.
///
/// When folding makes two keys equal (`Title` and `title`), a key that was not already lower
/// case wins over one that was, and among several the last in byte order wins.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Params(Map);

fn fold_map(m: &Map) -> Map {
    let mut out = Map::new();
    // Keys that are already lower case first, so a folded key overrides them.
    let (lower, other): (Vec<_>, Vec<_>) =
        m.entries().partition(|(k, _)| text::to_lower(k) == ***k);
    for (k, v) in lower {
        out.insert(Arc::clone(k), fold_value(v));
    }
    for (k, v) in other {
        out.insert(text::to_lower(k), fold_value(v));
    }
    out
}

fn fold_value(v: &Value) -> Value {
    match v {
        Value::Map(m) => Value::map(fold_map(m)),
        other => other.clone(),
    }
}

impl Params {
    /// Folds the keys of `m` to lower case recursively (not inside arrays).
    #[must_use]
    pub fn fold(m: &Map) -> Self {
        Self(fold_map(m))
    }

    /// The value of `key`, ignoring case.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(&text::to_lower(key))
    }

    /// The value of a dotted key (`author.name`), ignoring case: the whole key first, then
    /// the nested lookup through maps.
    #[must_use]
    pub fn get_path(&self, dotted: &str) -> Option<&Value> {
        let key = text::to_lower(dotted);
        if let Some(v) = self.0.get(&key) {
            return Some(v);
        }
        let mut segments = key.split('.');
        let mut current = self.0.get(segments.next()?)?;
        for seg in segments {
            current = current.as_map()?.get(seg)?;
        }
        Some(current)
    }

    /// Sets `key` (folded) to `value` (folded).
    pub fn insert(&mut self, key: &str, value: &Value) {
        self.0.insert(text::to_lower(key), fold_value(value));
    }

    /// Removes `key`, ignoring case.
    pub fn remove(&mut self, key: &str) -> Option<Value> {
        self.0.remove(&text::to_lower(key))
    }

    /// Adds the keys of `other` that are missing here, recursing where both sides hold a map
    /// (cascade, language fallback).
    pub fn fill_missing_from(&mut self, other: &Self) {
        fill_missing(&mut self.0, &other.0);
    }

    /// Overwrites with the values of `over`, recursing where both sides hold a map (a
    /// `params:` sub-map over the top level).
    pub fn merge_deep(&mut self, over: &Self) {
        merge_deep(&mut self.0, &over.0);
    }

    /// The folded map.
    #[must_use]
    pub fn as_map(&self) -> &Map {
        &self.0
    }

    /// The folded map, owned.
    #[must_use]
    pub fn into_map(self) -> Map {
        self.0
    }

    /// Whether there are no params.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The number of top-level params.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// The entries in byte order.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (&str, &Value)> + ExactSizeIterator {
        self.0.iter()
    }

    /// The template value.
    #[must_use]
    pub fn to_tera(&self) -> tera::Value {
        Value::map(self.0.clone()).to_tera()
    }
}

impl Serialize for Params {
    /// The folded map: lower-case keys in byte order.
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}

impl<'de> Deserialize<'de> for Params {
    /// A table, folded like [`Params::fold`].
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Map::deserialize(d).map(|m| Self::fold(&m))
    }
}

fn fill_missing(dst: &mut Map, src: &Map) {
    for (k, v) in src.entries() {
        match dst.get_mut(k) {
            None => {
                dst.insert(Arc::clone(k), v.clone());
            }
            Some(Value::Map(d)) => {
                if let Value::Map(s) = v {
                    fill_missing(Arc::make_mut(d), s);
                }
            }
            Some(_) => {}
        }
    }
}

fn merge_deep(dst: &mut Map, src: &Map) {
    for (k, v) in src.entries() {
        match (dst.get_mut(k), v) {
            (Some(Value::Map(d)), Value::Map(s)) => merge_deep(Arc::make_mut(d), s),
            _ => {
                dst.insert(Arc::clone(k), v.clone());
            }
        }
    }
}
