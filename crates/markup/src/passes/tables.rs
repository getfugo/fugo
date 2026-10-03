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

/// A synthetic-text range that came from `orig` (an offset of the parsed text).
struct Piece {
    synth: Range<usize>,
    orig: usize,
}

/// One table's part of the synthetic document.
struct Planned<'a> {
    paragraph: Node<'a>,
    found: Found,
    /// What the lines before the header become when a block needed them ([`retry`]).
    keep: Option<Keep<'a>>,
    /// Whether the paragraph's own inlines can stay for the lines before the header (none of
    /// them reaches into the header row), so that only the table is parsed again.
    split: bool,
    /// The task item whose checkbox goldmark reads as text of the header row
    /// ([`task_marker`]).
    untask: Option<Node<'a>>,
}

/// The task item of paragraph `p` and the offset of its `[ ]` marker, when `p` is the item's
/// first paragraph. goldmark's checkbox is an inline of the item's first paragraph
/// (`extension/tasklist.go`), so the marker is paragraph text to the table transformer; comrak
/// takes it off the paragraph at parse time.
fn task_marker<'a>(doc: &Doc<'_>, p: Node<'a>) -> Option<(Node<'a>, usize)> {
    let item = p.parent()?;
    let NodeValue::TaskItem(t) = &item.data().value else {
        return None;
    };
    if !item.first_child().is_some_and(|f| f.same_node(p)) {
        return None;
    }
    let symbol = doc.src.offset(t.symbol_sourcepos.start);
    let at = symbol.checked_sub(1)?;
    (doc.src.text.as_bytes().get(at) == Some(&b'[')
        && t.symbol_sourcepos.start.line == p.data().sourcepos.start.line
        && at < doc.start(p))
    .then_some((item, at))
}

/// Whether the inlines of paragraph `p` divide at the end of line `line`: goldmark parses the
/// lines before a table's header as a paragraph of their own, so an inline that runs from
/// them into the header row (an emphasis, a code span, a link) is parsed again instead.
fn divides_at(p: Node<'_>, line: usize) -> bool {
    p.children().all(|c| {
        let sp = c.data().sourcepos;
        sp.start.line > line || sp.end.line <= line
    })
}

/// Paragraph `p` cut to its inlines up to the end of line `line`: goldmark's paragraph of the
/// lines before a table's header, which ends without the line's break (a backslash there is
/// a literal backslash).
fn cut_inlines<'a>(doc: &Doc<'a>, p: Node<'a>, line: usize) {
    for c in p
        .children()
        .filter(|c| c.data().sourcepos.start.line > line)
        .collect::<Vec<_>>()
    {
        c.detach();
    }
    while let Some(br) = p
        .last_child()
        .filter(|c| matches!(c.data().value, NodeValue::SoftBreak | NodeValue::LineBreak))
    {
        let backslash = matches!(br.data().value, NodeValue::LineBreak)
            && doc.line(line).trim_end_matches('\r').ends_with('\\');
        let sp = br.data().sourcepos;
        br.detach();
        if backslash {
            let at = Sourcepos {
                start: sp.start,
                end: sp.start,
            };
            match p.last_child() {
                Some(t) if matches!(t.data().value, NodeValue::Text(_)) => {
                    let mut d = t.data_mut();
                    if let NodeValue::Text(s) = &mut d.value {
                        s.to_mut().push('\\');
                    }
                    d.sourcepos.end = at.end;
                }
                _ => p.append(doc.node(NodeValue::Text("\\".into()), at)),
            }
        }
    }
}

/// Whether `row` is a context marker line (see [`plan`]).
fn marker_row(text: &[u8], row: &Row) -> bool {
    let t = &text[row.span.clone()];
    t == CONTEXT_OPEN.as_bytes() || t == CONTEXT_CLOSE.as_bytes()
}

/// `text` with the `-` of every line that comrak could read as a delimiter row (after
/// blockquote markers and indentation) replaced, so that the page appended to the synthetic
/// document forms no table: comrak's table would take in a link reference definition on the
/// line before its header, which goldmark resolves.
fn without_delimiter_rows(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let rest = line.trim_start_matches([' ', '\t', '>']);
        let body = rest.trim_end_matches(['\n', '\r']);
        let delimiter = body.contains('-')
            && body.contains(['|', ':'])
            && body
                .bytes()
                .all(|b| matches!(b, b'|' | b'-' | b':' | b' ' | b'\t'));
        if delimiter {
            out.push_str(&line[..line.len() - rest.len()]);
            out.push_str(&rest.replace('-', "x"));
        } else {
            out.push_str(line);
        }
    }
    out
}

