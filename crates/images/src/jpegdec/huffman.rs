//! Huffman tables and decoding (Go's `huffman.go`).

use super::*;

impl Decoder<'_> {
    /// `ensureNBits`.
    pub(super) fn ensure_n_bits(&mut self, n: i32) -> Result<()> {
        loop {
            let c = match self.read_byte_stuffed_byte() {
                Ok(c) => c,
                Err(JpegError::UnexpectedEof) => return Err(SHORT_HUFFMAN_DATA),
                Err(e) => return Err(e),
            };
            self.bits.a = (self.bits.a << 8) | u32::from(c);
            self.bits.n += 8;
            if self.bits.m == 0 {
                self.bits.m = 1 << 7;
            } else {
                self.bits.m <<= 8;
            }
            if self.bits.n >= n {
                return Ok(());
            }
        }
    }

    /// `receiveExtend` (section F.2.2.1).
    pub(super) fn receive_extend(&mut self, t: u8) -> Result<i32> {
        if self.bits.n < i32::from(t) {
            self.ensure_n_bits(i32::from(t))?;
        }
        self.bits.n -= i32::from(t);
        self.bits.m >>= t;
        let s = 1i32 << t;
        let mut x = (self.bits.a >> self.bits.n) as i32 & (s - 1);
        if x < s >> 1 {
            x += (-1i32 << t) + 1;
        }
        Ok(x)
    }

    /// `processDHT` (section B.2.4.2).
    pub(super) fn process_dht(&mut self, n: usize) -> Result<()> {
        let mut n = n as i64;
        while n > 0 {
            if n < 17 {
                return Err(JpegError::Format("DHT has wrong length"));
            }
            self.read_full(0, 17)?;
            let tc = self.tmp[0] >> 4;
            if tc > MAX_TC {
                return Err(JpegError::Format("bad Tc value"));
            }
            let th = self.tmp[0] & 0x0f;
            if th > MAX_TH || (self.baseline && th > 1) {
                return Err(JpegError::Format("bad Th value"));
            }
            let (tc, th) = (usize::from(tc), usize::from(th));
            let mut n_codes = [0i32; MAX_CODE_LENGTH];
            let mut total = 0i32;
            for (i, c) in n_codes.iter_mut().enumerate() {
                *c = i32::from(self.tmp[i + 1]);
                total += *c;
            }
            self.huff[tc][th].n_codes = total;
            if total == 0 {
                return Err(JpegError::Format("Huffman table has zero length"));
            }
            if total > MAX_N_CODES as i32 {
                return Err(JpegError::Format("Huffman table has excessive length"));
            }
            n -= i64::from(total) + 17;
            if n < 0 {
                return Err(JpegError::Format("DHT has wrong length"));
            }
            self.read_vals(tc, th, total as usize)?;
            let h = &mut self.huff[tc][th];
            // The look-up table.
            h.lut = [0; 1 << LUT_SIZE];
            let (mut x, mut code) = (0usize, 0u32);
            for i in 0..LUT_SIZE {
                code <<= 1;
                for _ in 0..n_codes[i as usize] {
                    let base = (code << (7 - i)) as u8;
                    let lut_value = (u16::from(h.vals[x]) << 8) | (2 + i) as u16;
                    for k in 0..1u16 << (7 - i) {
                        h.lut[usize::from(base) | usize::from(k)] = lut_value;
                    }
                    code += 1;
                    x += 1;
                }
            }
            // minCodes, maxCodes and valsIndices.
            let (mut c, mut index) = (0i32, 0i32);
            for (i, &nc) in n_codes.iter().enumerate() {
                if nc == 0 {
                    h.min_codes[i] = -1;
                    h.max_codes[i] = -1;
                    h.vals_indices[i] = -1;
                } else {
                    h.min_codes[i] = c;
                    h.max_codes[i] = c + nc - 1;
                    h.vals_indices[i] = index;
                    c += nc;
                    index += nc;
                }
                c <<= 1;
            }
        }
        Ok(())
    }

    /// `decodeHuffman`: the next value coded with table `(tc, th)`.
    pub(super) fn decode_huffman(&mut self, tc: usize, th: usize) -> Result<u8> {
        if self.huff[tc][th].n_codes == 0 {
            return Err(JpegError::Format("uninitialized Huffman table"));
        }
        let mut slow = false;
        if self.bits.n < 8
            && let Err(e) = self.ensure_n_bits(8)
        {
            if e != MISSING_FF00 && e != SHORT_HUFFMAN_DATA {
                return Err(e);
            }
            // No more bytes in this segment, but the bits already read may still hold the
            // next symbol: undo the overshoot first.
            if self.n_unreadable != 0 {
                self.unread_byte_stuffed_byte();
            }
            slow = true;
        }
        if !slow {
            let idx = ((self.bits.a >> (self.bits.n - LUT_SIZE as i32)) & 0xff) as usize;
            let v = self.huff[tc][th].lut[idx];
            if v != 0 {
                let n = (v & 0xff) - 1;
                self.bits.n -= i32::from(n);
                self.bits.m >>= n;
                return Ok((v >> 8) as u8);
            }
        }
        let mut code = 0i32;
        for i in 0..MAX_CODE_LENGTH {
            if self.bits.n == 0 {
                self.ensure_n_bits(1)?;
            }
            if self.bits.a & self.bits.m != 0 {
                code |= 1;
            }
            self.bits.n -= 1;
            self.bits.m >>= 1;
            let h = &self.huff[tc][th];
            if code <= h.max_codes[i] {
                return Ok(h.vals[(h.vals_indices[i] + code - h.min_codes[i]) as usize]);
            }
            code <<= 1;
        }
        Err(JpegError::Format("bad Huffman code"))
    }

    /// `decodeBit`.
    pub(super) fn decode_bit(&mut self) -> Result<bool> {
        if self.bits.n == 0 {
            self.ensure_n_bits(1)?;
        }
        let ret = self.bits.a & self.bits.m != 0;
        self.bits.n -= 1;
        self.bits.m >>= 1;
        Ok(ret)
    }

    /// `decodeBits`.
    pub(super) fn decode_bits(&mut self, n: i32) -> Result<u32> {
        if self.bits.n < n {
            self.ensure_n_bits(n)?;
        }
        let mut ret = self.bits.a >> (self.bits.n - n);
        ret &= (1u32 << n) - 1;
        self.bits.n -= n;
        self.bits.m >>= n;
        Ok(ret)
    }
}
