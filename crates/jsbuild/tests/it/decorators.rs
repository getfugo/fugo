//! TC39 decorators end to end: bundled by `js.Build` (lowered in its `transform` hook, then by
//! rolldown to the target) and run with node. esbuild's own runtime suite
//! (`fixtures/decorator-tests.ts`, v0.25.6) must pass at every target esbuild runs it at and
//! below.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::{Value as Json, json};
use ssg_jsbuild::{JsBuildError, JsBuildOptions, JsBuilder, MountedDirs, Source};

/// Builds `entry` (with its sibling `files`) in a fresh site and returns the bundle's path.
fn build(
    name: &str,
    files: &[(&str, &str)],
    entry: &str,
    options: &Json,
) -> Result<PathBuf, JsBuildError> {
    let root = crate::scratch(&format!("decorators-{name}"));
    for (path, text) in files {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    let assets = root.join("assets");
    let contents = std::fs::read(assets.join(entry)).unwrap();
    let media_type = if Path::new(entry).extension().is_some_and(|e| e == "ts") {
        "text/typescript"
    } else {
        "text/javascript"
    };
    let source = Source {
        path: entry,
        media_type,
        contents: &contents,
    };
    let options = JsBuildOptions::from_json(options).unwrap();
    let out = JsBuilder::new(root.clone(), root.join("public"))
        .with_tsconfig(Some(root.join("tsconfig.json")).filter(|p| p.is_file()))
        .build(
            Arc::new(MountedDirs::new().mount(&assets, "")),
            &source,
            &options,
        )?;
    let script = root.join("out.mjs");
    std::fs::write(&script, &out.code).unwrap();
    Ok(script)
}

/// What the bundle prints under node, or `None` without node.
fn run(test: &str, script: &Path) -> Option<String> {
    let node = crate::run::node(test)?;
    let out = std::process::Command::new(node)
        .arg(script)
        .output()
        .expect("node");
    assert!(
        out.status.success(),
        "{test}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Builds one entry file and returns what it prints.
fn prints(name: &str, entry: &str, source: &str, options: &Json) -> Option<String> {
    let script = build(
        name,
        &[(&format!("assets/{entry}"), source)],
        entry,
        options,
    )
    .unwrap_or_else(|e| panic!("{name}: {e}"));
    run(name, &script)
}

fn conformance(target: &str) {
    let suite = include_str!("fixtures/decorator-tests.ts");
    let Some(out) = prints(
        &format!("suite-{target}"),
        "suite.ts",
        suite,
        &json!({"format": "esm", "target": target}),
    ) else {
        return;
    };
    let failures = out.lines().filter(|l| l.starts_with('❌')).count();
    assert!(
        out.contains("All checks passed"),
        "{failures} failures ({target}):\n{out}"
    );
}

#[test]
fn esbuild_suite_esnext() {
    conformance("esnext");
}

#[test]
fn esbuild_suite_es2022() {
    conformance("es2022");
}

#[test]
fn esbuild_suite_es2020() {
    conformance("es2020");
}

#[test]
fn esbuild_suite_es2015() {
    conformance("es2015");
}

#[test]
fn every_decorator_kind() {
    let source = r"
const seen = [];
const dec = (value, ctx) => {
  seen.push(`${ctx.kind}:${String(ctx.name)}:${ctx.static ? 's' : 'i'}${ctx.private ? 'p' : ''}`);
  if (ctx.kind === 'field') return (v) => v * 10;
  if (ctx.kind === 'accessor') return { init: (v) => v + 1 };
  if (ctx.kind === 'method') return function (...a) { return 'wrapped ' + value.apply(this, a); };
};
@dec class C {
  @dec field = 1;
  @dec static sfield = 2;
  @dec #pfield = 3;
  @dec accessor acc = 4;
  @dec static accessor #sacc = 5;
  @dec method() { return 'm'; }
  @dec static smethod() { return 'sm'; }
  @dec #pmethod() { return 'pm'; }
  @dec get g() { return 'g'; }
  @dec set s(v) { this.sv = v; }
  @dec static get #sg() { return 'sg'; }
  read() { return [this.#pfield, C.#sacc, this.#pmethod(), C.#sg].join(); }
}
const c = new C();
c.s = 'set';
console.log(seen.join(' '));
console.log(c.field, C.sfield, c.acc, c.method(), C.smethod(), c.g, c.sv, c.read());
";
    for target in ["esnext", "es2015"] {
        let Some(out) = prints(
            &format!("kinds-{target}"),
            "kinds.js",
            source,
            &json!({"target": target}),
        ) else {
            return;
        };
        assert_eq!(
            out,
            "accessor:#sacc:sp method:smethod:s getter:#sg:sp accessor:acc:i method:method:i \
             method:#pmethod:ip getter:g:i setter:s:i field:sfield:s field:field:i \
             field:#pfield:ip class:C:i\n10 20 5 wrapped m wrapped sm g set 30,6,wrapped pm,sg\n",
            "{target}"
        );
    }
}

#[test]
fn exports_typescript_and_namespaces() {
    let files = [
        (
            "assets/lib.ts",
            r"
const tag = (cls: any, ctx: ClassDecoratorContext) => { cls.tagged = ctx.name; };
export default @tag class { static who = 'default'; }
export @tag class Named { static who = Named.name; }
@tag export class Before { static me() { return Before; } }
",
        ),
        (
            "assets/main.ts",
            r"
import D, { Named, Before } from './lib';
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
console.log((D as any).tagged, (Named as any).tagged, Named.who, (Before as any).tagged, Before.me() === Before);
const log = (v: any, ctx: any) => { console.log('dec', ctx.kind, ctx.name) };
namespace N {
  export @log class Foo { @log x = 1; static self = Foo }
}
console.log(N.Foo.name, new N.Foo().x, N.Foo.self === N.Foo);
",
        ),
    ];
    let script = build("exports", &files, "main.ts", &json!({"target": "es2020"})).unwrap();
    let Some(out) = run("exports", &script) else {
        return;
    };
    assert_eq!(
        out,
        "42 1 B base\ndefault Named Named Before true\ndec field x\ndec class Foo\nFoo 1 true\n"
    );
}

#[test]
fn auto_accessors_without_decorators() {
    let source = "class A { accessor x = 1; static accessor y = 2 }\nconst a = new A(); a.x += 10;\nconsole.log(a.x, A.y);\n";
    let script = build(
        "accessor",
        &[("assets/a.js", source)],
        "a.js",
        &json!({"target": "es2022"}),
    )
    .unwrap();
    let code = std::fs::read_to_string(&script).unwrap();
    assert!(!code.contains("accessor x"), "{code}");
    if let Some(out) = run("accessor", &script) {
        assert_eq!(out, "11 2\n");
    }
}

#[test]
fn unsupported_decorators_are_errors_at_their_position() {
    let source = "const inject = () => {};\nclass A {\n  m(@inject x: number) {}\n}\n";
    let err = build("param", &[("assets/a.ts", source)], "a.ts", &json!({})).unwrap_err();
    let JsBuildError::Build(diags) = err else {
        panic!("{err:?}")
    };
    let p = diags[0].position.as_ref().expect("position");
    assert_eq!(
        (p.file.file_name().unwrap().to_str(), p.line, p.column),
        (Some("a.ts"), 3, 4),
        "{diags:?}"
    );
    assert!(
        diags[0].text.contains("experimental decorators"),
        "{diags:?}"
    );
}

#[test]
fn experimental_decorators_are_typescripts() {
    // With `experimentalDecorators`, rolldown lowers TypeScript's legacy decorators: a method
    // decorator gets (prototype, key, descriptor).
    let files = [
        (
            "tsconfig.json",
            "{\n  // comments are allowed\n  \"compilerOptions\": { \"experimentalDecorators\": true }\n}\n",
        ),
        (
            "assets/a.ts",
            r"
function log(target: any, key: string, desc: PropertyDescriptor) {
  console.log(typeof target, key, typeof desc.value);
}
class A { @log m() {} }
console.log(new A() instanceof A);
",
        ),
    ];
    let script = build("legacy", &files, "a.ts", &json!({})).unwrap();
    if let Some(out) = run("legacy", &script) {
        assert_eq!(out, "object m function\ntrue\n");
    }
}
