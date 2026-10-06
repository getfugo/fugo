//! `[fonts]` decoding and checks.

use ssg_fonts::{FontsConfig, Source};

use crate::support::load;

fn settings(toml: &str) -> Result<Option<FontsConfig>, String> {
    ssg_fonts::settings(&load(toml)).map_err(|e| e.to_string())
}

#[test]
fn nothing_to_do_without_entries() {
    assert!(settings("title = \"x\"").expect("ok").is_none());
    assert!(settings("[fonts]").expect("ok").is_none());
}

#[test]
fn entries_their_sources_and_globs() {
    let c = settings(
        r#"
[[fonts.subset]]
paths = ["/assets/webfonts/*"]
from = "Content"
keep = "→"
[[fonts.subset]]
paths = ["fonts/**.woff2"]
"#,
    )
    .expect("ok")
    .expect("fonts");
    assert_eq!(c.rules.len(), 2);
    assert_eq!(c.rules[0].from, Source::Content);
    assert_eq!(c.rules[0].keep, "→");
    assert_eq!(c.rules[1].from, Source::Text);
    assert!(c.needs_text());
    assert!(c.rules[0].matches("/assets/webfonts/fa-solid-900.woff2"));
    assert!(c.rules[0].matches("assets/webfonts/fa-solid-900.ttf"));
    // `*` stops at `/`; `**` does not.
    assert!(!c.rules[0].matches("assets/webfonts/v4/fa.woff2"));
    assert!(c.rules[1].matches("fonts/inter/inter.woff2"));
    assert!(!c.rules[1].matches("fonts/inter/inter.ttf"));
}

#[test]
fn wrong_settings() {
    let e = settings("[[fonts.subset]]\nfrom = \"text\"").expect_err("no paths");
    assert_eq!(e, "fonts.subset[0].paths: no paths");
    let e = settings("[[fonts.subset]]\npaths = [\"a/*\"]\nfrom = \"glyphs\"").expect_err("from");
    assert_eq!(
        e,
        "fonts.subset[0].from: \"glyphs\" is not one of text, content"
    );
    let e = settings("[[fonts.subset]]\npaths = [\"a/*\"]\nchars = \"x\"").expect_err("key");
    assert!(e.starts_with("fonts.subset"), "{e}");
    assert!(e.contains("chars"), "{e}");
}
