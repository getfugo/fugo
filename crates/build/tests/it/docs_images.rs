//! The docs layouts that draw text on images and make QR codes, unpatched (the files of
//! `sites/docs/layouts`, not the `patches/` variants), built on a small docs-shaped site:
//!
//! - `_partials/opengraph/get-featured-image.html`: a bundle's `*cover*` image, else the
//!   site card with the page's link title drawn on it (`{"op": "text"}` with the Mulish Black
//!   font resource, size 80 or 70 for long titles);
//! - `_partials/layouts/header/qr.html`: the `qr_img` component (`qr_code`) inside the `modal`
//!   component;
//! - `_partials/layouts/hooks/body-main-start.html`: the print QR code of sections and pages.

use std::fs;

use ssg_build::{BuildRequest, SinkKind, build};
use ssg_resources::{QrOptions, qr_target};
use ssg_testkit::fixture::repo_dir;

use crate::support::write_files;

const LAYOUT: &str = r#"{%- set featured = partial(name="opengraph/get-featured-image.html") -%}
<meta property="og:image" content="{{ featured.permalink }}" data-size="{{ featured.width }}x{{ featured.height }}">
{% include "_partials/layouts/header/qr.html" %}
{% include "_partials/layouts/hooks/body-main-start.html" %}
<main>{{ page.content }}</main>"#;

/// The docs layout files the test renders, from `sites/docs/layouts`.
const DOCS_LAYOUTS: &[&str] = &[
    "_partials/opengraph/get-featured-image.html",
    "_partials/layouts/header/qr.html",
    "_partials/layouts/hooks/body-main-start.html",
    "_partials/layouts/blocks/modal.html",
];

/// The site card's file name (without `.png`) in the legacy docs site's `assets/opengraph`,
/// as the docs layouts name it.
const CARD: &str = "gohugoio-card-base-1";

/// `(width, height)` from a PNG's IHDR chunk.
fn png_size(bytes: &[u8]) -> (u32, u32) {
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "a PNG");
    let be = |i: usize| u32::from_be_bytes(bytes[i..i + 4].try_into().expect("4 bytes"));
    (be(16), be(20))
}

/// The value of attribute `name` in the first tag of `html` that has it after `from`.
fn attr<'a>(html: &'a str, from: &str, name: &str) -> &'a str {
    let start = html
        .find(from)
        .unwrap_or_else(|| panic!("no {from} in {html}"));
    let rest = &html[start..];
    let at = rest
        .find(&format!("{name}=\""))
        .unwrap_or_else(|| panic!("no {name} after {from}"));
    let value = &rest[at + name.len() + 2..];
    &value[..value.find('"').expect("closing quote")]
}

