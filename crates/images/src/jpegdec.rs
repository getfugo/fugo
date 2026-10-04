//! Go's JPEG decoder, for the smart crop analysis: a port of `image/jpeg`'s reader as Go 1.25 has
//! it (`reader.go`, `scan.go`, `huffman.go`, `idct.go`), which the Go implementation's published
//! builds decoded sources with.
//!
//! The processing pipeline decodes JPEGs with the `image` crate, whose inverse DCT and chroma
//! upsampling give pixels a level or two away from Go's. That is invisible in a processed
//! image but not to smartcrop, which picks a region by comparing scores that can differ by
//! less than such noise; so the analysis reads what Go's decoder produces: the luma and
//! chroma planes of an `*image.YCbCr` (gift converts them itself, with nearest chroma), the
//! plane of an `*image.Gray`, and Go's conversions for RGB (`*image.RGBA`) and CMYK
//! (`*image.CMYK`) JPEGs. The inverse DCT is Go 1.25's, the MPEG Software Simulation
//! Group's integer IDCT (Go 1.26 replaced it). Baseline, extended and progressive JPEGs,
//! restart intervals, Huffman look-up and every check that makes Go reject a file are as in
//! Go; the input is all in memory, so the 4 KiB buffering of the reader has no equivalent.

use crate::gift::YCbCrPlanes;

mod huffman;
mod idct;
mod reader;
mod scan;

use idct::idct;

const BLOCK_SIZE: usize = 64;

/// A DCT block, coefficients in natural order.
type Block = [i32; BLOCK_SIZE];

const DC_TABLE: usize = 0;
const AC_TABLE: usize = 1;
const MAX_TC: u8 = 1;
const MAX_TH: u8 = 3;
const MAX_TQ: u8 = 3;
const MAX_COMPONENTS: usize = 4;

const SOF0_MARKER: u8 = 0xc0;
const SOF1_MARKER: u8 = 0xc1;
const SOF2_MARKER: u8 = 0xc2;
const DHT_MARKER: u8 = 0xc4;
const RST0_MARKER: u8 = 0xd0;
const RST7_MARKER: u8 = 0xd7;
const SOI_MARKER: u8 = 0xd8;
const EOI_MARKER: u8 = 0xd9;
const SOS_MARKER: u8 = 0xda;
const DQT_MARKER: u8 = 0xdb;
const DRI_MARKER: u8 = 0xdd;
const COM_MARKER: u8 = 0xfe;
const APP0_MARKER: u8 = 0xe0;
const APP14_MARKER: u8 = 0xee;
const APP15_MARKER: u8 = 0xef;

const ADOBE_TRANSFORM_UNKNOWN: u8 = 0;

/// `unzig`: zig-zag order to natural order.
const UNZIG: [usize; BLOCK_SIZE] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20,
    13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59,
    52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

