//! Building the AST of the lowered code: names, helpers, temporaries and nodes.

use super::*;

impl<'a> Lowerer<'a> {
    pub(super) fn ast(&self) -> AstBuilder<'a> {
        AstBuilder::new(self.allocator)
    }

    pub(super) fn id(&self, name: &str) -> Ident<'a> {
        Ident::from_str_in(name, &self.ast())
    }

    pub(super) fn text(&self, value: &str) -> Str<'a> {
        Str::from_str_in(value, &self.ast())
    }

    pub(super) fn fail(&mut self, pos: u32, message: String) {
        if self.error.is_none() {
            self.error = Some((pos, message));
        }
    }

    pub(super) fn fresh(&mut self, base: &str) -> Ident<'a> {
        let name = fresh_name(&mut self.used, base);
        self.id(&name)
    }

    pub(super) fn helper(&mut self, h: Helper) -> Ident<'a> {
        if let Some(&n) = self.helpers.get(&h) {
            return n;
        }
        for &d in h.deps() {
            self.helper(d);
        }
        let name = self.fresh(h.base());
        self.helpers.insert(h, name);
        name
    }

    /// A temporary in the current function.
    pub(super) fn temp(&mut self, base: &str) -> Ident<'a> {
        let name = self.fresh(base);
        if let Some(ctx) = self.ctxs.last_mut() {
            ctx.temps.push(name);
        }
        name
    }

    /// The index of the function whose `this` an arrow function at the current position sees,
    /// when inside an arrow function.
    pub(super) fn arrow_owner(&self) -> Option<usize> {
        if !self.ctxs.last().is_some_and(|c| c.kind == CtxKind::Arrow) {
            return None;
        }
        self.ctxs.iter().rposition(|c| c.kind != CtxKind::Arrow)
    }

    pub(super) fn ident(&self, span: Span, name: Ident<'a>) -> Expression<'a> {
        Expression::new_identifier(span, name, &self.ast())
    }

    pub(super) fn global(&self, name: &str) -> Expression<'a> {
        Expression::new_identifier(SPAN, self.id(name), &self.ast())
    }

    pub(super) fn string(&self, span: Span, value: &str, lone_surrogates: bool) -> Expression<'a> {
        Expression::new_string_literal_with_lone_surrogates(
            span,
            self.text(value),
            None,
            lone_surrogates,
            &self.ast(),
        )
    }

    pub(super) fn member(&self, object: Expression<'a>, property: &str) -> Expression<'a> {
        let ast = self.ast();
        let property = IdentifierName::new(SPAN, self.id(property), &ast);
        Expression::new_static_member_expression(SPAN, object, property, false, &ast)
    }

    pub(super) fn call(
        &self,
        span: Span,
        callee: Expression<'a>,
        args: Vec<Expression<'a>>,
    ) -> Expression<'a> {
        let ast = self.ast();
        let args = ArenaVec::from_iter_in(args.into_iter().map(Argument::from), &ast);
        Expression::new_call_expression(span, callee, None, args, false, &ast)
    }

    pub(super) fn assign(&self, target: Ident<'a>, value: Expression<'a>) -> Expression<'a> {
        let ast = self.ast();
        let target = AssignmentTarget::from(SimpleAssignmentTarget::AssignmentTargetIdentifier(
            ArenaBox::new_in(IdentifierReference::new(SPAN, target, &ast), &ast),
        ));
        Expression::new_assignment_expression(SPAN, AssignmentOperator::Assign, target, value, &ast)
    }

    pub(super) fn void0(&self, span: Span) -> Expression<'a> {
        Expression::new_void_0(span, &self.ast())
    }

    pub(super) fn var_stmt(
        &self,
        decls: Vec<(Ident<'a>, Option<Expression<'a>>)>,
    ) -> Statement<'a> {
        let ast = self.ast();
        let decls = ArenaVec::from_iter_in(
            decls.into_iter().map(|(name, init)| {
                VariableDeclarator::new(
                    SPAN,
                    BindingPattern::new_binding_identifier(SPAN, name, &ast),
                    None,
                    init,
                    false,
                    &ast,
                )
            }),
            &ast,
        );
        Statement::new_variable_declaration(SPAN, VariableDeclarationKind::Var, decls, false, &ast)
    }

    pub(super) fn function_expr(
        &self,
        span: Span,
        params: ArenaBox<'a, FormalParameters<'a>>,
        body: ArenaBox<'a, FunctionBody<'a>>,
    ) -> Expression<'a> {
        Expression::new_function_expression(
            span,
            FunctionType::FunctionExpression,
            None,
            false,
            false,
            false,
            None::<ArenaBox<'a, TSTypeParameterDeclaration<'a>>>,
            None::<ArenaBox<'a, TSThisParameter<'a>>>,
            params,
            None::<ArenaBox<'a, TSTypeAnnotation<'a>>>,
            Some(body),
            &self.ast(),
        )
    }

    /// Statements parsed from helper source, with empty spans (no source mappings).
    pub(super) fn parse_statements(&self, text: &str) -> ArenaVec<'a, Statement<'a>> {
        let text = self.allocator.alloc_str(text);
        let ret = Parser::new(self.allocator, text, SourceType::cjs()).parse();
        let mut program = ret.program;
        ZeroSpans.visit_program(&mut program);
        program.body
    }
}
