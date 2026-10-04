//! Planning the tables of a paragraph: which lines are rows, and where its inlines are cut.

use super::*;

/// A synthetic-text range that came from `orig` (an offset of the parsed text).
pub(super) struct Piece {
    pub(super) synth: Range<usize>,
    pub(super) orig: usize,
}

/// One table's part of the synthetic document.
pub(super) struct Planned<'a> {
    pub(super) paragraph: Node<'a>,
    pub(super) found: Found,
    /// What the lines before the header become when a block needed them ([`retry`]).
    pub(super) keep: Option<Keep<'a>>,
    /// Whether the paragraph's own inlines can stay for the lines before the header (none of
    /// them reaches into the header row), so that only the table is parsed again.
    pub(super) split: bool,
    /// The task item whose checkbox goldmark reads as text of the header row
    /// ([`task_marker`]).
    pub(super) untask: Option<Node<'a>>,
}

/// The task item of paragraph `p` and the offset of its `[ ]` marker, when `p` is the item's
/// first paragraph. goldmark's checkbox is an inline of the item's first paragraph
/// (`extension/tasklist.go`), so the marker is paragraph text to the table transformer; comrak
/// takes it off the paragraph at parse time.
pub(super) fn task_marker<'a>(doc: &Doc<'_>, p: Node<'a>) -> Option<(Node<'a>, usize)> {
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
pub(super) fn divides_at(p: Node<'_>, line: usize) -> bool {
    p.children().all(|c| {
        let sp = c.data().sourcepos;
        sp.start.line > line || sp.end.line <= line
    })
}

/// Paragraph `p` cut to its inlines up to the end of line `line`: goldmark's paragraph of the
/// lines before a table's header, which ends without the line's break (a backslash there is
/// a literal backslash).
pub(super) fn cut_inlines<'a>(doc: &Doc<'a>, p: Node<'a>, line: usize) {
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
pub(super) fn marker_row(text: &[u8], row: &Row) -> bool {
    let t = &text[row.span.clone()];
    t == CONTEXT_OPEN.as_bytes() || t == CONTEXT_CLOSE.as_bytes()
}

/// `text` with the `-` of every line that comrak could read as a delimiter row (after
/// blockquote markers and indentation) replaced, so that the page appended to the synthetic
/// document forms no table: comrak's table would take in a link reference definition on the
/// line before its header, which goldmark resolves.
pub(super) fn without_delimiter_rows(text: &str) -> String {
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
pub(super) fn plan<'a>(doc: &Doc<'a>, keep: &HashMap<NodeKey, Keep<'a>>) -> Vec<Planned<'a>> {
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
