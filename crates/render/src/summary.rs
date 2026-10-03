//! Summaries and derived text (Go's `.Summary`, `.Truncated`, `.Plain`, word counts).
//!
//! Go cuts summaries from the rendered HTML: at the summary divider (which the content phase
//! turns into its own paragraph, [`DIVIDER_SOURCE`]), else from the front matter `summary`,
//! else automatically after whole paragraphs once `summaryLength` words are counted.

use std::ops::Range;

/// The summary divider as the expanded Markdown carries it: its own paragraph, so that the
/// rendered HTML holds it as `<p>` + [`DIVIDER`] + `</p>` (Go does the same).
pub const DIVIDER_SOURCE: &str = "\n\nNHSUMMARYDIVIDERX\n\n";
/// The divider token looked for in the rendered HTML.
pub const DIVIDER: &str = "NHSUMMARYDIVIDERX";

/// A summary cut from rendered content.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Split {
    /// The content (without the divider for a manual summary).
    pub content: String,
    pub summary: String,
    /// Whether the summary stops before the end of the content.
    pub truncated: bool,
}

/// The paragraph container of Markdown and HTML content.
const PARAGRAPH: &str = "p";

/// A manual summary: `html` up to the first `divider`.
///
/// With `paragraph` (Markdown), the divider grows to the paragraph that holds it: a divider
/// alone in its paragraph removes the whole paragraph; a divider after text in a paragraph
/// ends the summary there and closes the paragraph in the summary. HTML content keeps the
/// divider's own range. `None` when `html` has no divider.
#[must_use]
pub fn manual(html: &str, divider: &str, paragraph: bool) -> Option<Split> {
    if divider.is_empty() {
        return None;
    }
    let lo = html.find(divider)?;
    let found = lo..lo + divider.len();
    let (cut, end_tag) = if paragraph {
        expand(html, found)
    } else {
        (found, None)
    };
    let mut summary = html[..cut.start].to_owned();
    if let Some(t) = end_tag {
        summary.push_str(&html[t]);
    }
    let mut content = String::with_capacity(html.len());
    content.push_str(&html[..cut.start]);
    content.push_str(&html[cut.end..]);
    Some(Split {
        content: content.trim().to_owned(),
        summary: summary.trim().to_owned(),
        truncated: cut.start < html.len(),
    })
}

/// The range a divider found at `div` removes, and the end tag a summary cut inside a
/// paragraph keeps.
fn expand(html: &str, div: Range<usize>) -> (Range<usize>, Option<Range<usize>>) {
    // Back to the paragraph's start tag, over white space and `<div>` wrappers.
    let mut end = div.start;
    let mut start = div.start;
    let mut inside = false;
    loop {
        let t = html[..end].trim_end();
        if t.ends_with('>') {
            if let Some(s) = open_tag_before(t, PARAGRAPH) {
                start = s;
                break;
            }
            if let Some(s) = short_div_before(t) {
                end = s;
                continue;
            }
        }
        inside = !t.is_empty();
        break;
    }
    // Forward to the end of the paragraph.
    let close = format!("</{PARAGRAPH}>");
    let para_end = html[div.end..]
        .find(&close)
        .map_or(html.len(), |i| div.end + i + close.len());
    let (mut cut, end_tag) = if inside {
        (div.clone(), Some(div.end..para_end))
    } else {
        (start..para_end, None)
    };
    if html[cut.end..].starts_with('\n') {
        cut.end += 1;
    }
    (cut, end_tag)
}

/// Where the start tag `<tag…>` that `t` ends with begins (`t` ends with `>`; the tag is the
/// leftmost `<tag` after the previous `>`).
fn open_tag_before(t: &str, tag: &str) -> Option<usize> {
    let body = &t[..t.len() - 1];
    let from = body.rfind('>').map_or(0, |i| i + 1);
    let open = format!("<{tag}");
    body[from..].find(&open).map(|i| from + i)
}

/// Where a `<div>` (or `<div` + one character + `>`) that `t` ends with begins.
fn short_div_before(t: &str) -> Option<usize> {
    let at = t.rfind("<div")?;
    let rest = &t[at + 4..t.len() - 1];
    (rest.chars().count() <= 1 && !rest.contains('>')).then_some(at)
}

