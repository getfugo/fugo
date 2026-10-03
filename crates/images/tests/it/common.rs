//! Test support: the oracle's synthetic images, source paths, PSNR, an in-memory sink.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use image::{
    DynamicImage, ExtendedColorType, ImageBuffer, ImageEncoder, Luma, Rgb, Rgba, RgbaImage,
    codecs::png::PngEncoder,
};
use ssg_base::Sink;
use ssg_base::paths::OutputPath;
use ssg_images::jpeg;
use ssg_testkit::fixture::{repo_dir, repo_file};

/// One table of `crates/images/expected_diffs.toml`: case or rule → reason.
pub fn expected_diffs(section: &str) -> BTreeMap<String, String> {
    let path = repo_dir().join("crates/images/expected_diffs.toml");
    let text = std::fs::read_to_string(&path).expect("expected_diffs.toml");
    let doc: toml::Table = text.parse().expect("expected_diffs.toml is TOML");
    doc.get(section)
        .and_then(toml::Value::as_table)
        .map(|t| {
            t.iter()
                .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_owned()))
                .collect()
        })
        .unwrap_or_default()
}

/// The file of an oracle source id (`file:<path as the oracle saw it>`), when available. The
/// oracle's sources from Go's own image test data (`crates/go-*/tests/fixtures/gotestdata/`) are
/// in `testdata/upstream/goroot/src/image/` (Go 1.24.7's files); the five Go 1.24.7 does not have
/// are the old port's copies in `testdata/upstream/old-port/` (`be02933a`).
pub fn source_path(id: &str) -> Option<PathBuf> {
    let rel = id.strip_prefix("file:")?;
    let map = [
        (
            "crates/go-image/tests/fixtures/gotestdata/",
            "testdata/upstream/goroot/src/image/testdata/",
        ),
        (
            "crates/go-png/tests/fixtures/gotestdata/image/",
            "testdata/upstream/goroot/src/image/testdata/",
        ),
        (
            "crates/go-png/tests/fixtures/gotestdata/",
            "testdata/upstream/goroot/src/image/png/testdata/",
        ),
        (
            "crates/go-image/tests/fixtures/gotestdata/",
            "testdata/upstream/old-port/go-image/",
        ),
        (
            "crates/go-png/tests/fixtures/gotestdata/",
            "testdata/upstream/old-port/go-png/",
        ),
        (
            "crates/go-png/tests/fixtures/repo/",
            "testdata/site-assets/repo/",
        ),
    ];
    let mut mapped = false;
    for (from, to) in map {
        if let Some(rest) = rel.strip_prefix(from) {
            let path = repo_file(to).join(rest);
            if path.is_file() {
                return Some(path);
            }
            mapped = true;
        }
    }
    if mapped {
        return None;
    }
    // Files of the Go tree (44529028) with a byte-identical twin in the repository.
    let twins = [
        (
            "hugolib/testdata/sunset.jpg",
            "resources/testdata/sunset.jpg",
        ),
        (
            "resources/assets/sunset.jpg",
            "resources/testdata/sunset.jpg",
        ),
        (
            "media/testdata/resource.jpg",
            "resources/testdata/iss8079.jpg",
        ),
        (
            "media/testdata/resource.png",
            "resources/testdata/gopher-hero8.png",
        ),
        (
            "media/testdata/resource.webp",
            "resources/testdata/sunset.webp",
        ),
        (
            "snap/local/logo.png",
            "docs/assets/images/logos/logo-512x512.png",
        ),
    ];
    let rel = twins
        .into_iter()
        .find_map(|(from, to)| (from == rel).then_some(to))
        .unwrap_or(rel);
    Some(repo_file(rel)).filter(|p| p.is_file())
}

// ---------------------------------------------------------------------------------------
// The oracle's synthetic images (tools/go-oracle/nh-images/process/main.go at 44529028).

