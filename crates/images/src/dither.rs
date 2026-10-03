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

/// An error-diffusion kernel: `(dx, dy, weight)` taps relative to the current pixel (`dy ≥ 0`,
/// `dx > 0` when `dy = 0`), weights over `divisor`.
struct Kernel {
    taps: &'static [(i32, i32, u16)],
    divisor: f32,
}

/// Floyd & Steinberg, "An adaptive algorithm for spatial grey scale", SID 1976.
const FLOYD_STEINBERG: Kernel = Kernel {
    taps: &[(1, 0, 7), (-1, 1, 3), (0, 1, 5), (1, 1, 1)],
    divisor: 16.0,
};

/// "False" Floyd–Steinberg: the three-tap simplification (right 3, below 3, below-right 2,
/// over 8), as in Tanner Helland's survey of dithering algorithms (2012).
const FALSE_FLOYD_STEINBERG: Kernel = Kernel {
    taps: &[(1, 0, 3), (0, 1, 3), (1, 1, 2)],
    divisor: 8.0,
};

/// Jarvis, Judice & Ninke, "A survey of techniques for the display of continuous tone
/// pictures on bilevel displays", CGIP 1976.
const JARVIS_JUDICE_NINKE: Kernel = Kernel {
    taps: &[
        (1, 0, 7),
        (2, 0, 5),
        (-2, 1, 3),
        (-1, 1, 5),
        (0, 1, 7),
        (1, 1, 5),
        (2, 1, 3),
        (-2, 2, 1),
        (-1, 2, 3),
        (0, 2, 5),
        (1, 2, 3),
        (2, 2, 1),
    ],
    divisor: 48.0,
};

/// Stucki, "MECCA — a multiple-error correcting computation algorithm for bilevel image
/// hardcopy reproduction", IBM Research 1981.
const STUCKI: Kernel = Kernel {
    taps: &[
        (1, 0, 8),
        (2, 0, 4),
        (-2, 1, 2),
        (-1, 1, 4),
        (0, 1, 8),
        (1, 1, 4),
        (2, 1, 2),
        (-2, 2, 1),
        (-1, 2, 2),
        (0, 2, 4),
        (1, 2, 2),
        (2, 2, 1),
    ],
    divisor: 42.0,
};

/// Bill Atkinson's (Apple, MacPaint): six eighths of the error, two pixels ahead and two rows
/// down.
const ATKINSON: Kernel = Kernel {
    taps: &[
        (1, 0, 1),
        (2, 0, 1),
        (-1, 1, 1),
        (0, 1, 1),
        (1, 1, 1),
        (0, 2, 1),
    ],
    divisor: 8.0,
};

/// Burkes (1988): the first two rows of Stucki's kernel.
const BURKES: Kernel = Kernel {
    taps: &[
        (1, 0, 8),
        (2, 0, 4),
        (-2, 1, 2),
        (-1, 1, 4),
        (0, 1, 8),
        (1, 1, 4),
        (2, 1, 2),
    ],
    divisor: 32.0,
};

/// Frankie Sierra's three-row kernel (1989), also called Sierra3.
const SIERRA: Kernel = Kernel {
    taps: &[
        (1, 0, 5),
        (2, 0, 3),
        (-2, 1, 2),
        (-1, 1, 4),
        (0, 1, 5),
        (1, 1, 4),
        (2, 1, 2),
        (-1, 2, 2),
        (0, 2, 3),
        (1, 2, 2),
    ],
    divisor: 32.0,
};

/// Sierra's two-row kernel (1990), also called Sierra2.
const TWO_ROW_SIERRA: Kernel = Kernel {
    taps: &[
        (1, 0, 4),
        (2, 0, 3),
        (-2, 1, 1),
        (-1, 1, 2),
        (0, 1, 3),
        (1, 1, 2),
        (2, 1, 1),
    ],
    divisor: 16.0,
};

/// Sierra's "filter lite" (1990), also called Sierra-2-4A.
const SIERRA_LITE: Kernel = Kernel {
    taps: &[(1, 0, 2), (-1, 1, 1), (0, 1, 1)],
    divisor: 4.0,
};

/// The simplest two-dimensional kernel: half the error to the right, half below.
const SIMPLE_2D: Kernel = Kernel {
    taps: &[(1, 0, 1), (0, 1, 1)],
    divisor: 2.0,
};

