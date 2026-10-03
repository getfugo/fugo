//! goldmark's retry of a line whose block parser needs the paragraph before it, when that
//! paragraph is a table (`parser.go` `openBlocks`, `RequireParagraph`): a setext underline
//! (`setextHeadingParser`) and the `:` of a list's first definition
//! (`extension/definition_list.go` `definitionListParser`).
//!
//! goldmark closes the paragraph and runs the paragraph transformers before the new block
//! takes it over. When the table transformer leaves lines before the header, the paragraph
//! survives and the block goes on with them, after the table (`setextHeadingParser.Close` and
//! the definition list take the paragraph's remaining lines). When it takes the whole
//! paragraph, the parser fails and goldmark parses the line again as the start of a block
//! that cannot continue a paragraph (`goto retry`): `---` is a thematic break, `-` an empty
//! list item, `===` and `: def` start paragraphs, and the lines after them go on from there.
//! A paragraph a blank line closed was transformed then, so a `:` after it finds the table and
//! starts a paragraph too.
//!
//! comrak made a heading or a definition list of the whole paragraph. Here the paragraph is
//! given back to [`super::plan`]; the block keeps the lines before the header ([`Keep`]), or
//! the rest of the container is parsed again from the retried line ([`reparse_after`]).

use std::collections::HashMap;

use comrak::nodes::{LineColumn, NodeValue, Sourcepos};
use comrak::{Options, parse_document};

use super::{
    content_start, may_have_delimiter, paragraph_lines, prefixes, range_pos, renumber_footnotes,
    transform, trim,
};
use crate::doc::{Doc, Node, NodeKey};
use crate::passes::blocks;

/// What the lines before a table's header become when a block needed them as its paragraph.
#[derive(Clone, Copy)]
pub(super) enum Keep<'a> {
    /// A setext heading of this level ending at its underline, after the table.
    Heading(u8, LineColumn),
    /// The term of this definition list, which goes after the table.
    Term(Node<'a>),
}

/// The blocks goldmark retries.
#[derive(Clone, Copy)]
enum Kind {
    Setext(u8),
    Definitions,
}

/// The setext headings and definition lists in `root` (itself included), in document order.
fn candidates(root: Node<'_>) -> Vec<Node<'_>> {
    root.descendants()
        .filter(|n| {
            matches!(&n.data().value,
                NodeValue::Heading(h) if h.setext)
                || matches!(n.data().value, NodeValue::DescriptionList)
        })
        .collect()
}

/// Gives the paragraphs that comrak's setext headings and definition lists took back to the
/// table pass where goldmark makes a table of them first; returns what the blocks keep.
pub(super) fn require_paragraph<'a>(
    doc: &Doc<'a>,
    copts: &Options<'_>,
) -> HashMap<NodeKey, Keep<'a>> {
    let mut keep = HashMap::new();
    let mut work = candidates(doc.root);
    work.reverse();
    let mut reparsed = Vec::new();
    while let Some(n) = work.pop() {
        // A block of a region parsed again is gone (its copy is in the new nodes).
        if !n.ancestors().last().is_some_and(|r| r.same_node(doc.root)) {
            continue;
        }
        let kind = match &n.data().value {
            NodeValue::Heading(h) if h.setext => Kind::Setext(h.level),
            NodeValue::DescriptionList => Kind::Definitions,
            _ => continue,
        };
        let retried = match kind {
            Kind::Setext(level) => setext(doc, n, level, &mut keep),
            Kind::Definitions => definitions(doc, n, &mut keep),
        };
        let Some((anchor, line)) = retried else {
            continue;
        };
        let inserted = reparse_after(doc, copts, anchor, line);
        let mut more: Vec<_> = inserted.iter().flat_map(|n| candidates(n)).collect();
        more.reverse();
        work.extend(more);
        reparsed.extend(inserted);
    }
    for n in &reparsed {
        blocks::fix_link_ref_lines(doc, n);
    }
    if reparsed.iter().any(|n| {
        n.descendants()
            .any(|d| matches!(d.data().value, NodeValue::FootnoteReference(_)))
    }) {
        renumber_footnotes(doc);
    }
    keep
}

