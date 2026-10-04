//! The glob cases of `tests/glob-cases.json`, which the editor's `compileGlob`
//! (`web/assets/common.ts`) is tested on too: the Worker must match paths exactly as
//! `ssg_base::glob` does.

use ssg_base::glob::{self, Case, GlobOpts, Separator};

#[test]
fn rust_globs_match_the_shared_cases() {
    let cases: Vec<(String, String, bool)> =
        serde_json::from_str(include_str!("../glob-cases.json")).expect("cases");
    assert!(cases.len() > 30);
    for (pattern, path, want) in cases {
        let g = glob::compile(
            &pattern,
            GlobOpts {
                case: Case::Sensitive,
                separator: Separator::Slash,
            },
        )
        .expect(&pattern);
        assert_eq!(g.is_match(&path), want, "{pattern:?} on {path:?}");
    }
}
