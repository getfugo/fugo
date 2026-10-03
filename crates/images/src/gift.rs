//! The resize of disintegration/gift v1.2.1 as the Go implementation's smart crop analysis runs it,
//! ported for identical results (`resize.go`, `pixels.go`, `utils.go`).
//!
//! The smart crop (`smartcrop.rs`) analyses a downscaled copy of the source, and the crop it picks
//! depends on every value of that copy: the processing pipeline's own resizer (`fast_image_resize`,
//! `pixels.rs`) is close to gift's but not equal, so the analysis uses this port instead. It has
//! the Go implementation's resample kernels (`resources/images/resampling.go`, and gift's
//! nearest-neighbour, box, linear and Lanczos kernels; `config.go` `imageFilters`), gift's float32
//! pixel getters and setters for each of Go's image types, its weights, its two-pass resize through
//! a 16-bit temporary image, and the Go implementation's choice of the result's type (`doFilter`,
//! `resources/images/image.go`). The arithmetic is float32 in Go's order and without fused
//! multiply-adds, as Go compiles it for amd64 (the platform of the published builds).

use crate::spec::Resample;

/// A pixel as gift holds it: non-premultiplied channels in 0…1 (`pixel`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Px {
    r: f32,
    g: f32,
    b: f32,
    a: f32,
}

/// `qf8`, `qf16` (`pixels.go`): Go rounds the constants to float32 at their use.
const QF8: f32 = 1.0 / 255.0;
const QF16: f32 = 1.0 / 65535.0;

// ---------------------------------------------------------------------------------------
// Go's image types

/// How Go's decoders hold a decoded image, which decides how gift reads its pixels
/// (`newPixelGetter`) and the type of a filtered copy (the Go implementation's `doFilter`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GoType {
    /// `*image.RGBA`: 8-bit true-colour PNGs without transparency, RGB JPEGs (opaque).
    Rgba,
    /// `*image.NRGBA`: 8-bit PNGs with alpha or a transparent colour, WebP.
    Nrgba,
    /// `*image.Gray`: greyscale JPEGs and 8-bit (or less) greyscale PNGs.
    Gray,
    /// `*image.Paletted`: paletted PNGs and GIFs (read through `color.Color`).
    Paletted,
    /// `*image.YCbCr`: colour JPEGs.
    YCbCr,
    /// `*image.CMYK`: four-component JPEGs (read through `color.Color`).
    Cmyk,
    /// `*image.RGBA64`: 16-bit true-colour PNGs without transparency.
    Rgba64,
    /// `*image.NRGBA64`: 16-bit PNGs with alpha or a transparent colour.
    Nrgba64,
    /// `*image.Gray16`: 16-bit greyscale PNGs.
    Gray16,
}

impl GoType {
    /// The type of a filtered copy (the Go implementation's `doFilter`): the source's own for
    /// `*image.RGBA`, `*image.NRGBA` and `*image.Gray`, else `*image.NRGBA`.
    fn filtered(self) -> DstType {
        match self {
            Self::Rgba => DstType::Rgba,
            Self::Gray => DstType::Gray,
            _ => DstType::Nrgba,
        }
    }
}

/// The planes of an `*image.YCbCr` (origin 0, 0, Go's strides).
pub(crate) struct YCbCrPlanes {
    pub y: Vec<u8>,
    pub cb: Vec<u8>,
    pub cr: Vec<u8>,
    pub y_stride: usize,
    pub c_stride: usize,
    /// The chroma subsampling, (horizontal, vertical): (1, 1) 4:4:4, (2, 1) 4:2:2,
    /// (2, 2) 4:2:0, (1, 2) 4:4:0, (4, 1) 4:1:1, (4, 2) 4:1:0.
    pub sub: (usize, usize),
}

