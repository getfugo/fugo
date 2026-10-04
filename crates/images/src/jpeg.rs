//! Baseline JPEG encoding as Go's `image/jpeg` `Encode` writes it, which the Go implementation uses
//! for every JPEG it writes. A port of Go's `src/image/jpeg/writer.go` (© The Go Authors,
//! BSD-3-Clause, `THIRD_PARTY/go/LICENSE`).
//!
//! What Go writes, and so what this writes: SOI, one DQT segment with the two tables of the
//! JPEG specification (K.1) scaled by quality as libjpeg does, SOF0 (8-bit, three components
//! with 4:2:0 subsampling, or one for greyscale), one DHT segment with the standard Huffman
//! tables (K.3), SOS, the entropy-coded data padded with one bits, EOI. No JFIF or EXIF
//! segment, no restart markers.
//!
//! Colour: every pixel goes through `color.RGBToYCbCr` (JFIF coefficients in 16-bit fixed
//! point); each 16×16 macroblock gives four luma blocks and one Cb and one Cr block, each
//! chroma sample the rounded mean of a 2×2 group of full-resolution samples. Edge blocks
//! repeat the last row and column. Planar YCbCr input (Go's `*image.YCbCr`) is sampled as Go
//! samples it, at the chroma sample of each pixel.
//!
//! The forward DCT: Go 1.26 replaced the IJG `jfdctint.c` port of earlier releases with a new
//! fixed-point implementation (the Go implementation's reference outputs come from Go 1.27.1). Its
//! output is the exactly rounded DCT but for a rare coefficient near a rounding boundary, so the
//! DCT here computes the exact one (in `f64`, rounded): the bytes equal Go 1.27's for most images
//! and differ in a few coefficients of large ones (crate README, `expected_diffs.toml` `[jpeg]`).

use std::fmt;

mod encoder;

use encoder::*;

/// A block of 64 samples or coefficients, in natural (row-major) order.
type Block = [i32; 64];

/// The chroma subsampling of planar [`YCbCr`] input (Go's `image.YCbCrSubsampleRatio`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subsample {
    /// 4:4:4: one chroma sample per pixel.
    S444,
    /// 4:2:2: one per two pixels of a row.
    S422,
    /// 4:2:0: one per 2×2 pixels.
    S420,
    /// 4:4:0: one per two pixels of a column.
    S440,
    /// 4:1:1: one per four pixels of a row.
    S411,
    /// 4:1:0: one per 4×2 pixels.
    S410,
}

impl Subsample {
    /// The horizontal and vertical chroma divisors.
    const fn divisors(self) -> (usize, usize) {
        match self {
            Self::S444 => (1, 1),
            Self::S422 => (2, 1),
            Self::S420 => (2, 2),
            Self::S440 => (1, 2),
            Self::S411 => (4, 1),
            Self::S410 => (4, 2),
        }
    }
}

/// Planar YCbCr pixels (Go's `*image.YCbCr` with its origin at 0, 0).
#[derive(Clone, Copy, Debug)]
pub struct YCbCr<'a> {
    /// Luma, `y_stride` bytes per row.
    pub y: &'a [u8],
    /// Blue-difference chroma, `c_stride` bytes per row.
    pub cb: &'a [u8],
    /// Red-difference chroma, `c_stride` bytes per row.
    pub cr: &'a [u8],
    /// Bytes per row of `y`.
    pub y_stride: usize,
    /// Bytes per row of `cb` and `cr`.
    pub c_stride: usize,
    /// How the chroma planes are subsampled.
    pub subsample: Subsample,
}

/// The pixels to encode, as Go's encoder distinguishes them.
#[derive(Clone, Copy, Debug)]
pub enum Pixels<'a> {
    /// Four bytes per pixel, R G B and an alpha that is ignored (Go's `*image.RGBA` path,
    /// which the Go implementation takes for opaque results): three components, 4:2:0.
    Rgba(&'a [u8]),
    /// Three bytes per pixel, R G B: encoded as [`Pixels::Rgba`].
    Rgb(&'a [u8]),
    /// One byte per pixel (Go's `*image.Gray`): one component.
    Gray(&'a [u8]),
    /// Planar YCbCr (Go's `*image.YCbCr`): three components, 4:2:0.
    YCbCr(YCbCr<'a>),
}

/// Why an image cannot be encoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JpegError {
    /// A side of 65536 pixels or more (Go: "jpeg: image is too large to encode").
    TooLarge,
    /// The pixel buffer is shorter than the size says.
    ShortBuffer,
}

impl fmt::Display for JpegError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge => f.write_str("image is too large to encode"),
            Self::ShortBuffer => f.write_str("pixel buffer too short for the image size"),
        }
    }
}

