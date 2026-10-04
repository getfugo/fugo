use super::*;

#[test]
fn go_regular_metrics_are_go_s() {
    let font = FontData::go_regular();
    let face = Face::new(font.bytes(), 20.0, "Go Regular").expect("face");
    // hhea ascender 1935 of 2048 units at 20 px: 1935·1280/2048 = 1209.4 → 1209 (26.6).
    assert_eq!(face.ascent(), 1209);
    assert_eq!(ceil_26_6(face.ascent()), 19);
    // Go Regular has neither GPOS nor kern: no kerning.
    assert!(matches!(face.kerning, Kerning::None));
    // 'H' advances 1479 units → 1479·1280/2048 = 924.4 → 924.
    let h = face.glyph('H').expect("H");
    assert_eq!(face.advance(h), scale(1479 * 1280, 2048));
    // A glyph the font lacks is skipped.
    assert_eq!(face.measure("H\u{10FFFD}H"), face.measure("HH"));
}

#[test]
fn gpos_kerning_of_a_real_font() {
    // The docs' opengraph font (Mulish Black): GPOS pair adjustments, no kern table.
    let path = ssg_testkit::fixture::legacy_docs().join("assets/opengraph/mulish-black.ttf");
    let bytes = std::fs::read(&path).expect("mulish-black.ttf");
    let face = Face::new(&bytes, 70.0, "mulish").expect("face");
    let Kerning::Gpos(tables) = &face.kerning else {
        panic!("no GPOS kerning found");
    };
    assert!(!tables.is_empty());
    let kern = |p: &str| {
        let mut c = p.chars();
        face.kern(c.next().expect("a"), c.next().expect("b"))
    };
    let kerned: Vec<(&str, i64)> = ["AV", "To", "Ty", "LT", "Yo", "Wa", "oo"]
        .into_iter()
        .map(|p| (p, kern(p)))
        .collect();
    assert!(
        kerned.iter().filter(|(_, k)| *k < 0).count() >= 4,
        "{kerned:?}"
    );
    // x/image's quirk: the value in font units is the 26.6 kerning, whatever the size.
    let small = Face::new(&bytes, 10.0, "mulish").expect("face");
    assert_eq!(small.kern('A', 'V'), kern("AV"));
    // The measured advance includes it.
    let (a, v) = (face.glyph('A').expect("A"), face.glyph('V').expect("V"));
    assert_eq!(
        face.measure("AV"),
        face.advance(a) + face.advance(v) + kern("AV")
    );
}

#[test]
fn scaling_rounds_half_away_from_zero() {
    assert_eq!(scale(3 * 64, 2), 96);
    assert_eq!(scale(5, 2), 3);
    assert_eq!(scale(-5, 2), -3);
    assert_eq!(ceil_26_6(64), 1);
    assert_eq!(ceil_26_6(65), 2);
    assert_eq!(ceil_26_6(-65), -1);
}
