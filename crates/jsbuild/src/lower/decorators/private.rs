//! Private names, `this` and `super` in code that moves: reads, writes and assignments rewritten to
//! the `WeakMap`/`WeakSet` helpers.

use super::*;

/// How a lowered private name is stored (esbuild's `privateGetters`/`privateSetters` symbols).
#[derive(Clone, Copy, Debug)]
pub(super) enum PrivateLowering<'a> {
    /// A field: its values live in this `WeakMap`.
    Field { map: &'a str },
    /// A method: instances are in the `brand` `WeakSet`, the function in `func`.
    Method { brand: &'a str, func: &'a str },
    /// A getter and/or setter (or an auto-accessor): instances are in `brand`.
    Accessor {
        brand: &'a str,
        get: Option<&'a str>,
        set: Option<&'a str>,
    },
}

impl<'a> PrivateLowering<'a> {
    /// The `WeakMap`/`WeakSet` of the instances that have the member.
    pub(super) fn member(self) -> &'a str {
        match self {
            Self::Field { map } => map,
            Self::Method { brand, .. } | Self::Accessor { brand, .. } => brand,
        }
    }

    pub(super) fn setter(self) -> Option<&'a str> {
        match self {
            Self::Accessor { set, .. } => set,
            _ => None,
        }
    }

    pub(super) fn getter(self) -> Option<&'a str> {
        match self {
            Self::Accessor { get, .. } => get,
            _ => None,
        }
    }
}

