use serde::Deserialize;
use ssg_testkit::fixture::{oracle, repo_file};

use super::*;

#[derive(Deserialize)]
struct Planes {
    cases: Vec<PlanesCase>,
}

#[derive(Deserialize)]
struct PlanesCase {
    src: String,
    #[serde(default)]
    fnv: Vec<String>,
    error: Option<String>,
}

fn fnv1a64(data: &[u8]) -> String {
    let h = data.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    format!("{h:016x}")
}

/// Go 1.25's decoding of every JPEG of the repository
/// (`testdata/oracle/images/smartcrop/jpeg.json.gz`, the FNV-1a hash of each plane as Go
/// allocates it, or Go's error): all equal, byte for byte.
#[test]
fn planes_equal_go_s() {
    let fx: Planes = oracle("oracle/images/smartcrop/jpeg.json.gz");
    assert_eq!(fx.cases.len(), 107);
    let mut failures = Vec::new();
    for c in &fx.cases {
        let path = repo_file(&c.src);
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let (got, err) = match decode(&bytes) {
            Err(e) => (Vec::new(), Some(e.to_string())),
            Ok(GoJpeg::Gray { pix, .. }) => (vec![fnv1a64(&pix)], None),
            Ok(GoJpeg::YCbCr { planes, .. }) => (
                vec![fnv1a64(&planes.y), fnv1a64(&planes.cb), fnv1a64(&planes.cr)],
                None,
            ),
            Ok(GoJpeg::Rgba { pix, .. } | GoJpeg::Cmyk { pix, .. }) => (vec![fnv1a64(&pix)], None),
        };
        if got != c.fnv || err != c.error {
            failures.push(format!(
                "{}: Go {:?} {:?}, here {got:?} {err:?}",
                c.src, c.fnv, c.error
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn idct_of_a_dc_block_is_flat() {
    let mut b = [0; BLOCK_SIZE];
    b[0] = 80;
    idct(&mut b);
    // A DC of 80 is a mean of 80/8 = 10 after the level shift is removed.
    assert!(b.iter().all(|&v| v == 10), "{b:?}");
}

#[test]
fn ycbcr_to_rgb_is_go_s() {
    assert_eq!(ycbcr_to_rgb(0, 128, 128), [0, 0, 0]);
    assert_eq!(ycbcr_to_rgb(255, 128, 128), [255, 255, 255]);
    assert_eq!(ycbcr_to_rgb(100, 30, 220), [229, 68, 0]);
}

#[test]
fn rejects_what_go_rejects() {
    assert_eq!(
        decode(b"\xff\xd9").err(),
        Some(JpegError::Format("missing SOI marker"))
    );
    assert_eq!(
        decode(b"\xff\xd8\xff").err(),
        Some(JpegError::UnexpectedEof)
    );
    assert_eq!(
        decode(b"\xff\xd8\xff\xd9").err(),
        Some(JpegError::Format("missing SOS marker"))
    );
}
