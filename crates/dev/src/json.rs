//! JSON as the harness's files hold it: sorted keys, `, ` and `: ` between items, one entry per
//! line in the large documents (manifests, baselines, patches.json), and gzip (no name, time 0)
//! for a file named `.gz`.

use std::io::{Read as _, Write as _};
use std::path::Path;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use serde_json::ser::Formatter;

use crate::{Fail, fail};

/// `value` with every object's keys sorted (serde_json keeps insertion order in this
/// workspace, `preserve_order`).
#[must_use]
pub fn canonical(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            Value::Object(
                keys.into_iter()
                    .map(|k| (k.clone(), canonical(&map[k])))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.iter().map(canonical).collect()),
        other => other.clone(),
    }
}

/// Writes `, ` between items and `: ` after keys, on one line.
struct Spaced;

impl Formatter for Spaced {
    fn begin_array_value<W: ?Sized + std::io::Write>(
        &mut self,
        w: &mut W,
        first: bool,
    ) -> std::io::Result<()> {
        if first { Ok(()) } else { w.write_all(b", ") }
    }

    fn begin_object_key<W: ?Sized + std::io::Write>(
        &mut self,
        w: &mut W,
        first: bool,
    ) -> std::io::Result<()> {
        if first { Ok(()) } else { w.write_all(b", ") }
    }

    fn begin_object_value<W: ?Sized + std::io::Write>(&mut self, w: &mut W) -> std::io::Result<()> {
        w.write_all(b": ")
    }
}

/// `value` on one line, keys sorted.
///
/// # Panics
/// When `value` does not serialize to JSON (a map with non-string keys).
#[must_use]
pub fn line<T: Serialize + ?Sized>(value: &T) -> String {
    let value = canonical(&serde_json::to_value(value).expect("a JSON value"));
    let mut out = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(&mut out, Spaced);
    value.serialize(&mut ser).expect("write to memory");
    String::from_utf8(out).expect("JSON is UTF-8")
}

/// A JSON string literal.
#[must_use]
pub fn string(s: &str) -> String {
    serde_json::to_string(s).expect("a string")
}

/// Gzip with no file name and time 0, at the best compression.
#[must_use]
pub fn gzip(data: &[u8]) -> Vec<u8> {
    let mut gz = flate2::GzBuilder::new()
        .mtime(0)
        .write(Vec::new(), flate2::Compression::best());
    gz.write_all(data).expect("write to memory");
    gz.finish().expect("write to memory")
}

fn is_gz(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "gz")
}

/// The bytes of a file, gunzipped when its name ends in `.gz`.
///
/// # Errors
/// An unreadable file.
pub fn read_bytes(path: &Path) -> Result<Vec<u8>, Fail> {
    let raw = std::fs::read(path).map_err(|e| fail!("{}: {e}", path.display()))?;
    if !is_gz(path) {
        return Ok(raw);
    }
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(&raw[..])
        .read_to_end(&mut out)
        .map_err(|e| fail!("{}: {e}", path.display()))?;
    Ok(out)
}

/// A JSON file (gunzipped when its name ends in `.gz`).
///
/// # Errors
/// An unreadable file or JSON that does not fit `T`.
pub fn read<T: DeserializeOwned>(path: &Path) -> Result<T, Fail> {
    serde_json::from_slice(&read_bytes(path)?).map_err(|e| fail!("{}: {e}", path.display()))
}

/// Writes `text`, gzipped when the name ends in `.gz`.
///
/// # Errors
/// The file cannot be written.
pub fn write(path: &Path, text: &str) -> Result<(), Fail> {
    let data = if is_gz(path) {
        gzip(text.as_bytes())
    } else {
        text.as_bytes().to_vec()
    };
    std::fs::write(path, data).map_err(|e| fail!("{}: {e}", path.display()))
}