/// The pixels of a [`Source`].
pub(crate) enum Pix<'a> {
    /// 8-bit non-premultiplied RGBA, 4 bytes per pixel.
    Rgba8(&'a [u8]),
    /// 16-bit non-premultiplied RGBA, 4 samples per pixel.
    Rgba16(&'a [u16]),
    /// One byte per pixel, `stride` bytes per row (Go's `*image.Gray`).
    Gray8 {
        pix: &'a [u8],
        stride: usize,
    },
    YCbCr(&'a YCbCrPlanes),
    /// C, M, Y, K, 4 bytes per pixel.
    Cmyk(&'a [u8]),
}

/// A decoded source as gift reads it.
pub(crate) struct Source<'a> {
    pub width: usize,
    pub height: usize,
    pub ty: GoType,
    pub pix: Pix<'a>,
}

impl Source<'_> {
    pub(crate) fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    /// `pixelGetter.getPixel`.
    fn get(&self, x: usize, y: usize) -> Px {
        let i = (y * self.width + x) * 4;
        match self.pix {
            Pix::YCbCr(planes) => ycbcr(planes, x, y),
            Pix::Gray8 { pix, stride } => gray8(pix[y * stride + x]),
            Pix::Cmyk(pix) => {
                from_color(cmyk_to_rgba16([pix[i], pix[i + 1], pix[i + 2], pix[i + 3]]))
            }
            Pix::Rgba16(wide) => {
                let q = [wide[i], wide[i + 1], wide[i + 2], wide[i + 3]];
                match self.ty {
                    GoType::Gray16 => {
                        let v = f32::from(q[0]) * QF16;
                        Px {
                            r: v,
                            g: v,
                            b: v,
                            a: 1.0,
                        }
                    }
                    GoType::Rgba64 if q[3] == 0xffff => Px {
                        r: f32::from(q[0]) * QF16,
                        g: f32::from(q[1]) * QF16,
                        b: f32::from(q[2]) * QF16,
                        a: 1.0,
                    },
                    _ => Px {
                        r: f32::from(q[0]) * QF16,
                        g: f32::from(q[1]) * QF16,
                        b: f32::from(q[2]) * QF16,
                        a: f32::from(q[3]) * QF16,
                    },
                }
            }
            Pix::Rgba8(raw) => {
                let p = [raw[i], raw[i + 1], raw[i + 2], raw[i + 3]];
                match self.ty {
                    GoType::Gray => gray8(p[0]),
                    GoType::Rgba if p[3] == 0xff => Px {
                        r: f32::from(p[0]) * QF8,
                        g: f32::from(p[1]) * QF8,
                        b: f32::from(p[2]) * QF8,
                        a: 1.0,
                    },
                    GoType::Paletted => from_color(nrgba_to_rgba16(p)),
                    // `*image.NRGBA`, and the planar or 16-bit types read from 8-bit pixels
                    // when their own samples are not at hand.
                    _ => nrgba8(p),
                }
            }
        }
    }
}

/// The `itGray` getter.
fn gray8(v: u8) -> Px {
    let v = f32::from(v) * QF8;
    Px {
        r: v,
        g: v,
        b: v,
        a: 1.0,
    }
}

/// `color.CMYK.RGBA`.
fn cmyk_to_rgba16([c, m, y, k]: [u8; 4]) -> [u32; 4] {
    let w = 0xffff - u32::from(k) * 0x101;
    let ch = |v: u8| (0xffff - u32::from(v) * 0x101) * w / 0xffff;
    [ch(c), ch(m), ch(y), 0xffff]
}

/// The `itNRGBA` getter.
fn nrgba8(p: [u8; 4]) -> Px {
    Px {
        r: f32::from(p[0]) * QF8,
        g: f32::from(p[1]) * QF8,
        b: f32::from(p[2]) * QF8,
        a: f32::from(p[3]) * QF8,
    }
}

/// `color.NRGBA.RGBA`: the 16-bit alpha-premultiplied channels of an 8-bit colour.
fn nrgba_to_rgba16(p: [u8; 4]) -> [u32; 4] {
    let a = u32::from(p[3]) * 0x101;
    let pre = |v: u8| u32::from(v) * 0x101 * a / 0xffff;
    [pre(p[0]), pre(p[1]), pre(p[2]), a]
}

/// `pixelFromColor`: a colour read through `color.Color.RGBA`.
fn from_color([r16, g16, b16, a16]: [u32; 4]) -> Px {
    match a16 {
        0 => Px::default(),
        0xffff => Px {
            r: r16 as f32 * QF16,
            g: g16 as f32 * QF16,
            b: b16 as f32 * QF16,
            a: 1.0,
        },
        _ => {
            let q = 1.0 / a16 as f32;
            Px {
                r: r16 as f32 * q,
                g: g16 as f32 * q,
                b: b16 as f32 * q,
                a: a16 as f32 * QF16,
            }
        }
    }
}

/// The `itYCbCr` getter: gift's own fixed-point conversion of the nearest chroma sample.
fn ycbcr(p: &YCbCrPlanes, x: usize, y: usize) -> Px {
    const MAX: i32 = 255 * 100_000;
    const INV: f32 = 1.0 / 25_500_000.0;
    let iy = y * p.y_stride + x;
    let ic = (y / p.sub.1) * p.c_stride + x / p.sub.0;
    let y1 = i32::from(p.y[iy]) * 100_000;
    let cb1 = i32::from(p.cb[ic]) - 128;
    let cr1 = i32::from(p.cr[ic]) - 128;
    let r1 = y1 + 140_200 * cr1;
    let g1 = y1 - 34_414 * cb1 - 71_414 * cr1;
    let b1 = y1 + 177_200 * cb1;
    // `clampi32`: values not above the minimum become 0 (the minimum is 0 here).
    let clamp = |v: i32| if v > MAX { MAX } else { v.max(0) };
    Px {
        r: clamp(r1) as f32 * INV,
        g: clamp(g1) as f32 * INV,
        b: clamp(b1) as f32 * INV,
        a: 1.0,
    }
}

// ---------------------------------------------------------------------------------------
// Results

/// The types gift writes here: a filtered copy, or the temporary of a two-pass resize
/// (`createTempImage`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DstType {
    Rgba,
    Nrgba,
    Gray,
    Nrgba64,
}

