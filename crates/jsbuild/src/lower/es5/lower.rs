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

mod analyze;
mod expressions;
mod functions;
mod nodes;
mod objects;
mod visit;

use analyze::*;
use objects::*;
use visit::*;

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

    // ---- node builders ----

    // ---- expressions ----
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