/// Go's automatic summary: whole paragraphs until `words` words are counted. Words that look
/// like HTML tags or attributes are not counted; with `cjk`, a word of non-ASCII text counts
/// each character; a paragraph is counted without its last character and with the `>` that ends the
/// previous one (Go's scan).
#[must_use]
pub fn auto(html: &str, words: usize, cjk: bool) -> Split {
    let whole = |truncated| Split {
        content: html.to_owned(),
        summary: html.trim().to_owned(),
        truncated,
    };
    if words == 0 {
        return Split {
            content: html.to_owned(),
            summary: String::new(),
            truncated: !html.is_empty(),
        };
    }
    let close = format!("</{PARAGRAPH}>");
    let mut count = 0;
    let mut at = 0;
    while let Some(i) = html[at..].find(&close) {
        // Go counts the paragraph without its last character: a one-character last word
        // is not a word, and a CJK last word counts one character less.
        let para = &html[at..at + i];
        let counted = para
            .char_indices()
            .next_back()
            .map_or("", |(last, _)| &para[..last]);
        count += counted
            .split_whitespace()
            .map(|w| word_weight(w, cjk))
            .sum::<usize>();
        let end = at + i + close.len();
        if count >= words {
            return Split {
                content: html.to_owned(),
                summary: html[..end].trim().to_owned(),
                truncated: end < html.len(),
            };
        }
        // Go resumes at the `>` of this `</p>`, which then sticks to the next paragraph's
        // first word (`></blockquote>` counts as a word).
        at = end - 1;
    }
    whole(false)
}

/// How many words `w` counts for.
fn word_weight(w: &str, cjk: bool) -> usize {
    if is_html_token(w) {
        return 0;
    }
    if cjk {
        let plain = ssg_markup::text::strip_html(w);
        let chars = plain.chars().count();
        if chars != plain.len() {
            return chars;
        }
    }
    1
}

/// `>`, a tag without attributes (`<p>`, `</em>`, `<a`) or the start of an attribute
/// (`href="…`): not counted as words.
fn is_html_token(w: &str) -> bool {
    if w == ">" {
        return true;
    }
    let letters = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphabetic());
    if let Some(tag) = w.strip_prefix('<') {
        let tag = tag.strip_prefix('/').unwrap_or(tag);
        let tag = tag.strip_suffix('>').unwrap_or(tag);
        return letters(tag);
    }
    w.split_once('=')
        .is_some_and(|(name, v)| letters(name) && v.starts_with(['"', '\'']))
}

/// Go's `.Plain`: the text of `html` without tags (paragraph ends and `<br>` become
/// newlines, other newlines spaces), runs of white space kept as their first character.
#[must_use]
pub fn plain(html: &str) -> String {
    let stripped = ssg_markup::text::strip_html(html);
    let mut out = String::with_capacity(stripped.len());
    let mut was_space = false;
    for c in stripped.chars() {
        let space = c.is_whitespace();
        if !(space && was_space) {
            out.push(c);
        }
        was_space = space;
    }
    out
}

/// Go's `.WordCount`, `.FuzzyWordCount` and `.ReadingTime` of `plain`.
#[must_use]
pub fn counts(plain: &str, cjk: bool) -> (usize, usize, usize) {
    let words = ssg_markup::text::word_count(plain, cjk);
    let fuzzy = (words + 100) / 100 * 100;
    let reading = if cjk {
        words.div_ceil(501)
    } else {
        words.div_ceil(213)
    };
    (words, fuzzy, reading)
}

/// Removes the `<p>` wrapper of a single paragraph (`markdownify`, a front matter summary).
#[must_use]
pub fn unwrap_paragraph(html: &str) -> String {
    if html.matches("<p>").count() == 1 {
        let t = html.trim();
        if let Some(inner) = t.strip_prefix("<p>").and_then(|s| s.strip_suffix("</p>")) {
            return inner.trim().to_owned();
        }
        return t.to_owned();
    }
    html.to_owned()
}
