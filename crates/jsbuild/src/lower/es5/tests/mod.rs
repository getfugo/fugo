//! The `es5` steps on their own: `check_es5` against esbuild 0.25.6's recorded errors
//! (`table.rs`), and `lower_to_es5` on ES2015 snippets of the kinds rolldown and oxc emit (the
//! result must print the same under node; that it is ES5, `lower_to_es5` checks itself). Whole
//! bundles are tested end to end (`tests/it/es5.rs`).

mod table;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use oxc::span::SourceType;

use super::{check_es5, lower_to_es5};

fn source_type(loader: &str) -> SourceType {
    match loader {
        "ts" => SourceType::ts(),
        "tsx" => SourceType::tsx(),
        "jsx" => SourceType::jsx(),
        _ => SourceType::mjs(),
    }
}

#[test]
fn check_matches_esbuild() {
    let mut failures = Vec::new();
    for case in table::CASES {
        let got: Vec<(u32, u32, String)> = check_es5(case.code, source_type(case.loader), "in.js")
            .into_iter()
            .map(|e| (e.line, e.column, e.message))
            .collect();
        let want: Vec<(u32, u32, String)> = case
            .errors
            .iter()
            .map(|&(l, c, m)| (l, c, m.to_owned()))
            .collect();
        if got != want {
            failures.push(format!(
                "{} [{}] {:?}\n   want {want:?}\n    got {got:?}",
                case.name, case.loader, case.code
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        table::CASES.len(),
        failures.join("\n")
    );
}

/// Whether `node` runs.
fn node() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// A fresh scratch directory.
fn scratch(name: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "ssg-jsbuild-es5-{}-{name}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

/// Runs `code` with node as `file` in `dir`: its output, or why it failed.
fn run(dir: &Path, file: &str, code: &str) -> Result<String, String> {
    let path = dir.join(file);
    std::fs::write(&path, code).map_err(|e| e.to_string())?;
    let out = Command::new("node")
        .arg(&path)
        .current_dir(dir)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!(
            "node {file} failed:\n{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ))
    }
}

/// (name, ES2015 input, is a module)
const CASES: &[(&str, &str, bool)] = &[
    (
        "arrows-this-arguments",
        "var o = { v: 1, f: function () { var g = () => () => [this.v, arguments.length]; return g()(); } };\n\
         console.log(JSON.stringify(o.f(7, 8)));\n\
         var h = (a, b) => { return a + b; };\n\
         console.log(h(1, 2), [1, 2].map(x => x * 2).join());\n\
         function F() { this.n = 3; var self = this; setTimeout(() => console.log(this === self, this.n), 0); }\n\
         new F();\n",
        false,
    ),
    (
        "let-const-function-level",
        "var __exportAll = (all, no_symbols) => { let target = {}; for (var name in all) Object.defineProperty(target, name, { get: all[name], enumerable: true }); return target; };\n\
         const ns = __exportAll({ a: () => 1, b: () => 2 });\n\
         console.log(Object.keys(ns).join(), ns.a, ns.b);\n",
        false,
    ),
    (
        "let-block-shadowing",
        "var x = 'outer';\n\
         function f() { var r = []; { let x = 'block'; r.push(x); } r.push(x); return r.join(); }\n\
         console.log(f());\n\
         { let y = 1; { let y = 2; console.log(y); } console.log(y); }\n\
         function g() { if (true) { const z = 'a'; console.log(z); } if (true) { const z = 'b'; console.log(z); } }\n\
         g();\n",
        false,
    ),
    (
        "let-uninitialized-in-loop",
        "for (var i = 0; i < 3; i++) { let v; if (i === 1) v = 'set'; console.log(i, v); }\n",
        false,
    ),
    (
        "let-for-head",
        "var out = []; for (let i = 0; i < 3; i++) out.push(i); for (const k in { a: 1, b: 2 }) out.push(k);\n\
         console.log(out.join());\n",
        false,
    ),
    (
        "ts-namespace-output",
        "let Outer;\n(function(_Outer) {\n\tlet Inner;\n\t(function(_Inner) {\n\t\t_Inner.deep = 'deep';\n\t})(Inner || (Inner = _Outer.Inner || (_Outer.Inner = {})));\n})(Outer || (Outer = {}));\n\
         console.log(Outer.Inner.deep);\n",
        false,
    ),
    (
        "block-function-strict",
        "(function () { 'use strict'; var r = []; { r.push(f()); function f() { return 'f'; } } switch (1) { case 0: function g() { return 'g0'; } break; default: r.push(g()); } console.log(r.join()); })();\n",
        false,
    ),
    (
        "templates",
        "var a = 1, b = { toString: function () { return 'B'; }, valueOf: function () { return 'V'; } };\n\
         console.log(`x${a}y${b}z`, `${a}`, `plain`, `\\u{1F600}`.length, `multi\nline`, `quote \" and ' and \\``);\n\
         function t(s) { return JSON.stringify([s, s.raw, [].slice.call(arguments, 1)]); }\n\
         function site() { return t`a${1}b\\n${2}`; }\n\
         console.log(site(), site === site, (function (s) { return s; })`x` === (function (s) { return s; })`x`);\n\
         function id(s) { return s; } function same() { return id`k`; } console.log(same() === same(), Object.isFrozen(same()), Object.isFrozen(same().raw));\n",
        false,
    ),
    (
        "object-literal-extensions",
        "var k = 'dyn', n = 5, __proto__ = { own: true };\n\
         var o = { n, m() { return this.n; }, [k]: 1, after: 2, get [k + 'G']() { return 'g'; }, set s(v) { this._s = v; }, [-1]: 'neg', 0.5: 'half' };\n\
         o.s = 3;\n\
         console.log(JSON.stringify(o), o.m(), o.dynG, Object.keys(o).join());\n\
         var p = { __proto__ };\n\
         console.log(Object.prototype.hasOwnProperty.call(p, '__proto__'), Object.getPrototypeOf(p) === Object.prototype);\n\
         var q = { __proto__: { inherited: 1 }, [k]: 2 };\n\
         console.log(q.inherited, q.dyn);\n",
        false,
    ),
    (
        "params-spread",
        "function f(a, b = a + 1, ...rest) { return [a, b, rest.length, arguments.length].join(); }\n\
         console.log(f(1), f(1, 2, 3, 4));\n\
         var g = (x = 10, ...ys) => x + ys.length;\n\
         console.log(g(), g(1, 2, 3));\n\
         var arr = [1, 2, 3], s = new Set([4, 5]);\n\
         console.log(Math.max(...arr), [0, ...arr, ...s, 'end'].join(), [...'ab'].join());\n\
         var obj = { base: 100, add: function () { return this.base + [].slice.call(arguments).reduce(function (x, y) { return x + y; }, 0); } };\n\
         console.log(obj.add(...arr), new Date(...[2020, 1, 2]).getFullYear());\n",
        false,
    ),
    (
        "regexp-bigint-catch",
        "console.log(/a/y.test('a'), /\\u{61}/u.test('a'), /[\\u{1F600}]/u.test('\\u{1F600}'), /a/gi.flags, /x/.source);\n\
         console.log(typeof 10n, String(0x1Fn), String(1_000n));\n\
         try { throw 1; } catch { console.log('caught'); }\n",
        false,
    ),
    (
        // In a CommonJS file, where esbuild's `__require` shim finds `require`.
        "dynamic-import",
        "import('node:path').then(function (m) { console.log(typeof m.join, typeof m.default.join); });\n",
        false,
    ),
    (
        "esm-exports",
        "let a = 1;\nconst b = () => a + 1;\nexport { a, b as c };\nconsole.log(b());\n",
        true,
    ),
    (
        "rolldown-runtime",
        "var __create = Object.create;\nvar __defProp = Object.defineProperty;\nvar __getOwnPropDesc = Object.getOwnPropertyDescriptor;\nvar __getOwnPropNames = Object.getOwnPropertyNames;\nvar __getProtoOf = Object.getPrototypeOf;\nvar __hasOwnProp = Object.prototype.hasOwnProperty;\n\
         var __name = (target, value) => __defProp(target, 'name', { value, configurable: true });\n\
         var __esm = (fn, res, err) => function () { if (err) throw err[0]; try { return (fn && (res = (0, fn[__getOwnPropNames(fn)[0]])((fn = 0))), res); } catch (e) { throw ((err = [e]), e); } };\n\
         var __esmMin = (fn, res, err) => () => { if (err) throw err[0]; try { return (fn && (res = fn((fn = 0))), res); } catch (e) { throw ((err = [e]), e); } };\n\
         var __commonJS = (cb, mod) => function () { return (mod || ((0, cb[__getOwnPropNames(cb)[0]])((mod = { exports: {} }).exports, mod), cb = null), mod.exports); };\n\
         var __commonJSMin = (cb, mod) => () => (mod || (cb((mod = { exports: {} }).exports, mod), cb = null), mod.exports);\n\
         var __copyProps = (to, from, except, desc) => { if ((from && typeof from === 'object') || typeof from === 'function') { for (var keys = __getOwnPropNames(from), i = 0, n = keys.length, key; i < n; i++) { key = keys[i]; if (!__hasOwnProp.call(to, key) && key !== except) { __defProp(to, key, { get: ((k) => from[k]).bind(null, key), enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable }); } } } return to; };\n\
         var __reExport = (target, mod, secondTarget) => (__copyProps(target, mod, 'default'), secondTarget && __copyProps(secondTarget, mod, 'default'));\n\
         var __toESM = (mod, isNodeMode, target) => ((target = mod != null ? __create(__getProtoOf(mod)) : {}), __copyProps(isNodeMode || !mod || !mod.__esModule || !__hasOwnProp.call(mod, 'default') ? __defProp(target, 'default', { value: mod, enumerable: true }) : target, mod));\n\
         var __toCommonJS = (mod) => __hasOwnProp.call(mod, 'module.exports') ? mod['module.exports'] : __copyProps(__defProp({}, '__esModule', { value: true }), mod);\n\
         var __toBinary = /* @__PURE__ */ (() => { var table = new Uint8Array(128); for (var i = 0; i < 64; i++) { table[i < 26 ? i + 65 : i < 52 ? i + 71 : i < 62 ? i - 4 : i * 4 - 205] = i; } return (base64) => { var n = base64.length, bytes = new Uint8Array((((n - (base64[n - 1] == '=') - (base64[n - 2] == '=')) * 3) / 4) | 0); for (var i = 0, j = 0; i < n; ) { var c0 = table[base64.charCodeAt(i++)], c1 = table[base64.charCodeAt(i++)]; var c2 = table[base64.charCodeAt(i++)], c3 = table[base64.charCodeAt(i++)]; bytes[j++] = (c0 << 2) | (c1 >> 4); bytes[j++] = (c1 << 4) | (c2 >> 2); bytes[j++] = (c2 << 6) | c3; } return bytes; }; })();\n\
         var __require = /* @__PURE__ */ (x => typeof require !== 'undefined' ? require : typeof Proxy !== 'undefined' ? new Proxy(x, { get: (a, b) => (typeof require !== 'undefined' ? require : a)[b] }) : x)(function(x) { if (typeof require !== 'undefined') return require.apply(this, arguments); throw Error('no require'); });\n\
         var req = __commonJSMin((exports, module) => { module.exports = { x: 1 }; });\n\
         var cjs = __toESM(req());\n\
         var fn = __name(function () {}, 'renamed');\n\
         var lazy, init = __esmMin(() => { lazy = 'lazy'; });\n\
         init();\n\
         var back = __toCommonJS({ a: 1 });\n\
         var target = {}; __reExport(target, { r: 2, default: 3 });\n\
         console.log(cjs.default.x, cjs.x, fn.name, lazy, back.a, back.__esModule, target.r, target.default, Array.prototype.join.call(__toBinary('AAEC/v8='), ','), typeof __require);\n",
        false,
    ),
    (
        // A tagged template in the IIFE's arguments needs a helper outside the wrapper.
        "helper-outside-iife-wrapper",
        "(function (s) { var f = () => s.raw[0]; console.log(f()); })((function (x) { return x; })`a\\b`);\n",
        false,
    ),
    (
        // Not a directive: oxc prints it as a template when minifying; it stays one.
        "string-statement",
        "function f() { ('use strict'); return typeof this; }\nconsole.log(f.call(1), [1].map(x => x)[0]);\n",
        false,
    ),
    (
        "using-helper-output",
        "function _usingCtx() { var e = {}, n = []; function using(r, e) { if (null != e) { var o = e[Symbol.dispose || Symbol['for']('Symbol.dispose')]; n.push({ v: e, d: o, a: r }); } return e; } return { e, u: using.bind(null, !1), d: function d() { for (var o; o = n.pop();) o.d.call(o.v); } }; }\n\
         try { var ctx = _usingCtx(); const r = ctx.u({ [Symbol.dispose || Symbol['for']('Symbol.dispose')]: function () { console.log('disposed'); } }); console.log('in'); } catch (_) { ctx.e = _; } finally { ctx.d(); }\n",
        false,
    ),
];

#[test]
fn snippets_lower_and_run() {
    if !node() {
        println!("SKIPPED snippets_lower_and_run: no node on PATH");
        return;
    }
    let mut failures = Vec::new();
    for &(name, src, module) in CASES {
        let tmp = scratch(name);
        let file = if module { "case.mjs" } else { "case.js" };
        let want = run(&tmp, file, src);
        for minify in [false, true] {
            let label = format!("{name}{}", if minify { " (minified)" } else { "" });
            let es5 = match lower_to_es5(src, minify, false) {
                Ok(l) => l.code,
                Err(e) => {
                    failures.push(format!("{label}: {e:?}"));
                    continue;
                }
            };
            let got = run(&tmp, file, &es5);
            if want != got {
                failures.push(format!(
                    "{label}: output differs\nwant {want:?}\n got {got:?}\n{es5}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn closure_capturing_loop_binding_is_an_error() {
    let src =
        "var fs = [];\nfor (let i = 0; i < 2; i++) fs.push(() => i);\nconsole.log(fs[0]());\n";
    let e = lower_to_es5(src, false, false).expect_err("per-iteration binding");
    assert_eq!((e.line, e.column), (2, 9), "{e:?}");
    assert!(
        e.message
            .contains("\"i\" is declared in a loop and captured by a closure"),
        "{e:?}"
    );
}

#[test]
fn unlowerable_syntax_is_named() {
    for (src, line, column, what) in [
        ("class A {}\n", 1, 0, "class syntax"),
        ("function* g() { yield 1; }\n", 1, 0, "generator functions"),
        ("var { a } = { a: 1 };\n", 1, 4, "destructuring"),
        ("for (var x of [1]) {}\n", 1, 0, "for-of loops"),
        ("var o = { ...a };\n", 1, 10, "object spread"),
        (
            "var f = function () { return new.target; };\n",
            1,
            29,
            "new.target",
        ),
    ] {
        let e = lower_to_es5(src, false, false).expect_err(src);
        assert_eq!(
            (e.line, e.column, e.message.as_str()),
            (
                line,
                column,
                format!("Transforming {what} to the configured target environment (\"es5\") is not supported yet").as_str()
            ),
            "{src}"
        );
    }
    let e = lower_to_es5("var a = 1;\nexport { a as \"b c\" };\n", false, false)
        .expect_err("string export name");
    assert_eq!(
        e.message,
        "Using the string \"b c\" as an export name is not supported in the configured target environment (\"es5\")"
    );
}

#[test]
fn import_meta_is_an_empty_object() {
    // esbuild: `"import.meta" is not available ... and will be empty`.
    let out = lower_to_es5("console.log(import.meta.url);\nexport {};\n", false, false)
        .expect("lowered")
        .code;
    assert!(out.contains("var import_meta = {};"), "{out}");
    assert!(out.contains("console.log(import_meta.url);"), "{out}");
}

#[test]
fn unparsable_bundle_is_an_error() {
    let e = lower_to_es5("var a = ;\n", false, false).expect_err("syntax error");
    assert!(
        e.message
            .starts_with("lower_to_es5: the bundle does not parse"),
        "{e:?}"
    );
}

#[test]
fn minified_output_has_no_backticks() {
    let src = "var s = ['a\"b', \"c'd\", `e${1}f`, 'line\\nbreak', 'tick`', '${x}'];\nconsole.log(s.join('|'));\n";
    let out = lower_to_es5(src, true, false).expect("lowered").code;
    assert!(!out.contains('`') || out.contains("\"tick`\""), "{out}");
    assert!(
        !out.contains('\n') || out.trim_end().lines().count() == 1,
        "{out}"
    );
    if node() {
        let tmp = scratch("backticks");
        assert_eq!(run(&tmp, "a.js", src), run(&tmp, "b.js", &out), "{out}");
    }
}

#[test]
fn source_map_points_into_the_input() {
    let src = "var f = (a) => `v${a}`;\nconsole.log(f(1));\n";
    for minify in [false, true] {
        let out = lower_to_es5(src, minify, true).expect("lowered");
        let map = out.map.expect("a source map");
        assert_eq!(map.get_sources().collect::<Vec<_>>(), ["es2015.js"]);
        // Every mapping points at a valid input position and comes from a valid output position.
        let in_lines: Vec<&str> = src.split('\n').collect();
        let out_lines: Vec<&str> = out.code.split('\n').collect();
        let mut tokens = 0;
        for t in map.get_tokens() {
            tokens += 1;
            let (sl, sc) = (t.get_src_line() as usize, t.get_src_col() as usize);
            let (dl, dc) = (t.get_dst_line() as usize, t.get_dst_col() as usize);
            assert!(
                sl < in_lines.len() && sc <= in_lines[sl].len(),
                "src {sl}:{sc}"
            );
            assert!(
                dl < out_lines.len() && dc <= out_lines[dl].len(),
                "dst {dl}:{dc}"
            );
        }
        assert!(tokens > 5, "{tokens} mappings");
        // `console` maps back to line 2, column 0.
        let lookup = map.generate_lookup_table();
        let (line, col) = out_lines
            .iter()
            .enumerate()
            .find_map(|(l, s)| s.find("console").map(|c| (l, c)))
            .expect("console in output");
        let tok = map
            .lookup_token(
                &lookup,
                u32::try_from(line).unwrap_or(0),
                u32::try_from(col).unwrap_or(0),
            )
            .expect("mapped");
        assert_eq!(
            (tok.get_src_line(), tok.get_src_col()),
            (1, 0),
            "minify={minify}"
        );
    }
}
