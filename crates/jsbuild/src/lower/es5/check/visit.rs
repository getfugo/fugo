//! The checker's traversal of the JavaScript parts of the AST.

use super::*;

impl<'a> VisitJs<'a> for Checker<'a, '_> {
    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) {
        Self::astral(&mut self.astral_decls, &it.name, it.span.start);
    }

    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        Self::astral(&mut self.astral_refs, &it.name, it.span.start);
    }

    fn visit_export_specifier(&mut self, it: &ExportSpecifier<'a>) {
        if let ModuleExportName::IdentifierName(n) = &it.exported {
            Self::astral(&mut self.astral_decls, &n.name, n.span.start);
        }
        walk_js::walk_export_specifier(self, it);
    }

    fn visit_variable_declaration(&mut self, it: &VariableDeclaration<'a>) {
        match it.kind {
            VariableDeclarationKind::Let | VariableDeclarationKind::Const => {
                let pos = self.scan.skip_words(it.span.start, &["export", "declare"]);
                let word = self.scan.word_at(pos);
                if matches!(word, "let" | "const") {
                    self.err(pos, transforming(word));
                }
            }
            VariableDeclarationKind::AwaitUsing => {
                let pos = self.scan.skip_trivia(it.span.start);
                self.top_level_await(pos);
            }
            _ => {}
        }
        walk_js::walk_variable_declaration(self, it);
    }

    fn visit_ts_enum_declaration(&mut self, it: &TSEnumDeclaration<'a>) {
        if it.r#const {
            let pos = self.scan.skip_words(it.span.start, &["export", "declare"]);
            if self.scan.word_at(pos) == "const" {
                self.err(pos, transforming("const"));
            }
        }
        walk_js::walk_ts_enum_declaration(self, it);
    }

    fn visit_object_pattern(&mut self, it: &ObjectPattern<'a>) {
        self.feature(Some(it.span.start), "destructuring");
        walk_js::walk_object_pattern(self, it);
    }

    fn visit_array_pattern(&mut self, it: &ArrayPattern<'a>) {
        self.feature(Some(it.span.start), "destructuring");
        if let Some(rest) = &it.rest
            && !matches!(rest.argument, BindingPattern::BindingIdentifier(_))
        {
            let pos = rest.argument.span().start;
            self.feature(Some(pos), "non-identifier array rest patterns");
        }
        walk_js::walk_array_pattern(self, it);
    }

    fn visit_function(&mut self, it: &Function<'a>, _flags: ScopeFlags) {
        if it.r#async {
            let pos = self
                .scan
                .skip_words(it.span.start, &["export", "default", "declare"]);
            let pos = (self.scan.word_at(pos) == "async").then_some(pos);
            self.async_fn(pos, it.generator);
        } else if it.generator {
            let pos = self
                .scan
                .skip_words(it.span.start, &["export", "default", "declare", "function"]);
            let pos = self.scan.starts(pos, "*").then_some(pos);
            self.feature(pos, "generator functions");
        }
        self.fn_inner(it);
    }

    fn visit_arrow_function_expression(&mut self, it: &ArrowFunctionExpression<'a>) {
        if it.r#async {
            self.async_fn(Some(it.span.start), false);
        }
        self.fn_depth += 1;
        for item in &it.params.items {
            self.cover_pattern(&item.pattern);
            if let Some(init) = &item.initializer {
                let eq = self.param_eq(item);
                self.feature(eq, "default arguments");
                self.visit_expression(init);
            }
        }
        if let Some(rest) = &it.params.rest {
            self.feature(Some(rest.rest.span.start), "rest arguments");
            self.cover_pattern(&rest.rest.argument);
        }
        self.visit_arrow_function_body(&it.body);
        self.fn_depth -= 1;
    }

    fn visit_class(&mut self, it: &Class<'a>) {
        for d in &it.decorators {
            self.visit_decorator(d);
        }
        let start = it
            .decorators
            .iter()
            .map(|d| d.span.end)
            .fold(it.span.start, u32::max);
        let pos = self
            .scan
            .skip_words(start, &["export", "default", "declare", "abstract"]);
        let pos = (self.scan.word_at(pos) == "class").then_some(pos);
        self.feature(pos, "class syntax");
        if let Some(id) = &it.id {
            self.visit_binding_identifier(id);
        }
        if let Some(h) = &it.heritage {
            self.visit_expression(&h.expression);
        }
        for el in &it.body.body {
            self.class_element(el);
        }
    }

    fn visit_object_expression(&mut self, it: &ObjectExpression<'a>) {
        let address = addr(it);
        let ctx = self.registered.get(&address).copied();
        let own = self.new_ctx();
        for prop in &it.properties {
            match prop {
                ObjectPropertyKind::SpreadProperty(s) => {
                    self.register(head_literal(&s.argument), own);
                    self.visit_expression(&s.argument);
                }
                ObjectPropertyKind::ObjectProperty(p) => {
                    if p.computed {
                        let pos = self.scan.find_after_modifiers(p.span.start, "[");
                        self.feature(pos, "object literal extensions");
                    }
                    let func = match &p.value {
                        Expression::FunctionExpression(f)
                            if p.method || p.kind != PropertyKind::Init =>
                        {
                            Some(f)
                        }
                        _ => None,
                    };
                    if let Some(f) = func {
                        if p.method {
                            self.method(p.span.start, f);
                        }
                        if let Some(key) = p.key.as_expression().filter(|_| p.computed) {
                            self.visit_expression(key);
                        }
                        self.fn_inner(f);
                    } else {
                        if let Some(key) = p.key.as_expression().filter(|_| p.computed) {
                            self.visit_expression(key);
                        }
                        if !p.shorthand {
                            self.register(head_literal(&p.value), own);
                        }
                        self.visit_expression(&p.value);
                    }
                }
            }
        }
        self.end_literal(address, ctx, own);
    }

    fn visit_array_expression(&mut self, it: &ArrayExpression<'a>) {
        let address = addr(it);
        let ctx = self.registered.get(&address).copied();
        let own = self.new_ctx();
        for el in &it.elements {
            match el {
                ArrayExpressionElement::SpreadElement(s) => {
                    self.spread_event(ctx, s.span.start);
                    self.register(head_literal(&s.argument), own);
                    self.visit_expression(&s.argument);
                }
                ArrayExpressionElement::Elision(_) => {}
                e => {
                    if let Some(e) = e.as_expression() {
                        self.register(head_literal(e), own);
                        self.visit_expression(e);
                    }
                }
            }
        }
        self.end_literal(address, ctx, own);
    }

    fn visit_array_assignment_target(&mut self, it: &ArrayAssignmentTarget<'a>) {
        self.feature(Some(it.span.start), "destructuring");
        let address = addr(it);
        let ctx = self.registered.get(&address).copied();
        let own = self.new_ctx();
        for el in it.elements.iter().flatten() {
            if let AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(d) = el
                && let Some(a) = target_head(&d.binding)
                && matches!(
                    d.binding,
                    AssignmentTarget::ArrayAssignmentTarget(_)
                        | AssignmentTarget::ObjectAssignmentTarget(_)
                )
            {
                self.followed_by_eq.insert(a);
            }
            self.register(maybe_default_head(el), own);
            self.visit_assignment_target_maybe_default(el);
        }
        if let Some(rest) = &it.rest {
            self.spread_event(ctx, rest.span.start);
            self.register(target_head(&rest.target), own);
            self.visit_assignment_target(&rest.target);
        }
        self.end_literal(address, ctx, own);
    }

    fn visit_object_assignment_target(&mut self, it: &ObjectAssignmentTarget<'a>) {
        self.feature(Some(it.span.start), "destructuring");
        let address = addr(it);
        let ctx = self.registered.get(&address).copied();
        let own = self.new_ctx();
        for prop in &it.properties {
            match prop {
                AssignmentTargetProperty::AssignmentTargetPropertyIdentifier(p) => {
                    self.visit_identifier_reference(&p.binding);
                    if let Some(init) = &p.init {
                        self.visit_expression(init);
                    }
                }
                AssignmentTargetProperty::AssignmentTargetPropertyProperty(p) => {
                    if p.computed {
                        let pos = self.scan.find_after_modifiers(p.span.start, "[");
                        self.feature(pos, "object literal extensions");
                        if let Some(key) = p.name.as_expression() {
                            self.visit_expression(key);
                        }
                    }
                    if let AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(d) = &p.binding
                        && matches!(
                            d.binding,
                            AssignmentTarget::ArrayAssignmentTarget(_)
                                | AssignmentTarget::ObjectAssignmentTarget(_)
                        )
                        && let Some(a) = target_head(&d.binding)
                    {
                        self.followed_by_eq.insert(a);
                    }
                    self.register(maybe_default_head(&p.binding), own);
                    self.visit_assignment_target_maybe_default(&p.binding);
                }
            }
        }
        if let Some(rest) = &it.rest {
            self.visit_assignment_target(&rest.target);
        }
        self.end_literal(address, ctx, own);
    }

    fn visit_assignment_expression(&mut self, it: &AssignmentExpression<'a>) {
        if matches!(
            it.left,
            AssignmentTarget::ArrayAssignmentTarget(_)
                | AssignmentTarget::ObjectAssignmentTarget(_)
        ) && let Some(a) = target_head(&it.left)
        {
            self.followed_by_eq.insert(a);
        }
        walk_js::walk_assignment_expression(self, it);
    }

    fn visit_parenthesized_expression(&mut self, it: &ParenthesizedExpression<'a>) {
        let ctx = self.new_ctx();
        match &it.expression {
            Expression::SequenceExpression(s) => {
                for e in &s.expressions {
                    self.register(head_literal(e), ctx);
                }
            }
            e => self.register(head_literal(e), ctx),
        }
        walk_js::walk_parenthesized_expression(self, it);
        self.end_group(ctx);
    }

    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        for arg in &it.arguments {
            if let Argument::SpreadElement(s) = arg {
                self.feature(Some(s.span.start), "rest arguments");
            }
        }
        // `async(...)` is parsed like the parameters of an async arrow function.
        let group = (!it.optional
            && matches!(&it.callee, Expression::Identifier(i) if i.name == "async"))
        .then(|| {
            let ctx = self.new_ctx();
            for arg in &it.arguments {
                if let Some(e) = arg.as_expression() {
                    self.register(head_literal(e), ctx);
                }
            }
            ctx
        });
        walk_js::walk_call_expression(self, it);
        if let Some(ctx) = group {
            self.end_group(ctx);
        }
    }

    fn visit_new_expression(&mut self, it: &NewExpression<'a>) {
        for arg in &it.arguments {
            if let Argument::SpreadElement(s) = arg {
                self.feature(Some(s.span.start), "rest arguments");
            }
        }
        walk_js::walk_new_expression(self, it);
    }

    fn visit_jsx_spread_child(&mut self, it: &JSXSpreadChild<'a>) {
        let pos = self.scan.token(it.span.start + 1, "...");
        self.feature(pos, "rest arguments");
        walk_js::walk_jsx_spread_child(self, it);
    }

    fn visit_new_target(&mut self, it: &NewTarget) {
        self.feature(Some(it.span.start), "new.target");
    }

    fn visit_await_expression(&mut self, it: &AwaitExpression<'a>) {
        self.top_level_await(it.span.start);
        walk_js::walk_await_expression(self, it);
    }

    fn visit_for_of_statement(&mut self, it: &ForOfStatement<'a>) {
        if it.r#await
            && let Some(pos) = self.scan.token(it.span.start + 3, "await")
        {
            self.top_level_await(pos);
            self.feature(Some(pos), "for-await loops");
        }
        if let Some(t) = it.left.as_assignment_target()
            && matches!(
                t,
                AssignmentTarget::ArrayAssignmentTarget(_)
                    | AssignmentTarget::ObjectAssignmentTarget(_)
            )
            && let Some(a) = target_head(t)
        {
            self.followed_by_eq.insert(a);
        }
        let of = self.scan.token(it.left.span().end, "of");
        self.feature(of, "for-of loops");
        walk_js::walk_for_of_statement(self, it);
    }

    fn visit_for_in_statement(&mut self, it: &ForInStatement<'a>) {
        if let Some(t) = it.left.as_assignment_target()
            && matches!(
                t,
                AssignmentTarget::ArrayAssignmentTarget(_)
                    | AssignmentTarget::ObjectAssignmentTarget(_)
            )
            && let Some(a) = target_head(t)
        {
            self.followed_by_eq.insert(a);
        }
        walk_js::walk_for_in_statement(self, it);
    }

    fn visit_import_expression(&mut self, it: &ImportExpression<'a>) {
        if let Some(options) = &it.options
            && !self.removable(options)
        {
            self.err(
                options.span().start,
                format!(
                    "Using an arbitrary value as the second argument to \"import()\" is not possible in {WHERE}"
                ),
            );
        }
        walk_js::walk_import_expression(self, it);
    }

    fn visit_if_statement(&mut self, it: &IfStatement<'a>) {
        self.visit_expression(&it.test);
        let test = to_boolean(&it.test);
        let dead_yes = u32::from(test == Some(false));
        self.dead += dead_yes;
        self.visit_statement(&it.consequent);
        self.dead -= dead_yes;
        if let Some(alt) = &it.alternate {
            let dead_no = u32::from(test == Some(true));
            self.dead += dead_no;
            self.visit_statement(alt);
            self.dead -= dead_no;
        }
    }

    fn visit_conditional_expression(&mut self, it: &ConditionalExpression<'a>) {
        self.visit_expression(&it.test);
        let test = to_boolean(&it.test);
        let dead_yes = u32::from(test == Some(false));
        self.dead += dead_yes;
        self.visit_expression(&it.consequent);
        self.dead -= dead_yes;
        let dead_no = u32::from(test == Some(true));
        self.dead += dead_no;
        self.visit_expression(&it.alternate);
        self.dead -= dead_no;
    }

    fn visit_logical_expression(&mut self, it: &LogicalExpression<'a>) {
        self.visit_expression(&it.left);
        let dead = u32::from(match it.operator {
            LogicalOperator::Or => to_boolean(&it.left) == Some(true),
            LogicalOperator::And => to_boolean(&it.left) == Some(false),
            LogicalOperator::Coalesce => not_nullish(&it.left),
        });
        self.dead += dead;
        self.visit_expression(&it.right);
        self.dead -= dead;
    }

    fn visit_try_statement(&mut self, it: &TryStatement<'a>) {
        self.visit_block_statement(&it.block);
        if let Some(handler) = &it.handler {
            let dead = u32::from(it.block.body.is_empty());
            self.dead += dead;
            self.visit_catch_clause(handler);
            self.dead -= dead;
        }
        if let Some(finalizer) = &it.finalizer {
            self.visit_block_statement(finalizer);
        }
    }
}
