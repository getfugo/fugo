//! QR codes (`images.QR`): the PNG image of a QR code, equal byte for byte to the one the Go
//! implementation makes with `rsc.io/qr` v0.2.0.
//!
//! The symbol is built with the `qrcode` crate (ISO/IEC 18004: data bits, Reed–Solomon
//! blocks, placement, format and version information) under the choices `rsc.io/qr` makes,
//! which the standard leaves to the encoder:
//!
//! * the whole text is one segment, in numeric mode when it is all digits, else alphanumeric
//!   when every character is in that set, else byte mode (`qrcode`'s optimal segmentation is
//!   not used: it could pick a smaller version);
//! * the version is the smallest whose data capacity at the level holds that segment;
//! * the mask is always pattern 0 (`(row + column) mod 2`; `rsc.io/qr` never evaluates the
//!   eight masks, `qrcode` would pick the lowest-penalty one).
//!
//! The PNG is written as `rsc.io/qr` writes it (BSD-3-Clause, The Go Authors;
//! `THIRD_PARTY/rsc-qr/LICENSE`): a 4-module white border, `scale` pixels per module,
//! 1-bit greyscale, a `tEXt` chunk naming the software, and one deflate block with fixed
//! Huffman codes that writes each pixel row once and repeats it by back-reference.

use qrcode::bits::Bits;
use qrcode::canvas::{Canvas, MaskPattern};
use qrcode::types::{Color as Module, EcLevel, Version};

use crate::error::ImageError;

named_enum! {
    /// The error correction level of a QR code: about 7, 15, 25 and 30 % of the symbol can be
    /// restored.
    pub enum QrLevel ("QR code error correction level") {
        Low = "low",
        Medium = "medium",
        Quartile = "quartile",
        High = "high",
    }
}

impl QrLevel {
    const fn ec(self) -> EcLevel {
        match self {
            Self::Low => EcLevel::L,
            Self::Medium => EcLevel::M,
            Self::Quartile => EcLevel::Q,
            Self::High => EcLevel::H,
        }
    }
}

/// The modules of a QR code symbol: `size × size`, row-major, `true` for dark.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QrModules {
    pub size: usize,
    pub dark: Vec<bool>,
}

impl QrModules {
    /// Whether the module at `(x, y)` is dark (outside the symbol: light).
    #[must_use]
    pub fn is_dark(&self, x: i64, y: i64) -> bool {
        let (Ok(x), Ok(y)) = (usize::try_from(x), usize::try_from(y)) else {
            return false;
        };
        x < self.size && y < self.size && self.dark[y * self.size + x]
    }
}

const ALPHANUMERIC: &str = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:";

#[derive(Clone, Copy)]
enum Mode {
    Numeric,
    Alphanumeric,
    Byte,
}

impl Mode {
    fn of(text: &str) -> Self {
        if text.chars().all(|c| c.is_ascii_digit()) {
            Self::Numeric
        } else if text.chars().all(|c| ALPHANUMERIC.contains(c)) {
            Self::Alphanumeric
        } else {
            Self::Byte
        }
    }

    /// The data bits of `text` as one segment of this mode in `version`, if they fit the
    /// version's capacity at `ec`.
    fn bits(self, text: &str, version: Version, ec: EcLevel) -> Option<Bits> {
        let mut bits = Bits::new(version);
        let data = text.as_bytes();
        match self {
            Self::Numeric => bits.push_numeric_data(data),
            Self::Alphanumeric => bits.push_alphanumeric_data(data),
            Self::Byte => bits.push_byte_data(data),
        }
        .ok()?;
        (bits.len() <= bits.max_len(ec).ok()?).then_some(bits)
    }
}

