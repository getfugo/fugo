//! The `smart` anchor: content-aware cropping, a port of muesli/smartcrop v0.3.0
//! (`smartcrop.go`, itself after Jonas Wagner's smartcrop.js) with the Go implementation's
//! adapter (`resources/images/smartcrop.go` at 44529028) and the filters it builds around it
//! (`FiltersFromConfig`, `resources/images/image.go`).
//!
//! The Go implementation finds the region on the operation's *source* (the decoded input, before
//! rotation and before the other filters of a chain): smartcrop downscales it so that its shorter
//! side is 400 pixels (gift's resize with the spec's filter, [`crate::gift`]), scores every
//! candidate region with the target's aspect ratio (full size and 90 %, every 8 pixels) by edge,
//! skin and saturation detail weighted towards the centre and the thirds, and scales the best one
//! back. `fill` then crops that region and resizes it to the target size; `crop` crops it and keeps
//! its centre at the target size (so a crop is the centre of the best region, not the region
//! itself). The arithmetic is float64 in Go's order, without fused multiply-adds (Go's amd64 code),
//! so the chosen region is Go's.

use crate::gift::{self, Source};
use crate::plan::Size;
use crate::spec::Resample;

/// A rectangle with Go's `image.Rectangle` semantics (`Min` inclusive, `Max` exclusive).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Rect {
    pub x0: i64,
    pub y0: i64,
    pub x1: i64,
    pub y1: i64,
}

impl Rect {
    /// `image.Rect`: canonical (min ≤ max).
    pub(crate) fn new(x0: i64, y0: i64, x1: i64, y1: i64) -> Self {
        Self {
            x0: x0.min(x1),
            y0: y0.min(y1),
            x1: x0.max(x1),
            y1: y0.max(y1),
        }
    }

    /// The rectangle of an image of `size` at the origin.
    pub(crate) fn of_size((w, h): Size) -> Self {
        Self::new(0, 0, i64::from(w), i64::from(h))
    }

    pub(crate) fn dx(self) -> i64 {
        self.x1 - self.x0
    }

    pub(crate) fn dy(self) -> i64 {
        self.y1 - self.y0
    }

    fn is_empty(self) -> bool {
        self.x0 >= self.x1 || self.y0 >= self.y1
    }

    /// `Rectangle.Intersect`: the empty rectangle (`ZR`) when they do not overlap.
    pub(crate) fn intersect(self, o: Self) -> Self {
        let r = Self {
            x0: self.x0.max(o.x0),
            y0: self.y0.max(o.y0),
            x1: self.x1.min(o.x1),
            y1: self.y1.min(o.y1),
        };
        if r.is_empty() { Self::default() } else { r }
    }

    /// The size, zero when empty.
    pub(crate) fn size(self) -> Size {
        let side = |v: i64| u32::try_from(v.max(0)).unwrap_or(u32::MAX);
        (side(self.dx()), side(self.dy()))
    }
}

// smartcrop.go's constants.
const DETAIL_WEIGHT: f64 = 0.2;
const SKIN_BIAS: f64 = 0.01;
const SKIN_BRIGHTNESS_MIN: f64 = 0.2;
const SKIN_BRIGHTNESS_MAX: f64 = 1.0;
const SKIN_THRESHOLD: f64 = 0.8;
const SKIN_WEIGHT: f64 = 1.8;
const SATURATION_BRIGHTNESS_MIN: f64 = 0.05;
const SATURATION_BRIGHTNESS_MAX: f64 = 0.9;
const SATURATION_THRESHOLD: f64 = 0.4;
const SATURATION_BIAS: f64 = 0.2;
const SATURATION_WEIGHT: f64 = 0.3;
const SCORE_DOWN_SAMPLE: usize = 8;
const STEP: i64 = 8;
const SCALE_STEP: f64 = 0.1;
const MIN_SCALE: f64 = 0.9;
const MAX_SCALE: f64 = 1.0;
const EDGE_RADIUS: f64 = 0.4;
const EDGE_WEIGHT: f64 = -20.0;
const OUTSIDE_IMPORTANCE: f64 = -0.5;
const PRESCALE_MIN: f64 = 400.0;
const SKIN_COLOR: [f64; 3] = [0.78, 0.57, 0.44];
/// `255.0 / (1.0 - skinThreshold)` and `255.0 / (1.0 - saturationThreshold)`: Go folds the
/// constant expressions exactly (1275 and 425), unlike float64 arithmetic.
const SKIN_SCALE: f64 = 1275.0;
const SATURATION_SCALE: f64 = 425.0;

