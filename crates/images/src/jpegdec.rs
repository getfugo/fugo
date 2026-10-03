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

    /// `unreadByteStuffedByte`.
    fn unread_byte_stuffed_byte(&mut self) {
        self.i -= self.n_unreadable;
        self.n_unreadable = 0;
        if self.bits.n >= 8 {
            self.bits.a >>= 8;
            self.bits.n -= 8;
            self.bits.m >>= 8;
        }
    }

    /// `readByte`.
    fn read_byte(&mut self) -> Result<u8> {
        let x = *self.data.get(self.i).ok_or(JpegError::UnexpectedEof)?;
        self.i += 1;
        self.n_unreadable = 0;
        Ok(x)
    }

    /// `readByteStuffedByte`: a byte of entropy-coded data (`0xff 0x00` is `0xff`).
    fn read_byte_stuffed_byte(&mut self) -> Result<u8> {
        if self.i + 2 <= self.data.len() {
            let x = self.data[self.i];
            self.i += 1;
            self.n_unreadable = 1;
            if x != 0xff {
                return Ok(x);
            }
            if self.data[self.i] != 0x00 {
                return Err(MISSING_FF00);
            }
            self.i += 1;
            self.n_unreadable = 2;
            return Ok(0xff);
        }
        self.n_unreadable = 0;
        let x = self.read_byte()?;
        self.n_unreadable = 1;
        if x != 0xff {
            return Ok(x);
        }
        let x = self.read_byte()?;
        self.n_unreadable = 2;
        if x != 0x00 {
            return Err(MISSING_FF00);
        }
        Ok(0xff)
    }

    /// Unreads the overshot bytes of the entropy-coded data, if any (`readFull`, `ignore`).
    fn unread_overshoot(&mut self) {
        if self.n_unreadable != 0 {
            if self.bits.n >= 8 {
                self.unread_byte_stuffed_byte();
            }
            self.n_unreadable = 0;
        }
    }

    /// `readFull` into `tmp[from..to]`.
    fn read_full(&mut self, from: usize, to: usize) -> Result<()> {
        self.unread_overshoot();
        let n = to - from;
        let src = self
            .data
            .get(self.i..self.i + n)
            .ok_or(JpegError::UnexpectedEof)?;
        self.tmp[from..to].copy_from_slice(src);
        self.i += n;
        Ok(())
    }

    /// `readFull` into a Huffman table's values.
    fn read_vals(&mut self, tc: usize, th: usize, n: usize) -> Result<()> {
        self.unread_overshoot();
        let src = self
            .data
            .get(self.i..self.i + n)
            .ok_or(JpegError::UnexpectedEof)?;
        self.huff[tc][th].vals[..n].copy_from_slice(src);
        self.i += n;
        Ok(())
    }

    /// `ignore`.
    fn ignore(&mut self, n: usize) -> Result<()> {
        self.unread_overshoot();
        if self.i + n > self.data.len() {
            self.i = self.data.len();
            return Err(JpegError::UnexpectedEof);
        }
        self.i += n;
        Ok(())
    }

    // -----------------------------------------------------------------------------------
    // Markers (reader.go)

    /// `processSOF` (section B.2.2).
    fn process_sof(&mut self, n: usize) -> Result<()> {
        if self.n_comp != 0 {
            return Err(JpegError::Format("multiple SOF markers"));
        }
        self.n_comp = match n {
            9 => 1,
            15 => 3,
            18 => 4,
            _ => return Err(JpegError::Unsupported("number of components")),
        };
        self.read_full(0, n)?;
        if self.tmp[0] != 8 {
            return Err(JpegError::Unsupported("precision"));
        }
        self.height = (usize::from(self.tmp[1]) << 8) + usize::from(self.tmp[2]);
        self.width = (usize::from(self.tmp[3]) << 8) + usize::from(self.tmp[4]);
        if usize::from(self.tmp[5]) != self.n_comp {
            return Err(JpegError::Format("SOF has wrong length"));
        }
        for i in 0..self.n_comp {
            self.comp[i].c = self.tmp[6 + 3 * i];
            for j in 0..i {
                if self.comp[i].c == self.comp[j].c {
                    return Err(JpegError::Format("repeated component identifier"));
                }
            }
            self.comp[i].tq = self.tmp[8 + 3 * i];
            if self.comp[i].tq > MAX_TQ {
                return Err(JpegError::Format("bad Tq value"));
            }
            let hv = self.tmp[7 + 3 * i];
            let (mut h, mut v) = (usize::from(hv >> 4), usize::from(hv & 0x0f));
            if !(1..=4).contains(&h) || !(1..=4).contains(&v) {
                return Err(JpegError::Format("luma/chroma subsampling ratio"));
            }
            if h == 3 || v == 3 {
                return Err(UNSUPPORTED_SUBSAMPLING);
            }
            match self.n_comp {
                // A single component is non-interleaved: its (h, v) is effectively (1, 1).
                1 => (h, v) = (1, 1),
                3 => match i {
                    0 if v == 4 => return Err(UNSUPPORTED_SUBSAMPLING),
                    1 if !self.comp[0].h.is_multiple_of(h) || !self.comp[0].v.is_multiple_of(v) => {
                        return Err(UNSUPPORTED_SUBSAMPLING);
                    }
                    2 if self.comp[1].h != h || self.comp[1].v != v => {
                        return Err(UNSUPPORTED_SUBSAMPLING);
                    }
                    _ => {}
                },
                _ => match i {
                    0 if hv != 0x11 && hv != 0x22 => return Err(UNSUPPORTED_SUBSAMPLING),
                    1 | 2 if hv != 0x11 => return Err(UNSUPPORTED_SUBSAMPLING),
                    3 if self.comp[0].h != h || self.comp[0].v != v => {
                        return Err(UNSUPPORTED_SUBSAMPLING);
                    }
                    _ => {}
                },
            }
            self.comp[i].h = h;
            self.comp[i].v = v;
        }
        Ok(())
    }

    /// `processDQT` (section B.2.4.1).
    fn process_dqt(&mut self, mut n: usize) -> Result<()> {
        while n > 0 {
            n -= 1;
            let x = self.read_byte()?;
            let tq = usize::from(x & 0x0f);
            if tq > usize::from(MAX_TQ) {
                return Err(JpegError::Format("bad Tq value"));
            }
            match x >> 4 {
                0 => {
                    if n < BLOCK_SIZE {
                        break;
                    }
                    n -= BLOCK_SIZE;
                    self.read_full(0, BLOCK_SIZE)?;
                    for i in 0..BLOCK_SIZE {
                        self.quant[tq][i] = i32::from(self.tmp[i]);
                    }
                }
                1 => {
                    if n < 2 * BLOCK_SIZE {
                        break;
                    }
                    n -= 2 * BLOCK_SIZE;
                    self.read_full(0, 2 * BLOCK_SIZE)?;
                    for i in 0..BLOCK_SIZE {
                        self.quant[tq][i] =
                            (i32::from(self.tmp[2 * i]) << 8) | i32::from(self.tmp[2 * i + 1]);
                    }
                }
                _ => return Err(JpegError::Format("bad Pq value")),
            }
        }
        if n != 0 {
            return Err(JpegError::Format("DQT has wrong length"));
        }
        Ok(())
    }

    /// `processDRI` (section B.2.4.4).
    fn process_dri(&mut self, n: usize) -> Result<()> {
        if n != 2 {
            return Err(JpegError::Format("DRI has wrong length"));
        }
        self.read_full(0, 2)?;
        self.ri = (usize::from(self.tmp[0]) << 8) + usize::from(self.tmp[1]);
        Ok(())
    }

    /// `processApp0Marker`: notes a JFIF header.
    fn process_app0(&mut self, mut n: usize) -> Result<()> {
        if n < 5 {
            return self.ignore(n);
        }
        self.read_full(0, 5)?;
        n -= 5;
        self.jfif = &self.tmp[..5] == b"JFIF\0";
        if n > 0 {
            return self.ignore(n);
        }
        Ok(())
    }

    /// `processApp14Marker`: notes an Adobe colour transform.
    fn process_app14(&mut self, mut n: usize) -> Result<()> {
        if n < 12 {
            return self.ignore(n);
        }
        self.read_full(0, 12)?;
        n -= 12;
        if &self.tmp[..5] == b"Adobe" {
            self.adobe_transform_valid = true;
            self.adobe_transform = self.tmp[11];
        }
        if n > 0 {
            return self.ignore(n);
        }
        Ok(())
    }

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

    /// `ensureNBits`.
    fn ensure_n_bits(&mut self, n: i32) -> Result<()> {
        loop {
            let c = match self.read_byte_stuffed_byte() {
                Ok(c) => c,
                Err(JpegError::UnexpectedEof) => return Err(SHORT_HUFFMAN_DATA),
                Err(e) => return Err(e),
            };
            self.bits.a = (self.bits.a << 8) | u32::from(c);
            self.bits.n += 8;
            if self.bits.m == 0 {
                self.bits.m = 1 << 7;
            } else {
                self.bits.m <<= 8;
            }
            if self.bits.n >= n {
                return Ok(());
            }
        }
    }

    /// `receiveExtend` (section F.2.2.1).
    fn receive_extend(&mut self, t: u8) -> Result<i32> {
        if self.bits.n < i32::from(t) {
            self.ensure_n_bits(i32::from(t))?;
        }
        self.bits.n -= i32::from(t);
        self.bits.m >>= t;
        let s = 1i32 << t;
        let mut x = (self.bits.a >> self.bits.n) as i32 & (s - 1);
        if x < s >> 1 {
            x += (-1i32 << t) + 1;
        }
        Ok(x)
    }

    /// `processDHT` (section B.2.4.2).
    fn process_dht(&mut self, n: usize) -> Result<()> {
        let mut n = n as i64;
        while n > 0 {
            if n < 17 {
                return Err(JpegError::Format("DHT has wrong length"));
            }
            self.read_full(0, 17)?;
            let tc = self.tmp[0] >> 4;
            if tc > MAX_TC {
                return Err(JpegError::Format("bad Tc value"));
            }
            let th = self.tmp[0] & 0x0f;
            if th > MAX_TH || (self.baseline && th > 1) {
                return Err(JpegError::Format("bad Th value"));
            }
            let (tc, th) = (usize::from(tc), usize::from(th));
            let mut n_codes = [0i32; MAX_CODE_LENGTH];
            let mut total = 0i32;
            for (i, c) in n_codes.iter_mut().enumerate() {
                *c = i32::from(self.tmp[i + 1]);
                total += *c;
            }
            self.huff[tc][th].n_codes = total;
            if total == 0 {
                return Err(JpegError::Format("Huffman table has zero length"));
            }
            if total > MAX_N_CODES as i32 {
                return Err(JpegError::Format("Huffman table has excessive length"));
            }
            n -= i64::from(total) + 17;
            if n < 0 {
                return Err(JpegError::Format("DHT has wrong length"));
            }
            self.read_vals(tc, th, total as usize)?;
            let h = &mut self.huff[tc][th];
            // The look-up table.
            h.lut = [0; 1 << LUT_SIZE];
            let (mut x, mut code) = (0usize, 0u32);
            for i in 0..LUT_SIZE {
                code <<= 1;
                for _ in 0..n_codes[i as usize] {
                    let base = (code << (7 - i)) as u8;
                    let lut_value = (u16::from(h.vals[x]) << 8) | (2 + i) as u16;
                    for k in 0..1u16 << (7 - i) {
                        h.lut[usize::from(base) | usize::from(k)] = lut_value;
                    }
                    code += 1;
                    x += 1;
                }
            }
            // minCodes, maxCodes and valsIndices.
            let (mut c, mut index) = (0i32, 0i32);
            for (i, &nc) in n_codes.iter().enumerate() {
                if nc == 0 {
                    h.min_codes[i] = -1;
                    h.max_codes[i] = -1;
                    h.vals_indices[i] = -1;
                } else {
                    h.min_codes[i] = c;
                    h.max_codes[i] = c + nc - 1;
                    h.vals_indices[i] = index;
                    c += nc;
                    index += nc;
                }
                c <<= 1;
            }
        }
        Ok(())
    }

    /// `decodeHuffman`: the next value coded with table `(tc, th)`.
    fn decode_huffman(&mut self, tc: usize, th: usize) -> Result<u8> {
        if self.huff[tc][th].n_codes == 0 {
            return Err(JpegError::Format("uninitialized Huffman table"));
        }
        let mut slow = false;
        if self.bits.n < 8
            && let Err(e) = self.ensure_n_bits(8)
        {
            if e != MISSING_FF00 && e != SHORT_HUFFMAN_DATA {
                return Err(e);
            }
            // No more bytes in this segment, but the bits already read may still hold the
            // next symbol: undo the overshoot first.
            if self.n_unreadable != 0 {
                self.unread_byte_stuffed_byte();
            }
            slow = true;
        }
        if !slow {
            let idx = ((self.bits.a >> (self.bits.n - LUT_SIZE as i32)) & 0xff) as usize;
            let v = self.huff[tc][th].lut[idx];
            if v != 0 {
                let n = (v & 0xff) - 1;
                self.bits.n -= i32::from(n);
                self.bits.m >>= n;
                return Ok((v >> 8) as u8);
            }
        }
        let mut code = 0i32;
        for i in 0..MAX_CODE_LENGTH {
            if self.bits.n == 0 {
                self.ensure_n_bits(1)?;
            }
            if self.bits.a & self.bits.m != 0 {
                code |= 1;
            }
            self.bits.n -= 1;
            self.bits.m >>= 1;
            let h = &self.huff[tc][th];
            if code <= h.max_codes[i] {
                return Ok(h.vals[(h.vals_indices[i] + code - h.min_codes[i]) as usize]);
            }
            code <<= 1;
        }
        Err(JpegError::Format("bad Huffman code"))
    }

    /// `decodeBit`.
    fn decode_bit(&mut self) -> Result<bool> {
        if self.bits.n == 0 {
            self.ensure_n_bits(1)?;
        }
        let ret = self.bits.a & self.bits.m != 0;
        self.bits.n -= 1;
        self.bits.m >>= 1;
        Ok(ret)
    }

    /// `decodeBits`.
    fn decode_bits(&mut self, n: i32) -> Result<u32> {
        if self.bits.n < n {
            self.ensure_n_bits(n)?;
        }
        let mut ret = self.bits.a >> (self.bits.n - n);
        ret &= (1u32 << n) - 1;
        self.bits.n -= n;
        self.bits.m >>= n;
        Ok(ret)
    }

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

    /// `processSOS` (section B.2.3).
    fn process_sos(&mut self, n: usize) -> Result<()> {
        if self.n_comp == 0 {
            return Err(JpegError::Format("missing SOF marker"));
        }
        if n < 6 || 4 + 2 * self.n_comp < n || !n.is_multiple_of(2) {
            return Err(JpegError::Format("SOS has wrong length"));
        }
        self.read_full(0, n)?;
        let n_comp = usize::from(self.tmp[0]);
        if n != 4 + 2 * n_comp {
            return Err(JpegError::Format(
                "SOS length inconsistent with number of components",
            ));
        }
        // (component index, DC table, AC table) of each scan component.
        let mut scan = [(0usize, 0usize, 0usize); MAX_COMPONENTS];
        let mut total_hv = 0;
        for i in 0..n_comp {
            let cs = self.tmp[1 + 2 * i];
            let comp_index = (0..self.n_comp)
                .rev()
                .find(|&j| self.comp[j].c == cs)
                .ok_or(JpegError::Format("unknown component selector"))?;
            scan[i].0 = comp_index;
            if scan[..i].iter().any(|s| s.0 == comp_index) {
                return Err(JpegError::Format("repeated component selector"));
            }
            total_hv += self.comp[comp_index].h * self.comp[comp_index].v;
            let td = self.tmp[2 + 2 * i] >> 4;
            if td > MAX_TH || (self.baseline && td > 1) {
                return Err(JpegError::Format("bad Td value"));
            }
            let ta = self.tmp[2 + 2 * i] & 0x0f;
            if ta > MAX_TH || (self.baseline && ta > 1) {
                return Err(JpegError::Format("bad Ta value"));
            }
            scan[i].1 = usize::from(td);
            scan[i].2 = usize::from(ta);
        }
        if self.n_comp > 1 && total_hv > 10 {
            return Err(JpegError::Format("total sampling factors too large"));
        }
        // Spectral selection and successive approximation (fixed for sequential JPEGs).
        let (mut zig_start, mut zig_end, mut ah, mut al) =
            (0i32, BLOCK_SIZE as i32 - 1, 0u32, 0u32);
        if self.progressive {
            zig_start = i32::from(self.tmp[1 + 2 * n_comp]);
            zig_end = i32::from(self.tmp[2 + 2 * n_comp]);
            ah = u32::from(self.tmp[3 + 2 * n_comp] >> 4);
            al = u32::from(self.tmp[3 + 2 * n_comp] & 0x0f);
            if (zig_start == 0 && zig_end != 0)
                || zig_start > zig_end
                || BLOCK_SIZE as i32 <= zig_end
            {
                return Err(JpegError::Format("bad spectral selection bounds"));
            }
            if zig_start != 0 && n_comp != 1 {
                return Err(JpegError::Format(
                    "progressive AC coefficients for more than one component",
                ));
            }
            if ah != 0 && ah != al + 1 {
                return Err(JpegError::Format("bad successive approximation values"));
            }
        }
        // The number of MCUs.
        let (h0, v0) = (self.comp[0].h, self.comp[0].v);
        let mxx = self.width.div_ceil(8 * h0);
        let myy = self.height.div_ceil(8 * v0);
        if self.img1.is_none() && self.img3.is_none() {
            self.make_img(mxx, myy);
        }
        if self.progressive {
            for s in &scan[..n_comp] {
                let ci = s.0;
                if self.prog_coeffs[ci].is_empty() {
                    self.prog_coeffs[ci] =
                        vec![[0; BLOCK_SIZE]; mxx * myy * self.comp[ci].h * self.comp[ci].v];
                }
            }
        }

        self.bits = Bits::default();
        let (mut mcu, mut expected_rst) = (0usize, RST0_MARKER);
        let mut dc = [0i32; MAX_COMPONENTS];
        let mut block_count = 0usize;
        for my in 0..myy {
            for mx in 0..mxx {
                for &(comp_index, td, ta) in &scan[..n_comp] {
                    let hi = self.comp[comp_index].h;
                    let vi = self.comp[comp_index].v;
                    for j in 0..hi * vi {
                        // Interleaved scans go MCU by MCU; non-interleaved ones left to
                        // right, top to bottom, skipping blocks outside the image.
                        let (bx, by);
                        if n_comp != 1 {
                            bx = hi * mx + j % hi;
                            by = vi * my + j / hi;
                        } else {
                            let q = mxx * hi;
                            bx = block_count % q;
                            by = block_count / q;
                            block_count += 1;
                            if bx * 8 >= self.width || by * 8 >= self.height {
                                continue;
                            }
                        }
                        let mut b: Block = if self.progressive {
                            self.prog_coeffs[comp_index][by * mxx * hi + bx]
                        } else {
                            [0; BLOCK_SIZE]
                        };
                        if ah != 0 {
                            self.refine(&mut b, ta, zig_start, zig_end, 1 << al)?;
                        } else {
                            let mut zig = zig_start;
                            if zig == 0 {
                                zig += 1;
                                // The DC coefficient (section F.2.2.1).
                                let value = self.decode_huffman(DC_TABLE, td)?;
                                if value > 16 {
                                    return Err(JpegError::Unsupported("excessive DC component"));
                                }
                                let dc_delta = self.receive_extend(value)?;
                                dc[comp_index] = dc[comp_index].wrapping_add(dc_delta);
                                b[0] = dc[comp_index] << al;
                            }
                            if zig <= zig_end && self.eob_run > 0 {
                                self.eob_run -= 1;
                            } else {
                                // The AC coefficients (section F.2.2.2).
                                while zig <= zig_end {
                                    let value = self.decode_huffman(AC_TABLE, ta)?;
                                    let val0 = value >> 4;
                                    let val1 = value & 0x0f;
                                    if val1 != 0 {
                                        zig += i32::from(val0);
                                        if zig > zig_end {
                                            break;
                                        }
                                        let ac = self.receive_extend(val1)?;
                                        b[UNZIG[zig as usize]] = ac << al;
                                    } else {
                                        if val0 != 0x0f {
                                            self.eob_run = 1u16 << val0;
                                            if val0 != 0 {
                                                let bits = self.decode_bits(i32::from(val0))?;
                                                self.eob_run |= bits as u16;
                                            }
                                            self.eob_run = self.eob_run.wrapping_sub(1);
                                            break;
                                        }
                                        zig += 0x0f;
                                    }
                                    zig += 1;
                                }
                            }
                        }
                        if self.progressive {
                            // Reconstructed after the last scan.
                            self.prog_coeffs[comp_index][by * mxx * hi + bx] = b;
                            continue;
                        }
                        self.reconstruct_block(&mut b, bx, by, comp_index)?;
                    }
                }
                mcu += 1;
                if self.ri > 0 && mcu % self.ri == 0 && mcu < mxx * myy {
                    // The RST marker should follow; resynchronise on corrupt input.
                    self.read_full(0, 2)?;
                    if self.tmp[0] != 0xff || self.tmp[1] != expected_rst {
                        self.find_rst(expected_rst)?;
                    }
                    expected_rst += 1;
                    if expected_rst == RST7_MARKER + 1 {
                        expected_rst = RST0_MARKER;
                    }
                    self.bits = Bits::default();
                    dc = [0; MAX_COMPONENTS];
                    self.eob_run = 0;
                }
            }
        }
        Ok(())
    }

    /// `refine`: a successive approximation refinement (section G.1.2).
    fn refine(
        &mut self,
        b: &mut Block,
        ta: usize,
        zig_start: i32,
        zig_end: i32,
        delta: i32,
    ) -> Result<()> {
        if zig_start == 0 {
            // DC refinement (zigEnd is 0, checked above).
            if self.decode_bit()? {
                b[0] |= delta;
            }
            return Ok(());
        }
        let mut zig = zig_start;
        if self.eob_run == 0 {
            while zig <= zig_end {
                let mut z = 0i32;
                let value = self.decode_huffman(AC_TABLE, ta)?;
                let val0 = value >> 4;
                let val1 = value & 0x0f;
                match val1 {
                    0 => {
                        if val0 != 0x0f {
                            self.eob_run = 1u16 << val0;
                            if val0 != 0 {
                                let bits = self.decode_bits(i32::from(val0))?;
                                self.eob_run |= bits as u16;
                            }
                            break;
                        }
                    }
                    1 => {
                        z = delta;
                        if !self.decode_bit()? {
                            z = -z;
                        }
                    }
                    _ => return Err(JpegError::Format("unexpected Huffman code")),
                }
                zig = self.refine_non_zeroes(b, zig, zig_end, i32::from(val0), delta)?;
                if zig > zig_end {
                    return Err(JpegError::Format("too many coefficients"));
                }
                if z != 0 {
                    b[UNZIG[zig as usize]] = z;
                }
                zig += 1;
            }
        }
        if self.eob_run > 0 {
            self.eob_run -= 1;
            self.refine_non_zeroes(b, zig, zig_end, -1, delta)?;
        }
        Ok(())
    }

    /// `refineNonZeroes`: refines the non-zero coefficients in zig-zag order, skipping the
    /// first `nz` zero ones when `nz >= 0`.
    fn refine_non_zeroes(
        &mut self,
        b: &mut Block,
        mut zig: i32,
        zig_end: i32,
        mut nz: i32,
        delta: i32,
    ) -> Result<i32> {
        while zig <= zig_end {
            let u = UNZIG[zig as usize];
            if b[u] == 0 {
                if nz == 0 {
                    break;
                }
                nz -= 1;
                zig += 1;
                continue;
            }
            if self.decode_bit()? {
                if b[u] >= 0 {
                    b[u] = b[u].wrapping_add(delta);
                } else {
                    b[u] = b[u].wrapping_sub(delta);
                }
            }
            zig += 1;
        }
        Ok(zig)
    }

    /// `reconstructProgressiveImage`.
    fn reconstruct_progressive_image(&mut self) -> Result<()> {
        let h0 = self.comp[0].h;
        let mxx = self.width.div_ceil(8 * h0);
        for i in 0..self.n_comp {
            if self.prog_coeffs[i].is_empty() {
                continue;
            }
            let v = 8 * self.comp[0].v / self.comp[i].v;
            let h = 8 * self.comp[0].h / self.comp[i].h;
            let stride = mxx * self.comp[i].h;
            let mut coeffs = std::mem::take(&mut self.prog_coeffs[i]);
            let mut by = 0;
            while by * v < self.height {
                let mut bx = 0;
                while bx * h < self.width {
                    self.reconstruct_block(&mut coeffs[by * stride + bx], bx, by, i)?;
                    bx += 1;
                }
                by += 1;
            }
            self.prog_coeffs[i] = coeffs;
        }
        Ok(())
    }

    /// `reconstructBlock`: dequantises, inverse-transforms and stores a block, level-shifted
    /// by 128 and clipped.
    fn reconstruct_block(
        &mut self,
        b: &mut Block,
        bx: usize,
        by: usize,
        comp_index: usize,
    ) -> Result<()> {
        let qt = &self.quant[usize::from(self.comp[comp_index].tq)];
        for zig in 0..BLOCK_SIZE {
            b[UNZIG[zig]] = b[UNZIG[zig]].wrapping_mul(qt[zig]);
        }
        idct(b);
        let (dst, stride): (&mut [u8], usize) = if self.n_comp == 1 {
            let (pix, stride) = self
                .img1
                .as_mut()
                .ok_or(JpegError::Format("missing SOS marker"))?;
            (pix.as_mut_slice(), *stride)
        } else {
            let img3 = self
                .img3
                .as_mut()
                .ok_or(JpegError::Format("missing SOS marker"))?;
            match comp_index {
                0 => (img3.y.as_mut_slice(), img3.y_stride),
                1 => (img3.cb.as_mut_slice(), img3.c_stride),
                2 => (img3.cr.as_mut_slice(), img3.c_stride),
                3 => {
                    let (pix, stride) = self
                        .black
                        .as_mut()
                        .ok_or(JpegError::Unsupported("too many components"))?;
                    (pix.as_mut_slice(), *stride)
                }
                _ => return Err(JpegError::Unsupported("too many components")),
            }
        };
        let base = 8 * (by * stride + bx);
        for y in 0..8 {
            for x in 0..8 {
                let c = b[y * 8 + x];
                let v = if c < -128 {
                    0
                } else if c > 127 {
                    255
                } else {
                    (c + 128) as u8
                };
                // Go panics on blocks beyond the planes (corrupt sampling factors).
                let slot = dst
                    .get_mut(base + y * stride + x)
                    .ok_or(JpegError::Format("block outside the image"))?;
                *slot = v;
            }
        }
        Ok(())
    }

    /// `findRST`: skips to the expected restart marker; any other marker is an error.
    fn find_rst(&mut self, expected_rst: u8) -> Result<()> {
        loop {
            let mut i = 0;
            if self.tmp[0] == 0xff {
                if self.tmp[1] == expected_rst {
                    return Ok(());
                } else if self.tmp[1] == 0xff {
                    i = 1;
                } else if self.tmp[1] != 0x00 {
                    return Err(JpegError::Format("bad RST marker"));
                }
            } else if self.tmp[1] == 0xff {
                self.tmp[0] = 0xff;
                i = 1;
            }
            self.read_full(i, 2)?;
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

// `idct.go` (Go 1.25): the MPEG Software Simulation Group's integer IDCT, from mpeg2decode's
// `idct.c` ("These software programs are available to the user without any license fee or
// royalty on an 'as is' basis"), as the Go Authors translated it.
const W1: i32 = 2841;
const W2: i32 = 2676;
const W3: i32 = 2408;
const W5: i32 = 1609;
const W6: i32 = 1108;
const W7: i32 = 565;
const W1PW7: i32 = W1 + W7;
const W1MW7: i32 = W1 - W7;
const W2PW6: i32 = W2 + W6;
const W2MW6: i32 = W2 - W6;
const W3PW5: i32 = W3 + W5;
const W3MW5: i32 = W3 - W5;
const R2: i32 = 181;

/// `idct`: the 2-D inverse DCT in fixed point, rows then columns. Go's int32 arithmetic
/// wraps, so every operation here wraps too.
fn idct(src: &mut Block) {
    use std::num::Wrapping as W;
    let w = W;
    // Horizontal 1-D IDCT.
    for y in 0..8 {
        let s = &mut src[y * 8..y * 8 + 8];
        if s[1..].iter().all(|&v| v == 0) {
            let dc = s[0] << 3;
            s.fill(dc);
            continue;
        }
        let mut x0 = (w(s[0]) << 11) + w(128);
        let mut x1 = w(s[4]) << 11;
        let mut x2 = w(s[6]);
        let mut x3 = w(s[2]);
        let mut x4 = w(s[1]);
        let mut x5 = w(s[7]);
        let mut x6 = w(s[5]);
        let mut x7 = w(s[3]);
        // Stage 1.
        let mut x8 = w(W7) * (x4 + x5);
        x4 = x8 + w(W1MW7) * x4;
        x5 = x8 - w(W1PW7) * x5;
        x8 = w(W3) * (x6 + x7);
        x6 = x8 - w(W3MW5) * x6;
        x7 = x8 - w(W3PW5) * x7;
        // Stage 2.
        x8 = x0 + x1;
        x0 -= x1;
        x1 = w(W6) * (x3 + x2);
        x2 = x1 - w(W2PW6) * x2;
        x3 = x1 + w(W2MW6) * x3;
        x1 = x4 + x6;
        x4 -= x6;
        x6 = x5 + x7;
        x5 -= x7;
        // Stage 3.
        x7 = x8 + x3;
        x8 -= x3;
        x3 = x0 + x2;
        x0 -= x2;
        x2 = (w(R2) * (x4 + x5) + w(128)) >> 8;
        x4 = (w(R2) * (x4 - x5) + w(128)) >> 8;
        // Stage 4.
        s[0] = ((x7 + x1) >> 8).0;
        s[1] = ((x3 + x2) >> 8).0;
        s[2] = ((x0 + x4) >> 8).0;
        s[3] = ((x8 + x6) >> 8).0;
        s[4] = ((x8 - x6) >> 8).0;
        s[5] = ((x0 - x4) >> 8).0;
        s[6] = ((x3 - x2) >> 8).0;
        s[7] = ((x7 - x1) >> 8).0;
    }
    // Vertical 1-D IDCT.
    for x in 0..8 {
        let at = |r: usize| w(src[8 * r + x]);
        let mut y0 = (at(0) << 8) + w(8192);
        let mut y1 = at(4) << 8;
        let mut y2 = at(6);
        let mut y3 = at(2);
        let mut y4 = at(1);
        let mut y5 = at(7);
        let mut y6 = at(5);
        let mut y7 = at(3);
        // Stage 1.
        let mut y8 = w(W7) * (y4 + y5) + w(4);
        y4 = (y8 + w(W1MW7) * y4) >> 3;
        y5 = (y8 - w(W1PW7) * y5) >> 3;
        y8 = w(W3) * (y6 + y7) + w(4);
        y6 = (y8 - w(W3MW5) * y6) >> 3;
        y7 = (y8 - w(W3PW5) * y7) >> 3;
        // Stage 2.
        y8 = y0 + y1;
        y0 -= y1;
        y1 = w(W6) * (y3 + y2) + w(4);
        y2 = (y1 - w(W2PW6) * y2) >> 3;
        y3 = (y1 + w(W2MW6) * y3) >> 3;
        y1 = y4 + y6;
        y4 -= y6;
        y6 = y5 + y7;
        y5 -= y7;
        // Stage 3.
        y7 = y8 + y3;
        y8 -= y3;
        y3 = y0 + y2;
        y0 -= y2;
        y2 = (w(R2) * (y4 + y5) + w(128)) >> 8;
        y4 = (w(R2) * (y4 - y5) + w(128)) >> 8;
        // Stage 4.
        let out = [
            (y7 + y1) >> 14,
            (y3 + y2) >> 14,
            (y0 + y4) >> 14,
            (y8 + y6) >> 14,
            (y8 - y6) >> 14,
            (y0 - y4) >> 14,
            (y3 - y2) >> 14,
            (y7 - y1) >> 14,
        ];
        for (r, v) in out.into_iter().enumerate() {
            src[8 * r + x] = v.0;
        }
    }
}

#[cfg(test)]
mod tests {
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
                Ok(GoJpeg::Rgba { pix, .. } | GoJpeg::Cmyk { pix, .. }) => {
                    (vec![fnv1a64(&pix)], None)
                }
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
}
