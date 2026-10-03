//! Pixel work: every [`Step`] on a non-premultiplied 8-bit RGBA image.
//!
//! Resizing uses `fast_image_resize` (alpha-premultiplied, so transparent edges do not darken) with
//! the Go implementation's fifteen kernels; blurs use `imageproc`; rotations by multiples of 90°
//! and EXIF orientations use `image`. The colour filters, compositing, padding, pixelation and
//! arbitrary-angle rotation are written here; text and dithering are in their own modules.

use fast_image_resize::{self as fir, FilterType, ResizeAlg, ResizeOptions, Resizer};
use image::{DynamicImage, Rgba, Rgba32FImage, RgbaImage, imageops};

use crate::codec::Decoded;
use crate::color::Color;
use crate::error::ImageError;
use crate::filter::{ImageFilter, PaddingSpec};
use crate::plan::{InputRef, Size, Step, cover_size, crop_rect, sin_cos};
use crate::smartcrop::{self, Rect};
use crate::spec::{Action, Anchor, Resample};
use crate::{dither, text};

/// Loads the pixels of an image a step reads (overlay, mask).
pub(crate) type LoadInput<'a> = dyn Fn(&InputRef) -> Result<RgbaImage, ImageError> + 'a;

/// The regions of the smart crops among `steps`, in order, found on the operation's source
/// `src` (the Go implementation analyses the source whatever steps come before).
pub(crate) fn smart_regions(src: &Decoded, steps: &[Step]) -> Vec<Rect> {
    steps
        .iter()
        .filter_map(|s| match s {
            Step::SmartCrop { target, filter, .. } => {
                Some(smartcrop::find(&src.source(), target.0, target.1, *filter))
            }
            _ => None,
        })
        .collect()
}

/// Runs `steps` on `img`; `regions` are the smart crops' regions ([`smart_regions`]).
pub(crate) fn run(
    mut img: RgbaImage,
    steps: &[Step],
    load: &LoadInput<'_>,
    regions: &[Rect],
) -> Result<RgbaImage, ImageError> {
    let mut regions = regions.iter();
    for step in steps {
        img = match step {
            Step::Rotate { degrees, size } => rotate(&img, *degrees, *size),
            Step::Resize { size, filter } => resize(&img, *size, *filter)?,
            Step::Crop { x, y, size } => {
                imageops::crop_imm(&img, *x, *y, size.0, size.1).to_image()
            }
            Step::SmartCrop {
                action,
                target,
                filter,
            } => {
                let region = regions.next().copied().unwrap_or_default();
                smart_crop(&img, region, *action, *target, *filter)?
            }
            Step::Orient(o) => orient(img, *o),
            Step::Adjust(f) => adjust(img, f),
            Step::Padding(p) => pad(&img, p),
            Step::Opacity(o) => opacity(img, *o),
            Step::Overlay { image, x, y } => {
                let top = load(image)?;
                let mut img = img;
                draw_over(&mut img, &top, i64::from(*x), i64::from(*y));
                img
            }
            Step::Mask { image } => {
                let mask = load(image)?;
                apply_mask(img, &mask)?
            }
            Step::Text { spec, font } => {
                let mut img = img;
                text::draw(&mut img, spec, font.bytes())?;
                img
            }
            Step::Dither(spec) => dither::apply(img, spec),
        };
    }
    Ok(img)
}

// ---------------------------------------------------------------------------------------
// Resampling

fn sinc(x: f64) -> f64 {
    if x == 0.0 {
        1.0
    } else {
        let px = std::f64::consts::PI * x;
        px.sin() / px
    }
}

