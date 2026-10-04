//! The checker's bookkeeping (errors, deferred array spreads, contexts) and the checks the
//! traversal shares: functions, methods, patterns and class elements.

use super::*;

impl<'a, 's> Checker<'a, 's> {
    pub(super) fn new(src: &'s str, declared: HashSet<&'a str>) -> Self {
        Self {
            scan: Scan::new(src),
            errors: Vec::new(),
            declared,
            fn_depth: 0,
            dead: 0,
            ctxs: Vec::new(),
            registered: HashMap::new(),
            followed_by_eq: HashSet::new(),
            astral_decls: HashMap::new(),
            astral_refs: HashMap::new(),
        }
    }

    pub(super) fn finish(mut self) -> Vec<(u32, String)> {
        let mut astral: Vec<(String, u32)> = self.astral_decls.drain().collect();
        let refs: Vec<(String, u32)> = self
            .astral_refs
            .drain()
            .filter(|(name, _)| !astral.iter().any(|(d, _)| d == name))
            .collect();
        astral.extend(refs);
        for (name, pos) in astral {
            self.errors.push((
                pos,
                format!(
                    "\"{name}\" cannot be escaped in {WHERE} but you can set the charset to \"utf8\" to allow unescaped Unicode characters"
                ),
            ));
        }
        self.errors
    }

    pub(super) fn err(&mut self, pos: u32, message: String) {
        self.errors.push((pos, message));
    }

    pub(super) fn feature(&mut self, pos: Option<u32>, what: &str) {
        if let Some(pos) = pos {
            self.err(pos, transforming(what));
        }
    }

    pub(super) fn top_level_await(&mut self, pos: u32) {
        if self.fn_depth == 0 && self.dead == 0 {
            self.err(pos, format!("Top-level await is not available in {WHERE}"));
        }
    }

    pub(super) fn astral(map: &mut HashMap<String, u32>, name: &str, pos: u32) {
        if name.chars().any(|c| u32::from(c) > 0xFFFF) {
            let e = map.entry(name.to_owned()).or_insert(pos);
            *e = (*e).min(pos);
        }
    }

    // ---- deferred array spread errors ----

    pub(super) fn new_ctx(&mut self) -> usize {
        self.ctxs.push(None);
        self.ctxs.len() - 1
    }

    pub(super) fn register(&mut self, literal: Option<usize>, ctx: usize) {
        if let Some(a) = literal {
            self.registered.insert(a, ctx);
        }
    }

    /// An array spread `...` at `pos` of a literal parsed with `ctx`: reported now, or kept in
    /// the context (replacing what it had).
    pub(super) fn spread_event(&mut self, ctx: Option<usize>, pos: u32) {
        match ctx {
            None => self.err(pos, transforming("array spread")),
            Some(id) => self.ctxs[id] = Some(pos),
        }
    }

    /// The end of a literal: its own context (spreads of nested literals) is dropped when it is
    /// a pattern, reported when it was parsed without a context, else moved to that context.
    pub(super) fn end_literal(&mut self, address: usize, ctx: Option<usize>, own: usize) {
        if self.followed_by_eq.contains(&address) {
            return;
        }
        if let Some(pos) = self.ctxs[own] {
            self.spread_event(ctx, pos);
        }
    }

    pub(super) fn end_group(&mut self, ctx: usize) {
        if let Some(pos) = self.ctxs[ctx] {
            self.err(pos, transforming("array spread"));
        }
    }

    // ---- token positions ----

    /// The `=` of a default value after `end` (a binding or its type annotation).
    pub(super) fn eq_after(&self, end: u32) -> Option<u32> {
        let p = self.scan.skip_trivia(end);
        let p = if self.scan.starts(p, "?") { p + 1 } else { p };
        self.scan.token(p, "=")
    }

    pub(super) fn param_eq(&self, item: &FormalParameter<'_>) -> Option<u32> {
        let mut end = item.pattern.span().end;
        if let Some(t) = &item.type_annotation {
            end = end.max(t.span.end);
        }
        self.eq_after(end)
    }

