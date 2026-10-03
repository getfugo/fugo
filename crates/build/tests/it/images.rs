//! Phase E6 with images (REWRITE_PLAN.md §3.1, §3.4): only the image operations whose URL
//! appears in an output are processed and published (into `[caches.images]`, then the sink);
//! the bundle's own file is published eagerly; an operation that is computed but never printed
//! writes nothing.

use std::fs;

use ssg_build::{BuildRequest, SinkKind, build};
use ssg_config::CliOverrides;
use ssg_testkit::fixture::testdata;

use crate::support::{files_below, write_files};

const SINGLE: &str = r#"{% set pic = page.resources | get_resource(name="pic.jpg") -%}
{% set small = pic | resize(width=100) -%}
{% set unused = pic | resize(width=50) -%}
<img src="{{ small.rel_permalink }}" width="{{ small.width }}" height="{{ small.height }}">"#;

#[test]
fn referenced_images_are_processed_and_published() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let site = tmp.path().join("site");
    write_files(
        &site,
        &[
            (
                "config.toml".to_owned(),
                "baseURL = \"https://example.org/\"\ntitle = \"Images\"\n".to_owned(),
            ),
            ("layouts/single.html".to_owned(), SINGLE.to_owned()),
            ("layouts/list.html".to_owned(), "list".to_owned()),
            (
                "content/p/index.md".to_owned(),
                "---\ntitle: P\n---\n".to_owned(),
            ),
        ],
    );
    fs::copy(
        testdata("site-assets/generated/banner-600x200.jpg"),
        site.join("content/p/pic.jpg"),
    )
    .expect("copy the image");
    let report = build(BuildRequest {
        source: site,
        sink: SinkKind::Memory,
        cli: CliOverrides {
            cache_dir: Some(tmp.path().join("cache")),
            ..CliOverrides::default()
        },
        ..BuildRequest::default()
    })
    .unwrap_or_else(|e| panic!("images build: {e}"));
    let mem = report.memory.as_ref().expect("memory");
    let paths: Vec<String> = mem
        .paths()
        .iter()
        .map(|p| p.relative().to_owned())
        .collect();
    println!(
        "{paths:?} images {} resources {}",
        report.images, report.resources
    );
    let page = mem.text("p/index.html").expect("page");
    let processed: Vec<&String> = paths.iter().filter(|p| p.contains("_hu_")).collect();
    assert_eq!(processed.len(), 1, "only the printed operation: {paths:?}");
    assert!(
        page.contains(&format!("src=\"/{}\"", processed[0])),
        "{page}"
    );
    assert!(page.contains("width=\"100\" height=\"33\""), "{page}");
    assert!(
        paths.iter().any(|p| p == "p/pic.jpg"),
        "the bundle file is eager: {paths:?}"
    );
    assert_eq!((report.images, report.resources), (1, 1));
    let bytes = mem.get(processed[0]).expect("image");
    assert_eq!(&bytes[..2], &[0xFF, 0xD8], "a JPEG");
    // The processed image went through `[caches.images]` (`:resourceDir/_gen/images`).
    let cached = files_below(&tmp.path().join("site/resources/_gen/images"));
    assert_eq!(cached.len(), 1, "{cached:?}");
}
