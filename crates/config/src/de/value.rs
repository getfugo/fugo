//! Deserializing a `Value`.

use super::*;

#[derive(Clone, Copy)]
pub(super) struct ValueDe<'a>(pub(super) &'a Value);

macro_rules! int_method {
    ($name:ident, $visit:ident, $ty:ty) => {
        fn $name<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
            let i = weak_i64(self.0).ok_or_else(|| mismatch(self.0, "an integer"))?;
            let n = <$ty>::try_from(i)
                .map_err(|_| <DeError as de::Error>::custom(format_args!("{i} is out of range")))?;
            visitor.$visit(n)
        }
    };
}

impl<'de> de::Deserializer<'de> for ValueDe<'_> {
    type Error = DeError;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        match self.0 {
            Value::Null => visitor.visit_unit(),
            Value::Bool(b) => visitor.visit_bool(*b),
            Value::Int(i) => visitor.visit_i64(*i),
            Value::Float(f) => visitor.visit_f64(*f),
            Value::String(s) => visitor.visit_str(s),
            Value::Date(d) => visitor.visit_string(d.to_string()),
            Value::Array(a) => visitor.visit_seq(SeqDe { items: a, next: 0 }),
            Value::Map(m) => visitor.visit_map(MapAccessDe::new(m, None)),
        }
    }

    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        visitor.visit_bool(weak_bool(self.0).ok_or_else(|| mismatch(self.0, "a boolean"))?)
    }

    int_method!(deserialize_i8, visit_i8, i8);
    int_method!(deserialize_i16, visit_i16, i16);
    int_method!(deserialize_i32, visit_i32, i32);
    int_method!(deserialize_i64, visit_i64, i64);
    int_method!(deserialize_u8, visit_u8, u8);
    int_method!(deserialize_u16, visit_u16, u16);
    int_method!(deserialize_u32, visit_u32, u32);
    int_method!(deserialize_u64, visit_u64, u64);

    fn deserialize_f32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        self.deserialize_f64(visitor)
    }

    fn deserialize_f64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        visitor.visit_f64(weak_f64(self.0).ok_or_else(|| mismatch(self.0, "a number"))?)
    }

    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        self.deserialize_str(visitor)
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        match self.0 {
            Value::String(s) => visitor.visit_str(s),
            other => {
                visitor.visit_string(weak_string(other).ok_or_else(|| mismatch(other, "a string"))?)
            }
        }
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        self.deserialize_str(visitor)
    }

    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        self.deserialize_str(visitor)
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        self.deserialize_str(visitor)
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        if self.0.is_null() {
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        visitor.visit_unit()
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, DeError> {
        visitor.visit_unit()
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, DeError> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        match self.0 {
            Value::Array(a) => visitor.visit_seq(SeqDe { items: a, next: 0 }),
            Value::Null => visitor.visit_seq(SeqDe {
                items: &[],
                next: 0,
            }),
            Value::Map(_) => Err(mismatch(self.0, "a list")),
            single => visitor.visit_seq(SeqDe {
                items: std::slice::from_ref(single),
                next: 0,
            }),
        }
    }

    fn deserialize_tuple<V: Visitor<'de>>(
        self,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, DeError> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, DeError> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        match self.0 {
            Value::Map(m) => visitor.visit_map(MapAccessDe::new(m, None)),
            Value::Null => visitor.visit_map(MapAccessDe::empty(None)),
            other => Err(mismatch(other, "a table")),
        }
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, DeError> {
        match self.0 {
            Value::Map(m) => visitor.visit_map(MapAccessDe::new(m, Some(fields))),
            Value::Null => visitor.visit_map(MapAccessDe::empty(Some(fields))),
            other => Err(mismatch(other, "a table")),
        }
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, DeError> {
        match self.0 {
            Value::Map(m) if m.len() == 1 => {
                let (k, v) = m.iter().next().expect("one entry");
                visitor.visit_enum(EnumDe {
                    variant: fold_name(k, variants),
                    value: Some(v),
                })
            }
            other => {
                let s = weak_string(other).ok_or_else(|| mismatch(other, "a string"))?;
                visitor.visit_enum(EnumDe {
                    variant: fold_name(&s, variants),
                    value: None,
                })
            }
        }
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        self.deserialize_str(visitor)
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DeError> {
        visitor.visit_unit()
    }
}
