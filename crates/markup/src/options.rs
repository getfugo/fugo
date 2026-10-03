//! Rendering options, and their derivation from `[markup]` (`ssg_config::MarkupConfig`).

use bitflags::bitflags;
use ssg_base::anchor;
use ssg_config::markup::{MarkupConfig, TocConfig};

bitflags! {
    /// Markdown extensions (goldmark's `[markup.goldmark.extensions]` and `parser` switches).
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct Extensions: u32 {
        const TABLES = 1;
        const FOOTNOTES = 1 << 1;
        const DEFINITION_LISTS = 1 << 2;
        /// `autoDefinitionTermID`: definition terms get ids like headings.
        const DEFINITION_TERM_IDS = 1 << 3;
        const STRIKETHROUGH = 1 << 4;
        const TASKLISTS = 1 << 5;
        const LINKIFY = 1 << 6;
        /// `parser.attribute.title`: `## Heading {#id .class}`.
        const HEADING_ATTRIBUTES = 1 << 7;
        /// `parser.attribute.block`: a `{.class}` line after a block.
        const BLOCK_ATTRIBUTES = 1 << 8;
        /// GitHub alerts (`> [!NOTE]`) are passed to the blockquote hook as alerts.
        const ALERTS = 1 << 9;
        /// `enableEmoji`: `:smile:` shortcodes.
        const EMOJI = 1 << 10;
    }
}

impl Default for Extensions {
    /// The Go implementation's defaults: everything but definition-term ids, block attributes
    /// and emoji.
    fn default() -> Self {
        Self::all() - Self::DEFINITION_TERM_IDS - Self::BLOCK_ATTRIBUTES - Self::EMOJI
    }
}

/// What happens to raw HTML in Markdown (goldmark `unsafe`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RawHtml {
    /// Replaced by `<!-- raw HTML omitted -->`; HTML comments are dropped.
    #[default]
    Omit,
    /// Written as is.
    Pass,
}

/// Code fences (`markup.highlight.codeFences`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CodeFences {
    /// The code-block hook runs; a fence it does not handle goes to the [`crate::Highlighter`].
    #[default]
    Hooked,
    /// `<pre><code class="language-x">`: no hook, no highlighter.
    Plain,
}

/// How a soft line break is written (`renderer.hardWraps`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LineBreaks {
    /// A newline.
    #[default]
    Soft,
    /// `<br>`.
    Hard,
}

/// Void-element syntax (`renderer.xhtml`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TagStyle {
    /// `<br>`, `<hr>`, `<img …>`.
    #[default]
    Html,
    /// `<br />`, `<hr />`, `<img … />`.
    Xhtml,
}

/// A paragraph holding only an image (`parser.wrapStandAloneImageWithinParagraph`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StandaloneImages {
    /// Stays in its `<p>`.
    #[default]
    InParagraph,
    /// Replaces its paragraph; the image hook sees `is_block = true`.
    Block,
}

/// The scheme linkify gives bare `www.` links (`extensions.linkifyProtocol`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LinkifyProtocol {
    Http,
    #[default]
    Https,
}

impl LinkifyProtocol {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
        }
    }
}

/// The typographer's replacements (goldmark: HTML written as is; empty disables one).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Typographer {
    pub left_single_quote: String,
    pub right_single_quote: String,
    pub left_double_quote: String,
    pub right_double_quote: String,
    pub en_dash: String,
    pub em_dash: String,
    pub ellipsis: String,
    pub left_angle_quote: String,
    pub right_angle_quote: String,
    pub apostrophe: String,
}

impl Default for Typographer {
    fn default() -> Self {
        Self::from(&ssg_config::markup::Typographer::default())
    }
}

impl From<&ssg_config::markup::Typographer> for Typographer {
    fn from(t: &ssg_config::markup::Typographer) -> Self {
        Self {
            left_single_quote: t.left_single_quote.clone(),
            right_single_quote: t.right_single_quote.clone(),
            left_double_quote: t.left_double_quote.clone(),
            right_double_quote: t.right_double_quote.clone(),
            en_dash: t.en_dash.clone(),
            em_dash: t.em_dash.clone(),
            ellipsis: t.ellipsis.clone(),
            left_angle_quote: t.left_angle_quote.clone(),
            right_angle_quote: t.right_angle_quote.clone(),
            apostrophe: t.apostrophe.clone(),
        }
    }
}

/// Opening and closing delimiters of a passthrough (math) element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delimiters {
    pub open: String,
    pub close: String,
    pub kind: crate::PassthroughKind,
}

/// The table of contents levels (`[markup.tableOfContents]`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TocOptions {
    pub start: u8,
    /// The deepest level shown; `None` shows all.
    pub end: Option<u8>,
    pub ordered: bool,
}

impl Default for TocOptions {
    fn default() -> Self {
        Self::from(&TocConfig::default())
    }
}

impl From<&TocConfig> for TocOptions {
    fn from(t: &TocConfig) -> Self {
        Self {
            start: t.start_level,
            end: t.end_level,
            ordered: t.ordered,
        }
    }
}

