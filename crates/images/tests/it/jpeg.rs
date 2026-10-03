//! The JPEG encoder (`ssg_images::jpeg`, Go's `image/jpeg` writer) against bytes Go wrote
//! for exactly known pixels (`oracle/images/process`, Go 1.27.1), and properties of its output.

use std::collections::BTreeMap;

use serde_json::Value as J;
use ssg_images::jpeg::{self, Pixels};
use ssg_images::{ImageInput, ImageQueue, ImageSpec, Imaging};
use ssg_testkit::fixture::oracle;

use crate::common::{decode, expected_diffs, pix, psnr, sha256_hex, synth, write_file};

/// The oracle cases whose input pixels are known exactly: the synthetic JPEG sources (Go's
/// `jpeg.Encode` of `*image.YCbCr` planes at q90), `d:jpg` of the synthetic opaque PNG sources
/// (the decoded pixels unchanged, q75), and JPEG results of the 1×1 PNG sources (a uniform
/// colour at any size) at q1, q75, q90 and q100. `(case, our bytes, Go's length, Go's SHA-256)`.
fn exact_cases() -> Vec<(String, Vec<u8>, u64, String)> {
    let doc: J = oracle("oracle/images/process/process.json.gz");
    let dir = tempfile::tempdir().expect("tempdir");
    let q = ImageQueue::new(Imaging::default(), None);
    let mut paths = BTreeMap::new();
    let mut out = Vec::new();
    for c in doc["cases"].as_array().expect("cases") {
        let src = c["src"].as_str().expect("src");
        let op = c["op"].as_str().expect("op");
        let Some(rest) = src.strip_prefix("gen:") else {
            continue;
        };
        let kind = rest.split(':').next().expect("kind");
        if op == "decode" && src.ends_with(":jpg") {
            let ours = synth(src).expect("synthetic source");
            let (len, sha) = (c["res"]["len"].as_u64(), c["srcsha"].as_str());
            out.push((
                src.to_owned(),
                ours,
                len.expect("len"),
                sha.expect("sha").to_owned(),
            ));
            continue;
        }
        let uniform = src.contains(":1x1:") && op.contains(" jpg");
        let identity = op == "d:jpg";
        if !src.ends_with(":png") || !["gray", "rgba", "nrgba"].contains(&kind) {
            continue;
        }
        if !((uniform && op.starts_with("d:")) || identity) {
            continue;
        }
        let path = paths
            .entry(src.to_owned())
            .or_insert_with(|| {
                let name = format!("s{}.png", paths_len_hint(src));
                write_file(dir.path(), &name, &synth(src).expect("synth"))
            })
            .clone();
        let spec: ImageSpec = op.trim_start_matches("d:").parse().expect("spec");
        let e = q
            .enqueue(&ImageInput::File(path), Some(&spec), &[])
            .expect("enqueue");
        let ours = q.encoded(e.id).expect("encode");
        let (len, sha) = (c["res"]["len"].as_u64(), c["res"]["sha"].as_str());
        out.push((
            format!("{src} {op}"),
            ours.to_vec(),
            len.expect("len"),
            sha.expect("sha").to_owned(),
        ));
    }
    out
}

/// A file name for a source id (its characters are not all valid in file names).
fn paths_len_hint(src: &str) -> String {
    sha256_hex(src.as_bytes())[..16].to_owned()
}

/// Go's bytes, except the cases of `expected_diffs.toml [jpeg]` (the DCT of Go 1.26+ rounds a
/// rare coefficient the other way): those must still differ (the list stays exact) and be
/// within 0.1 % of Go's size.
#[test]
fn bytes_equal_go() {
    let accepted = expected_diffs("jpeg");
    let cases = exact_cases();
    let mut equal = 0;
    let mut failures = Vec::new();
    for (id, ours, len, sha) in &cases {
        let same = sha256_hex(ours) == *sha;
        let listed = accepted.contains_key(id);
        let size = ours.len() as f64 / *len as f64;
        match (same, listed) {
            (true, false) => equal += 1,
            (false, true) if (size - 1.0).abs() <= 0.001 => {}
            (true, true) => {
                failures.push(format!("{id}: equals Go's bytes, remove it from [jpeg]"));
            }
            _ => failures.push(format!("{id}: {} bytes, Go {len}", ours.len())),
        }
    }
    eprintln!(
        "JPEG bytes: {equal}/{} cases equal Go's, {} accepted differences",
        cases.len(),
        accepted.len()
    );
    assert!(cases.len() >= 40, "only {} cases", cases.len());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The oracle's synthetic image `kind` of `w`×`h` as RGB and luma samples.
fn synthetic(kind: &str, w: u32, h: u32) -> (Vec<u8>, Vec<u8>) {
    let seed = crate::common::kind_seed(kind)
        .wrapping_add(u64::from(w) * 7919)
        .wrapping_add(u64::from(h));
    let mut rgb = Vec::new();
    let mut luma = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let v = pix(seed, w, x, y);
            rgb.extend_from_slice(&v[..3]);
            luma.push(v[0]);
        }
    }
    (rgb, luma)
}

