//! Lowers Rolldown's `es2015` bundle to ES5.
//!
//! Rolldown has already lowered ES2016+ (oxc's transformer at `es2015`). What reaches this pass
//! is the ES2015 that esbuild lowers itself and that [`super::check_es5`] lets through, plus
//! what Rolldown and oxc generate:
//!
//! - arrow functions (the runtime helpers, `__toESM`/`__exportAll` getters, dynamic import
//!   wrappers, and user code), with `this` and `arguments` captured like esbuild does;
//! - `let`/`const` (the runtime's `__exportAll`, TS namespaces, the `cjs` format's external
//!   requires), turned into `var` (see [`analyze`]);
//! - template literals (esbuild's `"a".concat(b, "c")`), tagged templates (esbuild's
//!   `__template` helper with a cached strings object per call site);
//! - shorthand properties and methods (also printed by oxc's code generator for `{a: a}`),
//!   computed keys (defined in order with `Object.defineProperty`);
//! - regular expressions with ES2015+ flags or syntax (`new RegExp(...)`, as esbuild), BigInt
//!   literals (`BigInt("...")`, as esbuild), `import()` and `import.meta` (as esbuild), optional
//!   catch bindings, block-level functions in strict code (`var f = function`, as esbuild);
//! - default and rest parameters, and spread arguments and elements, which the runtime and the
//!   minifier do not emit today but cost little to support.
//!
//! Anything else (classes, generators, destructuring, for-of, ...) is left in place for
//! [`super::verify`] to report.

use std::collections::{HashMap, HashSet};

use oxc::allocator::{Allocator, ArenaBox, ArenaVec, TakeIn};
use oxc::ast::AstKind;
use oxc::ast::ast::*;
use oxc::ast::builder::AstBuilder;
use oxc::ast_visit::{VisitMut, walk_mut};
use oxc::parser::{ParseOptions, Parser};
use oxc::semantic::{Scoping, Semantic, SemanticBuilder};
use oxc::span::{GetSpan, SPAN, SourceType, Span};
use oxc::str::{Ident, Str};
use oxc::syntax::number::NumberBase;
use oxc::syntax::operator::{AssignmentOperator, BinaryOperator, LogicalOperator};
use oxc::syntax::scope::{ScopeFlags, ScopeId};
use oxc::syntax::symbol::{SymbolFlags, SymbolId};

use super::{Lowered, print, verify};

/// An error at a byte offset of the input.
pub(super) type Error = (u32, String);

pub(super) fn lower(code: &str, minify: bool, sourcemap: bool) -> Result<Lowered, Error> {
    let allocator = Allocator::default();
    let options = ParseOptions {
        preserve_parens: false,
        ..ParseOptions::default()
    };
    let ret = Parser::new(&allocator, code, SourceType::unambiguous())
        .with_options(options)
        .parse();
    if let Some(e) = ret.diagnostics.errors().next() {
        let pos = e
            .labels
            .iter()
            .next()
            .map_or(0, oxc::diagnostics::LabeledSpan::offset);
        return Err((pos, format!("lower_to_es5: the bundle does not parse: {e}")));
    }
    let mut program = ret.program;
    let semantic = SemanticBuilder::new()
        .with_build_nodes(true)
        .build(&program)
        .semantic;
    let plan = analyze(&semantic)?;
    let scoping = semantic.into_scoping();
    let wrapper = iife_wrapper(&program);
    let mut lowerer = Lowerer::new(&allocator, scoping, plan, wrapper);
    lowerer.visit_program(&mut program);
    if let Some(e) = lowerer.error {
        return Err(e);
    }
    verify::verify(&program)?;
    Ok(print::print(&program, minify, sourcemap))
}

/// What [`analyze`] decided about block-scoped bindings.
struct Plan {
    /// Bindings that would clash once they are `var`s, and their new names.
    renames: HashMap<SymbolId, String>,
    /// Function declarations in blocks of strict code (block-scoped since ES2015).
    block_fns: HashSet<SymbolId>,
    /// Every name the program declares or references.
    used: HashSet<String>,
}