/// `chop`: truncation toward zero.
fn chop(x: f64) -> f64 {
    if x < 0.0 { x.ceil() } else { x.floor() }
}

/// How smartcrop sees a source: its size, the prescale factor and the size of the prescaled copy it
/// analyses (`FindBestCrop` with the Go implementation's resizer, `imagingResizer.Resize` and
/// `calcFactorsNfnt`: the prescaled width truncates, the height is rounded up).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Prescale {
    src: (usize, usize),
    low: (usize, usize),
    factor: f64,
}

impl Prescale {
    fn new((sw, sh): (usize, usize)) -> Self {
        let (swf, shf) = (sw as f64, sh as f64);
        let f = PRESCALE_MIN / swf.min(shf);
        let factor = if f < 1.0 { f } else { 1.0 };
        // `uint(float64(Dx) * prescalefactor)`, then `ceil(Dy / (Dx / width))`.
        let low_w = (swf * factor) as usize;
        let scale_x = swf / low_w as f64;
        let low_h = (shf / scale_x).ceil() as usize;
        Self {
            src: (sw, sh),
            low: (low_w, low_h),
            factor,
        }
    }

    /// The end of `FindBestCrop`: a region of the prescaled image in source pixels
    /// (`Canon` after scaling each coordinate back), intersected with the source (as in Go).
    fn scale_back(&self, r: Rect) -> Rect {
        let back = |v: i64| chop(v as f64 / self.factor) as i64;
        Rect::new(back(r.x0), back(r.y0), back(r.x1), back(r.y1)).intersect(Rect::new(
            0,
            0,
            self.src.0 as i64,
            self.src.1 as i64,
        ))
    }
}

/// What smartcrop tries for a target: the crop size in the prescaled image and the smallest
/// crop scale (`FindBestCrop` up to `analyse`).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Setup {
    low: (usize, usize),
    crop_w: f64,
    crop_h: f64,
    real_min_scale: f64,
}

impl Setup {
    fn new(p: &Prescale, width: u32, height: u32) -> Self {
        let (swf, shf) = (p.src.0 as f64, p.src.1 as f64);
        let scale = (swf / f64::from(width)).min(shf / f64::from(height));
        Self {
            low: p.low,
            crop_w: chop(f64::from(width) * scale * p.factor),
            crop_h: chop(f64::from(height) * scale * p.factor),
            real_min_scale: MAX_SCALE.min((1.0 / scale).max(MIN_SCALE)),
        }
    }

    /// `crops`: every candidate region of the prescaled image, largest scale first.
    fn crops(&self) -> Vec<Rect> {
        let (width, height) = self.low;
        let min_dimension = (width as f64).min(height as f64);
        let crop_w = if self.crop_w != 0.0 {
            self.crop_w
        } else {
            min_dimension
        };
        let crop_h = if self.crop_h != 0.0 {
            self.crop_h
        } else {
            min_dimension
        };
        let mut res = Vec::new();
        let mut scale = MAX_SCALE;
        while scale >= self.real_min_scale {
            let mut y = 0i64;
            while y as f64 + crop_h * scale <= height as f64 {
                let mut x = 0i64;
                while x as f64 + crop_w * scale <= width as f64 {
                    res.push(Rect::new(
                        x,
                        y,
                        x + (crop_w * scale) as i64,
                        y + (crop_h * scale) as i64,
                    ));
                    x += STEP;
                }
                y += STEP;
            }
            scale -= SCALE_STEP;
        }
        res
    }
}

/// The Go implementation's early answers (`smartCrop`): the empty rectangle for an empty target or
/// source, the whole source when it has the target size.
fn trivial((sw, sh): (usize, usize), width: u32, height: u32) -> Option<Rect> {
    if width == 0 || height == 0 || sw == 0 || sh == 0 {
        return Some(Rect::default());
    }
    ((sw, sh) == (width as usize, height as usize)).then(|| Rect::new(0, 0, sw as i64, sh as i64))
}

/// The Go implementation's `smartCrop`: the region of `src` the `smart` anchor keeps for a
/// `width`×`height` target with the spec's resample `filter`. Empty when a side is zero (or nothing
/// scores above −1, which smartcrop leaves as the empty rectangle); the whole image when it already
/// has the target size.
pub(crate) fn find(src: &Source<'_>, width: u32, height: u32, filter: Resample) -> Rect {
    match trivial(src.size(), width, height) {
        Some(r) => r,
        None => Analysed::new(src, filter).region(width, height),
    }
}

