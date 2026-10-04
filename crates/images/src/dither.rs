//! The dither filter (`images.Dither`): the image reduced to a palette, by error diffusion or
//! ordered dithering, with the Go implementation's options and defaults
//! (`resources/images/filters.go`).
//!
//! The Go implementation delegates to `github.com/makeworld-the-better-one/dither/v2` (MPL-2.0).
//! None of its code is used here: this module implements the published algorithms it names, with
//! the behaviour its documentation describes:
//!
//! * colours are compared in linear RGB (the sRGB transfer function undone, 16-bit scale), by
//!   squared Euclidean distance weighted by the luminance coefficients 0.2126, 0.7152 and
//!   0.0722 (ITU-R BT.709); the nearest palette colour wins, the first one on a tie;
//! * palette colours are taken as written (their alpha is ignored); every result pixel is a
//!   palette colour with the source pixel's alpha, and fully transparent pixels are left alone;
//! * error diffusion keeps the error in linear RGB; with `serpentine`, rows with an even index
//!   (the first row included) are processed right to left and the kernel mirrored;
//!   `strength` scales the kernel's weights;
//! * ordered dithering adds `65535 · strength · (cell / max − 0.50000006)` to each linear channel
//!   (the ordered-dithering threshold map as an offset; the constant is the float just above
//!   ½, so that black stays black) and rounds to the nearest colour.
//!
//! Where the error-diffusion kernels and threshold matrices come from is noted at each one.

use std::sync::LazyLock;

use image::{Rgba, RgbaImage};
use serde::{Deserialize, Serialize};

use crate::color::Color;
use crate::error::ImageError;

mod diffusion;
mod ordered;

use diffusion::*;
pub(crate) use ordered::*;

named_enum! {
    /// A dithering method: error diffusion (the first fourteen, `floydsteinberg` by default) or
    /// ordered dithering with a threshold matrix (the Go implementation's names,
    /// case-insensitive).
    pub enum DitherMethod ("dithering method") {
        Atkinson = "atkinson",
        Burkes = "burkes",
        FalseFloydSteinberg = "falsefloydsteinberg",
        FloydSteinberg = "floydsteinberg",
        JarvisJudiceNinke = "jarvisjudiceninke",
        Sierra = "sierra",
        Sierra2 = "sierra2",
        Sierra24A = "sierra2_4a",
        Sierra3 = "sierra3",
        SierraLite = "sierralite",
        Simple2D = "simple2d",
        StevenPigeon = "stevenpigeon",
        Stucki = "stucki",
        TwoRowSierra = "tworowsierra",
        ClusteredDot4x4 = "clustereddot4x4",
        ClusteredDot6x6 = "clustereddot6x6",
        ClusteredDot6x6_2 = "clustereddot6x6_2",
        ClusteredDot6x6_3 = "clustereddot6x6_3",
        ClusteredDot8x8 = "clustereddot8x8",
        ClusteredDotDiagonal16x16 = "clustereddotdiagonal16x16",
        ClusteredDotDiagonal6x6 = "clustereddotdiagonal6x6",
        ClusteredDotDiagonal8x8 = "clustereddotdiagonal8x8",
        ClusteredDotDiagonal8x8_2 = "clustereddotdiagonal8x8_2",
        ClusteredDotDiagonal8x8_3 = "clustereddotdiagonal8x8_3",
        ClusteredDotHorizontalLine = "clustereddothorizontalline",
        ClusteredDotSpiral5x5 = "clustereddotspiral5x5",
        ClusteredDotVerticalLine = "clustereddotverticalline",
        Horizontal3x5 = "horizontal3x5",
        Vertical5x3 = "vertical5x3",
    }
}

impl DitherMethod {
    /// Whether the method diffuses the error (else it is ordered dithering).
    #[must_use]
    pub const fn is_error_diffusion(self) -> bool {
        matches!(self.algorithm(), Algorithm::Diffusion(_))
    }

