//! Structural passes: sourcepos repair, dropped comments, block and heading attributes,
//! goldmark's definition lists, block images, autolinks and passthrough.

use comrak::nodes::{NodeValue, Sourcepos};
use ssg_base::Value;

use super::{Piece, fill_text, split_text, texts};
use crate::attributes::{self, Attr, Owner};
use crate::doc::{Doc, Node, Role};
use crate::{MarkupError, PassthroughKind};

/// comrak reports the inlines of a paragraph (or setext heading) that began with link
/// reference definitions on lines relative to the paragraph start; move them down by the
/// removed lines (in `root`).
pub(crate) fn fix_link_ref_lines(doc: &Doc<'_>, root: Node<'_>) {
    for p in root.descendants() {
        let mut sp = p.data().sourcepos;
        match &p.data().value {
            NodeValue::Paragraph => {}
            // The text ends on the line before the underline.
            NodeValue::Heading(h) if h.setext => sp.end.line -= 1,
            _ => continue,
        }
        // The paragraph starts after its containers' markers (`> [a]: /x`, `- [a]: /x`).
        if !doc
            .src
            .text
            .get(doc.start(p)..)
            .is_some_and(|t| t.starts_with('['))
        {
            continue;
        }
        let Some(last) = p
            .descendants()
            .skip(1)
            .map(|d| d.data().sourcepos.end.line)
            .max()
        else {
            continue;
        };
        let shift = sp.end.line.saturating_sub(last);
        if shift == 0 {
            continue;
        }
        for d in p.descendants().skip(1) {
            let mut data = d.data_mut();
            data.sourcepos.start.line += shift;
            data.sourcepos.end.line += shift;
        }
    }
}

/// The Go implementation writes nothing for an HTML comment when raw HTML is omitted (its
/// context-marker renderer's `renderHTMLBlock`, `renderRawHTML`); the node stays a sibling (a
/// text block before it ends with a newline, a list item starting with it gets one after
/// `<li>`), so it becomes an empty `Raw` node.
pub(crate) fn drop_comments(doc: &Doc<'_>) {
    for n in doc.root.descendants() {
        let mut d = n.data_mut();
        let comment = match &d.value {
            NodeValue::HtmlBlock(b) => b.literal.trim_start().starts_with("<!--"),
            NodeValue::HtmlInline(h) => h.starts_with("<!--"),
            _ => false,
        };
        if comment {
            d.value = NodeValue::Raw(String::new());
        }
    }
}

/// goldmark replaces a paragraph made only of link reference definitions by an empty text
/// block (`linkReferenceParagraphTransformer`), which still counts as a sibling: a text block
/// before it ends with a newline, a list item starting with it gets no newline after `<li>`,
/// and it is not the first paragraph of a definition. comrak drops such paragraphs, so an
/// empty text-block paragraph goes where their lines are: the non-blank lines of a container
/// that no child covers (a container's first line holds its marker and is left out).
///
/// Footnote definitions leave nothing behind in either (goldmark's footnote transformer
/// removes them; comrak moves the referenced ones to the end and drops the others), so their
/// lines count as covered (`footnotes`: the extension is on).
pub(crate) fn link_reference_blocks(doc: &mut Doc<'_>, footnotes: bool) {
    let containers: Vec<_> = doc
        .root
        .descendants()
        .filter(|n| {
            matches!(
                n.data().value,
                NodeValue::Document
                    | NodeValue::BlockQuote
                    | NodeValue::Item(_)
                    | NodeValue::TaskItem(_)
                    | NodeValue::DescriptionDetails
                    | NodeValue::FootnoteDefinition(_)
            )
        })
        .collect();
    let definitions: Vec<_> = containers
        .iter()
        .filter(|n| matches!(n.data().value, NodeValue::FootnoteDefinition(_)))
        .map(|n| {
            let sp = n.data().sourcepos;
            sp.start.line..=sp.end.line
        })
        .collect();
    let mut created = Vec::new();
    for c in containers {
        let sp = c.data().sourcepos;
        let (first, last) = if matches!(c.data().value, NodeValue::Document) {
            (1, doc.src.lines.count())
        } else {
            (sp.start.line + 1, sp.end.line)
        };
        if last < first {
            continue;
        }
        let quotes = c
            .ancestors()
            .filter(|a| matches!(a.data().value, NodeValue::BlockQuote))
            .count();
        let content = |line: usize| {
            let mut l = doc.line(line);
            for _ in 0..quotes {
                let t = l.trim_start_matches(' ');
                match t.strip_prefix('>') {
                    Some(rest) if l.len() - t.len() <= 3 => l = rest,
                    _ => break,
                }
            }
            l
        };
        let blank = |line: usize| content(line).trim().is_empty();
        let children: Vec<_> = c.children().collect();
        // Children are not always in line order (a moved footnote definition, a table put
        // before the setext heading it came from).
        let mut covered = vec![false; last + 1 - first];
        let lines = children
            .iter()
            .map(|n| {
                let sp = n.data().sourcepos;
                sp.start.line..=sp.end.line
            })
            .chain(definitions.iter().cloned());
        for r in lines {
            for line in r {
                if let Some(c) = line.checked_sub(first).and_then(|i| covered.get_mut(i)) {
                    *c = true;
                }
            }
        }
        let open = |line: usize| !covered[line - first] && !blank(line);
        let mut line = first;
        while line <= last {
            if !open(line) {
                line += 1;
                continue;
            }
            let run = line;
            while line <= last && open(line) {
                line += 1;
            }
            // An unreferenced footnote definition (and its lazy lines) comrak dropped.
            if footnotes && is_footnote_definition(content(run)) {
                continue;
            }
            let p = doc.node(NodeValue::Paragraph, Sourcepos::from((run, 1, line - 1, 1)));
            match children
                .iter()
                .find(|n| n.data().sourcepos.start.line >= line)
            {
                Some(n) => n.insert_before(p),
                None => c.append(p),
            }
            created.push(p);
        }
    }
    for p in created {
        doc.set_role(p, Role::TextBlock);
    }
}

