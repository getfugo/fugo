//! Inline CSS: the text of a `<style>` and the value of a `style` attribute, printed compactly by
//! this crate's CSS minifier in syntax the project's browser targets read ([`Css::inline`]).

use super::holds_template;
use crate::Minifier;
use crate::css::{self, Css};

/// The minified text of a `<style>`, if it changes. In SVG and MathML (`foreign`) the text is
/// markup: one with a character reference or a CDATA section is kept as written.
pub(super) fn element(m: &Minifier, text: &str, foreign: bool) -> Option<String> {
    let markup = |s: &str| foreign && s.contains(['&', '<']);
    if text.is_empty() || holds_template(m, text) || markup(text) {
        return None;
    }
    let min = css::minify(Css::inline(&m.options.css), text);
    // The style sheet must still end at its end tag.
    let ends = !min.to_ascii_lowercase().contains("</style") && !markup(&min);
    (ends && min != text).then_some(min)
}

/// The minified value of a `style` attribute that minify-html wrote as `raw` (between its
/// quotes), in double quotes, if it changes. A value that does not parse, that minifies to
/// nothing, or that holds a reference [`decode`] leaves (minify-html writes named ones for some
/// text outside ASCII) is kept as written. minify-html then writes the value as it writes every
/// attribute (the second pass of `html::minify`).
pub(super) fn attribute(m: &Minifier, raw: &str) -> Option<String> {
    let value = decode(raw)?;
    if holds_template(m, &value) {
        return None;
    }
    let min = css::declarations(Css::inline(&m.options.css), &value)?;
    if min.is_empty() || min == value {
        return None;
    }
    Some(format!(
        "\"{}\"",
        min.replace('&', "&amp;").replace('"', "&#34;")
    ))
}

/// The text of an attribute value with its character references decoded: `&amp;`, `&lt;`,
/// `&gt;`, `&quot;`, `&apos;` and the numeric references of ASCII (minify-html writes `&#34;`,
/// `&#32;`, …). `None` for any other reference, and for an `&` before a name without a `;`.
fn decode(raw: &str) -> Option<String> {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp + 1..];
        // An `&` that does not start a reference is text.
        if !rest.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '#') {
            out.push('&');
            continue;
        }
        let (name, after) = rest.split_once(';')?;
        out.push(match name {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            _ => numeric(name)?,
        });
        rest = after;
    }
    out.push_str(rest);
    Some(out)
}

/// The ASCII character of a numeric reference (`#34`, `#x22`), but NUL, which HTML replaces.
fn numeric(name: &str) -> Option<char> {
    let digits = name.strip_prefix('#')?;
    let (digits, radix) = match digits.strip_prefix(['x', 'X']) {
        Some(hex) => (hex, 16),
        None => (digits, 10),
    };
    if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
        return None;
    }
    let code = u32::from_str_radix(digits, radix).ok()?;
    char::from_u32(code).filter(|c| matches!(c, '\u{1}'..='\u{7f}'))
}

#[cfg(test)]
mod tests {
    use super::decode;

    #[test]
    fn references() {
        assert_eq!(
            decode("font-family:&#34;A B&#34;,&#x27;C&#39;").as_deref(),
            Some("font-family:\"A B\",'C'")
        );
        assert_eq!(
            decode("content:&quot;&amp;&lt;&gt;&apos;&quot;").as_deref(),
            Some("content:\"&<>'\"")
        );
        // An `&` before anything but a name or `#` is text.
        assert_eq!(
            decode("content:'a & b'&").as_deref(),
            Some("content:'a & b'&")
        );
        // Named references outside the five, references outside ASCII, NUL and unterminated
        // ones are not decoded.
        for raw in ["&nbsp;", "&#233;", "&#0;", "&#;", "&#+34;", "&amp", "a&b"] {
            assert_eq!(decode(raw), None, "{raw}");
        }
    }
}
