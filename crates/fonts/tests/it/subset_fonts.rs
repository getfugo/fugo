//! A site's files written through the recorder, then its fonts cut down.

use std::sync::Arc;

use ssg_base::Sink;
use ssg_base::paths::OutputPath;
use ssg_fonts::{Recorder, subset_fonts};

use crate::support::{TestSink, load, mapped, mulish};

const PAGE: &str = r#"<html><head><style>.i::before{content:"\48"}</style></head>
<body><p>Text</p><i class="i"></i></body></html>"#;

/// Writes a page and fonts through a recorder (`icons/` and `text/` hold the same font);
/// `static/m.ttf` goes around it, as static files do.
fn site(text: bool) -> (Arc<TestSink>, Recorder) {
    let sink = Arc::new(TestSink::default());
    let recorder = Recorder::new(Arc::clone(&sink) as Arc<dyn Sink>, text);
    let write = |path: &str, bytes: &[u8]| {
        recorder
            .write(&OutputPath::new(path), bytes)
            .expect("write");
    };
    write("index.html", PAGE.as_bytes());
    write("icons/m.ttf", &mulish());
    write("text/m.ttf", &mulish());
    sink.write(&OutputPath::new("static/m.ttf"), &mulish())
        .expect("write");
    (sink, recorder)
}

#[test]
fn content_and_text_entries() {
    let config = ssg_fonts::settings(&load(
        r#"
[[fonts.subset]]
paths = ["icons/*", "static/*"]
from = "content"
[[fonts.subset]]
paths = ["text/*"]
keep = "!"
"#,
    ))
    .expect("settings")
    .expect("fonts");
    let (sink, recorder) = site(config.needs_text());
    let done =
        subset_fonts(&config, &recorder, &[OutputPath::new("static/m.ttf")]).expect("subset");
    assert!(done.warnings.is_empty(), "{:?}", done.warnings);
    let paths: Vec<&str> = done.cuts.iter().map(|c| c.path.as_str()).collect();
    assert_eq!(paths, ["/icons/m.ttf", "/static/m.ttf", "/text/m.ttf"]);
    for c in &done.cuts {
        assert_eq!(c.before, mulish().len());
        assert_eq!(c.after, sink.get(c.path.as_str()).len());
        assert!(c.after < c.before / 10, "{c:?}");
    }
    // The icon font keeps the character of the `content` string only.
    assert_eq!(mapped(&sink.get("icons/m.ttf"), "HText!"), "H");
    assert_eq!(mapped(&sink.get("static/m.ttf"), "HText!"), "H");
    // The text font keeps the page's text, the `content` string and `keep`.
    assert_eq!(mapped(&sink.get("text/m.ttf"), "HText!z"), "HText!");
}

#[test]
fn an_entry_that_covers_no_font() {
    let config = ssg_fonts::settings(&load(
        "[[fonts.subset]]\npaths = [\"fonts/*\"]\nfrom = \"content\"",
    ))
    .expect("settings")
    .expect("fonts");
    let (sink, recorder) = site(false);
    let done = subset_fonts(&config, &recorder, &[]).expect("subset");
    assert!(done.cuts.is_empty());
    assert_eq!(done.warnings.len(), 1);
    assert_eq!(
        done.warnings[0].message,
        "fonts.subset[0]: no published font matches fonts/*"
    );
    assert_eq!(sink.get("icons/m.ttf"), mulish());
}
