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

use oxc::allocator::{Allocator, ArenaBox, ArenaVec, TakeIn};
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

// The helpers below are a port of esbuild's runtime (`internal/runtime/runtime.go` at tag
// v0.25.6, https://github.com/evanw/esbuild), changed only to avoid syntax newer than ES2015
// (`__decoratorStart` used `?.` and `??`). esbuild is MIT-licensed:
//
// MIT License
//
// Copyright (c) 2020 Evan Wallace
//
// Permission is hereby granted, free of charge, to any person obtaining a copy of this software
// and associated documentation files (the "Software"), to deal in the Software without
// restriction, including without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all copies or
// substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING
// BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
// NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
// DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

/// The ES module the lowered code imports its helpers from (the caller serves it as a virtual
/// module under the `helpers_specifier` given to [`lower_decorators`]). It uses no syntax newer
/// than ES2015 (arrow functions, shorthand properties, computed accessor names).
pub const DECORATOR_HELPERS: &str = r#"// Decorator helpers of js.Build, ported from esbuild v0.25.6
// (internal/runtime/runtime.go, https://github.com/evanw/esbuild).
//
// MIT License
//
// Copyright (c) 2020 Evan Wallace
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.
var __create = Object.create
var __defProp = Object.defineProperty
var __getOwnPropDesc = Object.getOwnPropertyDescriptor
var __getProtoOf = Object.getPrototypeOf
var __reflectGet = Reflect.get
var __reflectSet = Reflect.set
var __knownSymbol = (name, symbol) => (symbol = Symbol[name]) ? symbol : Symbol.for('Symbol.' + name)
var __typeError = msg => { throw TypeError(msg) }
var __defNormalProp = (obj, key, value) => key in obj
  ? __defProp(obj, key, { enumerable: true, configurable: true, writable: true, value })
  : obj[key] = value
var __name = (target, value) => __defProp(target, 'name', { value, configurable: true })
export var __decoratorStart = base => {
  var metadata = base == null ? void 0 : base[__knownSymbol('metadata')]
  return [, , , __create(metadata == null ? null : metadata)]
}
var __decoratorStrings = ['class', 'method', 'getter', 'setter', 'accessor', 'field', 'value', 'get', 'set']
var __expectFn = fn => fn !== void 0 && typeof fn !== 'function' ? __typeError('Function expected') : fn
var __decoratorContext = (kind, name, done, metadata, fns) => ({ kind: __decoratorStrings[kind], name, metadata, addInitializer: fn =>
  done._ ? __typeError('Already initialized') : fns.push(__expectFn(fn || null)) })
export var __decoratorMetadata = (array, target) => __defNormalProp(target, __knownSymbol('metadata'), array[3])
export var __runInitializers = (array, flags, self, value) => {
  for (var i = 0, fns = array[flags >> 1], n = fns && fns.length; i < n; i++) flags & 1 ? fns[i].call(self) : value = fns[i].call(self, value)
  return value
}
export var __decorateElement = (array, flags, name, decorators, target, extra) => {
  var fn, it, done, ctx, access, k = flags & 7, s = !!(flags & 8), p = !!(flags & 16)
  var j = k > 3 ? array.length + 1 : k ? s ? 1 : 2 : 0, key = __decoratorStrings[k + 5]
  var initializers = k > 3 && (array[j - 1] = []), extraInitializers = array[j] || (array[j] = [])
  var desc = k && (
    !p && !s && (target = target.prototype),
    k < 5 && (k > 3 || !p) &&
      __getOwnPropDesc(k < 4 ? target : { get [name]() { return __privateGet(this, extra) }, set [name](x) { return __privateSet(this, extra, x) } }, name)
  )
  k ? p && k < 4 && __name(extra, (k > 2 ? 'set ' : k > 1 ? 'get ' : '') + name) : __name(target, name)
  for (var i = decorators.length - 1; i >= 0; i--) {
    ctx = __decoratorContext(k, name, done = {}, array[3], extraInitializers)
    if (k) {
      ctx.static = s, ctx.private = p, access = ctx.access = { has: p ? x => __privateIn(target, x) : x => name in x }
      if (k ^ 3) access.get = p ? x => (k ^ 1 ? __privateGet : __privateMethod)(x, target, k ^ 4 ? extra : desc.get) : x => x[name]
      if (k > 2) access.set = p ? (x, y) => __privateSet(x, target, y, k ^ 4 ? extra : desc.set) : (x, y) => x[name] = y
    }
    it = (0, decorators[i])(k ? k < 4 ? p ? extra : desc[key] : k > 4 ? void 0 : { get: desc.get, set: desc.set } : target, ctx), done._ = 1
    if (k ^ 4 || it === void 0) __expectFn(it) && (k > 4 ? initializers.unshift(it) : k ? p ? extra = it : desc[key] = it : target = it)
    else if (typeof it !== 'object' || it === null) __typeError('Object expected')
    else __expectFn(fn = it.get) && (desc.get = fn), __expectFn(fn = it.set) && (desc.set = fn), __expectFn(fn = it.init) && initializers.unshift(fn)
  }
  return k || __decoratorMetadata(array, target),
    desc && __defProp(target, name, desc),
    p ? k ^ 4 ? extra : desc : target
}
export var __publicField = (obj, key, value) => __defNormalProp(obj, typeof key !== 'symbol' ? key + '' : key, value)
var __accessCheck = (obj, member, msg) => member.has(obj) || __typeError('Cannot ' + msg)
export var __privateIn = (member, obj) => Object(obj) !== obj ? __typeError('Cannot use the "in" operator on this value') : member.has(obj)
export var __privateGet = (obj, member, getter) => (__accessCheck(obj, member, 'read from private field'), getter ? getter.call(obj) : member.get(obj))
export var __privateAdd = (obj, member, value) => member.has(obj) ? __typeError('Cannot add the same private member more than once') : member instanceof WeakSet ? member.add(obj) : member.set(obj, value)
export var __privateSet = (obj, member, value, setter) => (__accessCheck(obj, member, 'write to private field'), setter ? setter.call(obj, value) : member.set(obj, value), value)
export var __privateMethod = (obj, member, method) => (__accessCheck(obj, member, 'access private method'), method)
export var __privateWrapper = (obj, member, setter, getter) => ({
  set _(value) { __privateSet(obj, member, value, setter) },
  get _() { return __privateGet(obj, member, getter) },
})
export var __superGet = (cls, obj, key) => __reflectGet(__getProtoOf(cls), key, obj)
export var __superSet = (cls, obj, key, val) => (__reflectSet(__getProtoOf(cls), key, val, obj), val)
export var __superWrapper = (cls, obj, key) => ({
  get _() { return __superGet(cls, obj, key) },
  set _(val) { __superSet(cls, obj, key, val) },
})
"#;

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

/// Byte offsets of line starts, for error positions. Line terminators are those of JavaScript.
struct LineIndex<'s> {
    source: &'s str,
    starts: Vec<u32>,
}

impl<'s> LineIndex<'s> {
    fn new(source: &'s str) -> Self {
        let mut starts = vec![0];
        let mut chars = source.char_indices().peekable();
        while let Some((i, c)) = chars.next() {
            let next = match c {
                '\r' if chars.peek().is_some_and(|&(_, n)| n == '\n') => continue,
                '\n' | '\r' | '\u{2028}' | '\u{2029}' => i + c.len_utf8(),
                _ => continue,
            };
            starts.push(u32::try_from(next).unwrap_or(u32::MAX));
        }
        Self { source, starts }
    }

    fn error(&self, offset: u32, message: impl Into<String>) -> LowerError {
        let offset = offset.min(u32::try_from(self.source.len()).unwrap_or(u32::MAX));
        let line = self.starts.partition_point(|&s| s <= offset).max(1);
        LowerError {
            message: message.into(),
            line: u32::try_from(line).unwrap_or(u32::MAX),
            column: offset - self.starts[line - 1],
        }
    }
}

/// Whether a program has a decorator or an auto-accessor, the only things this module changes.
struct NeedsLowering(bool);

impl<'a> Visit<'a> for NeedsLowering {
    fn visit_decorator(&mut self, _: &Decorator<'a>) {
        self.0 = true;
    }

    fn visit_accessor_property(&mut self, it: &AccessorProperty<'a>) {
        self.0 = true;
        walk::walk_accessor_property(self, it);
    }
}

/// Every identifier name of a program, which fresh names must avoid.
struct NameCollector<'n>(&'n mut HashSet<String>);

impl<'a> Visit<'a> for NameCollector<'_> {
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        self.0.insert(it.name.as_str().to_owned());
    }

    fn visit_binding_identifier(&mut self, it: &BindingIdentifier<'a>) {
        self.0.insert(it.name.as_str().to_owned());
    }
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

/// How a lowered private name is stored (esbuild's `privateGetters`/`privateSetters` symbols).
#[derive(Clone, Copy, Debug)]
enum PrivateLowering<'a> {
    /// A field: its values live in this `WeakMap`.
    Field { map: &'a str },
    /// A method: instances are in the `brand` `WeakSet`, the function in `func`.
    Method { brand: &'a str, func: &'a str },
    /// A getter and/or setter (or an auto-accessor): instances are in `brand`.
    Accessor {
        brand: &'a str,
        get: Option<&'a str>,
        set: Option<&'a str>,
    },
}

