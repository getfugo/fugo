//! The Markdown source: the expanded text of a page, the spans of it that came from other
//! pages, and the text comrak actually parses (with passthrough placeholders and blanked
//! block-attribute lines) mapped back to byte offsets of the expanded text.

use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use comrak::nodes::LineColumn;
use ssg_base::PageId;
use ssg_base::diag::Position;

/// Byte ranges of the expanded source that came from another page (`render_shortcodes`
/// includes). Spans nest; the innermost span containing a node decides its `inner_page`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceContexts(pub Vec<(Range<usize>, PageId)>);

impl SourceContexts {
    /// The page of the innermost span containing `offset`, else `page`.
    #[must_use]
    pub fn inner_page(&self, offset: usize, page: PageId) -> PageId {
        self.0
            .iter()
            .filter(|(r, _)| r.contains(&offset))
            .min_by_key(|(r, _)| r.len())
            .map_or(page, |&(_, p)| p)
    }
}

/// The line that opens an included page's text in an expanded source (the Go
/// implementation's opening context marker, which carries the page id).
///
/// Like Go's, the markers start with `{` and end with `}`, so emphasis delimiters next to
/// an include flank the same way; no comrak syntax, attribute, typographer or linkify rule
/// reads them.
pub const CONTEXT_OPEN: &str = "{{NHCTXO}}";
/// The line that closes an included page's text (the Go implementation's closing context
/// marker).
pub const CONTEXT_CLOSE: &str = "{{NHCTXC}}";

/// An included page's Markdown between context marker lines, as the Go implementation's
/// context wrapper wraps what `.RenderShortcodes` returns to a `{{% %}}` call of a Markdown page.
///
/// Goldmark parses the markers as ordinary lines (a marker line can start a paragraph, be a
/// lazy continuation line or a table row), so they shape the blocks around an include; the
/// renderer drops them (see the crate README). Returns the wrapped text and the range of
/// `md` in it.
#[must_use]
pub fn wrap_context(md: &str) -> (String, Range<usize>) {
    let mut s = String::with_capacity(md.len() + 2 * CONTEXT_OPEN.len() + 3);
    s.push_str(CONTEXT_OPEN);
    s.push('\n');
    let start = s.len();
    s.push_str(md);
    let end = s.len();
    // goldmark needs the closing marker on a line of its own.
    if !md.is_empty() && !md.ends_with('\n') {
        s.push('\n');
    }
    s.push_str(CONTEXT_CLOSE);
    s.push('\n');
    (s, start..end)
}

/// `s` without context marker lines (an included source printed outside Markdown; the Go
/// implementation's context-marker extension strips them from raw HTML blocks the same way).
#[must_use]
pub fn strip_context_markers(s: &str) -> std::borrow::Cow<'_, str> {
    if !s.contains(CONTEXT_OPEN) && !s.contains(CONTEXT_CLOSE) {
        return std::borrow::Cow::Borrowed(s);
    }
    std::borrow::Cow::Owned(
        s.replace(&format!("{CONTEXT_OPEN}\n"), "")
            .replace(&format!("{CONTEXT_CLOSE}\n"), "")
            .replace(CONTEXT_OPEN, "")
            .replace(CONTEXT_CLOSE, ""),
    )
}

/// A page's Markdown after shortcode expansion.
#[derive(Clone, Copy, Debug)]
pub struct ExpandedMarkdown<'a> {
    pub text: &'a str,
    /// The page being rendered.
    pub page: PageId,
    pub contexts: &'a SourceContexts,
    /// The content file, for hook positions and errors.
    pub file: &'a Arc<Path>,
}

/// One replacement in the parsed text: `len` bytes at `at` stand for `orig_len` bytes at
/// `orig_at` of the expanded text.
#[derive(Clone, Copy, Debug)]
struct Edit {
    at: usize,
    len: usize,
    orig_at: usize,
    orig_len: usize,
}

/// Byte offsets of line starts.
#[derive(Clone, Debug)]
pub(crate) struct Lines(Vec<usize>);

impl Lines {
    pub(crate) fn new(s: &str) -> Self {
        let mut v = vec![0];
        v.extend(s.match_indices('\n').map(|(i, _)| i + 1));
        Self(v)
    }

