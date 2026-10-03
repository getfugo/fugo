//! The matcher: the parse tree compiled to a small program and run by a backtracking machine
//! with an explicit stack (no recursion, so long inputs cannot overflow the thread's stack).
//!
//! The backtracking order and the loop rules are those of regexp2's interpreter (`runner.go`,
//! regexp2 v1.11.5, MIT, a port of .NET's `RegexInterpreter`): alternatives in order; greedy
//! single-character loops give back one character at a time, lazy ones take one more; general
//! loops are `Branchmark`/`Lazybranchmark` loops for `*`, `+` (an empty iteration ends the
//! loop) and `Branchcount`/`Lazybranchcount` loops for `?` and `{m,n}`, with the same
//! mark/count bookkeeping; atomic groups and look-around do not backtrack into their body, keep
//! the captures of a successful positive body and drop those of a negative one; captures and
//! loop state are restored when backtracking past them.

use super::Groups;
use super::Options;
use super::charclass::{CharSet, is_word_char, to_lower};
use super::parser::{INFINITE, Kind, Node, Tree};

/// Gives up on a match after this many steps (regexp2 gives up after 250 ms; Chroma treats a
/// timed-out rule as not matching).
const STEP_LIMIT: u64 = 50_000_000;

/// One character test.
#[derive(Clone, Debug)]
enum Leaf {
    One(char),
    Notone(char),
    Set(Box<CharSet>),
}

impl Leaf {
    fn matches(&self, ch: char, ci: bool) -> bool {
        let c = if ci { to_lower(ch) } else { ch };
        match self {
            Leaf::One(x) => c == *x,
            Leaf::Notone(x) => c != *x,
            Leaf::Set(s) => s.contains(c),
        }
    }
}

/// Case-insensitivity and direction of a node.
#[derive(Clone, Copy, Debug)]
struct Flags {
    ci: bool,
    rtl: bool,
}

