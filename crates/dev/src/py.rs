//! The data model and the text rules the harness's recorded data depends on.
//!
//! The golden manifests (`testdata/golden/`) and the baselines' diff fingerprints
//! (`testdata/baselines/`) were written by the harness's first implementation, in Python. To
//! produce the same bytes, this module reproduces what that data went through there: JSON
//! values with Python's types (`1` and `1.0` differ, `true == 1`), `json.dumps` with sorted keys,
//! `repr`, float formatting and rounding, `sum` of floats, and the whitespace, `split`, `strip`
//! and `splitlines` of `str`. The rules follow CPython 3.14.7 (`Lib/json`, `Objects/floatobject.c`,
//! `Objects/unicodeobject.c`, `Python/bltinmodule.c`; PSF-2.0, THIRD_PARTY/cpython/LICENSE;
//! PROVENANCE.md).

use std::fmt::Write as _;

use indexmap::IndexMap;
use unicode_properties::{GeneralCategory, GeneralCategoryGroup, UnicodeGeneralCategory};

/// A dictionary: insertion order, as Python's.
pub type Dict = IndexMap<String, Py>;

/// A JSON value with Python's types.
#[derive(Clone, Debug)]
pub enum Py {
    None,
    Bool(bool),
    Int(i64),
    /// An integer beyond `i64`, as its decimal text.
    BigInt(String),
    Float(f64),
    Str(String),
    List(Vec<Py>),
    /// A tuple: a list in JSON, `(…)` in `repr`.
    Tuple(Vec<Py>),
    Dict(Dict),
}

/// A [`Py::Dict`] from `key => value` pairs.
#[macro_export]
macro_rules! dict {
    () => { $crate::py::Py::Dict($crate::py::Dict::new()) };
    ($($k:expr => $v:expr),+ $(,)?) => {{
        let mut d = $crate::py::Dict::new();
        $( d.insert(::std::string::String::from($k), $crate::py::Py::from($v)); )+
        $crate::py::Py::Dict(d)
    }};
}

impl Py {
    /// `d.get(key)` of a dict; `None` for other values.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Py> {
        match self {
            Py::Dict(d) => d.get(key),
            _ => None,
        }
    }

    /// `d.get(key)`, with `None` for a missing key (as `Py::None`).
    #[must_use]
    pub fn get_or_none(&self, key: &str) -> &Py {
        self.get(key).unwrap_or(&Py::None)
    }

    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Py::Str(s) => Some(s),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_dict(&self) -> Option<&Dict> {
        match self {
            Py::Dict(d) => Some(d),
            _ => None,
        }
    }

    /// The dict itself; an empty one for other values.
    #[must_use]
    pub fn into_dict(self) -> Dict {
        match self {
            Py::Dict(d) => d,
            _ => Dict::new(),
        }
    }

    #[must_use]
    pub fn as_dict_mut(&mut self) -> Option<&mut Dict> {
        match self {
            Py::Dict(d) => Some(d),
            _ => None,
        }
    }

    /// The items of a list or tuple.
    #[must_use]
    pub fn as_list(&self) -> Option<&[Py]> {
        match self {
            Py::List(v) | Py::Tuple(v) => Some(v),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Py::Int(i) => Some(*i),
            Py::Bool(b) => Some(i64::from(*b)),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            #[allow(clippy::cast_precision_loss)]
            Py::Int(i) => Some(*i as f64),
            Py::Float(f) => Some(*f),
            Py::Bool(b) => Some(f64::from(u8::from(*b))),
            _ => None,
        }
    }

    /// Python's truth value.
    #[must_use]
    pub fn truthy(&self) -> bool {
        match self {
            Py::None => false,
            Py::Bool(b) => *b,
            Py::Int(i) => *i != 0,
            Py::BigInt(_) => true,
            Py::Float(f) => *f != 0.0,
            Py::Str(s) => !s.is_empty(),
            Py::List(v) | Py::Tuple(v) => !v.is_empty(),
            Py::Dict(d) => !d.is_empty(),
        }
    }

    #[must_use]
    pub fn is_none(&self) -> bool {
        matches!(self, Py::None)
    }
}

