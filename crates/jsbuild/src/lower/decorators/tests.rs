//! The lowering's API: fast paths, output shape, TypeScript and JSX, module kinds, source maps
//! and errors. What lowered code does is tested end to end (`tests/it/decorators.rs`).

use std::path::Path;

use oxc::allocator::Allocator;
use oxc::codegen::Codegen;
use oxc::parser::Parser;
use oxc::semantic::SemanticBuilder;
use oxc::span::SourceType;
use oxc::transformer::{
    EnvOptions, HelperLoaderMode, HelperLoaderOptions, TransformOptions, Transformer,
};

use super::{DECORATOR_HELPERS, lower_decorators};
use crate::lower::LowerError;

const HELPERS_SPECIFIER: &str = "./decorator-helpers.mjs";

fn lowered(source: &str, source_type: SourceType) -> Option<String> {
    lower_decorators(source, source_type, "input", HELPERS_SPECIFIER, false)
        .expect("lowering")
        .map(|l| l.code)
}

fn errors(source: &str, source_type: SourceType) -> Vec<LowerError> {
    lower_decorators(source, source_type, "input", HELPERS_SPECIFIER, false).expect_err("errors")
}

/// oxc's transformer (TypeScript, JSX, then syntax lowering to `target`) on `code`.
fn transform(code: &str, source_type: SourceType, target: &str) -> String {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, code, source_type).parse();
    assert!(
        parsed.diagnostics.is_empty(),
        "{:?}\n{code}",
        parsed.diagnostics
    );
    let mut program = parsed.program;
    let scoping = SemanticBuilder::new()
        .with_enum_eval(true)
        .build(&program)
        .semantic
        .into_scoping();
    let options = TransformOptions {
        env: EnvOptions::from_target(target).expect("target"),
        helper_loader: HelperLoaderOptions {
            mode: HelperLoaderMode::External,
            ..HelperLoaderOptions::default()
        },
        ..TransformOptions::default()
    };
    let ret = Transformer::new(&allocator, Path::new("input.ts"), &options)
        .build_with_scoping(scoping, &mut program);
    assert!(ret.diagnostics.is_empty(), "{:?}", ret.diagnostics);
    Codegen::new().build(&program).code
}

#[test]
fn fast_path_without_at_or_accessor() {
    assert!(lowered("class A { x = 1; m() {} }", SourceType::mjs()).is_none());
    // Not parsed: invalid syntax without `@` is still `None`.
    assert!(lowered("class {", SourceType::mjs()).is_none());
    // An auto-accessor needs lowering without a decorator (oxc leaves `accessor` fields).
    assert!(lowered("class A { accessor x = 1; }", SourceType::mjs()).is_some());
}

#[test]
fn none_without_decorators() {
    assert!(
        lowered(
            "const email = 'a@b.c'; class A { m() {} }",
            SourceType::mjs()
        )
        .is_none()
    );
    assert!(lowered("/** @type {number} */ let x = 1;", SourceType::ts()).is_none());
    assert!(lowered("@dec class A {}", SourceType::d_ts()).is_none());
    let ambient = "declare namespace N { class A { accessor x: number } } let e = '@';";
    assert!(lowered(ambient, SourceType::ts()).is_none());
}

#[test]
fn helpers_module_is_es2015() {
    assert!(!DECORATOR_HELPERS.contains("?."));
    assert!(!DECORATOR_HELPERS.contains("??"));
    assert!(DECORATOR_HELPERS.contains("Copyright (c) 2020 Evan Wallace"));
    for name in [
        "__decorateElement",
        "__decoratorStart",
        "__runInitializers",
        "__privateGet",
    ] {
        assert!(
            DECORATOR_HELPERS.contains(&format!("export var {name} ")),
            "{name}"
        );
    }
    // It must compile for an ES2015 target without changes to its syntax level.
    let es2015 = transform(DECORATOR_HELPERS, SourceType::mjs(), "es2015");
    assert!(es2015.contains("export var __decorateElement"), "{es2015}");
}

