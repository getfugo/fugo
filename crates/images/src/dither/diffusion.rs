//! Error diffusion: the kernels (Floyd–Steinberg and the others) and how they spread the error.

use super::*;

/// An error-diffusion kernel: `(dx, dy, weight)` taps relative to the current pixel (`dy ≥ 0`,
/// `dx > 0` when `dy = 0`), weights over `divisor`.
pub(super) struct Kernel {
    pub(super) taps: &'static [(i32, i32, u16)],
    pub(super) divisor: f32,
}

/// Floyd & Steinberg, "An adaptive algorithm for spatial grey scale", SID 1976.
pub(super) const FLOYD_STEINBERG: Kernel = Kernel {
    taps: &[(1, 0, 7), (-1, 1, 3), (0, 1, 5), (1, 1, 1)],
    divisor: 16.0,
};

/// "False" Floyd–Steinberg: the three-tap simplification (right 3, below 3, below-right 2,
/// over 8), as in Tanner Helland's survey of dithering algorithms (2012).
pub(super) const FALSE_FLOYD_STEINBERG: Kernel = Kernel {
    taps: &[(1, 0, 3), (0, 1, 3), (1, 1, 2)],
    divisor: 8.0,
};

/// Jarvis, Judice & Ninke, "A survey of techniques for the display of continuous tone
/// pictures on bilevel displays", CGIP 1976.
pub(super) const JARVIS_JUDICE_NINKE: Kernel = Kernel {
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
pub(super) const STUCKI: Kernel = Kernel {
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
pub(super) const ATKINSON: Kernel = Kernel {
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
pub(super) const BURKES: Kernel = Kernel {
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
pub(super) const SIERRA: Kernel = Kernel {
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
pub(super) const TWO_ROW_SIERRA: Kernel = Kernel {
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
pub(super) const SIERRA_LITE: Kernel = Kernel {
    taps: &[(1, 0, 2), (-1, 1, 1), (0, 1, 1)],
    divisor: 4.0,
};

/// The simplest two-dimensional kernel: half the error to the right, half below.
pub(super) const SIMPLE_2D: Kernel = Kernel {
    taps: &[(1, 0, 1), (0, 1, 1)],
    divisor: 2.0,
};

/// Stand-in for Steven Pigeon's kernel ("Dithering", Harder, Better, Faster, Stronger,
/// 2013-12-31), which could not be consulted offline: a sparse kernel with his reach (two
/// pixels ahead, two rows down) and weights falling with distance. Recorded in
/// `expected_diffs.toml`; replace it with the published matrix when available.
pub(super) const STEVEN_PIGEON: Kernel = Kernel {
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
pub(super) fn diffuse(
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
