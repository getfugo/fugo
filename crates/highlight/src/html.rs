//! The HTML the Go implementation writes for highlighted code: Chroma's HTML formatter (line
//! spans, line numbers inline or in a table, highlighted lines, classes or inline styles)
//! inside the Go implementation's wrappers (`<div class="highlight">`, `<pre tabindex="0">`,
//! `<code class="language-x" data-lang="x">`, inline code).
//!
//! The markup is Chroma's (`formatters/html/html.go`, v2.19.0, MIT) and the Go
//! implementation's (`markup/highlight/highlight.go`, Apache-2.0), rewritten.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use ssg_base::{Map, Value};

use crate::options::{CodeLayout, LineNumberLayout, Options, Styling};
use crate::token::TokenType;

/// Go's `html.EscapeString`.
pub(crate) fn escape(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '\'' => out.push_str("&#39;"),
            '"' => out.push_str("&#34;"),
            _ => out.push(c),
        }
    }
}

/// Code without a known lexer: escaped, in Go's `<pre><code>` (no `<div>`) or inline code.
pub(crate) fn plain(code: &str, lang: &str, layout: CodeLayout) -> String {
    let mut out = String::with_capacity(code.len() + 64);
    match layout {
        CodeLayout::Inline => {
            inline_code_start(&mut out, lang);
            escape(&mut out, code);
            out.push_str("</code>");
        }
        CodeLayout::Block => {
            pre_start(&mut out, lang, "");
            escape(&mut out, code);
            out.push_str("</code></pre>");
        }
    }
    out
}

fn inline_code_start(out: &mut String, lang: &str) {
    out.push_str("<code class=\"code-inline language-");
    out.push_str(lang);
    out.push_str("\">");
}

/// Go's `WritePreStart`.
fn pre_start(out: &mut String, lang: &str, style_attr: &str) {
    out.push_str("<pre tabindex=\"0\"");
    out.push_str(style_attr);
    out.push_str("><code");
    if !lang.is_empty() {
        let _ = write!(out, " class=\"language-{lang}\" data-lang=\"{lang}\"");
    }
    out.push('>');
}

/// The inputs of one formatted block besides its tokens.
pub(crate) struct Block<'a> {
    pub lang: &'a str,
    pub options: &'a Options,
    /// The fence's attributes (a `class` joins the wrapper class; the others are written on
    /// the `<div>`).
    pub attributes: Option<&'a Map>,
    /// Compressed inline declarations per token type (inline styling only).
    pub inline: &'a BTreeMap<TokenType, String>,
}