    /// The first `word` token from `pos`, skipping trivia, words and `*`.
    pub(super) fn find_word(&self, mut pos: u32, word: &str) -> Option<u32> {
        for _ in 0..16 {
            pos = self.scan.skip_trivia(pos);
            let w = self.scan.word_at(pos);
            if w == word {
                return Some(pos);
            }
            if !w.is_empty() {
                pos += u32::try_from(w.len()).unwrap_or(0);
            } else if self.scan.starts(pos, "*") {
                pos += 1;
            } else {
                return None;
            }
        }
        None
    }

    pub(super) fn async_fn(&mut self, pos: Option<u32>, generator: bool) {
        let what = if generator {
            "async generator functions"
        } else {
            "async functions"
        };
        self.feature(pos, what);
    }

    // ---- functions ----

    /// Parameters as esbuild's `parseFn` reads them (functions and methods, not arrows).
    pub(super) fn fn_params(&mut self, params: &FormalParameters<'a>) {
        for item in &params.items {
            for d in &item.decorators {
                self.visit_decorator(d);
            }
            self.visit_binding_pattern(&item.pattern);
            if let Some(init) = &item.initializer {
                let eq = self.param_eq(item);
                self.feature(eq, "default arguments");
                self.visit_expression(init);
            }
        }
        if let Some(rest) = &params.rest {
            self.feature(Some(rest.rest.span.start), "rest arguments");
            self.visit_binding_pattern(&rest.rest.argument);
        }
    }

    /// The parameters and body of a function, method or accessor.
    pub(super) fn fn_inner(&mut self, f: &Function<'a>) {
        self.fn_depth += 1;
        self.fn_params(&f.params);
        if let Some(body) = &f.body {
            self.visit_function_body(body);
        }
        self.fn_depth -= 1;
    }

