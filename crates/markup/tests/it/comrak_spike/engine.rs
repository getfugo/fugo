//! comrak option sets that mirror each oracle configuration as closely as comrak allows.
//!
//! Native only: no pass of ours runs here, so every difference the report shows is either
//! a comrak gap (a custom pass for T22) or a renderer difference the normaliser folds.

use comrak::nodes::NodeValue;
use comrak::{Arena, Options, parse_document};

use super::corpus::GoCfg;

/// The Go implementation's goldmark defaults: table, strikethrough, linkify, task list,
/// typographer, definition list, footnote, `attribute.title`; raw HTML omitted.
fn go_defaults() -> Options<'static> {
    let mut o = Options::default();
    o.extension.table = true;
    o.extension.strikethrough = true;
    o.extension.autolink = true;
    o.extension.tasklist = true;
    o.extension.description_lists = true;
    o.extension.footnotes = true;
    o.extension.header_attributes = true;
    o.parse.smart = true;
    o
}

pub fn go_options(cfg: GoCfg) -> Options<'static> {
    let mut o = go_defaults();
    match cfg {
        GoCfg::Default => {}
        GoCfg::Site | GoCfg::Ascii => o.render.r#unsafe = true,
        GoCfg::Blackfriday => {
            o.render.hardbreaks = true;
            o.parse.smart = false;
        }
        GoCfg::Cjk => {
            o.render.r#unsafe = true;
            o.extension.header_attributes = false;
            o.extension.table = false;
            o.extension.strikethrough = false;
            o.extension.autolink = false;
            o.extension.tasklist = false;
            o.extension.description_lists = false;
            o.extension.footnotes = false;
        }
        GoCfg::Noattr => o.extension.header_attributes = false,
    }
    o
}

pub fn to_html(md: &str, o: &Options<'_>) -> String {
    comrak::markdown_to_html(md, o)
}

/// [`to_html`] plus the one trivial pass prototyped here: Go drops HTML comments (block and
/// inline) instead of writing `<!-- raw HTML omitted -->` when `unsafe = false`.
pub fn to_html_passes(md: &str, o: &Options<'_>) -> String {
    let arena = Arena::new();
    let root = parse_document(&arena, md, o);
    if !o.render.r#unsafe {
        let comments: Vec<_> = root
            .descendants()
            .filter(|n| match &n.data().value {
                NodeValue::HtmlBlock(b) => b.literal.starts_with("<!--"),
                NodeValue::HtmlInline(h) => h.starts_with("<!--"),
                _ => false,
            })
            .collect();
        for n in comments {
            n.detach();
        }
    }
    let mut html = String::new();
    comrak::format_html(root, o, &mut html).expect("fmt::Write to String");
    html
}
