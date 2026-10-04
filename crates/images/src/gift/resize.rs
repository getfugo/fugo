//! Resizing: the kernels, their weights, and the horizontal and vertical passes.

use super::*;

/// A resample filter: its support and kernel (gift's `Resampling`).
#[derive(Clone, Copy)]
pub(super) struct Kernel {
    pub(super) support: f32,
    pub(super) kernel: fn(f32) -> f32,
}

/// `bcspline` (gift's and the Go implementation's): Mitchell–Netravali's cubic family.
pub(super) fn bcspline(x: f32, b: f32, c: f32) -> f32 {
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
pub(super) fn sinc(x: f32) -> f32 {
    if x == 0.0 {
        return 1.0;
    }
    let px = std::f64::consts::PI * f64::from(x);
    (px.sin() / px) as f32
}

/// `math.Pi*float64(x)/3.0`, the argument of the windows of the Go implementation's sinc
/// filters.
pub(super) fn third_pi(x: f32) -> f64 {
    std::f64::consts::PI * f64::from(x) / 3.0
}

/// `abs` then the kernel within `support`, as each of the Go implementation's kernels starts.
pub(super) fn within(x: f32, support: f32, f: impl Fn(f32) -> f32) -> f32 {
    let x = if x < 0.0 { -x } else { x };
    if x < support { f(x) } else { 0.0 }
}

impl Kernel {
    pub(super) fn of(filter: Resample) -> Self {
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

pub(super) struct Weight {
    pub(super) index: usize,
    pub(super) weight: f32,
}

/// `prepareResampWeights`.
pub(super) fn weights(dst: usize, src: usize, k: Kernel) -> Vec<Vec<Weight>> {
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
pub(super) fn resize_line(dst: &mut [Px], src: &[Px], weights: &[Vec<Weight>]) {
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
pub(super) enum Input<'a, 'b> {
    Source(&'a Source<'b>),
    Temp(&'a Dst),
}

impl Input<'_, '_> {
    pub(super) fn size(&self) -> (usize, usize) {
        match self {
            Self::Source(s) => s.size(),
            Self::Temp(t) => (t.w, t.h),
        }
    }

    pub(super) fn get(&self, x: usize, y: usize) -> Px {
        match self {
            Self::Source(s) => s.get(x, y),
            Self::Temp(t) => t.get64(x, y),
        }
    }
}

/// `resizeHorizontal`.
pub(super) fn resize_horizontal(dst: &mut Dst, src: &Input<'_, '_>, k: Kernel) {
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
pub(super) fn resize_vertical(dst: &mut Dst, src: &Input<'_, '_>, k: Kernel) {
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
pub(super) fn resize_nearest(dst: &mut Dst, src: &Input<'_, '_>) {
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
