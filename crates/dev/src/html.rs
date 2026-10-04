//! The HTML tokenizer of the manifest extractor: CPython 3.14's `html.parser.HTMLParser`
//! (`convert_charrefs=True`, `scripting=False`) and `html.unescape`, reproduced rule by rule
//! (see [`crate::py`] for why). The extractor reads the Go build's output and this port's with
//! the same tokenizer, so its leniencies (attribute values, unterminated tags, raw-text
//! elements) are part of the comparison and must not change.
//!
//! Positions are character indices, as in Python, which matters for the 34-character look-back
//! at a trailing `&`.
//!
//! Translated from CPython 3.14.7 (`Lib/html/parser.py`, `Lib/html/__init__.py`; PSF-2.0,
//! THIRD_PARTY/cpython/LICENSE; PROVENANCE.md).

use std::collections::HashMap;
use std::sync::OnceLock;

/// The callbacks of a parse: `HTMLParser`'s `handle_*` methods the extractor uses (comments,
/// declarations and processing instructions are skipped).
pub trait Handler {
    fn starttag(&mut self, tag: &str, attrs: &[(String, Option<String>)]);
    /// `<tag … />`; by default a start tag followed by its end tag.
    fn startendtag(&mut self, tag: &str, attrs: &[(String, Option<String>)]) {
        self.starttag(tag, attrs);
        self.endtag(tag);
    }
    fn endtag(&mut self, tag: &str);
    fn data(&mut self, data: &str);
}

/// `p.feed(text); p.close()`.
pub fn parse(text: &str, h: &mut impl Handler) {
    let mut p = Parser {
        raw: text.chars().collect(),
        cdata_elem: None,
        escapable: true,
    };
    if !p.raw.is_empty() {
        p.goahead(false, h);
    }
    p.goahead(true, h);
}

/// The raw-text elements (`CDATA_CONTENT_ELEMENTS`) and `plaintext`: their content is data up
/// to their end tag.
const RAWTEXT: [&str; 6] = ["script", "style", "xmp", "iframe", "noembed", "noframes"];
/// The escapable raw-text elements (`RCDATA_CONTENT_ELEMENTS`): character references are decoded.
const RCDATA: [&str; 2] = ["textarea", "title"];

struct Parser {
    raw: Vec<char>,
    cdata_elem: Option<Vec<char>>,
    escapable: bool,
}

/// `[\t\n\r\f ]`
fn is_ws(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{c}' | ' ')
}

fn collect(cs: &[char]) -> String {
    cs.iter().collect()
}

fn find(raw: &[char], c: char, from: usize) -> Option<usize> {
    raw.get(from..)?
        .iter()
        .position(|&x| x == c)
        .map(|p| p + from)
}

fn starts(raw: &[char], at: usize, s: &str) -> bool {
    s.chars()
        .enumerate()
        .all(|(k, c)| raw.get(at + k) == Some(&c))
}

/// `<[a-zA-Z]` at `i`.
fn starttagopen(raw: &[char], i: usize) -> bool {
    raw.get(i) == Some(&'<') && raw.get(i + 1).is_some_and(char::is_ascii_alphabetic)
}

/// `</[a-zA-Z]` at `i`.
fn endtagopen(raw: &[char], i: usize) -> bool {
    starts(raw, i, "</") && raw.get(i + 2).is_some_and(char::is_ascii_alphabetic)
}

/// `(?:[\t\n\r\f ]|/(?!>))*` from `p`: its end.
fn skip_ws_and_lone_slashes(raw: &[char], mut p: usize) -> usize {
    while let Some(&c) = raw.get(p) {
        if is_ws(c) || (c == '/' && raw.get(p + 1) != Some(&'>')) {
            p += 1;
        } else {
            break;
        }
    }
    p
}

/// `tagfind_tolerant` at `start` (a letter): `([a-zA-Z][^\t\n\r\f />]*)(?:[\t\n\r\f ]|/(?!>))*`;
/// returns (the end of the name, the end of the match).
fn tagfind(raw: &[char], start: usize) -> (usize, usize) {
    let mut p = start + 1;
    while raw
        .get(p)
        .is_some_and(|&c| !is_ws(c) && c != '/' && c != '>')
    {
        p += 1;
    }
    (p, skip_ws_and_lone_slashes(raw, p))
}

