//! esbuild's `--target=es5` errors, from the oxc AST of a module as loaded.
//!
//! esbuild reports these while parsing (and a few while visiting): its parser marks a syntax
//! feature it cannot lower at the token that introduces it. The checker walks the
//! JavaScript parts of the AST (TypeScript types are skipped, as esbuild skips them) and
//! finds those tokens in the source text.
//!
//! Array spread is the one feature whose errors esbuild defers: an array literal may turn out to
//! be a destructuring pattern, so the error is kept until the parser knows. The deferral is
//! modelled exactly (see [`Checker::spread_event`]), quirks included: in one parenthesized
//! group, only the last deferred array spread is reported.

use std::collections::{HashMap, HashSet};

use oxc::allocator::Allocator;
use oxc::ast::ast::*;
use oxc::ast_visit::{VisitJs, walk_js};
use oxc::parser::{ParseOptions, Parser};
use oxc::span::{GetSpan, SourceType};
use oxc::syntax::operator::{LogicalOperator, UnaryOperator};
use oxc::syntax::scope::ScopeFlags;

use super::Scan;

/// How esbuild names `target: "es5"` in its messages.
pub(super) const WHERE: &str = "the configured target environment (\"es5\")";

fn transforming(what: &str) -> String {
    format!("Transforming {what} to {WHERE} is not supported yet")
}

/// The errors, as (byte offset, message), unsorted.
pub(super) fn check(source: &str, source_type: SourceType) -> Vec<(u32, String)> {
    let allocator = Allocator::default();
    let options = ParseOptions {
        preserve_parens: true,
        ..ParseOptions::default()
    };
    let mut ret = Parser::new(&allocator, source, source_type)
        .with_options(options)
        .parse();
    // A CommonJS file loaded as a module may use sloppy-mode syntax (`with`, octal literals).
    if ret.diagnostics.has_errors() && !source_type.is_typescript() && source_type.is_module() {
        let script = Parser::new(&allocator, source, source_type.with_script(true))
            .with_options(options)
            .parse();
        if !script.diagnostics.has_errors() {
            ret = script;
        }
    }
    if ret.fatal_error {
        return Vec::new();
    }
    let mut declared = Declared::default();
    declared.visit_program(&ret.program);
    let mut checker = Checker::new(source, declared.names);
    checker.visit_program(&ret.program);
    checker.finish()
}

/// The names declared anywhere in the module (an approximation of esbuild's bound symbols).
#[derive(Default)]
struct Declared<'a> {
    names: HashSet<&'a str>,
}

impl<'a> VisitJs<'a> for Declared<'a> {
    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) {
        self.names.insert(it.name.as_str());
    }
}

fn addr<T>(node: &T) -> usize {
    std::ptr::from_ref(node) as usize
}

/// The literal that starts `expr`: esbuild parses it with the deferred errors of the context
/// `expr` is parsed in (operands after the first are parsed without).
fn head_literal(expr: &Expression<'_>) -> Option<usize> {
    match expr {
        Expression::ArrayExpression(a) => Some(addr(&**a)),
        Expression::ObjectExpression(o) => Some(addr(&**o)),
        Expression::AssignmentExpression(a) => target_head(&a.left),
        Expression::CallExpression(c) => head_literal(&c.callee),
        Expression::StaticMemberExpression(m) => head_literal(&m.object),
        Expression::ComputedMemberExpression(m) => head_literal(&m.object),
        Expression::PrivateFieldExpression(m) => head_literal(&m.object),
        Expression::TaggedTemplateExpression(t) => head_literal(&t.tag),
        Expression::BinaryExpression(b) => head_literal(&b.left),
        Expression::LogicalExpression(l) => head_literal(&l.left),
        Expression::ConditionalExpression(c) => head_literal(&c.test),
        Expression::SequenceExpression(s) => s.expressions.first().and_then(head_literal),
        Expression::ChainExpression(c) => match &c.expression {
            ChainElement::CallExpression(call) => head_literal(&call.callee),
            ChainElement::TSNonNullExpression(e) => head_literal(&e.expression),
            e => e
                .as_member_expression()
                .and_then(|m| head_literal(m.object())),
        },
        Expression::TSAsExpression(e) => head_literal(&e.expression),
        Expression::TSSatisfiesExpression(e) => head_literal(&e.expression),
        Expression::TSNonNullExpression(e) => head_literal(&e.expression),
        Expression::TSInstantiationExpression(e) => head_literal(&e.expression),
        _ => None,
    }
}