impl std::error::Error for JpegError {}

/// Encodes `width`×`height` pixels at `quality` (clamped to 1…100, as Go clamps it) into the
/// bytes Go's `jpeg.Encode` writes for the same pixels.
///
/// # Errors
/// A side of 65536 pixels or more; a buffer too short for the size.
pub fn encode(
    pixels: Pixels<'_>,
    width: u32,
    height: u32,
    quality: i32,
) -> Result<Vec<u8>, JpegError> {
    let (Ok(w), Ok(h)) = (u16::try_from(width), u16::try_from(height)) else {
        return Err(JpegError::TooLarge);
    };
    let (w, h) = (usize::from(w), usize::from(h));
    check_len(pixels, w, h)?;
    let quant = quant_tables(quality);
    let gray = matches!(pixels, Pixels::Gray(_));
    let mut e = Encoder {
        out: Vec::with_capacity(1024 + w * h / 4),
        bits: 0,
        n_bits: 0,
        quant,
        lut: huffman_luts(),
        basis: dct_basis(),
    };
    e.out.extend_from_slice(&[0xff, 0xd8]);
    e.write_dqt();
    e.write_sof0(w, h, gray);
    e.write_dht(gray);
    e.write_sos(pixels, w, h);
    e.out.extend_from_slice(&[0xff, 0xd9]);
    Ok(e.out)
}

fn check_len(pixels: Pixels<'_>, w: usize, h: usize) -> Result<(), JpegError> {
    let enough = match pixels {
        Pixels::Rgba(p) => p.len() >= w * h * 4,
        Pixels::Rgb(p) => p.len() >= w * h * 3,
        Pixels::Gray(p) => p.len() >= w * h,
        Pixels::YCbCr(m) => {
            let (dx, dy) = m.subsample.divisors();
            let (cw, ch) = (w.div_ceil(dx), h.div_ceil(dy));
            let plane = |stride: usize, pw: usize, ph: usize| {
                if pw == 0 || ph == 0 {
                    0
                } else {
                    stride * (ph - 1) + pw
                }
            };
            let need_y = plane(m.y_stride, w, h);
            let need_c = plane(m.c_stride, cw, ch);
            m.y_stride >= w
                && m.c_stride >= cw
                && m.y.len() >= need_y
                && m.cb.len() >= need_c
                && m.cr.len() >= need_c
        }
    };
    if enough {
        Ok(())
    } else {
        Err(JpegError::ShortBuffer)
    }
}

// ---------------------------------------------------------------------------------------
// Tables (writer.go).

/// `unzig[i]` is the natural-order index of the `i`th coefficient in zig-zag order.
const UNZIG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

/// The luminance and chrominance quantisation tables of the JPEG specification (K.1), in
/// zig-zag order, before scaling by quality.
const UNSCALED_QUANT: [[u8; 64]; 2] = [
    [
        16, 11, 12, 14, 12, 10, 16, 14, 13, 14, 18, 17, 16, 19, 24, 40, 26, 24, 22, 22, 24, 49, 35,
        37, 29, 40, 58, 51, 61, 60, 57, 51, 56, 55, 64, 72, 92, 78, 64, 68, 87, 69, 55, 56, 80,
        109, 81, 87, 95, 98, 103, 104, 103, 62, 77, 113, 121, 112, 100, 120, 92, 101, 103, 99,
    ],
    [
        17, 18, 18, 24, 21, 24, 47, 26, 26, 47, 99, 66, 56, 66, 99, 99, 99, 99, 99, 99, 99, 99, 99,
        99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
        99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
    ],
];

/// A Huffman table: `count[i]` codes of `i + 1` bits, for `values` in order.
struct HuffmanSpec {
    count: [u8; 16],
    values: &'static [u8],
}