/// The value indicator of an attribute after its name ends at `p`:
/// `[\t\n\r\f ]*=[\t\n\r\f ]*('[^']*'|"[^"]*"|(?!['"])[^>\t\n\r\f ]*)`, with the regular
/// expression's backtracking: an unclosed quote after spaces gives an empty bare value before
/// the last space. Returns the range of the value (quotes included), or `None`.
fn value_indicator(raw: &[char], p: usize) -> Option<(usize, usize)> {
    let mut q = p;
    while raw.get(q).is_some_and(|&c| is_ws(c)) {
        q += 1;
    }
    if raw.get(q) != Some(&'=') {
        return None;
    }
    q += 1;
    let mut spaces = 0;
    while raw.get(q + spaces).is_some_and(|&c| is_ws(c)) {
        spaces += 1;
    }
    for w in (0..=spaces).rev() {
        let r = q + w;
        if let Some(&quote @ ('\'' | '"')) = raw.get(r) {
            if let Some(close) = find(raw, quote, r + 1) {
                return Some((r, close + 1));
            }
            continue;
        }
        let mut e = r;
        while raw.get(e).is_some_and(|&c| c != '>' && !is_ws(c)) {
            e += 1;
        }
        return Some((r, e));
    }
    None
}

/// The start of an attribute name at `p`: `(?<=['"\t\n\r\f /])[^\t\n\r\f />]`.
fn attr_name_starts(raw: &[char], p: usize) -> bool {
    p > 0
        && (matches!(raw[p - 1], '\'' | '"' | '/') || is_ws(raw[p - 1]))
        && raw
            .get(p)
            .is_some_and(|&c| !is_ws(c) && c != '/' && c != '>')
}

/// The end of an attribute name starting at `p`: `[^\t\n\r\f /=>]*` after its first character.
fn attr_name_end(raw: &[char], p: usize) -> usize {
    let mut e = p + 1;
    while raw
        .get(e)
        .is_some_and(|&c| !is_ws(c) && !matches!(c, '/' | '=' | '>'))
    {
        e += 1;
    }
    e
}

/// `locatetagend` at `start` (a letter): the end of a tag's name and attributes, after its `>`
/// when there is one.
fn locatetagend(raw: &[char], start: usize) -> usize {
    let mut p = start + 1;
    while raw
        .get(p)
        .is_some_and(|&c| !is_ws(c) && c != '/' && c != '>')
    {
        p += 1;
    }
    while raw.get(p).is_some_and(|&c| is_ws(c) || c == '/') {
        p += 1;
    }
    while attr_name_starts(raw, p) {
        p = attr_name_end(raw, p);
        if let Some((_, e)) = value_indicator(raw, p) {
            p = e;
        }
        while raw.get(p).is_some_and(|&c| is_ws(c) || c == '/') {
            p += 1;
        }
    }
    if raw.get(p) == Some(&'>') {
        p += 1;
    }
    p
}

impl Parser {
    fn set_cdata_mode(&mut self, tag: &str, escapable: bool) {
        self.cdata_elem = Some(tag.chars().collect());
        self.escapable = escapable;
    }

    fn clear_cdata_mode(&mut self) {
        self.cdata_elem = None;
        self.escapable = true;
    }

    /// The `interesting` search in a raw-text element: `</<elem>` (ASCII case-insensitive)
    /// followed by `[\t\n\r\f />]`; `plaintext` never ends.
    fn cdata_end(&self, from: usize) -> Option<usize> {
        let elem = self.cdata_elem.as_ref()?;
        let n = self.raw.len();
        if elem.iter().copied().eq("plaintext".chars()) {
            return Some(n);
        }
        let raw = &self.raw;
        let mut p = from;
        while p + 2 + elem.len() < n {
            if raw[p] == '<'
                && raw[p + 1] == '/'
                && raw[p + 2..p + 2 + elem.len()]
                    .iter()
                    .zip(elem)
                    .all(|(a, b)| a.eq_ignore_ascii_case(b))
                && matches!(
                    raw[p + 2 + elem.len()],
                    '\t' | '\n' | '\r' | '\u{c}' | ' ' | '/' | '>'
                )
            {
                return Some(p);
            }
            p += 1;
        }
        None
    }

    fn handle_data(&self, h: &mut impl Handler, from: usize, to: usize) {
        let s = collect(&self.raw[from..to]);
        if self.escapable {
            h.data(&unescape(&s));
        } else {
            h.data(&s);
        }
    }