/// Turns the paragraphs goldmark makes tables of into comrak tables (`copts`: the parse
/// options; tables are switched on for the cells).
pub(crate) fn tables<'a>(doc: &mut Doc<'a>, copts: &Options<'_>) {
    let keep = retry::require_paragraph(doc, copts);
    let planned = plan(doc, &keep);
    if planned.is_empty() {
        return;
    }
    let padded = graft(doc, copts, &planned);
    for c in padded {
        doc.set_role(c, Role::PaddedCell);
    }
}

/// The paragraphs that goldmark turns into tables, in document order.
fn plan<'a>(doc: &Doc<'a>, keep: &HashMap<NodeKey, Keep<'a>>) -> Vec<Planned<'a>> {
    let text = doc.src.text.as_bytes();
    // A term's paragraph is the definition's as it is, unless the first definition needed it
    // ([`retry`]).
    let paragraphs: Vec<_> = doc
        .root
        .descendants()
        .filter(|n| {
            matches!(n.data().value, NodeValue::Paragraph)
                && (!n
                    .parent()
                    .is_some_and(|p| matches!(p.data().value, NodeValue::DescriptionTerm))
                    || keep.contains_key(&NodeKey::of(n)))
        })
        .collect();
    let mut planned = Vec::new();
    for p in paragraphs {
        let sp = p.data().sourcepos;
        if sp.start.line == sp.end.line {
            continue;
        }
        let mut lines = paragraph_lines(doc, p, sp.end.line);
        if !may_have_delimiter(text, &lines) {
            continue;
        }
        let task = task_marker(doc, p);
        let comrak_start = lines[0].start;
        if let Some((_, at)) = task {
            lines[0].start = at;
        }
        let Some(mut found) = transform(text, &lines) else {
            continue;
        };
        // A table from the first line takes the checkbox's text; lines before the header keep
        // it as comrak parsed it.
        let untask = task
            .filter(|_| found.before.is_empty())
            .map(|(item, _)| item);
        if let Some(first) = found.before.first_mut() {
            first.start = comrak_start;
        }
        // The Go implementation's closing context marker after an include that ends with a
        // table is a row of empty cells there; it is not reproduced (README, accepted deviations).
        found.body.retain(|r| !marker_row(text, r));
        let split = found
            .before
            .last()
            .is_none_or(|l| divides_at(p, doc.src.lines.line_col(l.start).0));
        planned.push(Planned {
            paragraph: p,
            found,
            keep: keep.get(&NodeKey::of(p)).copied(),
            split,
            untask,
        });
    }
    planned
}

/// The synthetic document of the planned tables: per table, the lines before its header when
/// the paragraph's inlines do not divide there ([`Planned::split`]), then the table rebuilt
/// as a canonical GFM table.
///
/// The lines before a header follow a dummy paragraph line, indented by four columns: they
/// are continuation lines, as in the page, and no line can start a block there (a setext
/// underline, a definition's `:`, a delimiter row or a list marker that a container's
/// indentation or laziness kept in the paragraph), while comrak drops the indentation from the
/// paragraph's text.
struct Synth {
    text: String,
    pieces: Vec<Piece>,
    /// Per planned table, the synthetic lines (1-based) of its dummy paragraph and of its
    /// table.
    lines: Vec<(Option<usize>, usize)>,
    /// The length of the tables part (the page follows).
    len: usize,
}

/// The dummy first line of the paragraph holding the lines before a header (its text is
/// dropped).
const DUMMY: &str = "X\n";

