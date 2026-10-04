//! Compiling the parse tree to a program: the first-character analysis and the code generator.

use super::*;

/// What can start a match of a node: whether it can match empty, and the first-character
/// tests (`None`: anything).
pub(super) struct First {
    pub(super) nullable: bool,
    pub(super) leaves: Option<Vec<(Leaf, bool)>>,
}

impl First {
    pub(super) fn empty() -> Self {
        Self {
            nullable: true,
            leaves: Some(Vec::new()),
        }
    }

    pub(super) fn any(nullable: bool) -> Self {
        Self {
            nullable,
            leaves: None,
        }
    }

    pub(super) fn leaf(leaf: Leaf, ci: bool) -> Self {
        Self {
            nullable: false,
            leaves: Some(vec![(leaf, ci)]),
        }
    }

    pub(super) fn union(&mut self, o: First) {
        self.leaves = match (self.leaves.take(), o.leaves) {
            (Some(mut a), Some(b)) => {
                a.extend(b);
                Some(a)
            }
            _ => None,
        };
    }
}

pub(super) fn first(node: &Node) -> First {
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

pub(super) struct Compiler<'t> {
    pub(super) insts: Vec<Inst>,
    pub(super) capnumlist: &'t [usize],
    pub(super) nreg: usize,
}

impl Compiler<'_> {
    pub(super) fn cap(&self, slot: usize) -> usize {
        self.capnumlist.binary_search(&slot).unwrap_or(0)
    }

    pub(super) fn emit(&mut self, i: Inst) -> usize {
        self.insts.push(i);
        self.insts.len() - 1
    }

    pub(super) fn patch(&mut self, at: usize, to: usize) {
        match &mut self.insts[at] {
            Inst::Split(t) | Inst::Jmp(t) | Inst::FrameStart(_, t) | Inst::TestRef(_, t) => *t = to,
            Inst::BranchMark { body, .. }
            | Inst::LazyBranchMark { body, .. }
            | Inst::BranchCount { body, .. }
            | Inst::LazyBranchCount { body, .. } => *body = to,
            _ => {}
        }
    }

    pub(super) fn leaf(node: &Node) -> Option<Leaf> {
        match &node.kind {
            Kind::One(c) => Some(Leaf::One(*c)),
            Kind::Notone(c) => Some(Leaf::Notone(*c)),
            Kind::Set(s) => Some(Leaf::Set(Box::new(s.clone()))),
            _ => None,
        }
    }

    pub(super) fn node(&mut self, node: &Node) {
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

    pub(super) fn frame(&mut self, kind: FrameKind, child: &Node) {
        self.emit(Inst::FrameStart(kind, 0));
        self.node(child);
        self.emit(Inst::FrameEnd);
    }

    /// A loop (regexp2's writer: `Setcount`/`Nullcount` + `Branchcount` when `max` is bounded
    /// or `min > 1`, else `Setmark`/`Nullmark` + `Branchmark`).
    pub(super) fn repeat(&mut self, child: &Node, min: u32, max: u32, lazy: bool, f: Flags) {
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
pub(super) fn starts_with_start(node: &Node) -> bool {
    match &node.kind {
        Kind::Start => true,
        Kind::Capture { child, .. } | Kind::Group(child) => starts_with_start(child),
        Kind::Alternate(c) if c.len() == 1 => starts_with_start(&c[0]),
        Kind::Concatenate(c) => c.first().is_some_and(starts_with_start),
        _ => false,
    }
}