/// An image gift wrote, origin (0, 0).
struct Dst {
    ty: DstType,
    w: usize,
    h: usize,
    /// One byte per channel (`Gray`: one per pixel); `Nrgba64`: in `wide`.
    pix: Vec<u8>,
    wide: Vec<u16>,
}

/// `f32u8`: rounds half up (Go's `int64(val + 0.5)` truncates toward zero) and clamps.
fn f32u8(val: f32) -> u8 {
    let x = (val + 0.5) as i64;
    x.clamp(0, 0xff) as u8
}

/// `f32u16`.
fn f32u16(val: f32) -> u16 {
    let x = (val + 0.5) as i64;
    x.clamp(0, 0xffff) as u16
}

impl Dst {
    fn new(ty: DstType, w: usize, h: usize) -> Self {
        let (pix, wide) = match ty {
            DstType::Gray => (vec![0; w * h], Vec::new()),
            DstType::Nrgba64 => (Vec::new(), vec![0; w * h * 4]),
            DstType::Rgba | DstType::Nrgba => (vec![0; w * h * 4], Vec::new()),
        };
        Self {
            ty,
            w,
            h,
            pix,
            wide,
        }
    }

    /// `pixelSetter.setPixel`.
    fn set(&mut self, x: usize, y: usize, px: Px) {
        let i = y * self.w + x;
        match self.ty {
            DstType::Nrgba => {
                self.pix[i * 4] = f32u8(px.r * 255.0);
                self.pix[i * 4 + 1] = f32u8(px.g * 255.0);
                self.pix[i * 4 + 2] = f32u8(px.b * 255.0);
                self.pix[i * 4 + 3] = f32u8(px.a * 255.0);
            }
            DstType::Rgba => {
                let fa = px.a * 255.0;
                self.pix[i * 4] = f32u8(px.r * fa);
                self.pix[i * 4 + 1] = f32u8(px.g * fa);
                self.pix[i * 4 + 2] = f32u8(px.b * fa);
                self.pix[i * 4 + 3] = f32u8(fa);
            }
            DstType::Gray => {
                self.pix[i] = f32u8((0.299 * px.r + 0.587 * px.g + 0.114 * px.b) * px.a * 255.0);
            }
            DstType::Nrgba64 => {
                self.wide[i * 4] = f32u16(px.r * 65535.0);
                self.wide[i * 4 + 1] = f32u16(px.g * 65535.0);
                self.wide[i * 4 + 2] = f32u16(px.b * 65535.0);
                self.wide[i * 4 + 3] = f32u16(px.a * 65535.0);
            }
        }
    }