/// Everything that decides how Markdown becomes HTML.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownOptions {
    pub extensions: Extensions,
    pub raw_html: RawHtml,
    /// `None` when the typographer is disabled.
    pub typographer: Option<Typographer>,
    /// `None` when headings get no automatic id (`autoHeadingID = false`).
    pub heading_ids: Option<anchor::Style>,
    pub passthrough: Vec<Delimiters>,
    pub code_fences: CodeFences,
    pub toc: TocOptions,
    pub line_breaks: LineBreaks,
    pub tags: TagStyle,
    pub standalone_images: StandaloneImages,
    pub linkify_protocol: LinkifyProtocol,
    /// The footnote back-link HTML; `None` for goldmark's `&#x21a9;&#xfe0e;`.
    pub footnote_backlink: Option<String>,
}

impl Default for MarkdownOptions {
    fn default() -> Self {
        Self::from_config(&MarkupConfig::default(), false)
    }
}

impl MarkdownOptions {
    /// The options of a site's `[markup]` section; `emoji` is the site's `enableEmoji`.
    #[must_use]
    pub fn from_config(m: &MarkupConfig, emoji: bool) -> Self {
        let g = &m.goldmark;
        let e = &g.extensions;
        let mut ext = Extensions::ALERTS;
        ext.set(Extensions::TABLES, e.table);
        ext.set(Extensions::FOOTNOTES, e.footnote.enable);
        ext.set(Extensions::DEFINITION_LISTS, e.definition_list);
        ext.set(
            Extensions::DEFINITION_TERM_IDS,
            g.parser.auto_definition_term_id,
        );
        ext.set(Extensions::STRIKETHROUGH, e.strikethrough);
        ext.set(Extensions::TASKLISTS, e.task_list);
        ext.set(Extensions::LINKIFY, e.linkify);
        ext.set(Extensions::HEADING_ATTRIBUTES, g.parser.attribute.title);
        ext.set(Extensions::BLOCK_ATTRIBUTES, g.parser.attribute.block);
        ext.set(Extensions::EMOJI, emoji);
        let pt = &e.passthrough;
        let passthrough = if pt.enable {
            let pairs = |v: &[[String; 2]], kind| {
                v.iter()
                    .map(|[open, close]| Delimiters {
                        open: open.clone(),
                        close: close.clone(),
                        kind,
                    })
                    .collect::<Vec<_>>()
            };
            let mut d = pairs(&pt.delimiters.block, crate::PassthroughKind::Block);
            d.extend(pairs(&pt.delimiters.inline, crate::PassthroughKind::Inline));
            d
        } else {
            Vec::new()
        };
        Self {
            extensions: ext,
            raw_html: if g.renderer.unsafe_html {
                RawHtml::Pass
            } else {
                RawHtml::Omit
            },
            typographer: (!e.typographer.disable).then(|| Typographer::from(&e.typographer)),
            heading_ids: g
                .parser
                .auto_heading_id
                .then(|| anchor::Style::from(g.parser.auto_id_type)),
            passthrough,
            code_fences: if m.highlight.code_fences {
                CodeFences::Hooked
            } else {
                CodeFences::Plain
            },
            toc: TocOptions::from(&m.table_of_contents),
            line_breaks: if g.renderer.hard_wraps {
                LineBreaks::Hard
            } else {
                LineBreaks::Soft
            },
            tags: if g.renderer.xhtml {
                TagStyle::Xhtml
            } else {
                TagStyle::Html
            },
            standalone_images: if g.parser.wrap_standalone_image_within_paragraph {
                StandaloneImages::InParagraph
            } else {
                StandaloneImages::Block
            },
            linkify_protocol: if e.linkify_protocol.eq_ignore_ascii_case("http") {
                LinkifyProtocol::Http
            } else {
                LinkifyProtocol::Https
            },
            footnote_backlink: Some(e.footnote.backlink_html.clone()).filter(|s| !s.is_empty()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn go_defaults() {
        let o = MarkdownOptions::default();
        assert_eq!(
            o.extensions,
            Extensions::TABLES
                | Extensions::FOOTNOTES
                | Extensions::DEFINITION_LISTS
                | Extensions::STRIKETHROUGH
                | Extensions::TASKLISTS
                | Extensions::LINKIFY
                | Extensions::HEADING_ATTRIBUTES
                | Extensions::ALERTS
        );
        assert_eq!(o.extensions, Extensions::default());
        assert_eq!(o.raw_html, RawHtml::Omit);
        assert_eq!(o.typographer, Some(Typographer::default()));
        assert_eq!(o.heading_ids, Some(anchor::Style::Github));
        assert_eq!(o.code_fences, CodeFences::Hooked);
        assert_eq!(
            o.toc,
            TocOptions {
                start: 2,
                end: Some(3),
                ordered: false
            }
        );
        assert_eq!(o.linkify_protocol, LinkifyProtocol::Https);
        assert!(o.passthrough.is_empty());

        let mut m = MarkupConfig::default();
        m.goldmark.extensions.typographer.disable = true;
        m.goldmark.parser.auto_heading_id = false;
        m.goldmark.renderer.unsafe_html = true;
        m.highlight.code_fences = false;
        m.goldmark.extensions.passthrough.enable = true;
        m.goldmark.extensions.passthrough.delimiters.inline = vec![["\\(".into(), "\\)".into()]];
        let o = MarkdownOptions::from_config(&m, true);
        assert!(o.typographer.is_none() && o.heading_ids.is_none());
        assert_eq!(
            (o.raw_html, o.code_fences),
            (RawHtml::Pass, CodeFences::Plain)
        );
        assert!(o.extensions.contains(Extensions::EMOJI));
        assert_eq!(o.passthrough[0].kind, crate::PassthroughKind::Inline);
    }
}
