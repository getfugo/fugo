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
            let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
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
