//! The YAML and TOML writers of `remarshal`: the output of Go's encoders (yaml.v2's emitter
//! and go-toml v2 with indented tables), so a configuration sample reads the same.
//!
//! - Scalars: YAML strings are plain when that reads back as the same string, else single-
//!   quoted, else double-quoted (yaml.v2); a multi-line string is a literal block. TOML strings
//!   are literal (`'…'`) unless they hold a `'`, a line break or a control character.
//! - Keys: YAML maps in yaml.v2's natural order (digit runs by value, letters after other
//!   characters); TOML keys bare when they can be, sorted bytewise, plain values before tables.
//! - Dates of a TOML document stay dates: TOML writes them bare, YAML a local date as a quoted
//!   string and an offset date-time plain, JSON as strings.

use std::cmp::Ordering;
use std::fmt::Write as _;
use std::sync::LazyLock;

use regex::Regex;
use tera::{Map, TeraResult, Value};

use super::value::entries;

/// The prefix that marks a TOML local date, date-time or time (as text) inside `remarshal`.
const LOCAL_DATE: &str = "\u{0}toml-local-date\u{0}";
/// The prefix that marks a TOML offset date-time (as RFC 3339 text) inside `remarshal`.
const OFFSET_DATE: &str = "\u{0}toml-offset-date\u{0}";

/// A marked TOML date: the text of a local (`false`) or offset (`true`) date.
pub(super) fn date_marker(text: &str, offset: bool) -> String {
    format!("{}{text}", if offset { OFFSET_DATE } else { LOCAL_DATE })
}

/// The date of a marked string: its text and whether it has an offset.
fn marked_date(s: &str) -> Option<(&str, bool)> {
    s.strip_prefix(LOCAL_DATE)
        .map(|d| (d, false))
        .or_else(|| s.strip_prefix(OFFSET_DATE).map(|d| (d, true)))
}

/// The data as Go's `remarshal` encodes it: integral floats of maps (not of arrays) become
/// integers (`applyMarshalTypes`); `plain_dates` turns marked dates back into strings (JSON).
pub(super) fn prepare(v: &Value, plain_dates: bool) -> Value {
    fn walk(v: &Value, in_map: bool, plain_dates: bool) -> Value {
        if let Some(m) = v.as_map() {
            let mut out = Map::new();
            for (k, item) in entries(m) {
                out.insert(k.into_owned().into(), walk(item, true, plain_dates));
            }
            Value::from(out)
        } else if let Some(a) = v.as_array() {
            Value::from(
                a.iter()
                    .map(|item| walk(item, false, plain_dates))
                    .collect::<Vec<_>>(),
            )
        } else if in_map && v.is_f64() {
            let f = v.as_f64().unwrap_or_default();
            #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
            let i = f as i64;
            #[allow(clippy::cast_precision_loss)]
            if f.is_finite() && i as f64 == f {
                Value::from(i)
            } else {
                v.clone()
            }
        } else if plain_dates && let Some((date, _)) = v.as_str().and_then(marked_date) {
            Value::from(date)
        } else {
            v.clone()
        }
    }
    walk(v, false, plain_dates)
}

// ── YAML (yaml.v2) ──

/// yaml.v2's order of map keys.
fn yaml_key_order(a: &str, b: &str) -> Ordering {
    let ar: Vec<char> = a.chars().collect();
    let br: Vec<char> = b.chars().collect();
    let mut i = 0;
    while i < ar.len() && i < br.len() {
        if ar[i] == br[i] {
            i += 1;
            continue;
        }
        let al = ar[i].is_alphabetic();
        let bl = br[i].is_alphabetic();
        if al && bl {
            return ar[i].cmp(&br[i]);
        }
        if al || bl {
            return if bl {
                Ordering::Less
            } else {
                Ordering::Greater
            };
        }
        let (mut an, mut bn) = (0_i64, 0_i64);
        if ar[i] == '0' || br[i] == '0' {
            let mut j = i;
            while j > 0 && ar[j - 1].is_ascii_digit() {
                j -= 1;
                if ar[j] != '0' {
                    an = 1;
                    bn = 1;
                    break;
                }
            }
        }
        let mut ai = i;
        while let Some(d) = ar.get(ai).and_then(|c| c.to_digit(10)) {
            an = an.wrapping_mul(10).wrapping_add(i64::from(d));
            ai += 1;
        }
        let mut bi = i;
        while let Some(d) = br.get(bi).and_then(|c| c.to_digit(10)) {
            bn = bn.wrapping_mul(10).wrapping_add(i64::from(d));
            bi += 1;
        }
        if an != bn {
            return an.cmp(&bn);
        }
        if ai != bi {
            return ai.cmp(&bi);
        }
        return ar[i].cmp(&br[i]);
    }
    ar.len().cmp(&br.len())
}