/// Stand-in for Steven Pigeon's kernel ("Dithering", Harder, Better, Faster, Stronger,
/// 2013-12-31), which could not be consulted offline: a sparse kernel with his reach (two
/// pixels ahead, two rows down) and weights falling with distance. Recorded in
/// `expected_diffs.toml`; replace it with the published matrix when available.
const STEVEN_PIGEON: Kernel = Kernel {
    taps: &[
        (1, 0, 2),
        (2, 0, 1),
        (-2, 1, 1),
        (-1, 1, 1),
        (0, 1, 2),
        (1, 1, 1),
        (2, 1, 1),
        (-1, 2, 1),
        (1, 2, 1),
    ],
    divisor: 11.0,
};

/// Error diffusion of `img` onto `palette` (see the module documentation).
fn diffuse(
    img: &mut RgbaImage,
    palette: &Palette,
    kernel: &Kernel,
    strength: f32,
    serpentine: bool,
) {
    let (w, h) = img.dimensions();
    let (wi, hi) = (w as usize, h as usize);
    let mut buf: Vec<[f32; 3]> = img.pixels().map(|p| linear(p.0)).collect();
    let taps: Vec<(i64, i64, f32)> = kernel
        .taps
        .iter()
        .map(|&(dx, dy, n)| {
            (
                i64::from(dx),
                i64::from(dy),
                f32::from(n) / kernel.divisor * strength,
            )
        })
        .collect();
    for y in 0..hi {
        let reverse = serpentine && y % 2 == 0;
        for i in 0..wi {
            let x = if reverse { wi - 1 - i } else { i };
            let idx = y * wi + x;
            let pixel = img.get_pixel_mut(x as u32, y as u32);
            if pixel.0[3] == 0 {
                continue;
            }
            let current = buf[idx].map(|v| v.clamp(0.0, MAX));
            let k = palette.nearest(current);
            let q = palette.linear[k];
            let [r, g, b] = palette.srgb[k];
            *pixel = Rgba([r, g, b, pixel.0[3]]);
            let err = [current[0] - q[0], current[1] - q[1], current[2] - q[2]];
            for &(dx, dy, weight) in &taps {
                let dx = if reverse { -dx } else { dx };
                let (Ok(tx), Ok(ty)) = (
                    usize::try_from(x as i64 + dx),
                    usize::try_from(y as i64 + dy),
                ) else {
                    continue;
                };
                if tx >= wi || ty >= hi {
                    continue;
                }
                let t = &mut buf[ty * wi + tx];
                for c in 0..3 {
                    t[c] += err[c] * weight;
                }
            }
        }
    }
}

// ── ordered dithering ────────────────────────────────────────────────────────────────────────

/// The threshold matrices of the ordered methods.
#[derive(Clone, Copy, Debug)]
enum Threshold {
    ClusteredDot4x4,
    ClusteredDot6x6,
    ClusteredDot6x6_2,
    ClusteredDot6x6_3,
    ClusteredDot8x8,
    Diagonal16x16,
    Diagonal6x6,
    Diagonal8x8,
    Diagonal8x8_2,
    Diagonal8x8_3,
    HorizontalLine,
    Spiral5x5,
    VerticalLine,
    Horizontal3x5,
    Vertical5x3,
}

/// A threshold matrix: `rows × cols` cells (row-major), each `< max`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Matrix {
    pub(crate) cols: usize,
    pub(crate) rows: usize,
    pub(crate) cells: Vec<u16>,
    pub(crate) max: u16,
}

impl Matrix {
    fn from_rows(rows: &[&[u16]], max: u16) -> Self {
        Self {
            cols: rows[0].len(),
            rows: rows.len(),
            cells: rows.iter().flat_map(|r| r.iter().copied()).collect(),
            max,
        }
    }

    fn transposed(&self) -> Self {
        let cells = (0..self.cols)
            .flat_map(|c| (0..self.rows).map(move |r| (r, c)))
            .map(|(r, c)| self.cells[r * self.cols + c])
            .collect();
        Self {
            cols: self.rows,
            rows: self.cols,
            cells,
            max: self.max,
        }
    }