fn synthesize(doc: &Doc<'_>, planned: &[Planned<'_>]) -> Synth {
    let text = doc.src.text.as_bytes();
    let mut synth = String::new();
    let mut pieces: Vec<Piece> = Vec::new();
    let mut lines = Vec::with_capacity(planned.len());
    let mut line = 1;
    let mut push = |synth: &mut String, r: Range<usize>| {
        pieces.push(Piece {
            synth: synth.len()..synth.len() + r.len(),
            orig: r.start,
        });
        synth.push_str(&doc.src.text[r]);
    };
    for t in planned {
        let before = (!t.split).then_some(line);
        if before.is_some() {
            synth.push_str(DUMMY);
            // goldmark keeps the lines' trailing spaces (hard line breaks); comrak drops them
            // at the end of the paragraph, as goldmark does without the newline.
            for l in &t.found.before {
                synth.push_str("    ");
                push(&mut synth, trim_start(text, l.clone()));
                synth.push('\n');
            }
            synth.push('\n');
            line += t.found.before.len() + 2;
        }
        lines.push((before, line));
        for (i, row) in std::iter::once(&t.found.header)
            .chain(&t.found.body)
            .enumerate()
        {
            synth.push('|');
            for c in &row.cells {
                synth.push(' ');
                if let Cell::Text(r) = c {
                    push(&mut synth, r.clone());
                }
                synth.push_str(" |");
            }
            synth.push('\n');
            if i == 0 {
                synth.push('|');
                for a in &t.found.alignments {
                    synth.push_str(match a {
                        TableAlignment::Left => " :-- |",
                        TableAlignment::Right => " --: |",
                        TableAlignment::Center => " :-: |",
                        TableAlignment::None => " --- |",
                    });
                }
                synth.push('\n');
                line += 1;
            }
            line += 1;
        }
        synth.push('\n');
        line += 1;
    }
    synth.push('\n');
    let len = synth.len();
    // The page follows for its link reference and footnote definitions.
    synth.push_str(&without_delimiter_rows(&doc.src.text));
    Synth {
        text: synth,
        pieces,
        lines,
        len,
    }
}

