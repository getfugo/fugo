//! The program traversal: classes lowered innermost first, private names, `this` and `super`
//! rewritten in code that moves.

use super::*;

impl<'a> VisitMut<'a> for Lowerer<'a, '_> {
    fn visit_program(&mut self, it: &mut Program<'a>) {
        let temps = self.with_var_scope(|s| s.visit_statements(&mut it.body));
        self.finish_var_scope(&temps, &mut it.body);
    }

    fn visit_statements(&mut self, it: &mut ArenaVec<'a, Statement<'a>>) {
        let old = it.take_in(&self.b);
        let mut out = ArenaVec::with_capacity_in(old.len(), &self.b);
        for stmt in old {
            match stmt {
                Statement::ClassDeclaration(class) if Self::is_decorated(&class) => {
                    let span = class.span;
                    self.changed = true;
                    out.extend(self.lower_class_statement(class, ClassKind::Stmt, span));
                }
                Statement::ExportDeclaration(e) if matches!(&e.declaration, Declaration::ClassDeclaration(c) if Self::is_decorated(c)) =>
                {
                    let e = e.unbox();
                    let Declaration::ClassDeclaration(class) = e.declaration else {
                        continue;
                    };
                    self.changed = true;
                    out.extend(self.lower_class_statement(class, ClassKind::ExportStmt, e.span));
                }
                Statement::ExportDefaultDeclaration(e) if matches!(&e.declaration, ExportDefaultDeclarationKind::ClassDeclaration(c) if Self::is_decorated(c)) =>
                {
                    let e = e.unbox();
                    let ExportDefaultDeclarationKind::ClassDeclaration(class) = e.declaration
                    else {
                        continue;
                    };
                    self.changed = true;
                    out.extend(self.lower_class_statement(class, ClassKind::ExportDefault, e.span));
                }
                mut stmt => {
                    self.visit_statement(&mut stmt);
                    out.push(stmt);
                }
            }
        }
        *it = out;
    }

    fn visit_function(&mut self, it: &mut Function<'a>, _flags: ScopeFlags) {
        self.visit_function_with_ctx(it, FnCtx::default());
    }

    fn visit_arrow_function_expression(&mut self, it: &mut ArrowFunctionExpression<'a>) {
        self.check_params(&it.params);
        self.visit_formal_parameters(&mut it.params);
        let temps = self.with_var_scope(|s| match &mut it.body {
            ArrowFunctionBody::FunctionBody(body) => s.visit_function_body(body),
            body => {
                if let Some(e) = body.as_expression_mut() {
                    s.visit_expression(e);
                }
            }
        });
        if temps.is_empty() {
            return;
        }
        if !matches!(it.body, ArrowFunctionBody::FunctionBody(_)) {
            let body = it.body.take_in(&self.b);
            let e = body.into_expression();
            let ret = Statement::new_return_statement(SPAN, Some(e), &self.b);
            it.body = ArrowFunctionBody::FunctionBody(FunctionBody::boxed(
                SPAN,
                ArenaVec::new_in(&self.b),
                ArenaVec::from_iter_in([ret], &self.b),
                &self.b,
            ));
        }
        if let ArrowFunctionBody::FunctionBody(body) = &mut it.body {
            self.finish_var_scope(&temps, &mut body.statements);
        }
    }

    fn visit_static_block(&mut self, it: &mut StaticBlock<'a>) {
        let temps = self.with_var_scope(|s| s.visit_statements(&mut it.body));
        self.finish_var_scope(&temps, &mut it.body);
    }

    fn visit_ts_module_block(&mut self, it: &mut TSModuleBlock<'a>) {
        self.namespace_depth += 1;
        let temps = self.with_var_scope(|s| s.visit_statements(&mut it.body));
        self.finish_var_scope(&temps, &mut it.body);
        self.namespace_depth -= 1;
    }

    fn visit_ts_namespace_declaration(&mut self, it: &mut TSNamespaceDeclaration<'a>) {
        if !it.declare {
            walk_mut::walk_ts_namespace_declaration(self, it);
        }
    }

    fn visit_ts_external_module_declaration(&mut self, it: &mut TSExternalModuleDeclaration<'a>) {
        if !it.declare {
            walk_mut::walk_ts_external_module_declaration(self, it);
        }
    }