/// The regions [`find`] can return for a source of `src` pixels (its candidates scaled
/// back; the empty rectangle when there is none), for callers that need the size of the
/// result before the pixels are analysed. A source whose every candidate scores −1 or less
/// also gives the empty rectangle; that needs a target whose aspect ratio is dozens of
/// times the source's, and is not listed.
pub(crate) fn candidates(src: Size, width: u32, height: u32) -> Vec<Rect> {
    let src = (src.0 as usize, src.1 as usize);
    if let Some(r) = trivial(src, width, height) {
        return vec![r];
    }
    let p = Prescale::new(src);
    let all: Vec<Rect> = Setup::new(&p, width, height)
        .crops()
        .into_iter()
        .map(|r| p.scale_back(r))
        .collect();
    if all.is_empty() {
        vec![Rect::default()]
    } else {
        all
    }
}

/// The channels of the RGBA pixel at `i` (alpha-premultiplied, as smartcrop reads them).
fn rgb(pix: &[u8], i: usize) -> [u8; 3] {
    [pix[i * 4], pix[i * 4 + 1], pix[i * 4 + 2]]
}

/// `cie`.
fn cie([r, g, b]: [u8; 3]) -> f64 {
    0.5126 * f64::from(b) + 0.7152 * f64::from(g) + 0.0722 * f64::from(r)
}

/// `skinCol`.
fn skin_col([r, g, b]: [u8; 3]) -> f64 {
    let (r8, g8, b8) = (f64::from(r), f64::from(g), f64::from(b));
    let mag = (r8 * r8 + g8 * g8 + b8 * b8).sqrt();
    let rd = r8 / mag - SKIN_COLOR[0];
    let gd = g8 / mag - SKIN_COLOR[1];
    let bd = b8 / mag - SKIN_COLOR[2];
    let d = (rd * rd + gd * gd + bd * bd).sqrt();
    1.0 - d
}

/// `saturation`.
fn saturation([r, g, b]: [u8; 3]) -> f64 {
    let c_max = r.max(g).max(b);
    let c_min = r.min(g).min(b);
    if c_max == c_min {
        return 0.0;
    }
    let maximum = f64::from(c_max) / 255.0;
    let minimum = f64::from(c_min) / 255.0;
    let l = (maximum + minimum) / 2.0;
    let d = maximum - minimum;
    if l > 0.5 {
        d / (2.0 - maximum - minimum)
    } else {
        d / (maximum + minimum)
    }
}

/// `bounds` then `uint8(...)` (truncation).
fn to_u8(l: f64) -> u8 {
    l.clamp(0.0, 255.0) as u8
}

/// `thirds`.
fn thirds(x: f64) -> f64 {
    let x = (((x - (1.0 / 3.0) + 1.0) % 2.0) * 0.5 - 0.5) * 16.0;
    (1.0 - x * x).max(0.0)
}

/// The parts of `importance` that depend on one coordinate's offset into a crop of `len`
/// pixels: `p` squared, the squared edge distance, and `thirds(p)`.
#[derive(Clone, Copy)]
struct Axis {
    p2: f64,
    d2: f64,
    thirds: f64,
}

impl Axis {
    fn new(offset: i64, len: i64) -> Self {
        let f = offset as f64 / len as f64;
        let p = (0.5 - f).abs() * 2.0;
        let d = (p - 1.0 + EDGE_RADIUS).max(0.0);
        Self {
            p2: p * p,
            d2: d * d,
            thirds: thirds(p),
        }
    }
}

/// The [`Axis`] values of the sample offsets (multiples of 8) into a crop of `size`.
#[derive(Default)]
struct Axes {
    size: (i64, i64),
    xs: Vec<Axis>,
    ys: Vec<Axis>,
}

impl Axes {
    fn new(cw: i64, ch: i64) -> Self {
        let table = |len: i64| {
            (0..len)
                .step_by(STEP as usize)
                .map(|offset| Axis::new(offset, len))
                .collect()
        };
        Self {
            size: (cw, ch),
            xs: table(cw),
            ys: table(ch),
        }
    }
}

/// `importance` inside the crop, from its two axes.
fn importance(x: Axis, y: Axis) -> f64 {
    let d = (x.d2 + y.d2) * EDGE_WEIGHT;
    let mut s = 1.41 - (x.p2 + y.p2).sqrt();
    s += (0.0f64.max(s + d + 0.5) * 1.2) * (x.thirds + y.thirds);
    s + d
}

/// The values `score` reads at one sample point.
struct Sample {
    x: i64,
    y: i64,
    r: f64,
    det: f64,
    b: f64,
}

/// A source prepared for `analyse`: the prescaled copy's detail image (`edgeDetect`,
/// `skinDetect`, `saturationDetect`) at the points `score` reads, one pixel in 64. It
/// depends on the source and the filter only, so one serves every target.
pub(crate) struct Analysed {
    prescale: Prescale,
    samples: Vec<Sample>,
}

