//! Inline `<svg>` kept as written when SVG is disabled (`disableSVG`), as the Go implementation's
//! minifier writes it: each outermost `<svg>` element of a page is swapped for an empty
//! placeholder `<svg>` before the page is minified, and put back after. The placeholder is an
//! `<svg>` too, so that minify-html treats the whitespace around it as it would the element's.

use std::ops::Range;

use super::{TEXT_ELEMENTS, Tag};

/// A page with its inline `<svg>` elements held out.
pub(super) struct Held<'a> {
    /// The page with a placeholder for each element.
    pub(super) html: String,
    /// The attribute that marks a placeholder; the page has no attribute of that name.
    marker: String,
    /// The elements as written, in page order.
    svgs: Vec<&'a str>,
}

impl<'a> Held<'a> {
    /// `page` with its inline `<svg>` elements held out, or `None` when it has none.
    pub(super) fn new(page: &'a str) -> Option<Self> {
        let lower = page.to_ascii_lowercase();
        let spans = spans(page, &lower);
        if spans.is_empty() {
            return None;
        }
        let mut marker = String::from("data-fugo-svg");
        while lower.contains(&marker) {
            marker.push('x');
        }
        let mut html = String::with_capacity(page.len());
        let mut copied = 0;
        let mut svgs = Vec::with_capacity(spans.len());
        for (i, span) in spans.into_iter().enumerate() {
            html.push_str(&page[copied..span.start]);
            html.push_str(&placeholder(&marker, i));
            svgs.push(&page[span.clone()]);
            copied = span.end;
        }
        html.push_str(&page[copied..]);
        Some(Self { html, marker, svgs })
    }

    /// `minified` (minified from [`Held::html`]) with each placeholder replaced by its element.
    pub(super) fn restore(&self, minified: &str) -> String {
        let mut out = String::with_capacity(
            minified.len() + self.svgs.iter().map(|s| s.len()).sum::<usize>(),
        );
        let mut copied = 0;
        for (i, svg) in self.svgs.iter().enumerate() {
            let p = placeholder(&self.marker, i);
            // minify-html writes each placeholder as given, in page order.
            if let Some(at) = minified[copied..].find(&p).map(|j| copied + j) {
                out.push_str(&minified[copied..at]);
                out.push_str(svg);
                copied = at + p.len();
            }
        }
        out.push_str(&minified[copied..]);
        out
    }
}

/// The placeholder of the `i`th element, as minify-html writes it.
fn placeholder(marker: &str, i: usize) -> String {
    format!("<svg {marker}={i}></svg>")
}

/// The spans of the outermost `<svg>` elements of `html` (`lower` is its lowercase copy), from
/// the `<` of the start tag to the `>` of the end tag (or of a self-closing start tag). An
/// element with no end tag is left to minify-html, and so is text that only looks like one: in
/// a comment, a `<script>` or an element whose content is text.
fn spans(html: &str, lower: &str) -> Vec<Range<usize>> {
    let mut spans = Vec::new();
    let mut at = 0;
    // The start of the outermost open `<svg>`, and the number of open ones.
    let mut start = 0;
    let mut depth = 0_usize;
    while let Some(lt) = lower[at..].find('<').map(|i| at + i) {
        if lower[lt..].starts_with("<!--") {
            at = lower[lt..].find("-->").map_or(lower.len(), |i| lt + i + 3);
            continue;
        }
        // In SVG, a CDATA section is text.
        if depth > 0 && lower[lt..].starts_with("<![cdata[") {
            at = lower[lt..].find("]]>").map_or(lower.len(), |i| lt + i + 3);
            continue;
        }
        let Some(tag) = Tag::parse(html, lower, lt) else {
            at = lt + 1;
            continue;
        };
        at = tag.end;
        if tag.name == "svg" {
            if tag.closing {
                if depth == 1 {
                    spans.push(start..tag.end);
                }
                depth = depth.saturating_sub(1);
            } else if tag.self_closing {
                if depth == 0 {
                    spans.push(lt..tag.end);
                }
            } else {
                if depth == 0 {
                    start = lt;
                }
                depth += 1;
            }
        } else if depth == 0
            && !tag.closing
            && (tag.name == "script" || TEXT_ELEMENTS.contains(&tag.name))
        {
            at = lower[at..]
                .find(&format!("</{}", tag.name))
                .map_or(lower.len(), |i| at + i);
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::{Held, spans};

    fn found(html: &str) -> Vec<&str> {
        spans(html, &html.to_ascii_lowercase())
            .into_iter()
            .map(|s| &html[s])
            .collect()
    }

    #[test]
    fn outermost_elements() {
        assert_eq!(
            found("<p>a <SVG x=1> <svg><g/></svg> </Svg> b <svg/></p><svg >"),
            ["<SVG x=1> <svg><g/></svg> </Svg>", "<svg/>"]
        );
        assert_eq!(
            found("<svg><!-- </svg> --><![CDATA[ </svg> ]]></svg>"),
            ["<svg><!-- </svg> --><![CDATA[ </svg> ]]></svg>"]
        );
        assert!(
            found("<!-- <svg></svg> --><script>'<svg></svg>'</script><title><svg></svg></title>")
                .is_empty()
        );
        assert!(found("</svg><p>no svg</p>").is_empty());
    }

    #[test]
    fn marker_is_not_in_the_page() {
        let page = "<p data-fugo-svg=1></p><svg> </svg>";
        let held = Held::new(page).unwrap();
        assert_eq!(
            held.html,
            "<p data-fugo-svg=1></p><svg data-fugo-svgx=0></svg>"
        );
        assert_eq!(held.restore(&held.html), page);
        assert!(Held::new("<p>none</p>").is_none());
    }
}