/// Why Go's decoder rejects a file (`FormatError`, `UnsupportedError`, and the sentinel
/// errors its Huffman decoding tells apart).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum JpegError {
    #[error("invalid JPEG format: {0}")]
    Format(&'static str),
    #[error("unsupported JPEG feature: {0}")]
    Unsupported(&'static str),
    #[error("unexpected EOF")]
    UnexpectedEof,
}

type Result<T> = std::result::Result<T, JpegError>;

/// `errMissingFF00`.
const MISSING_FF00: JpegError = JpegError::Format("missing 0xff00 sequence");
/// `errShortHuffmanData`.
const SHORT_HUFFMAN_DATA: JpegError = JpegError::Format("short Huffman data");
/// `errUnsupportedSubsamplingRatio`.
const UNSUPPORTED_SUBSAMPLING: JpegError = JpegError::Unsupported("luma/chroma subsampling ratio");

/// What Go's decoder returns.
pub(crate) enum GoJpeg {
    /// `*image.Gray`: the plane (`stride` bytes per row).
    Gray {
        width: usize,
        height: usize,
        pix: Vec<u8>,
        stride: usize,
    },
    /// `*image.YCbCr`.
    YCbCr {
        width: usize,
        height: usize,
        planes: YCbCrPlanes,
    },
    /// `*image.RGBA` (RGB JPEGs), 4 bytes per pixel, opaque.
    Rgba {
        width: usize,
        height: usize,
        pix: Vec<u8>,
    },
    /// `*image.CMYK`, 4 bytes per pixel.
    Cmyk {
        width: usize,
        height: usize,
        pix: Vec<u8>,
    },
}

/// `component`: a frame component (section B.2.2).
#[derive(Clone, Copy, Default)]
struct Component {
    h: usize,
    v: usize,
    c: u8,
    tq: u8,
}

const MAX_CODE_LENGTH: usize = 16;
const MAX_N_CODES: usize = 256;
const LUT_SIZE: u32 = 8;

/// `huffman`: a Huffman decoder (section C).
#[derive(Clone)]
struct Huffman {
    n_codes: i32,
    lut: [u16; 1 << LUT_SIZE],
    vals: [u8; MAX_N_CODES],
    min_codes: [i32; MAX_CODE_LENGTH],
    max_codes: [i32; MAX_CODE_LENGTH],
    vals_indices: [i32; MAX_CODE_LENGTH],
}

impl Default for Huffman {
    fn default() -> Self {
        Self {
            n_codes: 0,
            lut: [0; 1 << LUT_SIZE],
            vals: [0; MAX_N_CODES],
            min_codes: [0; MAX_CODE_LENGTH],
            max_codes: [0; MAX_CODE_LENGTH],
            vals_indices: [0; MAX_CODE_LENGTH],
        }
    }
}

/// `bits`: unread bits of the entropy-coded data, read MSB first.
#[derive(Clone, Copy, Default)]
struct Bits {
    a: u32,
    m: u32,
    n: i32,
}

/// The planes being decoded (`img3`), before Go wraps them.
struct Planes3 {
    y: Vec<u8>,
    cb: Vec<u8>,
    cr: Vec<u8>,
    y_stride: usize,
    c_stride: usize,
    sub: (usize, usize),
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "Go's decoder state, field for field"
)]
struct Decoder<'a> {
    data: &'a [u8],
    /// The read position; the bytes before it are consumed.
    i: usize,
    /// The bytes to back up after a Huffman overshoot (0, 1 or 2).
    n_unreadable: usize,
    bits: Bits,
    width: usize,
    height: usize,
    img1: Option<(Vec<u8>, usize)>,
    img3: Option<Planes3>,
    black: Option<(Vec<u8>, usize)>,
    ri: usize,
    n_comp: usize,
    baseline: bool,
    progressive: bool,
    jfif: bool,
    adobe_transform_valid: bool,
    adobe_transform: u8,
    eob_run: u16,
    comp: [Component; MAX_COMPONENTS],
    prog_coeffs: [Vec<Block>; MAX_COMPONENTS],
    huff: [[Huffman; (MAX_TH + 1) as usize]; (MAX_TC + 1) as usize],
    quant: [Block; (MAX_TQ + 1) as usize],
    tmp: [u8; 2 * BLOCK_SIZE],
}

/// Decodes a JPEG as Go's `jpeg.Decode` does.
///
/// # Errors
/// Whatever makes Go's decoder fail.
pub(crate) fn decode(data: &[u8]) -> Result<GoJpeg> {
    let mut d = Decoder {
        data,
        i: 0,
        n_unreadable: 0,
        bits: Bits::default(),
        width: 0,
        height: 0,
        img1: None,
        img3: None,
        black: None,
        ri: 0,
        n_comp: 0,
        baseline: false,
        progressive: false,
        jfif: false,
        adobe_transform_valid: false,
        adobe_transform: 0,
        eob_run: 0,
        comp: [Component::default(); MAX_COMPONENTS],
        prog_coeffs: Default::default(),
        huff: Default::default(),
        quant: [[0; BLOCK_SIZE]; (MAX_TQ + 1) as usize],
        tmp: [0; 2 * BLOCK_SIZE],
    };
    d.decode()
}