impl Analysed {
    /// Prescales `src` with `filter` (gift's resize) and computes its detail image.
    pub(crate) fn new(src: &Source<'_>, filter: Resample) -> Self {
        let prescale = Prescale::new(src.size());
        let (width, height) = prescale.low;
        let img = gift::resize_to_rgba(src, width, height, filter);
        let n = width * height;
        let cies: Vec<f64> = (0..n).map(|i| cie(rgb(&img, i))).collect();
        let mut out = vec![[0u8; 3]; n];
        // `edgeDetect`: the green channel.
        for y in 0..height {
            for x in 0..width {
                let i = y * width + x;
                let lightness = if x == 0 || x >= width - 1 || y == 0 || y >= height - 1 {
                    0.0
                } else {
                    cies[i] * 4.0 - cies[i - width] - cies[i - 1] - cies[i + 1] - cies[i + width]
                };
                out[i][1] = to_u8(lightness);
            }
        }
        // `skinDetect` (red) and `saturationDetect` (blue).
        for (i, o) in out.iter_mut().enumerate() {
            let c = rgb(&img, i);
            let lightness = cies[i] / 255.0;
            let skin = skin_col(c);
            o[0] = if skin > SKIN_THRESHOLD
                && (SKIN_BRIGHTNESS_MIN..=SKIN_BRIGHTNESS_MAX).contains(&lightness)
            {
                to_u8((skin - SKIN_THRESHOLD) * SKIN_SCALE)
            } else {
                0
            };
            let sat = saturation(c);
            o[2] = if sat > SATURATION_THRESHOLD
                && (SATURATION_BRIGHTNESS_MIN..=SATURATION_BRIGHTNESS_MAX).contains(&lightness)
            {
                to_u8((sat - SATURATION_THRESHOLD) * SATURATION_SCALE)
            } else {
                0
            };
        }
        // `score`'s sample points, row by row.
        let mut samples = Vec::new();
        let mut y = 0;
        while height >= SCORE_DOWN_SAMPLE && y <= height - SCORE_DOWN_SAMPLE {
            let mut x = 0;
            while width >= SCORE_DOWN_SAMPLE && x <= width - SCORE_DOWN_SAMPLE {
                let [r8, g8, b8] = out[y * width + x].map(f64::from);
                samples.push(Sample {
                    x: x as i64,
                    y: y as i64,
                    r: r8 / 255.0,
                    det: g8 / 255.0,
                    b: b8 / 255.0,
                });
                x += SCORE_DOWN_SAMPLE;
            }
            y += SCORE_DOWN_SAMPLE;
        }
        Self { prescale, samples }
    }

