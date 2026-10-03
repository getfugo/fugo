//! HTML normalisation shared by both sides of every comparison.
//!
//! Equal output means "the same element tree and the same text", not the same bytes:
//! entities are decoded, attributes sorted, whitespace collapsed outside `<pre>` and dropped
//! next to block tags, XHTML self-closing slashes ignored. Differences that belong to the Go
//! implementation's own renderers and are re-implemented by `ssg-markup` anyway are folded too:
//! highlighted code (Chroma's `div.highlight`) and plain `<pre><code>` both become
//! `<pre lang="…">text</pre>`, `align="x"` becomes `style="text-align: x"`, and ids on
//! `h1`–`h6`/`dt` are dropped when the caller asks (auto ids are a custom pass, T22).

use html5gum::{Token, Tokenizer};

#[derive(Clone, Copy, Default)]
pub struct Fold {
    /// Drop `id` on headings and definition terms.
    pub auto_ids: bool,
    /// Map typographer output (curly quotes, dashes, ellipsis) back to ASCII.
    pub typography: bool,
    /// Drop footnote references (`<sup>`) and the footnotes container.
    pub footnotes: bool,
}

fn fold_typography(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '‘' | '’' => out.push('\''),
            '“' | '”' => out.push('"'),
            '–' => out.push_str("--"),
            '—' => out.push_str("---"),
            '…' => out.push_str("..."),
            '«' => out.push_str("<<"),
            '»' => out.push_str(">>"),
            c => out.push(c),
        }
    }
    out
}

fn drop_footnotes(toks: Vec<Tok>) -> Vec<Tok> {
    let mut out = Vec::with_capacity(toks.len());
    // (element name, depth) while skipping.
    let mut skip: Option<(String, usize)> = None;
    for t in toks {
        if let Some((name, depth)) = skip.as_mut() {
            match &t {
                Tok::Open { name: n, .. } if n == name => *depth += 1,
                Tok::Close(n) if n == name => {
                    *depth -= 1;
                    if *depth == 0 {
                        skip = None;
                    }
                }
                _ => {}
            }
            continue;
        }
        if let Tok::Open { name, tag } = &t
            && (name == "sup" || tag.contains("footnotes"))
        {
            skip = Some((name.clone(), 1));
            continue;
        }
        out.push(t);
    }
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tok {
    Open { name: String, tag: String },
    Close(String),
    Text(String),
    Comment(String),
    Code { lang: String, text: String },
}

const BLOCK: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "dd",
    "details",
    "div",
    "dl",
    "dt",
    "figcaption",
    "figure",
    "footer",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hr",
    "li",
    "nav",
    "ol",
    "p",
    "section",
    "summary",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "tr",
    "ul",
    "br",
];

fn is_block(name: &str) -> bool {
    BLOCK.contains(&name)
}