impl<'a> PrivateLowering<'a> {
    /// The `WeakMap`/`WeakSet` of the instances that have the member.
    fn member(self) -> &'a str {
        match self {
            Self::Field { map } => map,
            Self::Method { brand, .. } | Self::Accessor { brand, .. } => brand,
        }
    }

    fn setter(self) -> Option<&'a str> {
        match self {
            Self::Accessor { set, .. } => set,
            _ => None,
        }
    }

    fn getter(self) -> Option<&'a str> {
        match self {
            Self::Accessor { get, .. } => get,
            _ => None,
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

    // ── names ──

    fn arena_str(&self, s: &str) -> &'a str {
        self.alloc.alloc_str(s)
    }

    /// A name not used anywhere in the program, derived from `base` (made a valid identifier).
    fn fresh(&mut self, base: &str) -> &'a str {
        let mut name: String = base
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) {
            name.insert(0, '_');
        }
        let mut candidate = name.clone();
        let mut n = 2;
        while self.used_names.contains(&candidate) || is_reserved_keyword(&candidate) {
            candidate = format!("{name}{n}");
            n += 1;
        }
        self.used_names.insert(candidate.clone());
        self.arena_str(&candidate)
    }

    /// A fresh `var` in the current scope, named like `base`.
    fn temp_named(&mut self, base: &str) -> &'a str {
        let name = self.fresh(base);
        self.declare(name);
        name
    }

    /// A fresh `var` in the current scope named `_a`, `_b`, ... (esbuild's naming).
    fn temp(&mut self) -> &'a str {
        loop {
            let mut n = self.temp_count;
            self.temp_count += 1;
            let mut suffix = String::new();
            loop {
                suffix.insert(0, char::from(b'a' + u8::try_from(n % 26).unwrap_or(0)));
                n /= 26;
                if n == 0 {
                    break;
                }
                n -= 1;
            }
            let name = format!("_{suffix}");
            if !self.used_names.contains(&name) {
                self.used_names.insert(name.clone());
                let name = self.arena_str(&name);
                self.declare(name);
                return name;
            }
        }
    }

    fn declare(&mut self, name: &'a str) {
        if let Some(scope) = self.var_scopes.last_mut() {
            scope.push(name);
        }
    }

    /// A `var` statement declaring `names`.
    fn var_decl(&self, names: &[&'a str]) -> Statement<'a> {
        let decls = ArenaVec::from_iter_in(
            names.iter().map(|n| {
                VariableDeclarator::new(
                    SPAN,
                    BindingPattern::new_binding_identifier(SPAN, *n, &self.b),
                    None,
                    None,
                    false,
                    &self.b,
                )
            }),
            &self.b,
        );
        Statement::new_variable_declaration(
            SPAN,
            VariableDeclarationKind::Var,
            decls,
            false,
            &self.b,
        )
    }

    fn with_var_scope(&mut self, f: impl FnOnce(&mut Self)) -> Vec<&'a str> {
        self.var_scopes.push(Vec::new());
        f(self);
        self.var_scopes.pop().unwrap_or_default()
    }

    // ── AST construction ──

    fn ident(&self, name: &'a str) -> Expression<'a> {
        Expression::new_identifier(SPAN, name, &self.b)
    }

    fn string(&self, s: &str) -> Expression<'a> {
        Expression::new_string_literal(SPAN, self.arena_str(s), None, &self.b)
    }

    fn number(&self, n: f64) -> Expression<'a> {
        Expression::new_numeric_literal(SPAN, n, None, NumberBase::Decimal, &self.b)
    }

    fn null(&self) -> Expression<'a> {
        Expression::new_null_literal(SPAN, &self.b)
    }

    fn void0(&self) -> Expression<'a> {
        Expression::new_void_0(SPAN, &self.b)
    }

    fn this(&self) -> Expression<'a> {
        Expression::new_this_expression(SPAN, &self.b)
    }

    fn call(&self, callee: Expression<'a>, args: Vec<Expression<'a>>) -> Expression<'a> {
        let args = ArenaVec::from_iter_in(args.into_iter().map(Argument::from), &self.b);
        Expression::new_call_expression(SPAN, callee, None, args, false, &self.b)
    }

    fn helper(&mut self, helper: Helper) -> Expression<'a> {
        let local = if let Some(local) = self.helpers.get(&helper) {
            *local
        } else {
            let local = self.fresh(helper.name());
            self.helpers.insert(helper, local);
            local
        };
        self.ident(local)
    }

    fn call_helper(&mut self, helper: Helper, args: Vec<Expression<'a>>) -> Expression<'a> {
        let callee = self.helper(helper);
        self.call(callee, args)
    }

    fn member(&self, object: Expression<'a>, name: &'a str) -> Expression<'a> {
        Expression::new_static_member_expression(
            SPAN,
            object,
            IdentifierName::new(SPAN, name, &self.b),
            false,
            &self.b,
        )
    }

    fn assign(&self, name: &'a str, value: Expression<'a>) -> Expression<'a> {
        Expression::new_assignment_expression(
            SPAN,
            AssignmentOperator::Assign,
            AssignmentTarget::new_assignment_target_identifier(SPAN, name, &self.b),
            value,
            &self.b,
        )
    }

    /// `a, b, c` (flattening nested sequences); a single expression stays as it is.
    fn seq(&self, exprs: Vec<Expression<'a>>) -> Expression<'a> {
        let mut flat = Vec::with_capacity(exprs.len());
        for e in exprs {
            match e {
                Expression::SequenceExpression(s) => flat.extend(s.unbox().expressions),
                e => flat.push(e),
            }
        }
        if flat.len() == 1 {
            return flat.pop().unwrap_or_else(|| self.void0());
        }
        Expression::new_sequence_expression(SPAN, ArenaVec::from_iter_in(flat, &self.b), &self.b)
    }

    fn array(&self, items: Vec<Expression<'a>>) -> Expression<'a> {
        let items =
            ArenaVec::from_iter_in(items.into_iter().map(ArrayExpressionElement::from), &self.b);
        Expression::new_array_expression(SPAN, items, &self.b)
    }

    fn expr_stmt(&self, e: Expression<'a>) -> Statement<'a> {
        Statement::new_expression_statement(SPAN, e, &self.b)
    }

    fn new_weak(&mut self, class: &'static str, span: Span) -> Expression<'a> {
        if self.shadowed_weak.contains(class) {
            self.error(
                span,
                format!("a binding named {class} shadows the global that lowered decorators need"),
            );
        }
        Expression::new_new_expression(
            SPAN,
            self.ident(class),
            None,
            ArenaVec::new_in(&self.b),
            &self.b,
        )
    }

    /// A function with the given parameters and statements.
    fn function(
        &self,
        params: ArenaVec<'a, FormalParameter<'a>>,
        rest: Option<ArenaBox<'a, FormalParameterRest<'a>>>,
        stmts: Vec<Statement<'a>>,
    ) -> ArenaBox<'a, Function<'a>> {
        let params = FormalParameters::boxed(
            SPAN,
            FormalParameterKind::UniqueFormalParameters,
            params,
            rest,
            &self.b,
        );
        let body = FunctionBody::boxed(
            SPAN,
            ArenaVec::new_in(&self.b),
            ArenaVec::from_iter_in(stmts, &self.b),
            &self.b,
        );
        Function::boxed(
            SPAN,
            FunctionType::FunctionExpression,
            None,
            false,
            false,
            false,
            None,
            None,
            params,
            None,
            Some(body),
            &self.b,
        )
    }

    /// `(() => { stmts })()`.
    fn arrow_iife(&self, stmts: ArenaVec<'a, Statement<'a>>) -> Expression<'a> {
        let params = FormalParameters::boxed(
            SPAN,
            FormalParameterKind::ArrowFormalParameters,
            ArenaVec::new_in(&self.b),
            None,
            &self.b,
        );
        let body = FunctionBody::boxed(SPAN, ArenaVec::new_in(&self.b), stmts, &self.b);
        let arrow = Expression::new_arrow_function_expression(
            SPAN,
            false,
            None,
            params,
            None,
            ArrowFunctionBody::FunctionBody(body),
            &self.b,
        );
        self.call(arrow, Vec::new())
    }

    /// A copy of a side-effect-free expression (`this` or an identifier), else `None`.
    fn clone_simple(&self, e: &Expression<'a>) -> Option<Expression<'a>> {
        match e {
            Expression::ThisExpression(_) => Some(self.this()),
            Expression::Identifier(id) => Some(match id.reference_id.get() {
                Some(r) => Expression::new_identifier_with_reference_id(SPAN, id.name, r, &self.b),
                None => Expression::new_identifier(SPAN, id.name, &self.b),
            }),
            _ => None,
        }
    }

    /// `e` and a reference to its value: itself twice when it is simple, else `(_a = e)`, `_a`.
    fn capture(&mut self, e: Expression<'a>) -> (Expression<'a>, Expression<'a>) {
        if let Some(copy) = self.clone_simple(&e) {
            return (e, copy);
        }
        let t = self.temp();
        (self.assign(t, e), self.ident(t))
    }

    // ── private names, `super`, `this` ──

    fn lookup_private(&self, name: &str) -> Option<PrivateLowering<'a>> {
        for scope in self.classes.iter().rev() {
            if let Some(l) = scope.privates.get(name) {
                return *l;
            }
        }
        None
    }

    fn private_read(&mut self, obj: Expression<'a>, l: PrivateLowering<'a>) -> Expression<'a> {
        match l {
            PrivateLowering::Field { map } => {
                let map = self.ident(map);
                self.call_helper(Helper::PrivateGet, vec![obj, map])
            }
            PrivateLowering::Method { brand, func } => {
                let (brand, func) = (self.ident(brand), self.ident(func));
                self.call_helper(Helper::PrivateMethod, vec![obj, brand, func])
            }
            PrivateLowering::Accessor { brand, get, .. } => {
                let mut args = vec![obj, self.ident(brand)];
                if let Some(get) = get {
                    args.push(self.ident(get));
                }
                self.call_helper(Helper::PrivateGet, args)
            }
        }
    }

    fn private_write(
        &mut self,
        obj: Expression<'a>,
        l: PrivateLowering<'a>,
        value: Expression<'a>,
    ) -> Expression<'a> {
        let mut args = vec![obj, self.ident(l.member()), value];
        if let Some(set) = l.setter() {
            args.push(self.ident(set));
        }
        self.call_helper(Helper::PrivateSet, args)
    }

    /// `__privateWrapper(obj, member, setter, getter)._`, an assignment target for any operator.
    fn private_wrapper_target(
        &mut self,
        obj: Expression<'a>,
        l: PrivateLowering<'a>,
    ) -> SimpleAssignmentTarget<'a> {
        let mut args = vec![obj, self.ident(l.member())];
        match (l.setter(), l.getter()) {
            (set, Some(get)) => {
                args.push(set.map_or_else(|| self.null(), |s| self.ident(s)));
                args.push(self.ident(get));
            }
            (Some(set), None) => args.push(self.ident(set)),
            (None, None) => {}
        }
        let wrapper = self.call_helper(Helper::PrivateWrapper, args);
        SimpleAssignmentTarget::new_static_member_expression(
            SPAN,
            wrapper,
            IdentifierName::new(SPAN, "_", &self.b),
            false,
            &self.b,
        )
    }

    /// `this` as moved code sees it.
    fn this_value(&self) -> Expression<'a> {
        self.fn_ctx
            .this_to
            .map_or_else(|| self.this(), |t| self.ident(t))
    }

    /// The home object of `super`: the class (static) or its prototype.
    fn super_home(&self, home: SuperHome<'a>) -> Expression<'a> {
        let class = self.ident(home.class);
        if home.is_static {
            class
        } else {
            self.member(class, "prototype")
        }
    }

    /// The key of `super.x` / `super[x]` as an expression, when `e` is one and `super` is lowered.
    fn take_super_key(&mut self, e: &mut Expression<'a>) -> Option<Expression<'a>> {
        self.fn_ctx.super_home?;
        match e {
            Expression::StaticMemberExpression(m) if m.object.is_super() => {
                Some(self.string(m.property.name.as_str()))
            }
            Expression::ComputedMemberExpression(m) if m.object.is_super() => {
                Some(m.expression.take_in(&self.b))
            }
            _ => None,
        }
    }

    fn super_get(&mut self, key: Expression<'a>) -> Expression<'a> {
        let home = self.fn_ctx.super_home.map(|h| self.super_home(h));
        let home = home.unwrap_or_else(|| self.void0());
        let this = self.this_value();
        self.call_helper(Helper::SuperGet, vec![home, this, key])
    }

    // ── statement lists and scopes ──

    fn finish_var_scope(&self, temps: &[&'a str], stmts: &mut ArenaVec<'a, Statement<'a>>) {
        if !temps.is_empty() {
            stmts.insert(0, self.var_decl(temps));
        }
    }

    fn visit_function_with_ctx(&mut self, func: &mut Function<'a>, ctx: FnCtx<'a>) {
        let old = std::mem::replace(&mut self.fn_ctx, ctx);
        self.check_params(&func.params);
        // Parameters see the enclosing `var` scope: a body `var` is not visible to them.
        self.visit_formal_parameters(&mut func.params);
        if let Some(body) = &mut func.body {
            let temps = self.with_var_scope(|s| s.visit_function_body(body));
            self.finish_var_scope(&temps, &mut body.statements);
        }
        self.fn_ctx = old;
    }

    fn check_params(&mut self, params: &FormalParameters<'a>) {
        let spans: Vec<Span> = params
            .items
            .iter()
            .flat_map(|p| p.decorators.iter().map(|d| d.span))
            .chain(
                params
                    .rest
                    .iter()
                    .flat_map(|r| r.decorators.iter().map(|d| d.span)),
            )
            .collect();
        for span in spans {
            self.error(
                span,
                "Parameter decorators only work when experimental decorators are enabled",
            );
        }
    }

    // ── classes ──

    /// Whether a class needs the decorator lowering (else at most its auto-accessors change).
    fn is_decorated(class: &Class<'a>) -> bool {
        !class.declare
            && (!class.decorators.is_empty()
                || class
                    .body
                    .body
                    .iter()
                    .any(|e| !element_decorators(e).is_empty()))
    }

    /// Records the name an anonymous class expression gets from its context.
    fn hint(&mut self, value: &Expression<'a>, name: &str) {
        if let Expression::ClassExpression(class) = value.get_inner_expression()
            && class.id.is_none()
        {
            self.name_hints.insert(class.span.start, name.to_owned());
        }
    }

    /// Plans the lowering of a decorated class before its body is visited.
    fn plan_class(&mut self, class: &Class<'a>, kind: ClassKind) -> ClassPlan<'a> {
        let name = match (&class.id, kind) {
            (Some(id), _) => id.name.as_str().to_owned(),
            (None, ClassKind::ExportDefault) => "default".to_owned(),
            (None, _) => self
                .name_hints
                .remove(&class.span.start)
                .unwrap_or_default(),
        };
        let symbol = class.id.as_ref().and_then(|id| id.symbol_id.get());
        let lower_members = class
            .body
            .body
            .iter()
            .any(|e| !element_decorators(e).is_empty());
        let mut declared_privates = HashSet::new();
        for e in &class.body.body {
            if let Some(PropertyKey::PrivateIdentifier(p)) = element_key(e) {
                declared_privates.insert(p.name.as_str().to_owned());
            }
        }
        let mut privates = HashMap::new();
        let (mut instance_brand, mut static_brand) = (None, None);
        if lower_members {
            let prefix = if name.is_empty() || !is_identifier_name(&name) {
                String::new()
            } else {
                format!("_{name}")
            };
            for e in &class.body.body {
                let Some(PropertyKey::PrivateIdentifier(p)) = element_key(e) else {
                    continue;
                };
                let pname = p.name.as_str();
                let is_static = element_is_static(e);
                let mut brand = |s: &mut Self| {
                    let slot = if is_static {
                        &mut static_brand
                    } else {
                        &mut instance_brand
                    };
                    *slot.get_or_insert_with(|| {
                        s.temp_named(&format!(
                            "{prefix}{}",
                            if is_static { "_static" } else { "_instances" }
                        ))
                    })
                };
                let lowering = match e {
                    ClassElement::PropertyDefinition(_) => PrivateLowering::Field {
                        map: self.temp_named(&format!("_{pname}")),
                    },
                    ClassElement::MethodDefinition(m) if m.value.body.is_none() => continue,
                    ClassElement::MethodDefinition(m) => match m.kind {
                        MethodDefinitionKind::Get | MethodDefinitionKind::Set => {
                            let brand = brand(self);
                            let is_get = m.kind == MethodDefinitionKind::Get;
                            let f = self.temp_named(&format!(
                                "{pname}{}",
                                if is_get { "_get" } else { "_set" }
                            ));
                            let (mut get, mut set) = match privates.get(pname) {
                                Some(PrivateLowering::Accessor { get, set, .. }) => (*get, *set),
                                _ => (None, None),
                            };
                            if is_get {
                                get = Some(f);
                            } else {
                                set = Some(f);
                            }
                            PrivateLowering::Accessor { brand, get, set }
                        }
                        _ => {
                            let brand = brand(self);
                            PrivateLowering::Method {
                                brand,
                                func: self.temp_named(&format!("{pname}_fn")),
                            }
                        }
                    },
                    ClassElement::AccessorProperty(_) => {
                        let brand = brand(self);
                        let get = self.temp_named(&format!("{pname}_get"));
                        let set = self.temp_named(&format!("{pname}_set"));
                        PrivateLowering::Accessor {
                            brand,
                            get: Some(get),
                            set: Some(set),
                        }
                    }
                    _ => continue,
                };
                privates.insert(self.arena_str(pname), lowering);
            }
        }
        let (class_ref, capture) = if kind == ClassKind::Expr {
            (self.temp(), true)
        } else if class.id.is_none() {
            (self.fresh("_default"), false)
        } else {
            let force = kind == ClassKind::ExportStmt && self.namespace_depth > 0;
            let mut scan = InnerRefScan {
                scoping: self.scoping,
                symbol,
                found: false,
            };
            if let Some(h) = &class.heritage {
                scan.visit_expression(&h.expression);
            }
            for e in &class.body.body {
                scan.visit_class_element(e);
            }
            let moved_this = lower_members
                && class
                    .body
                    .body
                    .iter()
                    .any(|e| uses_moved_this_or_super(e, &privates));
            if scan.found || moved_this || force {
                (self.fresh(&format!("_{name}")), true)
            } else {
                (self.arena_str(&name), false)
            }
        };
        ClassPlan {
            name,
            symbol,
            lower_members,
            has_class_decorators: !class.decorators.is_empty(),
            class_ref,
            capture,
            privates,
            instance_brand,
            static_brand,
            declared_privates,
        }
    }

    /// Visits a class with the private names it declares in scope, giving each element the
    /// `this`/`super` meaning it has once lowered (`plan` is `None` for a class kept as is).
    fn visit_class_parts(&mut self, class: &mut Class<'a>, plan: Option<&ClassPlan<'a>>) {
        // Class decorators and the heritage see the enclosing private names.
        for d in &mut class.decorators {
            self.visit_expression(&mut d.expression);
        }
        if let Some(h) = &mut class.heritage {
            self.visit_expression(&mut h.expression);
        }
        let mut privates = HashMap::new();
        for e in &class.body.body {
            if let Some(PropertyKey::PrivateIdentifier(p)) = element_key(e) {
                let name = self.arena_str(p.name.as_str());
                let lowering = plan.and_then(|p| p.privates.get(name).copied());
                privates.insert(name, lowering);
            }
        }
        self.classes.push(ClassScope { privates });
        let lower = plan.filter(|p| p.lower_members);
        for e in &mut class.body.body {
            let is_static = element_is_static(e);
            let field_ctx = match lower {
                Some(p) if is_static => FnCtx {
                    this_to: Some(p.class_ref),
                    super_home: Some(SuperHome {
                        class: p.class_ref,
                        is_static: true,
                    }),
                    new_target_undefined: true,
                },
                Some(_) => FnCtx {
                    new_target_undefined: true,
                    ..FnCtx::default()
                },
                None => FnCtx::default(),
            };
            match e {
                ClassElement::StaticBlock(block) => {
                    let old = std::mem::replace(&mut self.fn_ctx, field_ctx);
                    let temps = self.with_var_scope(|s| s.visit_statements(&mut block.body));
                    self.finish_var_scope(&temps, &mut block.body);
                    self.fn_ctx = old;
                }
                ClassElement::MethodDefinition(m) => {
                    for d in &mut m.decorators {
                        self.visit_expression(&mut d.expression);
                    }
                    if let Some(key) = m.key.as_expression_mut() {
                        self.visit_expression(key);
                    }
                    let lowered_private = matches!(&m.key, PropertyKey::PrivateIdentifier(_));
                    let ctx = match lower {
                        Some(p) if lowered_private => FnCtx {
                            super_home: Some(SuperHome {
                                class: p.class_ref,
                                is_static,
                            }),
                            ..FnCtx::default()
                        },
                        _ => FnCtx::default(),
                    };
                    self.visit_function_with_ctx(&mut m.value, ctx);
                }
                ClassElement::PropertyDefinition(p) => {
                    for d in &mut p.decorators {
                        self.visit_expression(&mut d.expression);
                    }
                    if let Some(key) = p.key.as_expression_mut() {
                        self.visit_expression(key);
                    }
                    let name = static_key_name(&p.key, p.computed);
                    if let Some(value) = &mut p.value {
                        if let Some(name) = name {
                            self.hint(value, &name);
                        }
                        let old = std::mem::replace(&mut self.fn_ctx, field_ctx);
                        self.visit_expression(value);
                        self.fn_ctx = old;
                    }
                }
                ClassElement::AccessorProperty(p) => {
                    for d in &mut p.decorators {
                        self.visit_expression(&mut d.expression);
                    }
                    if let Some(key) = p.key.as_expression_mut() {
                        self.visit_expression(key);
                    }
                    let name = static_key_name(&p.key, p.computed);
                    if let Some(value) = &mut p.value {
                        if let Some(name) = name {
                            self.hint(value, &name);
                        }
                        let old = std::mem::replace(&mut self.fn_ctx, field_ctx);
                        self.visit_expression(value);
                        self.fn_ctx = old;
                    }
                }
                ClassElement::TSIndexSignature(_) => {}
            }
        }
        self.classes.pop();
    }

    /// Reports decorators this lowering does not support on the members of a class.
    fn check_member_decorators(&mut self, class: &Class<'a>) {
        let mut bad = Vec::new();
        for e in &class.body.body {
            let decorators = element_decorators(e);
            let Some(first) = decorators.first() else {
                continue;
            };
            let message = match e {
                ClassElement::MethodDefinition(m)
                    if m.kind == MethodDefinitionKind::Constructor =>
                {
                    "Decorators are not allowed on class constructors"
                }
                e if element_kind(e) == ElemKind::TypeOnly => "Decorators are not valid here",
                _ => continue,
            };
            bad.push((first.span, message));
        }
        for (span, message) in bad {
            self.error(span, message);
        }
    }

    /// Lowers a decorated class statement (after its body was visited) into statements.
    fn lower_class_statement(
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

    fn let_decl(
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

    fn let_stmt(
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

    fn export_default_name(&self, local: &'a str) -> Statement<'a> {
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
    fn lower_class_expression(&mut self, expr: &mut Expression<'a>) {
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

    fn rename_refs(&self, e: &mut Expression<'a>, symbol: SymbolId, to: &'a str) {
        let mut r = Renamer {
            scoping: self.scoping,
            symbol,
            to,
        };
        r.visit_expression(e);
    }
}

/// Whether the class body uses `this` or `super` in code that moves out of the class
/// (static initializers and blocks) or `super` in a lowered private method.
fn uses_moved_this_or_super<'a>(
    e: &ClassElement<'a>,
    privates: &HashMap<&'a str, PrivateLowering<'a>>,
) -> bool {
    let mut scan = ThisSuperScan::default();
    match e {
        ClassElement::StaticBlock(b) => {
            for s in &b.body {
                scan.visit_statement(s);
            }
            scan.this || scan.sup
        }
        ClassElement::PropertyDefinition(p) if p.r#static => {
            if let Some(v) = &p.value {
                scan.visit_expression(v);
            }
            scan.this || scan.sup
        }
        ClassElement::AccessorProperty(p) if p.r#static => {
            if let Some(v) = &p.value {
                scan.visit_expression(v);
            }
            scan.this || scan.sup
        }
        ClassElement::MethodDefinition(m) => {
            let PropertyKey::PrivateIdentifier(p) = &m.key else {
                return false;
            };
            if !privates.contains_key(p.name.as_str()) {
                return false;
            }
            if let Some(body) = &m.value.body {
                for s in &body.statements {
                    scan.visit_statement(s);
                }
            }
            scan.sup
        }
        _ => false,
    }
}

/// Finds `this` and `super` that belong to the enclosing code (not to nested functions).
#[derive(Default)]
struct ThisSuperScan {
    this: bool,
    sup: bool,
}

impl<'a> Visit<'a> for ThisSuperScan {
    fn visit_this_expression(&mut self, _: &ThisExpression) {
        self.this = true;
    }

    fn visit_super(&mut self, _: &Super) {
        self.sup = true;
    }

    fn visit_function(&mut self, _: &Function<'a>, _: ScopeFlags) {}

    fn visit_class(&mut self, it: &Class<'a>) {
        for d in &it.decorators {
            self.visit_decorator(d);
        }
        if let Some(h) = &it.heritage {
            self.visit_expression(&h.expression);
        }
        for e in &it.body.body {
            if let Some(key) = element_key(e).and_then(PropertyKey::as_expression) {
                self.visit_expression(key);
            }
            for d in element_decorators(e) {
                self.visit_decorator(d);
            }
        }
    }
}

/// Finds a reference to a class binding.
struct InnerRefScan<'s> {
    scoping: &'s Scoping,
    symbol: Option<SymbolId>,
    found: bool,
}

impl<'a> Visit<'a> for InnerRefScan<'_> {
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        if let (Some(symbol), Some(r)) = (self.symbol, it.reference_id.get())
            && self.scoping.get_reference(r).symbol_id() == Some(symbol)
        {
            self.found = true;
        }
    }
}

/// Finds a reference named `name` (any binding).
struct NameScan<'n> {
    name: &'n str,
    found: bool,
}

impl<'a> Visit<'a> for NameScan<'_> {
    fn visit_identifier_reference(&mut self, it: &IdentifierReference<'a>) {
        if it.name.as_str() == self.name {
            self.found = true;
        }
    }
}

/// Renames the references to a class binding.
struct Renamer<'s, 'a> {
    scoping: &'s Scoping,
    symbol: SymbolId,
    to: &'a str,
}

impl<'a> VisitMut<'a> for Renamer<'_, 'a> {
    fn visit_identifier_reference(&mut self, it: &mut IdentifierReference<'a>) {
        if let Some(r) = it.reference_id.get()
            && self.scoping.get_reference(r).symbol_id() == Some(self.symbol)
        {
            it.name = self.to.into();
        }
    }
}

/// Counts `super(...)` calls that belong to a constructor (arrow functions included).
#[derive(Default)]
struct SuperCalls {
    count: usize,
}

impl<'a> Visit<'a> for SuperCalls {
    fn visit_call_expression(&mut self, it: &CallExpression<'a>) {
        if it.callee.is_super() {
            self.count += 1;
        }
        walk::walk_call_expression(self, it);
    }

    fn visit_function(&mut self, _: &Function<'a>, _: ScopeFlags) {}

    fn visit_class(&mut self, it: &Class<'a>) {
        if let Some(h) = &it.heritage {
            self.visit_expression(&h.expression);
        }
        for e in &it.body.body {
            if let Some(key) = element_key(e).and_then(PropertyKey::as_expression) {
                self.visit_expression(key);
            }
        }
    }
}

/// Replaces `super(...)` calls of a constructor with calls of `to` (esbuild's `__super` shim).
struct SuperCallRenamer<'a> {
    to: &'a str,
    b: &'a Allocator,
}

impl<'a> VisitMut<'a> for SuperCallRenamer<'a> {
    fn visit_call_expression(&mut self, it: &mut CallExpression<'a>) {
        if it.callee.is_super() {
            it.callee = Expression::new_identifier(SPAN, self.to, &AstBuilder::new(self.b));
        }
        walk_mut::walk_call_expression(self, it);
    }

    fn visit_function(&mut self, _: &mut Function<'a>, _: ScopeFlags) {}

    fn visit_class(&mut self, it: &mut Class<'a>) {
        if let Some(h) = &mut it.heritage {
            self.visit_expression(&mut h.expression);
        }
        for e in &mut it.body.body {
            if let Some(key) = element_key_mut(e).and_then(PropertyKey::as_expression_mut) {
                self.visit_expression(key);
            }
        }
    }
}

fn element_decorators<'b, 'a>(e: &'b ClassElement<'a>) -> &'b [Decorator<'a>] {
    match e {
        ClassElement::MethodDefinition(m) => &m.decorators,
        ClassElement::PropertyDefinition(p) => &p.decorators,
        ClassElement::AccessorProperty(p) => &p.decorators,
        ClassElement::StaticBlock(_) | ClassElement::TSIndexSignature(_) => &[],
    }
}

fn element_decorators_mut<'b, 'a>(
    e: &'b mut ClassElement<'a>,
) -> Option<&'b mut ArenaVec<'a, Decorator<'a>>> {
    match e {
        ClassElement::MethodDefinition(m) => Some(&mut m.decorators),
        ClassElement::PropertyDefinition(p) => Some(&mut p.decorators),
        ClassElement::AccessorProperty(p) => Some(&mut p.decorators),
        ClassElement::StaticBlock(_) | ClassElement::TSIndexSignature(_) => None,
    }
}

fn element_key<'b, 'a>(e: &'b ClassElement<'a>) -> Option<&'b PropertyKey<'a>> {
    match e {
        ClassElement::MethodDefinition(m) => Some(&m.key),
        ClassElement::PropertyDefinition(p) => Some(&p.key),
        ClassElement::AccessorProperty(p) => Some(&p.key),
        ClassElement::StaticBlock(_) | ClassElement::TSIndexSignature(_) => None,
    }
}

fn element_key_mut<'b, 'a>(e: &'b mut ClassElement<'a>) -> Option<&'b mut PropertyKey<'a>> {
    match e {
        ClassElement::MethodDefinition(m) => Some(&mut m.key),
        ClassElement::PropertyDefinition(p) => Some(&mut p.key),
        ClassElement::AccessorProperty(p) => Some(&mut p.key),
        ClassElement::StaticBlock(_) | ClassElement::TSIndexSignature(_) => None,
    }
}

fn element_computed_mut<'b>(e: &'b mut ClassElement<'_>) -> Option<&'b mut bool> {
    match e {
        ClassElement::MethodDefinition(m) => Some(&mut m.computed),
        ClassElement::PropertyDefinition(p) => Some(&mut p.computed),
        ClassElement::AccessorProperty(p) => Some(&mut p.computed),
        ClassElement::StaticBlock(_) | ClassElement::TSIndexSignature(_) => None,
    }
}

fn element_is_computed(e: &ClassElement<'_>) -> bool {
    match e {
        ClassElement::MethodDefinition(m) => m.computed,
        ClassElement::PropertyDefinition(p) => p.computed,
        ClassElement::AccessorProperty(p) => p.computed,
        ClassElement::StaticBlock(_) | ClassElement::TSIndexSignature(_) => false,
    }
}

fn element_is_static(e: &ClassElement<'_>) -> bool {
    match e {
        ClassElement::MethodDefinition(m) => m.r#static,
        ClassElement::PropertyDefinition(p) => p.r#static,
        ClassElement::AccessorProperty(p) => p.r#static,
        ClassElement::StaticBlock(_) => true,
        ClassElement::TSIndexSignature(s) => s.r#static,
    }
}

fn element_kind(e: &ClassElement<'_>) -> ElemKind {
    match e {
        ClassElement::StaticBlock(_) => ElemKind::StaticBlock,
        ClassElement::MethodDefinition(m) => {
            if m.value.body.is_none()
                || m.r#type == MethodDefinitionType::TSAbstractMethodDefinition
            {
                return ElemKind::TypeOnly;
            }
            match m.kind {
                MethodDefinitionKind::Constructor => ElemKind::Constructor,
                MethodDefinitionKind::Method => ElemKind::Method,
                MethodDefinitionKind::Get => ElemKind::Getter,
                MethodDefinitionKind::Set => ElemKind::Setter,
            }
        }
        ClassElement::PropertyDefinition(p) => {
            if p.declare || p.r#type == PropertyDefinitionType::TSAbstractPropertyDefinition {
                ElemKind::TypeOnly
            } else {
                ElemKind::Field
            }
        }
        ClassElement::AccessorProperty(p) => {
            if p.r#type == AccessorPropertyType::TSAbstractAccessorProperty {
                ElemKind::TypeOnly
            } else {
                ElemKind::Accessor
            }
        }
        ClassElement::TSIndexSignature(_) => ElemKind::TypeOnly,
    }
}

