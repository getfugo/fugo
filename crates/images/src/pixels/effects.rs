//! Blur, sharpening, pixelation, compositing, padding and masks.

use super::*;

pub(super) fn premultiplied(img: &RgbaImage) -> Rgba32FImage {
    Rgba32FImage::from_fn(img.width(), img.height(), |x, y| {
        let [r, g, b, a] = img.get_pixel(x, y).0.map(unit);
        Rgba([r * a, g * a, b * a, a])
    })
}

pub(super) fn unpremultiplied(img: &Rgba32FImage) -> RgbaImage {
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
pub(super) fn gaussian_kernel(sigma: f32) -> Vec<f32> {
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

pub(super) fn blurred(img: &RgbaImage, sigma: f32) -> Rgba32FImage {
    let pre = premultiplied(img);
    if sigma <= 0.0 {
        return pre;
    }
    imageproc::filter::separable_filter_equal(&pre, &gaussian_kernel(sigma))
}

pub(super) fn blur(img: &RgbaImage, sigma: f32) -> RgbaImage {
    unpremultiplied(&blurred(img, sigma))
}

pub(super) fn unsharp(mut img: RgbaImage, sigma: f32, amount: f32, threshold: f32) -> RgbaImage {
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
pub(super) fn pixelate(mut img: RgbaImage, size: u32) -> RgbaImage {
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

pub(super) fn pad(img: &RgbaImage, p: &PaddingSpec) -> RgbaImage {
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

pub(super) fn opacity(mut img: RgbaImage, o: f32) -> RgbaImage {
    for p in img.pixels_mut() {
        p.0[3] = to_u8(unit(p.0[3]) * o);
    }
    img
}

/// Multiplies the alpha of `img` by the luminance of `mask` scaled to its size.
pub(super) fn apply_mask(mut img: RgbaImage, mask: &RgbaImage) -> Result<RgbaImage, ImageError> {
    let mask = resize(mask, img.dimensions(), Resample::Lanczos)?;
    for (p, m) in img.pixels_mut().zip(mask.pixels()) {
        let [r, g, b, a] = m.0.map(unit);
        // The mask's own transparency counts as black.
        let l = luma([r, g, b]) * a;
        p.0[3] = to_u8(unit(p.0[3]) * l);
    }
    Ok(img)
}
