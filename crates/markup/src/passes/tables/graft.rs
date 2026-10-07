//! Synthesizing a table's nodes and grafting them in place of the paragraph's lines.

use super::*;

/// The synthetic document of the planned tables: per table, the lines before its header when
/// the paragraph's inlines do not divide there ([`Planned::split`]), then the table rebuilt
/// as a canonical GFM table.
///
/// The lines before a header follow a dummy paragraph line, indented by four columns: they
/// are continuation lines, as in the page, and no line can start a block there (a setext
/// underline, a definition's `:`, a delimiter row or a list marker that a container's
/// indentation or laziness kept in the paragraph), while comrak drops the indentation from the
/// paragraph's text.
pub(super) struct Synth {
    pub(super) text: String,
    pub(super) pieces: Vec<Piece>,
    /// Per planned table, the synthetic lines (1-based) of its dummy paragraph and of its
    /// table.
    pub(super) lines: Vec<(Option<usize>, usize)>,
    /// The length of the tables part (the page follows).
    pub(super) len: usize,
}

/// The dummy first line of the paragraph holding the lines before a header (its text is
/// dropped).
pub(super) const DUMMY: &str = "X\n";

pub(super) fn synthesize(doc: &Doc<'_>, planned: &[Planned<'_>]) -> Synth {
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
    // The page follows for its link reference and footnote definitions, when it can have any:
    // a definition's label is followed by `]:`.
    if doc.src.text.contains("]:") {
        synth.push_str(&without_delimiter_rows(&doc.src.text));
    }
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
pub(super) fn before_inlines(p: Node<'_>, n: usize) -> Option<Vec<Node<'_>>> {
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
pub(super) fn graft<'a>(
    doc: &Doc<'a>,
    copts: &Options<'_>,
    planned: &[Planned<'a>],
) -> Vec<Node<'a>> {
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