/// The QR code symbol of `text` at `level`, as `rsc.io/qr` encodes it (see the module
/// documentation).
///
/// # Errors
/// Empty text, or text too long for a version 40 symbol at this level.
pub fn qr_modules(text: &str, level: QrLevel) -> Result<QrModules, ImageError> {
    if text.is_empty() {
        return Err(ImageError::Qr("cannot encode an empty text".to_owned()));
    }
    let ec = level.ec();
    let mode = Mode::of(text);
    let (version, mut bits) = (1..=40)
        .map(Version::Normal)
        .find_map(|v| mode.bits(text, v, ec).map(|b| (v, b)))
        .ok_or_else(|| {
            ImageError::Qr(format!(
                "a text of {} bytes is too long for a QR code at level {level}",
                text.len()
            ))
        })?;
    let qr_err = |e: qrcode::types::QrError| ImageError::Qr(e.to_string());
    bits.push_terminator(ec).map_err(qr_err)?;
    let (data, ecc) =
        qrcode::ec::construct_codewords(&bits.into_bytes(), version, ec).map_err(qr_err)?;
    let mut canvas = Canvas::new(version, ec);
    canvas.draw_all_functional_patterns();
    canvas.draw_data(&data, &ecc);
    canvas.apply_mask(MaskPattern::Checkerboard);
    let dark: Vec<bool> = canvas
        .into_colors()
        .into_iter()
        .map(|c| c == Module::Dark)
        .collect();
    let size = version.width().unsigned_abs().into();
    Ok(QrModules { size, dark })
}

/// The PNG of the QR code of `text` at `level` with `scale` pixels per module (at least 2),
/// byte for byte what the Go implementation writes.
///
/// # Errors
/// As [`qr_modules`], a scale below 2, or one so large that a pixel row is longer than a
/// deflate back-reference can reach (32 KiB).
pub fn qr_png(text: &str, level: QrLevel, scale: u32) -> Result<Vec<u8>, ImageError> {
    if scale < 2 {
        return Err(ImageError::Qr(format!(
            "scale {scale} must be an integer of at least 2"
        )));
    }
    let modules = qr_modules(text, level)?;
    let scale = scale as usize;
    let side = (modules.size + 8) * scale;
    if 1 + side.div_ceil(8) > 32768 {
        return Err(ImageError::Qr(format!("scale {scale} is too large")));
    }
    Ok(png(&modules, scale))
}

// ── the PNG writer of rsc.io/qr ──────────────────────────────────────────────────────────────

fn crc32(parts: &[&[u8]]) -> u32 {
    let mut crc = !0u32;
    for part in parts {
        for &b in *part {
            crc ^= u32::from(b);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    0xEDB8_8320 ^ (crc >> 1)
                } else {
                    crc >> 1
                };
            }
        }
    }
    !crc
}

fn chunk(out: &mut Vec<u8>, name: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&u32::try_from(data.len()).unwrap_or(u32::MAX).to_be_bytes());
    out.extend_from_slice(name);
    out.extend_from_slice(data);
    out.extend_from_slice(&crc32(&[name, data]).to_be_bytes());
}

fn png(m: &QrModules, scale: usize) -> Vec<u8> {
    let side = u32::try_from((m.size + 8) * scale).unwrap_or(u32::MAX);
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&side.to_be_bytes());
    ihdr.extend_from_slice(&side.to_be_bytes());
    // 1-bit greyscale, deflate, no filter, not interlaced.
    ihdr.extend_from_slice(&[1, 0, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(
        &mut out,
        b"tEXt",
        b"Software\x00QR-PNG http://qr.swtch.com/",
    );
    chunk(&mut out, b"IDAT", &zlib(m, scale));
    chunk(&mut out, b"IEND", &[]);
    out
}

/// A deflate bit stream (bits packed from the least significant bit).
#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    bit: u32,
    nbit: u32,
}

impl BitWriter {
    /// Appends the low `nbit` bits of `bit`; `rev` for Huffman codes (most significant bit
    /// first).
    fn write_bits(&mut self, bit: u32, nbit: u32, rev: bool) {
        let bit = if rev {
            (0..nbit).fold(0, |acc, i| acc | ((bit >> i) & 1) << (nbit - 1 - i))
        } else {
            bit
        };
        self.bit |= bit << self.nbit;
        self.nbit += nbit;
        while self.nbit >= 8 {
            self.bytes.push(self.bit.to_le_bytes()[0]);
            self.bit >>= 8;
            self.nbit -= 8;
        }
    }

