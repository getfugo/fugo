//! The Go implementation's structure hash, for names that must equal the Go build's:
//! `hashing.HashString` / `HashStringHex` hash values with its fork of hashstructure (v0.5.0) and
//! xxHash64. Only the value kinds this port names things with are covered: strings, integers,
//! floats, booleans, nil, lists, maps and flat structs.
//!
//! The rules (hashstructure's walker): a string or a number is the xxHash64 of its bytes
//! (numbers little-endian, 8 bytes); a list folds its elements with `ordered(h, e)` from 0; a
//! map XORs `ordered(key, value)` over its entries, then hashes the result once more; a struct
//! starts from the hash of its type name and, for each exported field, XORs
//! `ordered(name, value)` and hashes the result.

use ssg_base::{Map, Value};
use xxhash_rust::xxh64::xxh64;

/// xxHash64 with seed 0.
pub(crate) fn xh(b: &[u8]) -> u64 {
    xxh64(b, 0)
}

/// `hashUpdateOrdered`: the hash of `a` then `b`, little-endian.
pub(crate) fn ordered(a: u64, b: u64) -> u64 {
    let mut buf = [0u8; 16];
    buf[..8].copy_from_slice(&a.to_le_bytes());
    buf[8..].copy_from_slice(&b.to_le_bytes());
    xh(&buf)
}

/// `hashFinishUnordered`.
fn finish(h: u64) -> u64 {
    xh(&h.to_le_bytes())
}

/// A Go `string`.
pub(crate) fn string(s: &str) -> u64 {
    xh(s.as_bytes())
}

/// A Go `int` (`int64`).
pub(crate) fn int(i: i64) -> u64 {
    xh(&i.to_le_bytes())
}

/// A slice or array of already hashed elements.
pub(crate) fn list(items: impl IntoIterator<Item = u64>) -> u64 {
    items.into_iter().fold(0, ordered)
}

/// A struct named `name` (`""` for an anonymous struct) with its exported fields in
/// declaration order, values already hashed.
pub(crate) fn structure(name: &str, fields: &[(&str, u64)]) -> u64 {
    fields
        .iter()
        .fold(string(name), |h, &(k, v)| finish(h ^ ordered(string(k), v)))
}

/// A `map[string]any` (`None`: nil, hashed as an empty map).
pub(crate) fn map(m: Option<&Map>) -> u64 {
    let mut h = 0u64;
    for (k, v) in m.into_iter().flat_map(Map::iter) {
        h ^= ordered(string(k), value(v));
    }
    finish(h)
}

/// A template or data value.
pub(crate) fn value(v: &Value) -> u64 {
    match v {
        Value::String(s) => string(s),
        Value::Int(i) => int(*i),
        Value::Float(f) => xh(&f.to_bits().to_le_bytes()),
        Value::Bool(b) => xh(&[u8::from(*b)]),
        Value::Null => int(0),
        Value::Date(d) => string(&d.to_string()),
        Value::Array(items) => list(items.iter().map(value)),
        Value::Map(m) => map(Some(m)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `hashing.HashStringHex(url, opts)` of `images.QR` for the URL of Go's `TestQR` (kept
    /// as the recorded Go data has it): `opts` is the anonymous struct
    /// `{Level string; Scale int; TargetDir string}`.
    #[test]
    fn qr_names_of_the_go_test() {
        let name = |level: &str, scale: i64, dir: &str| {
            let opts = structure(
                "",
                &[
                    ("Level", string(level)),
                    ("Scale", int(scale)),
                    ("TargetDir", string(dir)),
                ],
            );
            format!("{:x}", list([string("https://gohugo.io"), opts]))
        };
        assert_eq!(name("medium", 4, ""), "924bf7d80a564b23");
        assert_eq!(name("low", 2, ""), "9bf1ce25c5f2c058");
        assert_eq!(name("medium", 3, ""), "7af14b329dd10af7");
        assert_eq!(name("quartile", 5, ""), "9600ecb2010c2185");
        assert_eq!(name("high", 6, ""), "bdc74ee7f5c11cc6");
        assert_eq!(name("high", 6, "foo/bar"), "14162f02f2b83fff");
    }
}
