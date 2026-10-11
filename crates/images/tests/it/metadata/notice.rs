//! `rewrite_metadata` with a notice: the title, the author and the copyright in EXIF (`Artist`,
//! `Copyright`) and XMP (`dc:title`, `dc:creator`, `dc:rights`), in JPEG, PNG and WebP files
//! (simple WebP files get the extended format's header), with the same pixels.

use ssg_base::Value;
use ssg_images::{Notice, rewrite_metadata};

use super::*;

const NOTICE: Notice<'static> = Notice {
    title: "sunset",
    author: "A. Author & Co",
    copyright: "© 2026 by A. Author, all rights reserved",
};

/// The notice is in the EXIF and XMP of `bytes`; no GPS position is.
fn assert_notice(bytes: &[u8], what: &str) {
    let e = exif::read(bytes, &all_tags()).unwrap_or_else(|| panic!("{what}: no EXIF"));
    assert_eq!(e.lat_long, None, "{what}");
    let text = |tag: &str| e.tags.get(tag).and_then(Value::as_str).map(str::to_owned);
    assert_eq!(text("Artist").as_deref(), Some(NOTICE.author), "{what}");
    assert_eq!(
        text("Copyright").as_deref(),
        Some(NOTICE.copyright),
        "{what}"
    );
    for xmp in [
        "<dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">sunset</rdf:li>",
        "<dc:creator><rdf:Seq><rdf:li>A. Author &amp; Co</rdf:li>",
        "<dc:rights><rdf:Alt><rdf:li xml:lang=\"x-default\">© 2026 by A. Author, all rights reserved</rdf:li>",
    ] {
        assert!(contains(bytes, xmp.as_bytes()), "{what}: {xmp}");
    }
    assert_eq!(
        rewrite_metadata(bytes, &NOTICE),
        None,
        "{what}: written twice"
    );
}

#[test]
fn jpeg_gets_the_notice_first() {
    let original = read("resources/testdata/sunset.jpg");
    let out = rewrite_metadata(&original, &NOTICE).expect("rewritten");
    // The notice's EXIF and XMP first, then the ICC profile and the image.
    assert_eq!(
        jpeg_markers(&out),
        [0xE1, 0xE1, 0xE2, 0xDB, 0xDD, 0xEE, 0xC0, 0xC4]
    );
    assert_notice(&out, "sunset.jpg");
    assert_eq!(pixels(&out), pixels(&original));

    // After a JFIF header, with the orientation.
    let rotated = read("resources/testdata/exif/orientation6.jpg");
    let out = rewrite_metadata(&rotated, &NOTICE).expect("rewritten");
    assert_eq!(jpeg_markers(&out)[..4], [0xE0, 0xE1, 0xE1, 0xDB]);
    assert_eq!(exif::orientation(&out), Some(6));
    assert_notice(&out, "orientation6.jpg");
}

#[test]
fn png_gets_the_notice_after_its_header() {
    let original = read("resources/testdata/gohugoio.png");
    let out = rewrite_metadata(&original, &NOTICE).expect("rewritten");
    assert_eq!(
        chunks(&out)[..6],
        ["IHDR", "eXIf", "iTXt", "gAMA", "sRGB", "PLTE"]
    );
    assert_notice(&out, "gohugoio.png");
    assert_eq!(pixels(&out), pixels(&original));
}

#[test]
fn simple_webp_gets_the_extended_format() {
    // Lossy, without alpha.
    let original = read("resources/testdata/sunset.webp");
    assert_eq!(chunks(&original), ["VP8 "]);
    let out = rewrite_metadata(&original, &NOTICE).expect("rewritten");
    assert_eq!(chunks(&out), ["VP8X", "VP8 ", "EXIF", "XMP "]);
    // EXIF and XMP, no alpha; the canvas is the image's size.
    assert_eq!(out[20], 0x0C);
    let image = image::load_from_memory(&original).expect("decodes");
    assert_eq!(
        (canvas_of(&out, 24), canvas_of(&out, 27)),
        (image.width(), image.height())
    );
    let riff_size = u32::from_le_bytes(out[4..8].try_into().unwrap()) as usize;
    assert_eq!(riff_size + 8, out.len());
    assert_notice(&out, "sunset.webp");
    assert_eq!(pixels(&out), pixels(&original));

    // Lossless, with alpha.
    let rgba: Vec<u8> = (0..32u32 * 24)
        .flat_map(|i| [(i % 256) as u8, (i / 3 % 256) as u8, 90, (i % 7 * 36) as u8])
        .collect();
    let original = webp::Encoder::from_rgba(&rgba, 32, 24)
        .encode_lossless()
        .to_vec();
    assert_eq!(chunks(&original), ["VP8L"]);
    let out = rewrite_metadata(&original, &NOTICE).expect("rewritten");
    assert_eq!(chunks(&out), ["VP8X", "VP8L", "EXIF", "XMP "]);
    assert_eq!(out[20], 0x1C, "alpha, EXIF and XMP");
    assert_eq!((canvas_of(&out, 24), canvas_of(&out, 27)), (32, 24));
    assert_notice(&out, "lossless.webp");
    // libwebp clears the colour of transparent pixels: the original's, not `rgba`.
    assert_eq!(pixels(&out), pixels(&original));
}

/// A canvas dimension of `VP8X` (24 bits, minus one) at `at`.
fn canvas_of(webp: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([webp[at], webp[at + 1], webp[at + 2], 0]) + 1
}

#[test]
fn extended_webp_keeps_its_profile() {
    let original = read("resources/testdata/sunrise.webp");
    let out = rewrite_metadata(&original, &NOTICE).expect("rewritten");
    assert_eq!(chunks(&out), ["VP8X", "ICCP", "VP8 ", "EXIF", "XMP "]);
    assert_eq!(out[20] & 0x2C, 0x2C, "ICC profile, EXIF and XMP");
    assert_notice(&out, "sunrise.webp");
    assert_eq!(pixels(&out), pixels(&original));
}

/// Every JPEG, PNG and WebP file of the test data, with the notice, decodes to the same pixels
/// (those that decode) and carries it.
#[test]
fn test_data_images_take_the_notice() {
    let root = ssg_testkit::fixture::testdata_dir();
    let mut stack = vec![root.clone()];
    let mut compared = 0;
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read_dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                if !path.ends_with("golden") {
                    stack.push(path);
                }
                continue;
            }
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase);
            if !matches!(ext.as_deref(), Some("jpg" | "jpeg" | "png" | "webp")) {
                continue;
            }
            let bytes = std::fs::read(&path).expect("read");
            let Ok(want) = image::load_from_memory(&bytes) else {
                continue;
            };
            let name = path.strip_prefix(&root).unwrap().display().to_string();
            let Some(out) = rewrite_metadata(&bytes, &NOTICE) else {
                panic!("{name}: no notice written");
            };
            let got = image::load_from_memory(&out)
                .unwrap_or_else(|e| panic!("{name}: no longer decodes: {e}"));
            assert!(got.to_rgba8() == want.to_rgba8(), "{name}: other pixels");
            assert!(
                contains(&out, NOTICE.copyright.as_bytes()),
                "{name}: no copyright"
            );
            assert_eq!(
                rewrite_metadata(&out, &NOTICE),
                None,
                "{name}: written twice"
            );
            compared += 1;
        }
    }
    eprintln!("{compared} images with the notice");
    assert!(compared >= 200, "only {compared} images");
}
