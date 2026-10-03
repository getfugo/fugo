//! Encoding: `jsonify`, `remarshal`, `dump`, hashes.

use std::fmt::Write as _;

use md5::Digest as _;
use ssg_base::Value as Data;
use tera::{Kwargs, TeraResult, Value};

use super::value::{entries, text};
use super::{Registrar, marshal};

pub(super) fn register(r: &mut Registrar<'_>) {
    r.filter("jsonify", |v, kw, _| {
        let indent = kw.get::<&str>("indent")?;
        let json = Json {
            indent,
            html_safe: true,
        };
        Ok(Value::safe_string(&json.encode(&v)?))
    });
    r.filter("dump", |v, _, _| {
        let json = Json {
            indent: Some("  "),
            html_safe: false,
        };
        Ok(Value::from(json.encode(&v)?))
    });
    r.filter("remarshal", |v, kw, _| remarshal(&v, kw));
    r.filter("md5", |v, _, _| {
        Ok(Value::from(hex(&md5::Md5::digest(
            text(&v, "md5")?.as_bytes(),
        ))))
    });
    r.filter("sha1", |v, _, _| {
        Ok(Value::from(hex(&sha1::Sha1::digest(
            text(&v, "sha1")?.as_bytes(),
        ))))
    });
    r.filter("sha256", |v, _, _| {
        Ok(Value::from(hex(&sha2::Sha256::digest(
            text(&v, "sha256")?.as_bytes(),
        ))))
    });
    r.filter("fnv32a", |v, _, _| {
        Ok(Value::from(fnv32a(text(&v, "fnv32a")?.as_bytes())))
    });
    r.filter("xxhash", |v, _, _| {
        let h = xxhash_rust::xxh64::xxh64(text(&v, "xxhash")?.as_bytes(), 0);
        Ok(Value::from(format!("{h:016x}")))
    });
}

fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// FNV-1a, 32 bits.
fn fnv32a(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811c_9dc5_u32, |h, b| {
        (h ^ u32::from(*b)).wrapping_mul(0x0100_0193)
    })
}

/// A JSON writer with sorted keys and Go's number and escaping rules.
struct Json<'a> {
    /// Pretty-print with this indentation.
    indent: Option<&'a str>,
    /// Escape `<`, `>` and `&` (`<` …) so the output is safe in HTML and `<script>`.
    html_safe: bool,
}

impl Json<'_> {
    fn encode(&self, v: &Value) -> TeraResult<String> {
        let mut out = String::new();
        self.value(v, 0, &mut out)?;
        Ok(out)
    }

    fn newline(&self, depth: usize, out: &mut String) {
        if let Some(indent) = self.indent {
            out.push('\n');
            for _ in 0..depth {
                out.push_str(indent);
            }
        }
    }

    fn value(&self, v: &Value, depth: usize, out: &mut String) -> TeraResult<()> {
        if v.is_none() || v.is_undefined() {
            out.push_str("null");
        } else if let Some(b) = v.as_bool() {
            out.push_str(if b { "true" } else { "false" });
        } else if v.is_f64() {
            out.push_str(&go_float(v.as_f64().unwrap_or_default())?);
        } else if v.is_number() {
            out.push_str(&v.to_string());
        } else if let Some(s) = v.as_str() {
            self.string(s, out);
        } else if let Some(a) = v.as_array() {
            if a.is_empty() {
                out.push_str("[]");
                return Ok(());
            }
            out.push('[');
            for (i, item) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                self.newline(depth + 1, out);
                self.value(item, depth + 1, out)?;
            }
            self.newline(depth, out);
            out.push(']');
        } else if let Some(m) = v.as_map() {
            if m.is_empty() {
                out.push_str("{}");
                return Ok(());
            }
            let mut pairs: Vec<_> = entries(m).collect();
            pairs.sort_by(|a, b| a.0.cmp(&b.0));
            out.push('{');
            for (i, (k, item)) in pairs.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                self.newline(depth + 1, out);
                self.string(&k, out);
                out.push(':');
                if self.indent.is_some() {
                    out.push(' ');
                }
                self.value(item, depth + 1, out)?;
            }
            self.newline(depth, out);
            out.push('}');
        } else if let Some(b) = v.as_bytes() {
            self.string(&String::from_utf8_lossy(b), out);
        }
        Ok(())
    }

    fn string(&self, s: &str, out: &mut String) {
        out.push('"');
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                '\u{8}' => out.push_str("\\b"),
                '\u{c}' => out.push_str("\\f"),
                '<' | '>' | '&' if self.html_safe => {
                    let _ = write!(out, "\\u{:04x}", u32::from(c));
                }
                '\u{2028}' | '\u{2029}' => {
                    let _ = write!(out, "\\u{:04x}", u32::from(c));
                }
                c if u32::from(c) < 0x20 => {
                    let _ = write!(out, "\\u{:04x}", u32::from(c));
                }
                c => out.push(c),
            }
        }
        out.push('"');
    }
}

/// A float as Go's `encoding/json` writes it: shortest digits; exponent form below `1e-6` and
/// from `1e21` (`1e-7`, `1e+21`).
fn go_float(f: f64) -> TeraResult<String> {
    if !f.is_finite() {
        return Err(tera::Error::message(format!(
            "{f} cannot be written as JSON"
        )));
    }
    let abs = f.abs();
    if abs != 0.0 && !(1e-6..1e21).contains(&abs) {
        let e = format!("{f:e}");
        return Ok(match e.split_once('e') {
            Some((m, exp)) if !exp.starts_with('-') => format!("{m}e+{exp}"),
            _ => e,
        });
    }
    Ok(format!("{f}"))
}

/// A data format of `remarshal` and `unmarshal`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    Json,
    Toml,
    Yaml,
}

