//! goldmark's pipe tables: a paragraph transformer, not a block parser (goldmark v1.7.12
//! `extension/table.go`, `tableParagraphTransformer`).
//!
//! comrak's table extension follows cmark-gfm, where a table is a block of its own. goldmark
//! instead parses paragraphs first and turns a paragraph into a table when one of its lines
//! (after the first) is a delimiter row, so:
//!
//! - a table can sit on lazy continuation lines of a list item, a blockquote or a definition
//!   (the lines belong to the paragraph);
//! - a header row with fewer cells than the delimiter row is padded with cells without
//!   alignment (one with more cells means no table, and goldmark stops looking);
//! - every paragraph line after the delimiter row is a body row (more cells than columns are
//!   dropped, fewer are padded without alignment);
//! - the lines before the header row stay a paragraph;
//! - the new table has no blank line before it, so a blank line before a table that fills a
//!   list item's later paragraph does not make the list loose;
//! - a setext underline or a definition's `:` after such a paragraph sees the table first
//!   ([`retry`]);
//! - a task item's `[ ]` is text of the header row when the table starts on the item's first
//!   line ([`task_marker`]).
//!
//! comrak parses with tables off (so its paragraphs are goldmark's), this pass finds goldmark's
//! tables in the paragraphs' lines, and the cells' inline content is parsed by comrak from a
//! synthetic document: every table rebuilt as a canonical GFM table (one cell per goldmark
//! cell, so comrak's cells are goldmark's), followed by the whole page (its delimiter rows
//! defused) for its link reference and footnote definitions. The parsed tables are put in
//! place with positions mapped back; a table whose parse does not have the expected shape
//! leaves its paragraph alone, and only it. The lines before a header keep the paragraph's
//! own inlines, cut at the header row, unless an inline runs on into it; then they are parsed
//! again as a paragraph of their own in the synthetic document as well.

mod retry;

use std::collections::{HashMap, HashSet};
use std::ops::Range;

use comrak::nodes::{LineColumn, NodeHeading, NodeValue, Sourcepos, TableAlignment};
use comrak::{Options, parse_document};

use self::retry::Keep;

use crate::doc::{Doc, Node, NodeKey, Role};
use crate::source::{CONTEXT_CLOSE, CONTEXT_OPEN, Lines};

mod fixups;
mod graft;
mod plan;

use fixups::*;
use graft::*;
pub(crate) use plan::*;

/// goldmark's `util.IsSpace`.
fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// goldmark's `util.IndentWidth(bs, 0)`: the width of the leading spaces and tabs.
fn indent_width(bs: &[u8]) -> usize {
    let mut w = 0;
    for &b in bs {
        match b {
            b' ' => w += 1,
            b'\t' => w += 4 - w % 4,
            _ => break,
        }
    }
    w
}

/// `r` without goldmark's leading and trailing spaces (`TrimLeftSpace`, `TrimRightSpace`).
fn trim(text: &[u8], mut r: Range<usize>) -> Range<usize> {
    while r.start < r.end && is_space(text[r.start]) {
        r.start += 1;
    }
    while r.end > r.start && is_space(text[r.end - 1]) {
        r.end -= 1;
    }
    r
}

/// `r` without goldmark's leading spaces (`TrimLeftSpace`).
fn trim_start(text: &[u8], mut r: Range<usize>) -> Range<usize> {
    while r.start < r.end && is_space(text[r.start]) {
        r.start += 1;
    }
    r
}

/// goldmark's `isTableDelim`: at most 3 columns of indentation, then only spaces, `-`, `|`
/// and `:`.
fn is_table_delim(line: &[u8]) -> bool {
    indent_width(line) <= 3
        && line
            .iter()
            .all(|&b| is_space(b) || matches!(b, b'-' | b'|' | b':'))
}