fn is_abstract_element(e: &ClassElement<'_>) -> bool {
    match e {
        ClassElement::MethodDefinition(m) => {
            m.r#type == MethodDefinitionType::TSAbstractMethodDefinition
        }
        ClassElement::PropertyDefinition(p) => {
            p.r#type == PropertyDefinitionType::TSAbstractPropertyDefinition
        }
        ClassElement::AccessorProperty(p) => {
            p.r#type == AccessorPropertyType::TSAbstractAccessorProperty
        }
        ClassElement::StaticBlock(_) | ClassElement::TSIndexSignature(_) => false,
    }
}

/// The name a non-computed key gives an anonymous class or function (`x = class {}`).
fn static_key_name(key: &PropertyKey<'_>, computed: bool) -> Option<String> {
    match key {
        PropertyKey::StaticIdentifier(id) if !computed => Some(id.name.as_str().to_owned()),
        PropertyKey::PrivateIdentifier(p) => Some(format!("#{}", p.name)),
        PropertyKey::StringLiteral(s) => Some(s.value.as_str().to_owned()),
        _ => None,
    }
}

/// The name esbuild derives temporaries from (`propertyNameHint`).
fn key_hint(key: &PropertyKey<'_>) -> String {
    match key {
        PropertyKey::StaticIdentifier(id) => id.name.as_str().to_owned(),
        PropertyKey::PrivateIdentifier(p) => p.name.as_str().to_owned(),
        PropertyKey::StringLiteral(s) => s.value.as_str().to_owned(),
        PropertyKey::Identifier(id) => id.name.as_str().to_owned(),
        _ => String::new(),
    }
}