#[test]
fn imports_only_used_helpers() {
    let code = lowered("@dec class A {}", SourceType::mjs()).unwrap_or_default();
    let import = format!(
        "import {{ __decorateElement, __decoratorStart, __runInitializers }} from \"{HELPERS_SPECIFIER}\";"
    );
    assert!(code.starts_with(&import), "{code}");
}

#[test]
fn helper_names_avoid_user_bindings() {
    let source = "const __decorateElement = 1; const _init = 2; @dec class A {}";
    let code = lowered(source, SourceType::mjs()).unwrap_or_default();
    assert!(
        code.contains("__decorateElement as __decorateElement2"),
        "{code}"
    );
    assert!(code.contains("_init2 = __decoratorStart(null)"), "{code}");
}

#[test]
fn script_and_commonjs_require_the_helpers() {
    for source_type in [SourceType::cjs(), SourceType::script()] {
        let code = lowered("@dec class A {}\nmodule.exports = A;", source_type).unwrap_or_default();
        assert!(
            code.contains(&format!("= require(\"{HELPERS_SPECIFIER}\");")),
            "{code}"
        );
        assert!(!code.contains("import "), "{code}");
    }
    // Unambiguous TypeScript without ESM syntax is bundled as ESM: it imports.
    let code = lowered("@dec class A {}", SourceType::ts()).unwrap_or_default();
    assert!(code.starts_with("import {"), "{code}");
}

#[test]
fn typescript_is_kept() {
    let source = r"
interface Shape { area(): number }
enum Kind { A, B }
const dec = <T,>(value: T, ctx: ClassDecoratorContext): T => value;
@dec export abstract class Box<T> implements Shape {
  declare tag: string;
  abstract kind(): Kind;
  area(): number { return 1 as number; }
  constructor(private readonly size: number, public k: Kind = Kind.A) {}
  get value(): T | undefined { return undefined; }
}
";
    let code = lowered(source, SourceType::ts()).unwrap_or_default();
    for kept in [
        "interface Shape",
        "enum Kind",
        "class Box<T> implements Shape",
        "abstract class Box",
        "abstract kind(): Kind;",
        "area(): number",
        "1 as number",
        "T | undefined",
        "declare tag: string",
        // Parameter properties stay for TypeScript to lower (only class decorators).
        "private readonly size: number",
    ] {
        assert!(code.contains(kept), "{kept} missing in:\n{code}");
    }
    // With member decorators, fields move out of the class: `declare` fields (types only) go,
    // and parameter properties become assignments, first in the constructor (as esbuild).
    let members = source
        .replace("@dec export abstract", "export abstract")
        .replace("  area()", "  @dec area()");
    let code = lowered(&members, SourceType::ts()).unwrap_or_default();
    assert!(!code.contains("declare tag"), "{code}");
    assert!(
        code.contains(
            "constructor(size: number, k: Kind = Kind.A) {\n\t\tthis.size = size;\n\t\tthis.k = k;"
        ),
        "{code}"
    );
}

#[test]
fn jsx_is_kept() {
    let source = "const dec = () => {};\nclass View { @dec render() { return <div className=\"x\">{this.props}</div>; } }";
    let code = lowered(source, SourceType::jsx()).unwrap_or_default();
    assert!(
        code.contains("<div className=\"x\">{this.props}</div>"),
        "{code}"
    );
    let tsx = source.replace("const dec", "const dec: any");
    let code = lowered(&tsx, SourceType::tsx()).unwrap_or_default();
    assert!(code.contains("<div className=\"x\">"), "{code}");
}

