//! T22 acceptance (REWRITE_PLAN.md §8.2): `ssg-markup` against the Go oracles of
//! `testdata/oracle/markup/{convert,hooks}` (the Go implementation's goldmark setup on the docs
//! corpus and adversarial documents).
//!
//! Each test prints its table (`cargo test -p ssg-markup --test it acceptance --
//! --nocapture`) and asserts floors at the measured values.

mod compat;
mod context;
mod deep;
mod docs;
mod fences;
mod hooks;
mod passes;

use std::path::Path;
use std::sync::{Arc, LazyLock};

use serde::Deserialize;
use ssg_base::PageId;
use ssg_base::anchor::Style;
use ssg_markup::{
    CodeFences, ExpandedMarkdown, Extensions, Hooks, LineBreaks, LinkifyProtocol, MarkdownOptions,
    NoHooks, RawHtml, RenderedMarkdown, SourceContexts, StandaloneImages, TagStyle, TocOptions,
    Typographer, render,
};
use ssg_testkit::fixture::{GoString, oracle};

pub use super::comrak_spike::corpus::GoCfg;

/// The options of an oracle configuration (the TOML of `convert.json.gz` `configs`).
pub fn options(cfg: GoCfg) -> MarkdownOptions {
    let mut o = MarkdownOptions::default();
    match cfg {
        GoCfg::Default => {}
        GoCfg::Site => o.raw_html = RawHtml::Pass,
        GoCfg::Ascii => {
            o.raw_html = RawHtml::Pass;
            o.heading_ids = Some(Style::GithubAscii);
            o.extensions |= Extensions::DEFINITION_TERM_IDS | Extensions::BLOCK_ATTRIBUTES;
            o.standalone_images = StandaloneImages::Block;
            o.toc = TocOptions {
                start: 1,
                end: Some(4),
                ordered: true,
            };
        }
        GoCfg::Blackfriday => {
            o.tags = TagStyle::Xhtml;
            o.line_breaks = LineBreaks::Hard;
            o.heading_ids = Some(Style::Blackfriday);
            o.typographer = None;
            o.linkify_protocol = LinkifyProtocol::Http;
            o.toc = TocOptions {
                start: 3,
                end: None,
                ordered: false,
            };
        }
        GoCfg::Cjk => {
            o.code_fences = CodeFences::Plain;
            o.raw_html = RawHtml::Pass;
            o.heading_ids = None;
            o.extensions = Extensions::ALERTS;
            o.typographer = Some(Typographer {
                left_double_quote: "«".into(),
                right_double_quote: "»".into(),
                apostrophe: String::new(),
                ellipsis: "…".into(),
                ..Typographer::default()
            });
        }
        GoCfg::Noattr => {
            o.heading_ids = None;
            o.extensions |= Extensions::DEFINITION_TERM_IDS | Extensions::BLOCK_ATTRIBUTES;
            o.extensions -= Extensions::HEADING_ATTRIBUTES;
        }
    }
    o
}

pub const PAGE: PageId = PageId::from_raw(0);

pub fn file() -> Arc<Path> {
    Arc::from(Path::new("content/doc.md"))
}

/// Renders with `hooks`, no highlighter, no context spans.
pub fn render_with(md: &str, o: &MarkdownOptions, hooks: &dyn Hooks) -> RenderedMarkdown {
    let contexts = SourceContexts::default();
    let file = file();
    let src = ExpandedMarkdown {
        text: md,
        page: PAGE,
        contexts: &contexts,
        file: &file,
    };
    render(&src, o, hooks, None).unwrap_or_else(|e| panic!("render: {e}"))
}

/// The parse-only fragments.
pub fn fragments_of(md: &str, o: &MarkdownOptions) -> ssg_markup::Fragments {
    let contexts = SourceContexts::default();
    let file = file();
    let src = ExpandedMarkdown {
        text: md,
        page: PAGE,
        contexts: &contexts,
        file: &file,
    };
    ssg_markup::fragments(&src, o).unwrap_or_else(|e| panic!("fragments: {e}"))
}

