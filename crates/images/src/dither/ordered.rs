//! Ordered dithering: the threshold matrices and how they are built.

use super::*;

/// The threshold matrices of the ordered methods.
#[derive(Clone, Copy, Debug)]
pub(super) enum Threshold {
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
    pub(super) fn from_rows(rows: &[&[u16]], max: u16) -> Self {
        Self {
            cols: rows[0].len(),
            rows: rows.len(),
            cells: rows.iter().flat_map(|r| r.iter().copied()).collect(),
            max,
        }
    }

    pub(super) fn transposed(&self) -> Self {
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

    pub(super) fn at(&self, x: u32, y: u32) -> u16 {
        self.cells[(y as usize % self.rows) * self.cols + x as usize % self.cols]
    }
}

/// The clockwise angle of `(dx, dy)` (image coordinates, y down) from the direction just
/// above "left", in `0..2π`: the order in which the classic 4×4 clustered dot grows around
/// its centre.
pub(super) fn clockwise_from_left(dx: f64, dy: f64) -> f64 {
    let angle = (-dy).atan2(dx);
    (std::f64::consts::PI - angle).rem_euclid(std::f64::consts::TAU)
}

/// A clustered-dot matrix of `cols × rows` grown around `centres` (periodic): cells ranked by
/// `distance` to their nearest centre, then clockwise around it; each cell's value is its rank
/// divided by the number of centres (so each value occurs once per dot).
pub(super) fn grown(
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

pub(super) fn euclid(dx: f64, dy: f64) -> f64 {
    dx * dx + dy * dy
}

pub(super) fn chebyshev_then_euclid(dx: f64, dy: f64) -> f64 {
    dx.abs().max(dy.abs()) * 1000.0 + euclid(dx, dy)
}

pub(super) fn counter_clockwise(dx: f64, dy: f64) -> f64 {
    -clockwise_from_left(dx, dy)
}

/// A square spiral walked outward from the centre of an odd `n × n` block (right, down, left,
/// up, with growing legs).
pub(super) fn spiral(n: usize) -> Matrix {
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
pub(super) const CLUSTERED_DOT_4X4: [&[u16]; 4] = [
    &[12, 5, 6, 13],
    &[4, 0, 1, 7],
    &[11, 3, 2, 8],
    &[15, 10, 9, 14],
];

/// The classic 8×8 clustered dot at 45° ("mimics the halftoning techniques used by
/// newspapers", caca.zoy.org's halftoning study, part 2): two black dots growing from the
/// centres of the top-left and bottom-right quadrants, two white dots in the others.
pub(super) const DIAGONAL_8X8: [&[u16]; 8] = [
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
pub(super) const VERTICAL_5X3: [&[u16]; 3] =
    [&[9, 3, 0, 6, 12], &[10, 4, 1, 7, 13], &[11, 5, 2, 8, 14]];

impl Threshold {
    /// The matrix. The published tables of Ulichney's "Digital Halftoning" (figures 5.4, 5.9,
    /// 5.13), Lau & Arce's "Modern Digital Halftoning" (figure 1.5) and the web page the
    /// library cites for the `6x6_2`, `6x6_3` and `8x8_3` variants could not be consulted
    /// offline: those matrices are constructed here with the same size, number of grey levels
    /// and dot shape (recorded in `expected_diffs.toml`).
    pub(super) fn matrix(self) -> &'static Matrix {
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
pub(super) fn ordered(img: &mut RgbaImage, palette: &Palette, matrix: &Matrix, strength: f32) {
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
