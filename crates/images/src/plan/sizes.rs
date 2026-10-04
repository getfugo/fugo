//! The sizes of the steps: fit, cover, crop and rotation.

use super::*;

/// Rounds half up like the resize maths of the Go implementation (`int(x + 0.5)` on
/// non-negative values).
pub(super) fn round_half_up(x: f64) -> u32 {
    let r = (x + 0.5).floor();
    if r <= 0.0 {
        0
    } else if r >= f64::from(u32::MAX) {
        u32::MAX
    } else {
        // In range and integral: the conversion is exact.
        r as u32
    }
}

/// The size of a resize to `width` and/or `height` (the missing side keeps the aspect).
#[must_use]
pub fn resize_size((sw, sh): Size, width: Option<u32>, height: Option<u32>) -> Size {
    match (width, height) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (
            w,
            round_half_up(f64::from(w) * f64::from(sh) / f64::from(sw)).max(1),
        ),
        (None, Some(h)) => (
            round_half_up(f64::from(h) * f64::from(sw) / f64::from(sh)).max(1),
            h,
        ),
        (None, None) => (sw, sh),
    }
}

/// The size of fitting (scaling down, never up) into `w`×`h`.
#[must_use]
pub fn fit_size((sw, sh): Size, (w, h): Size) -> Size {
    if sw <= w && sh <= h {
        return (sw, sh);
    }
    let wratio = f64::from(sw) / f64::from(w);
    let hratio = f64::from(sh) / f64::from(h);
    if wratio > hratio {
        (w, round_half_up(f64::from(sh) / wratio).min(h))
    } else {
        (round_half_up(f64::from(sw) / hratio).min(w), h)
    }
}

/// The intermediate size of a fill to `w`×`h`: the smallest scale that covers the box.
#[must_use]
pub fn cover_size((sw, sh): Size, (w, h): Size) -> Size {
    let wratio = f64::from(sw) / f64::from(w);
    let hratio = f64::from(sh) / f64::from(h);
    if wratio < hratio {
        (w, round_half_up(f64::from(sh) / wratio).max(h))
    } else {
        (round_half_up(f64::from(sw) / hratio).max(w), h)
    }
}

/// The region of a `w`×`h` crop at `anchor`, intersected with the image.
#[must_use]
pub fn crop_rect(src: Size, (w, h): Size, anchor: Anchor) -> (u32, u32, Size) {
    let (x, y) = anchor.offset(src, (w, h));
    let clamp = |v: i64, max: u32| u32::try_from(v.clamp(0, i64::from(max))).unwrap_or(0);
    let x0 = clamp(x, src.0);
    let y0 = clamp(y, src.1);
    let x1 = clamp(x + i64::from(w), src.0);
    let y1 = clamp(y + i64::from(h), src.1);
    (x0, y0, (x1 - x0, y1 - y0))
}

/// The size of a smart crop to `target` that keeps `region` of an image of `size`: gift
/// crops the region (intersected with the image), then crops its centre to the target
/// (`CropToSize` at the centre anchor); nothing is left of an empty region.
pub(super) fn smart_crop_size(region: Rect, size: Size, target: Size) -> Size {
    let kept = region.intersect(Rect::of_size(size)).size();
    if kept.0 == 0 || kept.1 == 0 {
        return (0, 0);
    }
    crop_rect(kept, target, Anchor::Center).2
}

/// Sine and cosine of `degrees`, exact for multiples of 90.
pub(crate) fn sin_cos(degrees: u32) -> (f32, f32) {
    match degrees % 360 {
        0 => (0.0, 1.0),
        90 => (1.0, 0.0),
        180 => (0.0, -1.0),
        270 => (-1.0, 0.0),
        d => {
            let (s, c) = f64::from(d).to_radians().sin_cos();
            (s as f32, c as f32)
        }
    }
}

/// The size of the canvas that holds an image rotated by `degrees`: the bounding box of
/// the rotated pixel centres, plus one pixel, plus a one-pixel margin on each side when the
/// box is not a whole number of pixels.
#[must_use]
pub fn rotated_size((w, h): Size, degrees: u32) -> Size {
    match degrees % 360 {
        0 | 180 => return (w, h),
        90 | 270 => return (h, w),
        _ => {}
    }
    let (wf, hf) = (w as f32, h as f32);
    let (s, c) = sin_cos(degrees);
    let (xoff, yoff) = (wf.mul_add(0.5, -0.5), hf.mul_add(0.5, -0.5));
    let corners = [
        (-xoff, -yoff),
        (wf - 1.0 - xoff, -yoff),
        (wf - 1.0 - xoff, hf - 1.0 - yoff),
        (-xoff, hf - 1.0 - yoff),
    ];
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for (x, y) in corners {
        let rx = x * c - y * s;
        let ry = x * s + y * c;
        min_x = min_x.min(rx);
        max_x = max_x.max(rx);
        min_y = min_y.min(ry);
        max_y = max_y.max(ry);
    }
    let side = |span: f32| {
        let mut v = span + 1.0;
        if v - v.floor() > 0.01 {
            v += 2.0;
        }
        round_half_up(f64::from(v.floor()))
    };
    (side(max_x - min_x), side(max_y - min_y))
}

/// Whether an EXIF orientation swaps width and height.
pub(crate) const fn orientation_swaps(o: u8) -> bool {
    matches!(o, 5..=8)
}
