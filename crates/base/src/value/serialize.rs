//! Serde for values and maps: serialized as JSON is, deserialized from any self-describing format
//! (JSON numbers kept exact).

use super::*;

impl Serialize for Value {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Null => s.serialize_unit(),
            Self::Bool(b) => s.serialize_bool(*b),
            Self::Int(i) => s.serialize_i64(*i),
            Self::Float(f) => s.serialize_f64(*f),
            Self::String(v) => s.serialize_str(v),
            Self::Date(d) => s.collect_str(d),
            Self::Array(a) => {
                let mut seq = s.serialize_seq(Some(a.len()))?;
                for v in a.iter() {
                    seq.serialize_element(v)?;
                }
                seq.end()
            }
            Self::Map(m) => m.serialize(s),
        }
    }
}

impl Serialize for Map {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(self.len()))?;
        for (k, v) in self.iter() {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

pub(super) struct ValueVisitor;

impl<'de> Visitor<'de> for ValueVisitor {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("any data value")
    }

    fn visit_bool<E>(self, v: bool) -> Result<Value, E> {
        Ok(Value::Bool(v))
    }

    fn visit_i64<E>(self, v: i64) -> Result<Value, E> {
        Ok(Value::Int(v))
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "beyond i64 only a float can hold it"
    )]
    fn visit_u64<E>(self, v: u64) -> Result<Value, E> {
        Ok(i64::try_from(v).map_or(Value::Float(v as f64), Value::Int))
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "beyond i64 only a float can hold it"
    )]
    fn visit_i128<E>(self, v: i128) -> Result<Value, E> {
        Ok(i64::try_from(v).map_or(Value::Float(v as f64), Value::Int))
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "beyond i64 only a float can hold it"
    )]
    fn visit_u128<E>(self, v: u128) -> Result<Value, E> {
        Ok(i64::try_from(v).map_or(Value::Float(v as f64), Value::Int))
    }

    fn visit_f64<E>(self, v: f64) -> Result<Value, E> {
        Ok(Value::Float(v))
    }

    fn visit_str<E>(self, v: &str) -> Result<Value, E> {
        Ok(Value::String(v.into()))
    }

    fn visit_string<E>(self, v: String) -> Result<Value, E> {
        Ok(Value::String(v.into()))
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Value, E> {
        Ok(Value::String(String::from_utf8_lossy(v).into()))
    }

    fn visit_unit<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_none<E>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        Value::deserialize(d)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut items = Vec::with_capacity(seq.size_hint().unwrap_or(0));
        while let Some(v) = seq.next_element()? {
            items.push(v);
        }
        Ok(Value::array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, access: A) -> Result<Value, A::Error> {
        let m = read_map(access)?;
        // serde_json with `arbitrary_precision` (which rolldown turns on) passes a number as a
        // map holding its text under this key.
        if m.len() == 1
            && let Some(Value::String(text)) = m.get(JSON_NUMBER_TOKEN)
        {
            return Ok(json_number(text));
        }
        Ok(Value::map(m))
    }
}

/// The key serde_json's `arbitrary_precision` numbers deserialize under.
pub(super) const JSON_NUMBER_TOKEN: &str = "$serde_json::private::Number";

/// A JSON number's text as [`ValueVisitor`] reads numbers: an integer that fits `i64`, else a
/// float.
#[expect(
    clippy::cast_precision_loss,
    reason = "beyond i64 only a float can hold it"
)]
pub(super) fn json_number(text: &str) -> Value {
    if let Ok(i) = text.parse::<i64>() {
        Value::Int(i)
    } else if let Ok(u) = text.parse::<u64>() {
        Value::Float(u as f64)
    } else {
        Value::Float(text.parse().unwrap_or(f64::NAN))
    }
}

/// The entries of a table; a later duplicate key replaces an earlier one.
pub(super) fn read_map<'de, A: MapAccess<'de>>(mut access: A) -> Result<Map, A::Error> {
    let mut m = Map::new();
    while let Some(MapKey(k)) = access.next_key()? {
        let v: Value = access.next_value()?;
        m.insert(k, v);
    }
    Ok(m)
}

impl<'de> Deserialize<'de> for Value {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(ValueVisitor)
    }
}

impl<'de> Deserialize<'de> for Map {
    /// A table of any data format; keys keep their case (scalar keys become strings, as in
    /// [`Value`]) and a later duplicate key replaces an earlier one. Anything but a table is an
    /// error.
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct MapVisitor;
        impl<'de> Visitor<'de> for MapVisitor {
            type Value = Map;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a table")
            }
            fn visit_map<A: MapAccess<'de>>(self, access: A) -> Result<Map, A::Error> {
                read_map(access)
            }
        }
        d.deserialize_map(MapVisitor)
    }
}

/// A map key of any scalar type, written as a string (`1: x` has the key `"1"`).
pub(super) struct MapKey(pub(super) String);

impl<'de> Deserialize<'de> for MapKey {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct KeyVisitor;
        impl Visitor<'_> for KeyVisitor {
            type Value = MapKey;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a scalar map key")
            }
            fn visit_bool<E>(self, v: bool) -> Result<MapKey, E> {
                Ok(MapKey(v.to_string()))
            }
            fn visit_i64<E>(self, v: i64) -> Result<MapKey, E> {
                Ok(MapKey(v.to_string()))
            }
            fn visit_u64<E>(self, v: u64) -> Result<MapKey, E> {
                Ok(MapKey(v.to_string()))
            }
            fn visit_f64<E>(self, v: f64) -> Result<MapKey, E> {
                Ok(MapKey(v.to_string()))
            }
            fn visit_str<E>(self, v: &str) -> Result<MapKey, E> {
                Ok(MapKey(v.to_owned()))
            }
            fn visit_string<E>(self, v: String) -> Result<MapKey, E> {
                Ok(MapKey(v))
            }
            fn visit_unit<E>(self) -> Result<MapKey, E> {
                Ok(MapKey(String::new()))
            }
            fn visit_none<E>(self) -> Result<MapKey, E> {
                Ok(MapKey(String::new()))
            }
        }
        d.deserialize_any(KeyVisitor)
    }
}