/// Whether evaluating a key has no side effects (esbuild: strings, numbers, private names).
fn key_is_pure(key: &PropertyKey<'_>, computed: bool) -> bool {
    match key {
        PropertyKey::StaticIdentifier(_) | PropertyKey::PrivateIdentifier(_) => true,
        PropertyKey::StringLiteral(_) | PropertyKey::NumericLiteral(_) => true,
        PropertyKey::BigIntLiteral(_) => !computed,
        _ => false,
    }
}

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

impl<'a> Lowerer<'a, '_> {
    /// Whether an expression is a member access this traversal rewrites (lowered private name,
    /// or `super` in moved code).
    fn is_rewritten_expr(&self, e: &Expression<'a>) -> bool {
        match e {
            Expression::PrivateFieldExpression(p) => {
                self.lookup_private(p.field.name.as_str()).is_some()
            }
            Expression::StaticMemberExpression(m) => {
                m.object.is_super() && self.fn_ctx.super_home.is_some()
            }
            Expression::ComputedMemberExpression(m) => {
                m.object.is_super() && self.fn_ctx.super_home.is_some()
            }
            _ => false,
        }
    }

    fn is_rewritten_target(&self, t: &AssignmentTarget<'a>) -> bool {
        match t {
            AssignmentTarget::PrivateFieldExpression(p) => {
                self.lookup_private(p.field.name.as_str()).is_some()
            }
            AssignmentTarget::StaticMemberExpression(m) => {
                m.object.is_super() && self.fn_ctx.super_home.is_some()
            }
            AssignmentTarget::ComputedMemberExpression(m) => {
                m.object.is_super() && self.fn_ctx.super_home.is_some()
            }
            _ => t
                .get_expression()
                .is_some_and(|e| self.is_rewritten_expr(e.get_inner_expression())),
        }
    }

    /// `obj.#x = v` → `__privateSet(obj, member, v)`; `super.x = v` → `__superSet(home, this, "x", v)`.
    fn lower_simple_assignment(&mut self, it: &mut Expression<'a>) {
        let Expression::AssignmentExpression(a) = it else {
            return;
        };
        let mut target: Expression<'a> = match a.left.take_in(&self.b) {
            AssignmentTarget::PrivateFieldExpression(p) => Expression::PrivateFieldExpression(p),
            AssignmentTarget::StaticMemberExpression(m) => Expression::StaticMemberExpression(m),
            AssignmentTarget::ComputedMemberExpression(m) => {
                Expression::ComputedMemberExpression(m)
            }
            other => match other.get_expression() {
                Some(_) => {
                    let mut other = other;
                    match other.get_expression_mut() {
                        Some(e) => e.get_inner_expression_mut().take_in(&self.b),
                        None => return,
                    }
                }
                None => return,
            },
        };
        let mut value = a.right.take_in(&self.b);
        match &mut target {
            Expression::PrivateFieldExpression(p) => {
                let Some(l) = self.lookup_private(p.field.name.as_str()) else {
                    return;
                };
                self.visit_expression(&mut p.object);
                let obj = p.object.take_in(&self.b);
                self.visit_expression(&mut value);
                *it = self.private_write(obj, l, value);
            }
            Expression::ComputedMemberExpression(m) => {
                self.visit_expression(&mut m.expression);
                let key = m.expression.take_in(&self.b);
                self.visit_expression(&mut value);
                *it = self.super_set(key, value);
            }
            Expression::StaticMemberExpression(m) => {
                let key = self.string(m.property.name.as_str());
                self.visit_expression(&mut value);
                *it = self.super_set(key, value);
            }
            _ => {}
        }
    }

    fn super_set(&mut self, key: Expression<'a>, value: Expression<'a>) -> Expression<'a> {
        let home = self.fn_ctx.super_home.map(|h| self.super_home(h));
        let home = home.unwrap_or_else(|| self.void0());
        let this = self.this_value();
        self.call_helper(Helper::SuperSet, vec![home, this, key, value])
    }

    /// Whether an optional chain accesses a lowered private name (and must be lowered whole).
    fn chain_has_lowered_private(&self, e: &ChainElement<'a>) -> bool {
        let mut cur: &Expression<'a> = match e {
            ChainElement::CallExpression(c) => &c.callee,
            ChainElement::TSNonNullExpression(n) => &n.expression,
            ChainElement::PrivateFieldExpression(p) => {
                if self.lookup_private(p.field.name.as_str()).is_some() {
                    return true;
                }
                &p.object
            }
            ChainElement::StaticMemberExpression(m) => &m.object,
            ChainElement::ComputedMemberExpression(m) => &m.object,
        };
        loop {
            cur = match cur {
                Expression::PrivateFieldExpression(p) => {
                    if self.lookup_private(p.field.name.as_str()).is_some() {
                        return true;
                    }
                    &p.object
                }
                Expression::StaticMemberExpression(m) => &m.object,
                Expression::ComputedMemberExpression(m) => &m.object,
                Expression::CallExpression(c) => &c.callee,
                Expression::TSNonNullExpression(n) => &n.expression,
                _ => return false,
            };
        }
    }

    /// Lowers an optional chain with a lowered private name to conditionals
    /// (`a?.#x` → `(_a = a) == null ? void 0 : __privateGet(_a, _x)`).
    fn lower_chain(&mut self, it: &mut Expression<'a>) {
        let Expression::ChainExpression(chain) = it.take_in(&self.b) else {
            return;
        };
        let mut cur: Expression<'a> = match chain.unbox().expression {
            ChainElement::CallExpression(c) => Expression::CallExpression(c),
            ChainElement::TSNonNullExpression(n) => Expression::TSNonNullExpression(n),
            ChainElement::PrivateFieldExpression(p) => Expression::PrivateFieldExpression(p),
            ChainElement::StaticMemberExpression(m) => Expression::StaticMemberExpression(m),
            ChainElement::ComputedMemberExpression(m) => Expression::ComputedMemberExpression(m),
        };
        let mut links = VecDeque::new();
        let mut base = loop {
            cur = match cur {
                Expression::StaticMemberExpression(m) => {
                    let m = m.unbox();
                    links.push_front(Link {
                        optional: m.optional,
                        kind: LinkKind::Static(m.property),
                    });
                    m.object
                }
                Expression::ComputedMemberExpression(m) => {
                    let m = m.unbox();
                    links.push_front(Link {
                        optional: m.optional,
                        kind: LinkKind::Computed(m.expression),
                    });
                    m.object
                }
                Expression::PrivateFieldExpression(m) => {
                    let m = m.unbox();
                    links.push_front(Link {
                        optional: m.optional,
                        kind: LinkKind::Private(m.field),
                    });
                    m.object
                }
                Expression::CallExpression(c) => {
                    let c = c.unbox();
                    links.push_front(Link {
                        optional: c.optional,
                        kind: LinkKind::Call(c.arguments),
                    });
                    c.callee
                }
                Expression::TSNonNullExpression(n) => n.unbox().expression,
                other => break other,
            };
        };
        self.visit_expression(&mut base);
        for link in &mut links {
            match &mut link.kind {
                LinkKind::Computed(e) => self.visit_expression(e),
                LinkKind::Call(args) => self.visit_arguments(args),
                LinkKind::Static(_) | LinkKind::Private(_) => {}
            }
        }
        *it = self.build_chain(base, None, links);
    }

    fn build_chain(
        &mut self,
        mut cur: Expression<'a>,
        mut this_val: Option<Expression<'a>>,
        mut links: VecDeque<Link<'a>>,
    ) -> Expression<'a> {
        while let Some(mut link) = links.pop_front() {
            if link.optional {
                link.optional = false;
                let (check, reuse) = self.capture(cur);
                links.push_front(link);
                let rest = self.build_chain(reuse, this_val, links);
                let test = Expression::new_binary_expression(
                    SPAN,
                    check,
                    BinaryOperator::Equality,
                    self.null(),
                    &self.b,
                );
                return Expression::new_conditional_expression(
                    SPAN,
                    test,
                    self.void0(),
                    rest,
                    &self.b,
                );
            }
            let next_call = links.front().and_then(|l| match l.kind {
                LinkKind::Call(_) => Some(l.optional),
                _ => None,
            });
            match link.kind {
                LinkKind::Call(args) => {
                    cur = match this_val.take() {
                        Some(this) => {
                            let callee = self.member(cur, "call");
                            let mut all = ArenaVec::with_capacity_in(args.len() + 1, &self.b);
                            all.push(Argument::from(this));
                            all.extend(args);
                            Expression::new_call_expression(SPAN, callee, None, all, false, &self.b)
                        }
                        None => {
                            Expression::new_call_expression(SPAN, cur, None, args, false, &self.b)
                        }
                    };
                }
                kind => {
                    let lowered = match &kind {
                        LinkKind::Private(p) => self.lookup_private(p.name.as_str()),
                        _ => None,
                    };
                    let needs_this =
                        next_call.is_some_and(|optional| optional || lowered.is_some());
                    let obj = if needs_this {
                        let (obj, this) = self.capture(cur);
                        this_val = Some(this);
                        obj
                    } else {
                        this_val = None;
                        cur
                    };
                    cur = match kind {
                        LinkKind::Static(name) => Expression::new_static_member_expression(
                            SPAN, obj, name, false, &self.b,
                        ),
                        LinkKind::Computed(key) => Expression::new_computed_member_expression(
                            SPAN, obj, key, false, &self.b,
                        ),
                        LinkKind::Private(field) => match lowered {
                            Some(l) => self.private_read(obj, l),
                            None => Expression::new_private_field_expression(
                                SPAN, obj, field, false, &self.b,
                            ),
                        },
                        LinkKind::Call(_) => obj,
                    };
                }
            }
        }
        cur
    }

    /// Rewrites the auto-accessors of a class without decorators to private storage with a
    /// getter and setter (`accessor x = 1` → `#x = 1; get x() {…} set x(_) {…}`).
    fn rewrite_plain_accessors(&mut self, class: &mut Class<'a>) {
        let mut declared: HashSet<String> = class
            .body
            .body
            .iter()
            .filter_map(|e| match element_key(e) {
                Some(PropertyKey::PrivateIdentifier(p)) => Some(p.name.as_str().to_owned()),
                _ => None,
            })
            .collect();
        let old = class.body.body.take_in(&self.b);
        let mut out = ArenaVec::with_capacity_in(old.len() + 2, &self.b);
        let mut count = 0;
        for e in old {
            let ClassElement::AccessorProperty(mut a) = e else {
                out.push(e);
                continue;
            };
            if a.r#type == AccessorPropertyType::TSAbstractAccessorProperty {
                out.push(ClassElement::AccessorProperty(a));
                continue;
            }
            let key_value = if a.computed && !key_is_pure(&a.key, true) {
                let t = self.temp();
                let key = std::mem::replace(&mut a.key, PropertyKey::from(self.ident(t)));
                a.key = PropertyKey::from(self.assign(t, key.into_expression()));
                Some(self.ident(t))
            } else {
                None
            };
            let storage = unique_storage_name(&a.key, &mut declared, &mut count);
            let storage = self.arena_str(&storage);
            let a = a.unbox();
            let setter_key = match key_value {
                Some(k) => PropertyKey::from(k),
                None => self.clone_key(&a.key),
            };
            out.push(ClassElement::new_property_definition(
                a.span,
                PropertyDefinitionType::PropertyDefinition,
                ArenaVec::new_in(&self.b),
                PropertyKey::new_private_identifier(SPAN, storage, &self.b),
                a.type_annotation,
                a.value,
                false,
                a.r#static,
                false,
                false,
                false,
                a.definite,
                false,
                None,
                &self.b,
            ));
            let get = self.native_storage_get(storage);
            let set_value = self.ident("_");
            let set = self.native_storage_set(storage, set_value);
            out.push(self.getter(a.key, a.computed, a.r#static, get, a.span));
            out.push(self.setter(setter_key, a.computed, a.r#static, set, a.span));
        }
        class.body.body = out;
    }

    fn native_storage_get(&self, storage: &'a str) -> Expression<'a> {
        Expression::new_private_field_expression(
            SPAN,
            self.this(),
            PrivateIdentifier::new(SPAN, storage, &self.b),
            false,
            &self.b,
        )
    }

    fn native_storage_set(&self, storage: &'a str, value: Expression<'a>) -> Expression<'a> {
        let target = Expression::new_private_field_expression(
            SPAN,
            self.this(),
            PrivateIdentifier::new(SPAN, storage, &self.b),
            false,
            &self.b,
        );
        let target = AssignmentTarget::from(target.into_member_expression());
        Expression::new_assignment_expression(
            SPAN,
            AssignmentOperator::Assign,
            target,
            value,
            &self.b,
        )
    }

    fn getter(
        &self,
        key: PropertyKey<'a>,
        computed: bool,
        is_static: bool,
        value: Expression<'a>,
        span: Span,
    ) -> ClassElement<'a> {
        let ret = Statement::new_return_statement(SPAN, Some(value), &self.b);
        let func = self.function(ArenaVec::new_in(&self.b), None, vec![ret]);
        ClassElement::new_method_definition(
            span,
            MethodDefinitionType::MethodDefinition,
            ArenaVec::new_in(&self.b),
            key,
            func,
            MethodDefinitionKind::Get,
            computed,
            is_static,
            false,
            false,
            None,
            &self.b,
        )
    }

    fn setter(
        &self,
        key: PropertyKey<'a>,
        computed: bool,
        is_static: bool,
        body: Expression<'a>,
        span: Span,
    ) -> ClassElement<'a> {
        let func = self.setter_function(body);
        ClassElement::new_method_definition(
            span,
            MethodDefinitionType::MethodDefinition,
            ArenaVec::new_in(&self.b),
            key,
            func,
            MethodDefinitionKind::Set,
            computed,
            is_static,
            false,
            false,
            None,
            &self.b,
        )
    }

    /// `function (_) { body; }`.
    fn setter_function(&self, body: Expression<'a>) -> ArenaBox<'a, Function<'a>> {
        let param = FormalParameter::new_plain(
            SPAN,
            BindingPattern::new_binding_identifier(SPAN, "_", &self.b),
            &self.b,
        );
        self.function(
            ArenaVec::from_iter_in([param], &self.b),
            None,
            vec![self.expr_stmt(body)],
        )
    }

    /// A copy of a key without side effects (a literal, a name, or a temporary).
    fn clone_key(&self, key: &PropertyKey<'a>) -> PropertyKey<'a> {
        match key {
            PropertyKey::StaticIdentifier(id) => {
                PropertyKey::new_static_identifier(SPAN, id.name, &self.b)
            }
            PropertyKey::PrivateIdentifier(p) => {
                PropertyKey::new_private_identifier(SPAN, p.name, &self.b)
            }
            PropertyKey::StringLiteral(s) => PropertyKey::from(self.string(s.value.as_str())),
            PropertyKey::NumericLiteral(n) => PropertyKey::from(self.number(n.value)),
            PropertyKey::BigIntLiteral(n) => PropertyKey::from(Expression::new_big_int_literal(
                SPAN, n.value, n.raw, n.base, &self.b,
            )),
            PropertyKey::Identifier(id) => PropertyKey::from(
                self.clone_simple(&Expression::Identifier(id.clone_in(self.alloc)))
                    .unwrap_or_else(|| self.void0()),
            ),
            _ => PropertyKey::from(self.void0()),
        }
    }

    /// The value of a key as an expression (`foo` → `"foo"`), for helpers.
    fn key_value(&self, key: &PropertyKey<'a>, key_ref: Option<&'a str>) -> Expression<'a> {
        if let Some(r) = key_ref {
            return self.ident(r);
        }
        match key {
            PropertyKey::StaticIdentifier(id) => self.string(id.name.as_str()),
            PropertyKey::PrivateIdentifier(p) => self.string(&format!("#{}", p.name)),
            PropertyKey::StringLiteral(s) => self.string(s.value.as_str()),
            PropertyKey::NumericLiteral(n) => self.number(n.value),
            PropertyKey::BigIntLiteral(n) => self.string(n.value.as_str()),
            PropertyKey::Identifier(id) => self
                .clone_simple(&Expression::Identifier(id.clone_in(self.alloc)))
                .unwrap_or_else(|| self.void0()),
            _ => self.void0(),
        }
    }
}

