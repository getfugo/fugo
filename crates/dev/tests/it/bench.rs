//! `cargo dev bench`: the parts that need no fugo binary.

use ssg_dev::bench::{Templates, generate, median, peak_rss, thousands};

#[test]
fn reports_and_numbers() {
    assert_eq!(peak_rss("x\nmaxrss-kib 2048\n"), Some(2.0));
    assert_eq!(
        peak_rss("        1.00 real\n   3145728  maximum resident set size\n"),
        Some(3.0)
    );
    assert_eq!(peak_rss("nothing"), None);
    assert!((median(vec![3.0, 1.0, 2.0]) - 2.0).abs() < f64::EPSILON);
    assert!((median(vec![4.0, 1.0, 2.0, 3.0]) - 2.5).abs() < f64::EPSILON);
    assert_eq!(thousands(10_000), "10,000");
    assert_eq!(thousands(999), "999");
}

#[test]
fn generated_sites_are_the_same_every_time() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (a, b) = (tmp.path().join("a"), tmp.path().join("b"));
    generate(&a, 25, Templates::Tera).expect("generate a");
    generate(&b, 25, Templates::Tera).expect("generate b");
    let page = "content/section-3/page-13.md";
    let text = std::fs::read_to_string(a.join(page)).expect("page");
    assert_eq!(text, std::fs::read_to_string(b.join(page)).expect("page"));
    assert!(text.starts_with("---\ntitle: \"Page 13: "), "{text}");
    assert!(text.contains("```rust\n"));
    let pages = walkdir::WalkDir::new(a.join("content"))
        .into_iter()
        .filter(|e| e.as_ref().is_ok_and(|e| e.file_type().is_file()))
        .count();
    assert_eq!(pages, 25 + 10 + 1, "pages, section pages and the home page");
    assert!(
        generate(&a, 1, Templates::Tera).is_err(),
        "the directory must not exist"
    );
}

#[test]
fn go_sites_differ_only_in_their_layouts() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (tera, go) = (tmp.path().join("tera"), tmp.path().join("go"));
    generate(&tera, 12, Templates::Tera).expect("tera");
    generate(&go, 12, Templates::Go).expect("go");
    for file in [
        "config.toml",
        "content/_index.md",
        "content/section-1/page-11.md",
    ] {
        let read = |dir: &std::path::Path| std::fs::read_to_string(dir.join(file)).expect(file);
        assert_eq!(read(&tera), read(&go), "{file}");
    }
    let layout = |dir: &std::path::Path| {
        std::fs::read_to_string(dir.join("layouts/list.html")).expect("list.html")
    };
    assert!(layout(&tera).contains("{% extends \"baseof.html\" %}"));
    assert!(layout(&go).contains("{{ define \"main\" }}"));
}