    fn flush(&mut self) {
        if self.nbit > 0 {
            self.bytes.push(self.bit.to_le_bytes()[0]);
            self.nbit = 0;
            self.bit = 0;
        }
    }

    /// A literal/length symbol in the fixed Huffman code (RFC 1951, 3.2.6).
    fn hcode(&mut self, v: u32) {
        match v {
            0..=143 => self.write_bits(v + 0x30, 8, true),
            144..=255 => self.write_bits(v - 144 + 0x190, 9, true),
            256..=279 => self.write_bits(v - 256, 7, true),
            _ => self.write_bits(v - 280 + 0xc0, 8, true),
        }
    }

    fn literal(&mut self, b: u8) {
        self.hcode(u32::from(b));
    }

    /// Length symbol `c + val >> nx` and `nx` extra bits.
    fn codex(&mut self, c: u32, val: u32, nx: u32) {
        self.hcode(c + (val >> nx));
        self.write_bits(val & ((1 << nx) - 1), nx, false);
    }

    /// `n` bytes copied from `d` bytes back, in matches of at most 258.
    fn repeat(&mut self, mut n: usize, d: usize) {
        while n >= 258 + 3 {
            self.repeat1(258, d);
            n -= 258;
        }
        if n > 258 {
            self.repeat1(10, d);
            self.repeat1(n - 10, d);
            return;
        }
        self.repeat1(n, d);
    }

    fn repeat1(&mut self, n: usize, d: usize) {
        let n = u32::try_from(n).unwrap_or(258);
        match n {
            0..=10 => self.codex(257, n - 3, 0),
            11..=18 => self.codex(265, n - 11, 1),
            19..=34 => self.codex(269, n - 19, 2),
            35..=66 => self.codex(273, n - 35, 3),
            67..=130 => self.codex(277, n - 67, 4),
            131..=257 => self.codex(281, n - 131, 5),
            _ => self.hcode(285),
        }
        let d = u32::try_from(d).unwrap_or(32768);
        if d <= 4 {
            self.write_bits(d - 1, 5, true);
        } else {
            let mut nbit = 16;
            while d <= 1 << (nbit - 1) {
                nbit -= 1;
            }
            let mut v = d - 1;
            v &= !(1 << (nbit - 1));
            let code = (2 * nbit - 2) | (v >> (nbit - 2));
            v &= !(1 << (nbit - 2));
            self.write_bits(code, 5, true);
            self.write_bits(v, nbit - 2, false);
        }
    }
}

/// Adler-32 (RFC 1950), updated as the rows are written (the image data is never held whole).
struct Adler32 {
    a: u32,
    b: u32,
}

impl Adler32 {
    const fn new() -> Self {
        Self { a: 1, b: 0 }
    }

    fn update(&mut self, data: &[u8]) {
        // 5552 bytes keep the sums below 2³² between reductions.
        for chunk in data.chunks(5552) {
            for &x in chunk {
                self.a += u32::from(x);
                self.b += self.a;
            }
            self.a %= 65521;
            self.b %= 65521;
        }
    }

    const fn finish(&self) -> u32 {
        self.b << 16 | self.a
    }
}