/// A link of an optional chain being lowered.
struct Link<'a> {
    optional: bool,
    kind: LinkKind<'a>,
}

enum LinkKind<'a> {
    Static(IdentifierName<'a>),
    Computed(Expression<'a>),
    Private(PrivateIdentifier<'a>),
    Call(ArenaVec<'a, Argument<'a>>),
}

/// The private name an auto-accessor stores its value under (esbuild's naming: `#x`, `#_x` for
/// `accessor #x`, `#a`... for computed keys), unique in its class.
fn unique_storage_name(
    key: &PropertyKey<'_>,
    declared: &mut HashSet<String>,
    count: &mut usize,
) -> String {
    let base = match key {
        PropertyKey::StaticIdentifier(id) => id.name.as_str().to_owned(),
        PropertyKey::StringLiteral(s) if is_identifier_name(s.value.as_str()) => {
            s.value.as_str().to_owned()
        }
        PropertyKey::PrivateIdentifier(p) => format!("_{}", p.name),
        _ => {
            let mut n = *count;
            *count += 1;
            let mut s = String::new();
            loop {
                s.insert(0, char::from(b'a' + u8::try_from(n % 26).unwrap_or(0)));
                n /= 26;
                if n == 0 {
                    break;
                }
                n -= 1;
            }
            s
        }
    };
    let mut name = base.clone();
    let mut n = 2;
    while declared.contains(&name) {
        name = format!("{base}{n}");
        n += 1;
    }
    declared.insert(name.clone());
    name
}

use oxc::allocator::CloneIn;

impl<'a> Lowerer<'a, '_> {
    /// The esbuild lowering of a decorated class (`lowerClass`): returns the expressions to run
    /// before the class, the class, and the expressions to run after it.
    fn lower_class(
        &mut self,
        mut class: ArenaBox<'a, Class<'a>>,
        plan: &ClassPlan<'a>,
    ) -> LoweredClass<'a> {
        let mut out = ClassOut {
            storage_names: plan.declared_privates.clone(),
            ..ClassOut::default()
        };
        let has_class_decorators = !class.decorators.is_empty();
        out.init_ref = Some(self.temp_named("_init"));
        let lower = plan.lower_members;

        // Analysis.
        let elements = class.body.body.take_in(&self.b);
        let mut elems: Vec<Elem<'a>> = Vec::with_capacity(elements.len());
        for mut el in elements {
            let kind = element_kind(&el);
            let is_static = element_is_static(&el);
            let private = match element_key(&el) {
                Some(PropertyKey::PrivateIdentifier(p)) => Some(self.arena_str(p.name.as_str())),
                _ => None,
            };
            let decorators = element_decorators_mut(&mut el)
                .map(|d| {
                    d.take_in(&self.b)
                        .into_iter()
                        .map(|d| d.expression)
                        .collect()
                })
                .unwrap_or_default();
            elems.push(Elem {
                el,
                kind,
                is_static,
                private,
                decorators,
                decorators_ref: None,
                key_ref: None,
            });
        }

        // hoistComputedProperties: decorator lists and keys of moved fields are evaluated in
        // order, prepended to the next computed key kept in the class, else to the chain.
        let mut chain: Vec<Expression<'a>> = Vec::new();
        let mut next_computed: Option<usize> = None;
        for i in (0..elems.len()).rev() {
            let kind = elems[i].kind;
            if matches!(
                kind,
                ElemKind::StaticBlock | ElemKind::TypeOnly | ElemKind::Constructor
            ) {
                continue;
            }
            let has_decorators = !elems[i].decorators.is_empty();
            let mut decorators_expr = None;
            if has_decorators {
                let hint = element_key(&elems[i].el).map(key_hint).unwrap_or_default();
                let name = if hint.is_empty() {
                    "_dec".to_owned()
                } else {
                    format!("_{hint}_dec")
                };
                let r = self.temp_named(&name);
                let decs = std::mem::take(&mut elems[i].decorators);
                decorators_expr = Some(self.assign(r, self.array(decs)));
                elems[i].decorators_ref = Some(r);
            }
            let computed = element_is_computed(&elems[i].el);
            let pure = element_key(&elems[i].el).is_some_and(|k| key_is_pure(k, computed));
            if pure {
                if let Some(d) = decorators_expr {
                    match next_computed {
                        Some(n) => self.prepend_to_key(&mut elems[n].el, d),
                        None => chain.insert(0, d),
                    }
                }
                continue;
            }
            if let Some(d) = decorators_expr {
                self.prepend_to_key(&mut elems[i].el, d);
            }
            let rewrite_accessor = kind == ElemKind::Accessor && !has_decorators;
            let must_lower_field = matches!(kind, ElemKind::Field | ElemKind::Accessor) && lower;
            let moved = computed && (has_decorators || must_lower_field || rewrite_accessor);
            if moved {
                if !rewrite_accessor && must_lower_field {
                    let r = self.temp();
                    let key = self.take_key_expr(&mut elems[i].el);
                    let inline = self.assign(r, key);
                    if let Some(k) = element_key_mut(&mut elems[i].el) {
                        *k = PropertyKey::from(self.ident(r));
                    }
                    elems[i].key_ref = Some(r);
                    match next_computed {
                        Some(n) => self.prepend_to_key(&mut elems[n].el, inline),
                        None => chain.insert(0, inline),
                    }
                    continue;
                }
                let r = self.temp();
                let key = self.take_key_expr(&mut elems[i].el);
                let assigned = self.assign(r, key);
                if let Some(k) = element_key_mut(&mut elems[i].el) {
                    *k = PropertyKey::from(assigned);
                }
                elems[i].key_ref = Some(r);
            }
            if element_is_computed(&elems[i].el) {
                if !chain.is_empty() {
                    let r = match elems[i].key_ref {
                        Some(r) => r,
                        None => {
                            let r = self.temp();
                            let key = self.take_key_expr(&mut elems[i].el);
                            let assigned = self.assign(r, key);
                            if let Some(k) = element_key_mut(&mut elems[i].el) {
                                *k = PropertyKey::from(assigned);
                            }
                            elems[i].key_ref = Some(r);
                            r
                        }
                    };
                    let key = self.take_key_expr(&mut elems[i].el);
                    let mut all = vec![key];
                    all.append(&mut chain);
                    all.push(self.ident(r));
                    let joined = self.seq(all);
                    if let Some(k) = element_key_mut(&mut elems[i].el) {
                        *k = PropertyKey::from(joined);
                    }
                }
                next_computed = Some(i);
            }
        }
        if !chain.is_empty()
            && let Some(h) = &mut class.heritage
        {
            let r = self.temp();
            let base = h.expression.take_in(&self.b);
            let mut all = vec![self.assign(r, base)];
            all.append(&mut chain);
            all.push(self.ident(r));
            h.expression = self.seq(all);
            out.extends_ref = Some(r);
        }
        out.chain = chain;

        // Initializer slots of decorated fields and accessors, in decoration order.
        let mut counts = [0usize; 4];
        for e in &elems {
            if e.decorators_ref.is_none() {
                continue;
            }
            match (e.kind, e.is_static) {
                (ElemKind::Accessor, true) => counts[0] += 1,
                (ElemKind::Accessor, false) => counts[1] += 1,
                (ElemKind::Field, true) => counts[2] += 1,
                (ElemKind::Field, false) => counts[3] += 1,
                (_, true) => out.call_static_method_extra = true,
                (_, false) => out.call_instance_method_extra = true,
            }
        }
        let mut next_slot = [
            0,
            counts[0],
            counts[0] + counts[1],
            counts[0] + counts[1] + counts[2],
        ];

        // Class decorators are evaluated first of all, in the enclosing scope.
        let mut class_decorators = None;
        if has_class_decorators {
            let base = if plan.name.is_empty() {
                "class".to_owned()
            } else {
                plan.name.clone()
            };
            let r = self.temp_named(&format!("_{base}_decorators"));
            let decs = class
                .decorators
                .take_in(&self.b)
                .into_iter()
                .map(|d| d.expression)
                .collect();
            class_decorators = Some(self.assign(r, self.array(decs)));
            out.class_decorators_ref = Some(r);
        }

        // processProperties.
        let mut body: Vec<ClassElement<'a>> = Vec::with_capacity(elems.len());
        let mut ctor: Option<usize> = None;
        for e in elems {
            let Elem {
                mut el,
                kind,
                is_static,
                private,
                decorators_ref,
                key_ref,
                ..
            } = e;
            match kind {
                ElemKind::StaticBlock => {
                    if lower {
                        if let ClassElement::StaticBlock(block) = el {
                            self.lower_static_block(&mut out, block.unbox().body);
                        }
                    } else {
                        body.push(el);
                    }
                    continue;
                }
                ElemKind::TypeOnly => {
                    let lowered_private = private.is_some_and(|p| plan.privates.contains_key(p));
                    if !(lowered_private
                        || lower && matches!(el, ClassElement::PropertyDefinition(_)))
                    {
                        body.push(el);
                    }
                    continue;
                }
                _ => {}
            }
            let slot = decorators_ref.and_then(|_| {
                let group = match (kind, is_static) {
                    (ElemKind::Accessor, true) => 0,
                    (ElemKind::Accessor, false) => 1,
                    (ElemKind::Field, true) => 2,
                    (ElemKind::Field, false) => 3,
                    _ => return None,
                };
                let s = next_slot[group];
                next_slot[group] += 1;
                Some(s)
            });
            if let Some(decorators_ref) = decorators_ref {
                let init = out.init_ref.unwrap_or("_init");
                let mut flags = match kind {
                    ElemKind::Method => 1,
                    ElemKind::Getter => 2,
                    ElemKind::Setter => 3,
                    ElemKind::Accessor => 4,
                    _ => 5,
                };
                if is_static {
                    flags |= 8;
                }
                if private.is_some() {
                    flags |= 16;
                }
                let key =
                    element_key(&el).map_or_else(|| self.void0(), |k| self.key_value(k, key_ref));
                let mut args = vec![
                    self.ident(init),
                    self.number(f64::from(flags)),
                    key,
                    self.ident(decorators_ref),
                ];
                let lowering = private.and_then(|p| plan.privates.get(p).copied());
                let mut fn_ref = None;
                match lowering {
                    Some(l) => {
                        args.push(self.ident(l.member()));
                        fn_ref = match (kind, l) {
                            (ElemKind::Method, PrivateLowering::Method { func, .. }) => Some(func),
                            (ElemKind::Getter, PrivateLowering::Accessor { get, .. }) => get,
                            (ElemKind::Setter, PrivateLowering::Accessor { set, .. }) => set,
                            _ => None,
                        };
                        if let Some(f) = fn_ref {
                            args.push(self.ident(f));
                        }
                    }
                    None => args.push(self.ident(plan.class_ref)),
                }
                if kind == ElemKind::Accessor {
                    let ClassElement::AccessorProperty(a) = &mut el else {
                        continue;
                    };
                    let hint = key_hint(&a.key);
                    let storage = if hint.is_empty() {
                        self.temp()
                    } else {
                        self.temp_named(&format!("_{hint}"))
                    };
                    let value = a.value.take();
                    let span = a.span;
                    self.lower_field(
                        &mut out,
                        plan,
                        is_static,
                        FieldTarget::Private(storage),
                        value,
                        slot,
                        span,
                    );
                    args.push(self.ident(storage));
                }
                let mut element = self.call_helper(Helper::DecorateElement, args);
                if let Some(f) = fn_ref {
                    element = self.assign(f, element);
                } else if let (
                    ElemKind::Accessor,
                    Some(PrivateLowering::Accessor {
                        get: Some(get),
                        set: Some(set),
                        ..
                    }),
                ) = (kind, lowering)
                {
                    let t = self.temp();
                    let get_v = self.member(self.ident(t), "get");
                    let set_v = self.member(self.ident(t), "set");
                    element = self.seq(vec![
                        self.assign(t, element),
                        self.assign(get, get_v),
                        self.assign(set, set_v),
                    ]);
                }
                match (kind, is_static) {
                    (ElemKind::Field, true) => out.dec_static_field.push(element),
                    (ElemKind::Field, false) => out.dec_instance_field.push(element),
                    (_, true) => out.dec_static_non_field.push(element),
                    (_, false) => out.dec_instance_non_field.push(element),
                }
                if kind == ElemKind::Accessor {
                    if lowering.is_some() {
                        self.add_brand(&mut out, plan, is_static);
                    }
                    continue;
                }
            }
            match kind {
                ElemKind::Accessor => {
                    let ClassElement::AccessorProperty(a) = el else {
                        continue;
                    };
                    self.lower_plain_accessor(&mut out, plan, a.unbox(), key_ref, &mut body);
                }
                ElemKind::Field if lower => {
                    let ClassElement::PropertyDefinition(p) = el else {
                        continue;
                    };
                    let p = p.unbox();
                    let target = match private.and_then(|n| plan.privates.get(n).copied()) {
                        Some(PrivateLowering::Field { map }) => FieldTarget::Private(map),
                        _ => FieldTarget::Public(self.key_value(&p.key, key_ref)),
                    };
                    self.lower_field(&mut out, plan, is_static, target, p.value, slot, p.span);
                }
                ElemKind::Method | ElemKind::Getter | ElemKind::Setter
                    if private.is_some_and(|n| plan.privates.contains_key(n)) =>
                {
                    let ClassElement::MethodDefinition(m) = el else {
                        continue;
                    };
                    let m = m.unbox();
                    let Some(l) = private.and_then(|n| plan.privates.get(n).copied()) else {
                        continue;
                    };
                    let f = match (kind, l) {
                        (ElemKind::Method, PrivateLowering::Method { func, .. }) => func,
                        (ElemKind::Getter, PrivateLowering::Accessor { get: Some(g), .. }) => g,
                        (ElemKind::Setter, PrivateLowering::Accessor { set: Some(s), .. }) => s,
                        _ => continue,
                    };
                    self.add_brand(&mut out, plan, is_static);
                    let func = Expression::FunctionExpression(m.value);
                    let assigned = self.assign(f, func);
                    out.private_members.push(assigned);
                }
                ElemKind::Constructor => {
                    ctor = Some(body.len());
                    body.push(el);
                }
                _ => body.push(el),
            }
        }

        // insertInitializersIntoConstructor.
        if out.call_instance_method_extra
            || !out.instance_private_methods.is_empty()
            || !out.instance_members.is_empty()
        {
            let derived = class.heritage.is_some();
            let index = match ctor {
                Some(i) => i,
                None => {
                    let stmts = if derived {
                        let spread =
                            Argument::new_spread_element(SPAN, self.ident("arguments"), &self.b);
                        let call = Expression::new_call_expression(
                            SPAN,
                            Expression::new_super(SPAN, &self.b),
                            None,
                            ArenaVec::from_iter_in([spread], &self.b),
                            false,
                            &self.b,
                        );
                        vec![self.expr_stmt(call)]
                    } else {
                        Vec::new()
                    };
                    let func = self.function(ArenaVec::new_in(&self.b), None, stmts);
                    body.push(ClassElement::new_method_definition(
                        SPAN,
                        MethodDefinitionType::MethodDefinition,
                        ArenaVec::new_in(&self.b),
                        PropertyKey::new_static_identifier(SPAN, "constructor", &self.b),
                        func,
                        MethodDefinitionKind::Constructor,
                        false,
                        false,
                        false,
                        false,
                        None,
                        &self.b,
                    ));
                    body.len() - 1
                }
            };
            let mut stmts = Vec::new();
            if let ClassElement::MethodDefinition(m) = &mut body[index] {
                // TypeScript parameter properties first, as esbuild orders them.
                for p in &mut m.value.params.items {
                    if p.accessibility.is_some() || p.readonly || p.r#override {
                        p.accessibility = None;
                        p.readonly = false;
                        p.r#override = false;
                        if let BindingPattern::BindingIdentifier(id) = &p.pattern {
                            let name = id.name;
                            let target = self.member(self.this(), name.as_str());
                            let target = AssignmentTarget::from(target.into_member_expression());
                            let value = Expression::new_identifier(SPAN, name, &self.b);
                            stmts.push(self.expr_stmt(Expression::new_assignment_expression(
                                SPAN,
                                AssignmentOperator::Assign,
                                target,
                                value,
                                &self.b,
                            )));
                        }
                    }
                }
            }
            if out.call_instance_method_extra {
                let init = out.init_ref.unwrap_or("_init");
                let call = self.call_helper(
                    Helper::RunInitializers,
                    vec![self.ident(init), self.number(5.0), self.this()],
                );
                stmts.push(self.expr_stmt(call));
            }
            stmts.append(&mut out.instance_private_methods);
            stmts.append(&mut out.instance_members);
            if let ClassElement::MethodDefinition(m) = &mut body[index]
                && let Some(fbody) = &mut m.value.body
            {
                self.insert_after_super(fbody, stmts, derived);
            }
            let ctor_el = body.remove(index);
            body.insert(0, ctor_el);
        }
        class.body.body = ArenaVec::from_iter_in(body, &self.b);

        // finishAndGenerateCode.
        let init = out.init_ref.unwrap_or("_init");
        let decorate_class = if let Some(cdr) = out.class_decorators_ref {
            let args = vec![
                self.ident(init),
                self.number(0.0),
                self.string(&plan.name),
                self.ident(cdr),
                self.ident(plan.class_ref),
            ];
            let call = self.call_helper(Helper::DecorateElement, args);
            Some(self.assign(plan.class_ref, call))
        } else {
            Some(self.call_helper(
                Helper::DecoratorMetadata,
                vec![self.ident(init), self.ident(plan.class_ref)],
            ))
        };
        let base = match &mut class.heritage {
            Some(h) => {
                let r = match out.extends_ref {
                    Some(r) => r,
                    None => {
                        let r = self.temp();
                        let e = h.expression.take_in(&self.b);
                        h.expression = self.assign(r, e);
                        r
                    }
                };
                self.ident(r)
            }
            None => self.null(),
        };
        let mut suffix = Vec::new();
        let start = self.call_helper(Helper::DecoratorStart, vec![base]);
        suffix.push(self.assign(init, start));
        suffix.append(&mut out.private_members);
        suffix.append(&mut out.dec_static_non_field);
        suffix.append(&mut out.dec_instance_non_field);
        suffix.append(&mut out.dec_static_field);
        suffix.append(&mut out.dec_instance_field);
        suffix.append(&mut out.static_private_methods);
        suffix.extend(decorate_class);
        if out.call_static_method_extra {
            suffix.push(self.call_helper(
                Helper::RunInitializers,
                vec![
                    self.ident(init),
                    self.number(3.0),
                    self.ident(plan.class_ref),
                ],
            ));
        }
        suffix.append(&mut out.static_members);
        if out.class_decorators_ref.is_some() {
            suffix.push(self.call_helper(
                Helper::RunInitializers,
                vec![
                    self.ident(init),
                    self.number(1.0),
                    self.ident(plan.class_ref),
                ],
            ));
        }
        LoweredClass {
            class_decorators,
            chain: std::mem::take(&mut out.chain),
            class,
            suffix,
        }
    }

    /// Prepends `e` to the key of an element: `[key]` → `[(e, key)]`.
    fn prepend_to_key(&mut self, el: &mut ClassElement<'a>, e: Expression<'a>) {
        let key = self.take_key_expr(el);
        let joined = self.seq(vec![e, key]);
        if let Some(k) = element_key_mut(el) {
            *k = PropertyKey::from(joined);
        }
        if let Some(c) = element_computed_mut(el) {
            *c = true;
        }
    }

    /// Takes the key of an element as an expression (a name becomes a string).
    fn take_key_expr(&mut self, el: &mut ClassElement<'a>) -> Expression<'a> {
        let Some(key) = element_key_mut(el) else {
            return self.void0();
        };
        let dummy = PropertyKey::from(self.void0());
        match std::mem::replace(key, dummy) {
            PropertyKey::StaticIdentifier(id) => self.string(id.name.as_str()),
            PropertyKey::PrivateIdentifier(p) => {
                *key = PropertyKey::PrivateIdentifier(p);
                self.void0()
            }
            other => other.into_expression(),
        }
    }

    /// `lowerField`: a field's initialization moves into the constructor (instance) or after the
    /// class (static), with the decorators' initializers around it.
    #[expect(clippy::too_many_arguments)]
    fn lower_field(
        &mut self,
        out: &mut ClassOut<'a>,
        plan: &ClassPlan<'a>,
        is_static: bool,
        target: FieldTarget<'a>,
        value: Option<Expression<'a>>,
        slot: Option<usize>,
        span: Span,
    ) {
        let init = out.init_ref.unwrap_or("_init");
        let this_like = |s: &Self| {
            if is_static {
                s.ident(plan.class_ref)
            } else {
                s.this()
            }
        };
        let mut value = value;
        if let Some(slot) = slot {
            let flags = f64::from(u32::try_from((4 + 2 * slot) << 1).unwrap_or(u32::MAX));
            let mut args = vec![self.ident(init), self.number(flags), this_like(self)];
            args.extend(value.take());
            value = Some(self.call_helper(Helper::RunInitializers, args));
        }
        let mut member = match target {
            FieldTarget::Private(map) => {
                let weak = self.new_weak("WeakMap", span);
                let created = self.assign(map, weak);
                out.private_members.push(created);
                let mut args = vec![this_like(self), self.ident(map)];
                args.extend(value);
                self.call_helper(Helper::PrivateAdd, args)
            }
            FieldTarget::Public(key) => {
                let mut args = vec![this_like(self), key];
                args.extend(value);
                self.call_helper(Helper::PublicField, args)
            }
        };
        if let Some(slot) = slot {
            let flags = f64::from(u32::try_from(((5 + 2 * slot) << 1) | 1).unwrap_or(u32::MAX));
            let extra = self.call_helper(
                Helper::RunInitializers,
                vec![self.ident(init), self.number(flags), this_like(self)],
            );
            member = self.seq(vec![member, extra]);
        }
        if is_static {
            out.static_members.push(member);
        } else {
            out.instance_members.push(self.expr_stmt(member));
        }
    }

    /// Registers the `WeakSet` of the instances having the class's private methods.
    fn add_brand(&mut self, out: &mut ClassOut<'a>, plan: &ClassPlan<'a>, is_static: bool) {
        let (added, brand) = if is_static {
            (&mut out.brands_added.r#static, plan.static_brand)
        } else {
            (&mut out.brands_added.instance, plan.instance_brand)
        };
        let Some(brand) = brand else { return };
        if *added {
            return;
        }
        *added = true;
        let weak = self.new_weak("WeakSet", SPAN);
        let created = self.assign(brand, weak);
        out.private_members.push(created);
        let target = if is_static {
            self.ident(plan.class_ref)
        } else {
            self.this()
        };
        let add = self.call_helper(Helper::PrivateAdd, vec![target, self.ident(brand)]);
        if is_static {
            out.static_private_methods.push(add);
        } else {
            out.instance_private_methods.push(self.expr_stmt(add));
        }
    }

    /// `lowerStaticBlock`: its statements run after the class, inline when they are all
    /// expressions, else in an arrow function.
    fn lower_static_block(&mut self, out: &mut ClassOut<'a>, stmts: ArenaVec<'a, Statement<'a>>) {
        let all_exprs = stmts.iter().all(|s| {
            matches!(
                s,
                Statement::ExpressionStatement(_) | Statement::EmptyStatement(_)
            )
        });
        if all_exprs {
            for s in stmts {
                if let Statement::ExpressionStatement(e) = s {
                    out.static_members.push(e.unbox().expression);
                }
            }
        } else {
            out.static_members.push(self.arrow_iife(stmts));
        }
    }

    /// `rewriteAutoAccessorToGetSet` in a lowered class: the storage is a lowered field, and a
    /// private accessor's getter and setter are lowered private methods.
    fn lower_plain_accessor(
        &mut self,
        out: &mut ClassOut<'a>,
        plan: &ClassPlan<'a>,
        a: AccessorProperty<'a>,
        key_ref: Option<&'a str>,
        body: &mut Vec<ClassElement<'a>>,
    ) {
        let is_static = a.r#static;
        let storage = unique_storage_name(
            &a.key,
            &mut out.storage_names,
            &mut out.accessor_storage_count,
        );
        let setter_key = match key_ref {
            Some(r) => PropertyKey::from(self.ident(r)),
            None => self.clone_key(&a.key),
        };
        let (get_value, set_value) = if plan.lower_members {
            let map = self.temp_named(&format!("_{storage}"));
            self.lower_field(
                out,
                plan,
                is_static,
                FieldTarget::Private(map),
                a.value,
                None,
                a.span,
            );
            let get = self.call_helper(Helper::PrivateGet, vec![self.this(), self.ident(map)]);
            let set = self.call_helper(
                Helper::PrivateSet,
                vec![self.this(), self.ident(map), self.ident("_")],
            );
            (get, set)
        } else {
            let storage = self.arena_str(&storage);
            body.push(ClassElement::new_property_definition(
                a.span,
                PropertyDefinitionType::PropertyDefinition,
                ArenaVec::new_in(&self.b),
                PropertyKey::new_private_identifier(SPAN, storage, &self.b),
                a.type_annotation,
                a.value,
                false,
                is_static,
                false,
                false,
                false,
                a.definite,
                false,
                None,
                &self.b,
            ));
            (
                self.native_storage_get(storage),
                self.native_storage_set(storage, self.ident("_")),
            )
        };
        let lowered = match &a.key {
            PropertyKey::PrivateIdentifier(p) => plan.privates.get(p.name.as_str()).copied(),
            _ => None,
        };
        if let Some(PrivateLowering::Accessor {
            get: Some(get),
            set: Some(set),
            ..
        }) = lowered
        {
            self.add_brand(out, plan, is_static);
            let ret = Statement::new_return_statement(SPAN, Some(get_value), &self.b);
            let get_fn = self.function(ArenaVec::new_in(&self.b), None, vec![ret]);
            let get_fn = self.assign(get, Expression::FunctionExpression(get_fn));
            out.private_members.push(get_fn);
            let set_fn = self.setter_function(set_value);
            let set_fn = self.assign(set, Expression::FunctionExpression(set_fn));
            out.private_members.push(set_fn);
            return;
        }
        body.push(self.getter(a.key, a.computed, is_static, get_value, a.span));
        body.push(self.setter(setter_key, a.computed, is_static, set_value, a.span));
    }

    /// `insertStmtsAfterSuperCall`: runs `stmts` once `this` exists.
    fn insert_after_super(
        &mut self,
        body: &mut FunctionBody<'a>,
        stmts: Vec<Statement<'a>>,
        derived: bool,
    ) {
        if stmts.is_empty() {
            return;
        }
        if !derived {
            for (i, s) in stmts.into_iter().enumerate() {
                body.statements.insert(i, s);
            }
            return;
        }
        let mut count = SuperCalls::default();
        count.visit_function_body(body);
        if count.count == 0 {
            // The constructor never calls `super()`: instance fields are never initialized.
            return;
        }
        if count.count == 1 {
            for i in 0..body.statements.len() {
                let Statement::ExpressionStatement(es) = &mut body.statements[i] else {
                    continue;
                };
                let split = match &mut es.expression {
                    e if e.is_super_call_expression() => Some((Vec::new(), Vec::new())),
                    Expression::SequenceExpression(seq) => seq
                        .expressions
                        .iter()
                        .position(Expression::is_super_call_expression)
                        .map(|p| {
                            let exprs: Vec<Expression<'a>> =
                                seq.expressions.take_in(&self.b).into_iter().collect();
                            let mut before = exprs;
                            let after = before.split_off(p + 1);
                            (before, after)
                        }),
                    _ => None,
                };
                let Some((before, after)) = split else {
                    continue;
                };
                let mut new_stmts = Vec::new();
                if before.is_empty() {
                    // The statement is the call itself.
                    new_stmts.push(body.statements.remove(i));
                } else {
                    body.statements.remove(i);
                    new_stmts.push(self.expr_stmt(self.seq(before)));
                }
                new_stmts.extend(stmts);
                if !after.is_empty() {
                    new_stmts.push(self.expr_stmt(self.seq(after)));
                }
                for (j, s) in new_stmts.into_iter().enumerate() {
                    body.statements.insert(i + j, s);
                }
                return;
            }
        }
        // `var __super = (...args) => { super(...args); stmts; return this; };`
        let sup = self.fresh("__super");
        let args = self.fresh("args");
        let mut renamer = SuperCallRenamer {
            to: sup,
            b: self.alloc,
        };
        renamer.visit_function_body(body);
        let spread = Argument::new_spread_element(SPAN, self.ident(args), &self.b);
        let call = Expression::new_call_expression(
            SPAN,
            Expression::new_super(SPAN, &self.b),
            None,
            ArenaVec::from_iter_in([spread], &self.b),
            false,
            &self.b,
        );
        let mut inner = vec![self.expr_stmt(call)];
        inner.extend(stmts);
        inner.push(Statement::new_return_statement(
            SPAN,
            Some(self.this()),
            &self.b,
        ));
        let rest = FormalParameterRest::boxed(
            SPAN,
            ArenaVec::new_in(&self.b),
            BindingRestElement::new(
                SPAN,
                BindingPattern::new_binding_identifier(SPAN, args, &self.b),
                &self.b,
            ),
            None,
            &self.b,
        );
        let params = FormalParameters::boxed(
            SPAN,
            FormalParameterKind::ArrowFormalParameters,
            ArenaVec::new_in(&self.b),
            Some(rest),
            &self.b,
        );
        let arrow_body = FunctionBody::boxed(
            SPAN,
            ArenaVec::new_in(&self.b),
            ArenaVec::from_iter_in(inner, &self.b),
            &self.b,
        );
        let arrow = Expression::new_arrow_function_expression(
            SPAN,
            false,
            None,
            params,
            None,
            ArrowFunctionBody::FunctionBody(arrow_body),
            &self.b,
        );
        let decl = self.let_stmt(VariableDeclarationKind::Var, sup, arrow, SPAN);
        body.statements.insert(0, decl);
    }

    /// Adds the import (or `require`) of the helpers the lowered code uses.
    fn insert_helpers(&mut self, program: &mut Program<'a>, specifier: &str, import: bool) {
        if self.helpers.is_empty() {
            return;
        }
        let specifier = self.arena_str(specifier);
        let stmt = if import {
            let specs = ArenaVec::from_iter_in(
                self.helpers.iter().map(|(h, local)| {
                    ImportDeclarationSpecifier::new_import_specifier(
                        SPAN,
                        ModuleExportName::new_identifier_name(SPAN, h.name(), &self.b),
                        BindingIdentifier::new(SPAN, *local, &self.b),
                        ImportOrExportKind::Value,
                        &self.b,
                    )
                }),
                &self.b,
            );
            Statement::new_import_declaration(
                SPAN,
                Some(specs),
                StringLiteral::new(SPAN, specifier, None, &self.b),
                None,
                None,
                ImportOrExportKind::Value,
                &self.b,
            )
        } else {
            let props = ArenaVec::from_iter_in(
                self.helpers.iter().map(|(h, local)| {
                    BindingProperty::new(
                        SPAN,
                        PropertyKey::new_static_identifier(SPAN, h.name(), &self.b),
                        BindingPattern::new_binding_identifier(SPAN, *local, &self.b),
                        h.name() == *local,
                        false,
                        &self.b,
                    )
                }),
                &self.b,
            );
            let pattern = BindingPattern::new_object_pattern(SPAN, props, None, &self.b);
            let require = self.call(self.ident("require"), vec![self.string(specifier)]);
            let decl = VariableDeclarator::new(SPAN, pattern, None, Some(require), false, &self.b);
            Statement::new_variable_declaration(
                SPAN,
                VariableDeclarationKind::Var,
                ArenaVec::from_iter_in([decl], &self.b),
                false,
                &self.b,
            )
        };
        program.body.insert(0, stmt);
    }
}

/// A lowered class: what runs before it (the class decorators, then the decorators and keys of
/// its elements), the class, and what runs after it.
struct LoweredClass<'a> {
    class_decorators: Option<Expression<'a>>,
    chain: Vec<Expression<'a>>,
    class: ArenaBox<'a, Class<'a>>,
    suffix: Vec<Expression<'a>>,
}

/// Where a lowered field's value is stored.
enum FieldTarget<'a> {
    /// A private field, in this `WeakMap`.
    Private(&'a str),
    /// A public field, under this key.
    Public(Expression<'a>),
}

#[cfg(test)]
mod tests;