/// The inlines comrak parsed for the `n` lines before a header (`p`: the dummy paragraph):
/// the paragraph's after the dummy text and its line break, when the paragraph holds all the
/// lines.
fn before_inlines(p: Node<'_>, n: usize) -> Option<Vec<Node<'_>>> {
    if !matches!(p.data().value, NodeValue::Paragraph)
        || p.data().sourcepos.end.line != p.data().sourcepos.start.line + n
    {
        return None;
    }
    let mut children = p.children();
    let dummy = children.next()?;
    let br = children.next()?;
    let ok = matches!(&dummy.data().value, NodeValue::Text(t) if t.as_ref() == "X")
        && matches!(br.data().value, NodeValue::SoftBreak);
    ok.then(|| children.collect())
}

/// Parses the planned tables' cells (and the lines before their headers) from a synthetic
/// document and puts them in place of their paragraphs; a table whose parse does not have the
/// expected shape keeps its paragraph. Returns the padding cells.
#[expect(
    clippy::too_many_lines,
    reason = "one walk over the synthetic document"
)]
fn graft<'a>(doc: &Doc<'a>, copts: &Options<'_>, planned: &[Planned<'a>]) -> Vec<Node<'a>> {
    let text = doc.src.text.as_bytes();
    let synth = synthesize(doc, planned);
    let mut o = copts.clone();
    o.extension.table = true;
    let root = parse_document(doc.arena, &synth.text, &o);
    let synth_lines = Lines::new(&synth.text);
    let parsed: HashMap<usize, Node<'a>> = root
        .children()
        .filter(|n| synth_lines.offset(n.data().sourcepos.start, synth.text.len()) < synth.len)
        .map(|n| (n.data().sourcepos.start.line, n))
        .collect();

    // Synthetic positions back to the parsed text.
    let map = |lc: LineColumn| -> Option<LineColumn> {
        let at = synth_lines.offset(lc, synth.text.len());
        let i = synth
            .pieces
            .partition_point(|p| p.synth.start <= at)
            .checked_sub(1)?;
        let p = &synth.pieces[i];
        (at <= p.synth.end).then(|| {
            let (line, column) = doc.src.lines.line_col(p.orig + (at - p.synth.start));
            LineColumn { line, column }
        })
    };
    let lc = |at: usize| line_column(doc, at);
    let span_pos = |r: &Range<usize>| range_pos(doc, r);
    // Every inline is inside a copied range; anything else (comrak's own adjustments) takes
    // the position of the node before it.
    let relocate = |nodes: &[Node<'_>], first: Sourcepos| {
        let mut last = first;
        for d in nodes.iter().flat_map(|n| n.descendants()) {
            let sp = d.data().sourcepos;
            last = match (map(sp.start), map(sp.end)) {
                (Some(start), Some(end)) => Sourcepos { start, end },
                _ => last,
            };
            d.data_mut().sourcepos = last;
        }
    };

    let mut converted: HashSet<NodeKey> = HashSet::new();
    let mut lists: Vec<Node<'a>> = Vec::new();
    let mut padded = Vec::new();
    let mut footnotes = false;
    for (t, &(before_line, table_line)) in planned.iter().zip(&synth.lines) {
        let before = match before_line {
            None => None,
            Some(l) => match parsed
                .get(&l)
                .and_then(|q| before_inlines(q, t.found.before.len()))
            {
                Some(inlines) => Some(inlines),
                None => continue,
            },
        };
        let Some(table) = parsed.get(&table_line).copied().filter(|n| {
            matches!(n.data().value, NodeValue::Table(_))
                && n.children().count() == 1 + t.found.body.len()
                && n.children()
                    .all(|r| r.children().count() == t.found.alignments.len())
        }) else {
            continue;
        };
        let p = t.paragraph;
        // The table ends where the paragraph did (a dropped context-marker row included).
        let p_end = p.data().sourcepos.end;
        if let Some(last) = t.found.before.last() {
            if let Some(inlines) = &before {
                relocate(inlines, p.data().sourcepos);
                for c in p.children().collect::<Vec<_>>() {
                    c.detach();
                }
                for &c in inlines {
                    c.detach();
                    p.append(c);
                }
            } else {
                cut_inlines(doc, p, doc.src.lines.line_col(last.start).0);
            }
            if let Some(Keep::Heading(level, end)) = t.keep {
                {
                    let mut d = p.data_mut();
                    d.value = NodeValue::Heading(NodeHeading {
                        level,
                        setext: true,
                        closed: false,
                    });
                    d.sourcepos.end = end;
                }
                // goldmark's heading takes the paragraph's blank-line flag, which a paragraph
                // opened on an item's first line has from that line (set at the start of the
                // page); after the table it makes the list loose (`listParser.Close`).
                if let Some(item) = p.parent().filter(|i| {
                    matches!(i.data().value, NodeValue::Item(_) | NodeValue::TaskItem(_))
                        && i.first_child().is_some_and(|f| f.same_node(p))
                        && i.data().sourcepos.start.line == p.data().sourcepos.start.line
                        && (i.data().sourcepos.start.line < 2 || blank_before(doc, i))
                }) && let Some(list) = item.parent()
                    && let NodeValue::List(l) = &mut list.data_mut().value
                {
                    l.tight = false;
                }
            } else {
                p.data_mut().sourcepos.end = span_pos(&trim(text, last.clone())).end;
            }
        }
        relocate(&[table], span_pos(&t.found.header.span));
        escaped_pipes(doc, table);
        let rows = std::iter::once(&t.found.header).chain(&t.found.body);
        for (row_node, row) in table.children().zip(rows) {
            row_node.data_mut().sourcepos = span_pos(&row.span);
            for (cell_node, cell) in row_node.children().zip(&row.cells) {
                let pos = match cell {
                    Cell::Text(r) if !r.is_empty() => span_pos(r),
                    Cell::Text(r) => Sourcepos {
                        start: lc(r.start),
                        end: lc(r.start),
                    },
                    Cell::Pad => {
                        padded.push(cell_node);
                        let at = span_pos(&row.span).end;
                        Sourcepos { start: at, end: at }
                    }
                };
                cell_node.data_mut().sourcepos = pos;
            }
        }
        let last = t.found.body.last().unwrap_or(&t.found.header);
        table.data_mut().sourcepos = Sourcepos {
            start: lc(t.found.header.span.start),
            end: span_pos(&last.span).end.max(p_end),
        };
        footnotes |= table
            .descendants()
            .any(|d| matches!(d.data().value, NodeValue::FootnoteReference(_)));
        table.detach();
        if let Some(Keep::Term(list)) = t.keep {
            list.insert_before(table);
        } else if t.keep.is_some() {
            p.insert_before(table);
        } else if !t.found.before.is_empty() {
            p.insert_after(table);
        } else {
            p.insert_before(table);
            if let Some(item) = p.parent().filter(|i| {
                matches!(i.data().value, NodeValue::Item(_) | NodeValue::TaskItem(_))
                    && !i.first_child().is_some_and(|f| f.same_node(p))
            }) && let Some(list) = item.parent()
            {
                converted.insert(NodeKey::of(table));
                lists.push(list);
            }
            p.detach();
        }
        if let Some(item) = t.untask {
            let list = item.parent().and_then(|l| match &l.data().value {
                NodeValue::List(nl) => Some(*nl),
                _ => None,
            });
            if let Some(nl) = list {
                item.data_mut().value = NodeValue::Item(nl);
            }
        }
    }
    for list in lists {
        tighten(doc, list, &converted);
    }
    if footnotes {
        renumber_footnotes(doc);
    }
    padded
}

