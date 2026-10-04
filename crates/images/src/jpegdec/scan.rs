//! The scans: baseline and progressive, refinement and restarts (Go's `scan.go`).

use super::*;

impl Decoder<'_> {
    /// `processSOS` (section B.2.3).
    pub(super) fn process_sos(&mut self, n: usize) -> Result<()> {
        if self.n_comp == 0 {
            return Err(JpegError::Format("missing SOF marker"));
        }
        if n < 6 || 4 + 2 * self.n_comp < n || !n.is_multiple_of(2) {
            return Err(JpegError::Format("SOS has wrong length"));
        }
        self.read_full(0, n)?;
        let n_comp = usize::from(self.tmp[0]);
        if n != 4 + 2 * n_comp {
            return Err(JpegError::Format(
                "SOS length inconsistent with number of components",
            ));
        }
        // (component index, DC table, AC table) of each scan component.
        let mut scan = [(0usize, 0usize, 0usize); MAX_COMPONENTS];
        let mut total_hv = 0;
        for i in 0..n_comp {
            let cs = self.tmp[1 + 2 * i];
            let comp_index = (0..self.n_comp)
                .rev()
                .find(|&j| self.comp[j].c == cs)
                .ok_or(JpegError::Format("unknown component selector"))?;
            scan[i].0 = comp_index;
            if scan[..i].iter().any(|s| s.0 == comp_index) {
                return Err(JpegError::Format("repeated component selector"));
            }
            total_hv += self.comp[comp_index].h * self.comp[comp_index].v;
            let td = self.tmp[2 + 2 * i] >> 4;
            if td > MAX_TH || (self.baseline && td > 1) {
                return Err(JpegError::Format("bad Td value"));
            }
            let ta = self.tmp[2 + 2 * i] & 0x0f;
            if ta > MAX_TH || (self.baseline && ta > 1) {
                return Err(JpegError::Format("bad Ta value"));
            }
            scan[i].1 = usize::from(td);
            scan[i].2 = usize::from(ta);
        }
        if self.n_comp > 1 && total_hv > 10 {
            return Err(JpegError::Format("total sampling factors too large"));
        }
        // Spectral selection and successive approximation (fixed for sequential JPEGs).
        let (mut zig_start, mut zig_end, mut ah, mut al) =
            (0i32, BLOCK_SIZE as i32 - 1, 0u32, 0u32);
        if self.progressive {
            zig_start = i32::from(self.tmp[1 + 2 * n_comp]);
            zig_end = i32::from(self.tmp[2 + 2 * n_comp]);
            ah = u32::from(self.tmp[3 + 2 * n_comp] >> 4);
            al = u32::from(self.tmp[3 + 2 * n_comp] & 0x0f);
            if (zig_start == 0 && zig_end != 0)
                || zig_start > zig_end
                || BLOCK_SIZE as i32 <= zig_end
            {
                return Err(JpegError::Format("bad spectral selection bounds"));
            }
            if zig_start != 0 && n_comp != 1 {
                return Err(JpegError::Format(
                    "progressive AC coefficients for more than one component",
                ));
            }
            if ah != 0 && ah != al + 1 {
                return Err(JpegError::Format("bad successive approximation values"));
            }
        }
        // The number of MCUs.
        let (h0, v0) = (self.comp[0].h, self.comp[0].v);
        let mxx = self.width.div_ceil(8 * h0);
        let myy = self.height.div_ceil(8 * v0);
        if self.img1.is_none() && self.img3.is_none() {
            self.make_img(mxx, myy);
        }
        if self.progressive {
            for s in &scan[..n_comp] {
                let ci = s.0;
                if self.prog_coeffs[ci].is_empty() {
                    self.prog_coeffs[ci] =
                        vec![[0; BLOCK_SIZE]; mxx * myy * self.comp[ci].h * self.comp[ci].v];
                }
            }
        }

        self.bits = Bits::default();
        let (mut mcu, mut expected_rst) = (0usize, RST0_MARKER);
        let mut dc = [0i32; MAX_COMPONENTS];
        let mut block_count = 0usize;
        for my in 0..myy {
            for mx in 0..mxx {
                for &(comp_index, td, ta) in &scan[..n_comp] {
                    let hi = self.comp[comp_index].h;
                    let vi = self.comp[comp_index].v;
                    for j in 0..hi * vi {
                        // Interleaved scans go MCU by MCU; non-interleaved ones left to
                        // right, top to bottom, skipping blocks outside the image.
                        let (bx, by);
                        if n_comp != 1 {
                            bx = hi * mx + j % hi;
                            by = vi * my + j / hi;
                        } else {
                            let q = mxx * hi;
                            bx = block_count % q;
                            by = block_count / q;
                            block_count += 1;
                            if bx * 8 >= self.width || by * 8 >= self.height {
                                continue;
                            }
                        }
                        let mut b: Block = if self.progressive {
                            self.prog_coeffs[comp_index][by * mxx * hi + bx]
                        } else {
                            [0; BLOCK_SIZE]
                        };
                        if ah != 0 {
                            self.refine(&mut b, ta, zig_start, zig_end, 1 << al)?;
                        } else {
                            let mut zig = zig_start;
                            if zig == 0 {
                                zig += 1;
                                // The DC coefficient (section F.2.2.1).
                                let value = self.decode_huffman(DC_TABLE, td)?;
                                if value > 16 {
                                    return Err(JpegError::Unsupported("excessive DC component"));
                                }
                                let dc_delta = self.receive_extend(value)?;
                                dc[comp_index] = dc[comp_index].wrapping_add(dc_delta);
                                b[0] = dc[comp_index] << al;
                            }
                            if zig <= zig_end && self.eob_run > 0 {
                                self.eob_run -= 1;
                            } else {
                                // The AC coefficients (section F.2.2.2).
                                while zig <= zig_end {
                                    let value = self.decode_huffman(AC_TABLE, ta)?;
                                    let val0 = value >> 4;
                                    let val1 = value & 0x0f;
                                    if val1 != 0 {
                                        zig += i32::from(val0);
                                        if zig > zig_end {
                                            break;
                                        }
                                        let ac = self.receive_extend(val1)?;
                                        b[UNZIG[zig as usize]] = ac << al;
                                    } else {
                                        if val0 != 0x0f {
                                            self.eob_run = 1u16 << val0;
                                            if val0 != 0 {
                                                let bits = self.decode_bits(i32::from(val0))?;
                                                self.eob_run |= bits as u16;
                                            }
                                            self.eob_run = self.eob_run.wrapping_sub(1);
                                            break;
                                        }
                                        zig += 0x0f;
                                    }
                                    zig += 1;
                                }
                            }
                        }
                        if self.progressive {
                            // Reconstructed after the last scan.
                            self.prog_coeffs[comp_index][by * mxx * hi + bx] = b;
                            continue;
                        }
                        self.reconstruct_block(&mut b, bx, by, comp_index)?;
                    }
                }
                mcu += 1;
                if self.ri > 0 && mcu % self.ri == 0 && mcu < mxx * myy {
                    // The RST marker should follow; resynchronise on corrupt input.
                    self.read_full(0, 2)?;
                    if self.tmp[0] != 0xff || self.tmp[1] != expected_rst {
                        self.find_rst(expected_rst)?;
                    }
                    expected_rst += 1;
                    if expected_rst == RST7_MARKER + 1 {
                        expected_rst = RST0_MARKER;
                    }
                    self.bits = Bits::default();
                    dc = [0; MAX_COMPONENTS];
                    self.eob_run = 0;
                }
            }
        }
        Ok(())
    }

    /// `refine`: a successive approximation refinement (section G.1.2).
    pub(super) fn refine(
        &mut self,
        b: &mut Block,
        ta: usize,
        zig_start: i32,
        zig_end: i32,
        delta: i32,
    ) -> Result<()> {
        if zig_start == 0 {
            // DC refinement (zigEnd is 0, checked above).
            if self.decode_bit()? {
                b[0] |= delta;
            }
            return Ok(());
        }
        let mut zig = zig_start;
        if self.eob_run == 0 {
            while zig <= zig_end {
                let mut z = 0i32;
                let value = self.decode_huffman(AC_TABLE, ta)?;
                let val0 = value >> 4;
                let val1 = value & 0x0f;
                match val1 {
                    0 => {
                        if val0 != 0x0f {
                            self.eob_run = 1u16 << val0;
                            if val0 != 0 {
                                let bits = self.decode_bits(i32::from(val0))?;
                                self.eob_run |= bits as u16;
                            }
                            break;
                        }
                    }
                    1 => {
                        z = delta;
                        if !self.decode_bit()? {
                            z = -z;
                        }
                    }
                    _ => return Err(JpegError::Format("unexpected Huffman code")),
                }
                zig = self.refine_non_zeroes(b, zig, zig_end, i32::from(val0), delta)?;
                if zig > zig_end {
                    return Err(JpegError::Format("too many coefficients"));
                }
                if z != 0 {
                    b[UNZIG[zig as usize]] = z;
                }
                zig += 1;
            }
        }
        if self.eob_run > 0 {
            self.eob_run -= 1;
            self.refine_non_zeroes(b, zig, zig_end, -1, delta)?;
        }
        Ok(())
    }

    /// `refineNonZeroes`: refines the non-zero coefficients in zig-zag order, skipping the
    /// first `nz` zero ones when `nz >= 0`.
    pub(super) fn refine_non_zeroes(
        &mut self,
        b: &mut Block,
        mut zig: i32,
        zig_end: i32,
        mut nz: i32,
        delta: i32,
    ) -> Result<i32> {
        while zig <= zig_end {
            let u = UNZIG[zig as usize];
            if b[u] == 0 {
                if nz == 0 {
                    break;
                }
                nz -= 1;
                zig += 1;
                continue;
            }
            if self.decode_bit()? {
                if b[u] >= 0 {
                    b[u] = b[u].wrapping_add(delta);
                } else {
                    b[u] = b[u].wrapping_sub(delta);
                }
            }
            zig += 1;
        }
        Ok(zig)
    }

    /// `reconstructProgressiveImage`.
    pub(super) fn reconstruct_progressive_image(&mut self) -> Result<()> {
        let h0 = self.comp[0].h;
        let mxx = self.width.div_ceil(8 * h0);
        for i in 0..self.n_comp {
            if self.prog_coeffs[i].is_empty() {
                continue;
            }
            let v = 8 * self.comp[0].v / self.comp[i].v;
            let h = 8 * self.comp[0].h / self.comp[i].h;
            let stride = mxx * self.comp[i].h;
            let mut coeffs = std::mem::take(&mut self.prog_coeffs[i]);
            let mut by = 0;
            while by * v < self.height {
                let mut bx = 0;
                while bx * h < self.width {
                    self.reconstruct_block(&mut coeffs[by * stride + bx], bx, by, i)?;
                    bx += 1;
                }
                by += 1;
            }
            self.prog_coeffs[i] = coeffs;
        }
        Ok(())
    }

    /// `reconstructBlock`: dequantises, inverse-transforms and stores a block, level-shifted
    /// by 128 and clipped.
    pub(super) fn reconstruct_block(
        &mut self,
        b: &mut Block,
        bx: usize,
        by: usize,
        comp_index: usize,
    ) -> Result<()> {
        let qt = &self.quant[usize::from(self.comp[comp_index].tq)];
        for zig in 0..BLOCK_SIZE {
            b[UNZIG[zig]] = b[UNZIG[zig]].wrapping_mul(qt[zig]);
        }
        idct(b);
        let (dst, stride): (&mut [u8], usize) = if self.n_comp == 1 {
            let (pix, stride) = self
                .img1
                .as_mut()
                .ok_or(JpegError::Format("missing SOS marker"))?;
            (pix.as_mut_slice(), *stride)
        } else {
            let img3 = self
                .img3
                .as_mut()
                .ok_or(JpegError::Format("missing SOS marker"))?;
            match comp_index {
                0 => (img3.y.as_mut_slice(), img3.y_stride),
                1 => (img3.cb.as_mut_slice(), img3.c_stride),
                2 => (img3.cr.as_mut_slice(), img3.c_stride),
                3 => {
                    let (pix, stride) = self
                        .black
                        .as_mut()
                        .ok_or(JpegError::Unsupported("too many components"))?;
                    (pix.as_mut_slice(), *stride)
                }
                _ => return Err(JpegError::Unsupported("too many components")),
            }
        };
        let base = 8 * (by * stride + bx);
        for y in 0..8 {
            for x in 0..8 {
                let c = b[y * 8 + x];
                let v = if c < -128 {
                    0
                } else if c > 127 {
                    255
                } else {
                    (c + 128) as u8
                };
                // Go panics on blocks beyond the planes (corrupt sampling factors).
                let slot = dst
                    .get_mut(base + y * stride + x)
                    .ok_or(JpegError::Format("block outside the image"))?;
                *slot = v;
            }
        }
        Ok(())
    }

    /// `findRST`: skips to the expected restart marker; any other marker is an error.
    pub(super) fn find_rst(&mut self, expected_rst: u8) -> Result<()> {
        loop {
            let mut i = 0;
            if self.tmp[0] == 0xff {
                if self.tmp[1] == expected_rst {
                    return Ok(());
                } else if self.tmp[1] == 0xff {
                    i = 1;
                } else if self.tmp[1] != 0x00 {
                    return Err(JpegError::Format("bad RST marker"));
                }
            } else if self.tmp[1] == 0xff {
                self.tmp[0] = 0xff;
                i = 1;
            }
            self.read_full(i, 2)?;
        }
    }
}