impl Flags {
    fn of(o: Options) -> Self {
        Self {
            ci: o.contains(Options::IGNORE_CASE),
            rtl: o.contains(Options::RIGHT_TO_LEFT),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrameKind {
    /// `(?>…)`.
    Atomic,
    /// `(?=…)`, `(?<=…)`.
    Require,
    /// `(?!…)`, `(?<!…)`.
    Prevent,
    /// The condition of `(?(expr)yes|no)`.
    Cond,
}

#[derive(Clone, Debug)]
enum Inst {
    Leaf(Leaf, Flags),
    Multi(Vec<char>, Flags),
    /// A single-character loop (`Onerep` + `Oneloop`/`Onelazy` and friends).
    Rep {
        leaf: Leaf,
        f: Flags,
        min: u32,
        max: u32,
        lazy: bool,
    },
    Ref(usize, Flags),
    Bol,
    Eol,
    Boundary,
    Nonboundary,
    Beginning,
    Start,
    EndZ,
    End,
    /// Try the next instruction, then the one given.
    Split(usize),
    Jmp(usize),
    CapStart(usize),
    CapEnd(usize),
    LoopEnter {
        reg: usize,
        null: bool,
        count: i64,
    },
    BranchMark {
        reg: usize,
        body: usize,
    },
    LazyBranchMark {
        reg: usize,
        body: usize,
    },
    BranchCount {
        reg: usize,
        body: usize,
        limit: i64,
    },
    LazyBranchCount {
        reg: usize,
        body: usize,
        limit: i64,
    },
    FrameStart(FrameKind, usize),
    FrameEnd,
    PreventEnd,
    TestRef(usize, usize),
    Match,
}

/// A compiled pattern.
#[derive(Debug)]
pub(super) struct Program {
    insts: Vec<Inst>,
    /// Number of capture groups (dense, by group number).
    ncap: usize,
    /// Number of loop registers.
    nreg: usize,
    /// The pattern starts with `\G`: it can only match where the search starts.
    anchored: bool,
    /// The characters a match can start with (`None`: any, or the pattern can match empty).
    first: Option<Vec<(Leaf, bool)>>,
}

/// What can start a match of a node: whether it can match empty, and the first-character
/// tests (`None`: anything).
struct First {
    nullable: bool,
    leaves: Option<Vec<(Leaf, bool)>>,
}

impl First {
    fn empty() -> Self {
        Self {
            nullable: true,
            leaves: Some(Vec::new()),
        }
    }

    fn any(nullable: bool) -> Self {
        Self {
            nullable,
            leaves: None,
        }
    }

    fn leaf(leaf: Leaf, ci: bool) -> Self {
        Self {
            nullable: false,
            leaves: Some(vec![(leaf, ci)]),
        }
    }

    fn union(&mut self, o: First) {
        self.leaves = match (self.leaves.take(), o.leaves) {
            (Some(mut a), Some(b)) => {
                a.extend(b);
                Some(a)
            }
            _ => None,
        };
    }
}

fn first(node: &Node) -> First {
    let ci = node.opts.contains(Options::IGNORE_CASE);
    match &node.kind {
        Kind::One(c) => First::leaf(Leaf::One(*c), ci),
        Kind::Notone(c) => First::leaf(Leaf::Notone(*c), ci),
        Kind::Set(s) => First::leaf(Leaf::Set(Box::new(s.clone())), ci),
        Kind::Multi(s) => match s.first() {
            Some(c) => First::leaf(Leaf::One(*c), ci),
            None => First::empty(),
        },
        Kind::Ref(_) | Kind::Testref { .. } | Kind::Testgroup(_) => First::any(true),
        Kind::Bol
        | Kind::Eol
        | Kind::Boundary
        | Kind::Nonboundary
        | Kind::Beginning
        | Kind::Start
        | Kind::EndZ
        | Kind::End
        | Kind::Empty
        | Kind::Require(_)
        | Kind::Prevent(_) => First::empty(),
        Kind::Alternate(children) => {
            let mut out = First {
                nullable: false,
                leaves: Some(Vec::new()),
            };
            for c in children {
                let f = first(c);
                out.nullable |= f.nullable;
                out.union(f);
            }
            out
        }
        Kind::Concatenate(children) => {
            let mut out = First::empty();
            for c in children {
                let f = first(c);
                let nullable = f.nullable;
                out.union(f);
                if !nullable {
                    out.nullable = false;
                    return out;
                }
            }
            out
        }
        Kind::Loop { child, min, .. } => {
            let mut f = first(child);
            f.nullable |= *min == 0;
            f
        }
        Kind::Capture { child, .. } | Kind::Group(child) | Kind::Greedy(child) => first(child),
    }
}

struct Compiler<'t> {
    insts: Vec<Inst>,
    capnumlist: &'t [usize],
    nreg: usize,
}

impl Compiler<'_> {
    fn cap(&self, slot: usize) -> usize {
        self.capnumlist.binary_search(&slot).unwrap_or(0)
    }

    fn emit(&mut self, i: Inst) -> usize {
        self.insts.push(i);
        self.insts.len() - 1
    }

    fn patch(&mut self, at: usize, to: usize) {
        match &mut self.insts[at] {
            Inst::Split(t) | Inst::Jmp(t) | Inst::FrameStart(_, t) | Inst::TestRef(_, t) => *t = to,
            Inst::BranchMark { body, .. }
            | Inst::LazyBranchMark { body, .. }
            | Inst::BranchCount { body, .. }
            | Inst::LazyBranchCount { body, .. } => *body = to,
            _ => {}
        }
    }

    fn leaf(node: &Node) -> Option<Leaf> {
        match &node.kind {
            Kind::One(c) => Some(Leaf::One(*c)),
            Kind::Notone(c) => Some(Leaf::Notone(*c)),
            Kind::Set(s) => Some(Leaf::Set(Box::new(s.clone()))),
            _ => None,
        }
    }

    fn node(&mut self, node: &Node) {
        let f = Flags::of(node.opts);
        match &node.kind {
            Kind::One(_) | Kind::Notone(_) | Kind::Set(_) => {
                let leaf = Self::leaf(node).expect("leaf");
                self.emit(Inst::Leaf(leaf, f));
            }
            Kind::Multi(s) => {
                self.emit(Inst::Multi(s.clone(), f));
            }
            Kind::Ref(slot) => {
                let c = self.cap(*slot);
                self.emit(Inst::Ref(c, f));
            }
            Kind::Bol => {
                self.emit(Inst::Bol);
            }
            Kind::Eol => {
                self.emit(Inst::Eol);
            }
            Kind::Boundary => {
                self.emit(Inst::Boundary);
            }
            Kind::Nonboundary => {
                self.emit(Inst::Nonboundary);
            }
            Kind::Beginning => {
                self.emit(Inst::Beginning);
            }
            Kind::Start => {
                self.emit(Inst::Start);
            }
            Kind::EndZ => {
                self.emit(Inst::EndZ);
            }
            Kind::End => {
                self.emit(Inst::End);
            }
            Kind::Empty => {}
            Kind::Concatenate(children) => {
                for c in children {
                    self.node(c);
                }
            }
            Kind::Alternate(children) => {
                let mut jumps = Vec::new();
                for (i, c) in children.iter().enumerate() {
                    if i + 1 < children.len() {
                        let split = self.emit(Inst::Split(0));
                        self.node(c);
                        jumps.push(self.emit(Inst::Jmp(0)));
                        let next = self.insts.len();
                        self.patch(split, next);
                    } else {
                        self.node(c);
                    }
                }
                let end = self.insts.len();
                for j in jumps {
                    self.patch(j, end);
                }
            }
            Kind::Loop {
                child,
                min,
                max,
                lazy,
            } => self.repeat(child, *min, *max, *lazy, f),
            Kind::Capture { index, child } => {
                let c = self.cap(*index);
                self.emit(Inst::CapStart(c));
                self.node(child);
                self.emit(Inst::CapEnd(c));
            }
            Kind::Group(child) => self.node(child),
            Kind::Greedy(child) => self.frame(FrameKind::Atomic, child),
            Kind::Require(child) => self.frame(FrameKind::Require, child),
            Kind::Prevent(child) => {
                let start = self.emit(Inst::FrameStart(FrameKind::Prevent, 0));
                self.node(child);
                self.emit(Inst::PreventEnd);
                let end = self.insts.len();
                self.patch(start, end);
            }
            Kind::Testref { index, children } => {
                let c = self.cap(*index);
                let test = self.emit(Inst::TestRef(c, 0));
                if let Some(yes) = children.first() {
                    self.node(yes);
                }
                let jmp = self.emit(Inst::Jmp(0));
                let no = self.insts.len();
                self.patch(test, no);
                if let Some(n) = children.get(1) {
                    self.node(n);
                }
                let end = self.insts.len();
                self.patch(jmp, end);
            }
            Kind::Testgroup(children) => {
                let start = self.emit(Inst::FrameStart(FrameKind::Cond, 0));
                if let Some(cond) = children.first() {
                    self.node(cond);
                }
                self.emit(Inst::FrameEnd);
                if let Some(yes) = children.get(1) {
                    self.node(yes);
                }
                let jmp = self.emit(Inst::Jmp(0));
                let no = self.insts.len();
                self.patch(start, no);
                if let Some(n) = children.get(2) {
                    self.node(n);
                }
                let end = self.insts.len();
                self.patch(jmp, end);
            }
        }
    }

    fn frame(&mut self, kind: FrameKind, child: &Node) {
        self.emit(Inst::FrameStart(kind, 0));
        self.node(child);
        self.emit(Inst::FrameEnd);
    }

    /// A loop (regexp2's writer: `Setcount`/`Nullcount` + `Branchcount` when `max` is bounded
    /// or `min > 1`, else `Setmark`/`Nullmark` + `Branchmark`).
    fn repeat(&mut self, child: &Node, min: u32, max: u32, lazy: bool, f: Flags) {
        if let Some(leaf) = Self::leaf(child) {
            let f = Flags::of(child.opts);
            self.emit(Inst::Rep {
                leaf,
                f,
                min,
                max,
                lazy,
            });
            return;
        }
        let _ = f;
        let reg = self.nreg;
        self.nreg += 1;
        let counted = max < INFINITE || min > 1;
        let count = if min == 0 { 0 } else { 1 - i64::from(min) };
        self.emit(Inst::LoopEnter {
            reg,
            null: min == 0,
            count,
        });
        let jmp = (min == 0).then(|| self.emit(Inst::Jmp(0)));
        let body = self.insts.len();
        self.node(child);
        let limit = if max == INFINITE {
            i64::from(INFINITE)
        } else {
            i64::from(max) - i64::from(min)
        };
        let branch = self.emit(match (counted, lazy) {
            (false, false) => Inst::BranchMark { reg, body },
            (false, true) => Inst::LazyBranchMark { reg, body },
            (true, false) => Inst::BranchCount { reg, body, limit },
            (true, true) => Inst::LazyBranchCount { reg, body, limit },
        });
        if let Some(j) = jmp {
            self.patch(j, branch);
        }
    }
}

/// Whether the pattern (`Capture(0)` → alternation → concatenation) starts with `\G`.
fn starts_with_start(node: &Node) -> bool {
    match &node.kind {
        Kind::Start => true,
        Kind::Capture { child, .. } | Kind::Group(child) => starts_with_start(child),
        Kind::Alternate(c) if c.len() == 1 => starts_with_start(&c[0]),
        Kind::Concatenate(c) => c.first().is_some_and(starts_with_start),
        _ => false,
    }
}

/// A backtracking entry: a choice point to resume, or state to restore.
#[derive(Clone, Debug)]
enum Bt {
    Alt {
        pc: usize,
        pos: usize,
    },
    RepGreedy {
        pc: usize,
        base: usize,
        count: u32,
    },
    RepLazy {
        pc: usize,
        pos: usize,
        left: u32,
    },
    MarkBack {
        pc: usize,
        old: i64,
        pos: usize,
    },
    LazyMarkBack {
        pc: usize,
        saved: i64,
        pos: usize,
    },
    CountBack {
        pc: usize,
        old: i64,
    },
    LazyCountBack {
        pc: usize,
        mark: i64,
        count: i64,
        pos: usize,
    },
    LazyCountBack2 {
        reg: usize,
        old: i64,
    },
    Frame,
    // Restores.
    Cap {
        idx: usize,
        old: Option<(usize, usize)>,
    },
    CapStart {
        idx: usize,
        old: usize,
    },
    Loop {
        reg: usize,
        mark: i64,
        count: i64,
    },
}

impl Bt {
    fn is_restore(&self) -> bool {
        matches!(self, Bt::Cap { .. } | Bt::CapStart { .. } | Bt::Loop { .. })
    }
}

#[derive(Debug)]
struct Frame {
    height: usize,
    pos: usize,
    kind: FrameKind,
    resume: usize,
}

/// The machine's registers and stacks, reused across matches (a lexer keeps one per
/// tokenisation).
#[derive(Debug, Default)]
pub(crate) struct State {
    caps: Vec<Option<(usize, usize)>>,
    capstart: Vec<usize>,
    mark: Vec<i64>,
    count: Vec<i64>,
    bt: Vec<Bt>,
    frames: Vec<Frame>,
}

impl State {
    fn restore(&mut self, e: &Bt) {
        match *e {
            Bt::Cap { idx, old } => self.caps[idx] = old,
            Bt::CapStart { idx, old } => self.capstart[idx] = old,
            Bt::Loop { reg, mark, count } => {
                self.mark[reg] = mark;
                self.count[reg] = count;
            }
            _ => {}
        }
    }
}

#[expect(clippy::cast_possible_wrap, reason = "text positions fit in i64")]
fn ipos(p: usize) -> i64 {
    p as i64
}

impl Program {
    pub fn compile(tree: &Tree) -> Self {
        let mut c = Compiler {
            insts: Vec::new(),
            capnumlist: &tree.capnumlist,
            nreg: 0,
        };
        c.node(&tree.root);
        c.emit(Inst::Match);
        let f = first(&tree.root);
        Self {
            insts: c.insts,
            ncap: tree.capnumlist.len(),
            nreg: c.nreg,
            anchored: starts_with_start(&tree.root),
            first: if f.nullable { None } else { f.leaves },
        }
    }

