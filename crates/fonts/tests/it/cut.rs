//! One font cut down, in each format.

use std::collections::BTreeSet;

use ssg_fonts::{Format, Outcome, cut, encode_woff};

use crate::support::{glyph_count, has_table, mapped, mulish, mulish_variable};

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
    let woff2 = ttf2woff2::encode(&mulish(), ttf2woff2::BrotliQuality::default()).expect("woff2");
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
fn not_a_font() {
    let e = cut(b"<svg></svg>", &chars("a")).expect_err("not a font");
    assert!(e.to_string().contains("not a TrueType"), "{e}");
}
