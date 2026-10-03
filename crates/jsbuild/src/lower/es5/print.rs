//! Printing the lowered program.
//!
//! oxc's code generator prints ES5 from an ES5 AST with one exception: when minifying it quotes
//! strings with backticks (template literals, ES2015) whenever that is not longer. Those are
//! re-quoted with `"` afterwards, and the source map is moved along.

use std::path::PathBuf;

use oxc::allocator::Allocator;
use oxc::ast::ast::{Expression, ExpressionStatement, Program, TemplateLiteral};
use oxc::ast_visit::{Visit, walk};
use oxc::codegen::{Codegen, CodegenOptions, CommentOptions, LegalComment};
use oxc::parser::Parser;
use oxc::span::{SourceType, Span};
use oxc_sourcemap::{SourceMap, Token};

use super::Lowered;

/// The name the input bundle has in the source map (only its positions matter to the caller,
/// which collapses this map with Rolldown's).
const SOURCE_NAME: &str = "es2015.js";

pub(super) fn print(program: &Program<'_>, minify: bool, sourcemap: bool) -> Lowered {
    let comments = if minify {
        CommentOptions {
            normal: false,
            jsdoc: false,
            annotation: true,
            legal: LegalComment::Inline,
        }
    } else {
        CommentOptions::default()
    };
    let options = CodegenOptions {
        minify,
        comments,
        source_map_path: sourcemap.then(|| PathBuf::from(SOURCE_NAME)),
        ..CodegenOptions::default()
    };
    let ret = Codegen::new().with_options(options).build(program);
    let map = ret.map.map(SourceMap::into_owned);
    if minify {
        let (code, map) = requote(ret.code, map);
        Lowered { code, map }
    } else {
        Lowered {
            code: ret.code,
            map,
        }
    }
}

/// The template literals without substitutions in `code` (all from string literals by now),
/// and whether each is a whole expression statement (which oxc prints as a template so that it
/// does not read as a directive; it keeps parentheses when re-quoted).
struct Backticks(Vec<(Span, bool)>);

impl<'a> Visit<'a> for Backticks {
    fn visit_expression_statement(&mut self, it: &ExpressionStatement<'a>) {
        if let Expression::TemplateLiteral(t) = &it.expression
            && t.expressions.is_empty()
        {
            self.0.push((t.span, true));
            return;
        }
        walk::walk_expression_statement(self, it);
    }

    fn visit_template_literal(&mut self, it: &TemplateLiteral<'a>) {
        if it.expressions.is_empty() {
            self.0.push((it.span, false));
        }
        walk::walk_template_literal(self, it);
    }
}

/// `code` with its backtick strings quoted with `"`, and `map` adjusted.
fn requote(code: String, map: Option<SourceMap<'static>>) -> (String, Option<SourceMap<'static>>) {
    if !code.contains('`') {
        return (code, map);
    }
    let allocator = Allocator::default();
    let ret = Parser::new(&allocator, &code, SourceType::unambiguous()).parse();
    if ret.diagnostics.has_errors() {
        return (code, map);
    }
    let mut found = Backticks(Vec::new());
    found.visit_program(&ret.program);
    if found.0.is_empty() {
        return (code, map);
    }
    let mut spans = found.0;
    spans.sort_by_key(|(s, _)| s.start);
    // (old start, old end, new end) of each replacement, for the source map.
    let mut edits: Vec<(usize, usize, usize)> = Vec::with_capacity(spans.len());
    let mut out = String::with_capacity(code.len() + spans.len() * 2);
    let mut last = 0usize;
    for &(span, statement) in &spans {
        let (start, end) = (span.start as usize, span.end as usize);
        out.push_str(&code[last..start]);
        let new_start = out.len();
        if statement {
            out.push('(');
        }
        out.push('"');
        let mut chars = code[start + 1..end - 1].chars();
        while let Some(c) = chars.next() {
            match c {
                '\\' => {
                    out.push('\\');
                    if let Some(n) = chars.next() {
                        out.push(n);
                    }
                }
                '"' => out.push_str("\\\""),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\u{2028}' => out.push_str("\\u2028"),
                '\u{2029}' => out.push_str("\\u2029"),
                c => out.push(c),
            }
        }
        out.push('"');
        if statement {
            out.push(')');
        }
        edits.push((start, end, out.len() - new_start + start));
        last = end;
    }
    out.push_str(&code[last..]);
    let map = map.map(|m| remap(&code, &out, &edits, m));
    (out, map)
}

/// Byte offsets of the line starts of generated code (`\n` only: the code generator escapes
/// every other line terminator).
fn line_starts(code: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(code.match_indices('\n').map(|(i, _)| i + 1))
        .collect()
}

fn utf16_to_byte(line: &str, col: u32) -> usize {
    let mut units = 0u32;
    for (i, c) in line.char_indices() {
        if units >= col {
            return i;
        }
        units += u32::try_from(c.len_utf16()).unwrap_or(1);
    }
    line.len()
}

fn remap(
    old: &str,
    new: &str,
    edits: &[(usize, usize, usize)],
    map: SourceMap<'static>,
) -> SourceMap<'static> {
    let old_lines = line_starts(old);
    let new_lines = line_starts(new);
    // prefix[i]: the length change of the edits before edit i.
    let mut prefix = Vec::with_capacity(edits.len() + 1);
    let mut sum = 0isize;
    prefix.push(0isize);
    for &(_, end, new_end) in edits {
        sum += isize::try_from(new_end).unwrap_or(0) - isize::try_from(end).unwrap_or(0);
        prefix.push(sum);
    }
    let mut parts = map.into_parts();
    let tokens: Vec<Token> = parts
        .tokens
        .iter()
        .map(|t| {
            let line = t.get_dst_line() as usize;
            let line_start = old_lines.get(line).copied().unwrap_or(old.len());
            let line_end = old_lines.get(line + 1).copied().unwrap_or(old.len());
            let offset = line_start + utf16_to_byte(&old[line_start..line_end], t.get_dst_col());
            let done = edits.partition_point(|&(_, end, _)| end <= offset);
            // A position inside a re-quoted string maps to its start.
            let base = match edits.get(done) {
                Some(&(start, _, _)) if start <= offset => start,
                _ => offset,
            };
            let shifted =
                usize::try_from(isize::try_from(base).unwrap_or(0) + prefix[done]).unwrap_or(0);
            let new_line = new_lines
                .partition_point(|&s| s <= shifted)
                .saturating_sub(1);
            let col: usize = new[new_lines[new_line]..shifted.min(new.len())]
                .chars()
                .map(char::len_utf16)
                .sum();
            Token::new(
                u32::try_from(new_line).unwrap_or(u32::MAX),
                u32::try_from(col).unwrap_or(u32::MAX),
                t.get_src_line(),
                t.get_src_col(),
                t.get_source_id(),
                t.get_name_id(),
            )
        })
        .collect();
    parts.tokens = tokens.into_boxed_slice();
    parts.token_chunks = None;
    SourceMap::from_parts(parts)
}