/// goldmark's `parseDelimiter`: the column alignments of a delimiter row.
fn parse_delimiter(line: &[u8]) -> Option<Vec<TableAlignment>> {
    if !is_table_delim(line) {
        return None;
    }
    let mut cols: Vec<&[u8]> = line.split(|&b| b == b'|').collect();
    if cols.first().is_some_and(|c| c.iter().all(|&b| is_space(b))) {
        cols.remove(0);
    }
    if cols.last().is_some_and(|c| c.iter().all(|&b| is_space(b))) {
        cols.pop();
    }
    let mut out = Vec::with_capacity(cols.len());
    for col in cols {
        // `^\s*:-+\s*$`, `^\s*-+:\s*$`, `^\s*:-+:\s*$`, `^\s*-+\s*$` (Go's `\s`).
        let r = trim(col, 0..col.len());
        let c = &col[r];
        let left = c.first() == Some(&b':');
        let right = c.len() > 1 && c.last() == Some(&b':');
        let dashes = &c[usize::from(left)..c.len() - usize::from(right)];
        if dashes.is_empty() || dashes.iter().any(|&b| b != b'-') {
            return None;
        }
        out.push(match (left, right) {
            (true, true) => TableAlignment::Center,
            (true, false) => TableAlignment::Left,
            (false, true) => TableAlignment::Right,
            (false, false) => TableAlignment::None,
        });
    }
    (!out.is_empty()).then_some(out)
}

/// A cell of a goldmark row.
#[derive(Clone, Debug)]
enum Cell {
    /// The cell's trimmed source range (possibly empty); it has its column's alignment.
    Text(Range<usize>),
    /// A cell goldmark added to complete a short row (no alignment).
    Pad,
}

/// A row: its trimmed source range and cells.
#[derive(Clone, Debug)]
struct Row {
    span: Range<usize>,
    cells: Vec<Cell>,
}

/// goldmark's `parseRow` of the line at `seg` of `text`.
fn parse_row(text: &[u8], seg: Range<usize>, alignments: &[TableAlignment], header: bool) -> Row {
    let span = trim(text, seg);
    let line = &text[span.clone()];
    let mut pos = 0;
    let mut limit = line.len();
    if line.first() == Some(&b'|') {
        pos += 1;
    }
    if limit > 0 && line[limit - 1] == b'|' {
        limit -= 1;
    }
    let mut cells = Vec::new();
    let mut i = 0;
    while pos < limit {
        if i >= alignments.len() && !header {
            return Row { span, cells };
        }
        let mut closure = pos;
        while closure < limit {
            if line[closure] == b'|' && (closure == 0 || line[closure - 1] != b'\\') {
                break;
            }
            closure += 1;
        }
        let r = trim(text, span.start + pos..span.start + closure);
        cells.push(Cell::Text(r));
        pos = closure + 1;
        i += 1;
    }
    while i < alignments.len() {
        cells.push(Cell::Pad);
        i += 1;
    }
    Row { span, cells }
}

/// A table goldmark makes of a paragraph.
struct Found {
    /// The paragraph lines before the header row.
    before: Vec<Range<usize>>,
    alignments: Vec<TableAlignment>,
    header: Row,
    body: Vec<Row>,
}

/// goldmark's `Transform` over a paragraph's lines (byte ranges of `text`).
fn transform(text: &[u8], lines: &[Range<usize>]) -> Option<Found> {
    for i in 1..lines.len() {
        let Some(alignments) = parse_delimiter(&text[lines[i].clone()]) else {
            continue;
        };
        let header = parse_row(text, lines[i - 1].clone(), &alignments, true);
        if header.cells.len() != alignments.len() {
            return None;
        }
        let body = lines[i + 1..]
            .iter()
            .map(|l| parse_row(text, l.clone(), &alignments, false))
            .collect();
        return Some(Found {
            before: lines[..i - 1].to_vec(),
            alignments,
            header,
            body,
        });
    }
    None
}

/// A container's line prefix, as comrak matches it (`parser::check_open_blocks`).
#[derive(Clone, Copy)]
enum Prefix {
    /// Up to 3 spaces, `>`, an optional space.
    Quote,
    /// This many columns of indentation.
    Indent(usize),
}

