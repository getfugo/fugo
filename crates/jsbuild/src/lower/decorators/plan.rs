//! Planning a class: what is decorated, which private names are lowered, and esbuild's checks.

use super::*;

impl<'a> Lowerer<'a, '_> {
    /// Whether a class needs the decorator lowering (else at most its auto-accessors change).
    pub(super) fn is_decorated(class: &Class<'a>) -> bool {
        !class.declare
            && (!class.decorators.is_empty()
                || class
                    .body
                    .body
                    .iter()
                    .any(|e| !element_decorators(e).is_empty()))
    }

    /// Records the name an anonymous class expression gets from its context.
    pub(super) fn hint(&mut self, value: &Expression<'a>, name: &str) {
        if let Expression::ClassExpression(class) = value.get_inner_expression()
            && class.id.is_none()
        {
            self.name_hints.insert(class.span.start, name.to_owned());
        }
    }

    /// Plans the lowering of a decorated class before its body is visited.
    pub(super) fn plan_class(&mut self, class: &Class<'a>, kind: ClassKind) -> ClassPlan<'a> {
        let name = match (&class.id, kind) {
            (Some(id), _) => id.name.as_str().to_owned(),
            (None, ClassKind::ExportDefault) => "default".to_owned(),
            (None, _) => self
                .name_hints
                .remove(&class.span.start)
                .unwrap_or_default(),
        };
        let symbol = class.id.as_ref().and_then(|id| id.symbol_id.get());
        let lower_members = class
            .body
            .body
            .iter()
            .any(|e| !element_decorators(e).is_empty());
        let mut declared_privates = HashSet::new();
        for e in &class.body.body {
            if let Some(PropertyKey::PrivateIdentifier(p)) = element_key(e) {
                declared_privates.insert(p.name.as_str().to_owned());
            }
        }
        let mut privates = HashMap::new();
        let (mut instance_brand, mut static_brand) = (None, None);
        if lower_members {
            let prefix = if name.is_empty() || !is_identifier_name(&name) {
                String::new()
            } else {
                format!("_{name}")
            };
            for e in &class.body.body {
                let Some(PropertyKey::PrivateIdentifier(p)) = element_key(e) else {
                    continue;
                };
                let pname = p.name.as_str();
                let is_static = element_is_static(e);
                let mut brand = |s: &mut Self| {
                    let slot = if is_static {
                        &mut static_brand
                    } else {
                        &mut instance_brand
                    };
                    *slot.get_or_insert_with(|| {
                        s.temp_named(&format!(
                            "{prefix}{}",
                            if is_static { "_static" } else { "_instances" }
                        ))
                    })
                };
                let lowering = match e {
                    ClassElement::PropertyDefinition(_) => PrivateLowering::Field {
                        map: self.temp_named(&format!("_{pname}")),
                    },
                    ClassElement::MethodDefinition(m) if m.value.body.is_none() => continue,
                    ClassElement::MethodDefinition(m) => match m.kind {
                        MethodDefinitionKind::Get | MethodDefinitionKind::Set => {
                            let brand = brand(self);
                            let is_get = m.kind == MethodDefinitionKind::Get;
                            let f = self.temp_named(&format!(
                                "{pname}{}",
                                if is_get { "_get" } else { "_set" }
                            ));
                            let (mut get, mut set) = match privates.get(pname) {
                                Some(PrivateLowering::Accessor { get, set, .. }) => (*get, *set),
                                _ => (None, None),
                            };
                            if is_get {
                                get = Some(f);
                            } else {
                                set = Some(f);
                            }
                            PrivateLowering::Accessor { brand, get, set }
                        }
                        _ => {
                            let brand = brand(self);
                            PrivateLowering::Method {
                                brand,
                                func: self.temp_named(&format!("{pname}_fn")),
                            }
                        }
                    },
                    ClassElement::AccessorProperty(_) => {
                        let brand = brand(self);
                        let get = self.temp_named(&format!("{pname}_get"));
                        let set = self.temp_named(&format!("{pname}_set"));
                        PrivateLowering::Accessor {
                            brand,
                            get: Some(get),
                            set: Some(set),
                        }
                    }
                    _ => continue,
                };
                privates.insert(self.arena_str(pname), lowering);
            }
        }
        let (class_ref, capture) = if kind == ClassKind::Expr {
            (self.temp(), true)
        } else if class.id.is_none() {
            (self.fresh("_default"), false)
        } else {
            let force = kind == ClassKind::ExportStmt && self.namespace_depth > 0;
            let mut scan = InnerRefScan {
                scoping: self.scoping,
                symbol,
                found: false,
            };
            if let Some(h) = &class.heritage {
                scan.visit_expression(&h.expression);
            }
            for e in &class.body.body {
                scan.visit_class_element(e);
            }
            let moved_this = lower_members
                && class
                    .body
                    .body
                    .iter()
                    .any(|e| uses_moved_this_or_super(e, &privates));
            if scan.found || moved_this || force {
                (self.fresh(&format!("_{name}")), true)
            } else {
                (self.arena_str(&name), false)
            }
        };
        ClassPlan {
            name,
            symbol,
            lower_members,
            has_class_decorators: !class.decorators.is_empty(),
            class_ref,
            capture,
            privates,
            instance_brand,
            static_brand,
            declared_privates,
        }
    }

    /// Visits a class with the private names it declares in scope, giving each element the
    /// `this`/`super` meaning it has once lowered (`plan` is `None` for a class kept as is).
    pub(super) fn visit_class_parts(
        &mut self,
        class: &mut Class<'a>,
        plan: Option<&ClassPlan<'a>>,
    ) {
        // Class decorators and the heritage see the enclosing private names.
        for d in &mut class.decorators {
            self.visit_expression(&mut d.expression);
        }
        if let Some(h) = &mut class.heritage {
            self.visit_expression(&mut h.expression);
        }
        let mut privates = HashMap::new();
        for e in &class.body.body {
            if let Some(PropertyKey::PrivateIdentifier(p)) = element_key(e) {
                let name = self.arena_str(p.name.as_str());
                let lowering = plan.and_then(|p| p.privates.get(name).copied());
                privates.insert(name, lowering);
            }
        }
        self.classes.push(ClassScope { privates });
        let lower = plan.filter(|p| p.lower_members);
        for e in &mut class.body.body {
            let is_static = element_is_static(e);
            let field_ctx = match lower {
                Some(p) if is_static => FnCtx {
                    this_to: Some(p.class_ref),
                    super_home: Some(SuperHome {
                        class: p.class_ref,
                        is_static: true,
                    }),
                    new_target_undefined: true,
                },
                Some(_) => FnCtx {
                    new_target_undefined: true,
                    ..FnCtx::default()
                },
                None => FnCtx::default(),
            };
            match e {
                ClassElement::StaticBlock(block) => {
                    let old = std::mem::replace(&mut self.fn_ctx, field_ctx);
                    let temps = self.with_var_scope(|s| s.visit_statements(&mut block.body));
                    self.finish_var_scope(&temps, &mut block.body);
                    self.fn_ctx = old;
                }
                ClassElement::MethodDefinition(m) => {
                    for d in &mut m.decorators {
                        self.visit_expression(&mut d.expression);
                    }
                    if let Some(key) = m.key.as_expression_mut() {
                        self.visit_expression(key);
                    }
                    let lowered_private = matches!(&m.key, PropertyKey::PrivateIdentifier(_));
                    let ctx = match lower {
                        Some(p) if lowered_private => FnCtx {
                            super_home: Some(SuperHome {
                                class: p.class_ref,
                                is_static,
                            }),
                            ..FnCtx::default()
                        },
                        _ => FnCtx::default(),
                    };
                    self.visit_function_with_ctx(&mut m.value, ctx);
                }
                ClassElement::PropertyDefinition(p) => {
                    for d in &mut p.decorators {
                        self.visit_expression(&mut d.expression);
                    }
                    if let Some(key) = p.key.as_expression_mut() {
                        self.visit_expression(key);
                    }
                    let name = static_key_name(&p.key, p.computed);
                    if let Some(value) = &mut p.value {
                        if let Some(name) = name {
                            self.hint(value, &name);
                        }
                        let old = std::mem::replace(&mut self.fn_ctx, field_ctx);
                        self.visit_expression(value);
                        self.fn_ctx = old;
                    }
                }
                ClassElement::AccessorProperty(p) => {
                    for d in &mut p.decorators {
                        self.visit_expression(&mut d.expression);
                    }
                    if let Some(key) = p.key.as_expression_mut() {
                        self.visit_expression(key);
                    }
                    let name = static_key_name(&p.key, p.computed);
                    if let Some(value) = &mut p.value {
                        if let Some(name) = name {
                            self.hint(value, &name);
                        }
                        let old = std::mem::replace(&mut self.fn_ctx, field_ctx);
                        self.visit_expression(value);
                        self.fn_ctx = old;
                    }
                }
                ClassElement::TSIndexSignature(_) => {}
            }
        }
        self.classes.pop();
    }

    /// Reports decorators this lowering does not support on the members of a class.
    pub(super) fn check_member_decorators(&mut self, class: &Class<'a>) {
        let mut bad = Vec::new();
        for e in &class.body.body {
            let decorators = element_decorators(e);
            let Some(first) = decorators.first() else {
                continue;
            };
            let message = match e {
                ClassElement::MethodDefinition(m)
                    if m.kind == MethodDefinitionKind::Constructor =>
                {
                    "Decorators are not allowed on class constructors"
                }
                e if element_kind(e) == ElemKind::TypeOnly => "Decorators are not valid here",
                _ => continue,
            };
            bad.push((first.span, message));
        }
        for (span, message) in bad {
            self.error(span, message);
        }
    }
}