    fn goahead(&mut self, end: bool, h: &mut impl Handler) {
        let n = self.raw.len();
        let mut i = 0;
        while i < n {
            let j = if self.cdata_elem.is_none() {
                if let Some(j) = find(&self.raw, '<', i) {
                    j
                } else {
                    // A character reference may be cut at the end of the text so far: wait for
                    // more unless a `&` near the end is followed by a space or `;`.
                    let from = i.max(n.saturating_sub(34));
                    let amp = self.raw[from..]
                        .iter()
                        .rposition(|&c| c == '&')
                        .map(|p| p + from);
                    if let Some(a) = amp
                        && !self.raw[a..].iter().any(|&c| is_ws(c) || c == ';')
                    {
                        break;
                    }
                    n
                }
            } else {
                match self.cdata_end(i) {
                    Some(j) => j,
                    None => break,
                }
            };
            if i < j {
                self.handle_data(h, i, j);
            }
            i = j;
            if i == n {
                break;
            }
            let raw = &self.raw;
            let k = if starttagopen(raw, i) {
                self.parse_starttag(i, h)
            } else if starts(raw, i, "</") {
                self.parse_endtag(i, h)
            } else if starts(raw, i, "<!--") {
                self.parse_comment(i)
            } else if starts(raw, i, "<?") {
                self.parse_pi(i)
            } else if starts(raw, i, "<!") {
                self.parse_html_declaration(i)
            } else if i + 1 < n || end {
                h.data("<");
                Some(i + 1)
            } else {
                break;
            };
            i = match k {
                Some(k) => k,
                None => {
                    if !end {
                        break;
                    }
                    let raw = &self.raw;
                    if !starttagopen(raw, i) && starts(raw, i, "</") && i + 2 == n {
                        h.data("</");
                    }
                    n
                }
            };
        }
        if end && i < n {
            self.handle_data(h, i, n);
            i = n;
        }
        self.raw.drain(..i);
    }

    fn parse_starttag(&mut self, i: usize, h: &mut impl Handler) -> Option<usize> {
        let raw = &self.raw;
        let endpos = locatetagend(raw, i + 1);
        if raw[endpos - 1] != '>' {
            return None;
        }
        let (name_end, mut k) = tagfind(raw, i + 1);
        let tag = collect(&raw[i + 1..name_end]).to_lowercase();
        let mut attrs = Vec::new();
        while k < endpos {
            if !attr_name_starts(raw, k) {
                break;
            }
            let name_stop = attr_name_end(raw, k);
            let name = collect(&raw[k..name_stop]).to_lowercase();
            let mut p = name_stop;
            let value = value_indicator(raw, name_stop).map(|(a, b)| {
                p = b;
                let mut v = &raw[a..b];
                if v.len() >= 2 && matches!(v[0], '\'' | '"') && v[v.len() - 1] == v[0] {
                    v = &v[1..v.len() - 1];
                }
                let v = collect(v);
                if v.is_empty() {
                    v
                } else {
                    unescape_attrvalue(&v)
                }
            });
            attrs.push((name, value));
            k = skip_ws_and_lone_slashes(raw, p);
        }
        let rest = collect(&raw[k..endpos]);
        let rest = crate::py::strip(&rest);
        if rest != ">" && rest != "/>" {
            h.data(&collect(&raw[i..endpos]));
            return Some(endpos);
        }
        if rest == "/>" {
            h.startendtag(&tag, &attrs);
        } else {
            h.starttag(&tag, &attrs);
            if RAWTEXT.contains(&tag.as_str()) || tag == "plaintext" {
                self.set_cdata_mode(&tag, false);
            } else if RCDATA.contains(&tag.as_str()) {
                self.set_cdata_mode(&tag, true);
            }
        }
        Some(endpos)
    }

    fn parse_endtag(&mut self, i: usize, h: &mut impl Handler) -> Option<usize> {
        let raw = &self.raw;
        find(raw, '>', i + 2)?;
        if !endtagopen(raw, i) {
            if raw.get(i + 2) == Some(&'>') {
                return Some(i + 3); // `</>` is ignored
            }
            return self.parse_bogus_comment(i);
        }
        let j = locatetagend(raw, i + 2);
        if raw[j - 1] != '>' {
            return None;
        }
        let (name_end, _) = tagfind(raw, i + 2);
        let tag = collect(&raw[i + 2..name_end]).to_lowercase();
        h.endtag(&tag);
        self.clear_cdata_mode();
        Some(j)
    }

