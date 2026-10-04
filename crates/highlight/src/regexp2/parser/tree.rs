//! Tree building: the group stack, alternation and concatenation.

use super::*;

impl Parser<'_> {
    pub(super) fn start_group(&mut self, group: Group) {
        self.group = Some(group);
        self.alternation = (Vec::new(), self.options);
        self.concatenation = (Vec::new(), self.options);
    }

    pub(super) fn push_group(&mut self) {
        let group = self.group.take().expect("group");
        self.stack.push(Frame {
            group,
            alternation: std::mem::take(&mut self.alternation),
            concatenation: std::mem::take(&mut self.concatenation),
        });
    }

    pub(super) fn pop_group(&mut self) -> Result<(), Error> {
        let frame = self.stack.pop().expect("stack");
        self.group = Some(frame.group);
        self.alternation = frame.alternation;
        self.concatenation = frame.concatenation;
        let group = self.group.as_mut().expect("group");
        if matches!(group.kind, GroupKind::Testgroup) && group.children.is_empty() {
            let Some(unit) = self.unit.take() else {
                return Err(self.err("illegal conditional (?(...)) expression"));
            };
            self.group.as_mut().expect("group").children.push(unit);
        }
        Ok(())
    }

    /// The finished concatenation, reversed for right-to-left matching (`reverseLeft`).
    pub(super) fn take_concatenation(&mut self) -> Node {
        let (mut children, opts) =
            std::mem::replace(&mut self.concatenation, (Vec::new(), self.options));
        if opts.contains(Options::RIGHT_TO_LEFT) {
            children.reverse();
        }
        Node::new(Kind::Concatenate(children), opts)
    }

    pub(super) fn add_group(&mut self) -> Result<(), Error> {
        let concat = self.take_concatenation();
        let mut group = self.group.take().expect("group");
        match group.kind {
            GroupKind::Testgroup | GroupKind::Testref(_) => {
                group.children.push(concat);
                let max = if matches!(group.kind, GroupKind::Testref(_)) {
                    2
                } else {
                    3
                };
                if group.children.len() > max {
                    return Err(self.err("too many | in (?()|)"));
                }
            }
            _ => {
                let (mut alts, opts) = std::mem::take(&mut self.alternation);
                alts.push(concat);
                group.children.push(Node::new(Kind::Alternate(alts), opts));
            }
        }
        let opts = group.opts;
        let mut children = group.children;
        let only = |children: &mut Vec<Node>| Box::new(children.pop().expect("child"));
        let kind = match group.kind {
            GroupKind::Capture(index) => Kind::Capture {
                index,
                child: only(&mut children),
            },
            GroupKind::Group => Kind::Group(only(&mut children)),
            GroupKind::Require => Kind::Require(only(&mut children)),
            GroupKind::Prevent => Kind::Prevent(only(&mut children)),
            GroupKind::Greedy => Kind::Greedy(only(&mut children)),
            GroupKind::Testref(index) => Kind::Testref { index, children },
            GroupKind::Testgroup => Kind::Testgroup(children),
        };
        self.unit = Some(Node::new(kind, opts));
        Ok(())
    }

    pub(super) fn add_alternate(&mut self) {
        let concat = self.take_concatenation();
        let group = self.group.as_mut().expect("group");
        if matches!(group.kind, GroupKind::Testgroup | GroupKind::Testref(_)) {
            group.children.push(concat);
        } else {
            self.alternation.0.push(concat);
        }
    }

    pub(super) fn add_concatenate(&mut self) {
        if let Some(unit) = self.unit.take() {
            self.concatenation.0.push(unit);
        }
    }

    pub(super) fn add_concatenate_quantified(&mut self, lazy: bool, min: u32, max: u32) {
        if let Some(unit) = self.unit.take() {
            let q = make_quantifier(unit, lazy, min, max);
            self.concatenation.0.push(q);
        }
    }

    pub(super) fn add_unit_one(&mut self, mut ch: char) {
        if self.use_i() {
            ch = to_lower(ch);
        }
        self.unit = Some(Node::new(Kind::One(ch), self.options));
    }

    pub(super) fn add_unit_notone(&mut self, mut ch: char) {
        if self.use_i() {
            ch = to_lower(ch);
        }
        self.unit = Some(Node::new(Kind::Notone(ch), self.options));
    }

    pub(super) fn add_unit_set(&mut self, set: CharSet) {
        self.unit = Some(set_node(set, self.options));
    }

    pub(super) fn add_unit_type(&mut self, kind: Kind) {
        self.unit = Some(Node::new(kind, self.options));
    }

    /// Adds the literal run `pattern[pos..pos+n]` (`addToConcatenate`).
    pub(super) fn add_to_concatenate(&mut self, pos: usize, n: usize) {
        if n == 0 {
            return;
        }
        let i = self.use_i();
        let node = if n > 1 {
            let s = self.pattern[pos..pos + n]
                .iter()
                .map(|&c| if i { to_lower(c) } else { c })
                .collect();
            Node::new(Kind::Multi(s), self.options)
        } else {
            let c = self.pattern[pos];
            Node::new(Kind::One(if i { to_lower(c) } else { c }), self.options)
        };
        self.concatenation.0.push(node);
    }
}
