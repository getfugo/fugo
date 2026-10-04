use super::*;

#[test]
fn rgb_to_ycbcr_matches_go() {
    // Values of Go's color.RGBToYCbCr (its documented examples and clamping edges).
    assert_eq!(rgb_to_ycbcr(0, 0, 0), (0, 128, 128));
    assert_eq!(rgb_to_ycbcr(255, 255, 255), (255, 128, 128));
    assert_eq!(rgb_to_ycbcr(255, 0, 0), (76, 85, 255));
    assert_eq!(rgb_to_ycbcr(0, 255, 0), (150, 44, 21));
    assert_eq!(rgb_to_ycbcr(0, 0, 255), (29, 255, 107));
}

#[test]
fn quality_scales_like_libjpeg() {
    let q50 = quant_tables(50);
    assert_eq!(q50, UNSCALED_QUANT);
    let q100 = quant_tables(100);
    assert!(q100.iter().flatten().all(|&v| v == 1));
    let q1 = quant_tables(1);
    assert!(q1.iter().flatten().all(|&v| v == 255));
    assert_eq!(quant_tables(0), q1);
    assert_eq!(quant_tables(250), q100);
    assert_eq!(quant_tables(75)[0][..4], [8, 6, 6, 7]);
}

#[test]
fn huffman_codes_are_canonical() {
    let lut = huffman_luts();
    // Luminance DC: category 0 is the 2-bit code 00, category 1 the 3-bit code 010.
    assert_eq!(lut[0][0], 2 << 24);
    assert_eq!(lut[0][1], 3 << 24 | 0b010);
    // Luminance AC: EOB is 1010 (4 bits), ZRL 11111111001 (11 bits).
    assert_eq!(lut[1][0x00], 4 << 24 | 0b1010);
    assert_eq!(lut[1][0xf0], 11 << 24 | 0b111_1111_1001);
}

#[test]
fn flat_block_has_only_dc() {
    let mut b = [200; 64];
    fdct(&mut b, &dct_basis());
    assert_eq!(b[0], (200 - 128) * 64);
    assert!(b[1..].iter().all(|&c| c == 0));
}

#[test]
fn frame_layout() {
    let rgb = [10u8, 20, 30].repeat(17 * 9);
    let out = encode(Pixels::Rgb(&rgb), 17, 9, 75).expect("encode");
    assert_eq!(out[..4], [0xff, 0xd8, 0xff, 0xdb]);
    assert_eq!(out[out.len() - 2..], [0xff, 0xd9]);
    let sof = out
        .windows(2)
        .position(|w| w == [0xff, 0xc0])
        .expect("SOF0");
    assert_eq!(
        out[sof + 2..sof + 19],
        [0, 17, 8, 0, 9, 0, 17, 3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]
    );
    let gray = vec![7u8; 5 * 3];
    let out = encode(Pixels::Gray(&gray), 5, 3, 75).expect("encode");
    let sof = out
        .windows(2)
        .position(|w| w == [0xff, 0xc0])
        .expect("SOF0");
    assert_eq!(
        out[sof + 2..sof + 13],
        [0, 11, 8, 0, 3, 0, 5, 1, 1, 0x11, 0]
    );
    assert_eq!(
        encode(Pixels::Gray(&[]), 70_000, 1, 75),
        Err(JpegError::TooLarge)
    );
    assert_eq!(
        encode(Pixels::Gray(&[0; 3]), 2, 2, 75),
        Err(JpegError::ShortBuffer)
    );
}
