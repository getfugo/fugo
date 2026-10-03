//! Image filters (`images.Filter` and friends), deserialized from template maps such as
//! `{"op": "overlay", "image": logo, "x": 10, "y": 10}` or `{"op": "text", "text": "Hi"}`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use ssg_base::ImageOpId;

use crate::color::Color;
use crate::dither::DitherSpec;
use crate::error::ImageError;
use crate::spec::ImageSpec;
use crate::text::TextSpec;

/// An image a filter or an operation reads: a source file, an image the queue holds in memory
/// ([`ImageQueue::add_memory`](crate::ImageQueue::add_memory): a remote resource, a QR code),
/// or the result of a queued operation. In template maps a string is a file path, an integer
/// an operation id and `{"memory": …, "name": …}` an image in memory.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ImageInput {
    Op(ImageOpId),
    File(PathBuf),
    Memory(MemoryImage),
}

/// An image the queue holds in memory: the xxh3 of its bytes, and its file name (whose stem
/// the processed images keep, as for a file).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MemoryImage {
    pub memory: u64,
    pub name: String,
}

/// One filter of a filter chain. Colour filters work on non-premultiplied 8-bit channels
/// and keep the alpha channel; percentages are −100…100 unless noted.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ImageFilter {
    /// Adds `percentage`/100 to every channel.
    Brightness {
        percentage: f32,
    },
    /// Stretches (positive) or flattens (negative) the channels around 0.5.
    Contrast {
        percentage: f32,
    },
    /// `c^(1/gamma)`: above 1 brightens, below 1 darkens.
    Gamma {
        gamma: f32,
    },
    /// A Gaussian blur of standard deviation `sigma` pixels.
    GaussianBlur {
        sigma: f32,
    },
    Grayscale,
    /// Rotates the hue by `shift` degrees (−180…180).
    Hue {
        shift: f32,
    },
    Invert,
    /// Tints the image: `hue` in degrees, `saturation` and `percentage` 0…100.
    Colorize {
        hue: f32,
        saturation: f32,
        percentage: f32,
    },
    /// Scales the red, green and blue channels by the given percentages (−100…500).
    ColorBalance {
        r: f32,
        g: f32,
        b: f32,
    },
    /// −100…500.
    Saturation {
        percentage: f32,
    },
    /// 0…100.
    Sepia {
        percentage: f32,
    },
    /// A sigmoid contrast curve; `midpoint` 0…1, `factor` −10…10.
    Sigmoid {
        midpoint: f32,
        factor: f32,
    },
    /// Sharpens channels that differ from their blurred value by more than `threshold`.
    UnsharpMask {
        sigma: f32,
        amount: f32,
        threshold: f32,
    },
    /// Averages square cells of `size` pixels.
    Pixelate {
        size: u32,
    },
    /// Multiplies the alpha channel by `opacity` (0…1).
    Opacity {
        opacity: f32,
    },
    /// Adds (or with negative values removes) a canvas border.
    Padding(PaddingSpec),
    /// Draws `image` over the input with its top-left corner at (`x`, `y`).
    Overlay {
        image: ImageInput,
        #[serde(default)]
        x: i32,
        #[serde(default)]
        y: i32,
    },
    /// Uses the luminance of `image` (scaled to the input) as the input's alpha.
    Mask {
        image: ImageInput,
    },
    /// Applies the EXIF orientation of the source.
    AutoOrient,
    /// Draws text (`images.Text`), wrapped and aligned as the Go implementation does.
    Text(TextSpec),
    /// Reduces the image to a palette by error diffusion or ordered dithering
    /// (`images.Dither`).
    Dither(DitherSpec),
    /// A processing spec inside a filter chain; the last `Process` also decides the target
    /// format, quality, hint and background.
    Process {
        spec: ImageSpec,
    },
}