fn yaml_entries(v: &Value) -> Vec<(String, &Value)> {
    let mut pairs: Vec<(String, &Value)> = v
        .as_map()
        .map(|m| entries(m).map(|(k, v)| (k.into_owned(), v)).collect())
        .unwrap_or_default();
    pairs.sort_by(|a, b| yaml_key_order(&a.0, &b.0));
    pairs
}

static YAML_FLOAT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[-+]?(\.[0-9]+|[0-9]+(\.[0-9]*)?)([eE][-+]?[0-9]+)?$").expect("valid pattern")
});
static BASE60_FLOAT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[-+]?[0-9][0-9_]*(?::[0-5]?[0-9])+(?:\.[0-9_]*)?$").expect("valid pattern")
});
static TIMESTAMP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^[0-9]{4}-[0-9]{1,2}-[0-9]{1,2}(?:(?:[Tt][0-9]{1,2}:[0-9]{1,2}:[0-9]{1,2}(?:\.[0-9]+)?(?:Z|[-+][0-9]{2}:[0-9]{2}))|(?: [0-9]{1,2}:[0-9]{1,2}:[0-9]{1,2}(?:\.[0-9]+)?))?$",
    )
    .expect("valid pattern")
});

/// Whether yaml.v2 reads the plain scalar `s` back as a string (not null, a bool, a number or
/// a timestamp).
fn yaml_resolves_to_string(s: &str) -> bool {
    const SPECIAL: &[&str] = &[
        "", "~", "null", "Null", "NULL", "y", "Y", "yes", "Yes", "YES", "on", "On", "ON", "true",
        "True", "TRUE", "n", "N", "no", "No", "NO", "off", "Off", "OFF", "false", "False", "FALSE",
        ".nan", ".NaN", ".NAN", ".inf", ".Inf", ".INF", "+.inf", "+.Inf", "+.INF", "-.inf",
        "-.Inf", "-.INF", "<<",
    ];
    if SPECIAL.contains(&s) {
        return false;
    }
    let Some(first) = s.bytes().next() else {
        return false;
    };
    match first {
        b'.' => s.parse::<f64>().is_err(),
        b'+' | b'-' | b'0'..=b'9' => {
            if TIMESTAMP.is_match(s) {
                return false;
            }
            let plain = s.replace('_', "");
            let number = go_parse_int(&plain)
                || YAML_FLOAT.is_match(&plain)
                || binary_int(&plain)
                || BASE60_FLOAT.is_match(s);
            !number
        }
        _ => true,
    }
}

/// Go's `strconv.ParseInt(s, 0, 64)` or `ParseUint` succeeds.
fn go_parse_int(s: &str) -> bool {
    let (neg, body) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let lower = body.to_ascii_lowercase();
    let (radix, digits) = if let Some(d) = lower.strip_prefix("0x") {
        (16, d)
    } else if let Some(d) = lower.strip_prefix("0b") {
        (2, d)
    } else if let Some(d) = lower.strip_prefix("0o") {
        (8, d)
    } else if lower.len() > 1 && lower.starts_with('0') {
        (8, &lower[1..])
    } else {
        (10, lower.as_str())
    };
    if digits.is_empty() {
        return false;
    }
    match u64::from_str_radix(digits, radix) {
        Ok(n) if neg => n <= 1 << 63,
        Ok(_) => true,
        Err(_) => false,
    }
}

fn binary_int(s: &str) -> bool {
    let body = s.strip_prefix('-').unwrap_or(s);
    body.strip_prefix("0b")
        .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b == b'0' || b == b'1'))
}

/// What yaml.v2's emitter allows for a scalar (`yaml_emitter_analyze_scalar`).
struct ScalarAnalysis {
    block_plain: bool,
    single_quoted: bool,
    block: bool,
}