/// Plans turning `let`, `const` and block-level functions into `var`s.
///
/// A binding declared directly in a function (or the program) keeps its name: `let x` there
/// cannot coexist with another `x` in that scope. A binding in a block moves to the enclosing
/// function, so it is renamed when that function (including nested scopes) declares or
/// references another `x`, a binding further out or a global included; the new name is unused
/// in the whole program.
///
/// A binding in a loop is a new binding on every iteration; `var` is one for the whole function.
/// The two only differ when a closure captures the binding, which the generated code Rolldown
/// and oxc emit at `es2015` never does; this is an error rather than a wrong lowering.
fn analyze(semantic: &Semantic<'_>) -> Result<Plan, Error> {
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    let mut used: HashSet<String> = scoping.symbol_names().map(str::to_owned).collect();
    used.extend(
        scoping
            .root_unresolved_references()
            .keys()
            .map(|k| k.as_str().to_owned()),
    );
    let mut by_name: HashMap<&str, Vec<SymbolId>> = HashMap::new();
    for s in scoping.symbol_ids() {
        by_name.entry(scoping.symbol_name(s)).or_default().push(s);
    }
    let within = |inner: ScopeId, outer: ScopeId| {
        inner == outer || scoping.scope_is_descendant_of(inner, outer)
    };
    let mut plan = Plan {
        renames: HashMap::new(),
        block_fns: HashSet::new(),
        used,
    };
    for s in scoping.symbol_ids() {
        let flags = scoping.symbol_flags(s);
        let scope = scoping.symbol_scope_id(s);
        let scope_flags = scoping.scope_flags(scope);
        let lexical = flags.contains(SymbolFlags::BlockScopedVariable);
        let block_fn = flags.contains(SymbolFlags::Function)
            && !scope_flags.intersects(ScopeFlags::Var)
            && scope_flags.contains(ScopeFlags::StrictMode)
            && matches!(nodes.kind(scoping.symbol_declaration(s)), AstKind::Function(f) if f.is_declaration());
        if !lexical && !block_fn {
            continue;
        }
        let function = var_scope(scoping, scope);
        let function_node = scoping.get_node_id(function);
        let decl = scoping.symbol_declaration(s);
        let in_loop = nodes
            .ancestors(decl)
            .take_while(|n| n.id() != function_node)
            .any(|n| {
                matches!(
                    n.kind(),
                    AstKind::ForStatement(_)
                        | AstKind::ForInStatement(_)
                        | AstKind::ForOfStatement(_)
                        | AstKind::WhileStatement(_)
                        | AstKind::DoWhileStatement(_)
                )
            });
        if in_loop {
            for &r in scoping.get_resolved_reference_ids(s) {
                let from = scoping.get_reference(r).scope_id();
                let captured = scoping
                    .scope_ancestors(from)
                    .take_while(|&x| x != scope)
                    .any(|x| scoping.scope_flags(x).contains(ScopeFlags::Function));
                if captured {
                    return Err((
                        scoping.symbol_span(s).start,
                        format!(
                            "lower_to_es5: \"{}\" is declared in a loop and captured by a closure; ES5 has no per-iteration bindings",
                            scoping.symbol_name(s)
                        ),
                    ));
                }
            }
        }
        if block_fn {
            plan.block_fns.insert(s);
        }
        if scope == function {
            continue;
        }
        let name = scoping.symbol_name(s);
        let others = by_name.get(name).map_or(&[][..], Vec::as_slice);
        let clash = others.iter().any(|&t| {
            t != s
                && (within(scoping.symbol_scope_id(t), function)
                    || scoping
                        .get_resolved_reference_ids(t)
                        .iter()
                        .any(|&r| within(scoping.get_reference(r).scope_id(), function)))
        }) || scoping
            .root_unresolved_references()
            .iter()
            .filter(|(k, _)| k.as_str() == name)
            .flat_map(|(_, refs)| refs.iter())
            .any(|&r| within(scoping.get_reference(r).scope_id(), function));
        if clash {
            let fresh = fresh_name(&mut plan.used, name);
            plan.renames.insert(s, fresh);
        }
    }
    Ok(plan)
}

/// The function (or program) scope that a `var` in `scope` belongs to.
fn var_scope(scoping: &Scoping, mut scope: ScopeId) -> ScopeId {
    loop {
        if scoping.scope_flags(scope).intersects(ScopeFlags::Var) {
            return scope;
        }
        match scoping.scope_parent_id(scope) {
            Some(p) => scope = p,
            None => return scope,
        }
    }
}

/// `base`, or `base2`, `base3`, ... : the first not in `used` (then marked used).
fn fresh_name(used: &mut HashSet<String>, base: &str) -> String {
    let mut name = base.to_owned();
    let mut n = 2;
    while used.contains(&name) {
        name = format!("{base}{n}");
        n += 1;
    }
    used.insert(name.clone());
    name
}

/// The span of the function of an IIFE bundle (`(function () { ... })(...)`, possibly
/// assigned to a `var`): helpers go at the top of its body rather than the global scope.
fn iife_wrapper(program: &Program<'_>) -> Option<Span> {
    let mut stmts = program
        .body
        .iter()
        .filter(|s| !matches!(s, Statement::EmptyStatement(_)));
    let only = stmts.next()?;
    if stmts.next().is_some() {
        return None;
    }
    let expr = match only {
        Statement::ExpressionStatement(e) => &e.expression,
        Statement::VariableDeclaration(d) if d.declarations.len() == 1 => {
            d.declarations[0].init.as_ref()?
        }
        _ => return None,
    };
    let Expression::CallExpression(call) = expr.without_parentheses() else {
        return None;
    };
    match call.callee.without_parentheses() {
        Expression::FunctionExpression(f) if !f.r#async && !f.generator => Some(f.span),
        _ => None,
    }
}

/// What a [`FnCtx`] is.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum CtxKind {
    Program,
    #[default]
    Function,
    Arrow,
}