    /// [`find`] for a `width`×`height` target: `analyse`'s best candidate (the highest
    /// `totalScore`, the first of equals; the empty rectangle when none scores above −1),
    /// in source pixels.
    pub(crate) fn region(&self, width: u32, height: u32) -> Rect {
        if let Some(r) = trivial(self.prescale.src, width, height) {
            return r;
        }
        let mut top = Rect::default();
        let mut top_score = -1.0;
        // The axis tables of the current crop size (every crop of a scale has the same size).
        let mut axes = Axes::default();
        for crop in Setup::new(&self.prescale, width, height).crops() {
            let (cw, ch) = (crop.dx(), crop.dy());
            if axes.size != (cw, ch) {
                axes = Axes::new(cw, ch);
            }
            let (xs, ys) = (&axes.xs, &axes.ys);
            // `score`.
            let (mut skin, mut detail, mut sat) = (0.0f64, 0.0f64, 0.0f64);
            for s in &self.samples {
                let inside = crop.x0 <= s.x && s.x < crop.x1 && crop.y0 <= s.y && s.y < crop.y1;
                let imp = if inside {
                    // Crops start on multiples of 8, like the sample points.
                    let (ox, oy) = (s.x - crop.x0, s.y - crop.y0);
                    importance(xs[(ox / STEP) as usize], ys[(oy / STEP) as usize])
                } else {
                    OUTSIDE_IMPORTANCE
                };
                skin += s.r * (s.det + SKIN_BIAS) * imp;
                detail += s.det * imp;
                sat += s.b * (s.det + SATURATION_BIAS) * imp;
            }
            // `totalScore`.
            let total = (detail * DETAIL_WEIGHT + skin * SKIN_WEIGHT + sat * SATURATION_WEIGHT)
                / cw as f64
                / ch as f64;
            if total > top_score {
                top = crop;
                top_score = total;
            }
        }
        self.prescale.scale_back(top)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde::Deserialize;
    use ssg_testkit::fixture::{oracle, repo_file};

    use super::*;
    use crate::codec;

    #[derive(Deserialize)]
    struct Regions {
        cases: Vec<RegionCase>,
    }

    #[derive(Deserialize)]
    struct RegionCase {
        src: String,
        w: u32,
        h: u32,
        filter: Resample,
        rect: [i64; 4],
    }

    /// The regions Go's smart crop picks (`testdata/oracle/images/smartcrop/regions.json.gz`)
    /// for the legacy docs site's images, the Go implementation's and Go's test images (JPEG of
    /// every subsampling, progressive, restart intervals, RGB, CMYK and grey; PNG of every
    /// colour type and depth; GIF), at 19 targets each with the default box filter, and at
    /// four targets with each of the 15 filters on four sources: all equal.
    #[test]
    fn regions_equal_go_s() {
        let fx: Regions = oracle("oracle/images/smartcrop/regions.json.gz");
        assert_eq!(fx.cases.len(), 1576);
        let mut sources: BTreeMap<&str, codec::Decoded> = BTreeMap::new();
        let mut analysed: BTreeMap<(&str, Resample), Analysed> = BTreeMap::new();
        let mut failures = Vec::new();
        for c in &fx.cases {
            let src = sources.entry(&c.src).or_insert_with(|| {
                let path = repo_file(&c.src);
                let bytes =
                    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                codec::decode(&bytes, &c.src, true).expect("the oracle decoded it")
            });
            // The prescaled analysis serves every target of a source and filter.
            let r = match trivial(src.source().size(), c.w, c.h) {
                Some(r) => r,
                None => analysed
                    .entry((&c.src, c.filter))
                    .or_insert_with(|| Analysed::new(&src.source(), c.filter))
                    .region(c.w, c.h),
            };
            if [r.x0, r.y0, r.x1, r.y1] != c.rect {
                failures.push(format!(
                    "{} {}x{} {}: Go {:?}, here {r:?}",
                    c.src, c.w, c.h, c.filter, c.rect
                ));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// Every region [`find`] returned above is one of its [`candidates`], which planning uses
    /// for the size of a result.
    #[test]
    fn candidates_hold_the_regions() {
        let fx: Regions = oracle("oracle/images/smartcrop/regions.json.gz");
        for c in fx.cases.iter().filter(|c| c.filter == Resample::Box) {
            let path = repo_file(&c.src);
            let size = crate::probe_file(&path).expect("probe").0;
            let [x0, y0, x1, y1] = c.rect;
            assert!(
                candidates(size, c.w, c.h).contains(&Rect::new(x0, y0, x1, y1)),
                "{} {}x{}: {:?}",
                c.src,
                c.w,
                c.h,
                c.rect
            );
        }
    }

    #[test]
    fn rect_intersect_is_go_s() {
        let a = Rect::new(0, 0, 10, 10);
        assert_eq!(
            a.intersect(Rect::new(5, 5, 20, 20)),
            Rect::new(5, 5, 10, 10)
        );
        assert_eq!(a.intersect(Rect::new(10, 0, 20, 10)), Rect::default());
        assert_eq!(Rect::new(5, 6, 1, 2), Rect::new(1, 2, 5, 6));
    }

    #[test]
    fn prescale_follows_the_go_resizer() {
        // The legacy docs site's sunset: 900×562 → 640×400 (`uint(900·400/562)`,
        // `ceil(562/1.40625)`).
        let p = Prescale::new((900, 562));
        assert_eq!(p.low, (640, 400));
        let s = Setup::new(&p, 200, 200);
        assert!((s.real_min_scale - 0.9).abs() < f64::EPSILON);
        assert!((s.crop_w - 400.0).abs() < f64::EPSILON);
        // A small source is analysed at its own size.
        let p = Prescale::new((150, 103));
        assert_eq!(p.low, (150, 103));
        assert!((p.factor - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn thirds_peaks_at_a_third() {
        assert!((thirds(1.0 / 3.0) - 1.0).abs() < 1e-12);
        assert!(thirds(0.0).abs() < 1e-12);
    }

    #[test]
    fn crops_step_by_eight_at_two_scales() {
        let s = Setup {
            low: (32, 16),
            crop_w: 16.0,
            crop_h: 16.0,
            real_min_scale: 0.9,
        };
        let c = s.crops();
        // Scale 1: x 0, 8, 16; scale 0.9 (14×14): x 0, 8, 16.
        assert_eq!(c.len(), 6);
        assert_eq!(c[3], Rect::new(0, 0, 14, 14));
    }
}