/// Whether `line` starts a footnote definition (comrak's `footnote_definition` scanner:
/// `[^label]:` with no whitespace in the label).
fn is_footnote_definition(line: &str) -> bool {
    let t = line.trim_start_matches([' ', '\t']);
    let Some(rest) = t.strip_prefix("[^") else {
        return false;
    };
    let label = rest
        .bytes()
        .take_while(|b| !matches!(b, b']' | b' ' | b'\t' | b'\r' | b'\n' | 0))
        .count();
    label > 0 && rest[label..].starts_with("]:")
}

/// A block-attribute line, blanked before parsing.
pub(crate) struct BlockAttrLine {
    /// 1-based line.
    pub line: usize,
    /// `>` markers before the `{`.
    pub quote_depth: usize,
    /// Columns of indentation after the markers.
    pub indent: usize,
    pub attrs: Vec<Attr>,
}

/// Applies each `{…}` line to the block it follows (goldmark: to the previous sibling of the
/// attribute block, unless a blank line separates them or it is a fenced code block).
pub(crate) fn apply_block_attributes(
    doc: &mut Doc<'_>,
    lines: &[BlockAttrLine],
) -> Result<(), MarkupError> {
    for a in lines {
        let prev = doc
            .line(a.line - 1)
            .trim_start_matches([' ', '\t', '>'])
            .trim();
        if a.line < 2 || prev.is_empty() {
            continue;
        }
        let target = doc.root.descendants().skip(1).find(|n| {
            let d = n.data();
            let sp = d.sourcepos;
            if !d.value.block()
                || sp.start.line >= a.line
                || !(a.line - 1..=a.line).contains(&sp.end.line)
            {
                return false;
            }
            let quotes = n
                .ancestors()
                .skip(1)
                .filter(|p| matches!(p.data().value, NodeValue::BlockQuote))
                .count();
            let in_item = n
                .ancestors()
                .skip(1)
                .any(|p| matches!(p.data().value, NodeValue::Item(_) | NodeValue::TaskItem(_)));
            quotes == a.quote_depth && (a.indent >= 2 || !in_item)
        });
        let Some(target) = target else { continue };
        if let NodeValue::CodeBlock(c) = &target.data().value
            && c.fenced
        {
            continue;
        }
        let (plain, _) = attributes::convert(&a.attrs, Owner::General)
            .map_err(|e| doc_error(doc, target, &e.to_string()))?;
        let extra = doc.extra_mut(target);
        for (k, v) in plain {
            if !extra.attrs.iter().any(|(name, _)| *name == k) {
                extra.attrs.push((k, v));
            }
        }
    }
    Ok(())
}

