//! Phase E6 with images (REWRITE_PLAN.md §3.1, §3.4): only the image operations whose URL
//! appears in an output are processed and published (into `[caches.images]`, then the sink);
//! the bundle's own file is published eagerly; an operation that is computed but never printed
//! writes nothing.

use std::fs;

use ssg_build::{BuildRequest, SinkKind, build};
use ssg_config::CliOverrides;
use ssg_testkit::fixture::testdata;

use crate::support::{files_below, write_files};

mod credits;

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

/// `[imaging] stripMetadata`, on by default: the bundle's photo and the static one are
/// published without their EXIF (GPS position included), while `exif` still reads the source.
#[test]
fn originals_without_their_metadata() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let site = tmp.path().join("site");
    write_files(
        &site,
        &[
            (
                "config.toml".to_owned(),
                "baseURL = \"https://example.org/\"\ntitle = \"Images\"\n".to_owned(),
            ),
            (
                "layouts/single.html".to_owned(),
                "{% set pic = page.resources | get_resource(name=\"pic.jpg\") -%}\
                 {% set x = pic | exif %}{{ x.lat > 0 }}"
                    .to_owned(),
            ),
            ("layouts/list.html".to_owned(), "list".to_owned()),
            (
                "content/p/index.md".to_owned(),
                "---\ntitle: P\n---\n".to_owned(),
            ),
        ],
    );
    let photo = testdata("upstream/resources/testdata/sunset.jpg");
    fs::copy(&photo, site.join("content/p/pic.jpg")).expect("copy the image");
    fs::create_dir_all(site.join("static")).expect("static");
    fs::copy(&photo, site.join("static/pic.jpg")).expect("copy the image");
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
    assert_eq!(mem.text("p/index.html").as_deref(), Some("true"));
    let all = ssg_images::ExifSettings {
        include: None,
        exclude: None,
        ..ssg_images::ExifSettings::default()
    };
    let source = fs::read(&photo).expect("read");
    assert!(ssg_images::exif::read(&source, &all).is_some_and(|e| e.lat_long.is_some()));
    for path in ["p/pic.jpg", "pic.jpg"] {
        let bytes = mem.get(path).unwrap_or_else(|| panic!("{path}"));
        assert!(ssg_images::exif::read(&bytes, &all).is_none(), "{path}");
        assert!(bytes.len() < source.len(), "{path}");
    }
}

/// `[imaging] copyright`, `author` and `titleFromFileName`: the bundle's photo, the static one
/// and the processed one carry them (the processed one its source's title), the copyright with
/// the year of `--clock` in the site's time zone for `{year}`.
#[test]
fn published_images_carry_the_notice() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let site = tmp.path().join("site");
    write_files(
        &site,
        &[
            (
                "config.toml".to_owned(),
                "baseURL = \"https://example.org/\"\ntitle = \"Images\"\n\
                 timeZone = \"Asia/Bangkok\"\n[imaging]\n\
                 copyright = \"© 2019–{year} by A. Author\"\nauthor = \"A. Author\"\n\
                 titleFromFileName = true\n"
                    .to_owned(),
            ),
            ("layouts/single.html".to_owned(), SINGLE.to_owned()),
            ("layouts/list.html".to_owned(), "list".to_owned()),
            (
                "content/p/index.md".to_owned(),
                "---\ntitle: P\n---\n".to_owned(),
            ),
        ],
    );
    let photo = testdata("upstream/resources/testdata/sunset.jpg");
    fs::copy(&photo, site.join("content/p/pic.jpg")).expect("copy the image");
    fs::create_dir_all(site.join("static")).expect("static");
    fs::copy(&photo, site.join("static/view.jpg")).expect("copy the image");
    let report = build(BuildRequest {
        source: site,
        sink: SinkKind::Memory,
        cli: CliOverrides {
            cache_dir: Some(tmp.path().join("cache")),
            ..CliOverrides::default()
        },
        // 2032-01-01T00:30 in Bangkok.
        clock: Some("2031-12-31T17:30:00Z".parse().expect("clock")),
        ..BuildRequest::default()
    })
    .unwrap_or_else(|e| panic!("images build: {e}"));
    let mem = report.memory.as_ref().expect("memory");
    let processed = mem
        .paths()
        .iter()
        .map(|p| p.relative().to_owned())
        .find(|p| p.contains("_hu_"))
        .expect("a processed image");
    let all = ssg_images::ExifSettings {
        include: None,
        exclude: None,
        ..ssg_images::ExifSettings::default()
    };
    for (path, title) in [
        ("p/pic.jpg", "pic"),
        ("view.jpg", "view"),
        (&processed, "pic"),
    ] {
        let bytes = mem.get(path).unwrap_or_else(|| panic!("{path}"));
        let e = ssg_images::exif::read(&bytes, &all).unwrap_or_else(|| panic!("{path}: no EXIF"));
        let tag = |t: &str| {
            e.tags
                .get(t)
                .and_then(ssg_base::Value::as_str)
                .map(str::to_owned)
        };
        assert_eq!(
            tag("Copyright").as_deref(),
            Some("© 2019–2032 by A. Author"),
            "{path}"
        );
        assert_eq!(tag("Artist").as_deref(), Some("A. Author"), "{path}");
        assert_eq!(e.lat_long, None, "{path}");
        let xmp_title = format!("<dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">{title}<");
        assert!(
            bytes
                .windows(xmp_title.len())
                .any(|w| w == xmp_title.as_bytes()),
            "{path}: title {title}"
        );
    }
}
