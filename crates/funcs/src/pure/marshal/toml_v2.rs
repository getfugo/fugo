//! The TOML writer: go-toml v2's output, with indented tables.

use super::*;

/// go-toml v2's `needsQuoting`: a `'`, a line break or a control character other than tab.
pub(super) fn toml_needs_quoting(s: &str) -> bool {
    s.bytes()
        .any(|b| b == b'\'' || b == 0x7f || (b < 0x20 && b != b'\t'))
}

pub(super) fn toml_quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 || c == '\u{7f}' => {
                let _ = write!(out, "\\u{:04X}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub(super) fn toml_string(s: &str) -> String {
    if toml_needs_quoting(s) {
        toml_quoted(s)
    } else {
        format!("'{s}'")
    }
}

/// A TOML key: bare when it can be, else literal, else quoted.
pub(super) fn toml_key(k: &str) -> String {
    if k.is_empty() {
        return "''".to_owned();
    }
    if k.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        k.to_owned()
    } else {
        toml_string(k)
    }
}

pub(super) fn toml_entries(v: &Value) -> Vec<(String, &Value)> {
    let mut pairs: Vec<(String, &Value)> = v
        .as_map()
        .map(|m| entries(m).map(|(k, v)| (k.into_owned(), v)).collect())
        .unwrap_or_default();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    pairs
}

pub(super) fn toml_inline(v: &Value) -> TeraResult<String> {
    if v.is_none() || v.is_undefined() {
        return Err(tera::Error::message("remarshal: TOML has no null"));
    }
    if let Some(s) = v.as_str() {
        return Ok(match marked_date(s) {
            Some((date, _)) => date.to_owned(),
            None => toml_string(s),
        });
    }
    if v.is_f64() {
        let f = v.as_f64().unwrap_or_default();
        return Ok(if f.is_nan() {
            "nan".to_owned()
        } else if f.is_infinite() {
            if f > 0.0 { "inf" } else { "-inf" }.to_owned()
        } else if f.fract() == 0.0 {
            format!("{f:.1}")
        } else {
            format!("{f}")
        });
    }
    if let Some(a) = v.as_array() {
        let items = a.iter().map(toml_inline).collect::<TeraResult<Vec<_>>>()?;
        return Ok(format!("[{}]", items.join(", ")));
    }
    if v.is_map() {
        let items = toml_entries(v)
            .into_iter()
            .filter(|(_, x)| !x.is_none())
            .map(|(k, x)| Ok(format!("{} = {}", toml_key(&k), toml_inline(x)?)))
            .collect::<TeraResult<Vec<_>>>()?;
        return Ok(format!("{{{}}}", items.join(", ")));
    }
    Ok(v.to_string())
}

pub(super) fn is_table_array(v: &Value) -> bool {
    v.as_array()
        .is_some_and(|a| !a.is_empty() && a.iter().all(Value::is_map))
}

/// A TOML document: plain keys first, then tables (indented two spaces per level) and arrays of
/// tables.
pub(in super::super) fn toml(v: &Value) -> TeraResult<String> {
    if !v.is_map() {
        return Err(tera::Error::message(
            "remarshal: a TOML document is a table",
        ));
    }
    let mut out = String::new();
    toml_table(v, &[], &mut out)?;
    Ok(out)
}

pub(super) fn toml_table(v: &Value, path: &[String], out: &mut String) -> TeraResult<()> {
    let pad = "  ".repeat(path.len());
    let pairs = toml_entries(v);
    // a blank line between tables, and after the plain keys (not right after a bare header)
    let mut blank = false;
    for (k, item) in &pairs {
        if item.is_map() || is_table_array(item) || item.is_none() {
            continue;
        }
        let _ = writeln!(out, "{pad}{} = {}", toml_key(k), toml_inline(item)?);
        blank = true;
    }
    for (k, item) in &pairs {
        let mut sub = path.to_vec();
        sub.push(toml_key(k));
        let tables: &[Value] = if item.is_map() {
            std::slice::from_ref(item)
        } else if is_table_array(item) {
            item.as_array().unwrap_or_default()
        } else {
            continue;
        };
        for t in tables {
            if blank {
                out.push('\n');
            }
            blank = true;
            let (open, close) = if item.is_map() {
                ("[", "]")
            } else {
                ("[[", "]]")
            };
            let _ = writeln!(out, "{pad}{open}{}{close}", sub.join("."));
            toml_table(t, &sub, out)?;
        }
    }
    Ok(())
}