/// Markers and frame header of an encoded image: `(marker, segment)` up to SOS.
fn segments(bytes: &[u8]) -> Vec<(u8, Vec<u8>)> {
    assert_eq!(bytes[..2], [0xff, 0xd8], "SOI");
    let mut out = Vec::new();
    let mut i = 2;
    loop {
        assert_eq!(bytes[i], 0xff, "marker at {i}");
        let marker = bytes[i + 1];
        let len = usize::from(u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]));
        out.push((marker, bytes[i + 4..i + 2 + len].to_vec()));
        if marker == 0xda {
            return out;
        }
        i += 2 + len;
    }
}

#[test]
fn output_decodes_and_has_go_layout() {
    let (w, h) = (97, 61);
    // Smooth gradients (the oracle's synthetic images are noisy, bad for a PSNR floor).
    let rgb: Vec<u8> = (0..h)
        .flat_map(|y| (0..w).flat_map(move |x| [x * 2 + 20, y * 3 + 10, (x + y) + 40]))
        .map(|v| u8::try_from(v).expect("sample"))
        .collect();
    let (_, luma) = synthetic("gray", w, h);
    for quality in [1, 50, 75, 90, 100] {
        let bytes = jpeg::encode(Pixels::Rgb(&rgb), w, h, quality).expect("encode");
        // SOI, DQT (both tables), SOF0 (4:2:0), DHT (four tables), SOS; no APPn segment.
        let segs = segments(&bytes);
        let markers: Vec<u8> = segs.iter().map(|s| s.0).collect();
        assert_eq!(markers, [0xdb, 0xc0, 0xc4, 0xda], "q{quality}");
        assert_eq!(
            segs[1].1,
            [8, 0, 61, 0, 97, 3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1],
            "SOF0"
        );
        assert_eq!(bytes[bytes.len() - 2..], [0xff, 0xd9], "EOI");
        let decoded = decode(&bytes);
        let source = image::RgbaImage::from_fn(w, h, |x, y| {
            let o = usize::try_from((y * w + x) * 3).expect("offset");
            image::Rgba([rgb[o], rgb[o + 1], rgb[o + 2], 255])
        });
        let db = psnr(&decoded, &source);
        let min = match quality {
            1 => 15.0,
            50 => 30.0,
            _ => 33.0,
        };
        assert!(db >= min, "q{quality}: {db:.1} dB");
    }
    let bytes = jpeg::encode(Pixels::Gray(&luma), w, h, 75).expect("encode");
    let segs = segments(&bytes);
    assert_eq!(segs[1].1, [8, 0, 61, 0, 97, 1, 1, 0x11, 0], "grey SOF0");
    let decoded = image::load_from_memory(&bytes).expect("decode");
    assert_eq!(decoded.color(), image::ColorType::L8);
    // Out-of-range qualities clamp as in Go.
    assert_eq!(
        jpeg::encode(Pixels::Gray(&luma), w, h, 0),
        jpeg::encode(Pixels::Gray(&luma), w, h, 1)
    );
    assert_eq!(
        jpeg::encode(Pixels::Gray(&luma), w, h, 1000),
        jpeg::encode(Pixels::Gray(&luma), w, h, 100)
    );
    // RGBA input ignores alpha, as Go's *image.RGBA path does.
    let rgba: Vec<u8> = rgb.chunks(3).flat_map(|p| [p[0], p[1], p[2], 7]).collect();
    assert_eq!(
        jpeg::encode(Pixels::Rgba(&rgba), w, h, 75),
        jpeg::encode(Pixels::Rgb(&rgb), w, h, 75)
    );
}

#[test]
fn queue_writes_go_jpeg() {
    // A greyscale source stays one component; colour is 4:2:0; the spec's quality is Go's.
    let dir = tempfile::tempdir().expect("tempdir");
    let q = ImageQueue::new(Imaging::default(), None);
    for (src, comps) in [("gen:gray:33x17:png", 1u8), ("gen:rgba:33x17:png", 3)] {
        let path = write_file(
            dir.path(),
            &format!("{comps}.png"),
            &synth(src).expect("synth"),
        );
        for spec in ["jpg", "jpg q20", "resize 40x jpg q95"] {
            let e = q
                .enqueue(
                    &ImageInput::File(path.clone()),
                    Some(&spec.parse().expect("spec")),
                    &[],
                )
                .expect("enqueue");
            let bytes = q.encoded(e.id).expect("encode");
            let segs = segments(&bytes);
            assert_eq!(segs[1].1[5], comps, "{src} {spec}");
            let quality = spec
                .split_whitespace()
                .find_map(|t| t.strip_prefix('q'))
                .map_or(75, |v| v.parse().expect("quality"));
            // The first luminance quantiser: 16 scaled for the quality (libjpeg).
            let scale = if quality < 50 {
                5000 / quality
            } else {
                200 - 2 * quality
            };
            assert_eq!(
                i32::from(segs[0].1[1]),
                ((16 * scale + 50) / 100).clamp(1, 255),
                "{src} {spec}"
            );
        }
    }
}