/// Escaped pipes in `table`'s cells where comrak's unescaping before inline parsing differs
/// from goldmark, which leaves them to the inline parser: in code spans
/// ([`unescape_code_pipes`]) and in autolinks, which keep the backslash.
fn escaped_pipes(doc: &Doc<'_>, table: Node<'_>) {
    for d in table.descendants() {
        let autolink = match &d.data().value {
            // comrak's inline positions after an unescaped pipe are off by its backslash, so
            // the autolink ends at the first `>` (an autolink has none inside).
            NodeValue::Link(l) if !l.url.starts_with("mailto:") => doc
                .src
                .text
                .get(doc.start(d)..)
                .and_then(|s| s.strip_prefix('<'))
                .and_then(|s| s.split_once('>'))
                .map(|(inner, _)| inner)
                .filter(|inner| inner.contains("\\|") && !inner.contains([' ', '<', '\n']))
                .map(str::to_owned),
            _ => None,
        };
        if let Some(inner) = autolink {
            if let NodeValue::Link(l) = &mut d.data_mut().value {
                l.url = inner.clone();
            }
            if let Some(t) = d.first_child()
                && let NodeValue::Text(text) = &mut t.data_mut().value
            {
                *text = inner.into();
            }
            continue;
        }
        if let NodeValue::Code(c) = &mut d.data_mut().value
            && let Some(literal) = unescape_code_pipes(&c.literal)
        {
            c.literal = literal;
        }
    }
}

/// goldmark removes the backslash before every escaped pipe in a cell's code spans
/// (`tableASTTransformer`); comrak removes it before inline parsing, but not before a pipe
/// after an even run of backslashes (`unescape_pipes`), so `code` loses that one.
fn unescape_code_pipes(code: &str) -> Option<String> {
    let b = code.as_bytes();
    let mut out = String::new();
    let mut last = 0;
    for (i, &c) in b.iter().enumerate() {
        if c != b'|' {
            continue;
        }
        let run = b[..i].iter().rev().take_while(|&&x| x == b'\\').count();
        if run > 0 && run % 2 == 0 {
            out.push_str(&code[last..i - 1]);
            last = i;
        }
    }
    (last > 0).then(|| {
        out.push_str(&code[last..]);
        out
    })
}

/// Whether the line before `n` is blank (inside its blockquotes): goldmark's
/// `HasBlankPreviousLines`.
fn blank_before(doc: &Doc<'_>, n: Node<'_>) -> bool {
    let line = n.data().sourcepos.start.line;
    if line < 2 {
        return false;
    }
    let quotes = n
        .ancestors()
        .skip(1)
        .filter(|a| matches!(a.data().value, NodeValue::BlockQuote))
        .count();
    let mut l = doc.line(line - 1);
    for _ in 0..quotes {
        let t = l.trim_start_matches(' ');
        if l.len() - t.len() > 3 {
            break;
        }
        let Some(rest) = t.strip_prefix('>') else {
            break;
        };
        l = rest.strip_prefix([' ', '\t']).unwrap_or(rest);
    }
    l.trim().is_empty()
}

/// goldmark's list tightness (`listParser.Close`) for a list comrak made loose, where the
/// tables that replaced a later paragraph of an item have no blank line before them.
fn tighten(doc: &Doc<'_>, list: Node<'_>, converted: &HashSet<NodeKey>) {
    if !matches!(&list.data().value, NodeValue::List(l) if !l.tight) {
        return;
    }
    let mut tight = true;
    for (i, item) in list.children().enumerate() {
        if item
            .children()
            .skip(1)
            .any(|c| !converted.contains(&NodeKey::of(c)) && blank_before(doc, c))
            || (i > 0 && blank_before(doc, item))
        {
            tight = false;
            break;
        }
    }
    if tight && let NodeValue::List(l) = &mut list.data_mut().value {
        l.tight = true;
    }
}

