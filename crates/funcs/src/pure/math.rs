//! `to_math` (feature `math`): Go's `transform.ToMath`, LaTeX to MathML and HTML with KaTeX
//! ([`super::katex`]).
//!
//! Rewritten from `tpl/transform/transform.go` (`ToMath`) and `internal/warpc/katex.go`
//! (`KatexOptions`) of this repository at commit `44529028`: the same defaults (output
//! `mathml`, `minRuleThickness` 0.04, `errorColor` `#cc0000`, `throwOnError`, `strict`
//! `error`), the options decoded as `mapstructure.WeakDecode` decodes them, the same check of
//! `strict`, and the same message to KaTeX, so the markup is KaTeX's own.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, PoisonError};

use serde_json::{Map as JsonMap, Value as Json, json};
use ssg_base::diag::Diagnostic;
use tera::Value;

use super::value::{entries, text};
use super::{PureEnv, Registrar, katex};

/// `to_math(options=?, optional=?)`. A formula KaTeX rejects (with `throwOnError`, the default)
/// is an error, as are invalid options; with `optional=true` (Go's `try`) the error is a
/// warning instead (id `to_math`, as for `get_remote`) and the result none. The warnings of
/// `strict: "warn"` are warnings.
///
/// Rendered formulas are kept for the build (Go's `cacheMath`): the same formula with the
/// same options is rendered once, and its warnings are reported once.
pub(super) fn register(r: &mut Registrar<'_>, env: &Arc<PureEnv>) {
    let env = Arc::clone(env);
    let cache: Mutex<HashMap<String, Arc<str>>> = Mutex::default();
    r.filter("to_math", move |v, kw, _| {
        let optional = kw.get::<bool>("optional")?.unwrap_or(false);
        let options = kw.get::<Value>("options")?;
        let expression = text(&v, "to_math")?;
        let rendered = input(&expression, options.as_ref()).and_then(|input| {
            if let Some(output) = cache
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(&input)
            {
                return Ok(Arc::clone(output));
            }
            let rendered = katex::render(&input)?;
            let mut cache = cache.lock().unwrap_or_else(PoisonError::into_inner);
            let output = cache.entry(input).or_insert_with(|| {
                // the first render of this formula reports its warnings
                for warning in &rendered.warnings {
                    env.diagnostics
                        .push(Diagnostic::warning(format!("to_math: {warning}")));
                }
                Arc::from(rendered.output.as_str())
            });
            Ok(Arc::clone(output))
        });
        match rendered {
            Ok(output) => Ok(Value::safe_string(&output)),
            Err(message) if optional => {
                env.diagnostics
                    .push(Diagnostic::warning(message).with_id("to_math"));
                Ok(Value::none())
            }
            Err(message) => Err(tera::Error::message(message)),
        }
    });
}

/// The type of a field of `KatexOptions`, which decides how a value is decoded.
#[derive(Clone, Copy)]
enum Kind {
    String,
    Bool,
    Float,
    /// `map[string]string`.
    Macros,
}

/// The fields of Go's `warpc.KatexOptions` in their order: the Go name, which an option key
/// matches exactly or else case-insensitively (mapstructure), and the JSON name KaTeX reads.
const FIELDS: [(&str, &str, Kind); 9] = [
    ("Output", "output", Kind::String),
    ("DisplayMode", "displayMode", Kind::Bool),
    ("Leqno", "leqno", Kind::Bool),
    ("Fleqn", "fleqn", Kind::Bool),
    ("ErrorColor", "errorColor", Kind::String),
    ("Macros", "macros", Kind::Macros),
    ("MinRuleThickness", "minRuleThickness", Kind::Float),
    ("ThrowOnError", "throwOnError", Kind::Bool),
    ("Strict", "strict", Kind::String),
];

/// The message to KaTeX for `expression` with `options` (Go: `warpc.KatexInput` as JSON).
///
/// # Errors
/// An option that cannot be decoded, an invalid `strict`, or a `minRuleThickness` JSON cannot
/// hold (NaN, infinite: Go's encoder rejects it).
fn input(expression: &str, options: Option<&Value>) -> Result<String, String> {
    let options = katex_options(options)?;
    match options.get("strict").and_then(Json::as_str) {
        Some("error" | "ignore" | "warn") => {}
        other => {
            return Err(format!(
                "to_math: invalid strict mode; expected one of error, ignore, or warn; received {}",
                other.unwrap_or_default()
            ));
        }
    }
    Ok(json!({ "expression": expression, "options": options }).to_string())
}

