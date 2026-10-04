//! `difflib.SequenceMatcher(None, a, b).get_opcodes()` of CPython: the word hunks of the
//! structdiff report come from it (see [`crate::py`] for why it must be the same algorithm).
//! Translated from CPython 3.14.7's `Lib/difflib.py` (PSF-2.0, THIRD_PARTY/cpython/LICENSE;
//! PROVENANCE.md).

use std::collections::HashMap;

/// An edit: (tag, i1, i2, j1, j2), `a[i1..i2]` becoming `b[j1..j2]`.
pub type Opcode = (&'static str, usize, usize, usize, usize);

/// The opcodes turning `a` into `b`, with the "popular element" heuristic (`autojunk`) and no
/// junk function.
#[must_use]
pub fn opcodes<T: Eq + std::hash::Hash>(a: &[T], b: &[T]) -> Vec<Opcode> {
    let m = Matcher::new(a, b);
    let mut answer = Vec::new();
    let (mut i, mut j) = (0, 0);
    for (ai, bj, size) in m.matching_blocks() {
        let tag = if i < ai && j < bj {
            "replace"
        } else if i < ai {
            "delete"
        } else if j < bj {
            "insert"
        } else {
            ""
        };
        if !tag.is_empty() {
            answer.push((tag, i, ai, j, bj));
        }
        i = ai + size;
        j = bj + size;
        if size > 0 {
            answer.push(("equal", ai, i, bj, j));
        }
    }
    answer
}

struct Matcher<'a, T> {
    a: &'a [T],
    b: &'a [T],
    /// The indices of each element of `b`, without the popular ones.
    b2j: HashMap<&'a T, Vec<usize>>,
}

impl<'a, T: Eq + std::hash::Hash> Matcher<'a, T> {
    fn new(a: &'a [T], b: &'a [T]) -> Self {
        let mut b2j: HashMap<&T, Vec<usize>> = HashMap::new();
        for (i, x) in b.iter().enumerate() {
            b2j.entry(x).or_default().push(i);
        }
        let n = b.len();
        if n >= 200 {
            let ntest = n / 100 + 1;
            b2j.retain(|_, idxs| idxs.len() <= ntest);
        }
        Matcher { a, b, b2j }
    }

    /// The longest matching block of `a[alo..ahi]` and `b[blo..bhi]`, earliest in `a` then in
    /// `b`, extended by equal popular elements on both ends.
    fn find_longest_match(
        &self,
        alo: usize,
        ahi: usize,
        blo: usize,
        bhi: usize,
    ) -> (usize, usize, usize) {
        let (a, b) = (self.a, self.b);
        let (mut besti, mut bestj, mut bestsize) = (alo, blo, 0);
        let mut j2len: HashMap<usize, usize> = HashMap::new();
        for (i, x) in a.iter().enumerate().take(ahi).skip(alo) {
            let mut newj2len = HashMap::new();
            if let Some(js) = self.b2j.get(x) {
                for &j in js {
                    if j < blo {
                        continue;
                    }
                    if j >= bhi {
                        break;
                    }
                    let k = j
                        .checked_sub(1)
                        .and_then(|p| j2len.get(&p))
                        .copied()
                        .unwrap_or(0)
                        + 1;
                    newj2len.insert(j, k);
                    if k > bestsize {
                        (besti, bestj, bestsize) = (i + 1 - k, j + 1 - k, k);
                    }
                }
            }
            j2len = newj2len;
        }
        while besti > alo && bestj > blo && a[besti - 1] == b[bestj - 1] {
            (besti, bestj, bestsize) = (besti - 1, bestj - 1, bestsize + 1);
        }
        while besti + bestsize < ahi
            && bestj + bestsize < bhi
            && a[besti + bestsize] == b[bestj + bestsize]
        {
            bestsize += 1;
        }
        (besti, bestj, bestsize)
    }

    /// The matching blocks, ascending, adjacent ones joined, ending with `(len(a), len(b), 0)`.
    fn matching_blocks(&self) -> Vec<(usize, usize, usize)> {
        let (la, lb) = (self.a.len(), self.b.len());
        let mut queue = vec![(0, la, 0, lb)];
        let mut blocks = Vec::new();
        while let Some((alo, ahi, blo, bhi)) = queue.pop() {
            let (i, j, k) = self.find_longest_match(alo, ahi, blo, bhi);
            if k > 0 {
                blocks.push((i, j, k));
                if alo < i && blo < j {
                    queue.push((alo, i, blo, j));
                }
                if i + k < ahi && j + k < bhi {
                    queue.push((i + k, ahi, j + k, bhi));
                }
            }
        }
        blocks.sort_unstable();
        let (mut i1, mut j1, mut k1) = (0, 0, 0);
        let mut out = Vec::new();
        for (i2, j2, k2) in blocks {
            if i1 + k1 == i2 && j1 + k1 == j2 {
                k1 += k2;
            } else {
                if k1 > 0 {
                    out.push((i1, j1, k1));
                }
                (i1, j1, k1) = (i2, j2, k2);
            }
        }
        if k1 > 0 {
            out.push((i1, j1, k1));
        }
        out.push((la, lb, 0));
        out
    }
}