/// The prefixes of the containers around `n`, outermost first.
fn prefixes(n: Node<'_>) -> Vec<Prefix> {
    let mut out: Vec<Prefix> = n
        .ancestors()
        .skip(1)
        .filter_map(|a| match &a.data().value {
            NodeValue::BlockQuote => Some(Prefix::Quote),
            NodeValue::Item(nl) => Some(Prefix::Indent(nl.marker_offset + nl.padding)),
            NodeValue::TaskItem(_) => a.parent().and_then(|l| match &l.data().value {
                NodeValue::List(nl) => Some(Prefix::Indent(nl.marker_offset + nl.padding)),
                _ => None,
            }),
            NodeValue::DescriptionItem(di) => Some(Prefix::Indent(di.marker_offset + di.padding)),
            NodeValue::FootnoteDefinition(_) => Some(Prefix::Indent(4)),
            _ => None,
        })
        .collect();
    out.reverse();
    out
}

/// The offset where the paragraph content of the line starting at `at` begins: after the
/// prefixes its containers match; a line that stops matching is a lazy continuation line,
/// whose content is the rest of the line (goldmark's segment keeps its leading spaces).
fn content_start(text: &[u8], at: usize, prefixes: &[Prefix]) -> usize {
    let mut pos = at;
    let col = |p: usize| p - at;
    for p in prefixes {
        match *p {
            Prefix::Quote => {
                let spaces = text[pos..].iter().take_while(|&&b| b == b' ').count();
                if spaces > 3 || text.get(pos + spaces) != Some(&b'>') {
                    return pos;
                }
                pos += spaces + 1;
                if matches!(text.get(pos), Some(b' ' | b'\t')) {
                    pos += 1;
                }
            }
            Prefix::Indent(n) => {
                let mut width = 0;
                let mut q = pos;
                while width < n {
                    match text.get(q) {
                        Some(b' ') => width += 1,
                        Some(b'\t') => width += 4 - (col(q) % 4),
                        _ => break,
                    }
                    q += 1;
                }
                if width < n {
                    return pos;
                }
                pos = q;
            }
        }
    }
    pos
}

/// The lines of paragraph `p` (up to line `last`) as goldmark's paragraph parser holds them
/// when it closes: the first without its leading spaces and without the link reference
/// definitions that began the paragraph, the others after their containers' prefixes; no
/// newlines.
fn paragraph_lines(doc: &Doc<'_>, p: Node<'_>, last: usize) -> Vec<Range<usize>> {
    let sp = p.data().sourcepos;
    let start = if doc
        .src
        .text
        .get(doc.start(p)..)
        .is_some_and(|t| t.starts_with('['))
    {
        // Leading link reference definitions are gone (their lines hold no inline).
        p.descendants()
            .skip(1)
            .map(|d| d.data().sourcepos.start)
            .min()
            .unwrap_or(sp.start)
    } else {
        sp.start
    };
    let text = doc.src.text.as_bytes();
    let line_end = |line: usize| -> usize {
        let s = doc.src.offset(LineColumn { line, column: 1 });
        let l = doc.line(line);
        s + l.trim_end_matches('\r').len()
    };
    let prefixes = prefixes(p);
    let mut out = Vec::with_capacity((last + 1).saturating_sub(start.line));
    out.push(doc.src.offset(start)..line_end(start.line));
    for line in start.line + 1..=last {
        let at = doc.src.offset(LineColumn { line, column: 1 });
        out.push(content_start(text, at, &prefixes)..line_end(line));
    }
    out
}

/// Whether a line after the first has a `-` (a delimiter row needs one): the cheap test
/// before [`transform`].
fn may_have_delimiter(text: &[u8], lines: &[Range<usize>]) -> bool {
    lines
        .iter()
        .skip(1)
        .any(|l| text[l.clone()].contains(&b'-'))
}

/// A position of the parsed text.
fn line_column(doc: &Doc<'_>, at: usize) -> LineColumn {
    let (line, column) = doc.src.lines.line_col(at);
    LineColumn { line, column }
}

/// The source position of the non-empty range `r` of the parsed text.
fn range_pos(doc: &Doc<'_>, r: &Range<usize>) -> Sourcepos {
    Sourcepos {
        start: line_column(doc, r.start),
        end: line_column(doc, r.end.max(r.start + 1) - 1),
    }
}

#[cfg(test)]
mod tests;
