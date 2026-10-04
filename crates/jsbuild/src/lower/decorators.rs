//! TC39 decorators (the Stage 3 proposal) lowered for our Rolldown-based `js.Build`.
//!
//! Rolldown pins oxc 0.152, whose transformer lowers only TypeScript's legacy
//! `experimentalDecorators` and prints standard decorators verbatim. [`lower_decorators`] runs in
//! Rolldown's `transform` hook, before Rolldown's own oxc transform, and rewrites decorated
//! classes the way esbuild 0.25.6 does (`internal/js_parser/js_parser_lower_class.go`), so that
//! `js.Build` keeps esbuild's semantics:
//!
//! - decorator expressions and computed keys are evaluated once, in source order, inside the class
//!   heritage or a computed key (keeping the `this`, `await` and `arguments` of the enclosing
//!   code), or just before the class;
//! - in a class with decorated members every field, auto-accessor and static block moves out of
//!   the class body (instance ones into the constructor) and every private name of the class
//!   becomes a `WeakMap`/`WeakSet`, so that decorators can replace private methods and wrap
//!   initializers; `this`, `super` and `new.target` in moved code are rewritten;
//! - decorators run through esbuild's runtime helpers, ported in [`DECORATOR_HELPERS`] and
//!   imported from the caller's module so that Rolldown keeps one copy per bundle.
//!
//! Auto-accessors (`accessor x`) without decorators become a private field with a getter and a
//! setter, as esbuild does for targets without decorators (oxc 0.152 leaves them as they are).
//!
//! The output stays in the input's language: oxc's codegen prints TypeScript and JSX, which
//! Rolldown strips afterwards, and it uses ES2022 syntax (class fields, private names, static
//! blocks) that Rolldown lowers to the build target.
//!
//! Where this differs from esbuild 0.25.6 (all checked against node running the code natively):
//!
//! - classes keep their names (esbuild's `Foo.name` becomes `_Foo` or `_a` when it captures the
//!   class); an anonymous class expression is named from its context (`const Foo = class {}`),
//!   an anonymous `export default` class decorator sees the name `default` (esbuild: `""`);
//! - `super.m()` in a static initializer or static block moved out of the class calls `m` on the
//!   class (esbuild passes the enclosing `this`), and `o?.#m?.()` keeps `this` (esbuild loses it);
//! - `new.target` in a field initializer or static block that moves is `undefined`;
//! - a derived constructor that never calls `super()` does not initialize the moved fields
//!   (esbuild initializes them first, which throws).
//!
//! Like esbuild: fields have define semantics (TypeScript's `useDefineForClassFields: false` is
//! not honoured), temporaries are `var`s of the enclosing function (a decorated class in a loop
//! shares them between iterations), and code moved out of a class body runs in the strictness of
//! the enclosing code.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::PathBuf;

use oxc::allocator::{Allocator, ArenaBox, ArenaVec, CloneIn, TakeIn};
use oxc::ast::ast::*;
use oxc::ast::builder::AstBuilder;
use oxc::ast_visit::{Visit, VisitMut, walk, walk_mut};
use oxc::codegen::{Codegen, CodegenOptions};
use oxc::parser::Parser;
use oxc::semantic::{Scoping, SemanticBuilder, SymbolId};
use oxc::span::{SPAN, SourceType, Span};
use oxc::syntax::identifier::is_identifier_name;
use oxc::syntax::keyword::is_reserved_keyword;
use oxc::syntax::number::NumberBase;
use oxc::syntax::operator::{AssignmentOperator, BinaryOperator, UnaryOperator};
use oxc::syntax::scope::ScopeFlags;

use super::{LowerError, Lowered};

mod accessors;
mod ast;
mod chains;
mod class;
mod constructor;
mod elements;
mod members;
mod names;
mod plan;
mod private;
mod properties;
mod scans;
mod statements;
mod visit;

use class::*;
use elements::*;
use members::*;
use private::*;
use scans::*;

// The helpers, in `decorators/helpers.js`, are a port of esbuild's runtime
// (`internal/runtime/runtime.go` at tag v0.25.6, https://github.com/evanw/esbuild), changed only
// to avoid syntax newer than ES2015 (`__decoratorStart` used `?.` and `??`). esbuild is
// MIT-licensed; the file carries its licence.

/// The ES module the lowered code imports its helpers from (the caller serves it as a virtual
/// module under the `helpers_specifier` given to [`lower_decorators`]). It uses no syntax newer
/// than ES2015 (arrow functions, shorthand properties, computed accessor names).
pub const DECORATOR_HELPERS: &str = include_str!("decorators/helpers.js");