    /// `<!--…-->`: an empty comment ends at the first `>` or `->`, any other at `-->` or `--!>`.
    fn parse_comment(&self, i: usize) -> Option<usize> {
        let raw = &self.raw;
        let p = i + 4;
        if raw.get(p) == Some(&'>') {
            return Some(p + 1);
        }
        if starts(raw, p, "->") {
            return Some(p + 2);
        }
        (p..raw.len()).find_map(|q| {
            if starts(raw, q, "-->") {
                Some(q + 3)
            } else if starts(raw, q, "--!>") {
                Some(q + 4)
            } else {
                None
            }
        })
    }

    fn parse_bogus_comment(&self, i: usize) -> Option<usize> {
        find(&self.raw, '>', i + 2).map(|p| p + 1)
    }

    fn parse_pi(&self, i: usize) -> Option<usize> {
        find(&self.raw, '>', i + 2).map(|p| p + 1)
    }

    fn parse_html_declaration(&self, i: usize) -> Option<usize> {
        let raw = &self.raw;
        if starts(raw, i, "<!--") {
            return self.parse_comment(i);
        }
        if starts(raw, i, "<![CDATA[") {
            return (i + 9..raw.len())
                .find(|&j| starts(raw, j, "]]>"))
                .map(|j| j + 3);
        }
        let head = raw.get(i..i + 9).map(collect).unwrap_or_default();
        if head.eq_ignore_ascii_case("<!doctype") {
            return find(raw, '>', i + 9).map(|p| p + 1);
        }
        self.parse_bogus_comment(i)
    }
}

// ---------------------------------------------------------------------------------------------
// Character references

/// `html.entities.html5`: the HTML5 named character references, by name with or without its
/// `;` (only the legacy ones exist without).
fn html5() -> &'static HashMap<&'static str, &'static str> {
    static MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    MAP.get_or_init(|| {
        entities::ENTITIES
            .iter()
            .map(|e| (&e.entity[1..], e.characters))
            .collect()
    })
}

/// `html.unescape`: every named and numeric character reference of `s` decoded by the HTML5
/// rules for text (a name without its `;` is matched by its longest legacy prefix).
#[must_use]
pub fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    let cs: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < cs.len() {
        if cs[i] == '&'
            && let Some(end) = charref_end(&cs, i + 1)
        {
            out.push_str(&replace_charref(&cs[i + 1..end]));
            i = end;
            continue;
        }
        out.push(cs[i]);
        i += 1;
    }
    out
}

/// The end of a character reference whose `&` is just before `p`:
/// `#[0-9]+;?|#[xX][0-9a-fA-F]+;?|[^\t\n\f <&#;]{1,32};?`.
fn charref_end(cs: &[char], p: usize) -> Option<usize> {
    let semicolon = |q: usize| if cs.get(q) == Some(&';') { q + 1 } else { q };
    if cs.get(p) == Some(&'#') {
        let digits = cs[p + 1..]
            .iter()
            .take_while(|c| c.is_ascii_digit())
            .count();
        if digits > 0 {
            return Some(semicolon(p + 1 + digits));
        }
        if matches!(cs.get(p + 1), Some('x' | 'X')) {
            let hex = cs[p + 2..]
                .iter()
                .take_while(|c| c.is_ascii_hexdigit())
                .count();
            if hex > 0 {
                return Some(semicolon(p + 2 + hex));
            }
        }
        return None;
    }
    let name = cs[p..]
        .iter()
        .take(32)
        .take_while(|&&c| !matches!(c, '\t' | '\n' | '\u{c}' | ' ' | '<' | '&' | '#' | ';'))
        .count();
    (name > 0).then(|| semicolon(p + name))
}

/// `_replace_charref`: the text of one character reference (without its `&`).
fn replace_charref(s: &[char]) -> String {
    if s[0] == '#' {
        let (digits, radix) = if matches!(s.get(1), Some('x' | 'X')) {
            (&s[2..], 16)
        } else {
            (&s[1..], 10)
        };
        let mut num: u32 = 0;
        for c in digits.iter().take_while(|&&c| c != ';') {
            num = num
                .saturating_mul(radix)
                .saturating_add(c.to_digit(radix).unwrap_or(0))
                .min(0x11_0000);
        }
        if let Some(c) = invalid_charref(num) {
            return c.to_string();
        }
        if (0xD800..=0xDFFF).contains(&num) || num > 0x10_FFFF {
            return "\u{FFFD}".to_owned();
        }
        if is_invalid_codepoint(num) {
            return String::new();
        }
        return char::from_u32(num).map(String::from).unwrap_or_default();
    }
    let name = collect(s);
    let map = html5();
    if let Some(v) = map.get(name.as_str()) {
        return (*v).to_owned();
    }
    for x in (2..s.len()).rev() {
        if let Some(v) = map.get(collect(&s[..x]).as_str()) {
            return format!("{v}{}", collect(&s[x..]));
        }
    }
    format!("&{name}")
}