    const fn algorithm(self) -> Algorithm {
        use DitherMethod as M;
        match self {
            M::Atkinson => Algorithm::Diffusion(&ATKINSON),
            M::Burkes => Algorithm::Diffusion(&BURKES),
            M::FalseFloydSteinberg => Algorithm::Diffusion(&FALSE_FLOYD_STEINBERG),
            M::FloydSteinberg => Algorithm::Diffusion(&FLOYD_STEINBERG),
            M::JarvisJudiceNinke => Algorithm::Diffusion(&JARVIS_JUDICE_NINKE),
            M::Sierra | M::Sierra3 => Algorithm::Diffusion(&SIERRA),
            M::Sierra2 | M::TwoRowSierra => Algorithm::Diffusion(&TWO_ROW_SIERRA),
            M::Sierra24A | M::SierraLite => Algorithm::Diffusion(&SIERRA_LITE),
            M::Simple2D => Algorithm::Diffusion(&SIMPLE_2D),
            M::StevenPigeon => Algorithm::Diffusion(&STEVEN_PIGEON),
            M::Stucki => Algorithm::Diffusion(&STUCKI),
            M::ClusteredDot4x4 => Algorithm::Ordered(Threshold::ClusteredDot4x4),
            M::ClusteredDot6x6 => Algorithm::Ordered(Threshold::ClusteredDot6x6),
            M::ClusteredDot6x6_2 => Algorithm::Ordered(Threshold::ClusteredDot6x6_2),
            M::ClusteredDot6x6_3 => Algorithm::Ordered(Threshold::ClusteredDot6x6_3),
            M::ClusteredDot8x8 => Algorithm::Ordered(Threshold::ClusteredDot8x8),
            M::ClusteredDotDiagonal16x16 => Algorithm::Ordered(Threshold::Diagonal16x16),
            M::ClusteredDotDiagonal6x6 => Algorithm::Ordered(Threshold::Diagonal6x6),
            M::ClusteredDotDiagonal8x8 => Algorithm::Ordered(Threshold::Diagonal8x8),
            M::ClusteredDotDiagonal8x8_2 => Algorithm::Ordered(Threshold::Diagonal8x8_2),
            M::ClusteredDotDiagonal8x8_3 => Algorithm::Ordered(Threshold::Diagonal8x8_3),
            M::ClusteredDotHorizontalLine => Algorithm::Ordered(Threshold::HorizontalLine),
            M::ClusteredDotSpiral5x5 => Algorithm::Ordered(Threshold::Spiral5x5),
            M::ClusteredDotVerticalLine => Algorithm::Ordered(Threshold::VerticalLine),
            M::Horizontal3x5 => Algorithm::Ordered(Threshold::Horizontal3x5),
            M::Vertical5x3 => Algorithm::Ordered(Threshold::Vertical5x3),
        }
    }
}

/// The options of the dither filter.
///
/// In template maps (the Go implementation's option names, any case): `colors` (two or more
/// `#rrggbb` colours; black and white by default), `method` ([`DitherMethod`], `floydsteinberg` by
/// default), `serpentine` (error diffusion only; true by default), `strength` (1.0 by default; 0.8
/// is less noisy).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawDither")]
pub struct DitherSpec {
    pub colors: Vec<Color>,
    pub method: DitherMethod,
    pub serpentine: bool,
    pub strength: f32,
}

impl Default for DitherSpec {
    fn default() -> Self {
        Self {
            colors: vec![Color([0, 0, 0, 255]), Color::WHITE],
            method: DitherMethod::FloydSteinberg,
            serpentine: true,
            strength: 1.0,
        }
    }
}

impl DitherSpec {
    pub(crate) fn check(self) -> Result<Self, ImageError> {
        if self.colors.len() < 2 {
            return Err(ImageError::filter(
                "dither",
                "the palette needs at least two colors",
            ));
        }
        if !self.strength.is_finite() {
            return Err(ImageError::filter(
                "dither",
                format!("strength {} is not a number", self.strength),
            ));
        }
        Ok(self)
    }
}

/// `strength`: a number or a numeric string.
#[derive(Deserialize)]
#[serde(untagged)]
enum Strength {
    Number(f32),
    Text(String),
}

