//! `cargo dev bench`: the parts that need no fugo binary.

use ssg_dev::bench::{generate, median, peak_rss, thousands};

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
    generate(&a, 25).expect("generate a");
    generate(&b, 25).expect("generate b");
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
    assert!(generate(&a, 1).is_err(), "the directory must not exist");
}
