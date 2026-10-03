//! Page-local ASCII tokens in expanded sources (REWRITE_PLAN.md §3.2):
//!
//! - `NHSC<n>X` (`n` lower-case hex): the output of the `n`-th `{{< >}}` call of a page,
//!   swapped in after Markdown ([`swap`]); a `<p>` that holds only the token is removed.
//! - `NHRS<n>X`: an include (`render_shortcodes` in the content phase) that the expanding
//!   page replaces by the included page's expanded source ([`Inclusions`]).

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use ssg_base::PageId;
use ssg_view::ExpandedSource;

/// The prefix of a placeholder token.
pub(crate) const PLACEHOLDER: &str = "NHSC";
/// The prefix of an inclusion token.
pub(crate) const INCLUSION: &str = "NHRS";

/// The placeholder token of call `n`.
pub(crate) fn placeholder(n: usize) -> String {
    format!("{PLACEHOLDER}{n:x}X")
}

/// The tokens with `prefix` in `s`: their range and number.
pub(crate) fn find(s: &str, prefix: &str) -> Vec<(Range<usize>, u64)> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(i) = s[at..].find(prefix) {
        let start = at + i;
        let digits_at = start + prefix.len();
        let digits = s[digits_at..]
            .bytes()
            .take_while(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
            .count();
        let end = digits_at + digits;
        if digits > 0 && s.as_bytes().get(end) == Some(&b'X') {
            if let Ok(n) = u64::from_str_radix(&s[digits_at..end], 16) {
                out.push((start..end + 1, n));
            }
            at = end + 1;
        } else {
            at = digits_at;
        }
    }
    out
}

/// `s` with every token with `prefix` replaced by `with(n)` (`None` keeps it).
pub(crate) fn replace(
    s: &str,
    prefix: &str,
    mut with: impl FnMut(u64) -> Option<String>,
) -> String {
    let found = find(s, prefix);
    if found.is_empty() {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut last = 0;
    for (r, n) in found {
        out.push_str(&s[last..r.start]);
        match with(n) {
            Some(v) => out.push_str(&v),
            None => out.push_str(&s[r.clone()]),
        }
        last = r.end;
    }
    out.push_str(&s[last..]);
    out
}

/// `s` with its placeholder numbers shifted by `offset` (an included source appended to the
/// includer's table).
pub(crate) fn renumber(s: &str, offset: usize) -> String {
    replace(s, PLACEHOLDER, |n| {
        usize::try_from(n).ok().map(|n| placeholder(n + offset))
    })
}

/// Rendered content with the placeholders replaced by the calls' outputs. A placeholder that
/// is the only content of a `<p>` replaces the paragraph.
pub(crate) fn swap(html: &str, outputs: &[Arc<str>]) -> String {
    let found = find(html, PLACEHOLDER);
    if found.is_empty() {
        return html.to_owned();
    }
    let mut out = String::with_capacity(html.len());
    let mut last = 0;
    for (r, n) in found {
        let Some(v) = usize::try_from(n).ok().and_then(|n| outputs.get(n)) else {
            continue;
        };
        let (mut start, mut end) = (r.start, r.end);
        if html[..start].ends_with("<p>") && html[end..].starts_with("</p>") && start - 3 >= last {
            start -= 3;
            end += 4;
        }
        out.push_str(&html[last..start]);
        out.push_str(v);
        last = end;
    }
    out.push_str(&html[last..]);
    out
}

/// A source with its placeholders replaced by their outputs (no paragraph unwrapping) and
/// without the context markers of its own includes: what an include is outside Markdown.
pub(crate) fn resolve(src: &ExpandedSource) -> String {
    let text = ssg_markup::strip_context_markers(&src.markdown);
    replace(&text, PLACEHOLDER, |n| {
        usize::try_from(n)
            .ok()
            .and_then(|n| src.placeholders.get(n))
            .map(ToString::to_string)
    })
}

/// Included expanded sources handed out as `NHRS<n>X` tokens in the content phase, taken back
/// by the expansion that prints them.
#[derive(Debug, Default)]
pub(crate) struct Inclusions {
    next: AtomicU64,
    open: Mutex<BTreeMap<u64, (PageId, Arc<ExpandedSource>)>>,
}

impl Inclusions {
    /// A token for `src` (page `page`).
    pub(crate) fn token(&self, page: PageId, src: Arc<ExpandedSource>) -> String {
        let n = self.next.fetch_add(1, Ordering::Relaxed);
        self.open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(n, (page, src));
        format!("{INCLUSION}{n:x}X")
    }

    /// Takes the source of token `n`.
    pub(crate) fn take(&self, n: u64) -> Option<(PageId, Arc<ExpandedSource>)> {
        self.open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&n)
    }

    /// `s` with every inclusion token replaced by the included text with its placeholders
    /// resolved (output that is not Markdown).
    pub(crate) fn resolve_all(&self, s: &str) -> String {
        replace(s, INCLUSION, |n| self.take(n).map(|(_, src)| resolve(&src)))
    }
}
