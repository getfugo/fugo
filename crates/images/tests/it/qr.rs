//! QR codes against the Go implementation's: the golden images of
//! `tpl/images/images_integration_test.go` (`TestImagesGoldenFuncs`) byte for byte, and the content
//! hashes `TestQR` asserts.

use ssg_images::{QrLevel, qr_modules, qr_png};
use ssg_testkit::fixture::repo_file;
use xxhash_rust::xxh64::xxh64;

/// The text the Go implementation's QR tests encode (the recorded golden images and hashes are
/// of it).
const GO_TEST_URL: &str = "https://gohugo.io";

/// `images.QR GO_TEST_URL` with the options of `TestImagesGoldenFuncs`.
#[test]
fn equal_to_the_go_golden_images() {
    let dir = repo_file("tpl/images/testdata/images_golden/funcs");
    for (golden, level, scale) in [
        ("qr-default.png", QrLevel::Medium, 4),
        ("qr-level-high_scale-6.png", QrLevel::High, 6),
    ] {
        let ours = qr_png(GO_TEST_URL, level, scale).expect("qr");
        let go = std::fs::read(dir.join(golden)).expect("golden");
        assert!(
            ours == go,
            "{golden}: {} bytes, Go wrote {}",
            ours.len(),
            go.len()
        );
    }
}

/// `TestQR`: `{{ .Content | hash.XxHash }}` of `images.QR GO_TEST_URL` per option map
/// (level, scale).
#[test]
fn equal_to_the_content_hashes_of_the_go_test() {
    for (level, scale, hash) in [
        (QrLevel::Medium, 4, "6ccacf8056c41475"),
        (QrLevel::Low, 2, "c29338c3d105b156"),
        (QrLevel::Medium, 3, "8f7a639cea917b0e"),
        (QrLevel::Quartile, 5, "2d15d6dcb861b5da"),
        (QrLevel::High, 6, "113c45f2c091bc4d"),
    ] {
        let png = qr_png(GO_TEST_URL, level, scale).expect("qr");
        assert_eq!(
            format!("{:016x}", xxh64(&png, 0)),
            hash,
            "{level} scale {scale}"
        );
    }
}

/// The sizes the Go implementation's `qr` shortcode test prints: `(modules + 8) · scale`.
#[test]
fn sizes() {
    let side =
        |text: &str, level, scale: usize| (qr_modules(text, level).expect("qr").size + 8) * scale;
    assert_eq!(side(GO_TEST_URL, QrLevel::High, 4), 148);
    assert_eq!(side(&format!("{GO_TEST_URL}\""), QrLevel::Medium, 4), 132);
    let png = qr_png(GO_TEST_URL, QrLevel::High, 4).expect("qr");
    let (size, format) = ssg_images::probe(&png, "qr").expect("probe");
    assert_eq!(size, (148, 148));
    assert_eq!(format, ssg_images::ImageFormat::Png);
}

#[test]
fn deterministic_and_checked() {
    let a = qr_png("Hello, 世界", QrLevel::Quartile, 3).expect("qr");
    assert_eq!(a, qr_png("Hello, 世界", QrLevel::Quartile, 3).expect("qr"));
    assert!(qr_png("", QrLevel::Low, 4).is_err(), "empty text");
    assert!(qr_png("x", QrLevel::Low, 1).is_err(), "scale below 2");
    assert!(
        qr_png(&"x".repeat(4000), QrLevel::Low, 2).is_err(),
        "too long for version 40"
    );
    assert_eq!("MEDIUM".parse::<QrLevel>(), Ok(QrLevel::Medium));
    assert!("huge".parse::<QrLevel>().is_err());
}