fn splitmix64(x: u64) -> u64 {
    let x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

pub fn pix(seed: u64, w: u32, x: u32, y: u32) -> [u8; 8] {
    let h = splitmix64(
        seed.wrapping_mul(1_000_003)
            .wrapping_add(u64::from(y) * u64::from(w) + u64::from(x)),
    );
    let mut v = [0u8; 8];
    for (c, out) in (0u64..).zip(v.iter_mut()) {
        let n = u64::from(x) * (c + 1) * 3 + u64::from(y) * (c + 2) * 5 + ((h >> (8 * c)) & 0x1f);
        *out = n.to_le_bytes()[0];
    }
    v
}

pub fn kind_seed(kind: &str) -> u64 {
    kind.bytes()
        .fold(0u64, |s, c| s.wrapping_mul(131).wrapping_add(u64::from(c)))
}

fn alpha(v: u8) -> u8 {
    match v {
        0..40 => 0,
        201.. => 255,
        _ => v,
    }
}

/// The planes of the oracle's `image.YCbCr` synthetic images (`ycbcr444`, `ycbcr420`), as
/// main.go fills them: every pixel writes its chroma sample, so the last pixel of a group
/// wins.
pub fn synth_ycbcr(kind: &str, w: u32, h: u32) -> Option<Planes> {
    let sub = match kind {
        "ycbcr444" => 1,
        "ycbcr420" => 2,
        _ => return None,
    };
    let seed = synth_seed(kind, w, h);
    let (wu, hu) = (usize::try_from(w).ok()?, usize::try_from(h).ok()?);
    let (cw, ch) = (wu.div_ceil(sub), hu.div_ceil(sub));
    let (mut y, mut cb, mut cr) = (vec![0; wu * hu], vec![0; cw * ch], vec![0; cw * ch]);
    for (yy, row) in (0..h).zip(0usize..) {
        for (xx, col) in (0..w).zip(0usize..) {
            let v = pix(seed, w, xx, yy);
            y[row * wu + col] = v[0];
            let ci = row / sub * cw + col / sub;
            cb[ci] = v[1];
            cr[ci] = v[2];
        }
    }
    Some(Planes {
        y,
        cb,
        cr,
        c_stride: cw,
    })
}

/// Planar YCbCr samples (Go's `image.YCbCr`, origin 0, 0; the luma stride is the width).
pub struct Planes {
    pub y: Vec<u8>,
    pub cb: Vec<u8>,
    pub cr: Vec<u8>,
    pub c_stride: usize,
}

fn synth_seed(kind: &str, w: u32, h: u32) -> u64 {
    kind_seed(kind)
        .wrapping_add(u64::from(w) * 7919)
        .wrapping_add(u64::from(h))
}

/// The encoded synthetic source `gen:<kind>:<w>x<h>:<png|jpg>`. PNG sources decode to the
/// oracle's pixels exactly; JPEG sources (`ycbcr444`, `ycbcr420`) are Go's bytes, encoded
/// from the same planes at q90 with the port of Go's encoder (`ssg_images::jpeg`).
pub fn synth(id: &str) -> Option<Vec<u8>> {
    let mut parts = id.strip_prefix("gen:")?.split(':');
    let kind = parts.next()?;
    let (w, h) = parts.next()?.split_once('x')?;
    let (w, h): (u32, u32) = (w.parse().ok()?, h.parse().ok()?);
    let enc = parts.next()?;
    let seed = synth_seed(kind, w, h);
    if enc == "jpg" {
        let Planes {
            y,
            cb,
            cr,
            c_stride,
        } = synth_ycbcr(kind, w, h)?;
        let subsample = if kind == "ycbcr420" {
            jpeg::Subsample::S420
        } else {
            jpeg::Subsample::S444
        };
        let planes = jpeg::YCbCr {
            y: &y,
            cb: &cb,
            cr: &cr,
            y_stride: usize::try_from(w).ok()?,
            c_stride,
            subsample,
        };
        return jpeg::encode(jpeg::Pixels::YCbCr(planes), w, h, 90).ok();
    }
    let hi = |a: u8, b: u8| u16::from(a) << 8 | u16::from(b);
    let img: DynamicImage = match kind {
        "nrgba" | "nrgbaa" => DynamicImage::ImageRgba8(RgbaImage::from_fn(w, h, |x, y| {
            let v = pix(seed, w, x, y);
            let a = if kind == "nrgbaa" { alpha(v[3]) } else { 255 };
            Rgba([v[0], v[1], v[2], a])
        })),
        "rgba" => DynamicImage::ImageRgb8(ImageBuffer::from_fn(w, h, |x, y| {
            let v = pix(seed, w, x, y);
            Rgb([v[0], v[1], v[2]])
        })),
        "gray" => DynamicImage::ImageLuma8(ImageBuffer::from_fn(w, h, |x, y| {
            Luma([pix(seed, w, x, y)[0]])
        })),
        "gray16" => DynamicImage::ImageLuma16(ImageBuffer::from_fn(w, h, |x, y| {
            let v = pix(seed, w, x, y);
            Luma([hi(v[0], v[1])])
        })),
        "nrgba64" => DynamicImage::ImageRgba16(ImageBuffer::from_fn(w, h, |x, y| {
            let v = pix(seed, w, x, y);
            Rgba([
                hi(v[0], v[4]),
                hi(v[1], v[5]),
                hi(v[2], v[6]),
                hi(alpha(v[3]), v[7]),
            ])
        })),
        "rgba64" => DynamicImage::ImageRgb16(ImageBuffer::from_fn(w, h, |x, y| {
            let v = pix(seed, w, x, y);
            Rgb([hi(v[0], v[4]), hi(v[1], v[5]), hi(v[2], v[6])])
        })),
        "paletted" => {
            let palette: Vec<[u8; 4]> = (0..40u32)
                .map(|i| {
                    let v = pix(seed, 1, i, 0);
                    let a = if i % 3 == 0 { alpha(v[3]) } else { 255 };
                    [v[0], v[1], v[2], a]
                })
                .collect();
            DynamicImage::ImageRgba8(RgbaImage::from_fn(w, h, |x, y| {
                Rgba(palette[usize::from(pix(seed, w, x, y)[0] % 40)])
            }))
        }
        _ => return None,
    };
    if enc != "png" {
        return None;
    }
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
        .ok()?;
    Some(out)
}

/// SHA-256 of `data`, in lower-case hex (the oracle's content hashes; FIPS 180-4).
pub fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a_2f98,
        0x7137_4491,
        0xb5c0_fbcf,
        0xe9b5_dba5,
        0x3956_c25b,
        0x59f1_11f1,
        0x923f_82a4,
        0xab1c_5ed5,
        0xd807_aa98,
        0x1283_5b01,
        0x2431_85be,
        0x550c_7dc3,
        0x72be_5d74,
        0x80de_b1fe,
        0x9bdc_06a7,
        0xc19b_f174,
        0xe49b_69c1,
        0xefbe_4786,
        0x0fc1_9dc6,
        0x240c_a1cc,
        0x2de9_2c6f,
        0x4a74_84aa,
        0x5cb0_a9dc,
        0x76f9_88da,
        0x983e_5152,
        0xa831_c66d,
        0xb003_27c8,
        0xbf59_7fc7,
        0xc6e0_0bf3,
        0xd5a7_9147,
        0x06ca_6351,
        0x1429_2967,
        0x27b7_0a85,
        0x2e1b_2138,
        0x4d2c_6dfc,
        0x5338_0d13,
        0x650a_7354,
        0x766a_0abb,
        0x81c2_c92e,
        0x9272_2c85,
        0xa2bf_e8a1,
        0xa81a_664b,
        0xc24b_8b70,
        0xc76c_51a3,
        0xd192_e819,
        0xd699_0624,
        0xf40e_3585,
        0x106a_a070,
        0x19a4_c116,
        0x1e37_6c08,
        0x2748_774c,
        0x34b0_bcb5,
        0x391c_0cb3,
        0x4ed8_aa4a,
        0x5b9c_ca4f,
        0x682e_6ff3,
        0x748f_82ee,
        0x78a5_636f,
        0x84c8_7814,
        0x8cc7_0208,
        0x90be_fffa,
        0xa450_6ceb,
        0xbef9_a3f7,
        0xc671_78f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09_e667,
        0xbb67_ae85,
        0x3c6e_f372,
        0xa54f_f53a,
        0x510e_527f,
        0x9b05_688c,
        0x1f83_d9ab,
        0x5be0_cd19,
    ];
    let mut msg = data.to_vec();
    let bit_len = u64::try_from(data.len()).expect("length") * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in msg.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for (i, word) in chunk.as_chunks::<4>().0.iter().enumerate() {
            w[i] = u32::from_be_bytes(*word);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *x = x.wrapping_add(y);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

/// Writes a PNG of `img` (test helper).
pub fn png(img: &RgbaImage) -> Vec<u8> {
    let mut out = Vec::new();
    PngEncoder::new(&mut out)
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            ExtendedColorType::Rgba8,
        )
        .expect("encode png");
    out
}