impl Decoder<'_> {
    // -----------------------------------------------------------------------------------
    // Bytes (reader.go)

    // -----------------------------------------------------------------------------------
    // Markers (reader.go)

    /// `decode` (not `configOnly`).
    fn decode(&mut self) -> Result<GoJpeg> {
        self.read_full(0, 2)?;
        if self.tmp[0] != 0xff || self.tmp[1] != SOI_MARKER {
            return Err(JpegError::Format("missing SOI marker"));
        }
        loop {
            self.read_full(0, 2)?;
            // Extraneous data before a marker is ignored, as libjpeg does.
            while self.tmp[0] != 0xff {
                self.tmp[0] = self.tmp[1];
                self.tmp[1] = self.read_byte()?;
            }
            let mut marker = self.tmp[1];
            if marker == 0 {
                // "\xff\x00" is extraneous data.
                continue;
            }
            while marker == 0xff {
                // Fill bytes (section B.1.1.2).
                marker = self.read_byte()?;
            }
            if marker == EOI_MARKER {
                break;
            }
            if (RST0_MARKER..=RST7_MARKER).contains(&marker) {
                // A stray restart marker after the last entropy-coded segment.
                continue;
            }
            self.read_full(0, 2)?;
            let n = (i64::from(self.tmp[0]) << 8) + i64::from(self.tmp[1]) - 2;
            let n = usize::try_from(n).map_err(|_| JpegError::Format("short segment length"))?;
            match marker {
                SOF0_MARKER | SOF1_MARKER | SOF2_MARKER => {
                    self.baseline = marker == SOF0_MARKER;
                    self.progressive = marker == SOF2_MARKER;
                    self.process_sof(n)?;
                }
                DHT_MARKER => self.process_dht(n)?,
                DQT_MARKER => self.process_dqt(n)?,
                SOS_MARKER => self.process_sos(n)?,
                DRI_MARKER => self.process_dri(n)?,
                APP0_MARKER => self.process_app0(n)?,
                APP14_MARKER => self.process_app14(n)?,
                m if (APP0_MARKER..=APP15_MARKER).contains(&m) || m == COM_MARKER => {
                    self.ignore(n)?;
                }
                m if m < 0xc0 => return Err(JpegError::Format("unknown marker")),
                _ => return Err(JpegError::Unsupported("unknown marker")),
            }
        }
        if self.progressive {
            self.reconstruct_progressive_image()?;
        }
        let (width, height) = (self.width, self.height);
        if let Some((pix, stride)) = self.img1.take() {
            return Ok(GoJpeg::Gray {
                width,
                height,
                pix,
                stride,
            });
        }
        if let Some(img3) = self.img3.take() {
            if let Some((black, black_stride)) = self.black.take() {
                return self.apply_black(&img3, &black, black_stride);
            }
            if self.is_rgb() {
                return Ok(self.convert_to_rgb(&img3));
            }
            return Ok(GoJpeg::YCbCr {
                width,
                height,
                planes: YCbCrPlanes {
                    y: img3.y,
                    cb: img3.cb,
                    cr: img3.cr,
                    y_stride: img3.y_stride,
                    c_stride: img3.c_stride,
                    sub: img3.sub,
                },
            });
        }
        Err(JpegError::Format("missing SOS marker"))
    }

    /// `applyBlack`: the planes and the black channel as an `*image.CMYK` (Adobe CMYK
    /// JPEGs are inverted; YCbCrK ones are converted to RGB first, then inverted).
    fn apply_black(&self, img3: &Planes3, black: &[u8], black_stride: usize) -> Result<GoJpeg> {
        if !self.adobe_transform_valid {
            return Err(JpegError::Unsupported(
                "unknown color model: 4-component JPEG doesn't have Adobe APP14 metadata",
            ));
        }
        let (width, height) = (self.width, self.height);
        let mut pix = vec![0u8; width * height * 4];
        if self.adobe_transform != ADOBE_TRANSFORM_UNKNOWN {
            // YCbCrK: `imageutil.DrawYCbCr` (the planes are 4:4:4 or 4:2:0), then the
            // inverted K in the fourth channel.
            for y in 0..height {
                for x in 0..width {
                    let yy = img3.y[y * img3.y_stride + x];
                    let ci = (y / img3.sub.1) * img3.c_stride + x / img3.sub.0;
                    let [r, g, b] = ycbcr_to_rgb(yy, img3.cb[ci], img3.cr[ci]);
                    let o = (y * width + x) * 4;
                    pix[o..o + 3].copy_from_slice(&[r, g, b]);
                    pix[o + 3] = 255 - black[y * black_stride + x];
                }
            }
        } else {
            let sources: [(&[u8], usize); 4] = [
                (&img3.y, img3.y_stride),
                (&img3.cb, img3.c_stride),
                (&img3.cr, img3.c_stride),
                (black, black_stride),
            ];
            for (t, (src, stride)) in sources.into_iter().enumerate() {
                let subsample =
                    self.comp[t].h != self.comp[0].h || self.comp[t].v != self.comp[0].v;
                for y in 0..height {
                    let sy = if subsample { y / 2 } else { y };
                    for x in 0..width {
                        let sx = if subsample { x / 2 } else { x };
                        pix[(y * width + x) * 4 + t] = 255 - src[sy * stride + sx];
                    }
                }
            }
        }
        Ok(GoJpeg::Cmyk { width, height, pix })
    }

    /// `isRGB`.
    fn is_rgb(&self) -> bool {
        if self.jfif {
            return false;
        }
        if self.adobe_transform_valid && self.adobe_transform == ADOBE_TRANSFORM_UNKNOWN {
            return true;
        }
        self.comp[0].c == b'R' && self.comp[1].c == b'G' && self.comp[2].c == b'B'
    }

    /// `convertToRGB`: the three planes are R, G and B.
    fn convert_to_rgb(&self, img3: &Planes3) -> GoJpeg {
        let c_scale = self.comp[0].h / self.comp[1].h;
        let (width, height) = (self.width, self.height);
        let mut pix = vec![0u8; width * height * 4];
        for y in 0..height {
            let yo = y * img3.y_stride;
            // `COffset` of (0, y).
            let co = (y / img3.sub.1) * img3.c_stride;
            for i in 0..width {
                let o = (y * width + i) * 4;
                pix[o] = img3.y[yo + i];
                pix[o + 1] = img3.cb[co + i / c_scale];
                pix[o + 2] = img3.cr[co + i / c_scale];
                pix[o + 3] = 255;
            }
        }
        GoJpeg::Rgba { width, height, pix }
    }

    // -----------------------------------------------------------------------------------
    // Huffman decoding (huffman.go)

    // -----------------------------------------------------------------------------------
    // Scans (scan.go)

    /// `makeImg`: the destination planes, a whole number of MCUs (Go then takes the
    /// `width`×`height` sub-image, which keeps the strides).
    fn make_img(&mut self, mxx: usize, myy: usize) {
        if self.n_comp == 1 {
            self.img1 = Some((vec![0; 8 * mxx * 8 * myy], 8 * mxx));
            return;
        }
        let (h0, v0) = (self.comp[0].h, self.comp[0].v);
        let sub = (h0 / self.comp[1].h, v0 / self.comp[1].v);
        let (w, h) = (8 * h0 * mxx, 8 * v0 * myy);
        // `yCbCrSize` of the 4:4:4, 4:4:0, 4:2:2, 4:2:0, 4:1:1 or 4:1:0 image.
        let cw = w.div_ceil(sub.0);
        let ch = h.div_ceil(sub.1);
        self.img3 = Some(Planes3 {
            y: vec![0; w * h],
            cb: vec![0; cw * ch],
            cr: vec![0; cw * ch],
            y_stride: w,
            c_stride: cw,
            sub,
        });
        if self.n_comp == 4 {
            let (h3, v3) = (self.comp[3].h, self.comp[3].v);
            self.black = Some((vec![0; 8 * h3 * mxx * 8 * v3 * myy], 8 * h3 * mxx));
        }
    }
}

/// `color.YCbCrToRGB` (as `imageutil.DrawYCbCr` inlines it).
fn ycbcr_to_rgb(y: u8, cb: u8, cr: u8) -> [u8; 3] {
    let yy1 = i32::from(y) * 0x10101;
    let cb1 = i32::from(cb) - 128;
    let cr1 = i32::from(cr) - 128;
    let clip = |v: i32| -> u8 {
        if (v as u32) & 0xff00_0000 == 0 {
            (v >> 16) as u8
        } else {
            (!(v >> 31)) as u8
        }
    };
    [
        clip(yy1 + 91881 * cr1),
        clip(yy1 - 22554 * cb1 - 46802 * cr1),
        clip(yy1 + 116_130 * cb1),
    ]
}

#[cfg(test)]
mod tests;