/// The dither filter as written in a template map (`mapstructure` matches Go's field names
/// ignoring case).
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDither {
    #[serde(default, alias = "Colors", alias = "COLORS")]
    colors: Option<Vec<Color>>,
    #[serde(default, alias = "Method", alias = "METHOD")]
    method: Option<DitherMethod>,
    #[serde(default, alias = "Serpentine", alias = "SERPENTINE")]
    serpentine: Option<bool>,
    #[serde(default, alias = "Strength", alias = "STRENGTH")]
    strength: Option<Strength>,
}

impl TryFrom<RawDither> for DitherSpec {
    type Error = ImageError;

    fn try_from(r: RawDither) -> Result<Self, ImageError> {
        let d = Self::default();
        let strength = match r.strength {
            None => d.strength,
            Some(Strength::Number(v)) => v,
            Some(Strength::Text(s)) => s.trim().parse().map_err(|_| {
                ImageError::filter("dither", format!("strength {s:?} is not a number"))
            })?,
        };
        Self {
            colors: r.colors.unwrap_or(d.colors),
            method: r.method.unwrap_or(d.method),
            serpentine: r.serpentine.unwrap_or(d.serpentine),
            strength,
        }
        .check()
    }
}

// ── colour ───────────────────────────────────────────────────────────────────────────────────

/// Full scale of the linear channels.
const MAX: f32 = 65535.0;

/// The sRGB transfer function undone (IEC 61966-2-1), on 8-bit values, in `0..=65535`.
static LINEAR: LazyLock<[f32; 256]> = LazyLock::new(|| {
    std::array::from_fn(|i| {
        let v = i as f64 / 255.0;
        let l = if v <= 0.040_45 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        };
        (l * f64::from(MAX)) as f32
    })
});

fn linear([r, g, b, _]: [u8; 4]) -> [f32; 3] {
    let lut = &*LINEAR;
    [
        lut[usize::from(r)],
        lut[usize::from(g)],
        lut[usize::from(b)],
    ]
}

/// The palette: sRGB colours and their linear values.
struct Palette {
    srgb: Vec<[u8; 3]>,
    linear: Vec<[f32; 3]>,
}

impl Palette {
    fn new(colors: &[Color]) -> Self {
        Self {
            srgb: colors.iter().map(|c| [c.0[0], c.0[1], c.0[2]]).collect(),
            linear: colors.iter().map(|c| linear(c.0)).collect(),
        }
    }

    /// The nearest colour to linear `v`: luminance-weighted squared distance (BT.709
    /// coefficients), the first on a tie.
    fn nearest(&self, v: [f32; 3]) -> usize {
        let mut best = (0, f64::INFINITY);
        for (i, p) in self.linear.iter().enumerate() {
            let d = |c: usize| f64::from(v[c] - p[c]);
            let dist = 0.2126 * d(0) * d(0) + 0.7152 * d(1) * d(1) + 0.0722 * d(2) * d(2);
            if dist < best.1 {
                best = (i, dist);
            }
        }
        best.0
    }
}

// ── error diffusion ──────────────────────────────────────────────────────────────────────────

// ── ordered dithering ────────────────────────────────────────────────────────────────────────

// ── the filter ───────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
enum Algorithm {
    Diffusion(&'static Kernel),
    Ordered(Threshold),
}

/// Dithers `img` as `spec` says.
pub(crate) fn apply(mut img: RgbaImage, spec: &DitherSpec) -> RgbaImage {
    let palette = Palette::new(&spec.colors);
    match spec.method.algorithm() {
        Algorithm::Diffusion(kernel) => {
            diffuse(&mut img, &palette, kernel, spec.strength, spec.serpentine);
        }
        Algorithm::Ordered(t) => ordered(&mut img, &palette, t.matrix(), spec.strength),
    }
    img
}

/// The threshold matrix of an ordered method (tests).
#[cfg(test)]
pub(crate) fn matrix_of(method: DitherMethod) -> Option<&'static Matrix> {
    match method.algorithm() {
        Algorithm::Ordered(t) => Some(t.matrix()),
        Algorithm::Diffusion(_) => None,
    }
}

#[cfg(test)]
mod tests;