fn string(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// Tokenizes and folds `html`.
pub fn tokens(html: &str, fold: Fold) -> Vec<Tok> {
    let mut out = Vec::new();
    // Inside a code block: (closing tag name, div depth, lang, text).
    let mut code: Option<(String, usize, String, String)> = None;
    for Ok(token) in Tokenizer::new(html) {
        if let Some((end, depth, lang, text)) = code.as_mut() {
            match token {
                Token::StartTag(t) => {
                    let name = string(&t.name);
                    if name == "div" {
                        *depth += 1;
                    }
                    if name == "code" {
                        if let Some(l) = t.attributes.get(b"data-lang".as_slice()) {
                            *lang = string(l);
                        } else if let Some(c) = t.attributes.get(b"class".as_slice()) {
                            let c = string(c);
                            if let Some(l) = c.split(' ').find_map(|c| c.strip_prefix("language-"))
                            {
                                *lang = l.to_owned();
                            }
                        }
                    }
                }
                Token::EndTag(t) => {
                    let name = string(&t.name);
                    let done = if name == "div" && end == "div" {
                        *depth -= 1;
                        *depth == 0
                    } else {
                        name == *end
                    };
                    if done {
                        let text = text.trim_end_matches('\n').to_owned();
                        out.push(Tok::Code {
                            lang: std::mem::take(lang),
                            text,
                        });
                        code = None;
                    }
                }
                Token::String(s) => text.push_str(&string(&s)),
                _ => {}
            }
            continue;
        }
        match token {
            Token::StartTag(t) => {
                let name = string(&t.name);
                let class = t
                    .attributes
                    .get(b"class".as_slice())
                    .map(|c| string(c))
                    .unwrap_or_default();
                if name == "pre" {
                    // The convert oracle's stub highlighter writes `<pre data-lang="go">`.
                    let lang = t
                        .attributes
                        .get(b"data-lang".as_slice())
                        .map(|l| string(l))
                        .unwrap_or_default();
                    code = Some(("pre".into(), 0, lang, String::new()));
                    continue;
                }
                if name == "div" && class.split(' ').any(|c| c == "highlight") {
                    code = Some(("div".into(), 1, String::new(), String::new()));
                    continue;
                }
                let mut attrs: Vec<(String, String)> = t
                    .attributes
                    .iter()
                    .map(|(k, v)| (string(k), string(v)))
                    .collect();
                if fold.auto_ids && (name.len() == 2 && name.starts_with('h') || name == "dt") {
                    attrs.retain(|(k, _)| k != "id");
                }
                for (k, v) in &mut attrs {
                    if k == "align" {
                        *k = "style".into();
                        *v = format!("text-align: {v}");
                    }
                }
                attrs.sort();
                let mut tag = format!("<{name}");
                for (k, v) in attrs {
                    tag.push_str(&format!(" {k}=\"{}\"", v.replace('"', "&quot;")));
                }
                tag.push('>');
                out.push(Tok::Open { name, tag });
            }
            Token::EndTag(t) => out.push(Tok::Close(string(&t.name))),
            Token::String(s) => {
                let s = string(&s);
                out.push(Tok::Text(if fold.typography {
                    fold_typography(&s)
                } else {
                    s
                }));
            }
            Token::Comment(c) => out.push(Tok::Comment(string(&c))),
            Token::Doctype(_) | Token::Error(_) => {}
        }
    }
    if fold.footnotes {
        drop_footnotes(out)
    } else {
        out
    }
}

/// Serialises folded tokens with whitespace collapsed.
pub fn serialize(toks: &[Tok]) -> String {
    let mut out = String::new();
    let mut after_block = true;
    for t in toks {
        match t {
            Tok::Open { name, tag } => {
                if is_block(name) {
                    trim_end(&mut out);
                    after_block = true;
                }
                out.push_str(tag);
            }
            Tok::Close(name) => {
                if is_block(name) {
                    trim_end(&mut out);
                    after_block = true;
                }
                out.push_str("</");
                out.push_str(name);
                out.push('>');
            }
            Tok::Text(s) => {
                let mut prev_space = out.ends_with(' ') || after_block;
                for c in s.chars() {
                    if c.is_ascii_whitespace() {
                        if !prev_space {
                            out.push(' ');
                            prev_space = true;
                        }
                    } else {
                        out.push(c);
                        prev_space = false;
                        after_block = false;
                    }
                }
            }
            Tok::Comment(c) => {
                trim_end(&mut out);
                out.push_str("<!--");
                out.push_str(c.trim());
                out.push_str("-->");
                after_block = true;
            }
            Tok::Code { lang, text } => {
                trim_end(&mut out);
                out.push_str(&format!("<pre lang=\"{lang}\">{text}</pre>"));
                after_block = true;
            }
        }
    }
    trim_end(&mut out);
    out
}

fn trim_end(s: &mut String) {
    let n = s.trim_end_matches(' ').len();
    s.truncate(n);
}

/// The normalised document.
pub fn normalize(html: &str, fold: Fold) -> String {
    serialize(&tokens(html, fold))
}

/// Every outermost `<name>` element, normalised; `pre` selects folded code blocks.
pub fn elements(toks: &[Tok], name: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        match &toks[i] {
            Tok::Code { .. } if name == "pre" => {
                found.push(serialize(&toks[i..=i]));
                i += 1;
            }
            Tok::Open { name: n, .. } if n == name => {
                let mut depth = 0usize;
                let mut j = i;
                while j < toks.len() {
                    match &toks[j] {
                        Tok::Open { name: n, .. } if n == name => depth += 1,
                        Tok::Close(n) if n == name => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
                let end = j.min(toks.len() - 1);
                found.push(serialize(&toks[i..=end]));
                i = end + 1;
            }
            _ => i += 1,
        }
    }
    found
}

/// Σ min(count in `want`, count in `got`): multiset matches, robust to one missing element.
pub fn multiset_matches(want: &[String], got: &[String]) -> usize {
    let mut pool: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for g in got {
        *pool.entry(g.as_str()).or_default() += 1;
    }
    want.iter()
        .filter(|w| {
            pool.get_mut(w.as_str())
                .filter(|n| **n > 0)
                .map(|n| *n -= 1)
                .is_some()
        })
        .count()
}
