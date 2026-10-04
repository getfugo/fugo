//! `cargo dev file-length`: which files the 500-line limit applies to, and how lines are counted.

use ssg_dev::file_length::{is_checked, line_count};

#[test]
fn applies_to_code_but_not_to_generated_vendored_or_recorded_files() {
    assert!(is_checked("crates/site/src/lib.rs"));
    assert!(is_checked("crates/cms/web/assets/cms.ts"));
    assert!(is_checked("tools/dev/compare.sh"));
    assert!(is_checked("crates/highlight/src/chroma/golexers/lisp.rs"));
    assert!(!is_checked("README.md"));
    assert!(!is_checked("crates/cms/assets/admin/cms.js"));
    assert!(!is_checked("crates/funcs/assets/katex/katex.min.js"));
    assert!(!is_checked(
        "crates/jsbuild/tests/it/fixtures/decorator-tests.ts"
    ));
    assert!(!is_checked("testdata/legacy-docs/assets/main.js"));
    assert!(!is_checked("crates/highlight/src/chroma/lexers/elixir.rs"));
    assert!(!is_checked(
        "crates/highlight/src/chroma/golexers/exported/haxe.rs"
    ));
    assert!(!is_checked("crates/jsbuild/src/lower/es5/tests/table.rs"));
}

#[test]
fn counts_lines_like_wc_with_an_unterminated_last_line() {
    assert_eq!(line_count(b""), 0);
    assert_eq!(line_count(b"a\nb\n"), 2);
    assert_eq!(line_count(b"a\nb"), 2);
}