impl Format {
    fn parse(s: &str) -> TeraResult<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "json" => Ok(Self::Json),
            "toml" => Ok(Self::Toml),
            "yaml" | "yml" => Ok(Self::Yaml),
            other => Err(tera::Error::message(format!(
                "remarshal(format=): unknown format `{other}`; expected json, toml or yaml"
            ))),
        }
    }

    /// The format of a document, from the first of `{` (JSON), `:` (YAML) and `=` (TOML); a
    /// document starting with `[` is JSON only if it parses as JSON (TOML tables start with `[`).
    fn detect(s: &str) -> Option<Self> {
        if s.trim_start().starts_with('[') && Data::from_json_str(s).is_ok() {
            return Some(Self::Json);
        }
        let first = |c: char| s.find(c).unwrap_or(usize::MAX);
        let json = first('{');
        let yaml = first(':');
        let toml = first('=');
        let min = json.min(yaml).min(toml);
        if min == usize::MAX {
            None
        } else if min == json {
            Some(Self::Json)
        } else if min == toml {
            Some(Self::Toml)
        } else {
            Some(Self::Yaml)
        }
    }
}

/// Re-encodes data (a map, or a JSON/TOML/YAML document) as `format`.
fn remarshal(v: &Value, kw: &Kwargs) -> TeraResult<Value> {
    let format = Format::parse(kw.must_get::<&str>("format")?)?;
    let data: Value = if v.is_none() || v.is_undefined() {
        return Ok(Value::from(""));
    } else if let Some(s) = v.as_str() {
        if s.trim().is_empty() {
            return Ok(Value::from(""));
        }
        let from = Format::detect(s).ok_or_else(|| {
            tera::Error::message("remarshal: cannot detect the format of the input")
        })?;
        let decoded = match from {
            Format::Json => Data::from_json_str(s).map_err(|e| e.to_string()),
            Format::Toml => toml_with_text_dates(s),
            Format::Yaml => Data::from_yaml_str(s).map_err(|e| e.to_string()),
        }
        .map_err(|e| tera::Error::message(format!("remarshal: {e}")))?;
        decoded.to_tera()
    } else if v.is_map() || v.is_array() {
        v.clone()
    } else {
        return Err(tera::Error::message(format!(
            "remarshal: cannot detect the format of a {}",
            v.name()
        )));
    };
    let data = marshal::prepare(&data, format == Format::Json);
    let out = match format {
        Format::Json => {
            // Go's `json.MarshalIndent` escapes `<`, `>` and `&` (`<` …).
            let mut s = Json {
                indent: Some("   "),
                html_safe: true,
            }
            .encode(&data)?;
            s.push('\n');
            s
        }
        Format::Yaml => marshal::yaml(&data),
        Format::Toml => marshal::toml(&data)?,
    };
    Ok(Value::from(out))
}

/// Decodes a TOML document for `remarshal`, its date-times as marked text
/// ([`marshal::date_marker`]): a local date stays `2023-01-01` (as Go's `toml.LocalDate`
/// marshals), not midnight of that day, and TOML writes it back as a date.
fn toml_with_text_dates(s: &str) -> Result<Data, String> {
    fn walk(v: toml::Value) -> toml::Value {
        match v {
            toml::Value::Datetime(d) => toml::Value::String(marshal::date_marker(
                &toml_date_text(&d),
                d.offset.is_some(),
            )),
            toml::Value::Array(a) => toml::Value::Array(a.into_iter().map(walk).collect()),
            toml::Value::Table(t) => {
                toml::Value::Table(t.into_iter().map(|(k, v)| (k, walk(v))).collect())
            }
            other => other,
        }
    }
    let table: toml::Table = toml::from_str(s).map_err(|e| e.to_string())?;
    Ok(Data::from_toml(walk(toml::Value::Table(table))))
}

/// A TOML date-time as text: `2006-01-02`, `15:04:05[.frac]`, `2006-01-02T15:04:05[.frac]`, and
/// with an offset `…Z` or `…+07:00` (RFC 3339 with trailing fraction zeros trimmed).
fn toml_date_text(d: &toml::value::Datetime) -> String {
    let mut out = String::new();
    if let Some(date) = d.date {
        let _ = write!(out, "{:04}-{:02}-{:02}", date.year, date.month, date.day);
    }
    if let Some(time) = d.time {
        if d.date.is_some() {
            out.push('T');
        }
        let _ = write!(
            out,
            "{:02}:{:02}:{:02}",
            time.hour,
            time.minute,
            time.second.unwrap_or(0)
        );
        let nanos = time.nanosecond.unwrap_or(0);
        if nanos > 0 {
            let frac = format!("{nanos:09}");
            let _ = write!(out, ".{}", frac.trim_end_matches('0'));
        }
    }
    match d.offset {
        None => {}
        Some(toml::value::Offset::Z | toml::value::Offset::Custom { minutes: 0 }) => out.push('Z'),
        Some(toml::value::Offset::Custom { minutes }) => {
            let sign = if minutes < 0 { '-' } else { '+' };
            let m = minutes.unsigned_abs();
            let _ = write!(out, "{sign}{:02}:{:02}", m / 60, m % 60);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::Format;

    #[test]
    fn detect_tells_toml_tables_from_json_arrays() {
        assert!(Format::detect("[1, 2]") == Some(Format::Json));
        assert!(Format::detect("[{\"a\": 1}]") == Some(Format::Json));
        assert!(Format::detect("[[headers]]\nfor = '/*'\n") == Some(Format::Toml));
        assert!(Format::detect("[params.x]\ny = 1\n") == Some(Format::Toml));
        assert!(
            Format::detect("[server]\n  [[server.headers]]\n    for = '/*'\n")
                == Some(Format::Toml)
        );
    }
}