/// Python's `==`: numbers by value whatever their type (`True == 1 == 1.0`), lists and tuples
/// item by item (a list never equals a tuple), dicts by their items.
impl PartialEq for Py {
    fn eq(&self, other: &Py) -> bool {
        use Py::{BigInt, Bool, Dict, Float, Int, List, None, Str, Tuple};
        match (self, other) {
            (None, None) => true,
            (Str(a), Str(b)) | (BigInt(a), BigInt(b)) => a == b,
            (List(a), List(b)) | (Tuple(a), Tuple(b)) => a == b,
            (Dict(a), Dict(b)) => a.len() == b.len() && a.iter().all(|(k, v)| b.get(k) == Some(v)),
            (Bool(_) | Int(_), Bool(_) | Int(_)) => self.as_i64() == other.as_i64(),
            (Bool(_) | Int(_) | Float(_), Bool(_) | Int(_) | Float(_)) => {
                self.as_f64() == other.as_f64()
            }
            _ => false,
        }
    }
}

impl From<&str> for Py {
    fn from(s: &str) -> Py {
        Py::Str(s.to_owned())
    }
}
impl From<String> for Py {
    fn from(s: String) -> Py {
        Py::Str(s)
    }
}
impl From<&String> for Py {
    fn from(s: &String) -> Py {
        Py::Str(s.clone())
    }
}
impl From<bool> for Py {
    fn from(b: bool) -> Py {
        Py::Bool(b)
    }
}
impl From<i64> for Py {
    fn from(i: i64) -> Py {
        Py::Int(i)
    }
}
impl From<i32> for Py {
    fn from(i: i32) -> Py {
        Py::Int(i64::from(i))
    }
}
impl From<usize> for Py {
    fn from(i: usize) -> Py {
        Py::Int(i64::try_from(i).expect("a count fits i64"))
    }
}
impl From<u32> for Py {
    fn from(i: u32) -> Py {
        Py::Int(i64::from(i))
    }
}
impl From<f64> for Py {
    fn from(f: f64) -> Py {
        Py::Float(f)
    }
}
impl From<Vec<Py>> for Py {
    fn from(v: Vec<Py>) -> Py {
        Py::List(v)
    }
}
impl From<Vec<String>> for Py {
    fn from(v: Vec<String>) -> Py {
        Py::List(v.into_iter().map(Py::Str).collect())
    }
}
impl From<Dict> for Py {
    fn from(d: Dict) -> Py {
        Py::Dict(d)
    }
}
impl<T: Into<Py>> From<Option<T>> for Py {
    fn from(o: Option<T>) -> Py {
        o.map_or(Py::None, Into::into)
    }
}
impl From<&Py> for Py {
    fn from(v: &Py) -> Py {
        v.clone()
    }
}

// ---------------------------------------------------------------------------------------------
// JSON

/// `json.loads`: objects keep the document's key order (a repeated key keeps its first place
/// and its last value); a number with a fraction or an exponent is a float; `NaN`, `Infinity`
/// and `-Infinity` are numbers; control characters in strings and a leading byte order mark
/// are errors. (A lone surrogate escape, which Python keeps, becomes U+FFFD.)
///
/// # Errors
/// A message with the byte position when `text` is not JSON.
pub fn loads(text: &str) -> Result<Py, String> {
    if text.starts_with('\u{feff}') {
        return Err("Unexpected UTF-8 BOM (decode using utf-8-sig)".into());
    }
    let mut p = JsonParser {
        s: text.as_bytes(),
        text,
        i: 0,
    };
    p.ws();
    let v = p.value(0)?;
    p.ws();
    if p.i != p.s.len() {
        return Err(format!("Extra data at {}", p.i));
    }
    Ok(v)
}