/// A function (or the program) being lowered.
#[derive(Default)]
struct FnCtx<'a> {
    kind: CtxKind,
    this_used: bool,
    args_used: bool,
    /// Temporaries declared with `var` at the top of the function.
    temps: Vec<Ident<'a>>,
}

/// The ES5 helpers this pass can add (esbuild's, by the names it gives them where it has them).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
enum Helper {
    DefProp,
    Template,
    ToArray,
    CopyProps,
    ToEsm,
    Require,
}

impl Helper {
    fn base(self) -> &'static str {
        match self {
            Self::DefProp => "__defProp",
            Self::Template => "__template",
            Self::ToArray => "__toArray",
            Self::CopyProps => "__copyProps",
            Self::ToEsm => "__toESM",
            Self::Require => "__require",
        }
    }

    /// The helper's source; `$NAME` placeholders are other helpers' names.
    fn source(self) -> &'static str {
        match self {
            Self::DefProp => "var $defProp = Object.defineProperty;",
            Self::Template => {
                "var $template = function (cooked, raw) { return Object.freeze($defProp(cooked, \"raw\", { value: Object.freeze(raw || cooked.slice()) })); };"
            }
            Self::ToArray => {
                "var $toArray = function (a) { if (Array.isArray(a)) return a; if (a != null && typeof Symbol === \"function\" && typeof a[Symbol.iterator] === \"function\") { for (var it = a[Symbol.iterator](), r = [], s; !(s = it.next()).done;) r.push(s.value); return r; } return Array.prototype.slice.call(a); };"
            }
            Self::CopyProps => {
                "var $copyProps = function (to, from, except, desc) { if (from && typeof from === \"object\" || typeof from === \"function\") for (var keys = Object.getOwnPropertyNames(from), i = 0, n = keys.length, key; i < n; i++) { key = keys[i]; if (!Object.prototype.hasOwnProperty.call(to, key) && key !== except) $defProp(to, key, { get: function (k) { return from[k]; }.bind(null, key), enumerable: !(desc = Object.getOwnPropertyDescriptor(from, key)) || desc.enumerable }); } return to; };"
            }
            Self::ToEsm => {
                "var $toESM = function (mod, isNodeMode, target) { return target = mod != null ? Object.create(Object.getPrototypeOf(mod)) : {}, $copyProps(isNodeMode || !mod || !mod.__esModule ? $defProp(target, \"default\", { value: mod, enumerable: true }) : target, mod); };"
            }
            Self::Require => {
                "var $require = function (x) { return typeof require !== \"undefined\" ? require : typeof Proxy !== \"undefined\" ? new Proxy(x, { get: function (a, b) { return (typeof require !== \"undefined\" ? require : a)[b]; } }) : x; }(function (x) { if (typeof require !== \"undefined\") return require.apply(this, arguments); throw Error(\"Dynamic require of \\\"\" + x + \"\\\" is not supported\"); });"
            }
        }
    }

    fn deps(self) -> &'static [Self] {
        match self {
            Self::Template | Self::CopyProps => &[Self::DefProp],
            Self::ToEsm => &[Self::CopyProps, Self::DefProp],
            Self::DefProp | Self::ToArray | Self::Require => &[],
        }
    }
}

struct Lowerer<'a> {
    allocator: &'a Allocator,
    scoping: Scoping,
    renames: HashMap<SymbolId, Ident<'a>>,
    block_fns: HashSet<SymbolId>,
    used: HashSet<String>,
    ctxs: Vec<FnCtx<'a>>,
    this_name: Option<Ident<'a>>,
    args_name: Option<Ident<'a>>,
    import_meta: Option<Ident<'a>>,
    helpers: HashMap<Helper, Ident<'a>>,
    /// Template strings caches: `var` at the top level.
    caches: Vec<Ident<'a>>,
    wrapper: Option<Span>,
    /// Top-level declarations already inserted (in the IIFE wrapper).
    top_done: bool,
    /// The left of a for-in/for-of is being visited (its `let x` gets no `= void 0`).
    in_for_left: bool,
    error: Option<Error>,
}

impl<'a> Lowerer<'a> {
    fn new(allocator: &'a Allocator, scoping: Scoping, plan: Plan, wrapper: Option<Span>) -> Self {
        let renames = plan
            .renames
            .into_iter()
            .map(|(s, n)| (s, Ident::from_str_in(&n, &AstBuilder::new(allocator))))
            .collect();
        Self {
            allocator,
            scoping,
            renames,
            block_fns: plan.block_fns,
            used: plan.used,
            ctxs: Vec::new(),
            this_name: None,
            args_name: None,
            import_meta: None,
            helpers: HashMap::new(),
            caches: Vec::new(),
            wrapper,
            top_done: false,
            in_for_left: false,
            error: None,
        }
    }

