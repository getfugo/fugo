//! `strip_metadata`: original JPEG, PNG and WebP files without EXIF, XMP, IPTC, comments and
//! text, with the same pixels, colour profile and orientation; `rewrite_metadata` (`notice`):
//! with a title, an author and a copyright notice instead; an image's own (`credit`).

mod credit;
mod notice;

use serde_json::json;
use ssg_config::ImagingConfig;
use ssg_images::{ExifSettings, Imaging, exif, strip_metadata};
use ssg_testkit::fixture::repo_file;

fn read(rel: &str) -> Vec<u8> {
    std::fs::read(repo_file(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

/// The EXIF reader keeping every tag.
fn all_tags() -> ExifSettings {
    let cfg: ImagingConfig =
        serde_json::from_value(json!({"exif": {"includeFields": ".*"}})).expect("imaging config");
    Imaging::from_config(&cfg, 2026).expect("imaging").exif
}

fn contains(bytes: &[u8], needle: &[u8]) -> bool {
    bytes.windows(needle.len()).any(|w| w == needle)
}

fn pixels(bytes: &[u8]) -> Vec<u8> {
    image::load_from_memory(bytes)
        .expect("decodes")
        .to_rgba8()
        .into_raw()
}

/// The markers of a JPEG's segments up to the first scan.
fn jpeg_markers(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 2;
    while b[i] == 0xFF && b[i + 1] != 0xDA {
        out.push(b[i + 1]);
        i += 2 + usize::from(u16::from_be_bytes([b[i + 2], b[i + 3]]));
    }
    out
}

/// The chunk types of a PNG or a WebP file.
fn chunks(b: &[u8]) -> Vec<String> {
    let png = b.starts_with(b"\x89PNG");
    let mut out = Vec::new();
    let mut i = if png { 8 } else { 12 };
    while i + 8 <= b.len() {
        let (kind, len) = if png {
            (
                &b[i + 4..i + 8],
                u32::from_be_bytes(b[i..i + 4].try_into().unwrap()),
            )
        } else {
            (
                &b[i..i + 4],
                u32::from_le_bytes(b[i + 4..i + 8].try_into().unwrap()),
            )
        };
        out.push(String::from_utf8_lossy(kind).into_owned());
        let len = len as usize;
        i += if png { 12 + len } else { 8 + len + (len & 1) };
    }
    out
}

/// A PNG chunk.
fn png_chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = u32::try_from(data.len()).unwrap().to_be_bytes().to_vec();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = crc32fast::Hasher::new();
    crc.update(kind);
    crc.update(data);
    out.extend_from_slice(&crc.finalize().to_be_bytes());
    out
}

/// The TIFF data of a JPEG's first EXIF segment.
fn exif_block(jpeg: &[u8]) -> Vec<u8> {
    let at = jpeg
        .windows(6)
        .position(|w| w == b"Exif\0\0")
        .expect("an EXIF segment");
    let len = usize::from(u16::from_be_bytes([jpeg[at - 2], jpeg[at - 1]]));
    jpeg[at + 6..at - 2 + len].to_vec()
}

#[test]
fn jpeg_keeps_the_image_and_its_colour_profile() {
    let original = read("resources/testdata/sunset.jpg");
    assert!(exif::read(&original, &all_tags()).is_some_and(|e| e.lat_long.is_some()));
    let stripped = strip_metadata(&original).expect("stripped");
    // EXIF (with GPS), Photoshop (IPTC) and XMP are gone; the ICC profile, the quantization
    // tables, the restart interval, Adobe's colour transform and the frame stay.
    assert_eq!(
        jpeg_markers(&original),
        [0xE1, 0xED, 0xE2, 0xE1, 0xDB, 0xDD, 0xEE, 0xC0, 0xC4]
    );
    assert_eq!(
        jpeg_markers(&stripped),
        [0xE2, 0xDB, 0xDD, 0xEE, 0xC0, 0xC4]
    );
    assert!(exif::read(&stripped, &all_tags()).is_none());
    assert!(!contains(&stripped, b"http://ns.adobe.com/xap/1.0/"));
    assert!(contains(&stripped, b"ICC_PROFILE\0"));
    assert!(stripped.ends_with(&[0xFF, 0xD9]));
    assert_eq!(pixels(&stripped), pixels(&original));
    assert_eq!(strip_metadata(&stripped), None, "nothing left to strip");
}

#[test]
fn jpeg_keeps_only_the_orientation_of_its_exif() {
    let mut original = read("resources/testdata/exif/orientation6.jpg");
    // A trailer after the end of the image (as phones append videos and other images).
    original.extend_from_slice(b"appended data");
    let stripped = strip_metadata(&original).expect("stripped");
    assert_eq!(exif::orientation(&stripped), Some(6));
    let tags = exif::read(&stripped, &all_tags()).expect("exif").tags;
    assert_eq!(tags.keys().collect::<Vec<_>>(), ["Orientation"]);
    // The comment (COM) is gone, the JFIF header stays, first.
    assert_eq!(jpeg_markers(&original)[..3], [0xE1, 0xE0, 0xFE]);
    assert_eq!(jpeg_markers(&stripped)[..3], [0xE0, 0xE1, 0xDB]);
    assert!(stripped.ends_with(&[0xFF, 0xD9]), "the trailer is gone");
    assert_eq!(pixels(&stripped), pixels(&original));
    assert_eq!(strip_metadata(&stripped), None);
}

#[test]
fn progressive_jpeg_keeps_every_scan() {
    let original = read("resources/testdata/iss8079.jpg");
    let stripped = strip_metadata(&original).expect("stripped");
    assert_eq!(
        jpeg_markers(&stripped)[..2],
        [0xE0, 0xDB],
        "JFIF, then the tables"
    );
    assert!(!contains(&stripped, b"Photoshop 3.0"));
    assert!(!contains(&stripped, b"http://ns.adobe.com/xap/1.0/"));
    assert_eq!(pixels(&stripped), pixels(&original));
}

#[test]
fn png_keeps_its_critical_and_colour_chunks() {
    let original = read("resources/testdata/gohugoio.png");
    // EXIF with a GPS position, a text chunk and a time, before the image data.
    let sunset = read("resources/testdata/sunset.jpg");
    let iend = original.len() - 12;
    let first_idat = original
        .windows(4)
        .position(|w| w == b"IDAT")
        .expect("IDAT")
        - 4;
    let mut with_exif = original[..first_idat].to_vec();
    with_exif.extend(png_chunk(b"eXIf", &exif_block(&sunset)));
    with_exif.extend(png_chunk(b"tEXt", b"Author\0someone"));
    with_exif.extend(png_chunk(b"tIME", &[7, 226, 10, 27, 12, 0, 0]));
    with_exif.extend_from_slice(&original[first_idat..iend]);
    with_exif.extend(png_chunk(b"zTXt", b"Comment\0\0x"));
    with_exif.extend_from_slice(&original[iend..]);
    assert!(exif::read(&with_exif, &all_tags()).is_some_and(|e| e.lat_long.is_some()));

    let stripped = strip_metadata(&with_exif).expect("stripped");
    let kinds = chunks(&stripped);
    assert_eq!(kinds[..4], ["IHDR", "gAMA", "sRGB", "PLTE"]);
    assert!(kinds[4..kinds.len() - 1].iter().all(|k| k == "IDAT"));
    assert_eq!(kinds.last().map(String::as_str), Some("IEND"));
    assert!(exif::read(&stripped, &all_tags()).is_none());
    assert_eq!(pixels(&stripped), pixels(&original));
    assert_eq!(strip_metadata(&stripped), None);
    assert!(chunks(&original).contains(&"iTXt".to_owned()));
    assert_eq!(strip_metadata(&original).map(|b| chunks(&b)), Some(kinds));
}

#[test]
fn png_keeps_only_the_orientation_of_its_exif() {
    let original = read("resources/testdata/gohugoio.png");
    let rotated = read("resources/testdata/exif/orientation6.jpg");
    let mut with_exif = original[..33].to_vec();
    with_exif.extend(png_chunk(b"eXIf", &exif_block(&rotated)));
    with_exif.extend_from_slice(&original[33..]);
    let stripped = strip_metadata(&with_exif).expect("stripped");
    assert_eq!(chunks(&stripped)[1], "eXIf");
    assert_eq!(exif::orientation(&stripped), Some(6));
    let tags = exif::read(&stripped, &all_tags()).expect("exif").tags;
    assert_eq!(tags.keys().collect::<Vec<_>>(), ["Orientation"]);
    let image = image::load_from_memory(&stripped).expect("a valid PNG (CRCs included)");
    assert_eq!(image.to_rgba8().into_raw(), pixels(&original));
}

#[test]
fn webp_keeps_its_image_and_colour_profile() {
    let original = read("resources/testdata/sunrise.webp");
    assert_eq!(chunks(&original), ["VP8X", "ICCP", "VP8 ", "EXIF", "XMP "]);
    let stripped = strip_metadata(&original).expect("stripped");
    assert_eq!(chunks(&stripped), ["VP8X", "ICCP", "VP8 "]);
    // VP8X announces the ICC profile only now; the RIFF size is the file's.
    assert_eq!(stripped[20] & 0x2C, 0x20);
    let riff_size = u32::from_le_bytes(stripped[4..8].try_into().unwrap()) as usize;
    assert_eq!(riff_size + 8, stripped.len());
    assert!(exif::read(&stripped, &all_tags()).is_none());
    assert_eq!(pixels(&stripped), pixels(&original));
    assert_eq!(strip_metadata(&stripped), None);
}

#[test]
fn webp_keeps_only_the_orientation_of_its_exif() {
    let original = read("resources/testdata/sunrise.webp");
    let rotated = read("resources/testdata/exif/orientation6.jpg");
    let block = exif_block(&rotated);
    let mut with_exif = original[..12].to_vec();
    for (kind, data) in [
        (b"VP8X", {
            // No ICC profile in this file.
            let mut data = original[20..30].to_vec();
            data[0] &= !0x20;
            data
        }),
        (b"VP8 ", {
            let at = original.windows(4).position(|w| w == b"VP8 ").unwrap();
            let len = u32::from_le_bytes(original[at + 4..at + 8].try_into().unwrap()) as usize;
            original[at + 8..at + 8 + len].to_vec()
        }),
        (b"EXIF", block),
    ] {
        with_exif.extend_from_slice(kind);
        with_exif.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
        with_exif.extend_from_slice(&data);
        if data.len() % 2 == 1 {
            with_exif.push(0);
        }
    }
    let riff_size = u32::try_from(with_exif.len() - 8).unwrap();
    with_exif[4..8].copy_from_slice(&riff_size.to_le_bytes());
    let stripped = strip_metadata(&with_exif).expect("stripped");
    assert_eq!(chunks(&stripped), ["VP8X", "VP8 ", "EXIF"]);
    assert_eq!(stripped[20] & 0x0C, 0x08, "VP8X announces EXIF, not XMP");
    assert_eq!(exif::orientation(&stripped), Some(6));
    let tags = exif::read(&stripped, &all_tags()).expect("exif").tags;
    assert_eq!(tags.keys().collect::<Vec<_>>(), ["Orientation"]);
}

#[test]
fn other_files_are_left_alone() {
    for rel in [
        "resources/testdata/giphy.gif",
        "resources/testdata/sunset.webp",
        "resources/testdata/circle.svg",
    ] {
        assert_eq!(strip_metadata(&read(rel)), None, "{rel}");
    }
    assert_eq!(strip_metadata(b""), None);
    assert_eq!(strip_metadata(b"RIFF"), None);
}

/// Every JPEG, PNG and WebP file of the test data decodes to the same pixels after stripping
/// (those that decode), and stripping it again changes nothing.
#[test]
fn test_data_images_keep_their_pixels() {
    let root = ssg_testkit::fixture::testdata_dir();
    let mut stack = vec![root.clone()];
    let (mut stripped, mut compared) = (0, 0);
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
            let Some(out) = strip_metadata(&bytes) else {
                continue;
            };
            stripped += 1;
            let name = path.strip_prefix(&root).unwrap().display().to_string();
            assert_eq!(strip_metadata(&out), None, "{name}: stripped twice");
            if let Ok(want) = image::load_from_memory(&bytes) {
                let got = image::load_from_memory(&out)
                    .unwrap_or_else(|e| panic!("{name}: no longer decodes: {e}"));
                assert!(got.to_rgba8() == want.to_rgba8(), "{name}: other pixels");
                compared += 1;
            }
        }
    }
    eprintln!("stripped {stripped} images, {compared} compared");
    assert!(compared >= 50, "only {compared} images compared");
}