impl ImageFilter {
    /// The `op` name.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Brightness { .. } => "brightness",
            Self::Contrast { .. } => "contrast",
            Self::Gamma { .. } => "gamma",
            Self::GaussianBlur { .. } => "gaussian_blur",
            Self::Grayscale => "grayscale",
            Self::Hue { .. } => "hue",
            Self::Invert => "invert",
            Self::Colorize { .. } => "colorize",
            Self::ColorBalance { .. } => "color_balance",
            Self::Saturation { .. } => "saturation",
            Self::Sepia { .. } => "sepia",
            Self::Sigmoid { .. } => "sigmoid",
            Self::UnsharpMask { .. } => "unsharp_mask",
            Self::Pixelate { .. } => "pixelate",
            Self::Opacity { .. } => "opacity",
            Self::Padding(_) => "padding",
            Self::Overlay { .. } => "overlay",
            Self::Mask { .. } => "mask",
            Self::AutoOrient => "auto_orient",
            Self::Text(_) => "text",
            Self::Dither(_) => "dither",
            Self::Process { .. } => "process",
        }
    }
}

/// The largest padding on one side, in pixels.
pub const MAX_PADDING: i32 = 5000;

/// The border of [`ImageFilter::Padding`].
///
/// In template maps it is either `margin` (CSS shorthand: one to four values, top first,
/// clockwise) or the sides `top`, `right`, `bottom`, `left` (missing sides are 0), plus an
/// optional `color` (white by default).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "RawPadding")]
pub struct PaddingSpec {
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub left: i32,
    pub color: Color,
}

impl PaddingSpec {
    /// Padding from CSS shorthand values (one to four).
    ///
    /// # Errors
    /// No value, more than four, or a value above [`MAX_PADDING`].
    pub fn from_shorthand(values: &[i32], color: Color) -> Result<Self, ImageError> {
        let [top, right, bottom, left] = match *values {
            [a] => [a, a, a, a],
            [v, h] => [v, h, v, h],
            [t, h, b] => [t, h, b, h],
            [t, r, b, l] => [t, r, b, l],
            _ => {
                return Err(ImageError::filter(
                    "padding",
                    format!("expected 1 to 4 values, got {}", values.len()),
                ));
            }
        };
        Self {
            top,
            right,
            bottom,
            left,
            color,
        }
        .check()
    }

    fn check(self) -> Result<Self, ImageError> {
        if [self.top, self.right, self.bottom, self.left]
            .iter()
            .any(|&v| v > MAX_PADDING)
        {
            return Err(ImageError::filter(
                "padding",
                format!("values must not exceed {MAX_PADDING} pixels"),
            ));
        }
        Ok(self)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPadding {
    #[serde(default)]
    margin: Option<Margin>,
    #[serde(default)]
    top: Option<i32>,
    #[serde(default)]
    right: Option<i32>,
    #[serde(default)]
    bottom: Option<i32>,
    #[serde(default)]
    left: Option<i32>,
    #[serde(default)]
    color: Option<Color>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Margin {
    One(i32),
    Many(Vec<i32>),
}

impl TryFrom<RawPadding> for PaddingSpec {
    type Error = ImageError;

    fn try_from(r: RawPadding) -> Result<Self, ImageError> {
        let color = r.color.unwrap_or(Color::WHITE);
        let sides = [r.top, r.right, r.bottom, r.left];
        match r.margin {
            Some(_) if sides.iter().any(Option::is_some) => Err(ImageError::filter(
                "padding",
                "give either margin or top/right/bottom/left, not both",
            )),
            Some(Margin::One(v)) => Self::from_shorthand(&[v], color),
            Some(Margin::Many(v)) => Self::from_shorthand(&v, color),
            None => Self {
                top: r.top.unwrap_or(0),
                right: r.right.unwrap_or(0),
                bottom: r.bottom.unwrap_or(0),
                left: r.left.unwrap_or(0),
                color,
            }
            .check(),
        }
    }
}
