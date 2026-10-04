//! Reading the markers and segments before the scans (Go's `reader.go`).

use super::*;

impl Decoder<'_> {
    /// `unreadByteStuffedByte`.
    pub(super) fn unread_byte_stuffed_byte(&mut self) {
        self.i -= self.n_unreadable;
        self.n_unreadable = 0;
        if self.bits.n >= 8 {
            self.bits.a >>= 8;
            self.bits.n -= 8;
            self.bits.m >>= 8;
        }
    }

    /// `readByte`.
    pub(super) fn read_byte(&mut self) -> Result<u8> {
        let x = *self.data.get(self.i).ok_or(JpegError::UnexpectedEof)?;
        self.i += 1;
        self.n_unreadable = 0;
        Ok(x)
    }

    /// `readByteStuffedByte`: a byte of entropy-coded data (`0xff 0x00` is `0xff`).
    pub(super) fn read_byte_stuffed_byte(&mut self) -> Result<u8> {
        if self.i + 2 <= self.data.len() {
            let x = self.data[self.i];
            self.i += 1;
            self.n_unreadable = 1;
            if x != 0xff {
                return Ok(x);
            }
            if self.data[self.i] != 0x00 {
                return Err(MISSING_FF00);
            }
            self.i += 1;
            self.n_unreadable = 2;
            return Ok(0xff);
        }
        self.n_unreadable = 0;
        let x = self.read_byte()?;
        self.n_unreadable = 1;
        if x != 0xff {
            return Ok(x);
        }
        let x = self.read_byte()?;
        self.n_unreadable = 2;
        if x != 0x00 {
            return Err(MISSING_FF00);
        }
        Ok(0xff)
    }

    /// Unreads the overshot bytes of the entropy-coded data, if any (`readFull`, `ignore`).
    pub(super) fn unread_overshoot(&mut self) {
        if self.n_unreadable != 0 {
            if self.bits.n >= 8 {
                self.unread_byte_stuffed_byte();
            }
            self.n_unreadable = 0;
        }
    }

    /// `readFull` into `tmp[from..to]`.
    pub(super) fn read_full(&mut self, from: usize, to: usize) -> Result<()> {
        self.unread_overshoot();
        let n = to - from;
        let src = self
            .data
            .get(self.i..self.i + n)
            .ok_or(JpegError::UnexpectedEof)?;
        self.tmp[from..to].copy_from_slice(src);
        self.i += n;
        Ok(())
    }

    /// `readFull` into a Huffman table's values.
    pub(super) fn read_vals(&mut self, tc: usize, th: usize, n: usize) -> Result<()> {
        self.unread_overshoot();
        let src = self
            .data
            .get(self.i..self.i + n)
            .ok_or(JpegError::UnexpectedEof)?;
        self.huff[tc][th].vals[..n].copy_from_slice(src);
        self.i += n;
        Ok(())
    }

    /// `ignore`.
    pub(super) fn ignore(&mut self, n: usize) -> Result<()> {
        self.unread_overshoot();
        if self.i + n > self.data.len() {
            self.i = self.data.len();
            return Err(JpegError::UnexpectedEof);
        }
        self.i += n;
        Ok(())
    }

    /// `processSOF` (section B.2.2).
    pub(super) fn process_sof(&mut self, n: usize) -> Result<()> {
        if self.n_comp != 0 {
            return Err(JpegError::Format("multiple SOF markers"));
        }
        self.n_comp = match n {
            9 => 1,
            15 => 3,
            18 => 4,
            _ => return Err(JpegError::Unsupported("number of components")),
        };
        self.read_full(0, n)?;
        if self.tmp[0] != 8 {
            return Err(JpegError::Unsupported("precision"));
        }
        self.height = (usize::from(self.tmp[1]) << 8) + usize::from(self.tmp[2]);
        self.width = (usize::from(self.tmp[3]) << 8) + usize::from(self.tmp[4]);
        if usize::from(self.tmp[5]) != self.n_comp {
            return Err(JpegError::Format("SOF has wrong length"));
        }
        for i in 0..self.n_comp {
            self.comp[i].c = self.tmp[6 + 3 * i];
            for j in 0..i {
                if self.comp[i].c == self.comp[j].c {
                    return Err(JpegError::Format("repeated component identifier"));
                }
            }
            self.comp[i].tq = self.tmp[8 + 3 * i];
            if self.comp[i].tq > MAX_TQ {
                return Err(JpegError::Format("bad Tq value"));
            }
            let hv = self.tmp[7 + 3 * i];
            let (mut h, mut v) = (usize::from(hv >> 4), usize::from(hv & 0x0f));
            if !(1..=4).contains(&h) || !(1..=4).contains(&v) {
                return Err(JpegError::Format("luma/chroma subsampling ratio"));
            }
            if h == 3 || v == 3 {
                return Err(UNSUPPORTED_SUBSAMPLING);
            }
            match self.n_comp {
                // A single component is non-interleaved: its (h, v) is effectively (1, 1).
                1 => (h, v) = (1, 1),
                3 => match i {
                    0 if v == 4 => return Err(UNSUPPORTED_SUBSAMPLING),
                    1 if !self.comp[0].h.is_multiple_of(h) || !self.comp[0].v.is_multiple_of(v) => {
                        return Err(UNSUPPORTED_SUBSAMPLING);
                    }
                    2 if self.comp[1].h != h || self.comp[1].v != v => {
                        return Err(UNSUPPORTED_SUBSAMPLING);
                    }
                    _ => {}
                },
                _ => match i {
                    0 if hv != 0x11 && hv != 0x22 => return Err(UNSUPPORTED_SUBSAMPLING),
                    1 | 2 if hv != 0x11 => return Err(UNSUPPORTED_SUBSAMPLING),
                    3 if self.comp[0].h != h || self.comp[0].v != v => {
                        return Err(UNSUPPORTED_SUBSAMPLING);
                    }
                    _ => {}
                },
            }
            self.comp[i].h = h;
            self.comp[i].v = v;
        }
        Ok(())
    }

    /// `processDQT` (section B.2.4.1).
    pub(super) fn process_dqt(&mut self, mut n: usize) -> Result<()> {
        while n > 0 {
            n -= 1;
            let x = self.read_byte()?;
            let tq = usize::from(x & 0x0f);
            if tq > usize::from(MAX_TQ) {
                return Err(JpegError::Format("bad Tq value"));
            }
            match x >> 4 {
                0 => {
                    if n < BLOCK_SIZE {
                        break;
                    }
                    n -= BLOCK_SIZE;
                    self.read_full(0, BLOCK_SIZE)?;
                    for i in 0..BLOCK_SIZE {
                        self.quant[tq][i] = i32::from(self.tmp[i]);
                    }
                }
                1 => {
                    if n < 2 * BLOCK_SIZE {
                        break;
                    }
                    n -= 2 * BLOCK_SIZE;
                    self.read_full(0, 2 * BLOCK_SIZE)?;
                    for i in 0..BLOCK_SIZE {
                        self.quant[tq][i] =
                            (i32::from(self.tmp[2 * i]) << 8) | i32::from(self.tmp[2 * i + 1]);
                    }
                }
                _ => return Err(JpegError::Format("bad Pq value")),
            }
        }
        if n != 0 {
            return Err(JpegError::Format("DQT has wrong length"));
        }
        Ok(())
    }

    /// `processDRI` (section B.2.4.4).
    pub(super) fn process_dri(&mut self, n: usize) -> Result<()> {
        if n != 2 {
            return Err(JpegError::Format("DRI has wrong length"));
        }
        self.read_full(0, 2)?;
        self.ri = (usize::from(self.tmp[0]) << 8) + usize::from(self.tmp[1]);
        Ok(())
    }

    /// `processApp0Marker`: notes a JFIF header.
    pub(super) fn process_app0(&mut self, mut n: usize) -> Result<()> {
        if n < 5 {
            return self.ignore(n);
        }
        self.read_full(0, 5)?;
        n -= 5;
        self.jfif = &self.tmp[..5] == b"JFIF\0";
        if n > 0 {
            return self.ignore(n);
        }
        Ok(())
    }

    /// `processApp14Marker`: notes an Adobe colour transform.
    pub(super) fn process_app14(&mut self, mut n: usize) -> Result<()> {
        if n < 12 {
            return self.ignore(n);
        }
        self.read_full(0, 12)?;
        n -= 12;
        if &self.tmp[..5] == b"Adobe" {
            self.adobe_transform_valid = true;
            self.adobe_transform = self.tmp[11];
        }
        if n > 0 {
            return self.ignore(n);
        }
        Ok(())
    }
}