/// Go's defaults with `options` decoded over them, as JSON (`macros` only when not empty,
/// Go's `omitempty`), keys sorted (whether or not serde_json keeps insertion order: its
/// `preserve_order` feature is on in the binary's graph).
fn katex_options(options: Option<&Value>) -> Result<JsonMap<String, Json>, String> {
    let mut out = BTreeMap::new();
    out.insert("output".into(), "mathml".into());
    out.insert("displayMode".into(), false.into());
    out.insert("leqno".into(), false.into());
    out.insert("fleqn".into(), false.into());
    out.insert("errorColor".into(), "#cc0000".into());
    out.insert("minRuleThickness".into(), 0.04.into());
    out.insert("throwOnError".into(), true.into());
    out.insert("strict".into(), "error".into());
    let Some(options) = options.filter(|o| !o.is_none() && !o.is_undefined()) else {
        return Ok(out.into_iter().collect());
    };
    let map = options
        .as_map()
        .ok_or_else(|| format!("to_math: options must be a map, got {}", options.name()))?;
    let entries: Vec<_> = entries(map).collect();
    let mut macros = BTreeMap::new();
    for (go_name, json_name, kind) in FIELDS {
        let found = entries.iter().find(|(k, _)| k == go_name).or_else(|| {
            entries
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(go_name))
        });
        // a missing or none option keeps the default (mapstructure ignores nil input)
        let Some((key, value)) = found.filter(|(_, v)| !v.is_none() && !v.is_undefined()) else {
            continue;
        };
        let decoded = match kind {
            Kind::String => {
                Json::String(weak_string(value).ok_or_else(|| expected(key, "a string", value))?)
            }
            Kind::Bool => Json::Bool(weak_bool(key, value)?),
            Kind::Float => {
                let f = weak_float(key, value)?;
                if !f.is_finite() {
                    return Err(format!(
                        "to_math: option `{key}`: {f} is not a number JSON can hold"
                    ));
                }
                Json::from(f)
            }
            Kind::Macros => {
                weak_macros(key, value, &mut macros)?;
                continue;
            }
        };
        out.insert(json_name.to_owned(), decoded);
    }
    if !macros.is_empty() {
        let macros: JsonMap<String, Json> = macros
            .into_iter()
            .map(|(k, v)| (k, Json::String(v)))
            .collect();
        out.insert("macros".into(), Json::Object(macros));
    }
    Ok(out.into_iter().collect())
}

/// The text of a number that came from serde_json with `arbitrary_precision` (on in the binary's
/// graph): a map holding it under serde_json's private key.
fn json_number_text(value: &Value) -> Option<&str> {
    let map = value.as_map()?;
    let mut entries = entries(map);
    let (key, text) = entries.next()?;
    (entries.next().is_none() && key == "$serde_json::private::Number")
        .then(|| text.as_str())
        .flatten()
}

fn expected(key: &str, what: &str, value: &Value) -> String {
    format!(
        "to_math: option `{key}`: expected {what}, got {} `{value}`",
        value.name()
    )
}

/// A string as mapstructure's weak decoding makes one: strings as they are, booleans `1` or
/// `0`, numbers printed (Go's `strconv.FormatFloat(f, 'f', -1, 64)` for floats), bytes as
/// text; none for anything else.
fn weak_string(value: &Value) -> Option<String> {
    if let Some(s) = value.as_str() {
        return Some(s.to_owned());
    }
    if let Some(text) = json_number_text(value) {
        return weak_string(&number_from_text(text));
    }
    if let Some(b) = value.as_bool() {
        return Some(if b { "1" } else { "0" }.to_owned());
    }
    if value.is_f64() {
        let f = value.as_f64().unwrap_or_default();
        return Some(if f.is_nan() {
            "NaN".to_owned()
        } else if f.is_infinite() {
            if f > 0.0 { "+Inf" } else { "-Inf" }.to_owned()
        } else {
            // shortest round-trip digits without an exponent, as Go's 'f' with precision -1
            format!("{f}")
        });
    }
    if value.is_number() {
        return Some(value.to_string());
    }
    value
        .as_bytes()
        .map(|b| String::from_utf8_lossy(b).into_owned())
}

/// A boolean as mapstructure's weak decoding makes one: numbers are true when not zero,
/// strings as Go's `strconv.ParseBool` reads them (and the empty string false).
fn weak_bool(key: &str, value: &Value) -> Result<bool, String> {
    if let Some(b) = value.as_bool() {
        return Ok(b);
    }
    if let Some(text) = json_number_text(value) {
        return weak_bool(key, &number_from_text(text));
    }
    if value.is_number() {
        return Ok(value.as_f64().is_none_or(|f| f != 0.0));
    }
    match value.as_str() {
        Some("1" | "t" | "T" | "TRUE" | "true" | "True") => Ok(true),
        Some("0" | "f" | "F" | "FALSE" | "false" | "False" | "") => Ok(false),
        Some(s) => Err(format!(
            "to_math: option `{key}`: cannot parse {s:?} as a boolean"
        )),
        None => Err(expected(key, "a boolean", value)),
    }
}

