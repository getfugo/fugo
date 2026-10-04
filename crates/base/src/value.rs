//! Untyped data: front matter, configuration, data files, params and markup attributes.
//!
//! [`Value`] is cheap to clone (strings, arrays and maps are behind `Arc`). [`Map`] keeps its
//! keys exactly as written and iterates them in byte order (upper case sorts first), which is
//! the order templates range over maps in. The case-folded variant used for front matter and
//! configuration params is [`Params`](crate::Params).

use std::borrow::Borrow;
use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};

mod serialize;

/// A date from a data format that has dates (TOML).
#[derive(Clone, Debug, PartialEq)]
pub enum Date {
    /// A date-time with a UTC offset.
    Zoned(jiff::Zoned),
    /// A local date or date-time; the language's time zone is applied later (a local date is
    /// midnight).
    Local(jiff::civil::DateTime),
}

impl Date {
    /// The date in time zone `tz` (a local date is interpreted in it).
    ///
    /// # Errors
    /// When the local date-time does not exist in `tz` and cannot be disambiguated.
    pub fn in_tz(&self, tz: &jiff::tz::TimeZone) -> Result<jiff::Zoned, jiff::Error> {
        match self {
            Self::Zoned(z) => Ok(z.with_time_zone(tz.clone())),
            Self::Local(dt) => dt.to_zoned(tz.clone()),
        }
    }
}

impl fmt::Display for Date {
    /// RFC 3339 (`2024-07-14T17:31:59+07:00`; a local date-time has no offset).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Zoned(z) => write!(f, "{}", z.strftime("%Y-%m-%dT%H:%M:%S%.f%:z")),
            Self::Local(dt) => write!(f, "{}", dt.strftime("%Y-%m-%dT%H:%M:%S%.f")),
        }
    }
}

/// An untyped value.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum Value {
    #[default]
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(Arc<str>),
    Date(Date),
    Array(Arc<Vec<Value>>),
    Map(Arc<Map>),
}

/// A case-preserving map, iterated in byte order of its keys.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Map(BTreeMap<Arc<str>, Value>);

impl Map {
    /// An empty map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The value of `key` (exact match).
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }

    /// The value of `key`, mutably.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        self.0.get_mut(key)
    }

    /// Whether `key` is present.
    #[must_use]
    pub fn contains_key(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    /// Sets `key`, returning the previous value.
    pub fn insert(&mut self, key: impl Into<Arc<str>>, value: Value) -> Option<Value> {
        self.0.insert(key.into(), value)
    }

    /// Removes `key`.
    pub fn remove(&mut self, key: &str) -> Option<Value> {
        self.0.remove(key)
    }

    /// The number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether the map is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The entries in byte order of the keys.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (&str, &Value)> + ExactSizeIterator {
        self.0.iter().map(|(k, v)| (k.as_ref(), v))
    }

    /// The entries, values mutably.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&str, &mut Value)> {
        self.0.iter_mut().map(|(k, v)| (k.as_ref(), v))
    }

    /// The keys in byte order.
    pub fn keys(&self) -> impl DoubleEndedIterator<Item = &str> + ExactSizeIterator {
        self.0.keys().map(Borrow::borrow)
    }

    /// The values in key order.
    pub fn values(&self) -> impl DoubleEndedIterator<Item = &Value> + ExactSizeIterator {
        self.0.values()
    }

    /// The entries with shared keys (for building other maps without copying keys).
    pub(crate) fn entries(&self) -> impl Iterator<Item = (&Arc<str>, &Value)> {
        self.0.iter()
    }
}

impl std::ops::Index<&str> for Map {
    type Output = Value;

    /// The value of `key`.
    ///
    /// # Panics
    /// When `key` is missing.
    fn index(&self, key: &str) -> &Value {
        self.get(key)
            .unwrap_or_else(|| panic!("no key {key:?} in map"))
    }
}

