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
