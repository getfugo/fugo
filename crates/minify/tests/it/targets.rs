//! CSS with browser targets from a browserslist configuration: prefixes added and syntax lowered
//! for the targets, hand-written fallbacks kept as written.

use ssg_minify::{Minifier, MinifyTarget, project_browsers};

/// A minifier with the targets of `queries` (a `.browserslistrc`).
fn minifier(queries: &str) -> Minifier {
    let dir = tempfile::tempdir().expect("tmp");
    std::fs::write(dir.path().join(".browserslistrc"), queries).expect("write");
    let browsers = project_browsers(dir.path(), "production").expect("browserslist");
    assert!(browsers.is_some(), "no targets for {queries:?}");
    Minifier::default().with_browsers(browsers)
}

/// Minifies `input` and checks that the result is a fixed point.
fn min(m: &Minifier, input: &str) -> String {
    let out = m
        .minify(MinifyTarget::Css, input)
        .unwrap_or_else(|e| panic!("{e}: {input}"))
        .into_owned();
    let again = m
        .minify(MinifyTarget::Css, &out)
        .expect("again")
        .into_owned();
    assert_eq!(again, out, "not idempotent for {input:?}");
    out
}

#[test]
fn prefixes_for_the_targets() {
    let input = ".a{user-select:none;backdrop-filter:blur(2px)}";
    let out = min(&minifier("Safari >= 12\nFirefox >= 60\n"), input);
    for want in [
        "-webkit-user-select:none",
        "-moz-user-select:none",
        "user-select:none",
        "-webkit-backdrop-filter:blur(2px)",
        "backdrop-filter:blur(2px)",
    ] {
        assert!(out.contains(want), "{want} missing: {out}");
    }
    // Prefixes none of the targets needs go.
    let out = min(
        &minifier("Chrome >= 120\n"),
        ".r{-webkit-border-radius:2px;border-radius:2px}",
    );
    assert_eq!(out, ".r{border-radius:2px}");
    // Without targets nothing is added.
    let out = min(&Minifier::default(), input);
    assert_eq!(out, input);
}

#[test]
fn syntax_lowered_for_the_targets() {
    let out = min(&minifier("Safari >= 12\n"), ".o{inset:0}");
    assert!(
        out.contains("top:0") && out.contains("left:0") && !out.contains("inset"),
        "{out}"
    );
}

/// A rule declaring a property twice is kept as written, in a group rule too; the rules around it
/// still get their prefixes.
#[test]
fn fallback_rules_kept_as_written() {
    let m = minifier("Safari >= 12\n");
    let gradient = "background:radial-gradient(circle at 30% 107%,#fdf497 0%,#285aeb 90%);\
                    background:-webkit-radial-gradient(circle at 30% 107%,#fdf497 0%,#285aeb 90%)";
    for input in [
        format!(".i{{{gradient}}}.a{{user-select:none}}"),
        format!("@media (min-width:1px){{.i{{{gradient}}}}}.a{{user-select:none}}"),
    ] {
        let out = min(&m, &input);
        assert!(
            out.contains("background:radial-gradient(")
                && out.contains("background:-webkit-radial-gradient("),
            "fallback lost: {out}"
        );
        assert!(out.contains("-webkit-user-select:none"), "{out}");
    }
}
