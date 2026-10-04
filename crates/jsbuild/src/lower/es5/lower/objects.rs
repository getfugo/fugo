//! Object literals: shorthand properties, methods and computed keys.

use super::*;

/// A number property key as the string it names (for the keys oxc would print in brackets).
pub(super) fn number_key(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_owned()
    } else if value.is_infinite() {
        if value > 0.0 { "Infinity" } else { "-Infinity" }.to_owned()
    } else if value == 0.0 {
        "0".to_owned()
    } else {
        format!("{value}")
    }
}

/// Whether evaluating `e` has no side effects (a simple version of esbuild's
/// `ExprCanBeRemovedIfUnused`; an unbound identifier was already rejected by the check).
pub(super) fn side_effect_free(e: &Expression<'_>) -> bool {
    match e {
        Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BigIntLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::RegExpLiteral(_)
        | Expression::Identifier(_)
        | Expression::FunctionExpression(_)
        | Expression::ArrowFunctionExpression(_) => true,
        Expression::TemplateLiteral(t) => t.expressions.is_empty(),
        Expression::ArrayExpression(a) => a.elements.iter().all(|el| match el {
            ArrayExpressionElement::SpreadElement(_) => false,
            ArrayExpressionElement::Elision(_) => true,
            e => e.as_expression().is_some_and(side_effect_free),
        }),
        Expression::ObjectExpression(o) => o.properties.iter().all(|p| match p {
            ObjectPropertyKind::ObjectProperty(p) => {
                (!p.computed || p.key.as_expression().is_some_and(side_effect_free))
                    && (p.method || p.kind != PropertyKind::Init || side_effect_free(&p.value))
            }
            ObjectPropertyKind::SpreadProperty(_) => false,
        }),
        Expression::UnaryExpression(u) => side_effect_free(&u.argument),
        _ => false,
    }
}

impl<'a> Lowerer<'a> {
    /// Shorthand properties, methods, and keys oxc's code generator would print as ES2015;
    /// computed keys as a sequence of definitions.
    pub(super) fn lower_object(&mut self, expr: &mut Expression<'a>) {
        let Expression::ObjectExpression(obj) = expr else {
            return;
        };
        let ast = self.ast();
        let mut first_define = None;
        for (i, prop) in obj.properties.iter_mut().enumerate() {
            let ObjectPropertyKind::ObjectProperty(p) = prop else {
                continue; // object spread: reported by the verification
            };
            let proto_shorthand = p.shorthand
                && matches!(&p.key, PropertyKey::StaticIdentifier(k) if k.name == "__proto__");
            p.method = false;
            p.shorthand = false;
            if p.computed {
                let static_key = match &p.key {
                    PropertyKey::StringLiteral(s) => s.value != "__proto__",
                    PropertyKey::NumericLiteral(n) => n.value.is_finite() && n.value >= 0.0,
                    _ => false,
                };
                if static_key {
                    p.computed = false;
                } else if first_define.is_none() {
                    first_define = Some(i);
                }
            } else if proto_shorthand && first_define.is_none() {
                first_define = Some(i);
            }
            if !p.computed {
                // oxc prints `a: a` as `a`, and a negative or infinite number key in brackets.
                let replace = match &p.key {
                    PropertyKey::StaticIdentifier(k) => {
                        matches!(&p.value, Expression::Identifier(v) if v.name == k.name)
                            .then(|| k.name.as_str().to_owned())
                    }
                    PropertyKey::NumericLiteral(n) if !n.value.is_finite() || n.value < 0.0 => {
                        Some(number_key(n.value))
                    }
                    PropertyKey::BigIntLiteral(b) => Some(b.value.as_str().to_owned()),
                    _ => None,
                };
                if let Some(name) = replace {
                    let span = p.key.span();
                    p.key = PropertyKey::StringLiteral(ArenaBox::new_in(
                        StringLiteral::new(span, self.text(&name), None, &ast),
                        &ast,
                    ));
                }
            }
        }
        let Some(first) = first_define else {
            return;
        };
        let span = obj.span;
        let def_prop = self.helper(Helper::DefProp);
        let temp = self.temp("_o");
        let rest: Vec<ObjectPropertyKind<'a>> = obj.properties.drain(first..).collect();
        let prefix = expr.take_in(&ast);
        let mut seq = vec![self.assign(temp, prefix)];
        for prop in rest {
            let ObjectPropertyKind::ObjectProperty(p) = prop else {
                self.fail(
                    span.start,
                    "lower_to_es5: object spread after a computed key".to_owned(),
                );
                continue;
            };
            let p = p.unbox();
            let key_span = p.key.span();
            let is_proto = !p.computed
                && matches!(&p.key, PropertyKey::StaticIdentifier(k) if k.name == "__proto__");
            if is_proto
                && p.kind == PropertyKind::Init
                && !matches!(&p.value, Expression::Identifier(v) if v.name == "__proto__")
            {
                // `__proto__: v` sets the prototype.
                let target = SimpleAssignmentTarget::from(
                    MemberExpression::StaticMemberExpression(ArenaBox::new_in(
                        StaticMemberExpression::new(
                            SPAN,
                            self.ident(SPAN, temp),
                            IdentifierName::new(SPAN, self.id("__proto__"), &ast),
                            false,
                            &ast,
                        ),
                        &ast,
                    )),
                );
                seq.push(Expression::new_assignment_expression(
                    p.span,
                    AssignmentOperator::Assign,
                    AssignmentTarget::from(target),
                    p.value,
                    &ast,
                ));
                continue;
            }
            let key = match p.key {
                PropertyKey::StaticIdentifier(k) => self.string(key_span, k.name.as_str(), false),
                PropertyKey::PrivateIdentifier(_) => {
                    self.fail(
                        key_span.start,
                        "lower_to_es5: private name in an object literal".to_owned(),
                    );
                    continue;
                }
                k => k.into_expression(),
            };
            let flag = |name: &str| {
                ObjectPropertyKind::new_object_property(
                    SPAN,
                    PropertyKind::Init,
                    PropertyKey::StaticIdentifier(ArenaBox::new_in(
                        IdentifierName::new(SPAN, self.id(name), &ast),
                        &ast,
                    )),
                    Expression::new_boolean_literal(SPAN, true, &ast),
                    false,
                    false,
                    false,
                    &ast,
                )
            };
            let field = match p.kind {
                PropertyKind::Init => "value",
                PropertyKind::Get => "get",
                PropertyKind::Set => "set",
            };
            let mut descriptor = vec![ObjectPropertyKind::new_object_property(
                SPAN,
                PropertyKind::Init,
                PropertyKey::StaticIdentifier(ArenaBox::new_in(
                    IdentifierName::new(SPAN, self.id(field), &ast),
                    &ast,
                )),
                p.value,
                false,
                false,
                false,
                &ast,
            )];
            descriptor.push(flag("enumerable"));
            descriptor.push(flag("configurable"));
            if p.kind == PropertyKind::Init {
                descriptor.push(flag("writable"));
            }
            let descriptor = Expression::new_object_expression(
                SPAN,
                ArenaVec::from_iter_in(descriptor, &ast),
                &ast,
            );
            seq.push(self.call(
                p.span,
                self.ident(SPAN, def_prop),
                vec![self.ident(SPAN, temp), key, descriptor],
            ));
        }
        seq.push(self.ident(SPAN, temp));
        *expr = Expression::new_sequence_expression(span, ArenaVec::from_iter_in(seq, &ast), &ast);
    }
}