struct JsonParser<'a> {
    s: &'a [u8],
    text: &'a str,
    i: usize,
}

impl JsonParser<'_> {
    fn ws(&mut self) {
        while matches!(self.s.get(self.i), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }

    fn err<T>(&self, what: &str) -> Result<T, String> {
        Err(format!("{what} at {}", self.i))
    }

    fn eat(&mut self, lit: &str) -> bool {
        if self.s[self.i..].starts_with(lit.as_bytes()) {
            self.i += lit.len();
            true
        } else {
            false
        }
    }

    fn value(&mut self, depth: usize) -> Result<Py, String> {
        if depth > 900 {
            return self.err("Too deeply nested");
        }
        match self.s.get(self.i) {
            Some(b'"') => self.string().map(Py::Str),
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            _ if self.eat("null") => Ok(Py::None),
            _ if self.eat("true") => Ok(Py::Bool(true)),
            _ if self.eat("false") => Ok(Py::Bool(false)),
            _ => {
                if let Some(n) = self.number() {
                    return Ok(n);
                }
                if self.eat("NaN") {
                    Ok(Py::Float(f64::NAN))
                } else if self.eat("Infinity") {
                    Ok(Py::Float(f64::INFINITY))
                } else if self.eat("-Infinity") {
                    Ok(Py::Float(f64::NEG_INFINITY))
                } else {
                    self.err("Expecting value")
                }
            }
        }
    }

    /// `(-?(?:0|[1-9][0-9]*))(\.[0-9]+)?([eE][-+]?[0-9]+)?`
    fn number(&mut self) -> Option<Py> {
        let s = self.s;
        let start = self.i;
        let mut j = start;
        if s.get(j) == Some(&b'-') {
            j += 1;
        }
        let digits = |k: usize| s[k..].iter().take_while(|c| c.is_ascii_digit()).count();
        match s.get(j) {
            Some(b'0') => j += 1,
            Some(b'1'..=b'9') => j += digits(j),
            _ => return None,
        }
        let mut float = false;
        if s.get(j) == Some(&b'.') && digits(j + 1) > 0 {
            j += 1 + digits(j + 1);
            float = true;
        }
        if matches!(s.get(j), Some(b'e' | b'E')) {
            let k = if matches!(s.get(j + 1), Some(b'-' | b'+')) {
                j + 2
            } else {
                j + 1
            };
            if digits(k) > 0 {
                j = k + digits(k);
                float = true;
            }
        }
        self.i = j;
        let text = &self.text[start..j];
        Some(if float {
            Py::Float(text.parse().unwrap_or(f64::NAN))
        } else {
            text.parse()
                .map_or_else(|_| Py::BigInt(text.to_owned()), Py::Int)
        })
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let h = self
            .s
            .get(self.i..self.i + 4)
            .filter(|h| h.iter().all(u8::is_ascii_hexdigit));
        let Some(h) = h else {
            return self.err("Invalid \\uXXXX escape");
        };
        let v = u32::from_str_radix(std::str::from_utf8(h).expect("hex digits"), 16)
            .expect("hex digits");
        self.i += 4;
        Ok(v)
    }

    fn string(&mut self) -> Result<String, String> {
        self.i += 1;
        let mut out = String::new();
        loop {
            let run = self.s[self.i..]
                .iter()
                .take_while(|&&c| c != b'"' && c != b'\\' && c >= 0x20)
                .count();
            out.push_str(&self.text[self.i..self.i + run]);
            self.i += run;
            match self.s.get(self.i) {
                None => return self.err("Unterminated string"),
                Some(b'"') => {
                    self.i += 1;
                    return Ok(out);
                }
                Some(b'\\') => {
                    self.i += 1;
                    let Some(&e) = self.s.get(self.i) else {
                        return self.err("Unterminated string");
                    };
                    self.i += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let mut c = self.hex4()?;
                            if (0xd800..0xdc00).contains(&c) && self.s[self.i..].starts_with(b"\\u")
                            {
                                let save = self.i;
                                self.i += 2;
                                let low = self.hex4()?;
                                if (0xdc00..0xe000).contains(&low) {
                                    c = 0x10000 + ((c - 0xd800) << 10) + (low - 0xdc00);
                                } else {
                                    self.i = save;
                                }
                            }
                            out.push(char::from_u32(c).unwrap_or('\u{FFFD}'));
                        }
                        _ => return self.err("Invalid \\escape"),
                    }
                }
                Some(_) => return self.err("Invalid control character"),
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<Py, String> {
        self.i += 1;
        let mut d = Dict::new();
        self.ws();
        if self.s.get(self.i) == Some(&b'}') {
            self.i += 1;
            return Ok(Py::Dict(d));
        }
        loop {
            if self.s.get(self.i) != Some(&b'"') {
                return self.err("Expecting property name enclosed in double quotes");
            }
            let k = self.string()?;
            self.ws();
            if self.s.get(self.i) != Some(&b':') {
                return self.err("Expecting ':' delimiter");
            }
            self.i += 1;
            self.ws();
            let v = self.value(depth + 1)?;
            d.insert(k, v);
            self.ws();
            match self.s.get(self.i) {
                Some(b',') => {
                    self.i += 1;
                    self.ws();
                }
                Some(b'}') => {
                    self.i += 1;
                    return Ok(Py::Dict(d));
                }
                _ => return self.err("Expecting ',' delimiter"),
            }
        }
    }

    fn array(&mut self, depth: usize) -> Result<Py, String> {
        self.i += 1;
        let mut items = Vec::new();
        self.ws();
        if self.s.get(self.i) == Some(&b']') {
            self.i += 1;
            return Ok(Py::List(items));
        }
        loop {
            items.push(self.value(depth + 1)?);
            self.ws();
            match self.s.get(self.i) {
                Some(b',') => {
                    self.i += 1;
                    self.ws();
                }
                Some(b']') => {
                    self.i += 1;
                    return Ok(Py::List(items));
                }
                _ => return self.err("Expecting ',' delimiter"),
            }
        }
    }
}

/// `json.dumps(v, sort_keys=True, ensure_ascii=…)` with the default separators (`, `, `: `).
#[must_use]
pub fn dumps(v: &Py, ensure_ascii: bool) -> String {
    let mut out = String::new();
    dump(&mut out, v, ensure_ascii, None);
    out
}

/// `json.dumps(v, sort_keys=True, ensure_ascii=False, indent=0)`: every item on its own line,
/// `,` between items.
#[must_use]
pub fn dumps_indent0(v: &Py) -> String {
    let mut out = String::new();
    dump(&mut out, v, false, Some(0));
    out
}

fn dump(out: &mut String, v: &Py, ascii: bool, indent: Option<usize>) {
    match v {
        Py::None => out.push_str("null"),
        Py::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Py::Int(i) => {
            let _ = write!(out, "{i}");
        }
        Py::BigInt(s) => out.push_str(s),
        Py::Float(f) => out.push_str(&json_float(*f)),
        Py::Str(s) => out.push_str(&json_str(s, ascii)),
        Py::List(items) | Py::Tuple(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            let level = indent.map(|l| l + 1);
            for (n, x) in items.iter().enumerate() {
                if n > 0 {
                    out.push_str(if level.is_some() { "," } else { ", " });
                }
                if level.is_some() {
                    out.push('\n');
                }
                dump(out, x, ascii, level);
            }
            if level.is_some() {
                out.push('\n');
            }
            out.push(']');
        }
        Py::Dict(d) => {
            if d.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            let level = indent.map(|l| l + 1);
            let mut keys: Vec<&String> = d.keys().collect();
            keys.sort();
            for (n, k) in keys.into_iter().enumerate() {
                if n > 0 {
                    out.push_str(if level.is_some() { "," } else { ", " });
                }
                if level.is_some() {
                    out.push('\n');
                }
                out.push_str(&json_str(k, ascii));
                out.push_str(": ");
                dump(out, &d[k], ascii, level);
            }
            if level.is_some() {
                out.push('\n');
            }
            out.push('}');
        }
    }
}

/// A JSON string as `json.dumps` writes it: `ensure_ascii` escapes everything outside
/// printable ASCII (`\uXXXX`, surrogate pairs above the BMP); otherwise only `"`, `\` and the
/// C0 controls are escaped.
#[must_use]
pub fn json_str(s: &str, ensure_ascii: bool) -> String {
    let mut out = String::with_capacity(s.len() + 2);
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
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c if ensure_ascii && !(' '..='~').contains(&c) => {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    let _ = write!(out, "\\u{unit:04x}");
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_float(f: f64) -> String {
    if f.is_nan() {
        "NaN".to_owned()
    } else if f.is_infinite() {
        if f > 0.0 { "Infinity" } else { "-Infinity" }.to_owned()
    } else {
        float_repr(f)
    }
}

// ---------------------------------------------------------------------------------------------
// Numbers

/// `repr(f)` of a finite float: the shortest text that reads back as `f`, fixed notation for
/// 1e-4 <= |f| < 1e16 (with `.0` when integral), else `<digits>e<sign><2+ digits>`.
#[must_use]
pub fn float_repr(f: f64) -> String {
    if f.is_nan() {
        return "nan".to_owned();
    }
    if f.is_infinite() {
        return if f > 0.0 { "inf" } else { "-inf" }.to_owned();
    }
    // Rust's Debug form has the same digits and switches to exponents at the same bounds.
    let s = format!("{f:?}");
    match s.split_once('e') {
        None => s,
        Some((mantissa, exp)) => {
            let (sign, digits) = match exp.strip_prefix('-') {
                Some(d) => ('-', d),
                None => ('+', exp),
            };
            format!("{mantissa}e{sign}{digits:0>2}")
        }
    }
}

/// `round(f, digits)`: the float nearest to `f` rounded half-even on its exact binary value
/// (as `format!("{:.N}")` rounds).
#[must_use]
pub fn round(f: f64, digits: usize) -> f64 {
    format!("{f:.digits$}").parse().unwrap_or(f)
}

/// `sum(values)` of floats as CPython (3.12 and later) adds them: Neumaier's compensated sum.
#[must_use]
pub fn sum_floats(values: impl IntoIterator<Item = f64>) -> f64 {
    let (mut total, mut c) = (0.0f64, 0.0f64);
    for x in values {
        let t = total + x;
        if total.abs() >= x.abs() {
            c += (total - t) + x;
        } else {
            c += (x - t) + total;
        }
        total = t;
    }
    if c != 0.0 && c.is_finite() {
        total += c;
    }
    total
}

// ---------------------------------------------------------------------------------------------
// repr and str

/// `repr(v)`.
#[must_use]
pub fn repr(v: &Py) -> String {
    match v {
        Py::None => "None".to_owned(),
        Py::Bool(b) => if *b { "True" } else { "False" }.to_owned(),
        Py::Int(i) => i.to_string(),
        Py::BigInt(s) => s.clone(),
        Py::Float(f) => float_repr(*f),
        Py::Str(s) => repr_str(s),
        Py::List(items) => format!("[{}]", join_repr(items)),
        Py::Tuple(items) if items.len() == 1 => format!("({},)", repr(&items[0])),
        Py::Tuple(items) => format!("({})", join_repr(items)),
        Py::Dict(d) => {
            let items: Vec<String> = d
                .iter()
                .map(|(k, v)| format!("{}: {}", repr_str(k), repr(v)))
                .collect();
            format!("{{{}}}", items.join(", "))
        }
    }
}

fn join_repr(items: &[Py]) -> String {
    items.iter().map(repr).collect::<Vec<_>>().join(", ")
}

/// `str(v)`: a string itself, anything else its `repr`.
#[must_use]
pub fn str_of(v: &Py) -> String {
    match v {
        Py::Str(s) => s.clone(),
        _ => repr(v),
    }
}

/// `repr(s)` of a string: single quotes unless it has a `'` and no `"`; `\\`, the quote, `\t`,
/// `\n`, `\r` and non-printable characters escaped.
#[must_use]
pub fn repr_str(s: &str) -> String {
    let quote = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if is_printable(c) => out.push(c),
            c if (c as u32) <= 0xff => {
                let _ = write!(out, "\\x{:02x}", c as u32);
            }
            c if (c as u32) <= 0xffff => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => {
                let _ = write!(out, "\\U{:08x}", c as u32);
            }
        }
    }
    out.push(quote);
    out
}

/// `str.isprintable` of one character: not a control, format, surrogate, private-use,
/// unassigned or separator character, except the space.
#[must_use]
pub fn is_printable(c: char) -> bool {
    if c == ' ' {
        return true;
    }
    if c.is_ascii() {
        return !c.is_ascii_control();
    }
    !matches!(
        c.general_category_group(),
        GeneralCategoryGroup::Other | GeneralCategoryGroup::Separator
    )
}

// ---------------------------------------------------------------------------------------------
// str

/// `str.isspace` of one character: Unicode's White_Space characters plus U+001C to U+001F
/// (bidirectional class B or S).
#[must_use]
pub fn is_space(c: char) -> bool {
    matches!(c, '\u{1c}'..='\u{1f}') || c.is_whitespace()
}

/// `str.split()`: the runs of non-whitespace.
#[must_use]
pub fn split_ws(s: &str) -> Vec<&str> {
    s.split(is_space).filter(|w| !w.is_empty()).collect()
}

/// `" ".join(s.split())`.
#[must_use]
pub fn collapse_ws(s: &str) -> String {
    split_ws(s).join(" ")
}

/// `str.strip()`.
#[must_use]
pub fn strip(s: &str) -> &str {
    s.trim_matches(is_space)
}

/// `str.splitlines()`: lines end at `\n`, `\r`, `\r\n`, `\v`, `\f`, U+001C to U+001E, U+0085,
/// U+2028 and U+2029.
#[must_use]
pub fn splitlines(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut it = s.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        if matches!(
            c,
            '\n' | '\r'
                | '\u{b}'
                | '\u{c}'
                | '\u{1c}'
                | '\u{1d}'
                | '\u{1e}'
                | '\u{85}'
                | '\u{2028}'
                | '\u{2029}'
        ) {
            out.push(&s[start..i]);
            let mut end = i + c.len_utf8();
            if c == '\r' && it.peek().is_some_and(|&(_, n)| n == '\n') {
                it.next();
                end += 1;
            }
            start = end;
        }
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

/// A decimal digit in the sense of the regular expression `\d` (Unicode category Nd).
#[must_use]
pub fn is_decimal(c: char) -> bool {
    c.general_category() == GeneralCategory::DecimalNumber
}

/// `s[:n]`: the first `n` characters.
#[must_use]
pub fn prefix(s: &str, n: usize) -> &str {
    s.char_indices().nth(n).map_or(s, |(i, _)| &s[..i])
}

/// `len(s)`: the number of characters.
#[must_use]
pub fn len(s: &str) -> usize {
    s.chars().count()
}

/// `os.path.splitext(name)[1]` of a file name: from its last dot, unless that dot only
/// starts the name (leading dots do not start an extension).
#[must_use]
pub fn splitext_ext(name: &str) -> &str {
    let base = name.rsplit('/').next().unwrap_or(name);
    match base.rfind('.') {
        Some(i) if base[..i].chars().any(|c| c != '.') => &base[i..],
        _ => "",
    }
}
