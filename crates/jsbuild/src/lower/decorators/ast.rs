//! Building the AST of the lowered code, and importing the helpers it calls.

use super::*;

impl<'a> Lowerer<'a, '_> {
    pub(super) fn ident(&self, name: &'a str) -> Expression<'a> {
        Expression::new_identifier(SPAN, name, &self.b)
    }

    pub(super) fn string(&self, s: &str) -> Expression<'a> {
        Expression::new_string_literal(SPAN, self.arena_str(s), None, &self.b)
    }

    pub(super) fn number(&self, n: f64) -> Expression<'a> {
        Expression::new_numeric_literal(SPAN, n, None, NumberBase::Decimal, &self.b)
    }

    pub(super) fn null(&self) -> Expression<'a> {
        Expression::new_null_literal(SPAN, &self.b)
    }

    pub(super) fn void0(&self) -> Expression<'a> {
        Expression::new_void_0(SPAN, &self.b)
    }

    pub(super) fn this(&self) -> Expression<'a> {
        Expression::new_this_expression(SPAN, &self.b)
    }

    pub(super) fn call(&self, callee: Expression<'a>, args: Vec<Expression<'a>>) -> Expression<'a> {
        let args = ArenaVec::from_iter_in(args.into_iter().map(Argument::from), &self.b);
        Expression::new_call_expression(SPAN, callee, None, args, false, &self.b)
    }

    pub(super) fn helper(&mut self, helper: Helper) -> Expression<'a> {
        let local = if let Some(local) = self.helpers.get(&helper) {
            *local
        } else {
            let local = self.fresh(helper.name());
            self.helpers.insert(helper, local);
            local
        };
        self.ident(local)
    }

    pub(super) fn call_helper(
        &mut self,
        helper: Helper,
        args: Vec<Expression<'a>>,
    ) -> Expression<'a> {
        let callee = self.helper(helper);
        self.call(callee, args)
    }

    pub(super) fn member(&self, object: Expression<'a>, name: &'a str) -> Expression<'a> {
        Expression::new_static_member_expression(
            SPAN,
            object,
            IdentifierName::new(SPAN, name, &self.b),
            false,
            &self.b,
        )
    }

    pub(super) fn assign(&self, name: &'a str, value: Expression<'a>) -> Expression<'a> {
        Expression::new_assignment_expression(
            SPAN,
            AssignmentOperator::Assign,
            AssignmentTarget::new_assignment_target_identifier(SPAN, name, &self.b),
            value,
            &self.b,
        )
    }

    /// `a, b, c` (flattening nested sequences); a single expression stays as it is.
    pub(super) fn seq(&self, exprs: Vec<Expression<'a>>) -> Expression<'a> {
        let mut flat = Vec::with_capacity(exprs.len());
        for e in exprs {
            match e {
                Expression::SequenceExpression(s) => flat.extend(s.unbox().expressions),
                e => flat.push(e),
            }
        }
        if flat.len() == 1 {
            return flat.pop().unwrap_or_else(|| self.void0());
        }
        Expression::new_sequence_expression(SPAN, ArenaVec::from_iter_in(flat, &self.b), &self.b)
    }

    pub(super) fn array(&self, items: Vec<Expression<'a>>) -> Expression<'a> {
        let items =
            ArenaVec::from_iter_in(items.into_iter().map(ArrayExpressionElement::from), &self.b);
        Expression::new_array_expression(SPAN, items, &self.b)
    }

    pub(super) fn expr_stmt(&self, e: Expression<'a>) -> Statement<'a> {
        Statement::new_expression_statement(SPAN, e, &self.b)
    }

    pub(super) fn new_weak(&mut self, class: &'static str, span: Span) -> Expression<'a> {
        if self.shadowed_weak.contains(class) {
            self.error(
                span,
                format!("a binding named {class} shadows the global that lowered decorators need"),
            );
        }
        Expression::new_new_expression(
            SPAN,
            self.ident(class),
            None,
            ArenaVec::new_in(&self.b),
            &self.b,
        )
    }

    /// A function with the given parameters and statements.
    pub(super) fn function(
        &self,
        params: ArenaVec<'a, FormalParameter<'a>>,
        rest: Option<ArenaBox<'a, FormalParameterRest<'a>>>,
        stmts: Vec<Statement<'a>>,
    ) -> ArenaBox<'a, Function<'a>> {
        let params = FormalParameters::boxed(
            SPAN,
            FormalParameterKind::UniqueFormalParameters,
            params,
            rest,
            &self.b,
        );
        let body = FunctionBody::boxed(
            SPAN,
            ArenaVec::new_in(&self.b),
            ArenaVec::from_iter_in(stmts, &self.b),
            &self.b,
        );
        Function::boxed(
            SPAN,
            FunctionType::FunctionExpression,
            None,
            false,
            false,
            false,
            None,
            None,
            params,
            None,
            Some(body),
            &self.b,
        )
    }

    /// `(() => { stmts })()`.
    pub(super) fn arrow_iife(&self, stmts: ArenaVec<'a, Statement<'a>>) -> Expression<'a> {
        let params = FormalParameters::boxed(
            SPAN,
            FormalParameterKind::ArrowFormalParameters,
            ArenaVec::new_in(&self.b),
            None,
            &self.b,
        );
        let body = FunctionBody::boxed(SPAN, ArenaVec::new_in(&self.b), stmts, &self.b);
        let arrow = Expression::new_arrow_function_expression(
            SPAN,
            false,
            None,
            params,
            None,
            ArrowFunctionBody::FunctionBody(body),
            &self.b,
        );
        self.call(arrow, Vec::new())
    }

    /// A copy of a side-effect-free expression (`this` or an identifier), else `None`.
    pub(super) fn clone_simple(&self, e: &Expression<'a>) -> Option<Expression<'a>> {
        match e {
            Expression::ThisExpression(_) => Some(self.this()),
            Expression::Identifier(id) => Some(match id.reference_id.get() {
                Some(r) => Expression::new_identifier_with_reference_id(SPAN, id.name, r, &self.b),
                None => Expression::new_identifier(SPAN, id.name, &self.b),
            }),
            _ => None,
        }
    }

    /// `e` and a reference to its value: itself twice when it is simple, else `(_a = e)`, `_a`.
    pub(super) fn capture(&mut self, e: Expression<'a>) -> (Expression<'a>, Expression<'a>) {
        if let Some(copy) = self.clone_simple(&e) {
            return (e, copy);
        }
        let t = self.temp();
        (self.assign(t, e), self.ident(t))
    }

    /// Adds the import (or `require`) of the helpers the lowered code uses.
    pub(super) fn insert_helpers(
        &mut self,
        program: &mut Program<'a>,
        specifier: &str,
        import: bool,
    ) {
        if self.helpers.is_empty() {
            return;
        }
        let specifier = self.arena_str(specifier);
        let stmt = if import {
            let specs = ArenaVec::from_iter_in(
                self.helpers.iter().map(|(h, local)| {
                    ImportDeclarationSpecifier::new_import_specifier(
                        SPAN,
                        ModuleExportName::new_identifier_name(SPAN, h.name(), &self.b),
                        BindingIdentifier::new(SPAN, *local, &self.b),
                        ImportOrExportKind::Value,
                        &self.b,
                    )
                }),
                &self.b,
            );
            Statement::new_import_declaration(
                SPAN,
                Some(specs),
                StringLiteral::new(SPAN, specifier, None, &self.b),
                None,
                None,
                ImportOrExportKind::Value,
                &self.b,
            )
        } else {
            let props = ArenaVec::from_iter_in(
                self.helpers.iter().map(|(h, local)| {
                    BindingProperty::new(
                        SPAN,
                        PropertyKey::new_static_identifier(SPAN, h.name(), &self.b),
                        BindingPattern::new_binding_identifier(SPAN, *local, &self.b),
                        h.name() == *local,
                        false,
                        &self.b,
                    )
                }),
                &self.b,
            );
            let pattern = BindingPattern::new_object_pattern(SPAN, props, None, &self.b);
            let require = self.call(self.ident("require"), vec![self.string(specifier)]);
            let decl = VariableDeclarator::new(SPAN, pattern, None, Some(require), false, &self.b);
            Statement::new_variable_declaration(
                SPAN,
                VariableDeclarationKind::Var,
                ArenaVec::from_iter_in([decl], &self.b),
                false,
                &self.b,
            )
        };
        program.body.insert(0, stmt);
    }
}