/// Luminance DC, luminance AC, chrominance DC, chrominance AC (the tables of K.3).
const HUFFMAN_SPECS: [HuffmanSpec; 4] = [
    HuffmanSpec {
        count: [0, 1, 5, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0],
        values: &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
    },
    HuffmanSpec {
        count: [0, 2, 1, 3, 3, 2, 4, 3, 5, 5, 4, 4, 0, 0, 1, 125],
        values: &[
            0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06, 0x13, 0x51,
            0x61, 0x07, 0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xa1, 0x08, 0x23, 0x42, 0xb1, 0xc1,
            0x15, 0x52, 0xd1, 0xf0, 0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0a, 0x16, 0x17, 0x18,
            0x19, 0x1a, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39,
            0x3a, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x53, 0x54, 0x55, 0x56, 0x57,
            0x58, 0x59, 0x5a, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6a, 0x73, 0x74, 0x75,
            0x76, 0x77, 0x78, 0x79, 0x7a, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8a, 0x92,
            0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7,
            0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3,
            0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8,
            0xd9, 0xda, 0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9, 0xea, 0xf1, 0xf2,
            0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9, 0xfa,
        ],
    },
    HuffmanSpec {
        count: [0, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0],
        values: &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
    },
    HuffmanSpec {
        count: [0, 2, 1, 2, 4, 4, 3, 4, 7, 5, 4, 4, 0, 1, 2, 119],
        values: &[
            0x00, 0x01, 0x02, 0x03, 0x11, 0x04, 0x05, 0x21, 0x31, 0x06, 0x12, 0x41, 0x51, 0x07,
            0x61, 0x71, 0x13, 0x22, 0x32, 0x81, 0x08, 0x14, 0x42, 0x91, 0xa1, 0xb1, 0xc1, 0x09,
            0x23, 0x33, 0x52, 0xf0, 0x15, 0x62, 0x72, 0xd1, 0x0a, 0x16, 0x24, 0x34, 0xe1, 0x25,
            0xf1, 0x17, 0x18, 0x19, 0x1a, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x35, 0x36, 0x37, 0x38,
            0x39, 0x3a, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4a, 0x53, 0x54, 0x55, 0x56,
            0x57, 0x58, 0x59, 0x5a, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6a, 0x73, 0x74,
            0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89,
            0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0xa2, 0xa3, 0xa4, 0xa5,
            0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba,
            0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6,
            0xd7, 0xd8, 0xd9, 0xda, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9, 0xea, 0xf2,
            0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9, 0xfa,
        ],
    },
];

/// Per table and value: the code length in the top 8 bits, the code in the low 24.
type HuffmanLut = [[u32; 256]; 4];

fn huffman_luts() -> HuffmanLut {
    let mut lut = [[0u32; 256]; 4];
    for (table, spec) in lut.iter_mut().zip(&HUFFMAN_SPECS) {
        let (mut code, mut k) = (0u32, 0usize);
        for (len, &count) in (1u32..).zip(&spec.count) {
            for _ in 0..count {
                table[usize::from(spec.values[k])] = len << 24 | code;
                code += 1;
                k += 1;
            }
            code <<= 1;
        }
    }
    lut
}

/// The quantisation tables scaled for `quality` (libjpeg's scaling), in zig-zag order.
fn quant_tables(quality: i32) -> [[u8; 64]; 2] {
    let quality = quality.clamp(1, 100);
    let scale = if quality < 50 {
        5000 / quality
    } else {
        200 - quality * 2
    };
    UNSCALED_QUANT.map(|table| {
        table.map(|q| {
            let x = (i32::from(q) * scale + 50) / 100;
            u8::try_from(x.clamp(1, 255)).unwrap_or(u8::MAX)
        })
    })
}

// ---------------------------------------------------------------------------------------
// Colour conversion.

/// Go's `color.RGBToYCbCr`: JFIF YCbCr in 16-bit fixed point.
pub(crate) fn rgb_to_ycbcr(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let (r, g, b) = (i32::from(r), i32::from(g), i32::from(b));
    let y = (19595 * r + 38470 * g + 7471 * b + (1 << 15)) >> 16;
    // Go's clamping of the chroma: in range when the top byte is clear, else 0 or 255.
    let clamp = |v: i32| -> u8 {
        let v = if v.cast_unsigned() & 0xff00_0000 == 0 {
            v >> 16
        } else {
            !(v >> 31)
        };
        v.to_le_bytes()[0]
    };
    let cb = clamp(-11056 * r - 21712 * g + 32768 * b + (257 << 15));
    let cr = clamp(32768 * r - 27440 * g - 5328 * b + (257 << 15));
    (y.to_le_bytes()[0], cb, cr)
}