    fn at(&self, x: u32, y: u32) -> u16 {
        self.cells[(y as usize % self.rows) * self.cols + x as usize % self.cols]
    }
}

/// The clockwise angle of `(dx, dy)` (image coordinates, y down) from the direction just
/// above "left", in `0..2π`: the order in which the classic 4×4 clustered dot grows around
/// its centre.
fn clockwise_from_left(dx: f64, dy: f64) -> f64 {
    let angle = (-dy).atan2(dx);
    (std::f64::consts::PI - angle).rem_euclid(std::f64::consts::TAU)
}

/// A clustered-dot matrix of `cols × rows` grown around `centres` (periodic): cells ranked by
/// `distance` to their nearest centre, then clockwise around it; each cell's value is its rank
/// divided by the number of centres (so each value occurs once per dot).
fn grown(
    cols: usize,
    rows: usize,
    centres: &[(f64, f64)],
    distance: fn(f64, f64) -> f64,
    angle: fn(f64, f64) -> f64,
) -> Matrix {
    let wrap = |d: f64, n: usize| {
        let n = n as f64;
        let d = d.rem_euclid(n);
        if d > n / 2.0 { d - n } else { d }
    };
    let mut keyed: Vec<((f64, f64, usize), usize)> = (0..rows * cols)
        .map(|i| {
            let (x, y) = ((i % cols) as f64, (i / cols) as f64);
            let (dist, dx, dy, c) = centres
                .iter()
                .enumerate()
                .map(|(c, &(cx, cy))| {
                    let (dx, dy) = (wrap(x - cx, cols), wrap(y - cy, rows));
                    (distance(dx, dy), dx, dy, c)
                })
                .fold((f64::INFINITY, 0.0, 0.0, 0), |best, cand| {
                    if cand.0 < best.0 - 1e-9 { cand } else { best }
                });
            ((dist, angle(dx, dy), c), i)
        })
        .collect();
    keyed.sort_by(|a, b| {
        a.0.0
            .total_cmp(&b.0.0)
            .then(a.0.1.total_cmp(&b.0.1))
            .then(a.0.2.cmp(&b.0.2))
            .then(a.1.cmp(&b.1))
    });
    let n = centres.len();
    let mut cells = vec![0u16; rows * cols];
    for (rank, (_, i)) in keyed.into_iter().enumerate() {
        cells[i] = u16::try_from(rank / n).unwrap_or(u16::MAX);
    }
    Matrix {
        cols,
        rows,
        cells,
        max: u16::try_from((rows * cols).div_ceil(n)).unwrap_or(u16::MAX),
    }
}

fn euclid(dx: f64, dy: f64) -> f64 {
    dx * dx + dy * dy
}

fn chebyshev_then_euclid(dx: f64, dy: f64) -> f64 {
    dx.abs().max(dy.abs()) * 1000.0 + euclid(dx, dy)
}

fn counter_clockwise(dx: f64, dy: f64) -> f64 {
    -clockwise_from_left(dx, dy)
}

/// A square spiral walked outward from the centre of an odd `n × n` block (right, down, left,
/// up, with growing legs).
fn spiral(n: usize) -> Matrix {
    let mut cells = vec![0u16; n * n];
    let c = (n / 2) as i64;
    let (mut x, mut y) = (c, c);
    let mut rank = 0u16;
    let mut leg = 1;
    let dirs = [(1, 0), (0, 1), (-1, 0), (0, -1)];
    let mut d = 0;
    let n_i = n as i64;
    let mut put = |x: i64, y: i64, rank: &mut u16| {
        if (0..n_i).contains(&x) && (0..n_i).contains(&y) {
            cells[(y * n_i + x) as usize] = *rank;
            *rank += 1;
        }
    };
    put(x, y, &mut rank);
    while usize::from(rank) < n * n {
        for _ in 0..2 {
            let (dx, dy) = dirs[d % 4];
            for _ in 0..leg {
                x += dx;
                y += dy;
                put(x, y, &mut rank);
            }
            d += 1;
        }
        leg += 1;
    }
    Matrix {
        cols: n,
        rows: n,
        cells,
        max: u16::try_from(n * n).unwrap_or(u16::MAX),
    }
}

