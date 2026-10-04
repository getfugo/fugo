use super::*;

#[test]
fn rounding_helpers_match_go() {
    assert_eq!(f32u8(-0.7), 0);
    assert_eq!(f32u8(254.49), 254);
    assert_eq!(f32u8(254.5), 255);
    assert_eq!(f32u8(300.0), 255);
    assert_eq!(f32u16(65534.5), 65535);
}

#[test]
fn box_weights_average_neighbours() {
    let w = weights(2, 4, Kernel::of(Resample::Box));
    let got: Vec<Vec<(usize, f32)>> = w
        .iter()
        .map(|r| r.iter().map(|w| (w.index, w.weight)).collect())
        .collect();
    assert_eq!(
        got,
        vec![vec![(0, 0.5), (1, 0.5)], vec![(2, 0.5), (3, 0.5)]]
    );
}

#[test]
fn nrgba_premultiplies_like_draw_src() {
    let mut d = Dst::new(DstType::Nrgba, 1, 1);
    d.pix.copy_from_slice(&[200, 100, 50, 128]);
    // Go: sa = 128·0x101; r = 200·sa/0xff >> 8.
    assert_eq!(d.into_rgba(), vec![100, 50, 25, 128]);
}
