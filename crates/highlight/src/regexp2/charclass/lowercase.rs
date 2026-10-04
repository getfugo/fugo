//! Case folding: regexp2's lower-case table (`lcTable`) and the lower case of a set's ranges.

use super::*;

/// How the lower case of a run of code points is computed (`lcMap.op`).
#[derive(Clone, Copy)]
pub(super) enum LcOp {
    Set(u32),
    Add(i32),
    Bor,
    Bad,
}

pub(super) struct LcMap {
    pub(super) min: u32,
    pub(super) max: u32,
    pub(super) op: LcOp,
}

pub(super) const fn lc(min: u32, max: u32, op: LcOp) -> LcMap {
    LcMap { min, max, op }
}

/// regexp2's `lcTable`: intervals on which the lower-case function is non-decreasing.
pub(super) const LC_TABLE: &[LcMap] = &[
    lc(0x0041, 0x005A, LcOp::Add(32)),
    lc(0x00C0, 0x00DE, LcOp::Add(32)),
    lc(0x0100, 0x012E, LcOp::Bor),
    lc(0x0130, 0x0130, LcOp::Set(0x0069)),
    lc(0x0132, 0x0136, LcOp::Bor),
    lc(0x0139, 0x0147, LcOp::Bad),
    lc(0x014A, 0x0176, LcOp::Bor),
    lc(0x0178, 0x0178, LcOp::Set(0x00FF)),
    lc(0x0179, 0x017D, LcOp::Bad),
    lc(0x0181, 0x0181, LcOp::Set(0x0253)),
    lc(0x0182, 0x0184, LcOp::Bor),
    lc(0x0186, 0x0186, LcOp::Set(0x0254)),
    lc(0x0187, 0x0187, LcOp::Set(0x0188)),
    lc(0x0189, 0x018A, LcOp::Add(205)),
    lc(0x018B, 0x018B, LcOp::Set(0x018C)),
    lc(0x018E, 0x018E, LcOp::Set(0x01DD)),
    lc(0x018F, 0x018F, LcOp::Set(0x0259)),
    lc(0x0190, 0x0190, LcOp::Set(0x025B)),
    lc(0x0191, 0x0191, LcOp::Set(0x0192)),
    lc(0x0193, 0x0193, LcOp::Set(0x0260)),
    lc(0x0194, 0x0194, LcOp::Set(0x0263)),
    lc(0x0196, 0x0196, LcOp::Set(0x0269)),
    lc(0x0197, 0x0197, LcOp::Set(0x0268)),
    lc(0x0198, 0x0198, LcOp::Set(0x0199)),
    lc(0x019C, 0x019C, LcOp::Set(0x026F)),
    lc(0x019D, 0x019D, LcOp::Set(0x0272)),
    lc(0x019F, 0x019F, LcOp::Set(0x0275)),
    lc(0x01A0, 0x01A4, LcOp::Bor),
    lc(0x01A7, 0x01A7, LcOp::Set(0x01A8)),
    lc(0x01A9, 0x01A9, LcOp::Set(0x0283)),
    lc(0x01AC, 0x01AC, LcOp::Set(0x01AD)),
    lc(0x01AE, 0x01AE, LcOp::Set(0x0288)),
    lc(0x01AF, 0x01AF, LcOp::Set(0x01B0)),
    lc(0x01B1, 0x01B2, LcOp::Add(217)),
    lc(0x01B3, 0x01B5, LcOp::Bad),
    lc(0x01B7, 0x01B7, LcOp::Set(0x0292)),
    lc(0x01B8, 0x01B8, LcOp::Set(0x01B9)),
    lc(0x01BC, 0x01BC, LcOp::Set(0x01BD)),
    lc(0x01C4, 0x01C5, LcOp::Set(0x01C6)),
    lc(0x01C7, 0x01C8, LcOp::Set(0x01C9)),
    lc(0x01CA, 0x01CB, LcOp::Set(0x01CC)),
    lc(0x01CD, 0x01DB, LcOp::Bad),
    lc(0x01DE, 0x01EE, LcOp::Bor),
    lc(0x01F1, 0x01F2, LcOp::Set(0x01F3)),
    lc(0x01F4, 0x01F4, LcOp::Set(0x01F5)),
    lc(0x01FA, 0x0216, LcOp::Bor),
    lc(0x0386, 0x0386, LcOp::Set(0x03AC)),
    lc(0x0388, 0x038A, LcOp::Add(37)),
    lc(0x038C, 0x038C, LcOp::Set(0x03CC)),
    lc(0x038E, 0x038F, LcOp::Add(63)),
    lc(0x0391, 0x03AB, LcOp::Add(32)),
    lc(0x03E2, 0x03EE, LcOp::Bor),
    lc(0x0401, 0x040F, LcOp::Add(80)),
    lc(0x0410, 0x042F, LcOp::Add(32)),
    lc(0x0460, 0x0480, LcOp::Bor),
    lc(0x0490, 0x04BE, LcOp::Bor),
    lc(0x04C1, 0x04C3, LcOp::Bad),
    lc(0x04C7, 0x04C7, LcOp::Set(0x04C8)),
    lc(0x04CB, 0x04CB, LcOp::Set(0x04CC)),
    lc(0x04D0, 0x04EA, LcOp::Bor),
    lc(0x04EE, 0x04F4, LcOp::Bor),
    lc(0x04F8, 0x04F8, LcOp::Set(0x04F9)),
    lc(0x0531, 0x0556, LcOp::Add(48)),
    lc(0x10A0, 0x10C5, LcOp::Add(48)),
    lc(0x1E00, 0x1EF8, LcOp::Bor),
    lc(0x1F08, 0x1F0F, LcOp::Add(-8)),
    lc(0x1F18, 0x1F1F, LcOp::Add(-8)),
    lc(0x1F28, 0x1F2F, LcOp::Add(-8)),
    lc(0x1F38, 0x1F3F, LcOp::Add(-8)),
    lc(0x1F48, 0x1F4D, LcOp::Add(-8)),
    lc(0x1F59, 0x1F59, LcOp::Set(0x1F51)),
    lc(0x1F5B, 0x1F5B, LcOp::Set(0x1F53)),
    lc(0x1F5D, 0x1F5D, LcOp::Set(0x1F55)),
    lc(0x1F5F, 0x1F5F, LcOp::Set(0x1F57)),
    lc(0x1F68, 0x1F6F, LcOp::Add(-8)),
    lc(0x1F88, 0x1F8F, LcOp::Add(-8)),
    lc(0x1F98, 0x1F9F, LcOp::Add(-8)),
    lc(0x1FA8, 0x1FAF, LcOp::Add(-8)),
    lc(0x1FB8, 0x1FB9, LcOp::Add(-8)),
    lc(0x1FBA, 0x1FBB, LcOp::Add(-74)),
    lc(0x1FBC, 0x1FBC, LcOp::Set(0x1FB3)),
    lc(0x1FC8, 0x1FCB, LcOp::Add(-86)),
    lc(0x1FCC, 0x1FCC, LcOp::Set(0x1FC3)),
    lc(0x1FD8, 0x1FD9, LcOp::Add(-8)),
    lc(0x1FDA, 0x1FDB, LcOp::Add(-100)),
    lc(0x1FE8, 0x1FE9, LcOp::Add(-8)),
    lc(0x1FEA, 0x1FEB, LcOp::Add(-112)),
    lc(0x1FEC, 0x1FEC, LcOp::Set(0x1FE5)),
    lc(0x1FF8, 0x1FF9, LcOp::Add(-128)),
    lc(0x1FFA, 0x1FFB, LcOp::Add(-126)),
    lc(0x1FFC, 0x1FFC, LcOp::Set(0x1FF3)),
    lc(0x2160, 0x216F, LcOp::Add(16)),
    lc(0x24B6, 0x24D0, LcOp::Add(26)),
    lc(0xFF21, 0xFF3A, LcOp::Add(32)),
];