pub(crate) fn doc_error(doc: &Doc<'_>, n: Node<'_>, message: &str) -> MarkupError {
    MarkupError::Attributes {
        position: doc.position(n),
        message: message.to_owned(),
    }
}

/// `## Heading {#id .class}`: strips a trailing attribute block from each heading.
pub(crate) fn heading_attributes(doc: &mut Doc<'_>) -> Result<(), MarkupError> {
    let headings: Vec<_> = doc
        .root
        .descendants()
        .filter(|n| matches!(n.data().value, NodeValue::Heading(_)))
        .collect();
    for h in headings {
        let Some(last) = h.last_child() else { continue };
        let Some(text) = crate::doc::text_of(last) else {
            continue;
        };
        let trimmed = text.trim_end();
        if !trimmed.ends_with('}') {
            continue;
        }
        let Some(open) = trimmed.rfind('{') else {
            continue;
        };
        let Some(attrs) = attributes::parse_all(&trimmed[open..]) else {
            continue;
        };
        let keep = trimmed[..open].trim_end().to_owned();
        if keep.is_empty() {
            last.detach();
        } else {
            last.data_mut().value = NodeValue::Text(keep.into());
        }
        let (plain, _) = attributes::convert(&attrs, Owner::General)
            .map_err(|e| doc_error(doc, h, &e.to_string()))?;
        let mut rest = Vec::new();
        let mut id = None;
        for (k, v) in plain {
            if k == "id" {
                match v {
                    Value::String(s) => id = Some(s.to_string()),
                    _ => return Err(doc_error(doc, h, "a heading id must be a string")),
                }
            } else {
                rest.push((k, v));
            }
        }
        let extra = doc.extra_mut(h);
        extra.attrs = rest;
        extra.id = id;
    }
    Ok(())
}

/// Definition lists with goldmark's semantics: one term per line, per-definition tightness
/// from the blank line before its `:`, only the first paragraph of a tight definition
/// unwrapped (goldmark's `definitionDescriptionParser.Close` stops after replacing the first
/// paragraph child, whatever blocks come before it), and a list ends where a non-blank line
/// without nodes (a link reference definition) separates two items.
pub(crate) fn definition_lists(doc: &mut Doc<'_>) {
    let lists: Vec<_> = doc
        .root
        .descendants()
        .filter(|n| matches!(n.data().value, NodeValue::DescriptionList))
        .collect();
    for list in lists {
        split_list(doc, list);
    }
    let terms: Vec<_> = doc
        .root
        .descendants()
        .filter(|n| matches!(n.data().value, NodeValue::DescriptionTerm))
        .collect();
    for t in terms {
        split_term(doc, t);
    }
    let details: Vec<_> = doc
        .root
        .descendants()
        .filter(|n| matches!(n.data().value, NodeValue::DescriptionDetails))
        .collect();
    for d in details {
        let line = d.data().sourcepos.start.line;
        if line > 1 && doc.blank_line(line - 1) {
            continue;
        }
        doc.set_role(d, Role::TightDetails);
        if let Some(p) = d.children().find(|p| {
            matches!(p.data().value, NodeValue::Paragraph)
                && !matches!(doc.role(p), Some(Role::TextBlock))
        }) {
            doc.set_role(p, Role::TextBlock);
        }
    }
}

/// `[label]: destination` (the start of a link reference definition).
fn is_link_definition(line: &str) -> bool {
    let l = line.trim_start();
    l.starts_with('[')
        && l.find("]:")
            .is_some_and(|i| i > 1 && !l[1..i].contains(']'))
}

/// The first line holding content of `n` (inline positions are exact).
fn content_line(n: Node<'_>) -> usize {
    n.descendants()
        .find(|d| !d.data().value.block())
        .map_or(n.data().sourcepos.start.line, |d| {
            d.data().sourcepos.start.line
        })
}

fn split_list<'a>(doc: &Doc<'a>, list: Node<'a>) {
    let mut current = list;
    let mut prev_end: Option<usize> = None;
    let items: Vec<_> = list.children().collect();
    for item in items {
        let start = content_line(item);
        if let Some(end) = prev_end
            && (end + 1..start).any(|l| is_link_definition(doc.line(l)))
        {
            let sp = item.data().sourcepos;
            let next = doc.node(NodeValue::DescriptionList, sp);
            current.insert_after(next);
            current = next;
        }
        if !current.same_node(list) {
            item.detach();
            current.append(item);
        }
        prev_end = Some(item.data().sourcepos.end.line);
    }
}

