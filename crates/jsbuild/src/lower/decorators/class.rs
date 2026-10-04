//! The esbuild lowering of a decorated class (`lowerClass`), in esbuild's phases: the analysis
//! of the elements, `hoistComputedProperties`, `processProperties` (`properties.rs`),
//! `insertInitializersIntoConstructor` (`constructor.rs`) and `finishAndGenerateCode`.

use super::*;

/// A lowered class: what runs before it (the class decorators, then the decorators and keys of
/// its elements), the class, and what runs after it.
pub(super) struct LoweredClass<'a> {
    pub(super) class_decorators: Option<Expression<'a>>,
    pub(super) chain: Vec<Expression<'a>>,
    pub(super) class: ArenaBox<'a, Class<'a>>,
    pub(super) suffix: Vec<Expression<'a>>,
}

impl<'a> Lowerer<'a, '_> {
    /// The esbuild lowering of a decorated class (`lowerClass`): returns the expressions to run
    /// before the class, the class, and the expressions to run after it.
    pub(super) fn lower_class(
        &mut self,
        mut class: ArenaBox<'a, Class<'a>>,
        plan: &ClassPlan<'a>,
    ) -> LoweredClass<'a> {
        let mut out = ClassOut {
            storage_names: plan.declared_privates.clone(),
            ..ClassOut::default()
        };
        out.init_ref = Some(self.temp_named("_init"));
        let mut elems = self.analyse_elements(&mut class);
        self.hoist_computed_properties(&mut elems, &mut class, plan.lower_members, &mut out);
        let next_slot = initializer_slots(&elems, &mut out);
        let class_decorators = self.take_class_decorators(&mut class, plan, &mut out);
        let (mut body, ctor) = self.process_properties(elems, plan, next_slot, &mut out);
        let derived = class.heritage.is_some();
        self.insert_initializers_into_constructor(&mut body, ctor, derived, &mut out);
        class.body.body = ArenaVec::from_iter_in(body, &self.b);
        let suffix = self.finish_and_generate_code(&mut class, plan, &mut out);
        LoweredClass {
            class_decorators,
            chain: std::mem::take(&mut out.chain),
            class,
            suffix,
        }
    }

    /// The analysis: the elements of the class body (taken out of it), with their kind, private
    /// name and decorators.
    fn analyse_elements(&mut self, class: &mut Class<'a>) -> Vec<Elem<'a>> {
        let elements = class.body.body.take_in(&self.b);
        let mut elems: Vec<Elem<'a>> = Vec::with_capacity(elements.len());
        for mut el in elements {
            let kind = element_kind(&el);
            let is_static = element_is_static(&el);
            let private = match element_key(&el) {
                Some(PropertyKey::PrivateIdentifier(p)) => Some(self.arena_str(p.name.as_str())),
                _ => None,
            };
            let decorators = element_decorators_mut(&mut el)
                .map(|d| {
                    d.take_in(&self.b)
                        .into_iter()
                        .map(|d| d.expression)
                        .collect()
                })
                .unwrap_or_default();
            elems.push(Elem {
                el,
                kind,
                is_static,
                private,
                decorators,
                decorators_ref: None,
                key_ref: None,
            });
        }
        elems
    }

    /// `hoistComputedProperties`: decorator lists and keys of moved fields are evaluated in
    /// order, prepended to the next computed key kept in the class, else to the chain (in the
    /// class heritage when there is one, else `out.chain`, run before the class).
    fn hoist_computed_properties(
        &mut self,
        elems: &mut [Elem<'a>],
        class: &mut Class<'a>,
        lower: bool,
        out: &mut ClassOut<'a>,
    ) {
        let mut chain: Vec<Expression<'a>> = Vec::new();
        let mut next_computed: Option<usize> = None;
        for i in (0..elems.len()).rev() {
            let kind = elems[i].kind;
            if matches!(
                kind,
                ElemKind::StaticBlock | ElemKind::TypeOnly | ElemKind::Constructor
            ) {
                continue;
            }
            let has_decorators = !elems[i].decorators.is_empty();
            let decorators_expr = has_decorators.then(|| self.hoist_decorators(&mut elems[i]));
            let computed = element_is_computed(&elems[i].el);
            let pure = element_key(&elems[i].el).is_some_and(|k| key_is_pure(k, computed));
            if pure {
                if let Some(d) = decorators_expr {
                    match next_computed {
                        Some(n) => self.prepend_to_key(&mut elems[n].el, d),
                        None => chain.insert(0, d),
                    }
                }
                continue;
            }
            if let Some(d) = decorators_expr {
                self.prepend_to_key(&mut elems[i].el, d);
            }
            let rewrite_accessor = kind == ElemKind::Accessor && !has_decorators;
            let must_lower_field = matches!(kind, ElemKind::Field | ElemKind::Accessor) && lower;
            let moved = computed && (has_decorators || must_lower_field || rewrite_accessor);
            if moved {
                if !rewrite_accessor && must_lower_field {
                    let r = self.temp();
                    let key = self.take_key_expr(&mut elems[i].el);
                    let inline = self.assign(r, key);
                    if let Some(k) = element_key_mut(&mut elems[i].el) {
                        *k = PropertyKey::from(self.ident(r));
                    }
                    elems[i].key_ref = Some(r);
                    match next_computed {
                        Some(n) => self.prepend_to_key(&mut elems[n].el, inline),
                        None => chain.insert(0, inline),
                    }
                    continue;
                }
                self.capture_key(&mut elems[i]);
            }
            if element_is_computed(&elems[i].el) {
                if !chain.is_empty() {
                    let r = match elems[i].key_ref {
                        Some(r) => r,
                        None => self.capture_key(&mut elems[i]),
                    };
                    let key = self.take_key_expr(&mut elems[i].el);
                    let mut all = vec![key];
                    all.append(&mut chain);
                    all.push(self.ident(r));
                    let joined = self.seq(all);
                    if let Some(k) = element_key_mut(&mut elems[i].el) {
                        *k = PropertyKey::from(joined);
                    }
                }
                next_computed = Some(i);
            }
        }
        if !chain.is_empty()
            && let Some(h) = &mut class.heritage
        {
            let r = self.temp();
            let base = h.expression.take_in(&self.b);
            let mut all = vec![self.assign(r, base)];
            all.append(&mut chain);
            all.push(self.ident(r));
            h.expression = self.seq(all);
            out.extends_ref = Some(r);
        }
        out.chain = chain;
    }

    /// The decorators of an element assigned to a temporary (`_x_dec = [...]`), which becomes its
    /// `decorators_ref`.
    fn hoist_decorators(&mut self, e: &mut Elem<'a>) -> Expression<'a> {
        let hint = element_key(&e.el).map(key_hint).unwrap_or_default();
        let name = if hint.is_empty() {
            "_dec".to_owned()
        } else {
            format!("_{hint}_dec")
        };
        let r = self.temp_named(&name);
        let decs = std::mem::take(&mut e.decorators);
        e.decorators_ref = Some(r);
        self.assign(r, self.array(decs))
    }

    /// The computed key of an element assigned to a temporary in place (`[_a = key]`), which
    /// becomes its `key_ref`.
    fn capture_key(&mut self, e: &mut Elem<'a>) -> &'a str {
        let r = self.temp();
        let key = self.take_key_expr(&mut e.el);
        let assigned = self.assign(r, key);
        if let Some(k) = element_key_mut(&mut e.el) {
            *k = PropertyKey::from(assigned);
        }
        e.key_ref = Some(r);
        r
    }

    /// The class decorators, assigned to a temporary (`out.class_decorators_ref`): evaluated first
    /// of all, in the enclosing scope.
    fn take_class_decorators(
        &mut self,
        class: &mut Class<'a>,
        plan: &ClassPlan<'a>,
        out: &mut ClassOut<'a>,
    ) -> Option<Expression<'a>> {
        if class.decorators.is_empty() {
            return None;
        }
        let base = if plan.name.is_empty() {
            "class".to_owned()
        } else {
            plan.name.clone()
        };
        let r = self.temp_named(&format!("_{base}_decorators"));
        let decs = class
            .decorators
            .take_in(&self.b)
            .into_iter()
            .map(|d| d.expression)
            .collect();
        out.class_decorators_ref = Some(r);
        Some(self.assign(r, self.array(decs)))
    }

    /// `finishAndGenerateCode`: what runs after the class: `__decoratorStart`, the private
    /// members, the element decorations, the class decorators and the initializers.
    fn finish_and_generate_code(
        &mut self,
        class: &mut Class<'a>,
        plan: &ClassPlan<'a>,
        out: &mut ClassOut<'a>,
    ) -> Vec<Expression<'a>> {
        let init = out.init_ref.unwrap_or("_init");
        let decorate_class = if let Some(cdr) = out.class_decorators_ref {
            let args = vec![
                self.ident(init),
                self.number(0.0),
                self.string(&plan.name),
                self.ident(cdr),
                self.ident(plan.class_ref),
            ];
            let call = self.call_helper(Helper::DecorateElement, args);
            self.assign(plan.class_ref, call)
        } else {
            self.call_helper(
                Helper::DecoratorMetadata,
                vec![self.ident(init), self.ident(plan.class_ref)],
            )
        };
        let base = match &mut class.heritage {
            Some(h) => {
                let r = match out.extends_ref {
                    Some(r) => r,
                    None => {
                        let r = self.temp();
                        let e = h.expression.take_in(&self.b);
                        h.expression = self.assign(r, e);
                        r
                    }
                };
                self.ident(r)
            }
            None => self.null(),
        };
        let mut suffix = Vec::new();
        let start = self.call_helper(Helper::DecoratorStart, vec![base]);
        suffix.push(self.assign(init, start));
        suffix.append(&mut out.private_members);
        suffix.append(&mut out.dec_static_non_field);
        suffix.append(&mut out.dec_instance_non_field);
        suffix.append(&mut out.dec_static_field);
        suffix.append(&mut out.dec_instance_field);
        suffix.append(&mut out.static_private_methods);
        suffix.push(decorate_class);
        if out.call_static_method_extra {
            suffix.push(self.call_helper(
                Helper::RunInitializers,
                vec![
                    self.ident(init),
                    self.number(3.0),
                    self.ident(plan.class_ref),
                ],
            ));
        }
        suffix.append(&mut out.static_members);
        if out.class_decorators_ref.is_some() {
            suffix.push(self.call_helper(
                Helper::RunInitializers,
                vec![
                    self.ident(init),
                    self.number(1.0),
                    self.ident(plan.class_ref),
                ],
            ));
        }
        suffix
    }
}

/// The first initializer slot of each group of decorated elements (static accessors,
/// accessors, static fields, fields), slots being numbered in decoration order; notes in `out`
/// whether decorated methods need their initializers run.
fn initializer_slots(elems: &[Elem<'_>], out: &mut ClassOut<'_>) -> [usize; 4] {
    let mut counts = [0usize; 4];
    for e in elems {
        if e.decorators_ref.is_none() {
            continue;
        }
        match (e.kind, e.is_static) {
            (ElemKind::Accessor, true) => counts[0] += 1,
            (ElemKind::Accessor, false) => counts[1] += 1,
            (ElemKind::Field, true) => counts[2] += 1,
            (ElemKind::Field, false) => counts[3] += 1,
            (_, true) => out.call_static_method_extra = true,
            (_, false) => out.call_instance_method_extra = true,
        }
    }
    [
        0,
        counts[0],
        counts[0] + counts[1],
        counts[0] + counts[1] + counts[2],
    ]
}
