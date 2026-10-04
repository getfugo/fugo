//! Colour adjustments: per channel and in HSL, with their curves.

use super::*;

pub(super) fn unit(v: u8) -> f32 {
    f32::from(v) / 255.0
}

pub(super) fn to_u8(v: f32) -> u8 {
    // NaN clamps to 0.
    (v * 255.0 + 0.5).clamp(0.0, 255.0) as u8
}

/// Maps every colour channel through `f` (on 0…1 values), with a lookup table.
pub(super) fn map_channels(mut img: RgbaImage, f: impl Fn(f32) -> f32) -> RgbaImage {
    let lut: Vec<u8> = (0..=255u8).map(|v| to_u8(f(unit(v)))).collect();
    for p in img.pixels_mut() {
        for c in &mut p.0[..3] {
            *c = lut[usize::from(*c)];
        }
    }
    img
}

/// Maps the RGB of every pixel through `f` (on 0…1 values).
pub(super) fn map_rgb(mut img: RgbaImage, f: impl Fn([f32; 3]) -> [f32; 3]) -> RgbaImage {
    for p in img.pixels_mut() {
        let [r, g, b, _] = p.0;
        let out = f([unit(r), unit(g), unit(b)]);
        for (c, v) in p.0.iter_mut().zip(out) {
            *c = to_u8(v);
        }
    }
    img
}

pub(super) fn luma([r, g, b]: [f32; 3]) -> f32 {
    0.299 * r + 0.587 * g + 0.114 * b
}

pub(super) fn rgb_to_hsl([r, g, b]: [f32; 3]) -> [f32; 3] {
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

pub(super) fn hsl_to_rgb([h, s, l]: [f32; 3]) -> [f32; 3] {
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

pub(super) fn percent(p: f32, min: f32, max: f32) -> f32 {
    p.clamp(min, max) / 100.0
}

pub(super) fn sigmoid(midpoint: f32, factor: f32) -> impl Fn(f32) -> f32 {
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
pub(super) fn adjust(img: RgbaImage, f: &ImageFilter) -> RgbaImage {
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
