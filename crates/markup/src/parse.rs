//! Source preparation, comrak parsing and the pass pipeline.

use std::ops::Range;

use comrak::nodes::NodeValue;
use comrak::{Arena, Options, parse_document};

use crate::attributes;
use crate::doc::Doc;
use crate::passes::blocks::{self, BlockAttrLine, Passthrough};
use crate::passes::{contexts, ids, inline, tables};
use crate::source::{ExpandedMarkdown, Lines, Prepared};
use crate::{Extensions, MarkdownOptions, MarkupError, RawHtml, StandaloneImages};

/// comrak's options for `o`: parsing only; Go's typography, linkify, attributes, alerts
/// and math are passes of this crate, and so are tables (goldmark makes them of paragraphs,
/// [`crate::passes::tables`]).
pub(crate) fn comrak_options(o: &MarkdownOptions) -> Options<'static> {
    let e = o.extensions;
    let mut c = Options::default();
    c.extension.strikethrough = e.contains(Extensions::STRIKETHROUGH);
    c.extension.tasklist = e.contains(Extensions::TASKLISTS);
    c.extension.description_lists = e.contains(Extensions::DEFINITION_LISTS);
    c.extension.footnotes = e.contains(Extensions::FOOTNOTES);
    c.extension.shortcodes = e.contains(Extensions::EMOJI);
    c.parse.escaped_char_spans = true;
    c
}

/// Parses `md` and runs the Go implementation's passes.
pub(crate) fn parse<'a>(
    arena: &'a Arena<'a>,
    md: &ExpandedMarkdown<'_>,
    o: &MarkdownOptions,
) -> Result<Doc<'a>, MarkupError> {
    let copts = comrak_options(o);
    let prep = prepare(md.text, o, &copts);
    let root = parse_document(arena, &prep.prepared.text, &copts);
    let mut doc = Doc::new(arena, root, prep.prepared, std::sync::Arc::clone(md.file));
    let e = o.extensions;
    blocks::fix_link_ref_lines(&doc, doc.root);
    if e.contains(Extensions::TABLES) {
        tables::tables(&mut doc, &copts);
    }
    blocks::link_reference_blocks(&mut doc, e.contains(Extensions::FOOTNOTES));
    if o.raw_html == RawHtml::Omit {
        blocks::drop_comments(&doc);
    }
    if !prep.passthrough.is_empty() {
        blocks::passthrough(&mut doc, &prep.passthrough);
    }
    blocks::angle_autolinks(&mut doc);
    if e.contains(Extensions::BLOCK_ATTRIBUTES) {
        blocks::apply_block_attributes(&mut doc, &prep.block_attrs)?;
    }
    if e.contains(Extensions::HEADING_ATTRIBUTES) {
        blocks::heading_attributes(&mut doc)?;
    }
    if e.contains(Extensions::DEFINITION_LISTS) {
        blocks::definition_lists(&mut doc);
    }
    contexts::contexts(&mut doc);
    if o.standalone_images == StandaloneImages::Block {
        blocks::block_images(&mut doc);
    }
    inline::run(
        &mut doc,
        o.typographer.as_ref(),
        e.contains(Extensions::LINKIFY),
    );
    ids::assign(&mut doc, o);
    Ok(doc)
}

struct Prep {
    prepared: Prepared,
    block_attrs: Vec<BlockAttrLine>,
    passthrough: Vec<Passthrough>,
}

