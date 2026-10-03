//! HTML and XML text: `truncate_html`, `plainify`, `html_escape`, `xml_escape`, `html_unescape`,
//! `emojify`.

use std::sync::LazyLock;

use html5gum::{DefaultEmitter, Token, Tokenizer};
use regex::Regex;
use tera::{Kwargs, TeraResult, Value};

use super::Registrar;
use super::value::text;

pub(super) fn register(r: &mut Registrar<'_>) {
    r.filter("truncate_html", |v, kw, _| truncate_html(&v, kw));
    r.filter("plainify", |v, _, _| {
        Ok(Value::from(plainify(&text(&v, "plainify")?)))
    });
    r.filter("html_escape", |v, _, _| {
        Ok(Value::safe_string(&html_escape(&text(&v, "html_escape")?)))
    });
    r.filter("xml_escape", |v, _, _| {
        Ok(Value::safe_string(&xml_escape(&text(&v, "xml_escape")?)))
    });
    r.filter("html_unescape", |v, _, _| {
        Ok(Value::from(html_unescape(&text(&v, "html_unescape")?)))
    });
    r.filter("emojify", |v, _, _| {
        Ok(Value::safe_string(&emojify(&text(&v, "emojify")?)))
    });
}

/// Escapes `& < > " '` (as `&amp; &lt; &gt; &#34; &#39;`, the Go templates' `html`).
pub(super) fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + s.len() / 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&#34;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// Go's `transform.XMLEscape`: drops the characters XML 1.0 forbids, then escapes as Go's
/// `xml.EscapeText` (`& < > " '` as `&amp; &lt; &gt; &#34; &#39;`, tab, newline and CR as
/// `&#x9; &#xA; &#xD;`).
pub(super) fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + s.len() / 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&#34;"),
            '\'' => out.push_str("&#39;"),
            '\t' => out.push_str("&#x9;"),
            '\n' => out.push_str("&#xA;"),
            '\r' => out.push_str("&#xD;"),
            // https://www.w3.org/TR/xml/#NT-Char (surrogates cannot occur in a `char`)
            '\u{20}'..='\u{FFFD}' | '\u{10000}'.. => out.push(c),
            _ => {}
        }
    }
    out
}

/// The text of `s` with every character reference decoded (named, decimal and hex, as an HTML
/// parser reads text); nothing else changes.
pub(super) fn html_unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    // Everything is character data for the tokenizer once `<` and CR are references themselves.
    let guarded = s.replace('<', "&lt;").replace('\r', "&#13;");
    let mut out = String::with_capacity(s.len());
    for token in Tokenizer::new(guarded.as_str()).flatten() {
        if let Token::String(t) = token {
            out.push_str(&String::from_utf8_lossy(&t.value));
        }
    }
    out
}

/// Go's `plainify`: the text of an HTML fragment. Tags, comments and the bodies of `script` and
/// `style` are removed, `</p>` and
/// `<br>` become line breaks, other line breaks spaces, and each run of white space is reduced to
/// its first character. Character references are decoded (the result is text, escaped again
/// when printed). Input without `<` or `>` only has its references decoded.
pub(super) fn plainify(s: &str) -> String {
    if !s.contains(['<', '>']) {
        return html_unescape(s);
    }
    let mut emitter = DefaultEmitter::default();
    emitter.naively_switch_states(true); // script and style bodies are text
    let mut raw = String::with_capacity(s.len());
    let mut in_code = false; // inside script or style: not text
    for token in Tokenizer::new_with_emitter(s, emitter).flatten() {
        match token {
            Token::String(t) if !in_code => {
                raw.push_str(&String::from_utf8_lossy(&t.value).replace('\n', " "));
            }
            Token::StartTag(t) if matches!(t.name.as_slice(), b"script" | b"style") => {
                in_code = true;
            }
            Token::EndTag(t) if matches!(t.name.as_slice(), b"script" | b"style") => {
                in_code = false;
            }
            Token::EndTag(t) if t.name.as_slice() == b"p" => raw.push('\n'),
            Token::StartTag(t) if t.name.as_slice() == b"br" => raw.push('\n'),
            _ => {}
        }
    }
    let mut out = String::with_capacity(raw.len());
    let mut was_space = false;
    for c in raw.chars() {
        let space = c.is_whitespace();
        if !space || !was_space {
            out.push(c);
        }
        was_space = space;
    }
    out
}

static TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^<(/)?([^ ]+?)(?:(\s*/)| .*?)?>").expect("valid tag pattern"));

static CJK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[\p{Han}\p{Hangul}\p{Hiragana}\p{Katakana}]").expect("valid script pattern")
});

/// Elements without an end tag.
const VOID: &[&str] = &[
    "br", "col", "link", "base", "img", "param", "area", "hr", "input",
];

struct OpenTag<'s> {
    name: &'s str,
    pos: usize,
    open: bool,
}

/// Go's `truncate`: at most `length` characters of text (tags of a safe input do not count),
/// cut at the last word boundary (any character boundary before CJK text), then `ellipsis`
/// (default ` …`). A safe input keeps its markup valid: the tags left open are closed, and the
/// result is safe; a plain input stays plain.
fn truncate_html(v: &Value, kw: &Kwargs) -> TeraResult<Value> {
    let length = kw.must_get::<i64>("length")?;
    let ellipsis_value = kw.get::<Value>("ellipsis")?;
    let is_html = v.is_safe();
    let s = text(v, "truncate_html")?;
    let ellipsis = match &ellipsis_value {
        None => " …".to_owned(),
        Some(e) if !is_html || e.is_safe() => text(e, "truncate_html(ellipsis=)")?.into_owned(),
        Some(e) => html_escape(&text(e, "truncate_html(ellipsis=)")?),
    };
    let wrap = |out: String| {
        if is_html {
            Value::safe_string(&out)
        } else {
            Value::from(out)
        }
    };
    let limit = usize::try_from(length).unwrap_or(0);
    if length >= 0 && s.chars().count() <= limit {
        return Ok(wrap(s.into_owned()));
    }

    let mut tags: Vec<OpenTag<'_>> = Vec::new();
    let (mut last_word_end, mut last_non_space, mut count, mut skip_to) = (0, 0, 0usize, 0);
    for (at, c) in s.char_indices() {
        if at < skip_to {
            continue;
        }
        if is_html
            && c == '<'
            && let Some(caps) = TAG.captures(&s[at..])
        {
            skip_to = at + caps[0].len();
            let tag_name = caps.get(2).map_or("", |m| m.as_str());
            let Some(name) = tag_name.split_whitespace().next() else {
                continue;
            };
            last_word_end = last_non_space;
            if !VOID.contains(&name) && caps.get(3).is_none() {
                tags.push(OpenTag {
                    name,
                    pos: at,
                    open: caps.get(1).is_none(),
                });
            }
            continue;
        }
        count += 1;
        if c.is_whitespace() {
            last_word_end = last_non_space;
        } else if CJK.is_match(&s[at..]) {
            last_word_end = at;
        } else {
            last_non_space = at + c.len_utf8();
        }
        if count > limit {
            let end = if last_word_end == 0 {
                at
            } else {
                last_word_end
            };
            let mut out = s[..end].to_owned();
            out.push_str(&ellipsis);
            if is_html {
                close_tags(&tags, end, &mut out);
            }
            return Ok(wrap(out));
        }
    }
    Ok(wrap(s.into_owned()))
}

/// Appends end tags for the tags opened before `end` and not closed before it.
fn close_tags(tags: &[OpenTag<'_>], end: usize, out: &mut String) {
    let mut closed_by: Option<&str> = None;
    for tag in tags.iter().rev() {
        if tag.pos >= end || closed_by.is_some() {
            if closed_by == Some(tag.name) {
                closed_by = None;
            }
            continue;
        }
        if tag.open {
            out.push_str("</");
            out.push_str(tag.name);
            out.push('>');
        } else {
            closed_by = Some(tag.name);
        }
    }
}

/// Replaces `:shortcode:` emoji (GitHub's shortcodes) by the emoji.
pub(super) fn emojify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find(':') {
        let after = &rest[start + 1..];
        let Some(len) = after.find(':') else { break };
        let name = &after[..len];
        let valid = !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '-'));
        match emojis::get_by_shortcode(name).filter(|_| valid) {
            Some(emoji) => {
                out.push_str(&rest[..start]);
                out.push_str(emoji.as_str());
                rest = &after[len + 1..];
            }
            None => {
                // the closing colon may open the next shortcode
                out.push_str(&rest[..=start + len]);
                rest = &rest[start + 1 + len..];
            }
        }
    }
    out.push_str(rest);
    out
}