/// Lowers the TC39 decorators of a module; `Ok(None)` when it has none (fast path: a source
/// without an `@` byte is not parsed).
///
/// `filename` names the source in the source map (built when `sourcemap` is true). The lowered
/// code imports the helpers it uses from `helpers_specifier` (see [`DECORATOR_HELPERS`]); for a
/// script or CommonJS `source_type` it `require`s them instead, because an `import` is a syntax
/// error there.
///
/// A module that does not parse is left to rolldown, which reports the syntax error.
///
/// # Errors
/// Decorators this lowering does not support (TypeScript parameter decorators, decorators on
/// constructors, overloads, abstract or `declare` members), with their positions.
pub fn lower_decorators(
    source: &str,
    source_type: SourceType,
    filename: &str,
    helpers_specifier: &str,
    sourcemap: bool,
) -> Result<Option<Lowered>, Vec<LowerError>> {
    // Auto-accessors need lowering without a decorator too (oxc leaves `accessor` fields).
    if !(source.contains('@') || source.contains("accessor"))
        || source_type.is_typescript_definition()
    {
        return Ok(None);
    }
    let lines = LineIndex::new(source);
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, source, source_type).parse();
    // rolldown parses the module next and reports its syntax errors.
    if parsed.fatal_error || parsed.diagnostics.has_errors() {
        return Ok(None);
    }
    let mut program = parsed.program;
    let mut scan = NeedsLowering(false);
    scan.visit_program(&program);
    if !scan.0 {
        return Ok(None);
    }
    let scoping = SemanticBuilder::new()
        .build(&program)
        .semantic
        .into_scoping();
    let mut lowerer = Lowerer::new(&allocator, &scoping, &program, &lines);
    lowerer.visit_program(&mut program);
    if !lowerer.errors.is_empty() {
        let mut errors = lowerer.errors;
        errors.sort_by_key(|e| (e.line, e.column));
        errors.dedup();
        return Err(errors);
    }
    if !lowerer.changed {
        return Ok(None);
    }
    // `SourceType::ts()` is unambiguous and resolves to a script without ESM syntax, but bundlers
    // treat such modules as ESM: only an explicit script or CommonJS input gets `require`.
    let import = !(source_type.is_script() || source_type.is_commonjs());
    lowerer.insert_helpers(&mut program, helpers_specifier, import);
    let options = CodegenOptions {
        source_map_path: sourcemap.then(|| PathBuf::from(filename)),
        ..CodegenOptions::default()
    };
    let out = Codegen::new().with_options(options).build(&program);
    Ok(Some(Lowered {
        code: out.code,
        map: out.map.map(oxc_sourcemap::SourceMap::into_owned),
    }))
}

/// The esbuild helpers the lowered code calls, in the order of their names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Helper {
    DecorateElement,
    DecoratorMetadata,
    DecoratorStart,
    PrivateAdd,
    PrivateGet,
    PrivateIn,
    PrivateMethod,
    PrivateSet,
    PrivateWrapper,
    PublicField,
    RunInitializers,
    SuperGet,
    SuperSet,
    SuperWrapper,
}

impl Helper {
    fn name(self) -> &'static str {
        match self {
            Self::DecorateElement => "__decorateElement",
            Self::DecoratorMetadata => "__decoratorMetadata",
            Self::DecoratorStart => "__decoratorStart",
            Self::PrivateAdd => "__privateAdd",
            Self::PrivateGet => "__privateGet",
            Self::PrivateIn => "__privateIn",
            Self::PrivateMethod => "__privateMethod",
            Self::PrivateSet => "__privateSet",
            Self::PrivateWrapper => "__privateWrapper",
            Self::PublicField => "__publicField",
            Self::RunInitializers => "__runInitializers",
            Self::SuperGet => "__superGet",
            Self::SuperSet => "__superSet",
            Self::SuperWrapper => "__superWrapper",
        }
    }
}

/// The private names a class on the traversal stack declares: lowered, or kept (`None`).
struct ClassScope<'a> {
    privates: HashMap<&'a str, Option<PrivateLowering<'a>>>,
}

/// What `this`, `super` and `new.target` mean in the code being visited, when that code moves out
/// of its class (esbuild's `fnOnlyDataVisit`).
#[derive(Clone, Copy, Default)]
struct FnCtx<'a> {
    /// `this` becomes this identifier (static initializers and blocks moved after the class).
    this_to: Option<&'a str>,
    /// `super.x` becomes a helper call on this class (moved static code, lowered private methods).
    super_home: Option<SuperHome<'a>>,
    /// `new.target` becomes `void 0` (field initializers moved into the constructor or after the
    /// class, where it would mean something else).
    new_target_undefined: bool,
}

#[derive(Clone, Copy)]
struct SuperHome<'a> {
    class: &'a str,
    is_static: bool,
}

