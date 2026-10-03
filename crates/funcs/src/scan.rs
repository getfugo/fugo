//! A small tokenizer for Tera template tags, for static checks Tera does not make when templates
//! are added (REWRITE_PLAN.md §4.8): the kwargs of calls (ssg-testkit's contract test) and
//! the lints of `templates check`.
//!
//! [`tags`] splits a source into its `{{ … }}` and `{% … %}` tags (skipping `{# #}` comments and
//! `{% raw %}` sections) and tokenizes each; [`scan_calls`] reads the filter, function and test
//! calls of a template from them. The tokenizer is not a parser: it knows brace depth, strings
//! and identifiers, which is what the checks need.

use crate::spec::NameKind;

/// One token of a tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tok<'s> {
    Ident(&'s str),
    /// A string literal's content, without its quotes (escapes left as written).
    Str(&'s str),
    Num,
    /// Operators and brackets; `==`, `!=`, `<=`, `>=`, `?.`, `?[`, `//`, `**`, `</` are one
    /// token.
    Punct(&'s str),
}

/// Whether a tag prints (`{{ … }}`) or is a statement (`{% … %}`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TagKind {
    Expr,
    Stmt,
}

/// One tag: its kind, where it starts, its tokens (with byte offsets into the source) and where
/// it ends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tag<'s> {
    pub kind: TagKind,
    /// The offset of the opening `{{` / `{%`.
    pub start: usize,
    /// The offset after the closing `}}` / `%}`.
    pub end: usize,
    pub toks: Vec<(Tok<'s>, usize)>,
    /// Offsets of `}}` inside a nested map literal of a `{{ … }}` tag: Tera ends the tag there
    /// (write `} }`).
    pub inner_closes: Vec<usize>,
}

impl<'s> Tag<'s> {
    /// The index of the first token that is not the `-` of whitespace control.
    #[must_use]
    pub fn first(&self) -> Option<usize> {
        self.toks.iter().position(|(t, _)| *t != Tok::Punct("-"))
    }

    /// The statement keyword (`for`, `set`, `component`, …) of a `{% %}` tag.
    #[must_use]
    pub fn keyword(&self) -> Option<&'s str> {
        match (self.kind, self.first().map(|i| self.toks[i].0)) {
            (TagKind::Stmt, Some(Tok::Ident(k))) => Some(k),
            _ => None,
        }
    }
}

/// One filter, function or test call found in a template.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    pub kind: NameKind,
    pub name: String,
    pub kwargs: Vec<String>,
    /// 1-based line of the name.
    pub line: usize,
    /// Byte offset of the name.
    pub offset: usize,
}

/// Words that are never function names.
pub const KEYWORDS: &[&str] = &[
    "and",
    "or",
    "not",
    "is",
    "in",
    "if",
    "elif",
    "else",
    "endif",
    "for",
    "endfor",
    "set",
    "set_global",
    "endset",
    "block",
    "endblock",
    "extends",
    "include",
    "filter",
    "endfilter",
    "component",
    "endcomponent",
    "raw",
    "endraw",
    "break",
    "continue",
    "true",
    "false",
    "none",
    "True",
    "False",
];

/// The filter, function and test calls of a template, in source order. Comments and
/// `{% raw %}` sections are skipped; component definitions are not calls.
#[must_use]
pub fn scan_calls(source: &str) -> Vec<Call> {
    let mut calls = Vec::new();
    for tag in tags(source) {
        calls.extend(calls_in_tag(source, &tag));
    }
    calls
}

/// The 1-based line of `offset`.
#[must_use]
pub fn line_of(source: &str, offset: usize) -> usize {
    source[..offset].bytes().filter(|b| *b == b'\n').count() + 1
}

/// The 1-based line and column (in characters) of `offset`.
#[must_use]
pub fn line_col(source: &str, offset: usize) -> (usize, usize) {
    let before = &source[..offset];
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    (
        line_of(source, offset),
        before[line_start..].chars().count() + 1,
    )
}

/// Splits the source into tags and tokenizes each; `{# #}` comments and raw sections are skipped.
#[must_use]
pub fn tags(source: &str) -> Vec<Tag<'_>> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(off) = source[i..].find('{') {
        let start = i + off;
        match bytes.get(start + 1) {
            Some(b'#') => {
                i = source[start..]
                    .find("#}")
                    .map_or(bytes.len(), |e| start + e + 2);
            }
            Some(b'{' | b'%') => {
                let kind = if bytes[start + 1] == b'{' {
                    TagKind::Expr
                } else {
                    TagKind::Stmt
                };
                let tag = lex_tag(source, start, kind);
                i = tag.end;
                if tag.keyword() == Some("raw") {
                    i = find_endraw(source, tag.end);
                } else {
                    out.push(tag);
                }
            }
            _ => i = start + 1,
        }
    }
    out
}

fn find_endraw(source: &str, from: usize) -> usize {
    let mut i = from;
    while let Some(off) = source[i..].find("{%") {
        let start = i + off;
        let tag = lex_tag(source, start, TagKind::Stmt);
        if tag.keyword() == Some("endraw") {
            return tag.end;
        }
        i = tag.end;
    }
    source.len()
}

