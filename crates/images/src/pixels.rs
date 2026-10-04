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

mod adjust;
mod effects;
mod resample;

use adjust::*;
pub(crate) use effects::*;
pub(crate) use resample::*;

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

// ---------------------------------------------------------------------------------------
// Blur, sharpen, pixelate

// ---------------------------------------------------------------------------------------
// Compositing