/// Setext heading `h`: its text lines, when goldmark makes a table of them, become a
/// paragraph again. Returns the anchor and line to parse again from.
fn setext<'a>(
    doc: &Doc<'a>,
    h: Node<'a>,
    level: u8,
    keep: &mut HashMap<NodeKey, Keep<'a>>,
) -> Option<(Node<'a>, usize)> {
    let text = doc.src.text.as_bytes();
    let sp = h.data().sourcepos;
    let under = sp.end.line;
    if under < sp.start.line + 2 {
        return None;
    }
    let lines = paragraph_lines(doc, h, under - 1);
    if !may_have_delimiter(text, &lines) {
        return None;
    }
    let found = transform(text, &lines)?;
    {
        let mut d = h.data_mut();
        d.value = NodeValue::Paragraph;
        d.sourcepos.end = range_pos(doc, &trim(text, lines.last()?.clone())).end;
    }
    if found.before.is_empty() {
        return Some((h, under));
    }
    keep.insert(NodeKey::of(h), Keep::Heading(level, sp.end));
    None
}

/// Definition list `dl`: the term of its first item (goldmark's first definition needs the
/// paragraph; the later ones take theirs as they are). Returns the anchor and line to parse
/// again from.
fn definitions<'a>(
    doc: &Doc<'a>,
    dl: Node<'a>,
    keep: &mut HashMap<NodeKey, Keep<'a>>,
) -> Option<(Node<'a>, usize)> {
    if dl
        .previous_sibling()
        .is_some_and(|s| matches!(s.data().value, NodeValue::DescriptionList))
    {
        return None;
    }
    let text = doc.src.text.as_bytes();
    let item = dl.first_child()?;
    let term = item
        .first_child()
        .filter(|t| matches!(t.data().value, NodeValue::DescriptionTerm))?;
    let p = term
        .first_child()
        .filter(|p| matches!(p.data().value, NodeValue::Paragraph))?;
    let details = item
        .children()
        .find(|d| matches!(d.data().value, NodeValue::DescriptionDetails))?;
    let colon = details.data().sourcepos.start.line;
    let start = p.data().sourcepos.start.line;
    let last = (start..colon).rev().find(|&l| !doc.blank_line(l))?;
    let lines = paragraph_lines(doc, p, last);
    if !may_have_delimiter(text, &lines) {
        return None;
    }
    let found = transform(text, &lines)?;
    p.data_mut().sourcepos.end = range_pos(doc, &trim(text, lines.last()?.clone())).end;
    // A blank line closed (and transformed) the paragraph before the `:` was read.
    if found.before.is_empty() || last + 1 < colon {
        p.detach();
        dl.insert_before(p);
        return Some((p, colon));
    }
    keep.insert(NodeKey::of(p), Keep::Term(dl));
    None
}

/// Where a line of the region parsed again came from.
struct RegionLine {
    /// Offset (in the synthetic line) of the copied content after the leading whitespace.
    content: usize,
    /// Parsed-text offsets: the content start after the containers' prefixes, the content
    /// after its leading whitespace, and the line end.
    start: usize,
    rest: usize,
    end: usize,
}

