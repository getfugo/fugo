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

mod checker;
mod visit;

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