/// The zlib stream of the image data (see the module documentation).
fn zlib(m: &QrModules, scale: usize) -> Vec<u8> {
    let side = (m.size + 8) * scale;
    let n = side.div_ceil(8);
    let row_len = 1 + n;
    let mut adler = Adler32::new();
    let mut w = BitWriter::default();
    // zlib header: deflate, 32 KiB window, check bits.
    w.bytes.extend_from_slice(&[0x78, 0x01]);
    // One final block, fixed Huffman codes.
    w.write_bits(1, 1, false);
    w.write_bits(1, 2, false);

    let mut white = vec![255u8; row_len];
    white[0] = 0;
    let border = |w: &mut BitWriter, adler: &mut Adler32| {
        w.literal(0);
        w.literal(255);
        w.repeat(n - 1, 1);
        w.repeat((4 * scale - 1) * row_len, row_len);
        for _ in 0..4 * scale {
            adler.update(&white);
        }
    };
    border(&mut w, &mut adler);

    let mut row = vec![0u8; row_len];
    let size = i64::try_from(m.size).unwrap_or(i64::MAX);
    for y in 0..size {
        // Pixels packed from the most significant bit, white = 1. The byte after the last
        // full one keeps shifting in, so it holds the tail of the (white) border too.
        let mut j = 1;
        let (mut z, mut nz) = (0u8, 0);
        for x in -4..size + 4 {
            for _ in 0..scale {
                z <<= 1;
                if !m.is_dark(x, y) {
                    z |= 1;
                }
                nz += 1;
                if nz == 8 {
                    row[j] = z;
                    j += 1;
                    nz = 0;
                }
            }
        }
        if j < row.len() {
            row[j] = z;
        }
        for &b in &row {
            w.literal(b);
        }
        w.repeat((scale - 1) * row_len, row_len);
        for _ in 0..scale {
            adler.update(&row);
        }
    }

    border(&mut w, &mut adler);
    w.hcode(256);
    w.flush();
    w.bytes.extend_from_slice(&adler.finish().to_be_bytes());
    w.bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The text the Go implementation's QR tests encode (17 bytes).
    const GO_TEST_URL: &str = "https://gohugo.io";

    #[test]
    fn modes_and_versions_are_rsc_s() {
        // `GO_TEST_URL`: byte mode, 4 + 8 + 136 = 148 bits: version 2 at M (224 bits),
        // version 3 at H (208 bits).
        assert_eq!(
            qr_modules(GO_TEST_URL, QrLevel::Medium).expect("M").size,
            25
        );
        assert_eq!(qr_modules(GO_TEST_URL, QrLevel::High).expect("H").size, 29);
        // 41 digits fit version 1 at L in numeric mode (the most a version 1 symbol holds).
        let digits = "1".repeat(41);
        assert_eq!(qr_modules(&digits, QrLevel::Low).expect("numeric").size, 21);
        assert_eq!(
            qr_modules(&"1".repeat(42), QrLevel::Low).expect("v2").size,
            25
        );
        // 25 alphanumeric characters fit version 1 at L; lower case forces byte mode.
        assert_eq!(
            qr_modules(&"A".repeat(25), QrLevel::Low)
                .expect("alnum")
                .size,
            21
        );
        assert_eq!(
            qr_modules(&"a".repeat(25), QrLevel::Low)
                .expect("byte")
                .size,
            25
        );
        assert!(qr_modules("", QrLevel::Low).is_err());
        assert!(qr_modules(&"x".repeat(3000), QrLevel::High).is_err());
    }

    #[test]
    fn checksums() {
        let mut a = Adler32::new();
        a.update(b"Wiki");
        a.update(b"pedia");
        assert_eq!(a.finish(), 0x11E6_0398);
        // Long runs are reduced in time.
        let mut long = Adler32::new();
        long.update(&vec![255; 100_000]);
        assert!(long.a < 65521 && long.b < 65521);
        assert_eq!(crc32(&[b"IEND"]), 0xAE42_6082);
    }

    #[test]
    fn the_png_decodes_to_the_modules() {
        let m = qr_modules("HELLO", QrLevel::Quartile).expect("qr");
        let bytes = qr_png("HELLO", QrLevel::Quartile, 3).expect("png");
        let img = image::load_from_memory(&bytes)
            .expect("decode")
            .into_luma8();
        let side = (m.size + 8) * 3;
        assert_eq!(img.dimensions(), (side as u32, side as u32));
        for (x, y, p) in img.enumerate_pixels() {
            let (mx, my) = (i64::from(x) / 3 - 4, i64::from(y) / 3 - 4);
            assert_eq!(p.0[0] == 0, m.is_dark(mx, my), "({x}, {y})");
        }
        assert!(qr_png("HELLO", QrLevel::Low, 1).is_err());
    }
}
