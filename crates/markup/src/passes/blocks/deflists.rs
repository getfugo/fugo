//! Definition lists: a term and its `: definition` lines become a list.

use super::*;

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
pub(super) fn is_link_definition(line: &str) -> bool {
    let l = line.trim_start();
    l.starts_with('[')
        && l.find("]:")
            .is_some_and(|i| i > 1 && !l[1..i].contains(']'))
}

/// The first line holding content of `n` (inline positions are exact).
pub(super) fn content_line(n: Node<'_>) -> usize {
    n.descendants()
        .find(|d| !d.data().value.block())
        .map_or(n.data().sourcepos.start.line, |d| {
            d.data().sourcepos.start.line
        })
}

pub(super) fn split_list<'a>(doc: &Doc<'a>, list: Node<'a>) {
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
pub(super) fn split_term<'a>(doc: &Doc<'a>, term: Node<'a>) {
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
