//! A lowered class in its place: class statements, `export default` classes and class expressions.

use super::*;

impl<'a> Lowerer<'a, '_> {
    /// Lowers a decorated class statement (after its body was visited) into statements.
    pub(super) fn lower_class_statement(
        &mut self,
        mut class: ArenaBox<'a, Class<'a>>,
        kind: ClassKind,
        span: Span,
    ) -> Vec<Statement<'a>> {
        self.check_member_decorators(&class);
        let plan = self.plan_class(&class, kind);
        self.visit_class_parts(&mut class, Some(&plan));
        let LoweredClass {
            class_decorators,
            chain,
            mut class,
            suffix,
        } = self.lower_class(class, &plan);
        let mut stmts: Vec<Statement<'a>> = Vec::new();
        let rename = plan.capture.then_some(plan.symbol).flatten();
        // Class decorators see the outer binding of the class, the rest its captured value.
        if let Some(e) = class_decorators {
            stmts.push(self.expr_stmt(e));
        }
        for mut e in chain {
            if let Some(symbol) = rename {
                self.rename_refs(&mut e, symbol, plan.class_ref);
            }
            stmts.push(self.expr_stmt(e));
        }
        if plan.capture {
            // `class Foo {}` becomes `let _Foo = class Foo {}`: the class keeps its name, its own
            // references see the class itself, and code moved out uses `_Foo`.
            class.r#type = ClassType::ClassExpression;
            class.r#abstract = false;
            class.body.body.retain(|e| !is_abstract_element(e));
            let class_expr = Expression::ClassExpression(class);
            let decl_kind = if plan.has_class_decorators {
                VariableDeclarationKind::Let
            } else {
                VariableDeclarationKind::Const
            };
            stmts.push(self.let_stmt(decl_kind, plan.class_ref, class_expr, span));
        } else {
            class.r#type = ClassType::ClassDeclaration;
            if class.id.is_none() {
                class.id = Some(BindingIdentifier::new(SPAN, plan.class_ref, &self.b));
            }
            stmts.push(match kind {
                ClassKind::ExportStmt => Statement::new_export_declaration(
                    span,
                    Declaration::ClassDeclaration(class),
                    &self.b,
                ),
                ClassKind::ExportDefault if plan.symbol.is_some() => {
                    Statement::ExportDefaultDeclaration(ExportDefaultDeclaration::boxed(
                        span,
                        ExportDefaultDeclarationKind::ClassDeclaration(class),
                        &self.b,
                    ))
                }
                _ => Statement::ClassDeclaration(class),
            });
        }
        for mut e in suffix {
            if let Some(symbol) = rename {
                self.rename_refs(&mut e, symbol, plan.class_ref);
            }
            stmts.push(self.expr_stmt(e));
        }
        if plan.capture {
            let value = self.ident(plan.class_ref);
            let name = self.arena_str(&plan.name);
            match kind {
                ClassKind::ExportStmt => {
                    // oxc's namespace transform only supports exported `const`s.
                    let decl_kind = if self.namespace_depth > 0 {
                        VariableDeclarationKind::Const
                    } else {
                        VariableDeclarationKind::Let
                    };
                    let decl = self.let_decl(decl_kind, name, value);
                    stmts.push(Statement::new_export_declaration(SPAN, decl, &self.b));
                }
                ClassKind::ExportDefault => {
                    stmts.push(self.let_stmt(VariableDeclarationKind::Let, name, value, SPAN));
                    stmts.push(self.export_default_name(name));
                }
                _ => stmts.push(self.let_stmt(VariableDeclarationKind::Let, name, value, SPAN)),
            }
        } else if kind == ClassKind::ExportDefault && plan.symbol.is_none() {
            stmts.push(self.export_default_name(plan.class_ref));
        }
        stmts
    }

    pub(super) fn let_decl(
        &self,
        kind: VariableDeclarationKind,
        name: &'a str,
        value: Expression<'a>,
    ) -> Declaration<'a> {
        let decl = VariableDeclarator::new(
            SPAN,
            BindingPattern::new_binding_identifier(SPAN, name, &self.b),
            None,
            Some(value),
            false,
            &self.b,
        );
        Declaration::new_variable_declaration(
            SPAN,
            kind,
            ArenaVec::from_iter_in([decl], &self.b),
            false,
            &self.b,
        )
    }

    pub(super) fn let_stmt(
        &self,
        kind: VariableDeclarationKind,
        name: &'a str,
        value: Expression<'a>,
        span: Span,
    ) -> Statement<'a> {
        let decl = VariableDeclarator::new(
            SPAN,
            BindingPattern::new_binding_identifier(SPAN, name, &self.b),
            None,
            Some(value),
            false,
            &self.b,
        );
        Statement::new_variable_declaration(
            span,
            kind,
            ArenaVec::from_iter_in([decl], &self.b),
            false,
            &self.b,
        )
    }

    pub(super) fn export_default_name(&self, local: &'a str) -> Statement<'a> {
        let spec = ExportSpecifier::new(
            SPAN,
            ModuleExportName::new_identifier_reference(SPAN, local, &self.b),
            ModuleExportName::new_identifier_name(SPAN, "default", &self.b),
            ImportOrExportKind::Value,
            &self.b,
        );
        Statement::new_export_named_declaration(
            SPAN,
            ArenaVec::from_iter_in([spec], &self.b),
            ImportOrExportKind::Value,
            &self.b,
        )
    }

    /// Lowers a decorated class expression (in `expr`) into a sequence expression.
    pub(super) fn lower_class_expression(&mut self, expr: &mut Expression<'a>) {
        let Expression::ClassExpression(class) = expr.take_in(&self.b) else {
            return;
        };
        let mut class = class;
        self.check_member_decorators(&class);
        let plan = self.plan_class(&class, ClassKind::Expr);
        self.visit_class_parts(&mut class, Some(&plan));
        let LoweredClass {
            class_decorators,
            chain,
            mut class,
            suffix,
        } = self.lower_class(class, &plan);
        let anonymous = class.id.is_none();
        if anonymous
            && !plan.name.is_empty()
            && is_identifier_name(&plan.name)
            && !is_reserved_keyword(&plan.name)
        {
            // Name the class as its context would have (`const Foo = class {}`), unless that
            // name would capture a reference inside the class.
            let mut scan = NameScan {
                name: &plan.name,
                found: false,
            };
            scan.visit_class(&class);
            if !scan.found {
                class.id = Some(BindingIdentifier::new(
                    SPAN,
                    self.arena_str(&plan.name),
                    &self.b,
                ));
            }
        }
        let mut class_expr = Expression::ClassExpression(class);
        if let Some(symbol) = plan.symbol {
            self.rename_refs(&mut class_expr, symbol, plan.class_ref);
        }
        if matches!(&class_expr, Expression::ClassExpression(c) if c.id.is_none()) {
            // `_a = (0, class {})`: an anonymous class must not be named `_a`.
            class_expr = self.seq(vec![self.number(0.0), class_expr]);
            class_expr = Expression::new_parenthesized_expression(SPAN, class_expr, &self.b);
        }
        let mut exprs: Vec<Expression<'a>> = class_decorators.into_iter().collect();
        for mut e in chain {
            if let Some(symbol) = plan.symbol {
                self.rename_refs(&mut e, symbol, plan.class_ref);
            }
            exprs.push(e);
        }
        exprs.push(self.assign(plan.class_ref, class_expr));
        for mut e in suffix {
            if let Some(symbol) = plan.symbol {
                self.rename_refs(&mut e, symbol, plan.class_ref);
            }
            exprs.push(e);
        }
        exprs.push(self.ident(plan.class_ref));
        *expr = self.seq(exprs);
        if !matches!(expr, Expression::SequenceExpression(_)) {
            return;
        }
        *expr = Expression::new_parenthesized_expression(SPAN, expr.take_in(&self.b), &self.b);
    }

    pub(super) fn rename_refs(&self, e: &mut Expression<'a>, symbol: SymbolId, to: &'a str) {
        let mut r = Renamer {
            scoping: self.scoping,
            symbol,
            to,
        };
        r.visit_expression(e);
    }
}