    fn ast(&self) -> AstBuilder<'a> {
        AstBuilder::new(self.allocator)
    }

    fn id(&self, name: &str) -> Ident<'a> {
        Ident::from_str_in(name, &self.ast())
    }

    fn text(&self, value: &str) -> Str<'a> {
        Str::from_str_in(value, &self.ast())
    }

    fn fail(&mut self, pos: u32, message: String) {
        if self.error.is_none() {
            self.error = Some((pos, message));
        }
    }

    fn fresh(&mut self, base: &str) -> Ident<'a> {
        let name = fresh_name(&mut self.used, base);
        self.id(&name)
    }

    fn helper(&mut self, h: Helper) -> Ident<'a> {
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
    fn temp(&mut self, base: &str) -> Ident<'a> {
        let name = self.fresh(base);
        if let Some(ctx) = self.ctxs.last_mut() {
            ctx.temps.push(name);
        }
        name
    }

    /// The index of the function whose `this` an arrow function at the current position sees,
    /// when inside an arrow function.
    fn arrow_owner(&self) -> Option<usize> {
        if !self.ctxs.last().is_some_and(|c| c.kind == CtxKind::Arrow) {
            return None;
        }
        self.ctxs.iter().rposition(|c| c.kind != CtxKind::Arrow)
    }

    // ---- node builders ----

    fn ident(&self, span: Span, name: Ident<'a>) -> Expression<'a> {
        Expression::new_identifier(span, name, &self.ast())
    }

    fn global(&self, name: &str) -> Expression<'a> {
        Expression::new_identifier(SPAN, self.id(name), &self.ast())
    }

    fn string(&self, span: Span, value: &str, lone_surrogates: bool) -> Expression<'a> {
        Expression::new_string_literal_with_lone_surrogates(
            span,
            self.text(value),
            None,
            lone_surrogates,
            &self.ast(),
        )
    }

    fn member(&self, object: Expression<'a>, property: &str) -> Expression<'a> {
        let ast = self.ast();
        let property = IdentifierName::new(SPAN, self.id(property), &ast);
        Expression::new_static_member_expression(SPAN, object, property, false, &ast)
    }

    fn call(
        &self,
        span: Span,
        callee: Expression<'a>,
        args: Vec<Expression<'a>>,
    ) -> Expression<'a> {
        let ast = self.ast();
        let args = ArenaVec::from_iter_in(args.into_iter().map(Argument::from), &ast);
        Expression::new_call_expression(span, callee, None, args, false, &ast)
    }

    fn assign(&self, target: Ident<'a>, value: Expression<'a>) -> Expression<'a> {
        let ast = self.ast();
        let target = AssignmentTarget::from(SimpleAssignmentTarget::AssignmentTargetIdentifier(
            ArenaBox::new_in(IdentifierReference::new(SPAN, target, &ast), &ast),
        ));
        Expression::new_assignment_expression(SPAN, AssignmentOperator::Assign, target, value, &ast)
    }

    fn void0(&self, span: Span) -> Expression<'a> {
        Expression::new_void_0(span, &self.ast())
    }

    fn var_stmt(&self, decls: Vec<(Ident<'a>, Option<Expression<'a>>)>) -> Statement<'a> {
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

    fn function_expr(
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
    fn parse_statements(&self, text: &str) -> ArenaVec<'a, Statement<'a>> {
        let text = self.allocator.alloc_str(text);
        let ret = Parser::new(self.allocator, text, SourceType::cjs()).parse();
        let mut program = ret.program;
        ZeroSpans.visit_program(&mut program);
        program.body
    }

    /// The helpers, template caches and `import.meta` object for the top of the bundle.
    fn top_statements(&mut self) -> Vec<Statement<'a>> {
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
    fn prologue(
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
    fn lower_params(&mut self, params: &mut FormalParameters<'a>) -> Vec<Statement<'a>> {
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

    // ---- expressions ----

    fn lower_arrow(
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

    /// esbuild's lowering: `"a".concat(b, "c").concat(d)`, one call per substitution so that
    /// each is converted to a string before the next is evaluated.
    fn lower_template(&self, t: TemplateLiteral<'a>) -> Expression<'a> {
        let quasi = |q: &TemplateElement<'a>| {
            let cooked = q.value.cooked.map_or("", |c| c.as_str());
            self.string(q.span, cooked, q.lone_surrogates)
        };
        let mut acc = quasi(&t.quasis[0]);
        for (i, e) in t.expressions.into_iter().enumerate() {
            let mut args = vec![e];
            let next = &t.quasis[i + 1];
            if next.value.cooked.is_some_and(|c| !c.is_empty()) {
                args.push(quasi(next));
            }
            let callee = self.member(acc, "concat");
            acc = self.call(t.span, callee, args);
        }
        acc
    }

    fn lower_tagged(&mut self, t: ArenaBox<'a, TaggedTemplateExpression<'a>>) -> Expression<'a> {
        let ast = self.ast();
        let TaggedTemplateExpression {
            span, tag, quasi, ..
        } = t.unbox();
        let template = self.helper(Helper::Template);
        let cache = self.fresh("_t");
        self.caches.push(cache);
        let mut differ = false;
        let mut cooked = Vec::new();
        let mut raw = Vec::new();
        for q in &quasi.quasis {
            let r = q
                .value
                .raw
                .as_str()
                .replace("\r\n", "\n")
                .replace('\r', "\n");
            match q.value.cooked {
                Some(c) => {
                    differ |= c.as_str() != r;
                    cooked.push(self.string(q.span, c.as_str(), q.lone_surrogates));
                }
                None => {
                    differ = true;
                    cooked.push(self.void0(q.span));
                }
            }
            raw.push(self.string(q.span, &r, false));
        }
        let array = |items: Vec<Expression<'a>>| {
            Expression::new_array_expression(
                SPAN,
                ArenaVec::from_iter_in(items.into_iter().map(ArrayExpressionElement::from), &ast),
                &ast,
            )
        };
        let mut args = vec![array(cooked)];
        if differ {
            args.push(array(raw));
        }
        let make = self.call(SPAN, self.ident(SPAN, template), args);
        let cached = Expression::new_logical_expression(
            SPAN,
            self.ident(SPAN, cache),
            LogicalOperator::Or,
            self.assign(cache, make),
            &ast,
        );
        let mut call_args = vec![cached];
        call_args.extend(quasi.expressions);
        self.call(span, tag, call_args)
    }

    /// Shorthand properties, methods, and keys oxc's code generator would print as ES2015;
    /// computed keys as a sequence of definitions.
    fn lower_object(&mut self, expr: &mut Expression<'a>) {
        let Expression::ObjectExpression(obj) = expr else {
            return;
        };
        let ast = self.ast();
        let mut first_define = None;
        for (i, prop) in obj.properties.iter_mut().enumerate() {
            let ObjectPropertyKind::ObjectProperty(p) = prop else {
                continue; // object spread: reported by the verification
            };
            let proto_shorthand = p.shorthand
                && matches!(&p.key, PropertyKey::StaticIdentifier(k) if k.name == "__proto__");
            p.method = false;
            p.shorthand = false;
            if p.computed {
                let static_key = match &p.key {
                    PropertyKey::StringLiteral(s) => s.value != "__proto__",
                    PropertyKey::NumericLiteral(n) => n.value.is_finite() && n.value >= 0.0,
                    _ => false,
                };
                if static_key {
                    p.computed = false;
                } else if first_define.is_none() {
                    first_define = Some(i);
                }
            } else if proto_shorthand && first_define.is_none() {
                first_define = Some(i);
            }
            if !p.computed {
                // oxc prints `a: a` as `a`, and a negative or infinite number key in brackets.
                let replace = match &p.key {
                    PropertyKey::StaticIdentifier(k) => {
                        matches!(&p.value, Expression::Identifier(v) if v.name == k.name)
                            .then(|| k.name.as_str().to_owned())
                    }
                    PropertyKey::NumericLiteral(n) if !n.value.is_finite() || n.value < 0.0 => {
                        Some(number_key(n.value))
                    }
                    PropertyKey::BigIntLiteral(b) => Some(b.value.as_str().to_owned()),
                    _ => None,
                };
                if let Some(name) = replace {
                    let span = p.key.span();
                    p.key = PropertyKey::StringLiteral(ArenaBox::new_in(
                        StringLiteral::new(span, self.text(&name), None, &ast),
                        &ast,
                    ));
                }
            }
        }
        let Some(first) = first_define else {
            return;
        };
        let span = obj.span;
        let def_prop = self.helper(Helper::DefProp);
        let temp = self.temp("_o");
        let rest: Vec<ObjectPropertyKind<'a>> = obj.properties.drain(first..).collect();
        let prefix = expr.take_in(&ast);
        let mut seq = vec![self.assign(temp, prefix)];
        for prop in rest {
            let ObjectPropertyKind::ObjectProperty(p) = prop else {
                self.fail(
                    span.start,
                    "lower_to_es5: object spread after a computed key".to_owned(),
                );
                continue;
            };
            let p = p.unbox();
            let key_span = p.key.span();
            let is_proto = !p.computed
                && matches!(&p.key, PropertyKey::StaticIdentifier(k) if k.name == "__proto__");
            if is_proto
                && p.kind == PropertyKind::Init
                && !matches!(&p.value, Expression::Identifier(v) if v.name == "__proto__")
            {
                // `__proto__: v` sets the prototype.
                let target = SimpleAssignmentTarget::from(
                    MemberExpression::StaticMemberExpression(ArenaBox::new_in(
                        StaticMemberExpression::new(
                            SPAN,
                            self.ident(SPAN, temp),
                            IdentifierName::new(SPAN, self.id("__proto__"), &ast),
                            false,
                            &ast,
                        ),
                        &ast,
                    )),
                );
                seq.push(Expression::new_assignment_expression(
                    p.span,
                    AssignmentOperator::Assign,
                    AssignmentTarget::from(target),
                    p.value,
                    &ast,
                ));
                continue;
            }
            let key = match p.key {
                PropertyKey::StaticIdentifier(k) => self.string(key_span, k.name.as_str(), false),
                PropertyKey::PrivateIdentifier(_) => {
                    self.fail(
                        key_span.start,
                        "lower_to_es5: private name in an object literal".to_owned(),
                    );
                    continue;
                }
                k => k.into_expression(),
            };
            let flag = |name: &str| {
                ObjectPropertyKind::new_object_property(
                    SPAN,
                    PropertyKind::Init,
                    PropertyKey::StaticIdentifier(ArenaBox::new_in(
                        IdentifierName::new(SPAN, self.id(name), &ast),
                        &ast,
                    )),
                    Expression::new_boolean_literal(SPAN, true, &ast),
                    false,
                    false,
                    false,
                    &ast,
                )
            };
            let field = match p.kind {
                PropertyKind::Init => "value",
                PropertyKind::Get => "get",
                PropertyKind::Set => "set",
            };
            let mut descriptor = vec![ObjectPropertyKind::new_object_property(
                SPAN,
                PropertyKind::Init,
                PropertyKey::StaticIdentifier(ArenaBox::new_in(
                    IdentifierName::new(SPAN, self.id(field), &ast),
                    &ast,
                )),
                p.value,
                false,
                false,
                false,
                &ast,
            )];
            descriptor.push(flag("enumerable"));
            descriptor.push(flag("configurable"));
            if p.kind == PropertyKind::Init {
                descriptor.push(flag("writable"));
            }
            let descriptor = Expression::new_object_expression(
                SPAN,
                ArenaVec::from_iter_in(descriptor, &ast),
                &ast,
            );
            seq.push(self.call(
                p.span,
                self.ident(SPAN, def_prop),
                vec![self.ident(SPAN, temp), key, descriptor],
            ));
        }
        seq.push(self.ident(SPAN, temp));
        *expr = Expression::new_sequence_expression(span, ArenaVec::from_iter_in(seq, &ast), &ast);
    }

    /// The elements of an array literal or argument list with spreads, as one array:
    /// `[a].concat(__toArray(b), [c])`.
    fn spread_array(
        &mut self,
        span: Span,
        items: Vec<(bool, Option<Expression<'a>>)>,
    ) -> Expression<'a> {
        let ast = self.ast();
        let to_array = self.helper(Helper::ToArray);
        let mut chunks: Vec<Expression<'a>> = Vec::new();
        let mut run: Vec<ArrayExpressionElement<'a>> = Vec::new();
        let flush = |run: &mut Vec<ArrayExpressionElement<'a>>,
                     chunks: &mut Vec<Expression<'a>>| {
            if !run.is_empty() {
                chunks.push(Expression::new_array_expression(
                    SPAN,
                    ArenaVec::from_iter_in(run.drain(..), &ast),
                    &ast,
                ));
            }
        };
        for (spread, item) in items {
            match (spread, item) {
                (true, Some(e)) => {
                    flush(&mut run, &mut chunks);
                    chunks.push(self.call(e.span(), self.ident(SPAN, to_array), vec![e]));
                }
                (_, Some(e)) => run.push(ArrayExpressionElement::from(e)),
                (_, None) => run.push(ArrayExpressionElement::new_elision(SPAN, &ast)),
            }
        }
        flush(&mut run, &mut chunks);
        let mut chunks = chunks.into_iter();
        let first = chunks.next().unwrap_or_else(|| {
            Expression::new_array_expression(SPAN, ArenaVec::new_in(&ast), &ast)
        });
        let rest: Vec<Expression<'a>> = chunks.collect();
        if rest.is_empty() && !matches!(first, Expression::ArrayExpression(_)) {
            // `[...a]` alone: a copy, as spreading makes one.
            let empty = Expression::new_array_expression(SPAN, ArenaVec::new_in(&ast), &ast);
            return self.call(span, self.member(empty, "concat"), vec![first]);
        }
        if rest.is_empty() {
            return first;
        }
        let first = if matches!(first, Expression::ArrayExpression(_)) {
            first
        } else {
            let empty = Expression::new_array_expression(SPAN, ArenaVec::new_in(&ast), &ast);
            return {
                let mut all = vec![first];
                all.extend(rest);
                self.call(span, self.member(empty, "concat"), all)
            };
        };
        self.call(span, self.member(first, "concat"), rest)
    }

    fn lower_spread_call(&mut self, expr: &mut Expression<'a>) {
        let ast = self.ast();
        let Expression::CallExpression(call) = expr else {
            return;
        };
        let span = call.span;
        let items: Vec<(bool, Option<Expression<'a>>)> = call
            .arguments
            .drain(..)
            .map(|a| match a {
                Argument::SpreadElement(s) => (true, Some(s.unbox().argument)),
                a => (false, Some(a.into_expression())),
            })
            .collect();
        let args = self.spread_array(span, items);
        let callee = call.callee.take_in(&ast);
        let (callee, this) = match callee {
            Expression::StaticMemberExpression(m) => {
                let mut m = m.unbox();
                let (object, this) = self.reusable(m.object);
                m.object = object;
                (
                    Expression::StaticMemberExpression(ArenaBox::new_in(m, &ast)),
                    this,
                )
            }
            Expression::ComputedMemberExpression(m) => {
                let mut m = m.unbox();
                let (object, this) = self.reusable(m.object);
                m.object = object;
                (
                    Expression::ComputedMemberExpression(ArenaBox::new_in(m, &ast)),
                    this,
                )
            }
            Expression::Super(s) => {
                self.fail(
                    s.span.start,
                    "lower_to_es5: spread arguments to super()".to_owned(),
                );
                (Expression::Super(s), self.void0(SPAN))
            }
            callee => (callee, self.void0(SPAN)),
        };
        *expr = self.call(span, self.member(callee, "apply"), vec![this, args]);
    }

    /// `object` to use twice: itself when it has no side effects, else `(_r = object)` and `_r`.
    fn reusable(&mut self, object: Expression<'a>) -> (Expression<'a>, Expression<'a>) {
        match &object {
            Expression::Identifier(id) => {
                let again = self.ident(SPAN, id.name);
                (object, again)
            }
            Expression::ThisExpression(_) => {
                (object, Expression::new_this_expression(SPAN, &self.ast()))
            }
            _ => {
                let temp = self.temp("_r");
                (self.assign(temp, object), self.ident(SPAN, temp))
            }
        }
    }

    fn lower_spread_new(&mut self, expr: &mut Expression<'a>) {
        let ast = self.ast();
        let Expression::NewExpression(new) = expr else {
            return;
        };
        let span = new.span;
        let mut items = vec![(false, Some(Expression::new_null_literal(SPAN, &ast)))];
        items.extend(new.arguments.drain(..).map(|a| match a {
            Argument::SpreadElement(s) => (true, Some(s.unbox().argument)),
            a => (false, Some(a.into_expression())),
        }));
        let args = self.spread_array(span, items);
        let callee = new.callee.take_in(&ast);
        let bind = self.member(
            self.member(self.member(self.global("Function"), "prototype"), "bind"),
            "apply",
        );
        let bound = self.call(span, bind, vec![callee, args]);
        *expr = Expression::new_new_expression(span, bound, None, ArenaVec::new_in(&ast), &ast);
    }

    fn lower_regexp(&self, r: &RegExpLiteral<'a>) -> Option<Expression<'a>> {
        let flags = r.regex.flags.to_inline_string();
        let pattern = r.regex.pattern.text.as_str();
        let unicode = flags.contains('u') || flags.contains('v');
        if flags.chars().all(|c| matches!(c, 'g' | 'i' | 'm'))
            && !pattern_is_es2018(pattern, unicode)
        {
            return None;
        }
        let ast = self.ast();
        let mut args = vec![self.string(r.span, pattern, false)];
        if !flags.is_empty() {
            args.push(self.string(SPAN, flags.as_str(), false));
        }
        let args = ArenaVec::from_iter_in(args.into_iter().map(Argument::from), &ast);
        Some(Expression::new_new_expression(
            r.span,
            self.global("RegExp"),
            None,
            args,
            &ast,
        ))
    }

    fn lower_bigint(&self, b: &BigIntLiteral<'a>) -> Expression<'a> {
        let ast = self.ast();
        let raw = b.raw.map_or(b.value.as_str(), |r| r.as_str());
        let digits: String = raw
            .trim_end_matches('n')
            .chars()
            .filter(|&c| c != '_')
            .collect();
        let args =
            ArenaVec::from_array_in([Argument::from(self.string(SPAN, &digits, false))], &ast);
        Expression::new_call_expression_with_pure(
            b.span,
            self.global("BigInt"),
            None,
            args,
            false,
            true,
            &ast,
        )
    }

    fn lower_import(&mut self, imp: ArenaBox<'a, ImportExpression<'a>>) -> Expression<'a> {
        let ast = self.ast();
        let imp = imp.unbox();
        // As esbuild: options without side effects are dropped, others cannot be kept in ES5.
        if let Some(options) = &imp.options
            && !side_effect_free(options)
        {
            self.fail(
                options.span().start,
                format!(
                    "Using an arbitrary value as the second argument to \"import()\" is not possible in {}",
                    super::check::WHERE
                ),
            );
        }
        let require = self.helper(Helper::Require);
        let to_esm = self.helper(Helper::ToEsm);
        let required = self.call(SPAN, self.ident(SPAN, require), vec![imp.source]);
        let module = self.call(SPAN, self.ident(SPAN, to_esm), vec![required]);
        let ret = Statement::new_return_statement(SPAN, Some(module), &ast);
        let body = ArenaBox::new_in(
            FunctionBody::new(
                SPAN,
                ArenaVec::new_in(&ast),
                ArenaVec::from_array_in([ret], &ast),
                &ast,
            ),
            &ast,
        );
        let params = ArenaBox::new_in(
            FormalParameters::new(
                SPAN,
                FormalParameterKind::FormalParameter,
                ArenaVec::new_in(&ast),
                None,
                &ast,
            ),
            &ast,
        );
        let then_fn = self.function_expr(SPAN, params, body);
        let resolved = self.call(SPAN, self.member(self.global("Promise"), "resolve"), vec![]);
        self.call(imp.span, self.member(resolved, "then"), vec![then_fn])
    }

    /// Block-level function declarations of `stmts` (in strict code) as `var f = function`
    /// statements, removed from `stmts`.
    fn take_block_fns(&self, stmts: &mut ArenaVec<'a, Statement<'a>>) -> Vec<Statement<'a>> {
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

/// A number property key as the string it names (for the keys oxc would print in brackets).
fn number_key(value: f64) -> String {
    if value.is_nan() {
        "NaN".to_owned()
    } else if value.is_infinite() {
        if value > 0.0 { "Infinity" } else { "-Infinity" }.to_owned()
    } else if value == 0.0 {
        "0".to_owned()
    } else {
        format!("{value}")
    }
}

/// Whether evaluating `e` has no side effects (a simple version of esbuild's
/// `ExprCanBeRemovedIfUnused`; an unbound identifier was already rejected by the check).
fn side_effect_free(e: &Expression<'_>) -> bool {
    match e {
        Expression::BooleanLiteral(_)
        | Expression::NullLiteral(_)
        | Expression::NumericLiteral(_)
        | Expression::BigIntLiteral(_)
        | Expression::StringLiteral(_)
        | Expression::RegExpLiteral(_)
        | Expression::Identifier(_)
        | Expression::FunctionExpression(_)
        | Expression::ArrowFunctionExpression(_) => true,
        Expression::TemplateLiteral(t) => t.expressions.is_empty(),
        Expression::ArrayExpression(a) => a.elements.iter().all(|el| match el {
            ArrayExpressionElement::SpreadElement(_) => false,
            ArrayExpressionElement::Elision(_) => true,
            e => e.as_expression().is_some_and(side_effect_free),
        }),
        Expression::ObjectExpression(o) => o.properties.iter().all(|p| match p {
            ObjectPropertyKind::ObjectProperty(p) => {
                (!p.computed || p.key.as_expression().is_some_and(side_effect_free))
                    && (p.method || p.kind != PropertyKind::Init || side_effect_free(&p.value))
            }
            ObjectPropertyKind::SpreadProperty(_) => false,
        }),
        Expression::UnaryExpression(u) => side_effect_free(&u.argument),
        _ => false,
    }
}

/// Whether a regular expression pattern uses ES2018 syntax (lookbehind, named groups, Unicode
/// property escapes), scanned as esbuild scans it.
pub(super) fn pattern_is_es2018(pattern: &str, unicode: bool) -> bool {
    let b = pattern.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\\' => {
                if unicode
                    && matches!(b.get(i + 1), Some(b'p' | b'P'))
                    && b.get(i + 2) == Some(&b'{')
                {
                    return true;
                }
                i += 2;
            }
            b'[' => {
                i += 1;
                while i < b.len() && b[i] != b']' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                i += 1;
            }
            b'(' => {
                let tail = &pattern[i + 1..];
                if tail.starts_with("?<=")
                    || tail.starts_with("?<!")
                    || (tail.starts_with("?<") && tail.contains('>'))
                {
                    return true;
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    false
}

/// Empties the spans of parsed helper code, which has no position in the input.
struct ZeroSpans;

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

impl<'a> Lowerer<'a> {
    fn split_export_star_as(
        &mut self,
        e: ExportAllDeclaration<'a>,
    ) -> (Statement<'a>, Statement<'a>) {
        let ast = self.ast();
        let exported = e.exported.unwrap_or_else(|| {
            ModuleExportName::IdentifierName(IdentifierName::new(SPAN, self.id("ns"), &ast))
        });
        let local = self.fresh(match &exported {
            ModuleExportName::IdentifierName(n) => n.name.as_str(),
            ModuleExportName::IdentifierReference(n) => n.name.as_str(),
            ModuleExportName::StringLiteral(_) => "ns",
        });
        let specifier = ImportDeclarationSpecifier::new_import_namespace_specifier(
            SPAN,
            BindingIdentifier::new(SPAN, local, &ast),
            &ast,
        );
        let import = Statement::new_import_declaration(
            e.span,
            Some(ArenaVec::from_array_in([specifier], &ast)),
            e.source,
            None,
            e.with_clause,
            ImportOrExportKind::Value,
            &ast,
        );
        let spec = ExportSpecifier::new(
            SPAN,
            ModuleExportName::IdentifierReference(IdentifierReference::new(SPAN, local, &ast)),
            exported,
            ImportOrExportKind::Value,
            &ast,
        );
        let export = Statement::new_export_named_declaration(
            SPAN,
            ArenaVec::from_array_in([spec], &ast),
            ImportOrExportKind::Value,
            &ast,
        );
        (import, export)
    }
}