/// The classic 4×4 clustered dot (caca.zoy.org's halftoning study, part 2; the same matrix
/// appears in Ulichney's "Digital Halftoning").
const CLUSTERED_DOT_4X4: [&[u16]; 4] = [
    &[12, 5, 6, 13],
    &[4, 0, 1, 7],
    &[11, 3, 2, 8],
    &[15, 10, 9, 14],
];

/// The classic 8×8 clustered dot at 45° ("mimics the halftoning techniques used by
/// newspapers", caca.zoy.org's halftoning study, part 2): two black dots growing from the
/// centres of the top-left and bottom-right quadrants, two white dots in the others.
const DIAGONAL_8X8: [&[u16]; 8] = [
    &[24, 10, 12, 26, 35, 47, 49, 37],
    &[8, 0, 2, 14, 45, 59, 61, 51],
    &[22, 6, 4, 16, 43, 57, 63, 53],
    &[30, 20, 18, 28, 33, 41, 55, 39],
    &[34, 46, 48, 36, 25, 11, 13, 27],
    &[44, 58, 60, 50, 9, 1, 3, 15],
    &[42, 56, 62, 52, 23, 7, 5, 17],
    &[32, 40, 54, 38, 31, 21, 19, 29],
];

/// Vertical line clusters (caca.zoy.org's halftoning study, part 2: "artistic vertical line
/// artifacts"): each column fills top to bottom, from the middle column outwards.
const VERTICAL_5X3: [&[u16]; 3] = [&[9, 3, 0, 6, 12], &[10, 4, 1, 7, 13], &[11, 5, 2, 8, 14]];

impl Threshold {
    /// The matrix. The published tables of Ulichney's "Digital Halftoning" (figures 5.4, 5.9,
    /// 5.13), Lau & Arce's "Modern Digital Halftoning" (figure 1.5) and the web page the
    /// library cites for the `6x6_2`, `6x6_3` and `8x8_3` variants could not be consulted
    /// offline: those matrices are constructed here with the same size, number of grey levels
    /// and dot shape (recorded in `expected_diffs.toml`).
    fn matrix(self) -> &'static Matrix {
        static MATRICES: LazyLock<Vec<Matrix>> = LazyLock::new(|| {
            let centre = |n: usize| (n as f64 - 1.0) / 2.0;
            let single =
                |n: usize, distance, angle| grown(n, n, &[(centre(n), centre(n))], distance, angle);
            let diagonal = |m: usize| {
                let c = centre(m);
                grown(
                    2 * m,
                    2 * m,
                    &[(c, c), (c + m as f64, c + m as f64)],
                    euclid,
                    clockwise_from_left,
                )
            };
            let classic_8x8 = Matrix::from_rows(&DIAGONAL_8X8, 64);
            let horizontal_line = grown(
                6,
                6,
                &[(centre(6), centre(6))],
                |dx, dy| dy.abs() * 1000.0 + dx.abs(),
                clockwise_from_left,
            );
            vec![
                Matrix::from_rows(&CLUSTERED_DOT_4X4, 16),
                single(6, euclid, clockwise_from_left),
                single(6, euclid, counter_clockwise),
                single(6, chebyshev_then_euclid, clockwise_from_left),
                single(8, euclid, clockwise_from_left),
                diagonal(8),
                diagonal(3),
                classic_8x8.clone(),
                diagonal(4),
                Matrix {
                    cells: classic_8x8.cells.iter().map(|v| v / 2).collect(),
                    max: 32,
                    ..classic_8x8
                },
                horizontal_line.clone(),
                spiral(5),
                horizontal_line.transposed(),
                Matrix::from_rows(&VERTICAL_5X3, 15).transposed(),
                Matrix::from_rows(&VERTICAL_5X3, 15),
            ]
        });
        &MATRICES[self as usize]
    }
}

/// Ordered dithering of `img` onto `palette` with `matrix`.
fn ordered(img: &mut RgbaImage, palette: &Palette, matrix: &Matrix, strength: f32) {
    let scale = MAX * strength;
    let max = f32::from(matrix.max);
    for (x, y, p) in img.enumerate_pixels_mut() {
        if p.0[3] == 0 {
            continue;
        }
        let add = scale * (f32::from(matrix.at(x, y)) / max - 0.500_000_06);
        let v = linear(p.0).map(|c| (c + add).clamp(0.0, MAX).round_ties_even());
        let [r, g, b] = palette.srgb[palette.nearest(v)];
        *p = Rgba([r, g, b, p.0[3]]);
    }
}

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
mod tests {
    use super::*;