/// Decodes an encoded image to RGBA.
pub fn decode(bytes: &[u8]) -> RgbaImage {
    image::load_from_memory(bytes).expect("decode").into_rgba8()
}

/// PSNR in dB of two same-size images, over RGB composited on black and on white (the lower
/// of the two) so that alpha differences count. `f64::INFINITY` for identical images.
pub fn psnr(a: &RgbaImage, b: &RgbaImage) -> f64 {
    assert_eq!(a.dimensions(), b.dimensions(), "PSNR needs equal sizes");
    let mut worst = f64::INFINITY;
    for bg in [0.0f64, 255.0] {
        let mut se = 0.0f64;
        for (p, q) in a.pixels().zip(b.pixels()) {
            let (pa, qa) = (f64::from(p.0[3]) / 255.0, f64::from(q.0[3]) / 255.0);
            for i in 0..3 {
                let x = f64::from(p.0[i]) * pa + bg * (1.0 - pa);
                let y = f64::from(q.0[i]) * qa + bg * (1.0 - qa);
                se += (x - y) * (x - y);
            }
        }
        let n = f64::from(a.width()) * f64::from(a.height()) * 3.0;
        let mse = se / n;
        let v = if mse == 0.0 {
            f64::INFINITY
        } else {
            10.0 * (255.0 * 255.0 / mse).log10()
        };
        worst = worst.min(v);
    }
    worst
}

/// A sink that keeps the files in memory.
#[derive(Default)]
pub struct MemorySink {
    pub files: Mutex<BTreeMap<OutputPath, Vec<u8>>>,
}

impl Sink for MemorySink {
    fn write(&self, path: &OutputPath, bytes: &[u8]) -> std::io::Result<()> {
        self.files
            .lock()
            .expect("sink lock")
            .insert(path.clone(), bytes.to_vec());
        Ok(())
    }

    fn exists(&self, path: &OutputPath) -> bool {
        self.files.lock().expect("sink lock").contains_key(path)
    }

    fn read(&self, path: &OutputPath) -> std::io::Result<Vec<u8>> {
        self.files
            .lock()
            .expect("sink lock")
            .get(path)
            .cloned()
            .ok_or_else(|| std::io::ErrorKind::NotFound.into())
    }
}

/// Writes `bytes` to `dir/name` and returns the path.
pub fn write_file(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, bytes).expect("write test file");
    p
}