    /// A method of an object literal or class: esbuild reports `async`, else `*`, else `(`.
    pub(super) fn method(&mut self, start: u32, f: &Function<'a>) {
        if f.r#async {
            let pos = self.find_word(start, "async");
            self.async_fn(pos, f.generator);
        } else if f.generator {
            let pos = self.scan.find_after_modifiers(start, "*");
            self.feature(pos, "generator functions");
        } else {
            self.feature(Some(f.params.span.start), "object literal extensions");
        }
    }

    /// A pattern in arrow function parameters, which esbuild parses as an expression first.
    pub(super) fn cover_pattern(&mut self, pat: &BindingPattern<'a>) {
        match pat {
            BindingPattern::BindingIdentifier(id) => self.visit_binding_identifier(id),
            BindingPattern::ObjectPattern(o) => {
                self.feature(Some(o.span.start), "destructuring");
                for p in &o.properties {
                    if p.computed {
                        let pos = self.scan.find_after_modifiers(p.span.start, "[");
                        self.feature(pos, "object literal extensions");
                        if let Some(key) = p.key.as_expression() {
                            self.visit_expression(key);
                        }
                    }
                    match &p.value {
                        BindingPattern::AssignmentPattern(a) => {
                            if !p.shorthand {
                                let eq = self.eq_after(a.left.span().end);
                                self.feature(eq, "default arguments");
                            }
                            self.cover_pattern(&a.left);
                            self.visit_expression(&a.right);
                        }
                        v => self.cover_pattern(v),
                    }
                }
                if let Some(r) = &o.rest {
                    self.cover_pattern(&r.argument);
                }
            }
            BindingPattern::ArrayPattern(a) => {
                self.feature(Some(a.span.start), "destructuring");
                for el in a.elements.iter().flatten() {
                    self.cover_element(el);
                }
                if let Some(r) = &a.rest {
                    if !matches!(r.argument, BindingPattern::BindingIdentifier(_)) {
                        let pos = r.argument.span().start;
                        self.feature(Some(pos), "non-identifier array rest patterns");
                    }
                    self.cover_pattern(&r.argument);
                }
            }
            BindingPattern::AssignmentPattern(a) => self.cover_element_default(a),
        }
    }

    pub(super) fn cover_element(&mut self, el: &BindingPattern<'a>) {
        match el {
            BindingPattern::AssignmentPattern(a) => self.cover_element_default(a),
            e => self.cover_pattern(e),
        }
    }

    pub(super) fn cover_element_default(&mut self, a: &AssignmentPattern<'a>) {
        let eq = self.eq_after(a.left.span().end);
        self.feature(eq, "default arguments");
        self.cover_pattern(&a.left);
        self.visit_expression(&a.right);
    }

    pub(super) fn removable(&self, e: &Expression<'_>) -> bool {
        match e {
            Expression::BooleanLiteral(_)
            | Expression::NullLiteral(_)
            | Expression::NumericLiteral(_)
            | Expression::BigIntLiteral(_)
            | Expression::StringLiteral(_)
            | Expression::RegExpLiteral(_)
            | Expression::FunctionExpression(_)
            | Expression::ArrowFunctionExpression(_) => true,
            Expression::Identifier(id) => {
                id.name == "undefined" || self.declared.contains(id.name.as_str())
            }
            Expression::TemplateLiteral(t) => t.expressions.is_empty(),
            Expression::ArrayExpression(a) => a.elements.iter().all(|el| match el {
                ArrayExpressionElement::SpreadElement(_) => false,
                ArrayExpressionElement::Elision(_) => true,
                e => e.as_expression().is_some_and(|e| self.removable(e)),
            }),
            Expression::ObjectExpression(o) => o.properties.iter().all(|p| match p {
                ObjectPropertyKind::SpreadProperty(_) => false,
                ObjectPropertyKind::ObjectProperty(p) => {
                    (!p.computed || p.key.as_expression().is_some_and(|k| self.removable(k)))
                        && (p.method || p.kind != PropertyKind::Init || self.removable(&p.value))
                }
            }),
            Expression::UnaryExpression(u) => match u.operator {
                UnaryOperator::Typeof => {
                    matches!(u.argument, Expression::Identifier(_)) || self.removable(&u.argument)
                }
                UnaryOperator::Void | UnaryOperator::LogicalNot => self.removable(&u.argument),
                _ => false,
            },
            Expression::ParenthesizedExpression(p) => self.removable(&p.expression),
            _ => false,
        }
    }

    /// An element of a class body: its decorators, computed key and value or body.
    pub(super) fn class_element(&mut self, el: &ClassElement<'a>) {
        match el {
            ClassElement::MethodDefinition(m) => {
                for d in &m.decorators {
                    self.visit_decorator(d);
                }
                let start = m
                    .decorators
                    .iter()
                    .map(|d| d.span.end)
                    .fold(m.span.start, u32::max);
                if m.computed {
                    let pos = self.scan.find_after_modifiers(start, "[");
                    self.feature(pos, "object literal extensions");
                    if let Some(key) = m.key.as_expression() {
                        self.visit_expression(key);
                    }
                }
                if matches!(
                    m.kind,
                    MethodDefinitionKind::Method | MethodDefinitionKind::Constructor
                ) {
                    self.method(start, &m.value);
                }
                self.fn_inner(&m.value);
            }
            ClassElement::PropertyDefinition(p) => {
                for d in &p.decorators {
                    self.visit_decorator(d);
                }
                if p.computed {
                    let start = p
                        .decorators
                        .iter()
                        .map(|d| d.span.end)
                        .fold(p.span.start, u32::max);
                    let pos = self.scan.find_after_modifiers(start, "[");
                    self.feature(pos, "object literal extensions");
                    if let Some(key) = p.key.as_expression() {
                        self.visit_expression(key);
                    }
                }
                if let Some(v) = &p.value {
                    self.fn_depth += 1;
                    self.visit_expression(v);
                    self.fn_depth -= 1;
                }
            }
            ClassElement::AccessorProperty(p) => {
                for d in &p.decorators {
                    self.visit_decorator(d);
                }
                if p.computed {
                    let start = p
                        .decorators
                        .iter()
                        .map(|d| d.span.end)
                        .fold(p.span.start, u32::max);
                    let pos = self.scan.find_after_modifiers(start, "[");
                    self.feature(pos, "object literal extensions");
                    if let Some(key) = p.key.as_expression() {
                        self.visit_expression(key);
                    }
                }
                if let Some(v) = &p.value {
                    self.fn_depth += 1;
                    self.visit_expression(v);
                    self.fn_depth -= 1;
                }
            }
            ClassElement::StaticBlock(b) => {
                self.fn_depth += 1;
                self.visit_statements(&b.body);
                self.fn_depth -= 1;
            }
            ClassElement::TSIndexSignature(_) => {}
        }
    }
}
