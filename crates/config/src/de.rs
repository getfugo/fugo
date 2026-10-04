//! A serde [`Deserializer`](serde::Deserializer) over a [`Value`] tree: the typed step of the
//! pipeline.
//!
//! Configuration keys are case-insensitive, so struct fields and enum variants are matched
//! ignoring ASCII case (`baseURL` fills `base_url`, which serde knows as `baseUrl`). Map keys of
//! untyped maps (`Map`, `BTreeMap<String, _>`) are passed through unchanged. Scalars are
//! weakly typed the way configuration authors expect: `"true"` is a boolean, `"12"` a number,
//! `12` a string, and a single value where a list is expected is a one-element list.
//!
//! Errors carry the dotted key path of the offending value, which `load` maps back to a file
//! position.

use std::fmt;

use serde::de::{
    self, DeserializeOwned, DeserializeSeed, EnumAccess, IntoDeserializer, MapAccess, SeqAccess,
    VariantAccess, Visitor,
};
use ssg_base::{Map, Value};

mod value;

use value::*;

/// A value that does not fit its typed field, with the dotted key path from the root of the
/// decoded tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeError {
    /// Key path segments (array indices as decimal strings), outermost first.
    pub path: Vec<String>,
    /// What is wrong.
    pub message: String,
}

impl DeError {
    fn at(mut self, segment: impl Into<String>) -> Self {
        self.path.insert(0, segment.into());
        self
    }

    /// The dotted key path (`markup.goldmark.parser.autoHeadingID`, indices as `[2]`).
    #[must_use]
    pub fn dotted_path(&self) -> String {
        let mut out = String::new();
        for seg in &self.path {
            if seg.bytes().all(|b| b.is_ascii_digit()) && !seg.is_empty() {
                out.push('[');
                out.push_str(seg);
                out.push(']');
            } else {
                if !out.is_empty() {
                    out.push('.');
                }
                out.push_str(seg);
            }
        }
        out
    }
}

impl fmt::Display for DeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            f.write_str(&self.message)
        } else {
            write!(f, "{}: {}", self.dotted_path(), self.message)
        }
    }
}

impl std::error::Error for DeError {}

impl de::Error for DeError {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Self {
            path: Vec::new(),
            message: msg.to_string(),
        }
    }
}

/// Decodes `T` from `v`.
///
/// # Errors
/// When a value does not fit its field.
pub fn from_value<T: DeserializeOwned>(v: &Value) -> Result<T, DeError> {
    T::deserialize(ValueDe(v))
}

/// Decodes `T` from a map.
///
/// # Errors
/// When a value does not fit its field.
pub fn from_map<T: DeserializeOwned>(m: &Map) -> Result<T, DeError> {
    T::deserialize(MapDe(m))
}

fn kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "nothing",
        Value::Bool(_) => "a boolean",
        Value::Int(_) => "an integer",
        Value::Float(_) => "a number",
        Value::String(_) => "a string",
        Value::Date(_) => "a date",
        Value::Array(_) => "a list",
        Value::Map(_) => "a table",
    }
}

fn mismatch(v: &Value, expected: &str) -> DeError {
    de::Error::custom(format_args!("expected {expected}, found {}", kind(v)))
}

/// Parses a boolean the way configuration authors write it.
pub(crate) fn weak_bool(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        Value::Int(i) => Some(*i != 0),
        Value::String(s) => match s.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "t" => Some(true),
            "false" | "0" | "f" | "" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn weak_i64(v: &Value) -> Option<i64> {
    match v {
        Value::Int(i) => Some(*i),
        #[expect(clippy::cast_possible_truncation, reason = "only integral values")]
        Value::Float(f) if f.fract() == 0.0 && f.abs() < 9.0e15 => Some(*f as i64),
        Value::Bool(b) => Some(i64::from(*b)),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn weak_f64(v: &Value) -> Option<f64> {
    match v {
        Value::String(s) => s.trim().parse().ok(),
        other => other.as_f64(),
    }
}

/// The string form of a scalar (`12`, `true`, `1.5`, a date in RFC 3339).
pub(crate) fn weak_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.to_string()),
        Value::Int(i) => Some(i.to_string()),
        Value::Float(f) => Some(f.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Date(d) => Some(d.to_string()),
        _ => None,
    }
}

/// The name in `names` equal to `s` ignoring ASCII case, or `s` itself.
fn fold_name(s: &str, names: &'static [&'static str]) -> String {
    names
        .iter()
        .find(|n| n.eq_ignore_ascii_case(s))
        .map_or_else(|| s.to_owned(), |n| (*n).to_owned())
}

