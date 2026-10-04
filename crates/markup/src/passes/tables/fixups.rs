//! Fix-ups after grafting: escaped pipes, pipes in code spans, blank lines before tables, tight
//! lists and footnote numbers.

use super::*;

/// Escaped pipes in `table`'s cells where comrak's unescaping before inline parsing differs
/// from goldmark, which leaves them to the inline parser: in code spans
/// ([`unescape_code_pipes`]) and in autolinks, which keep the backslash.
pub(super) fn escaped_pipes(doc: &Doc<'_>, table: Node<'_>) {
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
pub(super) fn unescape_code_pipes(code: &str) -> Option<String> {
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
pub(super) fn blank_before(doc: &Doc<'_>, n: Node<'_>) -> bool {
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
pub(super) fn tighten(doc: &Doc<'_>, list: Node<'_>, converted: &HashSet<NodeKey>) {
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
pub(super) fn renumber_footnotes(doc: &Doc<'_>) {
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