    #[test]
    fn kernels_are_normalised_as_published() {
        for method in DitherMethod::ALL.iter().filter(|m| m.is_error_diffusion()) {
            let Algorithm::Diffusion(k) = method.algorithm() else {
                continue;
            };
            let sum: u16 = k.taps.iter().map(|t| t.2).sum();
            let expected = if *method == DitherMethod::Atkinson {
                // Atkinson diffuses six eighths only.
                6.0
            } else {
                k.divisor
            };
            assert!((f32::from(sum) - expected).abs() < f32::EPSILON, "{method}");
            assert!(
                k.taps.iter().all(|&(dx, dy, _)| dy > 0 || dx > 0),
                "{method}: a tap on an already processed pixel"
            );
        }
    }

    #[test]
    fn matrices_have_their_documented_levels() {
        // (method, cols, rows, max = grey levels − 1)
        for (m, cols, rows, max) in [
            (DitherMethod::ClusteredDot4x4, 4, 4, 16),
            (DitherMethod::ClusteredDot6x6, 6, 6, 36),
            (DitherMethod::ClusteredDot6x6_2, 6, 6, 36),
            (DitherMethod::ClusteredDot6x6_3, 6, 6, 36),
            (DitherMethod::ClusteredDot8x8, 8, 8, 64),
            (DitherMethod::ClusteredDotDiagonal16x16, 16, 16, 128),
            (DitherMethod::ClusteredDotDiagonal6x6, 6, 6, 18),
            (DitherMethod::ClusteredDotDiagonal8x8, 8, 8, 64),
            (DitherMethod::ClusteredDotDiagonal8x8_2, 8, 8, 32),
            (DitherMethod::ClusteredDotDiagonal8x8_3, 8, 8, 32),
            (DitherMethod::ClusteredDotHorizontalLine, 6, 6, 36),
            (DitherMethod::ClusteredDotSpiral5x5, 5, 5, 25),
            (DitherMethod::ClusteredDotVerticalLine, 6, 6, 36),
            (DitherMethod::Horizontal3x5, 3, 5, 15),
            (DitherMethod::Vertical5x3, 5, 3, 15),
        ] {
            let mx = matrix_of(m).expect("ordered");
            assert_eq!((mx.cols, mx.rows, mx.max), (cols, rows, max), "{m}");
            // Every value below max occurs, equally often.
            let mut counts = vec![0usize; usize::from(mx.max)];
            for &v in &mx.cells {
                counts[usize::from(v)] += 1;
            }
            let per = mx.cells.len() / usize::from(mx.max);
            assert!(counts.iter().all(|&c| c == per), "{m}: {counts:?}");
        }
        // The spiral grows from the centre; the 4×4 dot too.
        let s = matrix_of(DitherMethod::ClusteredDotSpiral5x5).expect("spiral");
        assert_eq!(s.at(2, 2), 0);
        assert_eq!(s.at(3, 2), 1);
        let c = matrix_of(DitherMethod::ClusteredDot4x4).expect("4x4");
        assert_eq!((c.at(1, 1), c.at(2, 1), c.at(0, 0)), (0, 1, 12));
        // A constructed matrix grows the same way as the published 4×4 one.
        let mut grown4 = grown(4, 4, &[(1.5, 1.5)], euclid, clockwise_from_left);
        grown4.max = 16;
        assert_eq!(&grown4, c);
    }

    #[test]
    fn nearest_uses_linear_luminance() {
        let p = Palette::new(&[Color([0, 0, 0, 255]), Color([255, 255, 255, 255])]);
        // sRGB mid grey 128 is 21.6 % linear: nearer black.
        assert_eq!(p.nearest(linear([128, 128, 128, 255])), 0);
        assert_eq!(p.nearest(linear([200, 200, 200, 255])), 1);
        // Luminance weights: pure green is nearer white, pure blue nearer black (unweighted
        // distances would put both nearer black).
        assert_eq!(p.nearest(linear([0, 255, 0, 255])), 1);
        assert_eq!(p.nearest(linear([0, 0, 255, 255])), 0);
    }
}
