//! The constructor: instance initializers inserted after `super()`.

use super::*;

impl<'a> Lowerer<'a, '_> {
    /// `insertInitializersIntoConstructor`: the instance initializers run in the constructor (one
    /// is added when the class has none), after `super()`, and the constructor moves first.
    pub(super) fn insert_initializers_into_constructor(
        &mut self,
        body: &mut Vec<ClassElement<'a>>,
        ctor: Option<usize>,
        derived: bool,
        out: &mut ClassOut<'a>,
    ) {
        if !out.call_instance_method_extra
            && out.instance_private_methods.is_empty()
            && out.instance_members.is_empty()
        {
            return;
        }
        let index = if let Some(i) = ctor {
            i
        } else {
            body.push(self.default_constructor(derived));
            body.len() - 1
        };
        let mut stmts = Vec::new();
        if let ClassElement::MethodDefinition(m) = &mut body[index] {
            // TypeScript parameter properties first, as esbuild orders them.
            for p in &mut m.value.params.items {
                if p.accessibility.is_some() || p.readonly || p.r#override {
                    p.accessibility = None;
                    p.readonly = false;
                    p.r#override = false;
                    if let BindingPattern::BindingIdentifier(id) = &p.pattern {
                        let name = id.name;
                        let target = self.member(self.this(), name.as_str());
                        let target = AssignmentTarget::from(target.into_member_expression());
                        let value = Expression::new_identifier(SPAN, name, &self.b);
                        stmts.push(self.expr_stmt(Expression::new_assignment_expression(
                            SPAN,
                            AssignmentOperator::Assign,
                            target,
                            value,
                            &self.b,
                        )));
                    }
                }
            }
        }
        if out.call_instance_method_extra {
            let init = out.init_ref.unwrap_or("_init");
            let call = self.call_helper(
                Helper::RunInitializers,
                vec![self.ident(init), self.number(5.0), self.this()],
            );
            stmts.push(self.expr_stmt(call));
        }
        stmts.append(&mut out.instance_private_methods);
        stmts.append(&mut out.instance_members);
        if let ClassElement::MethodDefinition(m) = &mut body[index]
            && let Some(fbody) = &mut m.value.body
        {
            self.insert_after_super(fbody, stmts, derived);
        }
        let ctor_el = body.remove(index);
        body.insert(0, ctor_el);
    }

    /// The constructor of a class that has none: `constructor() {}`, or
    /// `constructor() { super(...arguments); }` in a derived class.
    fn default_constructor(&mut self, derived: bool) -> ClassElement<'a> {
        let stmts = if derived {
            let spread = Argument::new_spread_element(SPAN, self.ident("arguments"), &self.b);
            let call = Expression::new_call_expression(
                SPAN,
                Expression::new_super(SPAN, &self.b),
                None,
                ArenaVec::from_iter_in([spread], &self.b),
                false,
                &self.b,
            );
            vec![self.expr_stmt(call)]
        } else {
            Vec::new()
        };
        let func = self.function(ArenaVec::new_in(&self.b), None, stmts);
        ClassElement::new_method_definition(
            SPAN,
            MethodDefinitionType::MethodDefinition,
            ArenaVec::new_in(&self.b),
            PropertyKey::new_static_identifier(SPAN, "constructor", &self.b),
            func,
            MethodDefinitionKind::Constructor,
            false,
            false,
            false,
            false,
            None,
            &self.b,
        )
    }

    /// `insertStmtsAfterSuperCall`: runs `stmts` once `this` exists.
    pub(super) fn insert_after_super(
        &mut self,
        body: &mut FunctionBody<'a>,
        stmts: Vec<Statement<'a>>,
        derived: bool,
    ) {
        if stmts.is_empty() {
            return;
        }
        if !derived {
            for (i, s) in stmts.into_iter().enumerate() {
                body.statements.insert(i, s);
            }
            return;
        }
        let mut count = SuperCalls::default();
        count.visit_function_body(body);
        if count.count == 0 {
            // The constructor never calls `super()`: instance fields are never initialized.
            return;
        }
        if count.count == 1 {
            for i in 0..body.statements.len() {
                let Statement::ExpressionStatement(es) = &mut body.statements[i] else {
                    continue;
                };
                let split = match &mut es.expression {
                    e if e.is_super_call_expression() => Some((Vec::new(), Vec::new())),
                    Expression::SequenceExpression(seq) => seq
                        .expressions
                        .iter()
                        .position(Expression::is_super_call_expression)
                        .map(|p| {
                            let exprs: Vec<Expression<'a>> =
                                seq.expressions.take_in(&self.b).into_iter().collect();
                            let mut before = exprs;
                            let after = before.split_off(p + 1);
                            (before, after)
                        }),
                    _ => None,
                };
                let Some((before, after)) = split else {
                    continue;
                };
                let mut new_stmts = Vec::new();
                if before.is_empty() {
                    // The statement is the call itself.
                    new_stmts.push(body.statements.remove(i));
                } else {
                    body.statements.remove(i);
                    new_stmts.push(self.expr_stmt(self.seq(before)));
                }
                new_stmts.extend(stmts);
                if !after.is_empty() {
                    new_stmts.push(self.expr_stmt(self.seq(after)));
                }
                for (j, s) in new_stmts.into_iter().enumerate() {
                    body.statements.insert(i + j, s);
                }
                return;
            }
        }
        // `var __super = (...args) => { super(...args); stmts; return this; };`
        let sup = self.fresh("__super");
        let args = self.fresh("args");
        let mut renamer = SuperCallRenamer {
            to: sup,
            b: self.alloc,
        };
        renamer.visit_function_body(body);
        let spread = Argument::new_spread_element(SPAN, self.ident(args), &self.b);
        let call = Expression::new_call_expression(
            SPAN,
            Expression::new_super(SPAN, &self.b),
            None,
            ArenaVec::from_iter_in([spread], &self.b),
            false,
            &self.b,
        );
        let mut inner = vec![self.expr_stmt(call)];
        inner.extend(stmts);
        inner.push(Statement::new_return_statement(
            SPAN,
            Some(self.this()),
            &self.b,
        ));
        let rest = FormalParameterRest::boxed(
            SPAN,
            ArenaVec::new_in(&self.b),
            BindingRestElement::new(
                SPAN,
                BindingPattern::new_binding_identifier(SPAN, args, &self.b),
                &self.b,
            ),
            None,
            &self.b,
        );
        let params = FormalParameters::boxed(
            SPAN,
            FormalParameterKind::ArrowFormalParameters,
            ArenaVec::new_in(&self.b),
            Some(rest),
            &self.b,
        );
        let arrow_body = FunctionBody::boxed(
            SPAN,
            ArenaVec::new_in(&self.b),
            ArenaVec::from_iter_in(inner, &self.b),
            &self.b,
        );
        let arrow = Expression::new_arrow_function_expression(
            SPAN,
            false,
            None,
            params,
            None,
            ArrowFunctionBody::FunctionBody(arrow_body),
            &self.b,
        );
        let decl = self.let_stmt(VariableDeclarationKind::Var, sup, arrow, SPAN);
        body.statements.insert(0, decl);
    }
}
