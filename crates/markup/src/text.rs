//! Text derived from rendered HTML: plain text, word counts and summaries.

/// A summary cut from rendered content.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub html: String,
    /// Whether content follows the summary.
    pub truncated: bool,
}

/// The text of `html` without tags (Go's `.Plain`): paragraph ends and `<br>` become
/// newlines, newlines inside the HTML become spaces; entities stay as written.
#[must_use]
pub fn strip_html(html: &str) -> String {
    if !html.contains(['<', '>']) {
        return html.to_owned();
    }
    let pre = html
        .replace('\n', " ")
        .replace("</p>", "\n")
        .replace("<br>", "\n")
        .replace("</br>", "\n")
        .replace("<br />", "\n");
    let b = pre.as_bytes();
    let mut out = String::with_capacity(pre.len());
    let mut i = 0;
    let mut last = 0;
    while i < b.len() {
        let tag = b[i] == b'<'
            && b.get(i + 1)
                .is_some_and(|&c| c.is_ascii_alphabetic() || matches!(c, b'/' | b'!' | b'?'));
        if !tag {
            i += 1;
            continue;
        }
        out.push_str(&pre[last..i]);
        let end = if pre[i..].starts_with("<!--") {
            pre[i..].find("-->").map_or(b.len(), |e| i + e + 3)
        } else {
            let mut j = i + 1;
            let mut quote = None;
            while j < b.len() {
                match (quote, b[j]) {
                    (None, q @ (b'"' | b'\'')) => quote = Some(q),
                    (Some(q), c) if c == q => quote = None,
                    (None, b'>') => break,
                    _ => {}
                }
                j += 1;
            }
            (j + 1).min(b.len())
        };
        i = end;
        last = end;
    }
    out.push_str(&pre[last..]);
    out
}

/// Whether `c` belongs to a script written without spaces between words.
fn is_cjk(c: char) -> bool {
    matches!(u32::from(c),
        0x1100..=0x11FF | 0x2E80..=0x9FFF | 0xA960..=0xA97F | 0xAC00..=0xD7FF
        | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFFEF | 0x20000..=0x2FFFF)
}

/// Words in `plain` (Go's `.WordCount`): runs of non-space; with `cjk`, a run containing
/// non-ASCII characters counts each character.
#[must_use]
pub fn word_count(plain: &str, cjk: bool) -> usize {
    plain
        .split_whitespace()
        .map(|w| {
            let chars = w.chars().count();
            if cjk && chars != w.len() { chars } else { 1 }
        })
        .sum()
}

fn counts_as_word(w: &str, cjk: bool) -> usize {
    if w.is_empty() || (w.starts_with('<') && w.ends_with('>')) {
        return 0;
    }
    if cjk {
        let plain = strip_html(w);
        let chars = plain.chars().count();
        return if chars == plain.len() {
            1
        } else {
            plain.chars().filter(|&c| is_cjk(c)).count().max(1)
        };
    }
    1
}

/// The Go implementation's automatic summary: whole paragraphs until `words` words are reached.
#[must_use]
pub fn auto_summary(html: &str, words: usize, cjk: bool) -> Summary {
    if words == 0 {
        return Summary {
            html: String::new(),
            truncated: !html.trim().is_empty(),
        };
    }
    let mut count = 0;
    let mut at = 0;
    while let Some(rel) = html[at..].find("</p>") {
        let end = at + rel + "</p>".len();
        for w in html[at..at + rel].split_whitespace() {
            count += counts_as_word(w, cjk);
            if count >= words {
                break;
            }
        }
        if count >= words {
            return Summary {
                html: html[..end].to_owned(),
                truncated: !html[end..].trim().is_empty(),
            };
        }
        at = end;
    }
    Summary {
        html: html.to_owned(),
        truncated: false,
    }
}

/// Splits rendered content at the summary divider `marker` (dropping a `<p>` that held only
/// the marker): `(summary, rest)`.
#[must_use]
pub fn split_at_marker(html: &str, marker: &str) -> Option<(String, String)> {
    let at = html.find(marker)?;
    let mut before = html[..at].trim_end();
    let mut after = html[at + marker.len()..].trim_start();
    if let (Some(b), Some(a)) = (before.strip_suffix("<p>"), after.strip_prefix("</p>")) {
        before = b.trim_end();
        after = a.trim_start();
    }
    Some((before.to_owned(), after.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain() {
        assert_eq!(
            strip_html("<p>A <em>b</em> &amp; c</p>\n<p>d<br>e</p>"),
            "A b &amp; c\n d\ne\n"
        );
        assert_eq!(strip_html("a < b"), "a < b");
    }

    #[test]
    fn words() {
        assert_eq!(word_count("one two\nthree", false), 3);
        assert_eq!(word_count("日本語 abc", true), 4);
    }

    #[test]
    fn summaries() {
        let html = "<p>one two</p>\n<p>three four</p>\n<p>five</p>\n";
        assert_eq!(
            auto_summary(html, 3, false),
            Summary {
                html: "<p>one two</p>\n<p>three four</p>".into(),
                truncated: true
            }
        );
        assert!(!auto_summary(html, 50, false).truncated);
        assert_eq!(
            split_at_marker("<p>a</p>\n<p>MORE</p>\n<p>b</p>", "MORE"),
            Some(("<p>a</p>".into(), "<p>b</p>".into()))
        );
    }
}