/// Fills the Y, Cb and Cr blocks of the 8×8 region at (`px`, `py`), repeating the last row
/// and column past the edges (writer.go `rgbaToYCbCr`, `yCbCrToYCbCr`).
fn to_ycbcr(
    pixels: Pixels<'_>,
    (w, h): (usize, usize),
    (px, py): (usize, usize),
    yb: &mut Block,
    cbb: &mut Block,
    crb: &mut Block,
) {
    for j in 0..8 {
        let sy = (py + j).min(h - 1);
        for i in 0..8 {
            let sx = (px + i).min(w - 1);
            let (y, cb, cr) = match pixels {
                Pixels::Rgba(p) => {
                    let o = (sy * w + sx) * 4;
                    rgb_to_ycbcr(p[o], p[o + 1], p[o + 2])
                }
                Pixels::Rgb(p) => {
                    let o = (sy * w + sx) * 3;
                    rgb_to_ycbcr(p[o], p[o + 1], p[o + 2])
                }
                Pixels::YCbCr(m) => {
                    let (dx, dy) = m.subsample.divisors();
                    let ci = sy / dy * m.c_stride + sx / dx;
                    (m.y[sy * m.y_stride + sx], m.cb[ci], m.cr[ci])
                }
                Pixels::Gray(p) => {
                    let v = p[sy * w + sx];
                    (v, 128, 128)
                }
            };
            yb[8 * j + i] = i32::from(y);
            cbb[8 * j + i] = i32::from(cb);
            crb[8 * j + i] = i32::from(cr);
        }
    }
}

/// Averages the 16×16 region of four chroma blocks (top left, top right, bottom left,
/// bottom right) into one 8×8 block, rounding (writer.go `scale`).
fn downsample(dst: &mut Block, src: &[Block; 4]) {
    for (i, s) in src.iter().enumerate() {
        let off = (i & 2) << 4 | (i & 1) << 2;
        for y in 0..4 {
            for x in 0..4 {
                let j = 16 * y + 2 * x;
                let sum = s[j] + s[j + 1] + s[j + 8] + s[j + 9];
                dst[8 * y + x + off] = (sum + 2) >> 2;
            }
        }
    }
}

// ---------------------------------------------------------------------------------------
// Forward DCT.

/// `8 × c(u) × cos((2x + 1)uπ/16)`, `c(0) = √(1/8)`, else `1/2`: the orthonormal DCT-II
/// basis, with the factor 8 of Go's scaling folded into one of the two passes.
fn dct_basis() -> [[f64; 8]; 8] {
    let mut m = [[0.0; 8]; 8];
    for (u, row) in (0u32..).zip(m.iter_mut()) {
        let c = if u == 0 { (0.125f64).sqrt() } else { 0.5 };
        for (x, v) in (0u32..).zip(row.iter_mut()) {
            *v = c * (f64::from(2 * x + 1) * f64::from(u) * std::f64::consts::PI / 16.0).cos();
        }
    }
    m
}

/// The forward DCT of a block with the level shift, in place: 8 × the orthonormal 2-D
/// DCT-II of `sample − 128`, rounded to the nearest integer (halves away from zero).
fn fdct(b: &mut Block, basis: &[[f64; 8]; 8]) {
    let mut rows = [[0.0f64; 8]; 8];
    for (y, row) in rows.iter_mut().enumerate() {
        for (u, out) in row.iter_mut().enumerate() {
            *out = (0..8)
                .map(|x| basis[u][x] * f64::from(b[8 * y + x] - 128))
                .sum();
        }
    }
    for v in 0..8 {
        for u in 0..8 {
            let c: f64 = (0..8).map(|y| basis[v][y] * rows[y][u]).sum();
            // |c| ≤ 8 × 1024: exact in i32.
            #[allow(clippy::cast_possible_truncation)]
            let r = (8.0 * c).round() as i32;
            b[8 * v + u] = r;
        }
    }
}

// ---------------------------------------------------------------------------------------
// The writer.

/// `a / b` rounded to the nearest integer, halves away from zero (writer.go `div`).
const fn div(a: i32, b: i32) -> i32 {
    if a >= 0 {
        (a + (b >> 1)) / b
    } else {
        -((-a + (b >> 1)) / b)
    }
}

/// The number of bits needed to hold `a` (0 for 0).
const fn bit_count(a: i32) -> u32 {
    32 - a.leading_zeros()
}

#[cfg(test)]
mod tests;
