//! Auto-accessors (`accessor x`): undecorated ones become a private field with a getter and a
//! setter.

use super::*;

/// The private name an auto-accessor stores its value under (esbuild's naming: `#x`, `#_x` for
/// `accessor #x`, `#a`... for computed keys), unique in its class.
pub(super) fn unique_storage_name(
    key: &PropertyKey<'_>,
    declared: &mut HashSet<String>,
    count: &mut usize,
) -> String {
    let base = match key {
        PropertyKey::StaticIdentifier(id) => id.name.as_str().to_owned(),
        PropertyKey::StringLiteral(s) if is_identifier_name(s.value.as_str()) => {
            s.value.as_str().to_owned()
        }
        PropertyKey::PrivateIdentifier(p) => format!("_{}", p.name),
        _ => {
            let mut n = *count;
            *count += 1;
            let mut s = String::new();
            loop {
                s.insert(0, char::from(b'a' + u8::try_from(n % 26).unwrap_or(0)));
                n /= 26;
                if n == 0 {
                    break;
                }
                n -= 1;
            }
            s
        }
    };
    let mut name = base.clone();
    let mut n = 2;
    while declared.contains(&name) {
        name = format!("{base}{n}");
        n += 1;
    }
    declared.insert(name.clone());
    name
}

impl<'a> Lowerer<'a, '_> {
    /// Rewrites the auto-accessors of a class without decorators to private storage with a
    /// getter and setter (`accessor x = 1` → `#x = 1; get x() {…} set x(_) {…}`).
    pub(super) fn rewrite_plain_accessors(&mut self, class: &mut Class<'a>) {
        let mut declared: HashSet<String> = class
            .body
            .body
            .iter()
            .filter_map(|e| match element_key(e) {
                Some(PropertyKey::PrivateIdentifier(p)) => Some(p.name.as_str().to_owned()),
                _ => None,
            })
            .collect();
        let old = class.body.body.take_in(&self.b);
        let mut out = ArenaVec::with_capacity_in(old.len() + 2, &self.b);
        let mut count = 0;
        for e in old {
            let ClassElement::AccessorProperty(mut a) = e else {
                out.push(e);
                continue;
            };
            if a.r#type == AccessorPropertyType::TSAbstractAccessorProperty {
                out.push(ClassElement::AccessorProperty(a));
                continue;
            }
            let key_value = if a.computed && !key_is_pure(&a.key, true) {
                let t = self.temp();
                let key = std::mem::replace(&mut a.key, PropertyKey::from(self.ident(t)));
                a.key = PropertyKey::from(self.assign(t, key.into_expression()));
                Some(self.ident(t))
            } else {
                None
            };
            let storage = unique_storage_name(&a.key, &mut declared, &mut count);
            let storage = self.arena_str(&storage);
            let a = a.unbox();
            let setter_key = match key_value {
                Some(k) => PropertyKey::from(k),
                None => self.clone_key(&a.key),
            };
            out.push(ClassElement::new_property_definition(
                a.span,
                PropertyDefinitionType::PropertyDefinition,
                ArenaVec::new_in(&self.b),
                PropertyKey::new_private_identifier(SPAN, storage, &self.b),
                a.type_annotation,
                a.value,
                false,
                a.r#static,
                false,
                false,
                false,
                a.definite,
                false,
                None,
                &self.b,
            ));
            let get = self.native_storage_get(storage);
            let set_value = self.ident("_");
            let set = self.native_storage_set(storage, set_value);
            out.push(self.getter(a.key, a.computed, a.r#static, get, a.span));
            out.push(self.setter(setter_key, a.computed, a.r#static, set, a.span));
        }
        class.body.body = out;
    }

    pub(super) fn native_storage_get(&self, storage: &'a str) -> Expression<'a> {
        Expression::new_private_field_expression(
            SPAN,
            self.this(),
            PrivateIdentifier::new(SPAN, storage, &self.b),
            false,
            &self.b,
        )
    }

    pub(super) fn native_storage_set(
        &self,
        storage: &'a str,
        value: Expression<'a>,
    ) -> Expression<'a> {
        let target = Expression::new_private_field_expression(
            SPAN,
            self.this(),
            PrivateIdentifier::new(SPAN, storage, &self.b),
            false,
            &self.b,
        );
        let target = AssignmentTarget::from(target.into_member_expression());
        Expression::new_assignment_expression(
            SPAN,
            AssignmentOperator::Assign,
            target,
            value,
            &self.b,
        )
    }

    pub(super) fn getter(
        &self,
        key: PropertyKey<'a>,
        computed: bool,
        is_static: bool,
        value: Expression<'a>,
        span: Span,
    ) -> ClassElement<'a> {
        let ret = Statement::new_return_statement(SPAN, Some(value), &self.b);
        let func = self.function(ArenaVec::new_in(&self.b), None, vec![ret]);
        ClassElement::new_method_definition(
            span,
            MethodDefinitionType::MethodDefinition,
            ArenaVec::new_in(&self.b),
            key,
            func,
            MethodDefinitionKind::Get,
            computed,
            is_static,
            false,
            false,
            None,
            &self.b,
        )
    }

    pub(super) fn setter(
        &self,
        key: PropertyKey<'a>,
        computed: bool,
        is_static: bool,
        body: Expression<'a>,
        span: Span,
    ) -> ClassElement<'a> {
        let func = self.setter_function(body);
        ClassElement::new_method_definition(
            span,
            MethodDefinitionType::MethodDefinition,
            ArenaVec::new_in(&self.b),
            key,
            func,
            MethodDefinitionKind::Set,
            computed,
            is_static,
            false,
            false,
            None,
            &self.b,
        )
    }

    /// `function (_) { body; }`.
    pub(super) fn setter_function(&self, body: Expression<'a>) -> ArenaBox<'a, Function<'a>> {
        let param = FormalParameter::new_plain(
            SPAN,
            BindingPattern::new_binding_identifier(SPAN, "_", &self.b),
            &self.b,
        );
        self.function(
            ArenaVec::from_iter_in([param], &self.b),
            None,
            vec![self.expr_stmt(body)],
        )
    }

    /// A copy of a key without side effects (a literal, a name, or a temporary).
    pub(super) fn clone_key(&self, key: &PropertyKey<'a>) -> PropertyKey<'a> {
        match key {
            PropertyKey::StaticIdentifier(id) => {
                PropertyKey::new_static_identifier(SPAN, id.name, &self.b)
            }
            PropertyKey::PrivateIdentifier(p) => {
                PropertyKey::new_private_identifier(SPAN, p.name, &self.b)
            }
            PropertyKey::StringLiteral(s) => PropertyKey::from(self.string(s.value.as_str())),
            PropertyKey::NumericLiteral(n) => PropertyKey::from(self.number(n.value)),
            PropertyKey::BigIntLiteral(n) => PropertyKey::from(Expression::new_big_int_literal(
                SPAN, n.value, n.raw, n.base, &self.b,
            )),
            PropertyKey::Identifier(id) => PropertyKey::from(
                self.clone_simple(&Expression::Identifier(id.clone_in(self.alloc)))
                    .unwrap_or_else(|| self.void0()),
            ),
            _ => PropertyKey::from(self.void0()),
        }
    }

    /// The value of a key as an expression (`foo` → `"foo"`), for helpers.
    pub(super) fn key_value(
        &self,
        key: &PropertyKey<'a>,
        key_ref: Option<&'a str>,
    ) -> Expression<'a> {
        if let Some(r) = key_ref {
            return self.ident(r);
        }
        match key {
            PropertyKey::StaticIdentifier(id) => self.string(id.name.as_str()),
            PropertyKey::PrivateIdentifier(p) => self.string(&format!("#{}", p.name)),
            PropertyKey::StringLiteral(s) => self.string(s.value.as_str()),
            PropertyKey::NumericLiteral(n) => self.number(n.value),
            PropertyKey::BigIntLiteral(n) => self.string(n.value.as_str()),
            PropertyKey::Identifier(id) => self
                .clone_simple(&Expression::Identifier(id.clone_in(self.alloc)))
                .unwrap_or_else(|| self.void0()),
            _ => self.void0(),
        }
    }

    /// `rewriteAutoAccessorToGetSet` in a lowered class: the storage is a lowered field, and a
    /// private accessor's getter and setter are lowered private methods.
    pub(super) fn lower_plain_accessor(
        &mut self,
        out: &mut ClassOut<'a>,
        plan: &ClassPlan<'a>,
        a: AccessorProperty<'a>,
        key_ref: Option<&'a str>,
        body: &mut Vec<ClassElement<'a>>,
    ) {
        let is_static = a.r#static;
        let storage = unique_storage_name(
            &a.key,
            &mut out.storage_names,
            &mut out.accessor_storage_count,
        );
        let setter_key = match key_ref {
            Some(r) => PropertyKey::from(self.ident(r)),
            None => self.clone_key(&a.key),
        };
        let (get_value, set_value) = if plan.lower_members {
            let map = self.temp_named(&format!("_{storage}"));
            self.lower_field(
                out,
                plan,
                is_static,
                FieldTarget::Private(map),
                a.value,
                None,
                a.span,
            );
            let get = self.call_helper(Helper::PrivateGet, vec![self.this(), self.ident(map)]);
            let set = self.call_helper(
                Helper::PrivateSet,
                vec![self.this(), self.ident(map), self.ident("_")],
            );
            (get, set)
        } else {
            let storage = self.arena_str(&storage);
            body.push(ClassElement::new_property_definition(
                a.span,
                PropertyDefinitionType::PropertyDefinition,
                ArenaVec::new_in(&self.b),
                PropertyKey::new_private_identifier(SPAN, storage, &self.b),
                a.type_annotation,
                a.value,
                false,
                is_static,
                false,
                false,
                false,
                a.definite,
                false,
                None,
                &self.b,
            ));
            (
                self.native_storage_get(storage),
                self.native_storage_set(storage, self.ident("_")),
            )
        };
        let lowered = match &a.key {
            PropertyKey::PrivateIdentifier(p) => plan.privates.get(p.name.as_str()).copied(),
            _ => None,
        };
        if let Some(PrivateLowering::Accessor {
            get: Some(get),
            set: Some(set),
            ..
        }) = lowered
        {
            self.add_brand(out, plan, is_static);
            let ret = Statement::new_return_statement(SPAN, Some(get_value), &self.b);
            let get_fn = self.function(ArenaVec::new_in(&self.b), None, vec![ret]);
            let get_fn = self.assign(get, Expression::FunctionExpression(get_fn));
            out.private_members.push(get_fn);
            let set_fn = self.setter_function(set_value);
            let set_fn = self.assign(set, Expression::FunctionExpression(set_fn));
            out.private_members.push(set_fn);
            return;
        }
        body.push(self.getter(a.key, a.computed, is_static, get_value, a.span));
        body.push(self.setter(setter_key, a.computed, is_static, set_value, a.span));
    }
}
