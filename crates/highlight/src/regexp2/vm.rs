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

mod compile;
mod run;
use compile::*;

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
}
