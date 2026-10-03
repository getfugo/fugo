//! Planning: a spec and a filter chain become concrete [`Step`]s with the size of every
//! intermediate result, from the source's metadata alone (no pixels are decoded, but for the
//! rare smart crop whose result's size depends on its region: [`Plan::smart_crop`]).

use std::collections::BTreeSet;
use std::fmt;

use crate::color::Color;
use crate::dither::DitherSpec;
use crate::error::ImageError;
use crate::filter::{ImageFilter, ImageInput, PaddingSpec};
use crate::font::FontData;
use crate::format::ImageFormat;
use crate::settings::Imaging;
use crate::smartcrop::{self, Rect};
use crate::spec::{Action, Anchor, Hint, ImageSpec, Resample, ResolvedSpec};
use crate::text::{FontInput, TextSpec};

/// A width and height in pixels.
pub type Size = (u32, u32);

/// An image read by a step (overlay, mask) with the identity its hash uses: the xxh3 of a
/// file's bytes, or an operation id.
#[derive(Clone, PartialEq)]
pub(crate) struct InputRef {
    pub input: ImageInput,
    pub identity: u64,
}

impl fmt::Debug for InputRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.identity)
    }
}

/// One concrete operation on pixels.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Step {
    /// Counter-clockwise rotation; `size` is the result's size.
    Rotate {
        degrees: u32,
        size: Size,
    },
    /// Scale to exactly this size.
    Resize {
        size: Size,
        filter: Resample,
    },
    /// Keep the region at `(x, y)` of this size (already inside the image).
    Crop {
        x: u32,
        y: u32,
        size: Size,
    },
    /// A crop or fill to `target` at the `smart` anchor (`smartcrop.rs`): keep the region
    /// the Go implementation's smart crop finds on the operation's source for `target` and `filter`
    /// (intersected with the image), then resize it to `target` (fill) or keep its centre at
    /// `target` (crop). The region depends on the source's pixels, so it is found when the
    /// steps run. A fill whose region is empty fills at the centre anchor instead (the Go
    /// implementation's fallback, [`Plan::smart_crop`]).
    SmartCrop {
        action: Action,
        target: Size,
        filter: Resample,
    },
    /// Apply an EXIF orientation (2–8).
    Orient(u8),
    /// A colour filter (size-preserving, reads no other image).
    Adjust(ImageFilter),
    Padding(PaddingSpec),
    Opacity(f32),
    Overlay {
        image: InputRef,
        x: i32,
        y: i32,
    },
    Mask {
        image: InputRef,
    },
    /// Text drawn with `font` (the spec's own `font` is cleared: the font's identity is its
    /// content, which `font` prints).
    Text {
        spec: TextSpec,
        font: FontData,
    },
    Dither(DitherSpec),
}

/// How the result is written.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Encode {
    pub format: ImageFormat,
    pub quality: u8,
    pub hint: Hint,
    /// An explicit background: transparent results are flattened onto it.
    pub background: Option<Color>,
    /// The background of formats without transparency.
    pub default_background: Color,
}

/// Steps, the final size and the encoding of one operation.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Plan {
    pub steps: Vec<Step>,
    pub size: Size,
    pub encode: Encode,
}

/// What planning needs to know about the input.
pub(crate) struct InputInfo {
    pub size: Size,
    pub format: ImageFormat,
    /// The EXIF orientation (1–8) when the input is a file that has one.
    pub orientation: Option<u8>,
}

/// Finds the region of a smart crop to a target size with a resample filter on the
/// operation's source (`smartcrop::find`), for [`Plan::new`] when the result's size depends
/// on it.
pub(crate) type ResolveSmart<'a> = dyn FnMut(Size, Resample) -> Result<Rect, ImageError> + 'a;

