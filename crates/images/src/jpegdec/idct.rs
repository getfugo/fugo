//! The inverse DCT: Go 1.25's (`idct.go`), the MPEG Software Simulation Group's integer IDCT.

use super::*;

// `idct.go` (Go 1.25): the MPEG Software Simulation Group's integer IDCT, from mpeg2decode's
// `idct.c` ("These software programs are available to the user without any license fee or
// royalty on an 'as is' basis"), as the Go Authors translated it.
pub(super) const W1: i32 = 2841;

pub(super) const W2: i32 = 2676;

pub(super) const W3: i32 = 2408;

pub(super) const W5: i32 = 1609;

pub(super) const W6: i32 = 1108;

pub(super) const W7: i32 = 565;

pub(super) const W1PW7: i32 = W1 + W7;

pub(super) const W1MW7: i32 = W1 - W7;

pub(super) const W2PW6: i32 = W2 + W6;

pub(super) const W2MW6: i32 = W2 - W6;

pub(super) const W3PW5: i32 = W3 + W5;

pub(super) const W3MW5: i32 = W3 - W5;

pub(super) const R2: i32 = 181;

/// `idct`: the 2-D inverse DCT in fixed point, rows then columns. Go's int32 arithmetic
/// wraps, so every operation here wraps too.
pub(super) fn idct(src: &mut Block) {
    use std::num::Wrapping as W;
    let w = W;
    // Horizontal 1-D IDCT.
    for y in 0..8 {
        let s = &mut src[y * 8..y * 8 + 8];
        if s[1..].iter().all(|&v| v == 0) {
            let dc = s[0] << 3;
            s.fill(dc);
            continue;
        }
        let mut x0 = (w(s[0]) << 11) + w(128);
        let mut x1 = w(s[4]) << 11;
        let mut x2 = w(s[6]);
        let mut x3 = w(s[2]);
        let mut x4 = w(s[1]);
        let mut x5 = w(s[7]);
        let mut x6 = w(s[5]);
        let mut x7 = w(s[3]);
        // Stage 1.
        let mut x8 = w(W7) * (x4 + x5);
        x4 = x8 + w(W1MW7) * x4;
        x5 = x8 - w(W1PW7) * x5;
        x8 = w(W3) * (x6 + x7);
        x6 = x8 - w(W3MW5) * x6;
        x7 = x8 - w(W3PW5) * x7;
        // Stage 2.
        x8 = x0 + x1;
        x0 -= x1;
        x1 = w(W6) * (x3 + x2);
        x2 = x1 - w(W2PW6) * x2;
        x3 = x1 + w(W2MW6) * x3;
        x1 = x4 + x6;
        x4 -= x6;
        x6 = x5 + x7;
        x5 -= x7;
        // Stage 3.
        x7 = x8 + x3;
        x8 -= x3;
        x3 = x0 + x2;
        x0 -= x2;
        x2 = (w(R2) * (x4 + x5) + w(128)) >> 8;
        x4 = (w(R2) * (x4 - x5) + w(128)) >> 8;
        // Stage 4.
        s[0] = ((x7 + x1) >> 8).0;
        s[1] = ((x3 + x2) >> 8).0;
        s[2] = ((x0 + x4) >> 8).0;
        s[3] = ((x8 + x6) >> 8).0;
        s[4] = ((x8 - x6) >> 8).0;
        s[5] = ((x0 - x4) >> 8).0;
        s[6] = ((x3 - x2) >> 8).0;
        s[7] = ((x7 - x1) >> 8).0;
    }
    // Vertical 1-D IDCT.
    for x in 0..8 {
        let at = |r: usize| w(src[8 * r + x]);
        let mut y0 = (at(0) << 8) + w(8192);
        let mut y1 = at(4) << 8;
        let mut y2 = at(6);
        let mut y3 = at(2);
        let mut y4 = at(1);
        let mut y5 = at(7);
        let mut y6 = at(5);
        let mut y7 = at(3);
        // Stage 1.
        let mut y8 = w(W7) * (y4 + y5) + w(4);
        y4 = (y8 + w(W1MW7) * y4) >> 3;
        y5 = (y8 - w(W1PW7) * y5) >> 3;
        y8 = w(W3) * (y6 + y7) + w(4);
        y6 = (y8 - w(W3MW5) * y6) >> 3;
        y7 = (y8 - w(W3PW5) * y7) >> 3;
        // Stage 2.
        y8 = y0 + y1;
        y0 -= y1;
        y1 = w(W6) * (y3 + y2) + w(4);
        y2 = (y1 - w(W2PW6) * y2) >> 3;
        y3 = (y1 + w(W2MW6) * y3) >> 3;
        y1 = y4 + y6;
        y4 -= y6;
        y6 = y5 + y7;
        y5 -= y7;
        // Stage 3.
        y7 = y8 + y3;
        y8 -= y3;
        y3 = y0 + y2;
        y0 -= y2;
        y2 = (w(R2) * (y4 + y5) + w(128)) >> 8;
        y4 = (w(R2) * (y4 - y5) + w(128)) >> 8;
        // Stage 4.
        let out = [
            (y7 + y1) >> 14,
            (y3 + y2) >> 14,
            (y0 + y4) >> 14,
            (y8 + y6) >> 14,
            (y8 - y6) >> 14,
            (y0 - y4) >> 14,
            (y3 - y2) >> 14,
            (y7 - y1) >> 14,
        ];
        for (r, v) in out.into_iter().enumerate() {
            src[8 * r + x] = v.0;
        }
    }
}