/// Parses lines `from..` of the container of `anchor` again (to the container's last line)
/// and puts the nodes in place of `anchor`'s later siblings. The lines go into a blockquote
/// of a synthetic document (so that an unclosed block ends with them), with their leading
/// tabs expanded at their columns, followed by the page for its link reference definitions.
/// Returns the new nodes.
fn reparse_after<'a>(
    doc: &Doc<'a>,
    copts: &Options<'_>,
    anchor: Node<'a>,
    from: usize,
) -> Vec<Node<'a>> {
    let Some(parent) = anchor.parent() else {
        return Vec::new();
    };
    let footnote = |n: Node<'_>| matches!(n.data().value, NodeValue::FootnoteDefinition(_));
    let later: Vec<_> = anchor
        .following_siblings()
        .skip(1)
        .filter(|n| !footnote(n))
        .collect();
    let mut to = if matches!(parent.data().value, NodeValue::Document) {
        doc.src.lines.count()
    } else {
        parent.data().sourcepos.end.line
    };
    to = later
        .iter()
        .map(|n| n.data().sourcepos.end.line)
        .fold(to, usize::max);
    if from > to {
        return Vec::new();
    }
    let text = doc.src.text.as_bytes();
    let prefixes = prefixes(anchor);
    let mut synth = String::new();
    let mut region = Vec::with_capacity(to + 1 - from);
    for line in from..=to {
        let at = doc.src.offset(LineColumn { line, column: 1 });
        let end = at + doc.line(line).trim_end_matches('\r').len();
        let start = content_start(text, at, &prefixes).min(end);
        let synth_at = synth.len();
        synth.push_str("> ");
        let mut column = text[at..start]
            .iter()
            .fold(0, |c, &b| if b == b'\t' { c + 4 - c % 4 } else { c + 1 });
        let mut rest = start;
        while rest < end && matches!(text[rest], b' ' | b'\t') {
            let width = if text[rest] == b'\t' {
                4 - column % 4
            } else {
                1
            };
            synth.extend(std::iter::repeat_n(' ', width));
            column += width;
            rest += 1;
        }
        region.push(RegionLine {
            content: synth.len() - synth_at,
            start,
            rest,
            end,
        });
        synth.push_str(&doc.src.text[rest..end]);
        synth.push('\n');
    }
    synth.push('\n');
    synth.push_str(&doc.src.text);
    let root = parse_document(doc.arena, &synth, copts);
    let Some(quote) = root.first_child().filter(|q| {
        matches!(q.data().value, NodeValue::BlockQuote) && q.data().sourcepos.start.line == 1
    }) else {
        return Vec::new();
    };
    let map = |lc: LineColumn| -> LineColumn {
        let Some(r) = lc.line.checked_sub(1).and_then(|i| region.get(i)) else {
            return lc;
        };
        let c = lc.column.saturating_sub(1);
        let at = if c >= r.content {
            (r.rest + c - r.content).min(r.end)
        } else if c >= 2 {
            (r.start + c - 2).min(r.rest)
        } else {
            r.start
        };
        let (line, column) = doc.src.lines.line_col(at);
        LineColumn { line, column }
    };
    // The page keeps its footnote definitions (comrak moved them out of their places).
    let nodes: Vec<_> = quote.children().filter(|n| !footnote(n)).collect();
    for n in &nodes {
        for d in n.descendants().filter(|d| footnote(d)).collect::<Vec<_>>() {
            d.detach();
        }
        for d in n.descendants() {
            let mut data = d.data_mut();
            data.sourcepos.start = map(data.sourcepos.start);
            data.sourcepos.end = map(data.sourcepos.end);
        }
    }
    for n in later {
        n.detach();
    }
    let mut after = anchor;
    for n in &nodes {
        n.detach();
        after.insert_after(n);
        after = n;
    }
    if !matches!(parent.data().value, NodeValue::Document) {
        lazy_lines(doc, parent, nodes.last().copied(), to);
    }
    nodes
}

/// A paragraph that ends the region parsed again in `container` (at line `to`) goes on with
/// the lazy continuation lines after the container: comrak's paragraph there, when it is the
/// next block.
fn lazy_lines<'a>(doc: &Doc<'a>, container: Node<'a>, last: Option<Node<'a>>, to: usize) {
    let mut p = last;
    while let Some(n) = p {
        match n.data().value {
            NodeValue::BlockQuote
            | NodeValue::List(_)
            | NodeValue::Item(_)
            | NodeValue::TaskItem(_)
            | NodeValue::DescriptionList
            | NodeValue::DescriptionItem(_)
            | NodeValue::DescriptionDetails => p = n.last_child(),
            _ => break,
        }
    }
    let Some(p) = p.filter(|p| {
        matches!(p.data().value, NodeValue::Paragraph) && p.data().sourcepos.end.line == to
    }) else {
        return;
    };
    let next = container
        .ancestors()
        .take_while(|a| !a.same_node(doc.root))
        .find_map(comrak::arena_tree::Node::next_sibling);
    let Some(q) = next.filter(|q| {
        matches!(q.data().value, NodeValue::Paragraph) && q.data().sourcepos.start.line == to + 1
    }) else {
        return;
    };
    let end = p.data().sourcepos.end;
    let after = LineColumn {
        line: end.line,
        column: end.column + 1,
    };
    p.append(doc.node(
        NodeValue::SoftBreak,
        Sourcepos {
            start: after,
            end: after,
        },
    ));
    for c in q.children().collect::<Vec<_>>() {
        c.detach();
        p.append(c);
    }
    p.data_mut().sourcepos.end = q.data().sourcepos.end;
    q.detach();
}