    pub fn find(&self, text: &[char], start: usize, st: &mut State) -> Option<Groups> {
        let mut at = start;
        loop {
            if at > text.len() {
                return None;
            }
            if self.may_start(text, at) && self.run(text, start, at, st) {
                return Some(st.caps.clone());
            }
            if self.anchored {
                return None;
            }
            at += 1;
        }
    }

    fn may_start(&self, text: &[char], at: usize) -> bool {
        match &self.first {
            None => true,
            Some(leaves) => text
                .get(at)
                .is_some_and(|&ch| leaves.iter().any(|(l, ci)| l.matches(ch, *ci))),
        }
    }

    /// One step of `leaf` from `pos` in its direction.
    fn step(text: &[char], pos: usize, leaf: &Leaf, f: Flags) -> Option<usize> {
        if f.rtl {
            (pos > 0 && leaf.matches(text[pos - 1], f.ci)).then(|| pos - 1)
        } else {
            (pos < text.len() && leaf.matches(text[pos], f.ci)).then(|| pos + 1)
        }
    }

    fn chars_eq(a: char, b: char, ci: bool) -> bool {
        if ci {
            to_lower(a) == to_lower(b)
        } else {
            a == b
        }
    }

    #[expect(clippy::too_many_lines, reason = "the interpreter's dispatch")]
    fn run(&self, text: &[char], textstart: usize, at: usize, st: &mut State) -> bool {
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