impl Block<'_> {
    /// ` class="…"` or ` style="…"` for `t` (Chroma's `styleAttr`).
    fn attr(&self, t: TokenType, extra: &str) -> String {
        match self.options.styling {
            Styling::Classes => {
                let class = t.class();
                if class.is_empty() {
                    String::new()
                } else {
                    format!(" class=\"{class}\"")
                }
            }
            Styling::Inline => {
                let css = self
                    .inline
                    .get(&t)
                    .or_else(|| t.sub_category().and_then(|s| self.inline.get(&s)))
                    .or_else(|| t.category().and_then(|c| self.inline.get(&c)));
                match css {
                    None => String::new(),
                    Some(css) if extra.is_empty() => format!(" style=\"{css}\""),
                    Some(css) => format!(" style=\"{css};{extra}\""),
                }
            }
        }
    }

    fn css(&self, t: TokenType) -> &str {
        self.inline.get(&t).map_or("", String::as_str)
    }

    /// Formats `tokens` (coalesced, in source order).
    pub fn format(&self, tokens: &[(TokenType, &str)]) -> String {
        let o = self.options;
        let mut out = String::new();
        let inline_code = o.layout == CodeLayout::Inline;
        if !inline_code {
            self.div_start(&mut out);
        }
        let lines = split_lines(tokens);
        let ranges = o.highlight_ranges();
        let last = o.line_no_start + i64::try_from(lines.len()).unwrap_or(i64::MAX) - 1;
        let digits = last.to_string().len();
        let in_table = o.line_nos && o.line_number_layout == LineNumberLayout::Table;
        let anchor_prefix = if o.line_anchors.is_empty() {
            String::new()
        } else {
            format!("{}-", o.line_anchors)
        };
        let pre = self.attr(TokenType::PreWrapper, "");
        let highlighted = |line: i64| ranges.iter().any(|r| r[0] <= line && line <= r[1]);
        let number = |out: &mut String, t: TokenType, line: i64, newline: bool| {
            out.push_str("<span");
            out.push_str(&self.attr(t, ""));
            if o.anchor_line_nos {
                let _ = write!(out, " id=\"{anchor_prefix}{line}\"");
            }
            out.push('>');
            let title = format!("{line:>digits$}");
            if o.anchor_line_nos {
                let link = self.attr(TokenType::LineLink, "");
                let _ = write!(out, "<a{link} href=\"#{anchor_prefix}{line}\">{title}</a>");
            } else {
                out.push_str(&title);
            }
            if newline {
                out.push('\n');
            }
            out.push_str("</span>");
        };

        if in_table {
            let _ = writeln!(out, "<div{pre}>");
            let _ = write!(out, "<table{}><tr>", self.attr(TokenType::LineTable, ""));
            let _ = writeln!(out, "<td{}>", self.attr(TokenType::LineTableTd, ""));
            if !inline_code {
                pre_start(&mut out, "", &pre);
            }
            for i in 0..lines.len() {
                let line = o.line_no_start + i64::try_from(i).unwrap_or(i64::MAX);
                let hl = highlighted(line);
                if hl {
                    let _ = write!(out, "<span{}>", self.attr(TokenType::LineHighlight, ""));
                }
                number(&mut out, TokenType::LineNumbersTable, line, true);
                if hl {
                    out.push_str("</span>");
                }
            }
            if !inline_code {
                out.push_str("</code></pre>");
            }
            out.push_str("</td>\n");
            let _ = writeln!(
                out,
                "<td{}>",
                self.attr(TokenType::LineTableTd, "width:100%")
            );
        }

        if inline_code {
            inline_code_start(&mut out, self.lang);
        } else {
            pre_start(&mut out, self.lang, &pre);
        }
        for (i, line_tokens) in lines.iter().enumerate() {
            let line = o.line_no_start + i64::try_from(i).unwrap_or(i64::MAX);
            if !inline_code {
                out.push_str("<span");
                if highlighted(line) {
                    match o.styling {
                        Styling::Classes => {
                            let _ = write!(
                                out,
                                " class=\"{} {}\"",
                                TokenType::Line.class(),
                                TokenType::LineHighlight.class()
                            );
                        }
                        Styling::Inline => {
                            let _ = write!(
                                out,
                                " style=\"{} {}\"",
                                self.css(TokenType::Line),
                                self.css(TokenType::LineHighlight)
                            );
                        }
                    }
                } else {
                    out.push_str(&self.attr(TokenType::Line, ""));
                }
                out.push('>');
                if o.line_nos && !in_table {
                    number(&mut out, TokenType::LineNumbers, line, false);
                }
                let _ = write!(out, "<span{}>", self.attr(TokenType::CodeLine, ""));
            }
            for (t, text) in line_tokens {
                let attr = self.attr(*t, "");
                if attr.is_empty() {
                    escape(&mut out, text);
                } else {
                    let _ = write!(out, "<span{attr}>");
                    escape(&mut out, text);
                    out.push_str("</span>");
                }
            }
            if !inline_code {
                out.push_str("</span></span>");
            }
        }
        out.push_str(if inline_code {
            "</code>"
        } else {
            "</code></pre>"
        });
        if in_table {
            out.push_str("</td></tr></table>\n</div>\n");
        }
        if !inline_code {
            out.push_str("</div>");
        }
        out
    }

    /// Go's `writeDivStart`: the wrapper class plus the fence's `class`, then the other
    /// attributes.
    fn div_start(&self, out: &mut String) {
        out.push_str("<div class=\"");
        out.push_str(&self.options.wrapper_class);
        let attrs = self.attributes.into_iter().flat_map(Map::iter);
        if let Some((_, class)) = self
            .attributes
            .into_iter()
            .flat_map(Map::iter)
            .find(|(k, _)| *k == "class")
        {
            out.push(' ');
            escape(out, &attr_value(class));
        }
        out.push('"');
        for (k, v) in attrs.filter(|(k, _)| !k.eq_ignore_ascii_case("class")) {
            let _ = write!(out, " {k}=\"");
            escape(out, &attr_value(v));
            out.push('"');
        }
        out.push('>');
    }
}

/// An attribute value as text (Go's `cast.ToString`).
fn attr_value(v: &Value) -> String {
    match v {
        Value::String(s) => s.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => f.to_string(),
        _ => String::new(),
    }
}

/// Chroma's `SplitTokensIntoLines`: each line keeps its `\n` at the end of its last token; an
/// empty last line is dropped.
fn split_lines<'a>(tokens: &[(TokenType, &'a str)]) -> Vec<Vec<(TokenType, &'a str)>> {
    let mut lines = Vec::new();
    let mut line = Vec::new();
    for &(t, mut text) in tokens {
        while let Some(at) = text.find('\n') {
            line.push((t, &text[..=at]));
            lines.push(std::mem::take(&mut line));
            text = &text[at + 1..];
        }
        line.push((t, text));
    }
    if !line.is_empty() {
        lines.push(line);
    }
    if lines
        .last()
        .is_some_and(|l: &Vec<(TokenType, &str)>| l.len() == 1 && l[0].1.is_empty())
    {
        lines.pop();
    }
    lines
}
