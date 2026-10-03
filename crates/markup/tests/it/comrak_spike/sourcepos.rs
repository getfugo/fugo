//! `sourcepos` accuracy for inline nodes: the render-hook context (`inner_page`) is the
//! innermost `render_shortcodes` span containing a node's start byte, so every hooked inline
//! node (link, image) must map back to the exact source bytes it came from.

use std::collections::BTreeMap;

use comrak::nodes::{AstNode, NodeValue};
use comrak::{Arena, Options, parse_document};

use super::Row;
use super::features::Lines;

#[derive(Default)]
struct Tally {
    ok: usize,
    total: usize,
    examples: Vec<String>,
}

impl Tally {
    fn add(&mut self, ok: bool, example: impl FnOnce() -> String) {
        self.total += 1;
        if ok {
            self.ok += 1;
        } else if self.examples.len() < 3 {
            self.examples.push(example());
        }
    }
}

/// The source slice `[start, end]` (end inclusive, widened to a char boundary).
fn slice<'s>(src: &'s str, lines: &Lines, n: &AstNode<'_>) -> Option<&'s str> {
    let sp = n.data().sourcepos;
    let s = lines.offset(sp.start.line, sp.start.column)?;
    let mut e = lines.offset(sp.end.line, sp.end.column)? + 1;
    while e < src.len() && !src.is_char_boundary(e) {
        e += 1;
    }
    src.get(s..e.min(src.len()))
}

/// The known comrak defect: a paragraph that starts with link reference definitions reports
/// its inlines' lines relative to the paragraph start, before the definitions were removed.
fn after_refdefs<'a>(n: &'a AstNode<'a>, src: &str, lines: &Lines) -> bool {
    n.ancestors()
        .find(|a| matches!(a.data().value, NodeValue::Paragraph))
        .is_some_and(|p| {
            let first = lines.line(src, p.data().sourcepos.start.line);
            let first = first.trim_start_matches(|c: char| c == '>' || c.is_whitespace());
            first.starts_with('[') && !first.starts_with("[^") && first.contains("]:")
        })
}

fn context<'a>(n: &'a AstNode<'a>) -> &'static str {
    for a in n.ancestors() {
        match a.data().value {
            NodeValue::TableCell => return "table",
            NodeValue::Heading(_) => return "heading",
            NodeValue::DescriptionDetails | NodeValue::DescriptionTerm => return "deflist",
            NodeValue::BlockQuote | NodeValue::Alert(_) => return "blockquote",
            NodeValue::Item(_) | NodeValue::TaskItem(_) => return "list",
            NodeValue::FootnoteDefinition(_) => return "footnote",
            _ => {}
        }
    }
    "paragraph"
}

fn text_of<'a>(n: &'a AstNode<'a>) -> String {
    n.descendants()
        .filter_map(|d| match &d.data().value {
            NodeValue::Text(t) => Some(t.to_string()),
            _ => None,
        })
        .collect()
}

/// Checks every inline node of `docs`; returns the report rows.
pub fn check(docs: &[(String, String)], o: &Options<'_>) -> Vec<Row> {
    let mut by_kind: BTreeMap<String, Tally> = BTreeMap::new();
    for (name, md) in docs {
        let arena = Arena::new();
        let root = parse_document(&arena, md, o);
        let lines = Lines::new(md);
        for n in root.descendants() {
            let Some(src) = slice(md, &lines, n) else {
                continue;
            };
            let first = src.chars().next().unwrap_or(' ');
            let last = src.chars().next_back().unwrap_or(' ');
            let ex = || format!("{name}: {}", src.chars().take(60).collect::<String>());
            let (kind, ok) = match &n.data().value {
                NodeValue::Link(l) => {
                    let ok = match first {
                        '[' => last == ')' || last == ']',
                        '<' => last == '>',
                        // linkify: the slice is the link text (the URL as written).
                        _ => src == text_of(n) || l.url.ends_with(src),
                    };
                    (format!("link ({})", context(n)), ok)
                }
                NodeValue::Image(_) => (
                    format!("image ({})", context(n)),
                    src.starts_with("![") && (last == ')' || last == ']'),
                ),
                NodeValue::Code(_) => ("code span".into(), first == '`' && last == '`'),
                NodeValue::Emph | NodeValue::Strong | NodeValue::Strikethrough => (
                    "emphasis".into(),
                    "*_~".contains(first) && "*_~".contains(last),
                ),
                NodeValue::HtmlInline(h) => ("raw inline HTML".into(), src == h.as_str()),
                NodeValue::Text(t) => {
                    let plain = !src.contains(['\\', '&'])
                        && !t.contains(['‘', '’', '“', '”', '–', '—', '…']);
                    if !plain
                        || n.parent()
                            .is_some_and(|p| matches!(p.data().value, NodeValue::Link(_)))
                    {
                        continue;
                    }
                    ("text".into(), src == t.as_ref())
                }
                _ => continue,
            };
            let kind = if after_refdefs(n, md, &lines) {
                format!("{kind}, paragraph after link ref defs")
            } else {
                kind
            };
            by_kind.entry(kind).or_default().add(ok, ex);
        }
    }
    by_kind
        .into_iter()
        .map(|(kind, t)| {
            let row = Row::new(format!("sourcepos {kind}"), "nodes", t.ok, t.total);
            if t.examples.is_empty() {
                row
            } else {
                row.note(format!("e.g. {:?}", t.examples))
            }
        })
        .collect()
}
