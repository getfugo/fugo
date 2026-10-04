//! Optional chains through lowered private names (`a?.#b.c`).

use super::*;

/// A link of an optional chain being lowered.
pub(super) struct Link<'a> {
    pub(super) optional: bool,
    pub(super) kind: LinkKind<'a>,
}

pub(super) enum LinkKind<'a> {
    Static(IdentifierName<'a>),
    Computed(Expression<'a>),
    Private(PrivateIdentifier<'a>),
    Call(ArenaVec<'a, Argument<'a>>),
}

impl<'a> Lowerer<'a, '_> {
    /// Whether an optional chain accesses a lowered private name (and must be lowered whole).
    pub(super) fn chain_has_lowered_private(&self, e: &ChainElement<'a>) -> bool {
        let mut cur: &Expression<'a> = match e {
            ChainElement::CallExpression(c) => &c.callee,
            ChainElement::TSNonNullExpression(n) => &n.expression,
            ChainElement::PrivateFieldExpression(p) => {
                if self.lookup_private(p.field.name.as_str()).is_some() {
                    return true;
                }
                &p.object
            }
            ChainElement::StaticMemberExpression(m) => &m.object,
            ChainElement::ComputedMemberExpression(m) => &m.object,
        };
        loop {
            cur = match cur {
                Expression::PrivateFieldExpression(p) => {
                    if self.lookup_private(p.field.name.as_str()).is_some() {
                        return true;
                    }
                    &p.object
                }
                Expression::StaticMemberExpression(m) => &m.object,
                Expression::ComputedMemberExpression(m) => &m.object,
                Expression::CallExpression(c) => &c.callee,
                Expression::TSNonNullExpression(n) => &n.expression,
                _ => return false,
            };
        }
    }

    /// Lowers an optional chain with a lowered private name to conditionals
    /// (`a?.#x` → `(_a = a) == null ? void 0 : __privateGet(_a, _x)`).
    pub(super) fn lower_chain(&mut self, it: &mut Expression<'a>) {
        let Expression::ChainExpression(chain) = it.take_in(&self.b) else {
            return;
        };
        let mut cur: Expression<'a> = match chain.unbox().expression {
            ChainElement::CallExpression(c) => Expression::CallExpression(c),
            ChainElement::TSNonNullExpression(n) => Expression::TSNonNullExpression(n),
            ChainElement::PrivateFieldExpression(p) => Expression::PrivateFieldExpression(p),
            ChainElement::StaticMemberExpression(m) => Expression::StaticMemberExpression(m),
            ChainElement::ComputedMemberExpression(m) => Expression::ComputedMemberExpression(m),
        };
        let mut links = VecDeque::new();
        let mut base = loop {
            cur = match cur {
                Expression::StaticMemberExpression(m) => {
                    let m = m.unbox();
                    links.push_front(Link {
                        optional: m.optional,
                        kind: LinkKind::Static(m.property),
                    });
                    m.object
                }
                Expression::ComputedMemberExpression(m) => {
                    let m = m.unbox();
                    links.push_front(Link {
                        optional: m.optional,
                        kind: LinkKind::Computed(m.expression),
                    });
                    m.object
                }
                Expression::PrivateFieldExpression(m) => {
                    let m = m.unbox();
                    links.push_front(Link {
                        optional: m.optional,
                        kind: LinkKind::Private(m.field),
                    });
                    m.object
                }
                Expression::CallExpression(c) => {
                    let c = c.unbox();
                    links.push_front(Link {
                        optional: c.optional,
                        kind: LinkKind::Call(c.arguments),
                    });
                    c.callee
                }
                Expression::TSNonNullExpression(n) => n.unbox().expression,
                other => break other,
            };
        };
        self.visit_expression(&mut base);
        for link in &mut links {
            match &mut link.kind {
                LinkKind::Computed(e) => self.visit_expression(e),
                LinkKind::Call(args) => self.visit_arguments(args),
                LinkKind::Static(_) | LinkKind::Private(_) => {}
            }
        }
        *it = self.build_chain(base, None, links);
    }

    pub(super) fn build_chain(
        &mut self,
        mut cur: Expression<'a>,
        mut this_val: Option<Expression<'a>>,
        mut links: VecDeque<Link<'a>>,
    ) -> Expression<'a> {
        while let Some(mut link) = links.pop_front() {
            if link.optional {
                link.optional = false;
                let (check, reuse) = self.capture(cur);
                links.push_front(link);
                let rest = self.build_chain(reuse, this_val, links);
                let test = Expression::new_binary_expression(
                    SPAN,
                    check,
                    BinaryOperator::Equality,
                    self.null(),
                    &self.b,
                );
                return Expression::new_conditional_expression(
                    SPAN,
                    test,
                    self.void0(),
                    rest,
                    &self.b,
                );
            }
            let next_call = links.front().and_then(|l| match l.kind {
                LinkKind::Call(_) => Some(l.optional),
                _ => None,
            });
            match link.kind {
                LinkKind::Call(args) => {
                    cur = match this_val.take() {
                        Some(this) => {
                            let callee = self.member(cur, "call");
                            let mut all = ArenaVec::with_capacity_in(args.len() + 1, &self.b);
                            all.push(Argument::from(this));
                            all.extend(args);
                            Expression::new_call_expression(SPAN, callee, None, all, false, &self.b)
                        }
                        None => {
                            Expression::new_call_expression(SPAN, cur, None, args, false, &self.b)
                        }
                    };
                }
                kind => {
                    let lowered = match &kind {
                        LinkKind::Private(p) => self.lookup_private(p.name.as_str()),
                        _ => None,
                    };
                    let needs_this =
                        next_call.is_some_and(|optional| optional || lowered.is_some());
                    let obj = if needs_this {
                        let (obj, this) = self.capture(cur);
                        this_val = Some(this);
                        obj
                    } else {
                        this_val = None;
                        cur
                    };
                    cur = match kind {
                        LinkKind::Static(name) => Expression::new_static_member_expression(
                            SPAN, obj, name, false, &self.b,
                        ),
                        LinkKind::Computed(key) => Expression::new_computed_member_expression(
                            SPAN, obj, key, false, &self.b,
                        ),
                        LinkKind::Private(field) => match lowered {
                            Some(l) => self.private_read(obj, l),
                            None => Expression::new_private_field_expression(
                                SPAN, obj, field, false, &self.b,
                            ),
                        },
                        LinkKind::Call(_) => obj,
                    };
                }
            }
        }
        cur
    }
}
