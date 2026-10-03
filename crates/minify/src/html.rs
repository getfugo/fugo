//! HTML through minify-html (inline CSS through lightningcss, inline JS through oxc).

use crate::MinifyError;
use crate::options::{HtmlComments, HtmlOptions, TemplateSyntax};

pub(crate) fn minify(
    o: &HtmlOptions,
    css: bool,
    js: bool,
    input: &str,
) -> Result<String, MinifyError> {
    let mut cfg = minify_html::Cfg {
        keep_comments: o.comments == HtmlComments::KeepAll,
        keep_ssi_comments: o.comments != HtmlComments::Remove,
        keep_closing_tags: o.keep_end_tags,
        keep_html_and_head_opening_tags: o.keep_document_tags,
        keep_input_type_text_attr: o.keep_default_attr_vals,
        preserve_brace_template_syntax: o.templates == TemplateSyntax::Braces,
        preserve_chevron_percent_template_syntax: o.templates == TemplateSyntax::ChevronPercent,
        minify_css: css,
        minify_js: js,
        // Spec-compliant output only: no `<!doctypehtml>`, no unquoted values with `"'=<>` or
        // backticks, no attributes run together.
        ..minify_html::Cfg::default()
    };
    let mut out = minify_html::minify(input.as_bytes(), &cfg);
    // A second pass (inline CSS and JS are done) makes the result idempotent where minify-html
    // decides on the tokens it has not yet removed: it collapses whitespace before it drops
    // comments (`a <!-- c --> b` leaves two spaces), and it omits an end tag by the next
    // sibling as written (`</tfoot>` before an omitted `</tbody>`).
    let dropped_comments = o.comments != HtmlComments::KeepAll && input.contains("<!--");
    if dropped_comments || !o.keep_end_tags {
        cfg.minify_css = false;
        cfg.minify_js = false;
        out = minify_html::minify(&out, &cfg);
    }
    let out = String::from_utf8(out).map_err(|_| MinifyError::HtmlEncoding)?;
    Ok(collapse_titles(&out))
}

/// `html` with the whitespace of every `<title>` collapsed and trimmed, as Go's minifier writes
/// it (a title is text; browsers and search engines read it that way). minify-html keeps the
/// layout's line breaks and indentation there.
fn collapse_titles(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut at = 0;
    while let Some(open) = lower[at..].find("<title").map(|i| at + i) {
        let after = lower.as_bytes().get(open + 6).copied();
        let Some(gt) = (matches!(after, Some(b'>' | b' ' | b'\t' | b'\n' | b'\r' | b'\x0c')))
            .then(|| lower[open..].find('>'))
            .flatten()
            .map(|i| open + i + 1)
        else {
            out.push_str(&html[at..open + 6]);
            at = open + 6;
            continue;
        };
        let Some(close) = lower[gt..].find("</title").map(|i| gt + i) else {
            break;
        };
        out.push_str(&html[at..gt]);
        out.push_str(
            &html[gt..close]
                .split_ascii_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
        );
        at = close;
    }
    out.push_str(&html[at..]);
    out
}

#[cfg(test)]
mod tests {
    use super::collapse_titles;

    #[test]
    fn titles_are_collapsed() {
        assert_eq!(
            collapse_titles("<head><title>\n   Snack  Diary ·\n Example\n </title></head>"),
            "<head><title>Snack Diary · Example</title></head>"
        );
        assert_eq!(
            collapse_titles(
                "<title lang=th> ไทย  ก </title><titles>x  y</titles><svg><title>a\nb</title></svg>"
            ),
            "<title lang=th>ไทย ก</title><titles>x  y</titles><svg><title>a b</title></svg>"
        );
        assert_eq!(collapse_titles("<title>unclosed  x"), "<title>unclosed  x");
    }
}