#[test]
fn docs_text_and_qr_layouts_render() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path().join("docs");
    let mut files: Vec<(String, String)> = [
        (
            "config.toml",
            "baseURL = \"https://example.org/\"\ntitle = \"Docs\"\ndisableKinds = [\"taxonomy\", \"term\", \"rss\", \"sitemap\"]\n",
        ),
        ("layouts/single.html", LAYOUT),
        ("layouts/list.html", LAYOUT),
        ("layouts/home.html", LAYOUT),
        ("content/_index.md", "---\ntitle: Home\n---\n"),
        ("content/guide.md", "---\ntitle: Guide\n---\nGuide.\n"),
        (
            "content/functions/_index.md",
            "---\ntitle: Functions\n---\n",
        ),
        (
            "content/functions/long.md",
            "---\ntitle: A title longer than twenty characters\n---\n",
        ),
        ("content/news/bundle/index.md", "---\ntitle: Bundle\n---\n"),
    ]
    .into_iter()
    .map(|(p, c)| (p.to_owned(), c.to_owned()))
    .collect();
    let docs = repo_dir().join("sites/docs/layouts");
    for rel in DOCS_LAYOUTS {
        let text = fs::read_to_string(docs.join(rel)).expect("docs layout");
        files.push((format!("layouts/{rel}"), text));
    }
    write_files(&dir, &files);
    let opengraph = ssg_testkit::fixture::legacy_docs().join("assets/opengraph");
    fs::create_dir_all(dir.join("assets/opengraph")).expect("mkdir");
    let card_file = format!("{CARD}.png");
    for name in [card_file.as_str(), "mulish-black.ttf"] {
        fs::copy(
            opengraph.join(name),
            dir.join("assets/opengraph").join(name),
        )
        .expect("copy");
    }
    fs::copy(
        opengraph.join(&card_file),
        dir.join("content/news/bundle/cover.png"),
    )
    .expect("copy cover");

    let report = build(BuildRequest {
        source: dir,
        sink: SinkKind::Memory,
        clock: Some("2026-09-27T12:00:00Z".parse().expect("clock")),
        ..BuildRequest::default()
    })
    .unwrap_or_else(|e| panic!("docs-like build: {e}"));
    let mem = report.memory.as_ref().expect("memory");
    let text = |p: &str| mem.text(p).unwrap_or_else(|| panic!("no {p}"));
    let card = fs::read(opengraph.join(&card_file)).expect("card");
    let processed = format!("opengraph/{CARD}_hu_");
    let card_size = png_size(&card);

    // The site card with the link title drawn on it, published; another size of text (70)
    // for long titles gives another image.
    let featured = |page: &str| {
        let html = text(page);
        let url = attr(&html, "og:image", "content").to_owned();
        let path = url
            .strip_prefix("https://example.org/")
            .unwrap_or_else(|| panic!("{url}"))
            .to_owned();
        let size = attr(&html, "og:image", "data-size").to_owned();
        (path, size)
    };
    let (guide, guide_size) = featured("guide/index.html");
    assert!(
        guide.starts_with(&processed) && guide.ends_with(".png"),
        "{guide}"
    );
    assert_eq!(guide_size, format!("{}x{}", card_size.0, card_size.1));
    let drawn = mem.get(&guide).expect("the processed card is published");
    assert_eq!(png_size(&drawn), card_size);
    assert_ne!(&*drawn, &card[..], "text drawn on the card");
    let (long, _) = featured("functions/long/index.html");
    assert!(long.starts_with(&processed), "{long}");
    assert_ne!(long, guide);
    assert!(mem.get(&long).is_some());
    // A bundle's `*cover*` image is used as it is.
    let (bundle, _) = featured("news/bundle/index.html");
    assert_eq!(bundle, "news/bundle/cover.png");

    // QR codes of the page's permalink in `images/qr`, named as Go names them: the header's
    // (in the modal, twice) and the print one of pages and sections.
    for (page, permalink, print) in [
        ("guide/index.html", "https://example.org/guide/", true),
        (
            "functions/index.html",
            "https://example.org/functions/",
            true,
        ),
        ("index.html", "https://example.org/", false),
    ] {
        let html = text(page);
        let options = QrOptions {
            target_dir: "images/qr".to_owned(),
            ..QrOptions::default()
        };
        let target = qr_target(permalink, &options);
        assert!(
            target.starts_with("/images/qr/qr_") && target.ends_with(".png"),
            "{target}"
        );
        let qr = mem
            .get(target.trim_start_matches('/'))
            .unwrap_or_else(|| panic!("{page}: {target} is not published"));
        let (w, h) = png_size(&qr);
        let header = format!("src=\"{target}\"\n      width=\"{w}\"\n      height=\"{h}\"");
        assert_eq!(html.matches(&header).count(), 2, "{page}: {html}");
        let print_img = format!(
            "<img class=\"mb-2 -mr-2\" src=\"{target}\" width=\"{}\" height=\"{}\"",
            w * 3 / 4,
            h * 3 / 4
        );
        assert_eq!(html.contains(&print_img), print, "{page}: {html}");
    }
}