/// A term paragraph of several lines is several terms.
fn split_term<'a>(doc: &Doc<'a>, term: Node<'a>) {
    let Some(p) = term.first_child() else { return };
    if !p
        .children()
        .any(|c| matches!(c.data().value, NodeValue::SoftBreak))
    {
        return;
    }
    let mut current_term = term;
    let mut current_para = p;
    let children: Vec<_> = p.children().collect();
    for c in children {
        if matches!(c.data().value, NodeValue::SoftBreak) {
            c.detach();
            let sp = Sourcepos::from((
                c.data().sourcepos.start.line + 1,
                1,
                c.data().sourcepos.start.line + 1,
                1,
            ));
            let t = doc.node(NodeValue::DescriptionTerm, sp);
            let para = doc.node(NodeValue::Paragraph, sp);
            t.append(para);
            current_term.insert_after(t);
            current_term = t;
            current_para = para;
            continue;
        }
        if !current_para.same_node(p) {
            c.detach();
            current_para.append(c);
        }
    }
}

/// Images alone in their paragraph replace it ([`crate::StandaloneImages::Block`]).
pub(crate) fn block_images(doc: &mut Doc<'_>) {
    let paras: Vec<_> = doc
        .root
        .descendants()
        .filter(|n| {
            matches!(n.data().value, NodeValue::Paragraph)
                && n.first_child().is_some_and(|c| {
                    matches!(c.data().value, NodeValue::Image(_)) && c.next_sibling().is_none()
                })
        })
        .collect();
    for p in paras {
        // The image takes the paragraph's place, and its block attributes.
        let attrs = std::mem::take(&mut doc.extra_mut(p).attrs);
        if let Some(img) = p.first_child() {
            let extra = doc.extra_mut(img);
            extra.role = Some(Role::BlockImage);
            extra.attrs = attrs;
        }
        doc.set_role(p, Role::TextBlock);
    }
}

/// `<https://…>` and `<me@example.org>` links are autolinks.
pub(crate) fn angle_autolinks(doc: &mut Doc<'_>) {
    let links: Vec<_> = doc
        .root
        .descendants()
        .filter(|n| matches!(n.data().value, NodeValue::Link(_)) && doc.role(n).is_none())
        .collect();
    for l in links {
        let at = doc.start(l);
        if doc.src.text.as_bytes().get(at) == Some(&b'<') {
            doc.set_role(l, Role::AutoLink { www: false });
        }
    }
}

/// A passthrough span replaced by a placeholder before parsing.
pub(crate) struct Passthrough {
    pub token: String,
    pub kind: PassthroughKind,
    pub inner: String,
    pub raw: String,
}

/// Swaps passthrough placeholders for passthrough nodes; a block placeholder alone in its
/// paragraph replaces the paragraph.
pub(crate) fn passthrough(doc: &mut Doc<'_>, spans: &[Passthrough]) {
    let nodes = texts(doc.root, |v| {
        matches!(v, NodeValue::CodeBlock(_) | NodeValue::HtmlBlock(_))
    });
    for n in nodes {
        let Some(text) = crate::doc::text_of(n) else {
            continue;
        };
        let mut found: Vec<(std::ops::Range<usize>, Piece)> = spans
            .iter()
            .filter_map(|s| {
                let at = text.find(&s.token)?;
                let role = Role::Passthrough {
                    kind: s.kind,
                    inner: s.inner.clone(),
                    raw: s.raw.clone(),
                };
                Some((
                    at..at + s.token.len(),
                    Piece::Node(NodeValue::Raw(s.raw.clone()), Some(role)),
                ))
            })
            .collect();
        if found.is_empty() {
            continue;
        }
        found.sort_by_key(|(r, _)| r.start);
        let parent = n.parent();
        let created = split_text(doc, n, fill_text(text.len(), found));
        // A block element alone in its paragraph takes the paragraph's place.
        if let [only] = created.as_slice()
            && let Some(p) = parent.filter(|p| {
                matches!(p.data().value, NodeValue::Paragraph) && p.children().count() == 1
            })
            && matches!(
                doc.role(only),
                Some(Role::Passthrough {
                    kind: PassthroughKind::Block,
                    ..
                })
            )
        {
            only.detach();
            p.insert_before(only);
            p.detach();
        }
    }
}