impl<K: Into<Arc<str>>> FromIterator<(K, Value)> for Map {
    fn from_iter<I: IntoIterator<Item = (K, Value)>>(iter: I) -> Self {
        Self(iter.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }
}

impl IntoIterator for Map {
    type Item = (Arc<str>, Value);
    type IntoIter = std::collections::btree_map::IntoIter<Arc<str>, Value>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// A data file or front matter that could not be decoded.
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Toml(#[from] toml::de::Error),
    #[error(transparent)]
    Yaml(#[from] serde_saphyr::Error),
}

impl Value {
    /// A string value.
    #[must_use]
    pub fn string(s: &str) -> Self {
        Self::String(s.into())
    }

    /// An array value.
    #[must_use]
    pub fn array(items: Vec<Self>) -> Self {
        Self::Array(Arc::new(items))
    }

    /// A map value.
    #[must_use]
    pub fn map(m: Map) -> Self {
        Self::Map(Arc::new(m))
    }

    /// The string, if this is one.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }

    /// The map, if this is one.
    #[must_use]
    pub fn as_map(&self) -> Option<&Map> {
        match self {
            Self::Map(m) => Some(m),
            _ => None,
        }
    }

    /// The array, if this is one.
    #[must_use]
    pub fn as_array(&self) -> Option<&[Self]> {
        match self {
            Self::Array(a) => Some(a),
            _ => None,
        }
    }

    /// The boolean, if this is one.
    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// The integer, if this is one.
    #[must_use]
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// The number as a float (integers converted).
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        reason = "template numbers are f64 anyway"
    )]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Int(i) => Some(*i as f64),
            Self::Float(f) => Some(*f),
            _ => None,
        }
    }

    /// Whether this is [`Value::Null`].
    #[must_use]
    pub fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Decodes JSON.
    ///
    /// # Errors
    /// Invalid JSON.
    pub fn from_json_str(s: &str) -> Result<Self, DecodeError> {
        Ok(serde_json::from_str(s)?)
    }

    /// Converts a JSON value (integers that do not fit `i64` become floats).
    #[must_use]
    pub fn from_json(v: serde_json::Value) -> Self {
        match v {
            serde_json::Value::Null => Self::Null,
            serde_json::Value::Bool(b) => Self::Bool(b),
            serde_json::Value::Number(n) => n
                .as_i64()
                .map_or_else(|| Self::Float(n.as_f64().unwrap_or(f64::NAN)), Self::Int),
            serde_json::Value::String(s) => Self::String(s.into()),
            serde_json::Value::Array(a) => {
                Self::array(a.into_iter().map(Self::from_json).collect())
            }
            serde_json::Value::Object(o) => Self::map(
                o.into_iter()
                    .map(|(k, v)| (k, Self::from_json(v)))
                    .collect(),
            ),
        }
    }

    /// Decodes TOML (a document is a table).
    ///
    /// # Errors
    /// Invalid TOML.
    pub fn from_toml_str(s: &str) -> Result<Self, DecodeError> {
        let table: toml::Table = toml::from_str(s)?;
        Ok(Self::from_toml(toml::Value::Table(table)))
    }

    /// Converts a TOML value. Offset date-times become [`Date::Zoned`], local dates and
    /// date-times [`Date::Local`]; a local time without a date stays a string.
    #[must_use]
    pub fn from_toml(v: toml::Value) -> Self {
        match v {
            toml::Value::String(s) => Self::String(s.into()),
            toml::Value::Integer(i) => Self::Int(i),
            toml::Value::Float(f) => Self::Float(f),
            toml::Value::Boolean(b) => Self::Bool(b),
            toml::Value::Datetime(d) => toml_date(&d),
            toml::Value::Array(a) => Self::array(a.into_iter().map(Self::from_toml).collect()),
            toml::Value::Table(t) => Self::map(
                t.into_iter()
                    .map(|(k, v)| (k, Self::from_toml(v)))
                    .collect(),
            ),
        }
    }

    /// Decodes YAML (YAML 1.2 scalars; timestamps stay strings; non-string keys are written
    /// as strings).
    ///
    /// # Errors
    /// Invalid YAML.
    pub fn from_yaml_str(s: &str) -> Result<Self, DecodeError> {
        // `.inf`/`.nan` would otherwise fail the whole document: an untyped value cannot hold a
        // non-finite float in serde-saphyr, so keep them as their canonical YAML strings.
        let mut options = serde_saphyr::Options::default();
        options.reject_non_finite_typeless_float = false;
        Ok(serde_saphyr::from_str_with_options(s, options)?)
    }

    /// The template value: maps keep key case and byte order, dates are RFC 3339 strings.
    #[must_use]
    pub fn to_tera(&self) -> tera::Value {
        match self {
            Self::Null => tera::Value::none(),
            Self::Bool(b) => tera::Value::from(*b),
            Self::Int(i) => tera::Value::from(*i),
            Self::Float(f) => tera::Value::from(*f),
            Self::String(s) => tera::Value::from(&**s),
            Self::Date(d) => tera::Value::from(d.to_string()),
            Self::Array(a) => tera::Value::from(a.iter().map(Self::to_tera).collect::<Vec<_>>()),
            Self::Map(m) => tera::Value::from(
                m.entries()
                    .map(|(k, v)| (tera::value::Key::String(Arc::clone(k)), v.to_tera()))
                    .collect::<BTreeMap<_, _>>(),
            ),
        }
    }
}

fn toml_date(d: &toml::value::Datetime) -> Value {
    let Some(date) = d.date else {
        return Value::String(d.to_string().into());
    };
    let (hour, minute, second, nanosecond) = d.time.map_or((0, 0, 0, 0), |t| {
        (
            t.hour,
            t.minute,
            t.second.unwrap_or(0),
            t.nanosecond.unwrap_or(0),
        )
    });
    let to_i8 = |n: u8| i8::try_from(n).unwrap_or(i8::MAX);
    let dt = jiff::civil::DateTime::new(
        i16::try_from(date.year).unwrap_or(i16::MAX),
        to_i8(date.month),
        to_i8(date.day),
        to_i8(hour),
        to_i8(minute),
        to_i8(second),
        i32::try_from(nanosecond).unwrap_or(0),
    );
    let Ok(dt) = dt else {
        return Value::String(d.to_string().into());
    };
    match d.offset {
        None => Value::Date(Date::Local(dt)),
        Some(offset) => {
            let seconds = match offset {
                toml::value::Offset::Z => 0,
                toml::value::Offset::Custom { minutes } => i32::from(minutes) * 60,
            };
            jiff::tz::Offset::from_seconds(seconds)
                .ok()
                .and_then(|o| dt.to_zoned(jiff::tz::TimeZone::fixed(o)).ok())
                .map_or_else(
                    || Value::String(d.to_string().into()),
                    |z| Value::Date(Date::Zoned(z)),
                )
        }
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Self::Bool(b)
    }
}

impl From<i64> for Value {
    fn from(i: i64) -> Self {
        Self::Int(i)
    }
}

impl From<f64> for Value {
    fn from(f: f64) -> Self {
        Self::Float(f)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Self::String(s.into())
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Self::String(s.into())
    }
}

impl From<Map> for Value {
    fn from(m: Map) -> Self {
        Self::map(m)
    }
}

impl From<Vec<Value>> for Value {
    fn from(a: Vec<Value>) -> Self {
        Self::array(a)
    }
}
