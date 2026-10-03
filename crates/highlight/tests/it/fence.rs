//! Code fences through ssg-markup's `Highlighter` seam: fence options, attributes on the
//! wrapper, the trailing newline Go's code block renderer adds, errors.

use std::path::Path;
use std::sync::Arc;

use ssg_base::PageId;
use ssg_config::markup::HighlightConfig;
use ssg_highlight::{Highlight, OptionsArg};
use ssg_markup::{ExpandedMarkdown, MarkdownOptions, NoHooks, SourceContexts, render};

fn classes() -> Highlight {
    Highlight::new(&HighlightConfig {
        no_classes: false,
        ..HighlightConfig::default()
    })
}

fn markdown(hl: &Highlight, md: &str) -> Result<String, String> {
    let contexts = SourceContexts::default();
    let file: Arc<Path> = Arc::from(Path::new("content/a.md"));
    let src = ExpandedMarkdown {
        text: md,
        page: PageId::from_raw(0),
        contexts: &contexts,
        file: &file,
    };
    render(&src, &MarkdownOptions::default(), &NoHooks, Some(hl))
        .map(|r| r.html)
        .map_err(|e| e.to_string())
}

#[test]
fn fence_options_and_attributes() {
    let hl = classes();
    let html = markdown(
        &hl,
        "```go {.wide id=\"main\" linenos=table hl_lines=[2] linenostart=5}\na := 1\nb := 2\n```\n",
    )
    .expect("render");
    assert!(
        html.starts_with("<div class=\"highlight wide\" id=\"main\"><div class=\"chroma\">\n<table class=\"lntable\">"),
        "{html}"
    );
    // hl_lines are relative to linenostart: the second line (number 6) is highlighted.
    assert!(
        html.contains(
            "<span class=\"lnt\">5\n</span><span class=\"hl\"><span class=\"lnt\">6\n</span></span>"
        ),
        "{html}"
    );
    assert!(
        html.contains("<span class=\"line hl\"><span class=\"cl\"><span class=\"nx\">b</span>"),
        "{html}"
    );
    // Go's code block renderer ends the code with a newline: every line keeps its `\n`.
    assert!(
        html.contains("2</span>\n</span></span></code></pre></td></tr></table>"),
        "{html}"
    );
}

#[test]
fn fence_equals_the_function_with_a_newline() {
    let hl = classes();
    let html = markdown(&hl, "```html\n<p>{{ .Title }}</p>\n```\n").expect("render");
    let direct = hl
        .highlight_with("<p>{{ .Title }}</p>\n", "html", OptionsArg::None)
        .expect("highlight");
    assert_eq!(html.trim_end(), direct);
}

#[test]
fn fence_line_anchors_are_numbered_per_code_block() {
    let hl = classes();
    let md = "```go {linenos=inline anchorlinenos=true}\na\n```\n\n\
              ```go {linenos=inline anchorlinenos=true}\nb\n```\n\n\
              ```go {linenos=inline anchorlinenos=true lineanchors=x}\nc\n```\n";
    let html = markdown(&hl, md).expect("render");
    // Go: `lineanchors` defaults to `hl-<ordinal>` for code blocks.
    for id in ["hl-0-1", "hl-1-1", "x-1"] {
        assert!(html.contains(&format!("id=\"{id}\"")), "{id}: {html}");
        assert!(html.contains(&format!("href=\"#{id}\"")), "{id}: {html}");
    }
    // The function has no ordinal: no prefix unless `lineanchors` is given.
    let direct = hl
        .highlight_with(
            "a\n",
            "go",
            OptionsArg::Str("linenos=inline,anchorlinenos=true"),
        )
        .expect("highlight");
    assert!(direct.contains("id=\"1\""), "{direct}");
}

#[test]
fn unknown_language_stays_plain() {
    let hl = classes();
    let html = markdown(&hl, "```nosuch\na < b\n```\n").expect("render");
    assert_eq!(
        html.trim_end(),
        "<pre tabindex=\"0\"><code class=\"language-nosuch\" data-lang=\"nosuch\">a &lt; b\n</code></pre>"
    );
}

#[test]
fn invalid_fence_option_is_an_error() {
    let hl = classes();
    let err = markdown(&hl, "```go {tabwidth=\"wide\"}\nx\n```\n").expect_err("invalid");
    assert!(err.contains("tabwidth"), "{err}");
}

#[test]
fn inline_styles_by_default() {
    let hl = Highlight::new(&HighlightConfig::default());
    let html = markdown(&hl, "```go\nx := 1\n```\n").expect("render");
    assert!(
        html.starts_with("<div class=\"highlight\"><pre tabindex=\"0\" style=\"color:#f8f8f2;background-color:#272822;"),
        "{html}"
    );
    assert!(!html.contains("class=\"chroma\""));
}
