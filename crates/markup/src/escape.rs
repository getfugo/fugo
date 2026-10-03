//! HTML and URL escaping as goldmark writes it.

use ssg_base::Value;

/// Escapes `& < > "` (text and attribute values).
pub(crate) fn html(out: &mut String, s: &str) {
    let mut last = 0;
    for (i, c) in s.char_indices() {
        let rep = match c {
            '&' => "&amp;",
            '<' => "&lt;",
            '>' => "&gt;",
            '"' => "&quot;",
            '\0' => "\u{fffd}",
            _ => continue,
        };
        out.push_str(&s[last..i]);
        out.push_str(rep);
        last = i + c.len_utf8();
    }
    out.push_str(&s[last..]);
}

fn url_safe(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b"-_.+!*'(),%#@?=;:/&$~".contains(&b)
}

/// Percent-encodes what does not belong in a URL (spaces, non-ASCII, `<>"[\]^{|}` and
/// backticks; an existing `%XX` stays), then HTML-escapes it for an attribute.
pub(crate) fn url(out: &mut String, s: &str) {
    let mut enc = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if url_safe(b) {
            enc.push(char::from(b));
        } else {
            enc.push_str(&format!("%{b:02X}"));
        }
    }
    html(out, &enc);
}

/// A value as written in an HTML attribute.
pub(crate) fn value_text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        Value::String(s) => s.to_string(),
        Value::Date(d) => d.to_string(),
        Value::Array(a) => {
            let items: Vec<String> = a.iter().map(value_text).collect();
            format!("[{}]", items.join(" "))
        }
        Value::Map(_) => String::new(),
    }
}

/// Every attribute, as Go writes a heading's (`on*` handlers are already gone; values
/// that are not strings, numbers or booleans are written empty).
pub(crate) fn all_attrs(out: &mut String, attrs: &[(String, Value)]) {
    for (k, v) in attrs {
        out.push(' ');
        out.push_str(k);
        out.push_str("=\"");
        if !matches!(v, Value::Array(_) | Value::Map(_)) {
            html(out, &value_text(v));
        }
        out.push('"');
    }
}

/// goldmark's dangerous URLs (dropped when raw HTML is omitted): `javascript:`,
/// `vbscript:`, `file:` and `data:` other than common images.
pub(crate) fn dangerous_url(url: &str) -> bool {
    let has = |p: &str| {
        url.get(..p.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(p))
    };
    if has("data:image/") {
        let v = &url["data:image/".len()..];
        return !["png", "gif", "jpeg", "webp", "svg+xml"]
            .iter()
            .any(|f| v.starts_with(f));
    }
    has("javascript:") || has("vbscript:") || has("file:") || has("data:")
}

/// Attributes goldmark renders on an element: the global HTML attributes and `data-*`,
/// plus `extra` element-specific ones.
pub(crate) fn attrs(out: &mut String, attrs: &[(String, Value)], extra: &[&str]) {
    const GLOBAL: &[&str] = &[
        "accesskey",
        "autocapitalize",
        "autofocus",
        "class",
        "contenteditable",
        "dir",
        "draggable",
        "enterkeyhint",
        "hidden",
        "id",
        "inert",
        "inputmode",
        "is",
        "itemid",
        "itemprop",
        "itemref",
        "itemscope",
        "itemtype",
        "lang",
        "part",
        "role",
        "slot",
        "spellcheck",
        "style",
        "tabindex",
        "title",
        "translate",
    ];
    for (k, v) in attrs {
        let k = k.as_str();
        if !(GLOBAL.contains(&k) || extra.contains(&k) || k.starts_with("data-")) {
            continue;
        }
        out.push(' ');
        out.push_str(k);
        out.push_str("=\"");
        html(out, &value_text(v));
        out.push('"');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        let mut s = String::new();
        url(&mut s, "https://en.wikipedia.org/wiki/Lay's?a=1&b=[2] é%20");
        assert_eq!(
            s,
            "https://en.wikipedia.org/wiki/Lay's?a=1&amp;b=%5B2%5D%20%C3%A9%20"
        );
    }
}