/// Renders without hooks.
pub fn html(md: &str, o: &MarkdownOptions) -> String {
    render_with(md, o, &NoHooks).html
}

/// A row of a printed acceptance table.
pub struct Row {
    pub what: String,
    pub matched: usize,
    pub total: usize,
}

impl Row {
    pub fn new(what: impl Into<String>, matched: usize, total: usize) -> Self {
        Self {
            what: what.into(),
            matched,
            total,
        }
    }
}

pub fn print(title: &str, rows: &[Row]) {
    println!("\n## {title}\n\n| what | matched / total |\n|---|---|");
    for r in rows {
        println!("| {} | {} / {} |", r.what, r.matched, r.total);
    }
}

pub fn text(s: &GoString) -> String {
    String::from_utf8_lossy(&s.0).into_owned()
}

/// `FUGO_T22_SHOW=<label substring>` prints differences of that row.
pub fn show(label: &str, name: &str, want: &str, got: &str) {
    let Ok(filter) = std::env::var("FUGO_T22_SHOW") else {
        return;
    };
    if filter.is_empty() || !label.contains(&filter) || want == got {
        return;
    }
    let at = want
        .char_indices()
        .zip(got.chars())
        .find(|((_, a), b)| a != b)
        .map_or(want.len().min(got.len()), |((i, _), _)| i);
    let cut = |s: &str| -> String {
        let mut start = at.saturating_sub(80).min(s.len());
        while !s.is_char_boundary(start) {
            start -= 1;
        }
        s[start..].chars().take(240).collect()
    };
    eprintln!(
        "--- {label} | {name}\n  want: {:?}\n  got:  {:?}",
        cut(want),
        cut(got)
    );
}

// ───────────────────────────── the convert oracle ─────────────────────────────

#[derive(Deserialize)]
struct ConvertDoc {
    name: String,
    src: GoString,
}

#[derive(Deserialize, Clone)]
pub struct TocHeading {
    pub id: GoString,
    pub level: i64,
    pub title: GoString,
    pub headings: Vec<TocHeading>,
}

#[derive(Deserialize, Clone)]
pub struct TocRecord {
    pub present: bool,
    pub headings: Option<Vec<TocHeading>>,
    pub identifiers: Option<Vec<GoString>>,
    pub html: Option<Vec<GoString>>,
}

#[derive(Deserialize)]
struct ConvertResult {
    doc: usize,
    cfg: usize,
    html: Option<GoString>,
    html_same: Option<usize>,
    toc: Option<TocRecord>,
    toc_same: Option<usize>,
}

#[derive(Deserialize)]
struct Convert {
    docs: Vec<ConvertDoc>,
    results: Vec<ConvertResult>,
}

/// One document of the convert oracle with the Go implementation's HTML and TOC per
/// configuration.
pub struct ConvertCase {
    pub name: String,
    pub md: String,
    pub html: [Option<String>; 6],
    pub toc: [Option<TocRecord>; 6],
}

pub static CONVERT: LazyLock<Vec<ConvertCase>> = LazyLock::new(|| {
    let c: Convert = oracle("oracle/markup/convert/convert.json.gz");
    let mut cases: Vec<ConvertCase> = c
        .docs
        .iter()
        .map(|d| ConvertCase {
            name: d.name.clone(),
            md: text(&d.src),
            html: Default::default(),
            toc: Default::default(),
        })
        .collect();
    let mut same = Vec::new();
    for r in &c.results {
        if let Some(h) = &r.html {
            cases[r.doc].html[r.cfg] = Some(text(h));
        }
        if let Some(t) = &r.toc
            && r.toc_same.is_none()
        {
            cases[r.doc].toc[r.cfg] = Some(t.clone());
        }
        same.push((r.doc, r.cfg, r.html_same, r.toc_same));
    }
    for (doc, cfg, h, t) in same {
        if let Some(k) = h {
            cases[doc].html[cfg] = cases[doc].html[k].clone();
        }
        if let Some(k) = t {
            cases[doc].toc[cfg] = cases[doc].toc[k].clone();
        }
    }
    cases
});
