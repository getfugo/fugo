//! `[fonts]`: a site's static font cut down to the characters its CSS draws, in memory and on
//! disk; a second build on disk starts again from the whole font (the static copy restores it).

use std::collections::BTreeSet;
use std::path::Path;

use ssg_build::{BuildRequest, SinkKind, build};
use ssg_fonts::Outcome;
use ssg_testkit::fixture::testdata_dir;

use crate::support::write_files;

fn font() -> Vec<u8> {
    std::fs::read(testdata_dir().join("legacy-docs/assets/opengraph/mulish-black.ttf"))
        .expect("mulish-black.ttf")
}

fn write_site(dir: &Path) {
    write_files(
        dir,
        &[
            (
                "config.toml".to_owned(),
                "baseURL = \"https://example.org/\"\n\
                 disableKinds = [\"taxonomy\", \"term\", \"rss\", \"sitemap\"]\n\
                 [[fonts.subset]]\npaths = [\"fonts/*\"]\nfrom = \"content\"\n"
                    .to_owned(),
            ),
            (
                "layouts/home.html".to_owned(),
                r#"<!DOCTYPE html><html><head><style>.i::before{content:"\48\69"}</style></head><body><p>Not with the icon font</p></body></html>"#
                    .to_owned(),
            ),
        ],
    );
    std::fs::create_dir_all(dir.join("static/fonts")).expect("mkdir");
    std::fs::write(dir.join("static/fonts/m.ttf"), font()).expect("font");
}

/// Whether the font `bytes` has a glyph for `c` (cutting it down to `c` finds it used).
fn has(bytes: &[u8], c: char) -> bool {
    let chars = BTreeSet::from([c]);
    !matches!(
        ssg_fonts::cut(bytes, &chars).expect("a font"),
        Outcome::Unused
    )
}

#[test]
fn a_static_font_cut_down_in_memory() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_site(dir.path());
    let r = build(BuildRequest {
        source: dir.path().to_path_buf(),
        sink: SinkKind::Memory,
        ..BuildRequest::default()
    })
    .expect("build");
    assert_eq!(r.fonts.len(), 1, "{:?}", r.diagnostics);
    let cut = &r.fonts[0];
    assert_eq!(cut.path.as_str(), "/fonts/m.ttf");
    assert_eq!(cut.before, font().len());
    assert!(cut.after < cut.before / 10, "{cut:?}");
    let bytes = r.memory.expect("memory").get("fonts/m.ttf").expect("font");
    assert_eq!(bytes.len(), cut.after);
    assert!(has(&bytes, 'H') && has(&bytes, 'i'));
    // The page's text is not drawn with an icon font (`from = "content"`).
    assert!(!has(&bytes, 'N'));
}

#[test]
fn a_rebuild_on_disk_cuts_the_whole_font_again() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_site(dir.path());
    let public = dir.path().join("public");
    let sizes: Vec<usize> = (0..2)
        .map(|_| {
            let r = build(BuildRequest {
                source: dir.path().to_path_buf(),
                destination: Some(public.clone()),
                sink: SinkKind::Disk,
                ..BuildRequest::default()
            })
            .expect("build");
            assert_eq!(r.fonts.len(), 1, "{:?}", r.diagnostics);
            assert_eq!(r.fonts[0].before, font().len(), "from the whole font");
            std::fs::read(public.join("fonts/m.ttf"))
                .expect("font")
                .len()
        })
        .collect();
    assert_eq!(sizes[0], sizes[1]);
    assert!(sizes[0] < font().len() / 10);
}