/// Tokenizes the tag opening at `start` up to its closing `}}` or `%}` at brace depth 0.
fn lex_tag(source: &str, start: usize, kind: TagKind) -> Tag<'_> {
    let bytes = source.as_bytes();
    let mut tag = Tag {
        kind,
        start,
        end: bytes.len(),
        toks: Vec::new(),
        inner_closes: Vec::new(),
    };
    let mut pos = start + 2;
    let mut depth = 0usize;
    while pos < bytes.len() {
        let c = bytes[pos];
        let next = bytes.get(pos + 1).copied();
        match c {
            b' ' | b'\t' | b'\r' | b'\n' => pos += 1,
            b'}' if depth == 0 && next == Some(b'}') => {
                tag.end = pos + 2;
                return tag;
            }
            b'%' if depth == 0 && next == Some(b'}') => {
                tag.end = pos + 2;
                return tag;
            }
            b'"' | b'\'' | b'`' => {
                let close = source[pos + 1..]
                    .find(c as char)
                    .map_or(bytes.len(), |e| pos + 1 + e);
                tag.toks.push((Tok::Str(&source[pos + 1..close]), pos));
                pos = (close + 1).min(bytes.len());
            }
            b'0'..=b'9' => {
                let len = source[pos..]
                    .bytes()
                    .take_while(|b| b.is_ascii_alphanumeric() || *b == b'.' || *b == b'_')
                    .count();
                tag.toks.push((Tok::Num, pos));
                pos += len;
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let len = source[pos..]
                    .bytes()
                    .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
                    .count();
                tag.toks.push((Tok::Ident(&source[pos..pos + len]), pos));
                pos += len;
            }
            _ => {
                let two = source.get(pos..pos + 2).unwrap_or("");
                let len = if ["==", "!=", "<=", ">=", "?.", "?[", "//", "**", "</"].contains(&two) {
                    2
                } else {
                    // a multi-byte character outside a string: skip it whole
                    source[pos..].chars().next().map_or(1, char::len_utf8)
                };
                let p = &source[pos..pos + len];
                match p {
                    "(" | "[" | "{" | "?[" => depth += 1,
                    ")" | "]" | "}" => {
                        if p == "}" && kind == TagKind::Expr && next == Some(b'}') {
                            tag.inner_closes.push(pos);
                        }
                        depth = depth.saturating_sub(1);
                    }
                    _ => {}
                }
                tag.toks.push((Tok::Punct(p), pos));
                pos += len;
            }
        }
    }
    tag
}

/// The calls of one tag.
#[must_use]
pub fn calls_in_tag(source: &str, tag: &Tag<'_>) -> Vec<Call> {
    let mut out = Vec::new();
    let toks = &tag.toks;
    let first = tag.first();
    if tag.keyword() == Some("component") {
        return out; // a definition: its parameters are not kwargs of a call
    }
    for (i, (tok, offset)) in toks.iter().enumerate() {
        let Tok::Ident(name) = tok else { continue };
        let prev = i.checked_sub(1).map(|j| &toks[j].0);
        let prev2 = i.checked_sub(2).map(|j| &toks[j].0);
        let opens = matches!(toks.get(i + 1), Some((Tok::Punct("("), _)));
        let kind = match (prev, prev2) {
            (Some(Tok::Punct("|")), _) => NameKind::Filter,
            (Some(Tok::Ident("filter")), _) if first == Some(i - 1) => NameKind::Filter,
            (Some(Tok::Ident("is")), _) | (Some(Tok::Ident("not")), Some(Tok::Ident("is"))) => {
                if *name == "not" {
                    continue;
                }
                NameKind::Test
            }
            (Some(Tok::Punct("." | "?.")), _) => continue,
            _ if opens && !KEYWORDS.contains(name) => NameKind::Function,
            _ => continue,
        };
        let kwargs = if opens {
            kwargs_at(toks, i + 1)
        } else {
            Vec::new()
        };
        out.push(Call {
            kind,
            name: (*name).to_owned(),
            kwargs,
            line: line_of(source, *offset),
            offset: *offset,
        });
    }
    out
}

/// The index of the `)` closing the bracket opened at `open` (or the last token).
#[must_use]
pub fn matching_close(toks: &[(Tok<'_>, usize)], open: usize) -> usize {
    let mut depth = 0usize;
    for (j, (t, _)) in toks.iter().enumerate().skip(open) {
        match t {
            Tok::Punct("(" | "[" | "{" | "?[") => depth += 1,
            Tok::Punct(")" | "]" | "}") => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return j;
                }
            }
            _ => {}
        }
    }
    toks.len().saturating_sub(1)
}

/// The calls of `FUNCS` names with an undeclared kwarg or without a required one, with the
/// message. Names unknown to `FUNCS` are left to Tera's own validation.
#[must_use]
pub fn kwarg_errors(source: &str) -> Vec<(Call, String)> {
    scan_calls(source)
        .into_iter()
        .filter_map(|call| {
            let spec = crate::spec::FUNCS
                .iter()
                .find(|f| f.name == call.name && f.kind == call.kind)?;
            crate::check_kwargs(spec, call.kwargs.iter().map(String::as_str))
                .err()
                .map(|message| (call, message))
        })
        .collect()
}

/// The kwarg names of the argument list opening at `open`: identifiers followed by `=` at
/// depth 1, right after `(` or `,`.
fn kwargs_at(toks: &[(Tok<'_>, usize)], open: usize) -> Vec<String> {
    let close = matching_close(toks, open);
    let mut names = Vec::new();
    let mut depth = 0usize;
    for j in open..=close.min(toks.len().saturating_sub(1)) {
        match &toks[j].0 {
            Tok::Punct("(" | "[" | "{" | "?[") => depth += 1,
            Tok::Punct(")" | "]" | "}") => depth = depth.saturating_sub(1),
            Tok::Ident(name)
                if depth == 1
                    && matches!(toks.get(j + 1), Some((Tok::Punct("="), _)))
                    && matches!(toks[j - 1].0, Tok::Punct("(" | ",")) =>
            {
                names.push((*name).to_owned());
            }
            _ => {}
        }
    }
    names
}