struct MapDe<'a>(&'a Map);

impl<'de> de::Deserializer<'de> for MapDe<'_> {
    type Error = DeError;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        visitor.visit_map(MapAccessDe::new(self.0, None))
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, DeError> {
        visitor.visit_map(MapAccessDe::new(self.0, Some(fields)))
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map enum identifier ignored_any
    }
}

struct SeqDe<'a> {
    items: &'a [Value],
    next: usize,
}

impl<'de> SeqAccess<'de> for SeqDe<'_> {
    type Error = DeError;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, DeError> {
        let Some(v) = self.items.get(self.next) else {
            return Ok(None);
        };
        let i = self.next;
        self.next += 1;
        seed.deserialize(ValueDe(v))
            .map(Some)
            .map_err(|e| e.at(i.to_string()))
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.items.len() - self.next)
    }
}

struct MapAccessDe<'a> {
    entries: Vec<(&'a str, &'a Value)>,
    fields: Option<&'static [&'static str]>,
    next: usize,
}

impl<'a> MapAccessDe<'a> {
    fn new(m: &'a Map, fields: Option<&'static [&'static str]>) -> Self {
        Self {
            entries: m.iter().collect(),
            fields,
            next: 0,
        }
    }

    fn empty(fields: Option<&'static [&'static str]>) -> Self {
        Self {
            entries: Vec::new(),
            fields,
            next: 0,
        }
    }
}

impl<'de> MapAccess<'de> for MapAccessDe<'_> {
    type Error = DeError;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, DeError> {
        let Some((k, _)) = self.entries.get(self.next) else {
            return Ok(None);
        };
        let key = match self.fields {
            Some(fields) => fold_name(k, fields),
            None => (*k).to_owned(),
        };
        seed.deserialize(key.into_deserializer()).map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, DeError> {
        let (k, v) = self.entries[self.next];
        self.next += 1;
        seed.deserialize(ValueDe(v)).map_err(|e| e.at(k))
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.entries.len() - self.next)
    }
}

struct EnumDe<'a> {
    variant: String,
    value: Option<&'a Value>,
}

impl<'de, 'a> EnumAccess<'de> for EnumDe<'a> {
    type Error = DeError;
    type Variant = VariantDe<'a>;

    fn variant_seed<T: DeserializeSeed<'de>>(
        self,
        seed: T,
    ) -> Result<(T::Value, VariantDe<'a>), DeError> {
        let v = seed.deserialize(self.variant.into_deserializer())?;
        Ok((v, VariantDe(self.value)))
    }
}

struct VariantDe<'a>(Option<&'a Value>);

impl<'de> VariantAccess<'de> for VariantDe<'_> {
    type Error = DeError;

    fn unit_variant(self) -> Result<(), DeError> {
        Ok(())
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, DeError> {
        seed.deserialize(ValueDe(self.0.unwrap_or(&Value::Null)))
    }

    fn tuple_variant<V: Visitor<'de>>(self, _len: usize, visitor: V) -> Result<V::Value, DeError> {
        de::Deserializer::deserialize_seq(ValueDe(self.0.unwrap_or(&Value::Null)), visitor)
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, DeError> {
        de::Deserializer::deserialize_struct(
            ValueDe(self.0.unwrap_or(&Value::Null)),
            "",
            fields,
            visitor,
        )
    }
}