impl<'a> Lowerer<'a, '_> {
    pub(super) fn lookup_private(&self, name: &str) -> Option<PrivateLowering<'a>> {
        for scope in self.classes.iter().rev() {
            if let Some(l) = scope.privates.get(name) {
                return *l;
            }
        }
        None
    }

    pub(super) fn private_read(
        &mut self,
        obj: Expression<'a>,
        l: PrivateLowering<'a>,
    ) -> Expression<'a> {
        match l {
            PrivateLowering::Field { map } => {
                let map = self.ident(map);
                self.call_helper(Helper::PrivateGet, vec![obj, map])
            }
            PrivateLowering::Method { brand, func } => {
                let (brand, func) = (self.ident(brand), self.ident(func));
                self.call_helper(Helper::PrivateMethod, vec![obj, brand, func])
            }
            PrivateLowering::Accessor { brand, get, .. } => {
                let mut args = vec![obj, self.ident(brand)];
                if let Some(get) = get {
                    args.push(self.ident(get));
                }
                self.call_helper(Helper::PrivateGet, args)
            }
        }
    }

    pub(super) fn private_write(
        &mut self,
        obj: Expression<'a>,
        l: PrivateLowering<'a>,
        value: Expression<'a>,
    ) -> Expression<'a> {
        let mut args = vec![obj, self.ident(l.member()), value];
        if let Some(set) = l.setter() {
            args.push(self.ident(set));
        }
        self.call_helper(Helper::PrivateSet, args)
    }

    /// `__privateWrapper(obj, member, setter, getter)._`, an assignment target for any operator.
    pub(super) fn private_wrapper_target(
        &mut self,
        obj: Expression<'a>,
        l: PrivateLowering<'a>,
    ) -> SimpleAssignmentTarget<'a> {
        let mut args = vec![obj, self.ident(l.member())];
        match (l.setter(), l.getter()) {
            (set, Some(get)) => {
                args.push(set.map_or_else(|| self.null(), |s| self.ident(s)));
                args.push(self.ident(get));
            }
            (Some(set), None) => args.push(self.ident(set)),
            (None, None) => {}
        }
        let wrapper = self.call_helper(Helper::PrivateWrapper, args);
        SimpleAssignmentTarget::new_static_member_expression(
            SPAN,
            wrapper,
            IdentifierName::new(SPAN, "_", &self.b),
            false,
            &self.b,
        )
    }

    /// `this` as moved code sees it.
    pub(super) fn this_value(&self) -> Expression<'a> {
        self.fn_ctx
            .this_to
            .map_or_else(|| self.this(), |t| self.ident(t))
    }

    /// The home object of `super`: the class (static) or its prototype.
    pub(super) fn super_home(&self, home: SuperHome<'a>) -> Expression<'a> {
        let class = self.ident(home.class);
        if home.is_static {
            class
        } else {
            self.member(class, "prototype")
        }
    }

    /// The key of `super.x` / `super[x]` as an expression, when `e` is one and `super` is lowered.
    pub(super) fn take_super_key(&mut self, e: &mut Expression<'a>) -> Option<Expression<'a>> {
        self.fn_ctx.super_home?;
        match e {
            Expression::StaticMemberExpression(m) if m.object.is_super() => {
                Some(self.string(m.property.name.as_str()))
            }
            Expression::ComputedMemberExpression(m) if m.object.is_super() => {
                Some(m.expression.take_in(&self.b))
            }
            _ => None,
        }
    }

    pub(super) fn super_get(&mut self, key: Expression<'a>) -> Expression<'a> {
        let home = self.fn_ctx.super_home.map(|h| self.super_home(h));
        let home = home.unwrap_or_else(|| self.void0());
        let this = self.this_value();
        self.call_helper(Helper::SuperGet, vec![home, this, key])
    }

    /// Whether an expression is a member access this traversal rewrites (lowered private name,
    /// or `super` in moved code).
    pub(super) fn is_rewritten_expr(&self, e: &Expression<'a>) -> bool {
        match e {
            Expression::PrivateFieldExpression(p) => {
                self.lookup_private(p.field.name.as_str()).is_some()
            }
            Expression::StaticMemberExpression(m) => {
                m.object.is_super() && self.fn_ctx.super_home.is_some()
            }
            Expression::ComputedMemberExpression(m) => {
                m.object.is_super() && self.fn_ctx.super_home.is_some()
            }
            _ => false,
        }
    }

    pub(super) fn is_rewritten_target(&self, t: &AssignmentTarget<'a>) -> bool {
        match t {
            AssignmentTarget::PrivateFieldExpression(p) => {
                self.lookup_private(p.field.name.as_str()).is_some()
            }
            AssignmentTarget::StaticMemberExpression(m) => {
                m.object.is_super() && self.fn_ctx.super_home.is_some()
            }
            AssignmentTarget::ComputedMemberExpression(m) => {
                m.object.is_super() && self.fn_ctx.super_home.is_some()
            }
            _ => t
                .get_expression()
                .is_some_and(|e| self.is_rewritten_expr(e.get_inner_expression())),
        }
    }

    /// `obj.#x = v` → `__privateSet(obj, member, v)`; `super.x = v` → `__superSet(home, this, "x", v)`.
    pub(super) fn lower_simple_assignment(&mut self, it: &mut Expression<'a>) {
        let Expression::AssignmentExpression(a) = it else {
            return;
        };
        let mut target: Expression<'a> = match a.left.take_in(&self.b) {
            AssignmentTarget::PrivateFieldExpression(p) => Expression::PrivateFieldExpression(p),
            AssignmentTarget::StaticMemberExpression(m) => Expression::StaticMemberExpression(m),
            AssignmentTarget::ComputedMemberExpression(m) => {
                Expression::ComputedMemberExpression(m)
            }
            other => match other.get_expression() {
                Some(_) => {
                    let mut other = other;
                    match other.get_expression_mut() {
                        Some(e) => e.get_inner_expression_mut().take_in(&self.b),
                        None => return,
                    }
                }
                None => return,
            },
        };
        let mut value = a.right.take_in(&self.b);
        match &mut target {
            Expression::PrivateFieldExpression(p) => {
                let Some(l) = self.lookup_private(p.field.name.as_str()) else {
                    return;
                };
                self.visit_expression(&mut p.object);
                let obj = p.object.take_in(&self.b);
                self.visit_expression(&mut value);
                *it = self.private_write(obj, l, value);
            }
            Expression::ComputedMemberExpression(m) => {
                self.visit_expression(&mut m.expression);
                let key = m.expression.take_in(&self.b);
                self.visit_expression(&mut value);
                *it = self.super_set(key, value);
            }
            Expression::StaticMemberExpression(m) => {
                let key = self.string(m.property.name.as_str());
                self.visit_expression(&mut value);
                *it = self.super_set(key, value);
            }
            _ => {}
        }
    }

    pub(super) fn super_set(
        &mut self,
        key: Expression<'a>,
        value: Expression<'a>,
    ) -> Expression<'a> {
        let home = self.fn_ctx.super_home.map(|h| self.super_home(h));
        let home = home.unwrap_or_else(|| self.void0());
        let this = self.this_value();
        self.call_helper(Helper::SuperSet, vec![home, this, key, value])
    }
}