/// Blanks block-attribute lines (same length, so positions stay) and replaces passthrough
/// spans by placeholder tokens, outside code and raw HTML.
fn prepare(text: &str, o: &MarkdownOptions, copts: &Options<'_>) -> Prep {
    let want_attrs = o.extensions.contains(Extensions::BLOCK_ATTRIBUTES) && text.contains('{');
    let want_math = !o.passthrough.is_empty()
        && o.passthrough
            .iter()
            .any(|d| !d.open.is_empty() && text.contains(d.open.as_str()));
    if !want_attrs && !want_math {
        return Prep {
            prepared: Prepared::new(text, &[]),
            block_attrs: Vec::new(),
            passthrough: Vec::new(),
        };
    }
    let verbatim = verbatim_ranges(text, copts);
    let in_verbatim = |at: usize| verbatim.iter().any(|r| r.contains(&at));
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    let mut block_attrs = Vec::new();
    if want_attrs {
        let mut start = 0;
        for (i, line) in text.split_inclusive('\n').enumerate() {
            let at = start;
            start += line.len();
            let body = line.trim_end_matches(['\n', '\r']);
            let Some((quote_depth, indent, brace)) = attribute_line(body) else {
                continue;
            };
            if indent >= 4 || in_verbatim(at + brace) {
                continue;
            }
            let Some(attrs) = attributes::parse_all(&body[brace..]) else {
                continue;
            };
            edits.push((at + brace..at + body.len(), " ".repeat(body.len() - brace)));
            block_attrs.push(BlockAttrLine {
                line: i + 1,
                quote_depth,
                indent,
                attrs,
            });
        }
    }
    let mut passthrough = Vec::new();
    if want_math {
        let taken = |r: &Range<usize>, edits: &[(Range<usize>, String)]| {
            edits
                .iter()
                .any(|(e, _)| e.start < r.end && r.start < e.end)
        };
        let mut i = 0;
        let b = text.as_bytes();
        while i < b.len() {
            let step = |i: usize| i + text[i..].chars().next().map_or(1, char::len_utf8);
            if in_verbatim(i) {
                i = step(i);
                continue;
            }
            if b[i] == b'\\' && i + 1 < b.len() && !starts_delim(o, &text[i..]) {
                i = step(i + 1);
                continue;
            }
            let Some((d, end)) = o.passthrough.iter().find_map(|d| {
                let close = find_close(text, i, d)?;
                Some((d, close))
            }) else {
                i += text[i..].chars().next().map_or(1, char::len_utf8);
                continue;
            };
            let r = i..end;
            if taken(&r, &edits) || verbatim.iter().any(|v| v.start < r.end && r.start < v.end) {
                i += 1;
                continue;
            }
            let token = format!("NHPT{}X", passthrough.len());
            passthrough.push(Passthrough {
                token: token.clone(),
                kind: d.kind,
                inner: text[i + d.open.len()..end - d.close.len()].to_owned(),
                raw: text[r.clone()].to_owned(),
            });
            edits.push((r, token));
            i = end;
        }
    }
    edits.sort_by_key(|(r, _)| r.start);
    Prep {
        prepared: Prepared::new(text, &edits),
        block_attrs,
        passthrough,
    }
}

fn starts_delim(o: &MarkdownOptions, s: &str) -> bool {
    o.passthrough
        .iter()
        .any(|d| !d.open.is_empty() && s.starts_with(d.open.as_str()))
}

/// The end of a passthrough element opening at `at` with delimiters `d`: after its closing
/// delimiter. Inline elements do not cross a blank line.
fn find_close(text: &str, at: usize, d: &crate::Delimiters) -> Option<usize> {
    if d.open.is_empty() || d.close.is_empty() || !text[at..].starts_with(d.open.as_str()) {
        return None;
    }
    let from = at + d.open.len();
    let rel = text[from..].find(d.close.as_str())?;
    let inner = &text[from..from + rel];
    if inner.is_empty() {
        return None;
    }
    if d.kind == crate::PassthroughKind::Inline
        && inner.split('\n').skip(1).any(|l| l.trim().is_empty())
    {
        return None;
    }
    Some(from + rel + d.close.len())
}

/// `(quote depth, indent, offset of '{')` when `line` is `> … {…}` shaped.
fn attribute_line(line: &str) -> Option<(usize, usize, usize)> {
    let b = line.as_bytes();
    let mut i = 0;
    let mut depth = 0;
    loop {
        let spaces = b[i..].iter().take_while(|&&c| c == b' ').count();
        if spaces <= 3 && b.get(i + spaces) == Some(&b'>') {
            depth += 1;
            i += spaces + 1;
            if b.get(i) == Some(&b' ') {
                i += 1;
            }
        } else {
            break;
        }
    }
    let indent = b[i..]
        .iter()
        .take_while(|&&c| c == b' ' || c == b'\t')
        .count();
    let brace = i + indent;
    (b.get(brace) == Some(&b'{') && line.trim_end().ends_with('}'))
        .then_some((depth, indent, brace))
}

/// Byte ranges of code (blocks and spans) and raw HTML in `text`.
fn verbatim_ranges(text: &str, copts: &Options<'_>) -> Vec<Range<usize>> {
    let arena = Arena::new();
    let root = parse_document(&arena, text, copts);
    let lines = Lines::new(text);
    let mut out = Vec::new();
    for n in root.descendants() {
        let d = n.data();
        let sp = d.sourcepos;
        match d.value {
            NodeValue::CodeBlock(_) | NodeValue::HtmlBlock(_) => {
                let start = lines.offset((sp.start.line, 1).into(), text.len());
                let end = lines.offset((sp.end.line + 1, 1).into(), text.len());
                out.push(start..end);
            }
            NodeValue::Code(_) | NodeValue::HtmlInline(_) => {
                let start = lines.offset(sp.start, text.len());
                let end = lines.offset(sp.end, text.len()) + 1;
                out.push(start..end.min(text.len()));
            }
            _ => {}
        }
    }
    out
}
