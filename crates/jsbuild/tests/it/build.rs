//! `js.Build` behaviour beyond the oracle: what this port adds to rolldown to behave like esbuild
//! (externals in IIFE output, `NODE_ENV`, `inject`, JSX `preserve`, CSS imports, diagnostics),
//! source map modes, and builds from a thread pool.

use std::path::PathBuf;
use std::sync::Arc;

use rayon::prelude::*;
use serde_json::{Value as Json, json};
use ssg_jsbuild::{JsBuildError, JsBuildOptions, JsBuildOutput, JsBuilder, MountedDirs, Source};

/// A site with `files` (paths relative to the site) and its assets mounted at the root.
struct Site {
    root: PathBuf,
    builder: JsBuilder,
    assets: MountedDirs,
}

fn site(name: &str, files: &[(&str, &str)]) -> Site {
    let root = crate::scratch(name);
    for (path, text) in files {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    Site {
        builder: JsBuilder::new(root.clone(), root.join("public")),
        assets: MountedDirs::new().mount(root.join("assets"), ""),
        root,
    }
}

impl Site {
    fn build(&self, path: &str, options: &Json) -> Result<JsBuildOutput, JsBuildError> {
        let contents = std::fs::read(self.root.join("assets").join(path)).unwrap();
        let media_type = match path.rsplit('.').next() {
            Some("ts") => "text/typescript",
            Some("tsx") => "text/tsx",
            Some("jsx") => "text/jsx",
            _ => "text/javascript",
        };
        let source = Source {
            path,
            media_type,
            contents: &contents,
        };
        let options = JsBuildOptions::from_json(options).unwrap();
        self.builder
            .build(Arc::new(self.assets.clone()), &source, &options)
    }

    fn code(&self, path: &str, options: &Json) -> String {
        let out = self
            .build(path, options)
            .unwrap_or_else(|e| panic!("{path}: {e}"));
        String::from_utf8(out.code).unwrap()
    }

    /// What the script does under node (see `run::trace`), or `None` without node.
    fn trace(&self, test: &str, code: &str, format: &str) -> Option<String> {
        let node = crate::run::node(test)?;
        let dir = self.root.join("run");
        std::fs::create_dir_all(&dir).unwrap();
        Some(crate::run::trace(
            &node,
            &dir,
            test,
            code.as_bytes(),
            format,
        ))
    }
}

#[test]
fn externals_in_iife_output_are_required() {
    let s = site(
        "iife-externals",
        &[(
            "assets/js/main.js",
            "import ext from 'extlib';\nimport { sub } from 'extlib/sub';\nconsole.log(ext, sub);\n",
        )],
    );
    let code = s.code("js/main.js", &json!({"externals": ["extlib"]}));
    // esbuild calls require() for an external; rolldown would read a global named after it.
    assert!(code.contains(r#"require("extlib")"#), "{code}");
    assert!(code.contains(r#"require("extlib/sub")"#), "{code}");
}

#[test]
fn node_env_follows_minify_even_with_drop() {
    let s = site(
        "node-env",
        &[(
            "assets/js/main.js",
            "console.log(process.env.NODE_ENV);\ndebugger;\n",
        )],
    );
    let env = |options: Json| {
        let code = s.code("js/main.js", &options);
        if code.contains("production") {
            "production"
        } else if code.contains("development") {
            "development"
        } else {
            panic!("no NODE_ENV in {code}")
        }
    };
    assert_eq!(env(json!({})), "development");
    assert_eq!(env(json!({"minify": true})), "production");
    // `drop` uses the minifier's compressor, which made rolldown pick "production".
    assert_eq!(env(json!({"drop": "debugger"})), "development");
    // The site's own define wins, and node gets none.
    let code = s.code(
        "js/main.js",
        &json!({"defines": {"process.env.NODE_ENV": "\"test\""}}),
    );
    assert!(code.contains("\"test\""), "{code}");
    let code = s.code("js/main.js", &json!({"platform": "node", "format": "cjs"}));
    assert!(code.contains("process.env.NODE_ENV"), "{code}");
}

#[test]
fn jsx_preserve_keeps_component_names_when_minified() {
    let s = site(
        "jsx-preserve",
        &[
            (
                "assets/js/button.jsx",
                "export const Button = (p) => <b>{p.label}</b>;\n",
            ),
            (
                "assets/js/main.jsx",
                "import { Button } from './button.jsx';\nexport const el = <Button label=\"x\" />;\n",
            ),
        ],
    );
    let code = s.code(
        "js/main.jsx",
        &json!({"jsx": "preserve", "minify": true, "format": "esm"}),
    );
    // A lower-case name would make JSX read the component as an HTML element.
    assert!(code.contains("<Button"), "{code}");
}

#[test]
fn inject_files_run_first_even_when_unused() {
    let s = site(
        "inject",
        &[
            (
                "assets/js/inject/side.js",
                "console.log('injected');\nexport const unused = 1;\n",
            ),
            (
                "assets/js/inject/h.js",
                "export function h(t) { return 'h:' + t; }\n",
            ),
            ("assets/js/main.js", "console.log(h('x'));\n"),
        ],
    );
    let code = s.code(
        "js/main.js",
        &json!({"inject": ["js/inject/side.js", "js/inject/h.js"]}),
    );
    if let Some(t) = s.trace("inject", &code, "iife") {
        assert_eq!(
            t, "console.log(\"injected\")\nconsole.log(\"h:x\")\n",
            "{code}"
        );
    }
}

#[test]
fn unresolved_imports_are_errors_in_esbuild_words() {
    let s = site(
        "unresolved",
        &[(
            "assets/js/main.js",
            "import x from 'nope';\nconsole.log(x);\n",
        )],
    );
    let err = s.build("js/main.js", &json!({})).unwrap_err();
    let JsBuildError::Build(diags) = err else {
        panic!("{err:?}")
    };
    let d = &diags[0];
    assert_eq!(d.text, r#"Could not resolve "nope""#);
    let p = d.position.as_ref().unwrap();
    assert_eq!(
        (p.file.as_path(), p.line, p.column),
        (s.root.join("assets/js/main.js").as_path(), 1, 14)
    );
}

#[test]
fn error_columns_are_bytes() {
    // "é" and "😀" are 2 and 4 bytes, 1 and 2 UTF-16 units: the ";" is at byte 21 (UTF-16
    // unit 18).
    let s = site(
        "columns",
        &[("assets/js/main.js", "const s = \"é😀\" + ;\n")],
    );
    let err = s.build("js/main.js", &json!({})).unwrap_err();
    let JsBuildError::Build(diags) = err else {
        panic!("{err:?}")
    };
    let p = diags[0].position.as_ref().unwrap();
    assert_eq!((p.line, p.column), (1, 21), "{diags:?}");
}

#[test]
fn source_map_modes() {
    let s = site(
        "maps",
        &[
            (
                "assets/js/lib.js",
                "export function n(x) { console.log('n'); return x * 2; }\n",
            ),
            (
                "assets/js/main.js",
                "import { n } from './lib.js';\nconsole.log(n(21));\n",
            ),
        ],
    );
    let lib_url = ssg_jsbuild::file_url(&s.root.join("assets/js/lib.js"));

    let linked = s
        .build(
            "js/main.js",
            &json!({"sourceMap": "linked", "targetPath": "out/app.js"}),
        )
        .unwrap();
    let code = String::from_utf8(linked.code).unwrap();
    assert!(
        code.ends_with("//# sourceMappingURL=app.js.map\n"),
        "{code}"
    );
    let map: Json = serde_json::from_slice(&linked.source_map.unwrap()).unwrap();
    assert!(
        map["sources"].as_array().unwrap().contains(&json!(lib_url)),
        "{map}"
    );
    assert_eq!(
        map.as_object().unwrap().keys().collect::<Vec<_>>(),
        ["version", "sources", "sourcesContent", "mappings", "names"]
    );

    let external = s
        .build(
            "js/main.js",
            &json!({"sourceMap": "external", "sourcesContent": false}),
        )
        .unwrap();
    assert!(
        !String::from_utf8(external.code)
            .unwrap()
            .contains("sourceMappingURL")
    );
    let map: Json = serde_json::from_slice(&external.source_map.unwrap()).unwrap();
    assert!(map["sourcesContent"].is_null(), "{map}");

    let inline = s.code("js/main.js", &json!({"sourceMap": "inline"}));
    assert!(
        inline.contains("//# sourceMappingURL=data:application/json;base64,"),
        "{inline}"
    );
    let none = s.build("js/main.js", &json!({})).unwrap();
    assert!(none.source_map.is_none());
}

#[test]
fn css_imports_build_and_publish_no_css() {
    let s = site(
        "css",
        &[
            ("assets/css/site.css", "body { margin: 0 }\n"),
            (
                "assets/js/main.js",
                "import '../css/site.css';\nconsole.log('ok');\n",
            ),
        ],
    );
    let code = s.code("js/main.js", &json!({}));
    assert!(!code.contains("margin"), "{code}");
    if let Some(t) = s.trace("css", &code, "iife") {
        assert_eq!(t, "console.log(\"ok\")\n");
    }
}

#[test]
fn builds_from_a_thread_pool() {
    let s = site(
        "pool",
        &[
            ("assets/js/lib.js", "export const f = (i) => i * 2;\n"),
            (
                "assets/js/main.js",
                "import { f } from './lib.js';\nconsole.log(f(21));\n",
            ),
        ],
    );
    let want = s.code("js/main.js", &json!({"minify": true}));
    // One and two threads, as our render pool can have: every build blocks its thread
    // until done, under a lock like the store's.
    for threads in [1, 2] {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        let lock = std::sync::Mutex::new(());
        let outputs: Vec<String> = pool.install(|| {
            (0..32)
                .into_par_iter()
                .map(|_| {
                    let _held = lock.lock().unwrap();
                    s.code("js/main.js", &json!({"minify": true}))
                })
                .collect()
        });
        assert!(outputs.iter().all(|o| *o == want), "threads {threads}");
    }
}

/// Comments: minified, only legal comments stay (no `@__PURE__`, no coverage hints); legal
/// comments go to the end of the script once each (`legalComments: eof`, the default, as
/// esbuild does for bundles), stay where they are (`inline`) or go (`none`).
#[test]
fn comments_and_legal_comments() {
    let lib = "/*! lib v1 | MIT */\n/* istanbul ignore next */\nexport const make = () => /* @__PURE__ */ new Map();\n";
    let other = "/*! lib v1 | MIT */\nexport const twice = (n) => n * 2;\n";
    let s = site(
        "comments",
        &[
            ("assets/lib/a.js", lib),
            ("assets/lib/b.js", other),
            (
                "assets/main.js",
                "/*! main | Apache-2.0 */\nimport { make } from \"./lib/a.js\";\nimport { twice } from \"./lib/b.js\";\nexport const run = () => [make(), twice(2)];\n",
            ),
        ],
    );
    let min = s.code("main.js", &json!({"minify": true, "format": "esm"}));
    assert!(!min.contains("__PURE__"), "{min}");
    assert!(!min.contains("istanbul"), "{min}");
    assert_eq!(min.matches("/*! lib v1 | MIT */").count(), 1, "{min}");
    assert!(
        min.trim_end()
            .ends_with("/*! lib v1 | MIT */\n/*! main | Apache-2.0 */")
            || min
                .trim_end()
                .ends_with("/*! main | Apache-2.0 */\n/*! lib v1 | MIT */"),
        "the legal comments are at the end:\n{min}"
    );

    let inline = s.code(
        "main.js",
        &json!({"minify": true, "format": "esm", "legalComments": "inline"}),
    );
    assert_eq!(inline.matches("/*! lib v1 | MIT */").count(), 2, "{inline}");

    let none = s.code(
        "main.js",
        &json!({"minify": true, "format": "esm", "legalComments": "none"}),
    );
    assert!(!none.contains("/*"), "{none}");

    let plain = s.code("main.js", &json!({"format": "esm"}));
    assert!(
        plain.contains("__PURE__"),
        "not minified, annotations stay:\n{plain}"
    );
    assert_eq!(plain.matches("/*! lib v1 | MIT */").count(), 1, "{plain}");

    let external = s
        .build(
            "main.js",
            &json!({"minify": true, "format": "esm", "legalComments": "external"}),
        )
        .unwrap();
    let code = String::from_utf8(external.code).unwrap();
    assert!(!code.contains("/*"), "{code}");
    let legal = String::from_utf8(external.legal.expect("a legal file")).unwrap();
    assert_eq!(legal.matches("/*! lib v1 | MIT */").count(), 1, "{legal}");
    assert!(legal.contains("/*! main | Apache-2.0 */"), "{legal}");

    let linked = s
        .build(
            "main.js",
            &json!({"minify": true, "format": "esm", "legalComments": "linked"}),
        )
        .unwrap();
    let code = String::from_utf8(linked.code).unwrap();
    assert!(
        code.trim_end()
            .ends_with("/*! For license information please see main.js.LEGAL.txt */"),
        "{code}"
    );
    assert!(linked.legal.is_some());

    let plain_external = s
        .build("lib/b.js", &json!({"legalComments": "inline"}))
        .unwrap();
    assert!(plain_external.legal.is_none(), "inline writes no file");

    let bad = JsBuildOptions::from_json(&json!({"legalComments": "somewhere"}));
    assert!(bad.is_err());
}

/// A script without legal comments gets no legal file, whatever the mode.
#[test]
fn no_legal_file_without_legal_comments() {
    let s = site("no-legal", &[("assets/a.js", "export const a = 1;\n")]);
    for mode in ["external", "linked"] {
        let out = s
            .build("a.js", &json!({"format": "esm", "legalComments": mode}))
            .unwrap();
        assert!(out.legal.is_none(), "{mode}");
        assert!(
            !String::from_utf8(out.code).unwrap().contains("LEGAL"),
            "{mode}"
        );
    }
}