/// How a decorated class appears in the code.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ClassKind {
    Expr,
    Stmt,
    ExportStmt,
    ExportDefault,
}

/// The decisions about a decorated class made before its body is visited.
struct ClassPlan<'a> {
    /// The name decorators see (`ctx.name` of the class decorator).
    name: String,
    /// The class binding (declaration name, or class expression name).
    symbol: Option<SymbolId>,
    /// A member has decorators: fields and static blocks move out, private names are lowered.
    lower_members: bool,
    /// The class itself has decorators (which may replace it).
    has_class_decorators: bool,
    /// The identifier generated code uses for the class.
    class_ref: &'a str,
    /// A class statement is assigned to `class_ref` and its own binding initialized at the end.
    capture: bool,
    privates: HashMap<&'a str, PrivateLowering<'a>>,
    instance_brand: Option<&'a str>,
    static_brand: Option<&'a str>,
    /// Private names declared by the class, for generated storage names.
    declared_privates: HashSet<String>,
}

/// An element of a class being lowered, with what the analysis found.
struct Elem<'a> {
    el: ClassElement<'a>,
    kind: ElemKind,
    is_static: bool,
    private: Option<&'a str>,
    decorators: Vec<Expression<'a>>,
    decorators_ref: Option<&'a str>,
    key_ref: Option<&'a str>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ElemKind {
    Method,
    Getter,
    Setter,
    Constructor,
    Field,
    Accessor,
    StaticBlock,
    /// Erased by TypeScript (index signatures, overloads, abstract and `declare` members).
    TypeOnly,
}

/// The pieces of code lowering one class produces, in esbuild's order.
#[derive(Default)]
struct ClassOut<'a> {
    chain: Vec<Expression<'a>>,
    init_ref: Option<&'a str>,
    class_decorators_ref: Option<&'a str>,
    extends_ref: Option<&'a str>,
    private_members: Vec<Expression<'a>>,
    static_members: Vec<Expression<'a>>,
    static_private_methods: Vec<Expression<'a>>,
    instance_members: Vec<Statement<'a>>,
    instance_private_methods: Vec<Statement<'a>>,
    dec_static_non_field: Vec<Expression<'a>>,
    dec_instance_non_field: Vec<Expression<'a>>,
    dec_static_field: Vec<Expression<'a>>,
    dec_instance_field: Vec<Expression<'a>>,
    call_instance_method_extra: bool,
    call_static_method_extra: bool,
    brands_added: BrandsAdded,
    accessor_storage_count: usize,
    /// Private names of the class, with the storage names generated for auto-accessors.
    storage_names: HashSet<String>,
}

/// Whether the `WeakSet`s of a class's private methods were created.
#[derive(Default)]
struct BrandsAdded {
    instance: bool,
    r#static: bool,
}

/// The program traversal: lowers decorated classes (innermost first), rewrites private names,
/// `this` and `super` in code that moves, and collects fresh temporaries per `var` scope.
struct Lowerer<'a, 's> {
    alloc: &'a Allocator,
    b: AstBuilder<'a>,
    scoping: &'s Scoping,
    lines: &'s LineIndex<'s>,
    used_names: HashSet<String>,
    temp_count: usize,
    helpers: BTreeMap<Helper, &'a str>,
    var_scopes: Vec<Vec<&'a str>>,
    name_hints: HashMap<u32, String>,
    classes: Vec<ClassScope<'a>>,
    fn_ctx: FnCtx<'a>,
    namespace_depth: usize,
    shadowed_weak: HashSet<&'static str>,
    errors: Vec<LowerError>,
    changed: bool,
}

impl<'a, 's> Lowerer<'a, 's> {
    fn new(
        alloc: &'a Allocator,
        scoping: &'s Scoping,
        program: &Program<'a>,
        lines: &'s LineIndex<'s>,
    ) -> Self {
        let mut used_names = HashSet::new();
        NameCollector(&mut used_names).visit_program(program);
        let shadowed_weak = ["WeakMap", "WeakSet"]
            .into_iter()
            .filter(|n| scoping.symbol_names().any(|s| s == *n))
            .collect();
        Self {
            alloc,
            b: AstBuilder::new(alloc),
            scoping,
            lines,
            used_names,
            temp_count: 0,
            helpers: BTreeMap::new(),
            var_scopes: Vec::new(),
            name_hints: HashMap::new(),
            classes: Vec::new(),
            fn_ctx: FnCtx::default(),
            namespace_depth: 0,
            shadowed_weak,
            errors: Vec::new(),
            changed: false,
        }
    }

    fn error(&mut self, span: Span, message: impl Into<String>) {
        self.errors.push(self.lines.error(span.start, message));
    }
}

#[cfg(test)]
mod tests;