/// Mitchell–Netravali's cubic family with parameters `b` and `c`.
fn bc_spline(x: f64, b: f64, c: f64) -> f64 {
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
fn windowed_sinc(x: f64, window: fn(f64) -> f64) -> f64 {
    let x = x.abs();
    if x < 3.0 { sinc(x) * window(x) } else { 0.0 }
}

fn hermite(x: f64) -> f64 {
    if x.abs() < 1.0 {
        bc_spline(x, 0.0, 0.0)
    } else {
        0.0
    }
}
fn mitchell(x: f64) -> f64 {
    bc_spline(x, 1.0 / 3.0, 1.0 / 3.0)
}
fn catmull_rom(x: f64) -> f64 {
    bc_spline(x, 0.0, 0.5)
}
fn bspline(x: f64) -> f64 {
    bc_spline(x, 1.0, 0.0)
}
fn gaussian(x: f64) -> f64 {
    if x.abs() < 2.0 {
        (-2.0 * x * x).exp()
    } else {
        0.0
    }
}
fn hann(x: f64) -> f64 {
    windowed_sinc(x, |x| 0.5 + 0.5 * (std::f64::consts::PI * x / 3.0).cos())
}
fn hamming(x: f64) -> f64 {
    windowed_sinc(x, |x| 0.54 + 0.46 * (std::f64::consts::PI * x / 3.0).cos())
}
fn blackman(x: f64) -> f64 {
    windowed_sinc(x, |x| {
        let a = std::f64::consts::PI * x / 3.0;
        0.42 - 0.5 * (a + std::f64::consts::PI).cos() + 0.08 * (2.0 * a).cos()
    })
}
fn bartlett(x: f64) -> f64 {
    windowed_sinc(x, |x| (3.0 - x) / 3.0)
}
fn welch(x: f64) -> f64 {
    windowed_sinc(x, |x| 1.0 - x * x / 9.0)
}
fn cosine(x: f64) -> f64 {
    windowed_sinc(x, |x| (std::f64::consts::FRAC_PI_2 * x / 3.0).cos())
}

/// The `fast_image_resize` algorithm of a resample filter of the Go implementation. Box, linear,
/// Catmull-Rom, Mitchell and Lanczos are the crate's own kernels; the others are custom kernels
/// with the Go implementation's definitions and supports.
fn algorithm(filter: Resample) -> ResizeAlg {
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

// ---------------------------------------------------------------------------------------
// Geometry

/// The Go implementation's smart crop or fill once the region is known: `gift.Crop(region)`, then
/// `gift.Resize` to the target (fill) or `gift.CropToSize` at the centre (crop).
///
/// Nothing is left of an empty region (no candidate, or none scoring above −1): a fill then fills
/// at the centre anchor instead (`gift.ResizeToFill`, as the Go implementation's `processOptions`
/// in `resources/image.go` does for issue 7955); a crop would be empty, which planning rejects when
/// it can tell ([`crate::plan`]) and this rejects otherwise.
fn smart_crop(
    img: &RgbaImage,
    region: Rect,
    action: Action,
    target: Size,
    filter: Resample,
) -> Result<RgbaImage, ImageError> {
    let kept = region.intersect(Rect::of_size(img.dimensions()));
    let (w, h) = kept.size();
    if w == 0 || h == 0 {
        if action == Action::Fill {
            let cover = resize(img, cover_size(img.dimensions(), target), filter)?;
            let (x, y, size) = crop_rect(cover.dimensions(), target, Anchor::Center);
            return Ok(imageops::crop_imm(&cover, x, y, size.0, size.1).to_image());
        }
        return Err(ImageError::filter(
            "smart crop",
            format!(
                "the smart anchor keeps nothing of the image for {}x{} (the result would be \
                 empty); use another anchor",
                target.0, target.1
            ),
        ));
    }
    let (x, y) = (
        u32::try_from(kept.x0).unwrap_or(0),
        u32::try_from(kept.y0).unwrap_or(0),
    );
    let cropped = imageops::crop_imm(img, x, y, w, h).to_image();
    if action == Action::Fill {
        return resize(&cropped, target, filter);
    }
    let (x, y, size) = crop_rect((w, h), target, Anchor::Center);
    Ok(imageops::crop_imm(&cropped, x, y, size.0, size.1).to_image())
}

/// Rotates counter-clockwise onto a transparent canvas of `size` (nearest neighbour for
/// angles that are not multiples of 90°).
fn rotate(img: &RgbaImage, degrees: u32, size: Size) -> RgbaImage {
    match degrees % 360 {
        0 => return img.clone(),
        90 => return imageops::rotate270(img),
        180 => return imageops::rotate180(img),
        270 => return imageops::rotate90(img),
        _ => {}
    }
    let (s, c) = sin_cos(degrees);
    let (sw, sh) = img.dimensions();
    let half = |v: u32| (v as f32 - 1.0) / 2.0;
    let (scx, scy, dcx, dcy) = (half(sw), half(sh), half(size.0), half(size.1));
    RgbaImage::from_fn(size.0, size.1, |x, y| {
        let (dx, dy) = (x as f32 - dcx, y as f32 - dcy);
        // The inverse of a counter-clockwise rotation in image coordinates (y down).
        let sx = (dx * c - dy * s + scx).round();
        let sy = (dx * s + dy * c + scy).round();
        let inside = sx >= 0.0 && sy >= 0.0 && sx < sw as f32 && sy < sh as f32;
        if inside {
            *img.get_pixel(sx as u32, sy as u32)
        } else {
            Rgba([0, 0, 0, 0])
        }
    })
}

fn orient(img: RgbaImage, o: u8) -> RgbaImage {
    let Some(orientation) = image::metadata::Orientation::from_exif(o) else {
        return img;
    };
    let mut d = DynamicImage::ImageRgba8(img);
    d.apply_orientation(orientation);
    d.into_rgba8()
}

// ---------------------------------------------------------------------------------------
// Colour

fn unit(v: u8) -> f32 {
    f32::from(v) / 255.0
}

fn to_u8(v: f32) -> u8 {
    // NaN clamps to 0.
    (v * 255.0 + 0.5).clamp(0.0, 255.0) as u8
}

/// Maps every colour channel through `f` (on 0…1 values), with a lookup table.
fn map_channels(mut img: RgbaImage, f: impl Fn(f32) -> f32) -> RgbaImage {
    let lut: Vec<u8> = (0..=255u8).map(|v| to_u8(f(unit(v)))).collect();
    for p in img.pixels_mut() {
        for c in &mut p.0[..3] {
            *c = lut[usize::from(*c)];
        }
    }
    img
}

/// Maps the RGB of every pixel through `f` (on 0…1 values).
fn map_rgb(mut img: RgbaImage, f: impl Fn([f32; 3]) -> [f32; 3]) -> RgbaImage {
    for p in img.pixels_mut() {
        let [r, g, b, _] = p.0;
        let out = f([unit(r), unit(g), unit(b)]);
        for (c, v) in p.0.iter_mut().zip(out) {
            *c = to_u8(v);
        }
    }
    img
}

fn luma([r, g, b]: [f32; 3]) -> f32 {
    0.299 * r + 0.587 * g + 0.114 * b
}

fn rgb_to_hsl([r, g, b]: [f32; 3]) -> [f32; 3] {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if max == min {
        return [0.0, 0.0, l];
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if r == max {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if g == max {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    [h / 6.0, s, l]
}

fn hsl_to_rgb([h, s, l]: [f32; 3]) -> [f32; 3] {
    if s == 0.0 {
        return [l, l, l];
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let channel = |t: f32| {
        let t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    [channel(h + 1.0 / 3.0), channel(h), channel(h - 1.0 / 3.0)]
}

fn percent(p: f32, min: f32, max: f32) -> f32 {
    p.clamp(min, max) / 100.0
}

fn sigmoid(midpoint: f32, factor: f32) -> impl Fn(f32) -> f32 {
    let a = midpoint.clamp(0.0, 1.0);
    let b = factor.abs();
    let s = move |x: f32| 1.0 / (1.0 + (b * (a - x)).exp());
    let (s0, s1) = (s(0.0), s(1.0));
    move |x| {
        if factor == 0.0 {
            x
        } else if factor > 0.0 {
            (s(x) - s0) / (s1 - s0)
        } else {
            let e = 1.0e-5;
            let arg = ((s1 - s0) * x + s0).clamp(e, 1.0 - e);
            a - (1.0 / arg - 1.0).ln() / b
        }
    }
}

/// A colour filter, or a blur/sharpen/pixelate (size-preserving filters that read only
/// their input).
fn adjust(img: RgbaImage, f: &ImageFilter) -> RgbaImage {
    match *f {
        ImageFilter::Brightness { percentage } => {
            let shift = percent(percentage, -100.0, 100.0);
            map_channels(img, |x| x + shift)
        }
        ImageFilter::Contrast { percentage } => {
            let p = 1.0 + percent(percentage, -100.0, 100.0);
            map_channels(img, |x| {
                if p <= 1.0 {
                    (x - 0.5) * p + 0.5
                } else if p < 2.0 {
                    (x - 0.5) / (2.0 - p) + 0.5
                } else if x < 0.5 {
                    0.0
                } else {
                    1.0
                }
            })
        }
        ImageFilter::Gamma { gamma } => {
            let e = 1.0 / gamma.max(1.0e-5);
            map_channels(img, |x| x.powf(e))
        }
        ImageFilter::Invert => map_channels(img, |x| 1.0 - x),
        ImageFilter::Sigmoid { midpoint, factor } => map_channels(img, sigmoid(midpoint, factor)),
        ImageFilter::Grayscale => map_rgb(img, |c| {
            let y = luma(c);
            [y, y, y]
        }),
        ImageFilter::Sepia { percentage } => {
            let a = percent(percentage, 0.0, 100.0);
            map_rgb(img, |[r, g, b]| {
                [
                    r * (1.0 - 0.607 * a) + g * 0.769 * a + b * 0.189 * a,
                    r * 0.349 * a + g * (1.0 - 0.314 * a) + b * 0.168 * a,
                    r * 0.272 * a + g * 0.534 * a + b * (1.0 - 0.869 * a),
                ]
            })
        }
        ImageFilter::Hue { shift } => {
            let p = (shift / 360.0).rem_euclid(1.0);
            map_rgb(img, |c| {
                let [h, s, l] = rgb_to_hsl(c);
                hsl_to_rgb([(h + p).rem_euclid(1.0), s, l])
            })
        }
        ImageFilter::Saturation { percentage } => {
            let p = 1.0 + percent(percentage, -100.0, 500.0);
            map_rgb(img, |c| {
                let [h, s, l] = rgb_to_hsl(c);
                hsl_to_rgb([h, (s * p).min(1.0), l])
            })
        }
        ImageFilter::Colorize {
            hue,
            saturation,
            percentage,
        } => {
            let h = (hue / 360.0).rem_euclid(1.0);
            let s = percent(saturation, 0.0, 100.0);
            let p = percent(percentage, 0.0, 100.0);
            map_rgb(img, |c| {
                let [_, _, l] = rgb_to_hsl(c);
                let tint = hsl_to_rgb([h, s, l]);
                [0, 1, 2].map(|i| c[i] + (tint[i] - c[i]) * p)
            })
        }
        ImageFilter::ColorBalance { r, g, b } => {
            let m = [r, g, b].map(|v| 1.0 + percent(v, -100.0, 500.0));
            map_rgb(img, |c| [0, 1, 2].map(|i| c[i] * m[i]))
        }
        ImageFilter::GaussianBlur { sigma } => blur(&img, sigma),
        ImageFilter::UnsharpMask {
            sigma,
            amount,
            threshold,
        } => unsharp(img, sigma, amount, threshold),
        ImageFilter::Pixelate { size } => pixelate(img, size),
        // Planned as other steps.
        ImageFilter::Opacity { .. }
        | ImageFilter::Padding(_)
        | ImageFilter::Overlay { .. }
        | ImageFilter::Mask { .. }
        | ImageFilter::AutoOrient
        | ImageFilter::Text(_)
        | ImageFilter::Dither(_)
        | ImageFilter::Process { .. } => img,
    }
}

// ---------------------------------------------------------------------------------------
// Blur, sharpen, pixelate

fn premultiplied(img: &RgbaImage) -> Rgba32FImage {
    Rgba32FImage::from_fn(img.width(), img.height(), |x, y| {
        let [r, g, b, a] = img.get_pixel(x, y).0.map(unit);
        Rgba([r * a, g * a, b * a, a])
    })
}

fn unpremultiplied(img: &Rgba32FImage) -> RgbaImage {
    RgbaImage::from_fn(img.width(), img.height(), |x, y| {
        let [r, g, b, a] = img.get_pixel(x, y).0;
        if a <= 0.0 {
            Rgba([0, 0, 0, 0])
        } else {
            Rgba([to_u8(r / a), to_u8(g / a), to_u8(b / a), to_u8(a)])
        }
    })
}

/// A normalised Gaussian kernel of radius ⌈3σ⌉.
fn gaussian_kernel(sigma: f32) -> Vec<f32> {
    let radius = (sigma * 3.0).ceil().max(1.0) as i32;
    let mut k: Vec<f32> = (-radius..=radius)
        .map(|i| {
            let x = i as f32;
            (-x * x / (2.0 * sigma * sigma)).exp()
        })
        .collect();
    let sum: f32 = k.iter().sum();
    for v in &mut k {
        *v /= sum;
    }
    k
}

fn blurred(img: &RgbaImage, sigma: f32) -> Rgba32FImage {
    let pre = premultiplied(img);
    if sigma <= 0.0 {
        return pre;
    }
    imageproc::filter::separable_filter_equal(&pre, &gaussian_kernel(sigma))
}

fn blur(img: &RgbaImage, sigma: f32) -> RgbaImage {
    unpremultiplied(&blurred(img, sigma))
}

fn unsharp(mut img: RgbaImage, sigma: f32, amount: f32, threshold: f32) -> RgbaImage {
    let soft = unpremultiplied(&blurred(&img, sigma));
    for (p, s) in img.pixels_mut().zip(soft.pixels()) {
        for i in 0..3 {
            let v = unit(p.0[i]);
            let diff = v - unit(s.0[i]);
            if diff.abs() >= threshold {
                p.0[i] = to_u8(v + diff * amount);
            }
        }
    }
    img
}

/// Replaces each `size`×`size` cell (from the top left) by its average colour.
fn pixelate(mut img: RgbaImage, size: u32) -> RgbaImage {
    let (w, h) = img.dimensions();
    for cy in (0..h).step_by(size as usize) {
        for cx in (0..w).step_by(size as usize) {
            let (x1, y1) = ((cx + size).min(w), (cy + size).min(h));
            let mut sum = [0.0f64; 4];
            for y in cy..y1 {
                for x in cx..x1 {
                    let [r, g, b, a] = img.get_pixel(x, y).0.map(f64::from);
                    sum[0] += r * a;
                    sum[1] += g * a;
                    sum[2] += b * a;
                    sum[3] += a;
                }
            }
            let n = f64::from((x1 - cx) * (y1 - cy));
            let avg = if sum[3] > 0.0 {
                let a = sum[3];
                [sum[0] / a, sum[1] / a, sum[2] / a, a / n]
            } else {
                [0.0; 4]
            };
            let px = Rgba(avg.map(|v| v.round().clamp(0.0, 255.0) as u8));
            for y in cy..y1 {
                for x in cx..x1 {
                    img.put_pixel(x, y, px);
                }
            }
        }
    }
    img
}

// ---------------------------------------------------------------------------------------
// Compositing

/// Draws `top` over `dst` (source-over, non-premultiplied) with its corner at `(x, y)`.
pub(crate) fn draw_over(dst: &mut RgbaImage, top: &RgbaImage, x: i64, y: i64) {
    let (dw, dh) = (i64::from(dst.width()), i64::from(dst.height()));
    for (tx, ty, p) in top.enumerate_pixels() {
        let (px, py) = (x + i64::from(tx), y + i64::from(ty));
        if px < 0 || py < 0 || px >= dw || py >= dh {
            continue;
        }
        let (Ok(px), Ok(py)) = (u32::try_from(px), u32::try_from(py)) else {
            continue;
        };
        let d = dst.get_pixel_mut(px, py);
        *d = over(*d, *p);
    }
}

/// `top` over `bottom`, non-premultiplied.
pub(crate) fn over(bottom: Rgba<u8>, top: Rgba<u8>) -> Rgba<u8> {
    let ta = unit(top.0[3]);
    if ta >= 1.0 {
        return top;
    }
    if ta <= 0.0 {
        return bottom;
    }
    let ba = unit(bottom.0[3]);
    let a = ta + ba * (1.0 - ta);
    let mix = |i: usize| to_u8((unit(top.0[i]) * ta + unit(bottom.0[i]) * ba * (1.0 - ta)) / a);
    Rgba([mix(0), mix(1), mix(2), to_u8(a)])
}

/// Flattens `img` onto an opaque (or not) background colour.
pub(crate) fn flatten(img: &RgbaImage, bg: Color) -> RgbaImage {
    let mut out = RgbaImage::from_pixel(img.width(), img.height(), Rgba(bg.0));
    draw_over(&mut out, img, 0, 0);
    out
}

fn pad(img: &RgbaImage, p: &PaddingSpec) -> RgbaImage {
    let grow = |side: u32, a: i32, b: i32| {
        u32::try_from(i64::from(side) + i64::from(a) + i64::from(b))
            .unwrap_or(1)
            .max(1)
    };
    let (w, h) = (
        grow(img.width(), p.left, p.right),
        grow(img.height(), p.top, p.bottom),
    );
    let mut out = RgbaImage::from_pixel(w, h, Rgba(p.color.0));
    draw_over(&mut out, img, i64::from(p.left), i64::from(p.top));
    out
}

fn opacity(mut img: RgbaImage, o: f32) -> RgbaImage {
    for p in img.pixels_mut() {
        p.0[3] = to_u8(unit(p.0[3]) * o);
    }
    img
}

/// Multiplies the alpha of `img` by the luminance of `mask` scaled to its size.
fn apply_mask(mut img: RgbaImage, mask: &RgbaImage) -> Result<RgbaImage, ImageError> {
    let mask = resize(mask, img.dimensions(), Resample::Lanczos)?;
    for (p, m) in img.pixels_mut().zip(mask.pixels()) {
        let [r, g, b, a] = m.0.map(unit);
        // The mask's own transparency counts as black.
        let l = luma([r, g, b]) * a;
        p.0[3] = to_u8(unit(p.0[3]) * l);
    }
    Ok(img)
}