    /// The number of lines.
    pub(crate) fn count(&self) -> usize {
        self.0.len()
    }

    /// The byte offset of a 1-based line and byte column (clamped to `len`).
    pub(crate) fn offset(&self, lc: LineColumn, len: usize) -> usize {
        let start = self
            .0
            .get(lc.line.saturating_sub(1))
            .copied()
            .unwrap_or(len);
        (start + lc.column.saturating_sub(1)).min(len)
    }

    /// 1-based line and byte column of `offset`.
    pub(crate) fn line_col(&self, offset: usize) -> (usize, usize) {
        let line = self.0.partition_point(|&s| s <= offset);
        (line, offset - self.0[line - 1] + 1)
    }

    /// The text of 1-based line `line`, without its newline.
    pub(crate) fn line<'s>(&self, s: &'s str, line: usize) -> &'s str {
        let Some(&start) = self.0.get(line.wrapping_sub(1)) else {
            return "";
        };
        let end = self.0.get(line).map_or(s.len(), |e| e - 1);
        s.get(start..end).unwrap_or("")
    }
}

/// The text comrak parses and how it maps back to the expanded source.
#[derive(Clone, Debug)]
pub(crate) struct Prepared {
    pub text: String,
    pub lines: Lines,
    edits: Vec<Edit>,
    original_lines: Lines,
    original_len: usize,
}

impl Prepared {
    /// `original` with `edits` (sorted, non-overlapping `(range, replacement)` pairs) applied.
    pub(crate) fn new(original: &str, edits: &[(Range<usize>, String)]) -> Self {
        let mut text = String::with_capacity(original.len());
        let mut out = Vec::with_capacity(edits.len());
        let mut last = 0;
        for (r, with) in edits {
            text.push_str(&original[last..r.start]);
            out.push(Edit {
                at: text.len(),
                len: with.len(),
                orig_at: r.start,
                orig_len: r.len(),
            });
            text.push_str(with);
            last = r.end;
        }
        text.push_str(&original[last..]);
        Self {
            lines: Lines::new(&text),
            text,
            edits: out,
            original_lines: Lines::new(original),
            original_len: original.len(),
        }
    }

    /// The byte offset in the parsed text of a comrak line/column.
    pub(crate) fn offset(&self, lc: LineColumn) -> usize {
        self.lines.offset(lc, self.text.len())
    }

    /// The offset in the expanded source of offset `at` of the parsed text.
    pub(crate) fn original(&self, at: usize) -> usize {
        let i = self.edits.partition_point(|e| e.at <= at);
        let Some(e) = i.checked_sub(1).map(|i| self.edits[i]) else {
            return at.min(self.original_len);
        };
        let v = if at < e.at + e.len {
            e.orig_at + (at - e.at).min(e.orig_len)
        } else {
            e.orig_at + e.orig_len + (at - e.at - e.len)
        };
        v.min(self.original_len)
    }

    /// A diagnostic position for expanded-source offset `at`.
    pub(crate) fn position(&self, file: &Arc<Path>, at: usize) -> Position {
        let (line, col) = self.original_lines.line_col(at);
        Position {
            file: Arc::clone(file),
            line: u32::try_from(line).unwrap_or(u32::MAX),
            col: u32::try_from(col).unwrap_or(u32::MAX),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_map_back() {
        let src = "a $x+y$ b\nc";
        let p = Prepared::new(src, &[(2..7, "NHPT0X".to_owned())]);
        assert_eq!(p.text, "a NHPT0X b\nc");
        assert_eq!(p.original(0), 0);
        assert_eq!(p.original(3), 3);
        assert_eq!(p.original(9), 8);
        assert_eq!(p.original(11), 10);
        let (l, c) = p.original_lines.line_col(10);
        assert_eq!((l, c), (2, 1));
    }

    #[test]
    fn innermost_context() {
        let c = SourceContexts(vec![
            (0..10, PageId::from_raw(7)),
            (2..5, PageId::from_raw(8)),
        ]);
        let p = PageId::from_raw(1);
        assert_eq!(c.inner_page(3, p), PageId::from_raw(8));
        assert_eq!(c.inner_page(7, p), PageId::from_raw(7));
        assert_eq!(c.inner_page(12, p), p);
    }
}
