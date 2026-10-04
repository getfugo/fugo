//! Resampling: the filters of resizing, and resizing itself.

use super::*;

pub(super) fn sinc(x: f64) -> f64 {
    if x == 0.0 {
        1.0
    } else {
        let px = std::f64::consts::PI * x;
        px.sin() / px
    }
}

/// Mitchell–Netravali's cubic family with parameters `b` and `c`.
pub(super) fn bc_spline(x: f64, b: f64, c: f64) -> f64 {
    let x = x.abs();
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

/// A sinc windowed by `window` over the support ±3.
pub(super) fn windowed_sinc(x: f64, window: fn(f64) -> f64) -> f64 {
    let x = x.abs();
    if x < 3.0 { sinc(x) * window(x) } else { 0.0 }
}

pub(super) fn hermite(x: f64) -> f64 {
    if x.abs() < 1.0 {
        bc_spline(x, 0.0, 0.0)
    } else {
        0.0
    }
}

pub(super) fn mitchell(x: f64) -> f64 {
    bc_spline(x, 1.0 / 3.0, 1.0 / 3.0)
}

pub(super) fn catmull_rom(x: f64) -> f64 {
    bc_spline(x, 0.0, 0.5)
}

pub(super) fn bspline(x: f64) -> f64 {
    bc_spline(x, 1.0, 0.0)
}

pub(super) fn gaussian(x: f64) -> f64 {
    if x.abs() < 2.0 {
        (-2.0 * x * x).exp()
    } else {
        0.0
    }
}

pub(super) fn hann(x: f64) -> f64 {
    windowed_sinc(x, |x| 0.5 + 0.5 * (std::f64::consts::PI * x / 3.0).cos())
}

pub(super) fn hamming(x: f64) -> f64 {
    windowed_sinc(x, |x| 0.54 + 0.46 * (std::f64::consts::PI * x / 3.0).cos())
}

pub(super) fn blackman(x: f64) -> f64 {
    windowed_sinc(x, |x| {
        let a = std::f64::consts::PI * x / 3.0;
        0.42 - 0.5 * (a + std::f64::consts::PI).cos() + 0.08 * (2.0 * a).cos()
    })
}

pub(super) fn bartlett(x: f64) -> f64 {
    windowed_sinc(x, |x| (3.0 - x) / 3.0)
}

pub(super) fn welch(x: f64) -> f64 {
    windowed_sinc(x, |x| 1.0 - x * x / 9.0)
}

pub(super) fn cosine(x: f64) -> f64 {
    windowed_sinc(x, |x| (std::f64::consts::FRAC_PI_2 * x / 3.0).cos())
}

/// The `fast_image_resize` algorithm of a resample filter of the Go implementation. Box, linear,
/// Catmull-Rom, Mitchell and Lanczos are the crate's own kernels; the others are custom kernels
/// with the Go implementation's definitions and supports.
pub(super) fn algorithm(filter: Resample) -> ResizeAlg {
    let custom = |name, f: fn(f64) -> f64, support| {
        ResizeAlg::Convolution(FilterType::Custom(
            fir::Filter::new(name, f, support).expect("positive finite support"),
        ))
    };
    match filter {
        Resample::NearestNeighbor => ResizeAlg::Nearest,
        Resample::Box => ResizeAlg::Convolution(FilterType::Box),
        Resample::Linear => ResizeAlg::Convolution(FilterType::Bilinear),
        Resample::CatmullRom => custom("catmullrom", catmull_rom, 2.0),
        Resample::MitchellNetravali => custom("mitchellnetravali", mitchell, 2.0),
        Resample::Lanczos => ResizeAlg::Convolution(FilterType::Lanczos3),
        Resample::Hermite => custom("hermite", hermite, 1.0),
        Resample::BSpline => custom("bspline", bspline, 2.0),
        Resample::Gaussian => custom("gaussian", gaussian, 2.0),
        Resample::Hann => custom("hann", hann, 3.0),
        Resample::Hamming => custom("hamming", hamming, 3.0),
        Resample::Blackman => custom("blackman", blackman, 3.0),
        Resample::Bartlett => custom("bartlett", bartlett, 3.0),
        Resample::Welch => custom("welch", welch, 3.0),
        Resample::Cosine => custom("cosine", cosine, 3.0),
    }
}

/// Scales `img` to `size` (alpha-premultiplied).
pub(crate) fn resize(
    img: &RgbaImage,
    size: Size,
    filter: Resample,
) -> Result<RgbaImage, ImageError> {
    if img.dimensions() == size {
        return Ok(img.clone());
    }
    let mut dst = RgbaImage::new(size.0, size.1);
    Resizer::new()
        .resize(
            img,
            &mut dst,
            &ResizeOptions::new().resize_alg(algorithm(filter)),
        )
        .map_err(|e| ImageError::filter("resize", e.to_string()))?;
    Ok(dst)
}