fn yaml_printable(c: char) -> bool {
    matches!(c, '\n' | '\u{20}'..='\u{7e}' | '\u{85}' | '\u{a0}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..)
        && c != '\u{feff}'
}

fn analyze(s: &str) -> ScalarAnalysis {
    if s.is_empty() {
        return ScalarAnalysis {
            block_plain: false,
            single_quoted: true,
            block: false,
        };
    }
    let chars: Vec<char> = s.chars().collect();
    let blankz = |i: usize| {
        chars.get(i).is_none_or(|c| {
            matches!(
                c,
                ' ' | '\t' | '\r' | '\n' | '\u{85}' | '\u{2028}' | '\u{2029}'
            )
        })
    };
    let is_break = |c: char| matches!(c, '\r' | '\n' | '\u{85}' | '\u{2028}' | '\u{2029}');
    // the block context only: flow indicators (`,`, `[`…) do not matter there
    let mut block_indicators = false;
    let (mut line_breaks, mut special) = (false, false);
    let (mut leading_space, mut leading_break, mut trailing_space, mut trailing_break) =
        (false, false, false, false);
    let (mut break_space, mut space_break) = (false, false);
    let (mut previous_space, mut previous_break) = (false, false);
    if s.starts_with("---") || s.starts_with("...") {
        block_indicators = true;
    }
    let mut preceded_by_whitespace = true;
    let mut followed_by_whitespace = blankz(1);
    let last = chars.len() - 1;
    for (i, &c) in chars.iter().enumerate() {
        if i == 0 {
            match c {
                '#' | ',' | '[' | ']' | '{' | '}' | '&' | '*' | '!' | '|' | '>' | '\'' | '"'
                | '%' | '@' | '`' => block_indicators = true,
                '?' | ':' | '-' if followed_by_whitespace => block_indicators = true,
                _ => {}
            }
        } else if (c == ':' && followed_by_whitespace) || (c == '#' && preceded_by_whitespace) {
            block_indicators = true;
        }
        if !yaml_printable(c) {
            special = true;
        }
        if is_break(c) {
            line_breaks = true;
        }
        if c == ' ' {
            if i == 0 {
                leading_space = true;
            }
            if i == last {
                trailing_space = true;
            }
            if previous_break {
                break_space = true;
            }
            previous_space = true;
            previous_break = false;
        } else if is_break(c) {
            if i == 0 {
                leading_break = true;
            }
            if i == last {
                trailing_break = true;
            }
            if previous_space {
                space_break = true;
            }
            previous_space = false;
            previous_break = true;
        } else {
            previous_space = false;
            previous_break = false;
        }
        preceded_by_whitespace = blankz(i);
        followed_by_whitespace = blankz(i + 2);
    }
    let mut a = ScalarAnalysis {
        block_plain: true,
        single_quoted: true,
        block: true,
    };
    if leading_space || leading_break || trailing_space || trailing_break {
        a.block_plain = false;
    }
    if trailing_space {
        a.block = false;
    }
    if break_space {
        a.block_plain = false;
        a.single_quoted = false;
    }
    if space_break || special {
        a.block_plain = false;
        a.single_quoted = false;
        a.block = false;
    }
    if line_breaks || block_indicators {
        a.block_plain = false;
    }
    a
}

fn yaml_double_quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\0' => out.push_str("\\0"),
            '\u{7}' => out.push_str("\\a"),
            '\u{8}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{b}' => out.push_str("\\v"),
            '\u{c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            '\u{1b}' => out.push_str("\\e"),
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{85}' => out.push_str("\\N"),
            '\u{a0}' => out.push_str("\\_"),
            '\u{2028}' => out.push_str("\\L"),
            '\u{2029}' => out.push_str("\\P"),
            c if !yaml_printable(c) || c == '\u{feff}' => {
                let n = u32::from(c);
                let _ = if n <= 0xff {
                    write!(out, "\\x{n:02X}")
                } else if n <= 0xffff {
                    write!(out, "\\u{n:04X}")
                } else {
                    write!(out, "\\U{n:08X}")
                };
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A YAML string scalar; `indent` is the indentation of a literal block's lines, `None` for a
/// key (a simple key is never a block).
fn yaml_string(s: &str, indent: Option<usize>) -> String {
    if let Some((date, offset)) = marked_date(s) {
        // An offset date-time is Go's `time.Time` (written plain); a local date is text.
        return if offset {
            date.to_owned()
        } else {
            yaml_string(date, indent)
        };
    }
    let a = analyze(s);
    if s.contains('\n') {
        if let Some(indent) = indent
            && a.block
        {
            return yaml_literal(s, indent);
        }
        return yaml_double_quoted(s);
    }
    if !yaml_resolves_to_string(s) {
        return yaml_double_quoted(s);
    }
    if a.block_plain {
        s.to_owned()
    } else if a.single_quoted {
        format!("'{}'", s.replace('\'', "''"))
    } else {
        yaml_double_quoted(s)
    }
}

fn yaml_literal(s: &str, indent: usize) -> String {
    let mut out = String::from("|");
    if s.starts_with([' ', '\n']) {
        out.push('2');
    }
    let trailing = s.len() - s.trim_end_matches('\n').len();
    match trailing {
        0 => out.push('-'),
        1 => {}
        _ => out.push('+'),
    }
    let pad = " ".repeat(indent);
    for line in s.trim_end_matches('\n').split('\n') {
        out.push('\n');
        if !line.is_empty() {
            out.push_str(&pad);
            out.push_str(line);
        }
    }
    for _ in 1..trailing {
        out.push('\n');
    }
    out
}

/// A float as Go's `strconv.FormatFloat(f, 'g', -1, 64)`, in YAML's spelling of the
/// non-finite values.
fn yaml_float(f: f64) -> String {
    if f.is_nan() {
        return ".nan".to_owned();
    }
    if f.is_infinite() {
        return if f > 0.0 { ".inf" } else { "-.inf" }.to_owned();
    }
    if f == 0.0 {
        return if f.is_sign_negative() { "-0" } else { "0" }.to_owned();
    }
    let e = format!("{f:e}");
    let (mantissa, exp) = e.split_once('e').unwrap_or((&e, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    // shortest `%g`: the exponent form below 1e-4 and from 1e6
    if !(-4..6).contains(&exp) {
        let sign = if exp < 0 { '-' } else { '+' };
        return format!("{mantissa}e{sign}{:02}", exp.abs());
    }
    format!("{f}")
}

fn yaml_scalar(v: &Value, indent: Option<usize>) -> String {
    if v.is_none() || v.is_undefined() {
        return "null".to_owned();
    }
    if let Some(s) = v.as_str() {
        return yaml_string(s, indent);
    }
    if v.is_f64() {
        return yaml_float(v.as_f64().unwrap_or_default());
    }
    v.to_string()
}

fn yaml_inline(v: &Value, indent: usize) -> String {
    if v.is_map() {
        "{}".to_owned()
    } else if v.is_array() {
        "[]".to_owned()
    } else {
        yaml_scalar(v, Some(indent))
    }
}

fn yaml_node(v: &Value, depth: usize, out: &mut String) {
    let pad = "  ".repeat(depth);
    if v.is_map() {
        for (k, item) in yaml_entries(v) {
            let key = yaml_string(&k, None);
            if item.as_map().is_some_and(|m| !m.is_empty()) {
                let _ = writeln!(out, "{pad}{key}:");
                yaml_node(item, depth + 1, out);
            } else if item.as_array().is_some_and(|a| !a.is_empty()) {
                let _ = writeln!(out, "{pad}{key}:");
                yaml_node(item, depth, out);
            } else {
                let _ = writeln!(out, "{pad}{key}: {}", yaml_inline(item, (depth + 1) * 2));
            }
        }
    } else if let Some(a) = v.as_array() {
        for item in a {
            let nests = item.as_map().is_some_and(|m| !m.is_empty())
                || item.as_array().is_some_and(|a| !a.is_empty());
            if nests {
                // the first line of the nested block follows the dash
                let mut nested = String::new();
                yaml_node(item, depth + 1, &mut nested);
                let _ = write!(out, "{pad}- {}", nested.trim_start());
            } else {
                let _ = writeln!(out, "{pad}- {}", yaml_inline(item, (depth + 1) * 2));
            }
        }
    } else {
        let _ = writeln!(out, "{pad}{}", yaml_scalar(v, Some((depth + 1) * 2)));
    }
}

/// A YAML document.
pub(super) fn yaml(v: &Value) -> String {
    let mut out = String::new();
    yaml_node(v, 0, &mut out);
    out
}

// ── TOML (go-toml v2) ──

/// go-toml v2's `needsQuoting`: a `'`, a line break or a control character other than tab.
fn toml_needs_quoting(s: &str) -> bool {
    s.bytes()
        .any(|b| b == b'\'' || b == 0x7f || (b < 0x20 && b != b'\t'))
}

fn toml_quoted(s: &str) -> String {
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

fn toml_string(s: &str) -> String {
    if toml_needs_quoting(s) {
        toml_quoted(s)
    } else {
        format!("'{s}'")
    }
}

/// A TOML key: bare when it can be, else literal, else quoted.
fn toml_key(k: &str) -> String {
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

fn toml_entries(v: &Value) -> Vec<(String, &Value)> {
    let mut pairs: Vec<(String, &Value)> = v
        .as_map()
        .map(|m| entries(m).map(|(k, v)| (k.into_owned(), v)).collect())
        .unwrap_or_default();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    pairs
}

fn toml_inline(v: &Value) -> TeraResult<String> {
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

fn is_table_array(v: &Value) -> bool {
    v.as_array()
        .is_some_and(|a| !a.is_empty() && a.iter().all(Value::is_map))
}

/// A TOML document: plain keys first, then tables (indented two spaces per level) and arrays of
/// tables.
pub(super) fn toml(v: &Value) -> TeraResult<String> {
    if !v.is_map() {
        return Err(tera::Error::message(
            "remarshal: a TOML document is a table",
        ));
    }
    let mut out = String::new();
    toml_table(v, &[], &mut out)?;
    Ok(out)
}

fn toml_table(v: &Value, path: &[String], out: &mut String) -> TeraResult<()> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaml_scalars_as_yaml_v2() {
        let s = |x: &str| yaml_string(x, Some(2));
        assert_eq!(s(":resourceDir/_gen"), ":resourceDir/_gen");
        assert_eq!(s("{{ .Date }}"), "'{{ .Date }}'");
        assert_eq!(s("a: b"), "'a: b'");
        assert_eq!(s("true"), "\"true\"");
        assert_eq!(s("1.5"), "\"1.5\"");
        assert_eq!(s("0x1F"), "\"0x1F\"");
        assert_eq!(s("2023-01-01"), "\"2023-01-01\"");
        assert_eq!(s("1:20"), "\"1:20\"");
        assert_eq!(s(""), "\"\"");
        assert_eq!(s("- x"), "'- x'");
        assert_eq!(s("-x"), "-x");
        assert_eq!(s("it's"), "it's");
        assert_eq!(s("a #b"), "'a #b'");
        assert_eq!(s("a\tb"), "\"a\\tb\"");
        assert_eq!(s("l1\nl2"), "|-\n  l1\n  l2");
        assert_eq!(s("l1\nl2\n"), "|\n  l1\n  l2");
        assert_eq!(yaml_string("l1\nl2", None), "\"l1\\nl2\"");
    }

    #[test]
    fn yaml_key_order_is_natural() {
        let mut keys = vec!["b", "a10", "a9", "_x", "Z", "a"];
        keys.sort_by(|a, b| yaml_key_order(a, b));
        assert_eq!(keys, ["_x", "Z", "a", "a9", "a10", "b"]);
    }

    #[test]
    fn yaml_floats_as_go() {
        assert_eq!(yaml_float(1.5), "1.5");
        assert_eq!(yaml_float(1e21), "1e+21");
        assert_eq!(yaml_float(1_234_567.5), "1.2345675e+06");
        assert_eq!(yaml_float(123_456.5), "123456.5");
        assert_eq!(yaml_float(1e-5), "1e-05");
        assert_eq!(yaml_float(0.0001), "0.0001");
    }

    #[test]
    fn toml_strings_as_go_toml() {
        assert_eq!(toml_string("red"), "'red'");
        assert_eq!(toml_string(r"a\.b"), r"'a\.b'");
        assert_eq!(toml_string("it's"), "\"it's\"");
        assert_eq!(toml_string("a\nb"), "\"a\\nb\"");
        assert_eq!(toml_key("text/html"), "'text/html'");
        assert_eq!(toml_key("a-b_c"), "a-b_c");
        assert_eq!(toml_key("it's"), "\"it's\"");
    }
}