/// Footnote numbers in document order after cells parsed elsewhere joined the page.
fn renumber_footnotes(doc: &Doc<'_>) {
    let mut ix: Vec<(String, u32)> = Vec::new();
    let mut refs: Vec<(String, u32)> = Vec::new();
    for d in doc.root.descendants() {
        if let NodeValue::FootnoteReference(f) = &mut d.data_mut().value {
            let n = match ix.iter().find(|(name, _)| *name == f.name) {
                Some((_, n)) => *n,
                None => {
                    let n = u32::try_from(ix.len() + 1).unwrap_or(u32::MAX);
                    ix.push((f.name.clone(), n));
                    n
                }
            };
            let count = match refs.iter_mut().find(|(name, _)| *name == f.name) {
                Some((_, c)) => {
                    *c += 1;
                    *c
                }
                None => {
                    refs.push((f.name.clone(), 1));
                    1
                }
            };
            f.ix = n;
            f.ref_num = count;
        }
    }
    for d in doc.root.descendants() {
        if let NodeValue::FootnoteDefinition(f) = &mut d.data_mut().value
            && let Some((_, c)) = refs.iter().find(|(name, _)| *name == f.name)
        {
            f.total_references = *c;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delim(s: &str) -> Option<Vec<TableAlignment>> {
        parse_delimiter(s.as_bytes())
    }

    #[test]
    fn delimiters() {
        use TableAlignment::{Center, Left, None as N, Right};
        assert_eq!(delim(":--|:--|:--"), Some(vec![Left, Left, Left]));
        assert_eq!(delim("| --- | :-: | --: |"), Some(vec![N, Center, Right]));
        assert_eq!(delim("|-"), Some(vec![N]));
        assert_eq!(delim("    | --- |"), None);
        assert_eq!(delim("| |"), None);
        assert_eq!(delim("|"), None);
        assert_eq!(delim("| : -- |"), None);
        assert_eq!(delim("| -- | x |"), None);
    }

    fn row(s: &str, n: usize, header: bool) -> Vec<String> {
        let a = vec![TableAlignment::None; n];
        parse_row(s.as_bytes(), 0..s.len(), &a, header)
            .cells
            .iter()
            .map(|c| match c {
                Cell::Text(r) => s[r.clone()].to_owned(),
                Cell::Pad => "<pad>".to_owned(),
            })
            .collect()
    }

    #[test]
    fn rows() {
        assert_eq!(row("a|b", 3, true), ["a", "b", "<pad>"]);
        assert_eq!(row("| a | b | c |", 2, false), ["a", "b"]);
        assert_eq!(row("| a | b | c |", 2, true), ["a", "b", "c"]);
        assert_eq!(row(r"| a \| b | c", 2, false), [r"a \| b", "c"]);
        assert_eq!(row("a||", 2, false), ["a", "<pad>"]);
        assert_eq!(row("|", 2, false), ["<pad>", "<pad>"]);
        assert_eq!(row("plain", 2, false), ["plain", "<pad>"]);
        assert_eq!(row("| |", 1, false), [""]);
    }

    #[test]
    fn code_pipes() {
        assert_eq!(unescape_code_pipes(r"a\\|b").as_deref(), Some(r"a\|b"));
        assert_eq!(unescape_code_pipes(r"a\\\\|b").as_deref(), Some(r"a\\\|b"));
        assert_eq!(unescape_code_pipes(r"a\|b"), None);
        assert_eq!(unescape_code_pipes("a|b"), None);
    }

    #[test]
    fn transforms() {
        let t = "intro\n  Field | Value\n  | --- | --- |\n  | a | b |";
        let mut lines = Vec::new();
        let mut at = 0;
        for l in t.split('\n') {
            lines.push(at..at + l.len());
            at += l.len() + 1;
        }
        let f = transform(t.as_bytes(), &lines).expect("a table");
        assert_eq!(f.before.len(), 1);
        assert_eq!(f.body.len(), 1);
        // A header with more cells than the delimiter: no table, and no later one either.
        let t = "a|b|c\n-|-\nd|e\n-|-";
        let mut lines = Vec::new();
        let mut at = 0;
        for l in t.split('\n') {
            lines.push(at..at + l.len());
            at += l.len() + 1;
        }
        assert!(transform(t.as_bytes(), &lines).is_none());
    }
}