    /// The getter of the temporary image (`itNRGBA64`).
    fn get64(&self, x: usize, y: usize) -> Px {
        let i = (y * self.w + x) * 4;
        Px {
            r: f32::from(self.wide[i]) * QF16,
            g: f32::from(self.wide[i + 1]) * QF16,
            b: f32::from(self.wide[i + 2]) * QF16,
            a: f32::from(self.wide[i + 3]) * QF16,
        }
    }

    /// The pixels as an `*image.RGBA` (alpha-premultiplied): smartcrop's `toRGBA`, which
    /// copies other types with `draw.Src` (`drawNRGBASrc`, `drawGray`).
    fn into_rgba(self) -> Vec<u8> {
        match self.ty {
            DstType::Rgba => self.pix,
            DstType::Gray => self.pix.iter().flat_map(|&v| [v, v, v, 0xff]).collect(),
            DstType::Nrgba => {
                let mut pix = self.pix;
                for p in pix.as_chunks_mut::<4>().0 {
                    let sa = u32::from(p[3]) * 0x101;
                    let pre = |v: u8| ((u32::from(v) * sa / 0xff) >> 8) as u8;
                    p[0] = pre(p[0]);
                    p[1] = pre(p[1]);
                    p[2] = pre(p[2]);
                    p[3] = (sa >> 8) as u8;
                }
                pix
            }
            DstType::Nrgba64 => unreachable!("the temporary image is never a result"),
        }
    }
}

// ---------------------------------------------------------------------------------------
// Kernels

/// A resample filter: its support and kernel (gift's `Resampling`).
#[derive(Clone, Copy)]
struct Kernel {
    support: f32,
    kernel: fn(f32) -> f32,
}

/// `bcspline` (gift's and the Go implementation's): Mitchell–Netravali's cubic family.
fn bcspline(x: f32, b: f32, c: f32) -> f32 {
    let x = if x < 0.0 { -x } else { x };
    if x < 1.0 {
        ((12.0 - 9.0 * b - 6.0 * c) * x * x * x
            + (-18.0 + 12.0 * b + 6.0 * c) * x * x
            + (6.0 - 2.0 * b))
            / 6.0
    } else if x < 2.0 {
        ((-b - 6.0 * c) * x * x * x
            + (6.0 * b + 30.0 * c) * x * x
            + (-12.0 * b - 48.0 * c) * x
            + (8.0 * b + 24.0 * c))
            / 6.0
    } else {
        0.0
    }
}

/// `sinc`: computed in float64, rounded to float32.
fn sinc(x: f32) -> f32 {
    if x == 0.0 {
        return 1.0;
    }
    let px = std::f64::consts::PI * f64::from(x);
    (px.sin() / px) as f32
}

/// `math.Pi*float64(x)/3.0`, the argument of the windows of the Go implementation's sinc
/// filters.
fn third_pi(x: f32) -> f64 {
    std::f64::consts::PI * f64::from(x) / 3.0
}

/// `abs` then the kernel within `support`, as each of the Go implementation's kernels starts.
fn within(x: f32, support: f32, f: impl Fn(f32) -> f32) -> f32 {
    let x = if x < 0.0 { -x } else { x };
    if x < support { f(x) } else { 0.0 }
}

