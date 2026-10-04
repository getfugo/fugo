//! The traversal that lowers each node.

use super::*;

/// Empties the spans of parsed helper code, which has no position in the input.
pub(super) struct ZeroSpans;

impl<'a> VisitMut<'a> for ZeroSpans {
    fn visit_span(&mut self, span: &mut Span) {
        *span = SPAN;
    }
}

impl<'a> VisitMut<'a> for Lowerer<'a> {
    fn visit_program(&mut self, it: &mut Program<'a>) {
        self.ctxs.push(FnCtx {
            kind: CtxKind::Program,
            ..FnCtx::default()
        });
        walk_mut::walk_program(self, it);
        let ctx = self.ctxs.pop().unwrap_or_default();
        // What the IIFE wrapper did not get (all of it without one) goes at the top.
        let mut top = self.top_statements();
        top.extend(self.prologue(ctx, None));
        it.body.splice(0..0, top);
        // `export * as ns from "x"` (ES2020): `import * as ns2 from "x"; export { ns2 as ns }`.
        let mut i = 0;
        while i < it.body.len() {
            if let Statement::ExportAllDeclaration(e) = &it.body[i]
                && e.exported.is_some()
            {
                let Statement::ExportAllDeclaration(e) = it.body.remove(i) else {
                    continue;
                };
                let e = e.unbox();
                let (import, export) = self.split_export_star_as(e);
                it.body.insert(i, export);
                it.body.insert(i, import);
                i += 2;
                continue;
            }
            i += 1;
        }
    }

    fn visit_function(&mut self, it: &mut Function<'a>, flags: ScopeFlags) {
        self.ctxs.push(FnCtx::default());
        walk_mut::walk_function(self, it, flags);
        let ctx = self.ctxs.pop().unwrap_or_default();
        let wrapper = self.wrapper == Some(it.span) && !self.top_done;
        let mut prologue = Vec::new();
        if wrapper {
            prologue = self.top_statements();
            self.top_done = true;
        }
        prologue.extend(self.prologue(ctx, Some(&mut it.params)));
        if let Some(body) = &mut it.body {
            body.statements.splice(0..0, prologue);
        }
    }

    fn visit_expression(&mut self, expr: &mut Expression<'a>) {
        let arrow = matches!(expr, Expression::ArrowFunctionExpression(_));
        if arrow {
            if self.this_name.is_none() {
                let name = self.fresh("_this");
                self.this_name = Some(name);
                let args = self.fresh("_arguments");
                self.args_name = Some(args);
            }
            self.ctxs.push(FnCtx {
                kind: CtxKind::Arrow,
                ..FnCtx::default()
            });
        }
        walk_mut::walk_expression(self, expr);
        let ast = self.ast();
        match expr {
            Expression::ArrowFunctionExpression(_) => {
                let ctx = self.ctxs.pop().unwrap_or_default();
                let Expression::ArrowFunctionExpression(a) = expr.take_in(&ast) else {
                    return;
                };
                *expr = self.lower_arrow(a, ctx);
            }
            Expression::ThisExpression(t) => {
                if let Some(owner) = self.arrow_owner()
                    && let Some(name) = self.this_name
                {
                    self.ctxs[owner].this_used = true;
                    *expr = self.ident(t.span, name);
                }
            }
            Expression::TemplateLiteral(_) => {
                let Expression::TemplateLiteral(t) = expr.take_in(&ast) else {
                    return;
                };
                *expr = self.lower_template(t.unbox());
            }
            Expression::TaggedTemplateExpression(_) => {
                let Expression::TaggedTemplateExpression(t) = expr.take_in(&ast) else {
                    return;
                };
                *expr = self.lower_tagged(t);
            }
            Expression::ObjectExpression(_) => self.lower_object(expr),
            Expression::ArrayExpression(a)
                if a.elements
                    .iter()
                    .any(|e| matches!(e, ArrayExpressionElement::SpreadElement(_))) =>
            {
                let span = a.span;
                let items = a
                    .elements
                    .drain(..)
                    .map(|e| match e {
                        ArrayExpressionElement::SpreadElement(s) => {
                            (true, Some(s.unbox().argument))
                        }
                        ArrayExpressionElement::Elision(_) => (false, None),
                        e => (false, Some(e.into_expression())),
                    })
                    .collect();
                *expr = self.spread_array(span, items);
            }
            Expression::CallExpression(c)
                if c.arguments
                    .iter()
                    .any(|a| matches!(a, Argument::SpreadElement(_))) =>
            {
                self.lower_spread_call(expr);
            }
            Expression::NewExpression(n)
                if n.arguments
                    .iter()
                    .any(|a| matches!(a, Argument::SpreadElement(_))) =>
            {
                self.lower_spread_new(expr);
            }
            Expression::RegExpLiteral(r) => {
                if let Some(e) = self.lower_regexp(r) {
                    *expr = e;
                }
            }
            Expression::BigIntLiteral(b) => *expr = self.lower_bigint(b),
            Expression::ImportMeta(m) => {
                let span = m.span;
                let name = match self.import_meta {
                    Some(n) => n,
                    None => {
                        let n = self.fresh("import_meta");
                        self.import_meta = Some(n);
                        n
                    }
                };
                *expr = self.ident(span, name);
            }
            Expression::ImportExpression(_) => {
                let Expression::ImportExpression(i) = expr.take_in(&ast) else {
                    return;
                };
                *expr = self.lower_import(i);
            }
            _ => {}
        }
    }

    fn visit_identifier_reference(&mut self, it: &mut IdentifierReference<'a>) {
        let symbol = it
            .reference_id
            .get()
            .and_then(|r| self.scoping.get_reference(r).symbol_id());
        if let Some(s) = symbol {
            if let Some(&name) = self.renames.get(&s) {
                it.name = name;
            }
        } else if it.name == "arguments"
            && let Some(owner) = self.arrow_owner()
            && self.ctxs[owner].kind != CtxKind::Program
            && let Some(name) = self.args_name
        {
            self.ctxs[owner].args_used = true;
            it.name = name;
        }
    }

    fn visit_binding_identifier(&mut self, it: &mut BindingIdentifier<'a>) {
        if let Some(s) = it.symbol_id.get()
            && let Some(&name) = self.renames.get(&s)
        {
            it.name = name;
        }
    }

    fn visit_variable_declaration(&mut self, it: &mut VariableDeclaration<'a>) {
        let for_left = std::mem::take(&mut self.in_for_left);
        walk_mut::walk_variable_declaration(self, it);
        match it.kind {
            VariableDeclarationKind::Let | VariableDeclarationKind::Const => {
                it.kind = VariableDeclarationKind::Var;
                if !for_left {
                    // A block-level `let x;` is `undefined` each time its block runs.
                    for d in &mut it.declarations {
                        if d.init.is_some() {
                            continue;
                        }
                        let block_level = d.id.get_binding_identifiers().iter().any(|id| {
                            id.symbol_id.get().is_some_and(|s| {
                                let scope = self.scoping.symbol_scope_id(s);
                                var_scope(&self.scoping, scope) != scope
                            })
                        });
                        if block_level {
                            d.init = Some(self.void0(SPAN));
                        }
                    }
                }
            }
            VariableDeclarationKind::Using | VariableDeclarationKind::AwaitUsing => {
                self.fail(
                    it.span.start,
                    "lower_to_es5: `using` declarations remain in the bundle".to_owned(),
                );
            }
            VariableDeclarationKind::Var => {}
        }
    }

    fn visit_for_in_statement(&mut self, it: &mut ForInStatement<'a>) {
        self.in_for_left = matches!(it.left, ForStatementLeft::VariableDeclaration(_));
        self.visit_for_statement_left(&mut it.left);
        self.in_for_left = false;
        self.visit_expression(&mut it.right);
        self.visit_statement(&mut it.body);
    }

    fn visit_for_of_statement(&mut self, it: &mut ForOfStatement<'a>) {
        self.in_for_left = matches!(it.left, ForStatementLeft::VariableDeclaration(_));
        self.visit_for_statement_left(&mut it.left);
        self.in_for_left = false;
        self.visit_expression(&mut it.right);
        self.visit_statement(&mut it.body);
    }

    fn visit_catch_clause(&mut self, it: &mut CatchClause<'a>) {
        walk_mut::walk_catch_clause(self, it);
        if it.param.is_none() {
            let ast = self.ast();
            let name = self.fresh("_unused");
            it.param = Some(CatchParameter::new(
                SPAN,
                BindingPattern::new_binding_identifier(SPAN, name, &ast),
                None,
                &ast,
            ));
        }
    }

    fn visit_block_statement(&mut self, it: &mut BlockStatement<'a>) {
        walk_mut::walk_block_statement(self, it);
        let hoisted = self.take_block_fns(&mut it.body);
        it.body.splice(0..0, hoisted);
    }

    fn visit_statements(&mut self, it: &mut ArenaVec<'a, Statement<'a>>) {
        walk_mut::walk_statements(self, it);
        // Block-level functions of a switch: `var f = function` before it, in a block.
        let ast = self.ast();
        for stmt in it.iter_mut() {
            let Statement::SwitchStatement(sw) = stmt else {
                continue;
            };
            let mut hoisted = Vec::new();
            for case in &mut sw.cases {
                hoisted.extend(self.take_block_fns(&mut case.consequent));
            }
            if hoisted.is_empty() {
                continue;
            }
            let switch = stmt.take_in(&ast);
            hoisted.push(switch);
            *stmt =
                Statement::new_block_statement(SPAN, ArenaVec::from_iter_in(hoisted, &ast), &ast);
        }
    }
}
