//! Functions: the top-level statements, prologues capturing `this` and `arguments`, parameters,
//! arrow functions and block-level functions.

use super::*;

impl<'a> Lowerer<'a> {
    /// The helpers, template caches and `import.meta` object for the top of the bundle.
    pub(super) fn top_statements(&mut self) -> Vec<Statement<'a>> {
        let mut out = Vec::new();
        let mut helpers: Vec<(Helper, Ident<'a>)> = self.helpers.drain().collect();
        helpers.sort_by_key(|(h, _)| *h);
        let names: HashMap<Helper, Ident<'a>> = helpers.iter().copied().collect();
        for (h, name) in &helpers {
            let mut text = h
                .source()
                .replace(&format!("${}", &h.base()[2..]), name.as_str());
            for (other, other_name) in &names {
                text = text.replace(&format!("${}", &other.base()[2..]), other_name.as_str());
            }
            out.extend(self.parse_statements(&text));
        }
        let mut vars: Vec<(Ident<'a>, Option<Expression<'a>>)> =
            self.caches.drain(..).map(|c| (c, None)).collect();
        if let Some(meta) = self.import_meta.take() {
            let ast = self.ast();
            vars.push((
                meta,
                Some(Expression::new_object_expression(
                    SPAN,
                    ArenaVec::new_in(&ast),
                    &ast,
                )),
            ));
        }
        if !vars.is_empty() {
            out.push(self.var_stmt(vars));
        }
        out
    }

    /// The statements a function body starts with: captured `this`/`arguments`, lowered
    /// parameters, temporaries.
    pub(super) fn prologue(
        &mut self,
        ctx: FnCtx<'a>,
        params: Option<&mut FormalParameters<'a>>,
    ) -> Vec<Statement<'a>> {
        let mut out = Vec::new();
        let mut captures = Vec::new();
        if ctx.this_used
            && let Some(name) = self.this_name
        {
            captures.push((
                name,
                Some(Expression::new_this_expression(SPAN, &self.ast())),
            ));
        }
        if ctx.args_used
            && let Some(name) = self.args_name
        {
            captures.push((name, Some(self.global("arguments"))));
        }
        if !captures.is_empty() {
            out.push(self.var_stmt(captures));
        }
        if let Some(params) = params {
            out.extend(self.lower_params(params));
        }
        if !ctx.temps.is_empty() {
            out.push(self.var_stmt(ctx.temps.into_iter().map(|t| (t, None)).collect()));
        }
        out
    }

    /// Default values and a rest parameter, as statements at the top of the body.
    pub(super) fn lower_params(&mut self, params: &mut FormalParameters<'a>) -> Vec<Statement<'a>> {
        let ast = self.ast();
        let mut out = Vec::new();
        params.kind = FormalParameterKind::FormalParameter;
        for item in &mut params.items {
            let Some(init) = item.initializer.take() else {
                continue;
            };
            let BindingPattern::BindingIdentifier(id) = &item.pattern else {
                // Destructuring is reported by the verification.
                item.initializer = Some(init);
                continue;
            };
            let name = id.name;
            let test = Expression::new_binary_expression(
                SPAN,
                self.ident(SPAN, name),
                BinaryOperator::StrictEquality,
                self.void0(SPAN),
                &ast,
            );
            let set =
                Statement::new_expression_statement(SPAN, self.assign(name, init.unbox()), &ast);
            out.push(Statement::new_if_statement(
                item.span, test, set, None, &ast,
            ));
        }
        if let Some(rest) = params.rest.take() {
            if let BindingPattern::BindingIdentifier(id) = &rest.rest.argument {
                let index = params.items.len();
                let slice = self.member(
                    self.member(self.member(self.global("Array"), "prototype"), "slice"),
                    "call",
                );
                #[allow(clippy::cast_precision_loss)]
                let start = Expression::new_numeric_literal(
                    SPAN,
                    index as f64,
                    None,
                    NumberBase::Decimal,
                    &ast,
                );
                let value = self.call(rest.span, slice, vec![self.global("arguments"), start]);
                out.push(self.var_stmt(vec![(id.name, Some(value))]));
            } else {
                params.rest = Some(rest);
            }
        }
        out
    }

    pub(super) fn lower_arrow(
        &mut self,
        arrow: ArenaBox<'a, ArrowFunctionExpression<'a>>,
        ctx: FnCtx<'a>,
    ) -> Expression<'a> {
        let ast = self.ast();
        let ArrowFunctionExpression {
            span,
            mut params,
            body,
            ..
        } = arrow.unbox();
        let mut body = match body {
            ArrowFunctionBody::FunctionBody(b) => b,
            body => {
                let expr = body.into_expression();
                let expr_span = expr.span();
                let ret = Statement::new_return_statement(expr_span, Some(expr), &ast);
                ArenaBox::new_in(
                    FunctionBody::new(
                        expr_span,
                        ArenaVec::new_in(&ast),
                        ArenaVec::from_array_in([ret], &ast),
                        &ast,
                    ),
                    &ast,
                )
            }
        };
        let prologue = self.prologue(ctx, Some(&mut params));
        body.statements.splice(0..0, prologue);
        self.function_expr(span, params, body)
    }

    /// Block-level function declarations of `stmts` (in strict code) as `var f = function`
    /// statements, removed from `stmts`.
    pub(super) fn take_block_fns(
        &self,
        stmts: &mut ArenaVec<'a, Statement<'a>>,
    ) -> Vec<Statement<'a>> {
        let ast = self.ast();
        let mut out = Vec::new();
        let mut i = 0;
        while i < stmts.len() {
            let is_block_fn = matches!(&stmts[i], Statement::FunctionDeclaration(f)
                if f.id.as_ref().and_then(|id| id.symbol_id.get()).is_some_and(|s| self.block_fns.contains(&s)));
            if !is_block_fn {
                i += 1;
                continue;
            }
            let Statement::FunctionDeclaration(f) = stmts.remove(i) else {
                continue;
            };
            let mut f = f.unbox();
            let Some(id) = f.id.take() else { continue };
            f.r#type = FunctionType::FunctionExpression;
            let value = Expression::FunctionExpression(ArenaBox::new_in(f, &ast));
            let decl = VariableDeclarator::new(
                id.span,
                BindingPattern::BindingIdentifier(ArenaBox::new_in(id, &ast)),
                None,
                Some(value),
                false,
                &ast,
            );
            out.push(Statement::new_variable_declaration(
                SPAN,
                VariableDeclarationKind::Var,
                ArenaVec::from_array_in([decl], &ast),
                false,
                &ast,
            ));
        }
        out
    }
}