impl Kernel {
    fn of(filter: Resample) -> Self {
        let (support, kernel): (f32, fn(f32) -> f32) = match filter {
            Resample::NearestNeighbor => (0.0, |_| 0.0),
            Resample::Box => (0.5, |x| {
                let x = if x < 0.0 { -x } else { x };
                if x <= 0.5 { 1.0 } else { 0.0 }
            }),
            Resample::Linear => (1.0, |x| within(x, 1.0, |x| 1.0 - x)),
            Resample::Hermite => (1.0, |x| within(x, 1.0, |x| bcspline(x, 0.0, 0.0))),
            Resample::MitchellNetravali => (2.0, |x| {
                within(x, 2.0, |x| bcspline(x, 1.0 / 3.0, 1.0 / 3.0))
            }),
            Resample::CatmullRom => (2.0, |x| within(x, 2.0, |x| bcspline(x, 0.0, 0.5))),
            Resample::BSpline => (2.0, |x| within(x, 2.0, |x| bcspline(x, 1.0, 0.0))),
            Resample::Gaussian => (2.0, |x| {
                within(x, 2.0, |x| f64::from(-2.0 * x * x).exp() as f32)
            }),
            Resample::Lanczos => (3.0, |x| within(x, 3.0, |x| sinc(x) * sinc(x / 3.0))),
            Resample::Hann => (3.0, |x| {
                within(x, 3.0, |x| sinc(x) * (0.5 + 0.5 * third_pi(x).cos()) as f32)
            }),
            Resample::Hamming => (3.0, |x| {
                within(x, 3.0, |x| {
                    sinc(x) * (0.54 + 0.46 * third_pi(x).cos()) as f32
                })
            }),
            Resample::Blackman => (3.0, |x| {
                within(x, 3.0, |x| {
                    let w = 0.42 - 0.5 * (third_pi(x) + std::f64::consts::PI).cos()
                        + 0.08 * (2.0 * std::f64::consts::PI * f64::from(x) / 3.0).cos();
                    sinc(x) * w as f32
                })
            }),
            Resample::Bartlett => (3.0, |x| within(x, 3.0, |x| sinc(x) * (3.0 - x) / 3.0)),
            Resample::Welch => (3.0, |x| within(x, 3.0, |x| sinc(x) * (1.0 - (x * x / 9.0)))),
            Resample::Cosine => (3.0, |x| {
                within(x, 3.0, |x| {
                    sinc(x) * (std::f64::consts::FRAC_PI_2 * (f64::from(x) / 3.0)).cos() as f32
                })
            }),
        };
        Self { support, kernel }
    }
}

// ---------------------------------------------------------------------------------------
// Resizing

struct Weight {
    index: usize,
    weight: f32,
}

/// `prepareResampWeights`.
fn weights(dst: usize, src: usize, k: Kernel) -> Vec<Vec<Weight>> {
    let delta = src as f32 / dst as f32;
    let scale = if delta < 1.0 { 1.0 } else { delta };
    let radius = f64::from(scale * k.support).ceil() as f32;
    let last = src as i64 - 1;
    (0..dst)
        .map(|i| {
            let center = (i as f32 + 0.5) * delta - 0.5;
            let left = (f64::from(center - radius).ceil() as i64).max(0);
            let right = (f64::from(center + radius).floor() as i64).min(last);
            let mut row = Vec::new();
            let mut sum = 0.0f32;
            for j in left..=right {
                let weight = (k.kernel)((j as f32 - center) / scale);
                if weight == 0.0 {
                    continue;
                }
                row.push(Weight {
                    index: j as usize,
                    weight,
                });
                sum += weight;
            }
            for w in &mut row {
                w.weight /= sum;
            }
            row
        })
        .collect()
}

/// `resizeLine`.
fn resize_line(dst: &mut [Px], src: &[Px], weights: &[Vec<Weight>]) {
    for (out, ws) in dst.iter_mut().zip(weights) {
        let (mut r, mut g, mut b, mut a) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        for w in ws {
            let c = src[w.index];
            let wa = c.a * w.weight;
            r += c.r * wa;
            g += c.g * wa;
            b += c.b * wa;
            a += wa;
        }
        if a != 0.0 {
            r /= a;
            g /= a;
            b /= a;
        }
        *out = Px { r, g, b, a };
    }
}

/// Either image gift reads here: the source, or the temporary of a two-pass resize.
enum Input<'a, 'b> {
    Source(&'a Source<'b>),
    Temp(&'a Dst),
}

impl Input<'_, '_> {
    fn size(&self) -> (usize, usize) {
        match self {
            Self::Source(s) => s.size(),
            Self::Temp(t) => (t.w, t.h),
        }
    }