/// Rounds half up like the resize maths of the Go implementation (`int(x + 0.5)` on
/// non-negative values).
fn round_half_up(x: f64) -> u32 {
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
fn smart_crop_size(region: Rect, size: Size, target: Size) -> Size {
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

impl Plan {
    /// The steps of `spec` and `filters` applied to an input.
    ///
    /// `resolve_input` gives the identity of the images overlay and mask filters read,
    /// `resolve_font` the font of a text filter (`None`: the default font), `resolve_smart`
    /// the region of a smart crop whose result's size depends on it (rare: see
    /// [`Plan::smart_crop`]).
    pub(crate) fn new(
        input: &InputInfo,
        spec: Option<&ImageSpec>,
        filters: &[ImageFilter],
        imaging: &Imaging,
        resolve_input: &mut dyn FnMut(&ImageInput) -> Result<InputRef, ImageError>,
        resolve_font: &mut dyn FnMut(Option<&FontInput>) -> Result<FontData, ImageError>,
        resolve_smart: &mut ResolveSmart<'_>,
    ) -> Result<Self, ImageError> {
        let mut encode = Encode {
            format: input.format,
            quality: imaging.quality,
            hint: imaging.hint,
            background: None,
            default_background: imaging.background,
        };
        let mut plan = Self {
            steps: Vec::new(),
            size: input.size,
            encode: encode.clone(),
        };
        // The encoding options of every spec, later ones overriding earlier ones.
        let mut merged = ImageSpec::default();
        let mut merge = |s: &ImageSpec| {
            merged.format = s.format.or(merged.format);
            merged.quality = s.quality.or(merged.quality);
            merged.hint = s.hint.or(merged.hint);
            merged.background = s.background.or(merged.background);
        };
        if let Some(spec) = spec {
            merge(spec);
            plan.push_spec(
                &spec.resolve(imaging, input.format),
                input.size,
                resolve_smart,
            )?;
        }
        for f in filters {
            match f {
                ImageFilter::Process { spec } => {
                    merge(spec);
                    plan.push_spec(
                        &spec.resolve(imaging, input.format),
                        input.size,
                        resolve_smart,
                    )?;
                }
                ImageFilter::AutoOrient => {
                    if let Some(o @ 2..=8) = input.orientation {
                        plan.steps.push(Step::Orient(o));
                        if orientation_swaps(o) {
                            plan.size = (plan.size.1, plan.size.0);
                        }
                    }
                }
                ImageFilter::Padding(p) => {
                    let grow =
                        |side: u32, a: i32, b: i32| i64::from(side) + i64::from(a) + i64::from(b);
                    let w = grow(plan.size.0, p.left, p.right);
                    let h = grow(plan.size.1, p.top, p.bottom);
                    let size = u32::try_from(w)
                        .ok()
                        .zip(u32::try_from(h).ok())
                        .filter(|&(w, h)| w >= 1 && h >= 1)
                        .ok_or_else(|| {
                            ImageError::filter(
                                "padding",
                                format!(
                                    "a {}x{} image padded by {} {} {} {} would be empty",
                                    plan.size.0, plan.size.1, p.top, p.right, p.bottom, p.left
                                ),
                            )
                        })?;
                    plan.steps.push(Step::Padding(*p));
                    plan.size = size;
                }
                ImageFilter::Opacity { opacity } => {
                    plan.steps.push(Step::Opacity(opacity.clamp(0.0, 1.0)));
                }
                ImageFilter::Overlay { image, x, y } => {
                    let image = resolve_input(image)?;
                    plan.steps.push(Step::Overlay {
                        image,
                        x: *x,
                        y: *y,
                    });
                }
                ImageFilter::Mask { image } => {
                    let image = resolve_input(image)?;
                    plan.steps.push(Step::Mask { image });
                }
                ImageFilter::Text(t) => {
                    let t = t.clone().check()?;
                    let font = resolve_font(t.font.as_ref())?;
                    plan.steps.push(Step::Text {
                        spec: TextSpec { font: None, ..t },
                        font,
                    });
                }
                ImageFilter::Dither(d) => plan.steps.push(Step::Dither(d.clone().check()?)),
                ImageFilter::Pixelate { size: 0 } => {
                    return Err(ImageError::filter(
                        "pixelate",
                        "the cell size must be positive",
                    ));
                }
                ImageFilter::GaussianBlur { sigma } | ImageFilter::UnsharpMask { sigma, .. }
                    if !sigma.is_finite() || *sigma < 0.0 =>
                {
                    return Err(ImageError::filter(
                        f.name(),
                        format!("sigma {sigma} must be a non-negative number"),
                    ));
                }
                other => plan.steps.push(Step::Adjust(other.clone())),
            }
        }
        encode.format = merged.format.unwrap_or(input.format);
        encode.quality = merged.quality.unwrap_or(imaging.quality);
        encode.hint = merged.hint.unwrap_or(imaging.hint);
        encode.background = merged.background;
        // Checked here rather than by the encoder, so that the template call gets the error.
        if let Some(max) = encode.format.max_side()
            && (plan.size.0 > max || plan.size.1 > max)
        {
            return Err(ImageError::Encode {
                format: encode.format,
                reason: format!(
                    "the result would be {}x{} pixels; {} images are at most {max}x{max}",
                    plan.size.0, plan.size.1, encode.format
                ),
            });
        }
        plan.encode = encode;
        Ok(plan)
    }

    /// Adds the steps of `spec`; `source` is the size of the operation's source, which smart
    /// crops analyse.
    fn push_spec(
        &mut self,
        spec: &ResolvedSpec,
        source: Size,
        resolve_smart: &mut ResolveSmart<'_>,
    ) -> Result<(), ImageError> {
        if spec.rotate != 0 {
            let size = rotated_size(self.size, spec.rotate);
            self.steps.push(Step::Rotate {
                degrees: spec.rotate,
                size,
            });
            self.size = size;
        }
        let Some(action) = spec.action else {
            return Ok(());
        };
        let src = self.size;
        let boxed = || (spec.width.unwrap_or(0), spec.height.unwrap_or(0));
        match action {
            Action::Resize => {
                self.resize(resize_size(src, spec.width, spec.height), spec.filter);
            }
            Action::Fit => self.resize(fit_size(src, boxed()), spec.filter),
            Action::Fill | Action::Crop if spec.anchor == Anchor::Smart => {
                self.smart_crop(action, boxed(), spec.filter, source, resolve_smart)?;
            }
            Action::Fill => {
                let target = boxed();
                self.resize(cover_size(src, target), spec.filter);
                self.crop(target, spec.anchor);
            }
            Action::Crop => self.crop(boxed(), spec.anchor),
        }
        if self.size.0 == 0 || self.size.1 == 0 {
            return Err(ImageError::spec(
                format!("{action}"),
                format!("the result of {}x{} would be empty", src.0, src.1),
            ));
        }
        Ok(())
    }

    fn resize(&mut self, size: Size, filter: Resample) {
        if size != self.size {
            self.steps.push(Step::Resize { size, filter });
        }
        self.size = size;
    }

    /// A smart crop or fill (the Go implementation's `FiltersFromConfig`: `gift.Crop` of the
    /// region, then `gift.Resize` or `gift.CropToSize` at the centre).
    ///
    /// A fill always ends at the target size: when the region is empty (smartcrop has no
    /// candidate, or none scores above −1), the Go implementation fills again at the centre anchor
    /// (`processOptions` in `resources/image.go`, issue 7955), and so does the step
    /// ([`crate::pixels`]). A crop ends at the target size clipped to the region, and the
    /// regions smartcrop can pick for a source differ by a pixel or so after scaling back from
    /// its analysis, so when they would give different sizes (a target about the size of the
    /// source) the region is found here, from the pixels. A crop with no candidate is empty
    /// (the Go implementation writes an empty image; an error here).
    fn smart_crop(
        &mut self,
        action: Action,
        target: Size,
        filter: Resample,
        source: Size,
        resolve_smart: &mut ResolveSmart<'_>,
    ) -> Result<(), ImageError> {
        let size = if action == Action::Fill {
            target
        } else {
            let sizes: BTreeSet<Size> = smartcrop::candidates(source, target.0, target.1)
                .into_iter()
                .map(|r| smart_crop_size(r, self.size, target))
                .collect();
            match sizes.first() {
                Some(&only) if sizes.len() == 1 => only,
                _ => smart_crop_size(resolve_smart(target, filter)?, self.size, target),
            }
        };
        if size.0 == 0 || size.1 == 0 {
            return Err(ImageError::spec(
                format!("{action}"),
                format!(
                    "the smart anchor keeps nothing of {}x{} for {}x{} (the result would be \
                     empty); use another anchor",
                    source.0, source.1, target.0, target.1
                ),
            ));
        }
        self.steps.push(Step::SmartCrop {
            action,
            target,
            filter,
        });
        self.size = size;
        Ok(())
    }

    fn crop(&mut self, target: Size, anchor: Anchor) {
        let (x, y, size) = crop_rect(self.size, target, anchor);
        if size != self.size {
            self.steps.push(Step::Crop { x, y, size });
        }
        self.size = size;
    }

    /// The identity of the plan for hashing (inputs by content, not by path).
    pub(crate) fn key(&self) -> String {
        format!("{:?}|{:?}|{:?}", self.steps, self.size, self.encode)
    }
}