/// `_invalid_charrefs`: the numeric references the HTML5 parser maps to other characters.
fn invalid_charref(num: u32) -> Option<char> {
    Some(match num {
        0x00 => '\u{FFFD}',
        0x0d => '\r',
        0x80 => '\u{20AC}',
        0x81 => '\u{81}',
        0x82 => '\u{201A}',
        0x83 => '\u{0192}',
        0x84 => '\u{201E}',
        0x85 => '\u{2026}',
        0x86 => '\u{2020}',
        0x87 => '\u{2021}',
        0x88 => '\u{02C6}',
        0x89 => '\u{2030}',
        0x8a => '\u{0160}',
        0x8b => '\u{2039}',
        0x8c => '\u{0152}',
        0x8d => '\u{8D}',
        0x8e => '\u{017D}',
        0x8f => '\u{8F}',
        0x90 => '\u{90}',
        0x91 => '\u{2018}',
        0x92 => '\u{2019}',
        0x93 => '\u{201C}',
        0x94 => '\u{201D}',
        0x95 => '\u{2022}',
        0x96 => '\u{2013}',
        0x97 => '\u{2014}',
        0x98 => '\u{02DC}',
        0x99 => '\u{2122}',
        0x9a => '\u{0161}',
        0x9b => '\u{203A}',
        0x9c => '\u{0153}',
        0x9d => '\u{9D}',
        0x9e => '\u{017E}',
        0x9f => '\u{0178}',
        _ => return None,
    })
}

/// `_invalid_codepoints`: the numeric references that decode to nothing.
fn is_invalid_codepoint(num: u32) -> bool {
    matches!(num, 0x1..=0x8 | 0xb | 0xe..=0x1f | 0x7f..=0x9f | 0xfdd0..=0xfdef)
        || (num & 0xfffe == 0xfffe && num <= 0x10_ffff)
}

/// `_unescape_attrvalue`: in an attribute value a numeric reference is always decoded, a named
/// one only when it is an exact name (with or without `;`) and not followed by `=`.
fn unescape_attrvalue(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    let cs: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < cs.len() {
        if cs[i] == '&'
            && let Some(end) = attr_charref_end(&cs, i + 1)
        {
            let r = collect(&cs[i..end]);
            if cs.get(i + 1) == Some(&'#') || (!r.ends_with('=') && html5().contains_key(&r[1..])) {
                out.push_str(&unescape(&r));
            } else {
                out.push_str(&r);
            }
            i = end;
            continue;
        }
        out.push(cs[i]);
        i += 1;
    }
    out
}

/// `&(#[0-9]+|#[xX][0-9a-fA-F]+|[a-zA-Z][a-zA-Z0-9]*)[;=]?` after the `&` just before `p`.
fn attr_charref_end(cs: &[char], p: usize) -> Option<usize> {
    let tail = |q: usize| {
        if matches!(cs.get(q), Some(';' | '=')) {
            q + 1
        } else {
            q
        }
    };
    match cs.get(p) {
        Some('#') => {
            let digits = cs[p + 1..]
                .iter()
                .take_while(|c| c.is_ascii_digit())
                .count();
            if digits > 0 {
                return Some(tail(p + 1 + digits));
            }
            if matches!(cs.get(p + 1), Some('x' | 'X')) {
                let hex = cs[p + 2..]
                    .iter()
                    .take_while(|c| c.is_ascii_hexdigit())
                    .count();
                if hex > 0 {
                    return Some(tail(p + 2 + hex));
                }
            }
            None
        }
        Some(c) if c.is_ascii_alphabetic() => {
            let name = cs[p..]
                .iter()
                .take_while(|c| c.is_ascii_alphanumeric())
                .count();
            Some(tail(p + name))
        }
        _ => None,
    }
}
