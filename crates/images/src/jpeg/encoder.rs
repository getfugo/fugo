//! The encoder: markers, tables, Huffman coding of the blocks and the scan.

use super::*;

pub(super) struct Encoder {
    pub(super) out: Vec<u8>,
    /// Pending bits, left-aligned, and how many.
    pub(super) bits: u32,
    pub(super) n_bits: u32,
    pub(super) quant: [[u8; 64]; 2],
    pub(super) lut: HuffmanLut,
    pub(super) basis: [[f64; 8]; 8],
}

impl Encoder {
    /// Appends the low `n_bits` bits of `bits` to the entropy-coded data, stuffing a zero
    /// byte after each 0xff.
    pub(super) fn emit(&mut self, bits: u32, n_bits: u32) {
        let n_bits = n_bits + self.n_bits;
        let mut bits = bits << (32 - n_bits) | self.bits;
        let mut n = n_bits;
        while n >= 8 {
            let b = bits.to_be_bytes()[0];
            self.out.push(b);
            if b == 0xff {
                self.out.push(0);
            }
            bits <<= 8;
            n -= 8;
        }
        self.bits = bits;
        self.n_bits = n;
    }

    pub(super) fn emit_huff(&mut self, table: usize, value: i32) {
        let x = self.lut[table][usize::try_from(value).unwrap_or(0) & 0xff];
        self.emit(x & ((1 << 24) - 1), x >> 24);
    }

    /// A run length and a value: the Huffman code of `run << 4 | size`, then `size` bits of
    /// the value (one's complement for negative values).
    pub(super) fn emit_huff_rle(&mut self, table: usize, run: i32, value: i32) {
        let (a, b) = if value < 0 {
            (-value, value - 1)
        } else {
            (value, value)
        };
        let n = bit_count(a);
        self.emit_huff(table, run << 4 | n.cast_signed());
        if n > 0 {
            self.emit(b.cast_unsigned() & ((1 << n) - 1), n);
        }
    }

    pub(super) fn marker(&mut self, marker: u8, len: usize) {
        let len = u16::try_from(len).unwrap_or(u16::MAX).to_be_bytes();
        self.out.extend_from_slice(&[0xff, marker, len[0], len[1]]);
    }

    pub(super) fn write_dqt(&mut self) {
        self.marker(0xdb, 2 + 2 * 65);
        for (i, q) in (0u8..).zip(self.quant) {
            self.out.push(i);
            self.out.extend_from_slice(&q);
        }
    }

    pub(super) fn write_sof0(&mut self, w: usize, h: usize, gray: bool) {
        let n = if gray { 1 } else { 3 };
        self.marker(0xc0, 8 + 3 * n);
        let (w, h) = (
            u16::try_from(w).unwrap_or(0).to_be_bytes(),
            u16::try_from(h).unwrap_or(0).to_be_bytes(),
        );
        self.out.extend_from_slice(&[8, h[0], h[1], w[0], w[1]]);
        if gray {
            self.out.extend_from_slice(&[1, 1, 0x11, 0x00]);
        } else {
            // 4:2:0: luma sampled 2×2, chroma 1×1 with table 1.
            self.out
                .extend_from_slice(&[3, 1, 0x22, 0x00, 2, 0x11, 0x01, 3, 0x11, 0x01]);
        }
    }

    pub(super) fn write_dht(&mut self, gray: bool) {
        let specs = if gray {
            &HUFFMAN_SPECS[..2]
        } else {
            &HUFFMAN_SPECS[..]
        };
        let len = 2 + specs.iter().map(|s| 17 + s.values.len()).sum::<usize>();
        self.marker(0xc4, len);
        for (class, s) in [0x00, 0x10, 0x01, 0x11].into_iter().zip(specs) {
            self.out.push(class);
            self.out.extend_from_slice(&s.count);
            self.out.extend_from_slice(s.values);
        }
    }

    /// Transforms, quantises and codes a block; returns its quantised DC coefficient.
    pub(super) fn write_block(&mut self, b: &mut Block, q: usize, prev_dc: i32) -> i32 {
        fdct(b, &self.basis);
        let quant = self.quant[q];
        let dc = div(b[0], 8 * i32::from(quant[0]));
        self.emit_huff_rle(2 * q, 0, dc - prev_dc);
        let table = 2 * q + 1;
        let mut run = 0;
        for zig in 1..64 {
            let ac = div(b[UNZIG[zig]], 8 * i32::from(quant[zig]));
            if ac == 0 {
                run += 1;
            } else {
                while run > 15 {
                    self.emit_huff(table, 0xf0);
                    run -= 16;
                }
                self.emit_huff_rle(table, run, ac);
                run = 0;
            }
        }
        if run > 0 {
            self.emit_huff(table, 0x00);
        }
        dc
    }

    pub(super) fn write_sos(&mut self, pixels: Pixels<'_>, w: usize, h: usize) {
        let mut b: Block = [0; 64];
        if let Pixels::Gray(p) = pixels {
            self.out
                .extend_from_slice(&[0xff, 0xda, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3f, 0x00]);
            let mut prev = 0;
            for y in (0..h).step_by(8) {
                for x in (0..w).step_by(8) {
                    for j in 0..8 {
                        let row = (y + j).min(h - 1) * w;
                        for i in 0..8 {
                            b[8 * j + i] = i32::from(p[row + (x + i).min(w - 1)]);
                        }
                    }
                    prev = self.write_block(&mut b, 0, prev);
                }
            }
        } else {
            self.out.extend_from_slice(&[
                0xff, 0xda, 0x00, 0x0c, 0x03, 0x01, 0x00, 0x02, 0x11, 0x03, 0x11, 0x00, 0x3f, 0x00,
            ]);
            let mut cb = [[0; 64]; 4];
            let mut cr = [[0; 64]; 4];
            let (mut prev_y, mut prev_cb, mut prev_cr) = (0, 0, 0);
            for y in (0..h).step_by(16) {
                for x in (0..w).step_by(16) {
                    for i in 0..4 {
                        let p = (x + (i & 1) * 8, y + (i & 2) * 4);
                        to_ycbcr(pixels, (w, h), p, &mut b, &mut cb[i], &mut cr[i]);
                        prev_y = self.write_block(&mut b, 0, prev_y);
                    }
                    downsample(&mut b, &cb);
                    prev_cb = self.write_block(&mut b, 1, prev_cb);
                    downsample(&mut b, &cr);
                    prev_cr = self.write_block(&mut b, 1, prev_cr);
                }
            }
        }
        // Pad the last byte with one bits.
        self.emit(0x7f, 7);
    }
}