#[test]
fn source_map_points_back() {
    let source =
        "const dec = () => {};\n\nclass Foo {\n  @dec\n  method() {\n    return 42;\n  }\n}\n";
    let lowered = lower_decorators(
        source,
        SourceType::mjs(),
        "src/foo.js",
        HELPERS_SPECIFIER,
        true,
    )
    .expect("lowering")
    .expect("lowered");
    let map = lowered.map.expect("map");
    assert_eq!(map.get_sources().collect::<Vec<_>>(), ["src/foo.js"]);
    assert_eq!(
        map.get_source_contents().collect::<Vec<_>>(),
        [Some(source)]
    );
    let line_of = |needle: &str| {
        let line = lowered
            .code
            .lines()
            .position(|l| l.contains(needle))
            .expect(needle);
        u32::try_from(line).expect("line")
    };
    let src_line = |dst_line: u32| {
        map.get_tokens()
            .find(|t| t.get_dst_line() == dst_line)
            .map(|t| t.get_src_line())
    };
    let src_lines = |dst_line: u32| {
        map.get_tokens()
            .filter(|t| t.get_dst_line() == dst_line)
            .map(|t| t.get_src_line())
            .collect::<Vec<_>>()
    };
    // 0-based: the method element starts at its decorator (line 3), its key `method` is on
    // line 4, `return 42;` on line 5.
    let method = src_lines(line_of("method()"));
    assert!(
        method.contains(&3) && method.contains(&4),
        "{method:?}\n{}",
        lowered.code
    );
    assert_eq!(src_line(line_of("return 42")), Some(5), "{}", lowered.code);
    let no_map = lower_decorators(source, SourceType::mjs(), "x", HELPERS_SPECIFIER, false)
        .expect("lowering")
        .expect("lowered");
    assert!(no_map.map.is_none());
}

#[test]
fn unsupported_decorators_are_errors() {
    let errs = errors("class A {\n  m(@inject x) {}\n}", SourceType::ts());
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert_eq!((errs[0].line, errs[0].column), (2, 4), "{errs:?}");
    assert!(
        errs[0].message.contains("experimental decorators"),
        "{errs:?}"
    );

    let errs = errors("class A {\n  @dec constructor() {}\n}", SourceType::mjs());
    assert_eq!((errs[0].line, errs[0].column), (2, 2), "{errs:?}");
    assert!(errs[0].message.contains("constructors"), "{errs:?}");

    // What oxc's parser already rejects is left to rolldown, which reports the syntax error;
    // nothing is lowered.
    for source in [
        "abstract class A {\n  @dec abstract x: number;\n}",
        "abstract class A {\n  @dec abstract m(): void;\n}",
        "class A {\n  @dec m(): void;\n  m() {}\n}",
        "class A {\n  @dec declare x: number;\n}",
    ] {
        match lower_decorators(source, SourceType::ts(), "input", HELPERS_SPECIFIER, false) {
            Ok(None) => {}
            Err(errs) => assert_eq!((errs[0].line, errs[0].column), (2, 2), "{errs:?}"),
            Ok(Some(l)) => panic!("lowered {source:?}:\n{}", l.code),
        }
    }
}

#[test]
fn export_forms() {
    let module = r"
const tag = (cls, ctx) => { cls.tagged = ctx.name; };
export default @tag class { static who = 'default'; }
export @tag class Named { static who = Named.name; }
@tag export class Before { static me() { return Before; } }
";
    let code = lowered(module, SourceType::mjs()).unwrap_or_default();
    assert!(code.contains("export { _default as default };"), "{code}");
    assert!(code.contains("export let Named = _Named;"), "{code}");
    assert!(code.contains("export let Before = _Before;"), "{code}");
}

#[test]
fn typescript_parameter_properties_are_lowered_with_fields() {
    let source = r"
const dec = (v: undefined, ctx: ClassFieldDecoratorContext) => (x: number) => x * 2;
class Base { constructor(public tag: string) {} }
class P extends Base {
  @dec n: number = 21;
  constructor(public a: number, private b = 'B') {
    super('base');
    console.log(this.n, this.a, this.b, this.tag);
  }
}
new P(1);
";
    let code = lowered(source, SourceType::ts()).unwrap_or_default();
    assert!(code.contains("constructor(a: number, b = \"B\")"), "{code}");
}

#[test]
fn syntax_errors_are_left_to_rolldown() {
    assert!(lowered("const a = 1;\n@dec class {\n  x = ;\n}", SourceType::mjs()).is_none());
}
