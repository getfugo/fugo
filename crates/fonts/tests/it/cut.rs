//! One font cut down, in each format.

use std::collections::BTreeSet;

use ssg_fonts::{Format, Outcome, cut, encode_woff, encode_woff2};

use crate::support::{
    cff_strings, glyph_count, glyph_id, has_table, mapped, mulish, mulish_cff, mulish_variable,
    outline, sfnt,
};

fn chars(s: &str) -> BTreeSet<char> {
    s.chars().collect()
}

fn cut_bytes(font: &[u8], used: &str) -> Vec<u8> {
    match cut(font, &chars(used)).expect("cut") {
        Outcome::Cut(bytes) => bytes,
        other => panic!("not cut: {other:?}"),
    }
}

#[test]
fn truetype_keeps_the_characters_and_their_layout() {
    let font = mulish();
    let out = cut_bytes(&font, "AVTo fi");
    assert_eq!(Format::sniff(&out), Some(Format::OpenType));
    assert!(out.len() < font.len() / 10, "{} bytes", out.len());
    assert_eq!(mapped(&out, "AVTofi xyz"), "AVTofi ");
    // Of 1,058: .notdef, the characters, and the glyphs layout features reach from them (the
    // `fi` ligature, alternates).
    assert!(glyph_count(&out) < 20, "{} glyphs", glyph_count(&out));
    assert!(has_table(&out, b"GPOS"), "kerning of A V and T o kept");
    assert!(has_table(&out, b"GSUB"));
}

#[test]
fn woff2_in_woff2_out() {
    let woff2 = encode_woff2(&mulish()).expect("woff2");
    let out = cut_bytes(&woff2, "Hi");
    assert_eq!(Format::sniff(&out), Some(Format::Woff2));
    assert!(out.len() < woff2.len() / 5, "{} bytes", out.len());
    assert_eq!(mapped(&out, "Hix"), "Hi");
}

#[test]
fn woff_in_woff_out() {
    let woff = encode_woff(&mulish()).expect("woff");
    assert_eq!(Format::sniff(&woff), Some(Format::Woff));
    assert_eq!(
        mapped(&woff, "Hix"),
        "Hix",
        "the whole font, written as WOFF"
    );
    let out = cut_bytes(&woff, "Hi");
    assert_eq!(Format::sniff(&out), Some(Format::Woff));
    assert!(out.len() < woff.len() / 5, "{} bytes", out.len());
    assert_eq!(mapped(&out, "Hix"), "Hi");
}

#[test]
fn a_font_the_site_does_not_use_is_left_alone() {
    let outcome = cut(&mulish(), &chars("\u{f005}\u{e61a}")).expect("cut");
    assert!(matches!(outcome, Outcome::Unused), "{outcome:?}");
}

#[test]
fn a_variable_font_that_would_lose_its_kerning_stays_whole() {
    let outcome = cut(&mulish_variable(), &chars("AVTo")).expect("cut");
    let Outcome::Whole(Some(reason)) = outcome else {
        panic!("{outcome:?}");
    };
    assert!(reason.contains("GPOS"), "{reason}");
}

#[test]
fn cff_in_cff_out() {
    // Font Awesome 7's fonts: CFF outlines, and no tables but those that draw them.
    let otf = mulish_cff(false);
    let fonts = [
        (Format::OpenType, otf.clone()),
        (Format::Woff, encode_woff(&otf).expect("woff")),
        (Format::Woff2, encode_woff2(&otf).expect("woff2")),
    ];
    for (format, font) in fonts {
        let out = cut_bytes(&font, "Hi");
        assert_eq!(Format::sniff(&out), Some(format));
        assert!(
            out.len() < font.len() / 3,
            "{format:?}: {} bytes",
            out.len()
        );
        assert_eq!(&sfnt(&out)[..4], b"OTTO", "{format:?}");
        assert_eq!(mapped(&out, "Hix"), "Hi", "{format:?}");
        assert_eq!(glyph_count(&out), 3, "{format:?}: .notdef, H and i");
        for c in ['H', 'i'] {
            assert_eq!(
                outline(&out, c),
                outline(&otf, c),
                "{format:?}: the glyph of {c}"
            );
        }
    }
}

#[test]
fn cff_keeps_the_names_of_its_glyphs() {
    let otf = mulish_cff(false);
    let out = cut_bytes(&otf, "H");
    let strings = cff_strings(&out);
    assert_eq!(
        strings.len(),
        cff_strings(&otf).len(),
        "each string keeps its id"
    );
    let kept: Vec<String> = strings
        .iter()
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();
    assert!(kept.len() == 1, "{} strings kept", kept.len());
    assert_eq!(kept[0], format!("g{}", glyph_id(&otf, 'H')));
}

#[test]
fn cff_with_layout_tables_stays_whole() {
    // The subsetter of CFF outlines would leave the layout tables out: no kerning, no ligatures.
    let otf = mulish_cff(true);
    let woff = encode_woff(&otf).expect("woff");
    for font in [otf, woff] {
        match cut(&font, &chars("AVTo")).expect("cut") {
            Outcome::Whole(Some(reason)) => {
                assert!(reason.contains("CFF outlines"), "{reason}");
                assert!(
                    reason.contains("GPOS") && reason.contains("GSUB"),
                    "{reason}"
                );
            }
            Outcome::Cut(bytes) => panic!("cut down to {} bytes", bytes.len()),
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn cff2_stays_whole() {
    // A font is known to have CFF2 outlines by the tag of their table, before it is read.
    let mut otf = mulish_cff(false);
    let at = otf.windows(4).position(|w| w == b"CFF ").expect("CFF");
    otf[at..at + 4].copy_from_slice(b"CFF2");
    let outcome = cut(&otf, &chars("Hi")).expect("cut");
    let Outcome::Whole(Some(reason)) = outcome else {
        panic!("{outcome:?}");
    };
    assert!(reason.contains("CFF2 outlines"), "{reason}");
}

#[test]
fn not_a_font() {
    let e = cut(b"<svg></svg>", &chars("a")).expect_err("not a font");
    assert!(e.to_string().contains("not a TrueType"), "{e}");
}