fn target_head(target: &AssignmentTarget<'_>) -> Option<usize> {
    match target {
        AssignmentTarget::ArrayAssignmentTarget(a) => Some(addr(&**a)),
        AssignmentTarget::ObjectAssignmentTarget(o) => Some(addr(&**o)),
        t => t
            .as_member_expression()
            .and_then(|m| head_literal(m.object())),
    }
}

fn maybe_default_head(target: &AssignmentTargetMaybeDefault<'_>) -> Option<usize> {
    match target {
        AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(d) => target_head(&d.binding),
        t => t.as_assignment_target().and_then(target_head),
    }
}

/// esbuild's `ToBooleanWithSideEffects`, for the dead branches where top-level `await` is
/// silently dropped.
fn to_boolean(expr: &Expression<'_>) -> Option<bool> {
    match expr {
        Expression::NullLiteral(_) => Some(false),
        Expression::BooleanLiteral(b) => Some(b.value),
        Expression::NumericLiteral(n) => Some(n.value != 0.0 && !n.value.is_nan()),
        Expression::BigIntLiteral(b) => Some(!b.value.as_str().trim_start_matches('0').is_empty()),
        Expression::StringLiteral(s) => Some(!s.value.is_empty()),
        Expression::Identifier(i) if i.name == "undefined" => Some(false),
        Expression::FunctionExpression(_)
        | Expression::ArrowFunctionExpression(_)
        | Expression::RegExpLiteral(_)
        | Expression::ObjectExpression(_)
        | Expression::ArrayExpression(_)
        | Expression::ClassExpression(_) => Some(true),
        Expression::UnaryExpression(u) => match u.operator {
            UnaryOperator::Void => Some(false),
            UnaryOperator::Typeof => Some(true),
            UnaryOperator::LogicalNot => to_boolean(&u.argument).map(|b| !b),
            _ => None,
        },
        Expression::ParenthesizedExpression(p) => to_boolean(&p.expression),
        _ => None,
    }
}

/// Whether `expr` is known not to be `null` or `undefined`.
fn not_nullish(expr: &Expression<'_>) -> bool {
    match expr {
        Expression::BooleanLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BigIntLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::TemplateLiteral(_)
        | Expression::RegExpLiteral(_)
        | Expression::FunctionExpression(_)
        | Expression::ArrowFunctionExpression(_)
        | Expression::ObjectExpression(_)
        | Expression::ArrayExpression(_)
        | Expression::ClassExpression(_) => true,
        Expression::ParenthesizedExpression(p) => not_nullish(&p.expression),
        _ => false,
    }
}

struct Checker<'a, 's> {
    scan: Scan<'s>,
    errors: Vec<(u32, String)>,
    declared: HashSet<&'a str>,
    /// Functions and arrow functions around the current node (`await` at 0 is top-level).
    fn_depth: u32,
    /// Statically dead branches around the current node.
    dead: u32,
    /// Deferred array spread errors: the last spread recorded in each context.
    ctxs: Vec<Option<u32>>,
    /// The deferred context each literal is parsed with, by address.
    registered: HashMap<usize, usize>,
    /// Destructuring assignment patterns followed by `=`, `in` or `of`.
    followed_by_eq: HashSet<usize>,
    /// Non-BMP identifiers esbuild cannot escape for ES5 (its default charset is ASCII):
    /// first declaration and first reference of each name.
    astral_decls: HashMap<String, u32>,
    astral_refs: HashMap<String, u32>,
}

