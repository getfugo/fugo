//! Members that move out of the class body: fields, private brands and static blocks, and computed
//! keys.

use super::*;

/// Where a lowered field's value is stored.
pub(super) enum FieldTarget<'a> {
    /// A private field, in this `WeakMap`.
    Private(&'a str),
    /// A public field, under this key.
    Public(Expression<'a>),
}

impl<'a> Lowerer<'a, '_> {
    /// Prepends `e` to the key of an element: `[key]` → `[(e, key)]`.
    pub(super) fn prepend_to_key(&mut self, el: &mut ClassElement<'a>, e: Expression<'a>) {
        let key = self.take_key_expr(el);
        let joined = self.seq(vec![e, key]);
        if let Some(k) = element_key_mut(el) {
            *k = PropertyKey::from(joined);
        }
        if let Some(c) = element_computed_mut(el) {
            *c = true;
        }
    }

    /// Takes the key of an element as an expression (a name becomes a string).
    pub(super) fn take_key_expr(&mut self, el: &mut ClassElement<'a>) -> Expression<'a> {
        let Some(key) = element_key_mut(el) else {
            return self.void0();
        };
        let dummy = PropertyKey::from(self.void0());
        match std::mem::replace(key, dummy) {
            PropertyKey::StaticIdentifier(id) => self.string(id.name.as_str()),
            PropertyKey::PrivateIdentifier(p) => {
                *key = PropertyKey::PrivateIdentifier(p);
                self.void0()
            }
            other => other.into_expression(),
        }
    }

    /// `lowerField`: a field's initialization moves into the constructor (instance) or after the
    /// class (static), with the decorators' initializers around it.
    #[expect(clippy::too_many_arguments)]
    pub(super) fn lower_field(
        &mut self,
        out: &mut ClassOut<'a>,
        plan: &ClassPlan<'a>,
        is_static: bool,
        target: FieldTarget<'a>,
        value: Option<Expression<'a>>,
        slot: Option<usize>,
        span: Span,
    ) {
        let init = out.init_ref.unwrap_or("_init");
        let this_like = |s: &Self| {
            if is_static {
                s.ident(plan.class_ref)
            } else {
                s.this()
            }
        };
        let mut value = value;
        if let Some(slot) = slot {
            let flags = f64::from(u32::try_from((4 + 2 * slot) << 1).unwrap_or(u32::MAX));
            let mut args = vec![self.ident(init), self.number(flags), this_like(self)];
            args.extend(value.take());
            value = Some(self.call_helper(Helper::RunInitializers, args));
        }
        let mut member = match target {
            FieldTarget::Private(map) => {
                let weak = self.new_weak("WeakMap", span);
                let created = self.assign(map, weak);
                out.private_members.push(created);
                let mut args = vec![this_like(self), self.ident(map)];
                args.extend(value);
                self.call_helper(Helper::PrivateAdd, args)
            }
            FieldTarget::Public(key) => {
                let mut args = vec![this_like(self), key];
                args.extend(value);
                self.call_helper(Helper::PublicField, args)
            }
        };
        if let Some(slot) = slot {
            let flags = f64::from(u32::try_from(((5 + 2 * slot) << 1) | 1).unwrap_or(u32::MAX));
            let extra = self.call_helper(
                Helper::RunInitializers,
                vec![self.ident(init), self.number(flags), this_like(self)],
            );
            member = self.seq(vec![member, extra]);
        }
        if is_static {
            out.static_members.push(member);
        } else {
            out.instance_members.push(self.expr_stmt(member));
        }
    }

    /// Registers the `WeakSet` of the instances having the class's private methods.
    pub(super) fn add_brand(
        &mut self,
        out: &mut ClassOut<'a>,
        plan: &ClassPlan<'a>,
        is_static: bool,
    ) {
        let (added, brand) = if is_static {
            (&mut out.brands_added.r#static, plan.static_brand)
        } else {
            (&mut out.brands_added.instance, plan.instance_brand)
        };
        let Some(brand) = brand else { return };
        if *added {
            return;
        }
        *added = true;
        let weak = self.new_weak("WeakSet", SPAN);
        let created = self.assign(brand, weak);
        out.private_members.push(created);
        let target = if is_static {
            self.ident(plan.class_ref)
        } else {
            self.this()
        };
        let add = self.call_helper(Helper::PrivateAdd, vec![target, self.ident(brand)]);
        if is_static {
            out.static_private_methods.push(add);
        } else {
            out.instance_private_methods.push(self.expr_stmt(add));
        }
    }

    /// `lowerStaticBlock`: its statements run after the class, inline when they are all
    /// expressions, else in an arrow function.
    pub(super) fn lower_static_block(
        &mut self,
        out: &mut ClassOut<'a>,
        stmts: ArenaVec<'a, Statement<'a>>,
    ) {
        let all_exprs = stmts.iter().all(|s| {
            matches!(
                s,
                Statement::ExpressionStatement(_) | Statement::EmptyStatement(_)
            )
        });
        if all_exprs {
            for s in stmts {
                if let Statement::ExpressionStatement(e) = s {
                    out.static_members.push(e.unbox().expression);
                }
            }
        } else {
            out.static_members.push(self.arrow_iife(stmts));
        }
    }
}