/// A float as mapstructure's weak decoding makes one: numbers, booleans 1 or 0, strings parsed
/// (the empty string 0).
fn weak_float(key: &str, value: &Value) -> Result<f64, String> {
    if let Some(b) = value.as_bool() {
        return Ok(if b { 1.0 } else { 0.0 });
    }
    if let Some(text) = json_number_text(value) {
        return weak_float(key, &number_from_text(text));
    }
    if value.is_number() {
        return value
            .as_f64()
            .or_else(|| value.as_i128().map(|i| i as f64))
            .ok_or_else(|| expected(key, "a number", value));
    }
    match value.as_str() {
        Some("") => Ok(0.0),
        Some(s) => s
            .parse::<f64>()
            .map_err(|_| format!("to_math: option `{key}`: cannot parse {s:?} as a number")),
        None => Err(expected(key, "a number", value)),
    }
}

/// A number's text as a value: an integer that fits `i64`, else a float.
fn number_from_text(text: &str) -> Value {
    text.parse::<i64>().map_or_else(
        |_| Value::from(text.parse::<f64>().unwrap_or(f64::NAN)),
        Value::from,
    )
}

/// `macros` (Go: `map[string]string`) decoded into `out`: a map's entries with their values as
/// strings (none: the empty string); an array of maps merges them (mapstructure's weak
/// decoding; an empty array is no macros).
fn weak_macros(key: &str, value: &Value, out: &mut BTreeMap<String, String>) -> Result<(), String> {
    if let Some(map) = value.as_map() {
        for (name, definition) in entries(map) {
            let definition = if definition.is_none() || definition.is_undefined() {
                String::new()
            } else {
                weak_string(definition)
                    .ok_or_else(|| expected(&format!("{key}[{name}]"), "a string", definition))?
            };
            out.insert(name.into_owned(), definition);
        }
        return Ok(());
    }
    if let Some(items) = value.as_array() {
        for (i, item) in items.iter().enumerate() {
            weak_macros(&format!("{key}[{i}]"), item, out)?;
        }
        return Ok(());
    }
    Err(expected(key, "a map", value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(src: &str) -> Result<JsonMap<String, Json>, String> {
        let v: Json = serde_json::from_str(src).unwrap();
        katex_options(Some(&Value::from_serializable(&v)))
    }

    #[test]
    fn defaults() {
        let o = katex_options(None).unwrap();
        assert_eq!(
            Json::Object(o).to_string(),
            r##"{"displayMode":false,"errorColor":"#cc0000","fleqn":false,"leqno":false,"minRuleThickness":0.04,"output":"mathml","strict":"error","throwOnError":true}"##
        );
    }

    #[test]
    fn weak_decoding() {
        let o = opts(r#"{"OUTPUT": "html", "displaymode": "t", "leqno": 1, "fleqn": 0.0, "errorColor": 5, "minRuleThickness": "0.5", "throwOnError": "", "strict": null, "other": [1]}"#).unwrap();
        assert_eq!(o["output"], "html");
        assert_eq!(o["displayMode"], true);
        assert_eq!(o["leqno"], true);
        assert_eq!(o["fleqn"], false);
        assert_eq!(o["errorColor"], "5");
        assert_eq!(o["minRuleThickness"], 0.5);
        assert_eq!(o["throwOnError"], false);
        assert_eq!(o["strict"], "error");
        // an exact match of the Go name wins over a case-insensitive one
        let o = opts(r#"{"output": "html", "Output": "mathml"}"#).unwrap();
        assert_eq!(o["output"], "mathml");
        let o = opts(r#"{"macros": {"\\a": 1, "\\b": true, "\\c": 1.5, "\\d": null}}"#).unwrap();
        assert_eq!(
            o["macros"],
            json!({"\\a": "1", "\\b": "1", "\\c": "1.5", "\\d": ""})
        );
        let o = opts(r#"{"macros": [{"\\a": "x"}, {"\\b": "y"}]}"#).unwrap();
        assert_eq!(o["macros"], json!({"\\a": "x", "\\b": "y"}));
        assert!(!opts(r#"{"macros": []}"#).unwrap().contains_key("macros"));
    }

    #[test]
    fn decoding_errors() {
        for (src, want) in [
            (
                r#"{"displayMode": "yes"}"#,
                "option `displayMode`: cannot parse \"yes\" as a boolean",
            ),
            (
                r#"{"minRuleThickness": "x"}"#,
                "option `minRuleThickness`: cannot parse \"x\" as a number",
            ),
            (
                r#"{"output": [1]}"#,
                "option `output`: expected a string, got array",
            ),
            (
                r#"{"macros": "x"}"#,
                "option `macros`: expected a map, got string",
            ),
            (
                r#"{"minRuleThickness": "nan"}"#,
                "option `minRuleThickness`: NaN is not a number JSON can hold",
            ),
        ] {
            let e = opts(src).unwrap_err();
            assert!(e.contains(want), "{src}: {e}");
        }
        let e = katex_options(Some(&Value::from("x"))).unwrap_err();
        assert_eq!(e, "to_math: options must be a map, got string");
        let e = input(
            "x",
            Some(&Value::from_serializable(&json!({"strict": "nope"}))),
        )
        .unwrap_err();
        assert_eq!(
            e,
            "to_math: invalid strict mode; expected one of error, ignore, or warn; received nope"
        );
    }
}