impl<'a, 's> Checker<'a, 's> {
    fn new(src: &'s str, declared: HashSet<&'a str>) -> Self {
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

    fn finish(mut self) -> Vec<(u32, String)> {
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

    fn err(&mut self, pos: u32, message: String) {
        self.errors.push((pos, message));
    }

    fn feature(&mut self, pos: Option<u32>, what: &str) {
        if let Some(pos) = pos {
            self.err(pos, transforming(what));
        }
    }

    fn top_level_await(&mut self, pos: u32) {
        if self.fn_depth == 0 && self.dead == 0 {
            self.err(pos, format!("Top-level await is not available in {WHERE}"));
        }
    }

    fn astral(map: &mut HashMap<String, u32>, name: &str, pos: u32) {
        if name.chars().any(|c| u32::from(c) > 0xFFFF) {
            let e = map.entry(name.to_owned()).or_insert(pos);
            *e = (*e).min(pos);
        }
    }

    // ---- deferred array spread errors ----

    fn new_ctx(&mut self) -> usize {
        self.ctxs.push(None);
        self.ctxs.len() - 1
    }

    fn register(&mut self, literal: Option<usize>, ctx: usize) {
        if let Some(a) = literal {
            self.registered.insert(a, ctx);
        }
    }

    /// An array spread `...` at `pos` of a literal parsed with `ctx`: reported now, or kept in
    /// the context (replacing what it had).
    fn spread_event(&mut self, ctx: Option<usize>, pos: u32) {
        match ctx {
            None => self.err(pos, transforming("array spread")),
            Some(id) => self.ctxs[id] = Some(pos),
        }
    }

    /// The end of a literal: its own context (spreads of nested literals) is dropped when it is
    /// a pattern, reported when it was parsed without a context, else moved to that context.
    fn end_literal(&mut self, address: usize, ctx: Option<usize>, own: usize) {
        if self.followed_by_eq.contains(&address) {
            return;
        }
        if let Some(pos) = self.ctxs[own] {
            self.spread_event(ctx, pos);
        }
    }

    fn end_group(&mut self, ctx: usize) {
        if let Some(pos) = self.ctxs[ctx] {
            self.err(pos, transforming("array spread"));
        }
    }

    // ---- token positions ----

    /// The `=` of a default value after `end` (a binding or its type annotation).
    fn eq_after(&self, end: u32) -> Option<u32> {
        let p = self.scan.skip_trivia(end);
        let p = if self.scan.starts(p, "?") { p + 1 } else { p };
        self.scan.token(p, "=")
    }

    fn param_eq(&self, item: &FormalParameter<'_>) -> Option<u32> {
        let mut end = item.pattern.span().end;
        if let Some(t) = &item.type_annotation {
            end = end.max(t.span.end);
        }
        self.eq_after(end)
    }

    /// The first `word` token from `pos`, skipping trivia, words and `*`.
    fn find_word(&self, mut pos: u32, word: &str) -> Option<u32> {
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

    fn async_fn(&mut self, pos: Option<u32>, generator: bool) {
        let what = if generator {
            "async generator functions"
        } else {
            "async functions"
        };
        self.feature(pos, what);
    }

    // ---- functions ----

    /// Parameters as esbuild's `parseFn` reads them (functions and methods, not arrows).
    fn fn_params(&mut self, params: &FormalParameters<'a>) {
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
    fn fn_inner(&mut self, f: &Function<'a>) {
        self.fn_depth += 1;
        self.fn_params(&f.params);
        if let Some(body) = &f.body {
            self.visit_function_body(body);
        }
        self.fn_depth -= 1;
    }

    /// A method of an object literal or class: esbuild reports `async`, else `*`, else `(`.
    fn method(&mut self, start: u32, f: &Function<'a>) {
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
    fn cover_pattern(&mut self, pat: &BindingPattern<'a>) {
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

    fn cover_element(&mut self, el: &BindingPattern<'a>) {
        match el {
            BindingPattern::AssignmentPattern(a) => self.cover_element_default(a),
            e => self.cover_pattern(e),
        }
    }

    fn cover_element_default(&mut self, a: &AssignmentPattern<'a>) {
        let eq = self.eq_after(a.left.span().end);
        self.feature(eq, "default arguments");
        self.cover_pattern(&a.left);
        self.visit_expression(&a.right);
    }

    fn removable(&self, e: &Expression<'_>) -> bool {
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
}

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
