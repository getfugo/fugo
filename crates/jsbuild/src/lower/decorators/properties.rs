//! `processProperties`: the elements of a class being lowered, decorated (`__decorateElement`)
//! or not, moved out of the class body or kept in it.

use super::*;

impl<'a> Lowerer<'a, '_> {
    /// `processProperties`: decorated elements become `__decorateElement` calls; lowered fields,
    /// auto-accessors, static blocks and private methods move out of the class body. Returns the
    /// elements that stay, and the index of the constructor among them.
    pub(super) fn process_properties(
        &mut self,
        elems: Vec<Elem<'a>>,
        plan: &ClassPlan<'a>,
        mut next_slot: [usize; 4],
        out: &mut ClassOut<'a>,
    ) -> (Vec<ClassElement<'a>>, Option<usize>) {
        let lower = plan.lower_members;
        let mut body: Vec<ClassElement<'a>> = Vec::with_capacity(elems.len());
        let mut ctor: Option<usize> = None;
        for mut e in elems {
            match e.kind {
                ElemKind::StaticBlock => {
                    if lower {
                        if let ClassElement::StaticBlock(block) = e.el {
                            self.lower_static_block(out, block.unbox().body);
                        }
                    } else {
                        body.push(e.el);
                    }
                    continue;
                }
                ElemKind::TypeOnly => {
                    let lowered_private = e.private.is_some_and(|p| plan.privates.contains_key(p));
                    if !(lowered_private
                        || lower && matches!(e.el, ClassElement::PropertyDefinition(_)))
                    {
                        body.push(e.el);
                    }
                    continue;
                }
                _ => {}
            }
            let slot = e.decorators_ref.and_then(|_| {
                let group = match (e.kind, e.is_static) {
                    (ElemKind::Accessor, true) => 0,
                    (ElemKind::Accessor, false) => 1,
                    (ElemKind::Field, true) => 2,
                    (ElemKind::Field, false) => 3,
                    _ => return None,
                };
                let s = next_slot[group];
                next_slot[group] += 1;
                Some(s)
            });
            if self.decorate_element(&mut e, slot, plan, out) {
                continue;
            }
            let Elem {
                el,
                kind,
                is_static,
                private,
                key_ref,
                ..
            } = e;
            match kind {
                ElemKind::Accessor => {
                    let ClassElement::AccessorProperty(a) = el else {
                        continue;
                    };
                    self.lower_plain_accessor(out, plan, a.unbox(), key_ref, &mut body);
                }
                ElemKind::Field if lower => {
                    let ClassElement::PropertyDefinition(p) = el else {
                        continue;
                    };
                    let p = p.unbox();
                    let target = match private.and_then(|n| plan.privates.get(n).copied()) {
                        Some(PrivateLowering::Field { map }) => FieldTarget::Private(map),
                        _ => FieldTarget::Public(self.key_value(&p.key, key_ref)),
                    };
                    self.lower_field(out, plan, is_static, target, p.value, slot, p.span);
                }
                ElemKind::Method | ElemKind::Getter | ElemKind::Setter
                    if private.is_some_and(|n| plan.privates.contains_key(n)) =>
                {
                    let ClassElement::MethodDefinition(m) = el else {
                        continue;
                    };
                    let m = m.unbox();
                    let Some(l) = private.and_then(|n| plan.privates.get(n).copied()) else {
                        continue;
                    };
                    let f = match (kind, l) {
                        (ElemKind::Method, PrivateLowering::Method { func, .. }) => func,
                        (ElemKind::Getter, PrivateLowering::Accessor { get: Some(g), .. }) => g,
                        (ElemKind::Setter, PrivateLowering::Accessor { set: Some(s), .. }) => s,
                        _ => continue,
                    };
                    self.add_brand(out, plan, is_static);
                    let func = Expression::FunctionExpression(m.value);
                    let assigned = self.assign(f, func);
                    out.private_members.push(assigned);
                }
                ElemKind::Constructor => {
                    ctor = Some(body.len());
                    body.push(el);
                }
                _ => body.push(el),
            }
        }
        (body, ctor)
    }

    /// The `__decorateElement` call of a decorated element, in the `out.dec_*` list of its kind
    /// (an auto-accessor's storage lowered first). Returns whether the element is done (an
    /// auto-accessor); otherwise it is lowered as an undecorated one would be.
    fn decorate_element(
        &mut self,
        e: &mut Elem<'a>,
        slot: Option<usize>,
        plan: &ClassPlan<'a>,
        out: &mut ClassOut<'a>,
    ) -> bool {
        let Some(decorators_ref) = e.decorators_ref else {
            return false;
        };
        let (kind, is_static, private) = (e.kind, e.is_static, e.private);
        let init = out.init_ref.unwrap_or("_init");
        let mut flags = match kind {
            ElemKind::Method => 1,
            ElemKind::Getter => 2,
            ElemKind::Setter => 3,
            ElemKind::Accessor => 4,
            _ => 5,
        };
        if is_static {
            flags |= 8;
        }
        if private.is_some() {
            flags |= 16;
        }
        let key = element_key(&e.el).map_or_else(|| self.void0(), |k| self.key_value(k, e.key_ref));
        let mut args = vec![
            self.ident(init),
            self.number(f64::from(flags)),
            key,
            self.ident(decorators_ref),
        ];
        let lowering = private.and_then(|p| plan.privates.get(p).copied());
        let mut fn_ref = None;
        match lowering {
            Some(l) => {
                args.push(self.ident(l.member()));
                fn_ref = match (kind, l) {
                    (ElemKind::Method, PrivateLowering::Method { func, .. }) => Some(func),
                    (ElemKind::Getter, PrivateLowering::Accessor { get, .. }) => get,
                    (ElemKind::Setter, PrivateLowering::Accessor { set, .. }) => set,
                    _ => None,
                };
                if let Some(f) = fn_ref {
                    args.push(self.ident(f));
                }
            }
            None => args.push(self.ident(plan.class_ref)),
        }
        if kind == ElemKind::Accessor {
            let ClassElement::AccessorProperty(a) = &mut e.el else {
                return true;
            };
            let hint = key_hint(&a.key);
            let storage = if hint.is_empty() {
                self.temp()
            } else {
                self.temp_named(&format!("_{hint}"))
            };
            let value = a.value.take();
            let span = a.span;
            let target = FieldTarget::Private(storage);
            self.lower_field(out, plan, is_static, target, value, slot, span);
            args.push(self.ident(storage));
        }
        let mut element = self.call_helper(Helper::DecorateElement, args);
        if let Some(f) = fn_ref {
            element = self.assign(f, element);
        } else if let (
            ElemKind::Accessor,
            Some(PrivateLowering::Accessor {
                get: Some(get),
                set: Some(set),
                ..
            }),
        ) = (kind, lowering)
        {
            let t = self.temp();
            let get_v = self.member(self.ident(t), "get");
            let set_v = self.member(self.ident(t), "set");
            element = self.seq(vec![
                self.assign(t, element),
                self.assign(get, get_v),
                self.assign(set, set_v),
            ]);
        }
        match (kind, is_static) {
            (ElemKind::Field, true) => out.dec_static_field.push(element),
            (ElemKind::Field, false) => out.dec_instance_field.push(element),
            (_, true) => out.dec_static_non_field.push(element),
            (_, false) => out.dec_instance_non_field.push(element),
        }
        if kind == ElemKind::Accessor {
            if lowering.is_some() {
                self.add_brand(out, plan, is_static);
            }
            return true;
        }
        false
    }
}