    fn visit_ts_global_declaration(&mut self, _: &mut TSGlobalDeclaration<'a>) {}

    fn visit_class(&mut self, it: &mut Class<'a>) {
        if it.declare {
            return;
        }
        // Not decorated (decorated classes are lowered from their statement or expression).
        let has_accessor = it
            .body
            .body
            .iter()
            .any(|e| element_kind(e) == ElemKind::Accessor);
        self.visit_class_parts(it, None);
        if has_accessor {
            self.changed = true;
            self.rewrite_plain_accessors(it);
        }
    }

    fn visit_expression(&mut self, it: &mut Expression<'a>) {
        match it {
            Expression::ClassExpression(class) if Self::is_decorated(class) => {
                self.changed = true;
                self.lower_class_expression(it);
                return;
            }
            Expression::ChainExpression(chain)
                if self.chain_has_lowered_private(&chain.expression) =>
            {
                self.lower_chain(it);
                return;
            }
            Expression::AssignmentExpression(a)
                if a.operator == AssignmentOperator::Assign
                    && self.is_rewritten_target(&a.left) =>
            {
                self.lower_simple_assignment(it);
                return;
            }
            Expression::UnaryExpression(u)
                if u.operator == UnaryOperator::Delete
                    && self.fn_ctx.super_home.is_some()
                    && matches!(u.argument.get_inner_expression(), Expression::StaticMemberExpression(m) if m.object.is_super())
                        | matches!(u.argument.get_inner_expression(), Expression::ComputedMemberExpression(m) if m.object.is_super()) =>
            {
                let span = u.span;
                self.error(span, "`delete super[...]` in code that lowered decorators move out of a class is not supported");
                return;
            }
            _ => {}
        }
        walk_mut::walk_expression(self, it);
        match it {
            Expression::ThisExpression(t) => {
                if let Some(to) = self.fn_ctx.this_to {
                    *it = Expression::new_identifier(t.span, to, &self.b);
                }
            }
            Expression::NewTarget(_) if self.fn_ctx.new_target_undefined => {
                *it = self.void0();
            }
            Expression::PrivateFieldExpression(p) if !p.optional => {
                if let Some(l) = self.lookup_private(p.field.name.as_str()) {
                    let obj = p.object.take_in(&self.b);
                    *it = self.private_read(obj, l);
                }
            }
            Expression::PrivateInExpression(p) => {
                if let Some(l) = self.lookup_private(p.left.name.as_str()) {
                    let right = p.right.take_in(&self.b);
                    let member = self.ident(l.member());
                    *it = self.call_helper(Helper::PrivateIn, vec![member, right]);
                }
            }
            Expression::StaticMemberExpression(_) | Expression::ComputedMemberExpression(_) => {
                if let Some(key) = self.take_super_key(it) {
                    *it = self.super_get(key);
                }
            }
            _ => {}
        }
    }

    fn visit_call_expression(&mut self, it: &mut CallExpression<'a>) {
        let callee = it.callee.get_inner_expression_mut();
        // `obj.#m(args)` → `__privateMethod(obj, brand, m_fn).call(obj, args)`.
        if let Expression::PrivateFieldExpression(p) = callee
            && !p.optional
            && let Some(l) = self.lookup_private(p.field.name.as_str())
        {
            self.visit_expression(&mut p.object);
            let obj = p.object.take_in(&self.b);
            let (obj, this) = self.capture(obj);
            let read = self.private_read(obj, l);
            it.callee = self.member(read, "call");
            self.visit_arguments(&mut it.arguments);
            it.arguments.insert(0, Argument::from(this));
            return;
        }
        // `super.m(args)` → `__superGet(home, this, "m").call(this, args)`.
        if self.fn_ctx.super_home.is_some()
            && let Some(key) = {
                if let Expression::ComputedMemberExpression(m) = callee
                    && m.object.is_super()
                {
                    self.visit_expression(&mut m.expression);
                }
                self.take_super_key(callee)
            }
        {
            let get = self.super_get(key);
            let mut call_member = self.member(get, "call");
            if it.optional {
                // `super.m?.()`: the function is checked, `.call` keeps `this`.
                if let Expression::StaticMemberExpression(m) = &mut call_member {
                    m.optional = true;
                }
                it.optional = false;
            }
            it.callee = call_member;
            self.visit_arguments(&mut it.arguments);
            let this = self.this_value();
            it.arguments.insert(0, Argument::from(this));
            return;
        }
        walk_mut::walk_call_expression(self, it);
    }

    fn visit_tagged_template_expression(&mut self, it: &mut TaggedTemplateExpression<'a>) {
        let tag = it.tag.get_inner_expression_mut();
        if let Expression::PrivateFieldExpression(p) = tag
            && let Some(l) = self.lookup_private(p.field.name.as_str())
        {
            self.visit_expression(&mut p.object);
            let obj = p.object.take_in(&self.b);
            let (obj, this) = self.capture(obj);
            let read = self.private_read(obj, l);
            let bind = self.member(read, "bind");
            it.tag = self.call(bind, vec![this]);
            self.visit_template_literal(&mut it.quasi);
            return;
        }
        if self.fn_ctx.super_home.is_some()
            && let Some(key) = {
                if let Expression::ComputedMemberExpression(m) = tag
                    && m.object.is_super()
                {
                    self.visit_expression(&mut m.expression);
                }
                self.take_super_key(tag)
            }
        {
            let get = self.super_get(key);
            let bind = self.member(get, "bind");
            let this = self.this_value();
            it.tag = self.call(bind, vec![this]);
            self.visit_template_literal(&mut it.quasi);
            return;
        }
        walk_mut::walk_tagged_template_expression(self, it);
    }

    fn visit_simple_assignment_target(&mut self, it: &mut SimpleAssignmentTarget<'a>) {
        // `(obj.#x as T) += 1`: TypeScript erases the wrapper, so it may go.
        if let Some(inner) = it.get_expression_mut()
            && self.is_rewritten_expr(inner.get_inner_expression())
        {
            let inner = inner.get_inner_expression_mut().take_in(&self.b);
            *it = SimpleAssignmentTarget::from(inner.into_member_expression());
        }
        walk_mut::walk_simple_assignment_target(self, it);
        match it {
            SimpleAssignmentTarget::PrivateFieldExpression(p) => {
                if let Some(l) = self.lookup_private(p.field.name.as_str()) {
                    let obj = p.object.take_in(&self.b);
                    *it = self.private_wrapper_target(obj, l);
                }
            }
            SimpleAssignmentTarget::StaticMemberExpression(_)
            | SimpleAssignmentTarget::ComputedMemberExpression(_) => {
                let key = match it {
                    SimpleAssignmentTarget::StaticMemberExpression(m) if m.object.is_super() => {
                        self.fn_ctx
                            .super_home
                            .map(|_| self.string(m.property.name.as_str()))
                    }
                    SimpleAssignmentTarget::ComputedMemberExpression(m) if m.object.is_super() => {
                        if self.fn_ctx.super_home.is_some() {
                            Some(m.expression.take_in(&self.b))
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                if let Some(key) = key {
                    let home = self.fn_ctx.super_home.map(|h| self.super_home(h));
                    let home = home.unwrap_or_else(|| self.void0());
                    let this = self.this_value();
                    let wrapper = self.call_helper(Helper::SuperWrapper, vec![home, this, key]);
                    *it = SimpleAssignmentTarget::new_static_member_expression(
                        SPAN,
                        wrapper,
                        IdentifierName::new(SPAN, "_", &self.b),
                        false,
                        &self.b,
                    );
                }
            }
            _ => {}
        }
    }

    fn visit_jsx_member_expression_object(&mut self, it: &mut JSXMemberExpressionObject<'a>) {
        if let JSXMemberExpressionObject::ThisExpression(t) = it
            && let Some(to) = self.fn_ctx.this_to
        {
            let span = t.span;
            *it = JSXMemberExpressionObject::IdentifierReference(IdentifierReference::boxed(
                span, to, &self.b,
            ));
            return;
        }
        walk_mut::walk_jsx_member_expression_object(self, it);
    }

    fn visit_formal_parameter(&mut self, it: &mut FormalParameter<'a>) {
        if let (BindingPattern::BindingIdentifier(id), Some(init)) = (&it.pattern, &it.initializer)
        {
            let name = id.name.as_str().to_owned();
            self.hint(init, &name);
        }
        walk_mut::walk_formal_parameter(self, it);
    }

    fn visit_variable_declarator(&mut self, it: &mut VariableDeclarator<'a>) {
        if let (BindingPattern::BindingIdentifier(id), Some(init)) = (&it.id, &it.init) {
            let name = id.name.as_str().to_owned();
            self.hint(init, &name);
        }
        walk_mut::walk_variable_declarator(self, it);
    }

    fn visit_assignment_pattern(&mut self, it: &mut AssignmentPattern<'a>) {
        if let BindingPattern::BindingIdentifier(id) = &it.left {
            let name = id.name.as_str().to_owned();
            self.hint(&it.right, &name);
        }
        walk_mut::walk_assignment_pattern(self, it);
    }

    fn visit_assignment_expression(&mut self, it: &mut AssignmentExpression<'a>) {
        if matches!(
            it.operator,
            AssignmentOperator::Assign
                | AssignmentOperator::LogicalAnd
                | AssignmentOperator::LogicalOr
                | AssignmentOperator::LogicalNullish
        ) && let AssignmentTarget::AssignmentTargetIdentifier(id) = &it.left
        {
            let name = id.name.as_str().to_owned();
            self.hint(&it.right, &name);
        }
        walk_mut::walk_assignment_expression(self, it);
    }

    fn visit_assignment_target_with_default(&mut self, it: &mut AssignmentTargetWithDefault<'a>) {
        if let AssignmentTarget::AssignmentTargetIdentifier(id) = &it.binding {
            let name = id.name.as_str().to_owned();
            self.hint(&it.init, &name);
        }
        walk_mut::walk_assignment_target_with_default(self, it);
    }

    fn visit_assignment_target_property_identifier(
        &mut self,
        it: &mut AssignmentTargetPropertyIdentifier<'a>,
    ) {
        if let Some(init) = &it.init {
            let name = it.binding.name.as_str().to_owned();
            self.hint(init, &name);
        }
        walk_mut::walk_assignment_target_property_identifier(self, it);
    }

    fn visit_object_property(&mut self, it: &mut ObjectProperty<'a>) {
        if it.kind == PropertyKind::Init
            && let Some(name) = static_key_name(&it.key, it.computed)
        {
            self.hint(&it.value, &name);
        }
        walk_mut::walk_object_property(self, it);
    }

    fn visit_export_default_declaration(&mut self, it: &mut ExportDefaultDeclaration<'a>) {
        if let Some(e) = it.declaration.as_expression() {
            self.hint(e, "default");
        }
        walk_mut::walk_export_default_declaration(self, it);
    }
}