    fn get(&self, x: usize, y: usize) -> Px {
        match self {
            Self::Source(s) => s.get(x, y),
            Self::Temp(t) => t.get64(x, y),
        }
    }
}

/// `resizeHorizontal`.
fn resize_horizontal(dst: &mut Dst, src: &Input<'_, '_>, k: Kernel) {
    let (sw, sh) = src.size();
    let weights = weights(dst.w, sw, k);
    let mut src_buf = vec![Px::default(); sw];
    let mut dst_buf = vec![Px::default(); dst.w];
    for y in 0..sh {
        for (x, p) in src_buf.iter_mut().enumerate() {
            *p = src.get(x, y);
        }
        resize_line(&mut dst_buf, &src_buf, &weights);
        for (x, &p) in dst_buf.iter().enumerate() {
            dst.set(x, y, p);
        }
    }
}

/// `resizeVertical`.
fn resize_vertical(dst: &mut Dst, src: &Input<'_, '_>, k: Kernel) {
    let (sw, sh) = src.size();
    let weights = weights(dst.h, sh, k);
    let mut src_buf = vec![Px::default(); sh];
    let mut dst_buf = vec![Px::default(); dst.h];
    for x in 0..sw {
        for (y, p) in src_buf.iter_mut().enumerate() {
            *p = src.get(x, y);
        }
        resize_line(&mut dst_buf, &src_buf, &weights);
        for (y, &p) in dst_buf.iter().enumerate() {
            dst.set(x, y, p);
        }
    }
}

/// `resizeNearest`.
fn resize_nearest(dst: &mut Dst, src: &Input<'_, '_>) {
    let (sw, sh) = src.size();
    let dx = sw as f64 / dst.w as f64;
    let dy = sh as f64 / dst.h as f64;
    for y in 0..dst.h {
        for x in 0..dst.w {
            let sx = ((x as f64 + 0.5) * dx).floor() as usize;
            let sy = ((y as f64 + 0.5) * dy).floor() as usize;
            dst.set(x, y, src.get(sx, sy));
        }
    }
}

/// `gift.Resize(width, height, filter)` drawn into the type the Go implementation's `doFilter`
/// gives a filtered copy of `src`, returned as smartcrop's `toRGBA` makes it (8-bit,
/// alpha-premultiplied RGBA). Both sides must be positive.
pub(crate) fn resize_to_rgba(
    src: &Source<'_>,
    width: usize,
    height: usize,
    filter: Resample,
) -> Vec<u8> {
    let k = Kernel::of(filter);
    let mut dst = Dst::new(src.ty.filtered(), width, height);
    let input = Input::Source(src);
    let (sw, sh) = src.size();
    if (sw, sh) == (width, height) {
        // `copyimage`.
        for y in 0..sh {
            for x in 0..sw {
                dst.set(x, y, src.get(x, y));
            }
        }
    } else if k.support <= 0.0 {
        resize_nearest(&mut dst, &input);
    } else if sw == width {
        resize_vertical(&mut dst, &input, k);
    } else if sh == height {
        resize_horizontal(&mut dst, &input, k);
    } else {
        let mut tmp = Dst::new(DstType::Nrgba64, width, sh);
        resize_horizontal(&mut tmp, &input, k);
        resize_vertical(&mut dst, &Input::Temp(&tmp), k);
    }
    dst.into_rgba()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_helpers_match_go() {
        assert_eq!(f32u8(-0.7), 0);
        assert_eq!(f32u8(254.49), 254);
        assert_eq!(f32u8(254.5), 255);
        assert_eq!(f32u8(300.0), 255);
        assert_eq!(f32u16(65534.5), 65535);
    }

    #[test]
    fn box_weights_average_neighbours() {
        let w = weights(2, 4, Kernel::of(Resample::Box));
        let got: Vec<Vec<(usize, f32)>> = w
            .iter()
            .map(|r| r.iter().map(|w| (w.index, w.weight)).collect())
            .collect();
        assert_eq!(
            got,
            vec![vec![(0, 0.5), (1, 0.5)], vec![(2, 0.5), (3, 0.5)]]
        );
    }

    #[test]
    fn nrgba_premultiplies_like_draw_src() {
        let mut d = Dst::new(DstType::Nrgba, 1, 1);
        d.pix.copy_from_slice(&[200, 100, 50, 128]);
        // Go: sa = 128·0x101; r = 200·sa/0xff >> 8.
        assert_eq!(d.into_rgba(), vec![100, 50, 25, 128]);
    }
}