impl CharSet {
    /// Adds the lower-case versions of the class's characters (`addLowercase`): a single
    /// character is replaced by its lower case, a range gains the lower-case ranges.
    pub fn add_lowercase(&mut self) {
        if self.anything {
            return;
        }
        let mut to_add = Vec::new();
        for r in &mut self.ranges {
            if r.first == r.last {
                let lower = char::from_u32(r.first).map_or(r.first, |c| to_lower(c) as u32);
                *r = Range {
                    first: lower,
                    last: lower,
                };
            } else {
                to_add.push(*r);
            }
        }
        for r in to_add {
            self.add_lowercase_range(r.first, r.last);
        }
        self.canonicalize();
    }

    pub(super) fn add_lowercase_range(&mut self, ch_min: u32, ch_max: u32) {
        let mut i = LC_TABLE.partition_point(|lc| lc.max < ch_min);
        while i < LC_TABLE.len() {
            let lc = &LC_TABLE[i];
            i += 1;
            if lc.min > ch_max {
                return;
            }
            let mut min_t = lc.min.max(ch_min);
            let mut max_t = lc.max.min(ch_max);
            match lc.op {
                LcOp::Set(v) => {
                    min_t = v;
                    max_t = v;
                }
                LcOp::Add(d) => {
                    min_t = min_t.wrapping_add_signed(d);
                    max_t = max_t.wrapping_add_signed(d);
                }
                LcOp::Bor => {
                    min_t |= 1;
                    max_t |= 1;
                }
                LcOp::Bad => {
                    min_t += min_t & 1;
                    max_t += max_t & 1;
                }
            }
            if min_t < ch_min || max_t > ch_max {
                self.add_range(min_t, max_t);
            }
        }
    }
}
