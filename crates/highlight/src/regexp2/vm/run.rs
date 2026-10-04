//! The interpreter: runs a program at one position with its backtracking stack (regexp2's `run`).

use super::*;

impl Program {
    #[expect(clippy::too_many_lines, reason = "the interpreter's dispatch")]
    pub(super) fn run(&self, text: &[char], textstart: usize, at: usize, st: &mut State) -> bool {
        st.caps.clear();
        st.caps.resize(self.ncap, None);
        st.capstart.clear();
        st.capstart.resize(self.ncap, 0);
        st.mark.clear();
        st.mark.resize(self.nreg, 0);
        st.count.clear();
        st.count.resize(self.nreg, 0);
        st.bt.clear();
        st.frames.clear();
        let len = text.len();
        let mut pc = 0usize;
        let mut pos = at;
        let mut steps: u64 = 0;
        loop {
            steps += 1;
            if steps > STEP_LIMIT {
                return false;
            }
            let ok = match &self.insts[pc] {
                Inst::Leaf(leaf, f) => match Self::step(text, pos, leaf, *f) {
                    Some(p) => {
                        pos = p;
                        pc += 1;
                        true
                    }
                    None => false,
                },
                Inst::Multi(s, f) => {
                    let n = s.len();
                    let from = if f.rtl { pos.checked_sub(n) } else { Some(pos) };
                    match from {
                        Some(from)
                            if from + n <= len
                                && text[from..from + n]
                                    .iter()
                                    .zip(s)
                                    .all(|(&a, &b)| (if f.ci { to_lower(a) } else { a }) == b) =>
                        {
                            pos = if f.rtl { from } else { from + n };
                            pc += 1;
                            true
                        }
                        _ => false,
                    }
                }
                Inst::Rep {
                    leaf,
                    f,
                    min,
                    max,
                    lazy,
                } => {
                    let mut p = pos;
                    let mut ok = true;
                    for _ in 0..*min {
                        match Self::step(text, p, leaf, *f) {
                            Some(q) => p = q,
                            None => {
                                ok = false;
                                break;
                            }
                        }
                    }
                    if ok {
                        let room = max - min;
                        if *lazy {
                            if room > 0 {
                                st.bt.push(Bt::RepLazy {
                                    pc,
                                    pos: p,
                                    left: room,
                                });
                            }
                            pos = p;
                        } else {
                            let mut count = 0u32;
                            let mut q = p;
                            while count < room {
                                match Self::step(text, q, leaf, *f) {
                                    Some(r) => {
                                        q = r;
                                        count += 1;
                                    }
                                    None => break,
                                }
                            }
                            if count > 0 {
                                st.bt.push(Bt::RepGreedy { pc, base: p, count });
                            }
                            pos = q;
                        }
                        pc += 1;
                    }
                    ok
                }
                Inst::Ref(c, f) => match st.caps[*c] {
                    Some((s, e)) => {
                        let n = e - s;
                        let from = if f.rtl { pos.checked_sub(n) } else { Some(pos) };
                        match from {
                            Some(from)
                                if from + n <= len
                                    && (0..n).all(|i| {
                                        Self::chars_eq(text[from + i], text[s + i], f.ci)
                                    }) =>
                            {
                                pos = if f.rtl { from } else { from + n };
                                pc += 1;
                                true
                            }
                            _ => false,
                        }
                    }
                    None => false,
                },
                Inst::Bol => {
                    let ok = pos == 0 || text[pos - 1] == '\n';
                    pc += 1;
                    ok
                }
                Inst::Eol => {
                    let ok = pos >= len || text[pos] == '\n';
                    pc += 1;
                    ok
                }
                Inst::Boundary | Inst::Nonboundary => {
                    let b = (pos > 0 && is_word_char(text[pos - 1]))
                        != (pos < len && is_word_char(text[pos]));
                    let ok = b == matches!(self.insts[pc], Inst::Boundary);
                    pc += 1;
                    ok
                }
                Inst::Beginning => {
                    pc += 1;
                    pos == 0
                }
                Inst::Start => {
                    pc += 1;
                    pos == textstart
                }
                Inst::EndZ => {
                    let right = len - pos;
                    pc += 1;
                    right == 0 || (right == 1 && text[pos] == '\n')
                }
                Inst::End => {
                    pc += 1;
                    pos >= len
                }
                Inst::Split(alt) => {
                    st.bt.push(Bt::Alt { pc: *alt, pos });
                    pc += 1;
                    true
                }
                Inst::Jmp(to) => {
                    pc = *to;
                    true
                }
                Inst::CapStart(c) => {
                    st.bt.push(Bt::CapStart {
                        idx: *c,
                        old: st.capstart[*c],
                    });
                    st.capstart[*c] = pos;
                    pc += 1;
                    true
                }
                Inst::CapEnd(c) => {
                    let s = st.capstart[*c];
                    let span = if s <= pos { (s, pos) } else { (pos, s) };
                    st.bt.push(Bt::Cap {
                        idx: *c,
                        old: st.caps[*c],
                    });
                    st.caps[*c] = Some(span);
                    pc += 1;
                    true
                }
                Inst::LoopEnter { reg, null, count } => {
                    st.bt.push(Bt::Loop {
                        reg: *reg,
                        mark: st.mark[*reg],
                        count: st.count[*reg],
                    });
                    st.mark[*reg] = if *null { -1 } else { ipos(pos) };
                    st.count[*reg] = *count;
                    pc += 1;
                    true
                }
                Inst::BranchMark { reg, body } => {
                    let m = st.mark[*reg];
                    if ipos(pos) != m {
                        st.bt.push(Bt::MarkBack { pc, old: m, pos });
                        st.mark[*reg] = ipos(pos);
                        pc = *body;
                    } else {
                        st.bt.push(Bt::Loop {
                            reg: *reg,
                            mark: m,
                            count: st.count[*reg],
                        });
                        pc += 1;
                    }
                    true
                }
                Inst::LazyBranchMark { reg, .. } => {
                    let old = st.mark[*reg];
                    if ipos(pos) != old {
                        let saved = if old == -1 { ipos(pos) } else { old };
                        st.bt.push(Bt::LazyMarkBack { pc, saved, pos });
                    } else {
                        st.bt.push(Bt::Loop {
                            reg: *reg,
                            mark: old,
                            count: st.count[*reg],
                        });
                    }
                    pc += 1;
                    true
                }
                Inst::BranchCount { reg, body, limit } => {
                    let (m, c) = (st.mark[*reg], st.count[*reg]);
                    let matched = ipos(pos) - m;
                    if c >= *limit || (matched == 0 && c >= 0) {
                        st.bt.push(Bt::Loop {
                            reg: *reg,
                            mark: m,
                            count: c,
                        });
                        pc += 1;
                    } else {
                        st.bt.push(Bt::CountBack { pc, old: m });
                        st.mark[*reg] = ipos(pos);
                        st.count[*reg] = c + 1;
                        pc = *body;
                    }
                    true
                }
                Inst::LazyBranchCount { reg, body, .. } => {
                    let (m, c) = (st.mark[*reg], st.count[*reg]);
                    if c < 0 {
                        st.bt.push(Bt::LazyCountBack2 { reg: *reg, old: m });
                        st.mark[*reg] = ipos(pos);
                        st.count[*reg] = c + 1;
                        pc = *body;
                    } else {
                        st.bt.push(Bt::LazyCountBack {
                            pc,
                            mark: m,
                            count: c,
                            pos,
                        });
                        pc += 1;
                    }
                    true
                }
                Inst::FrameStart(kind, resume) => {
                    st.frames.push(Frame {
                        height: st.bt.len(),
                        pos,
                        kind: *kind,
                        resume: *resume,
                    });
                    st.bt.push(Bt::Frame);
                    pc += 1;
                    true
                }
                Inst::FrameEnd => {
                    let frame = st.frames.pop().expect("frame");
                    // The body's choices go; what it changed stays restorable.
                    let tail = st.bt.split_off(frame.height);
                    st.bt
                        .extend(tail.into_iter().skip(1).filter(Bt::is_restore));
                    if matches!(frame.kind, FrameKind::Require | FrameKind::Cond) {
                        pos = frame.pos;
                    }
                    pc += 1;
                    true
                }
                Inst::PreventEnd => {
                    // The body matched: the negative assertion fails, its captures are undone.
                    let frame = st.frames.pop().expect("frame");
                    while st.bt.len() > frame.height + 1 {
                        let e = st.bt.pop().expect("entry");
                        st.restore(&e);
                    }
                    st.bt.pop();
                    false
                }
                Inst::TestRef(c, no) => {
                    if st.caps[*c].is_some() {
                        pc += 1;
                    } else {
                        pc = *no;
                    }
                    true
                }
                Inst::Match => return true,
            };
            if ok {
                continue;
            }
            // Backtrack.
            loop {
                steps += 1;
                if steps > STEP_LIMIT {
                    return false;
                }
                let Some(e) = st.bt.pop() else {
                    return false;
                };
                match e {
                    Bt::Alt { pc: p, pos: q } => {
                        pc = p;
                        pos = q;
                        break;
                    }
                    Bt::RepGreedy { pc: p, base, count } => {
                        let Inst::Rep { f, .. } = &self.insts[p] else {
                            unreachable!()
                        };
                        let count = count - 1;
                        pos = if f.rtl {
                            base - count as usize
                        } else {
                            base + count as usize
                        };
                        if count > 0 {
                            st.bt.push(Bt::RepGreedy { pc: p, base, count });
                        }
                        pc = p + 1;
                        break;
                    }
                    Bt::RepLazy {
                        pc: p,
                        pos: q,
                        left,
                    } => {
                        let Inst::Rep { leaf, f, .. } = &self.insts[p] else {
                            unreachable!()
                        };
                        if let Some(r) = Self::step(text, q, leaf, *f) {
                            if left > 1 {
                                st.bt.push(Bt::RepLazy {
                                    pc: p,
                                    pos: r,
                                    left: left - 1,
                                });
                            }
                            pos = r;
                            pc = p + 1;
                            break;
                        }
                    }
                    Bt::MarkBack { pc: p, old, pos: q } => {
                        let Inst::BranchMark { reg, .. } = &self.insts[p] else {
                            unreachable!()
                        };
                        st.bt.push(Bt::Loop {
                            reg: *reg,
                            mark: old,
                            count: st.count[*reg],
                        });
                        pos = q;
                        pc = p + 1;
                        break;
                    }
                    Bt::LazyMarkBack {
                        pc: p,
                        saved,
                        pos: q,
                    } => {
                        let Inst::LazyBranchMark { reg, body } = &self.insts[p] else {
                            unreachable!()
                        };
                        st.bt.push(Bt::Loop {
                            reg: *reg,
                            mark: saved,
                            count: st.count[*reg],
                        });
                        st.mark[*reg] = ipos(q);
                        pos = q;
                        pc = *body;
                        break;
                    }
                    Bt::CountBack { pc: p, old } => {
                        let Inst::BranchCount { reg, .. } = &self.insts[p] else {
                            unreachable!()
                        };
                        let (m, c) = (st.mark[*reg], st.count[*reg]);
                        if c > 0 {
                            st.bt.push(Bt::Loop {
                                reg: *reg,
                                mark: old,
                                count: c - 1,
                            });
                            pos = usize::try_from(m).unwrap_or(0);
                            pc = p + 1;
                            break;
                        }
                        st.mark[*reg] = old;
                        st.count[*reg] = c - 1;
                    }
                    Bt::LazyCountBack {
                        pc: p,
                        mark,
                        count,
                        pos: q,
                    } => {
                        let Inst::LazyBranchCount { reg, body, limit } = &self.insts[p] else {
                            unreachable!()
                        };
                        if count < *limit && ipos(q) != mark {
                            st.mark[*reg] = ipos(q);
                            st.count[*reg] = count + 1;
                            st.bt.push(Bt::LazyCountBack2 {
                                reg: *reg,
                                old: mark,
                            });
                            pos = q;
                            pc = *body;
                            break;
                        }
                        st.mark[*reg] = mark;
                        st.count[*reg] = count;
                    }
                    Bt::LazyCountBack2 { reg, old } => {
                        st.mark[reg] = old;
                        st.count[reg] -= 1;
                    }
                    Bt::Frame => {
                        let frame = st.frames.pop().expect("frame");
                        if matches!(frame.kind, FrameKind::Prevent | FrameKind::Cond) {
                            // The body failed: `(?!…)` holds; `(?(cond)…)` takes the no branch.
                            pos = frame.pos;
                            pc = frame.resume;
                            break;
                        }
                    }
                    e => st.restore(&e),
                }
            }
        }
    }
}
