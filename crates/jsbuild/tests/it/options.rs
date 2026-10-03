use std::collections::BTreeMap;

use serde_json::json;
use ssg_jsbuild::{
    DropKind, Format, JsBuildOptions, Jsx, Loader, OptionsError, Platform, SourceMap, Target,
};

fn decode(v: &serde_json::Value) -> Result<JsBuildOptions, OptionsError> {
    JsBuildOptions::from_json(v)
}

#[test]
fn null_and_empty_give_the_defaults() {
    let d = JsBuildOptions::default();
    assert_eq!(decode(&json!(null)).unwrap(), d);
    assert_eq!(decode(&json!({})).unwrap(), d);
    assert!(d.sources_content);
    assert_eq!(
        (d.target, d.format, d.platform, d.jsx, d.source_map),
        (
            Target::EsNext,
            Format::Iife,
            Platform::Browser,
            Jsx::Transform,
            SourceMap::None
        )
    );
}

#[test]
fn keys_are_case_insensitive_and_scalars_lenient() {
    let o = decode(&json!({
        "TARGET": "ES6", "Format": "CJS", "minify": "true", "SourceMap": "Linked",
        "sourcesContent": 0, "targetPath": "/out/x.js", "JSX": "automatic",
        "JSXImportSource": "preact", "externals": "react", "drop": "console",
        "defines": {"n": 42, "dev": false, "env": "\"prod\""},
        "loaders": {".svg": "dataurl", ".bin": "copy"},
        "shims": {"react": "js/shims/react.js"},
        "params": {"a": [1, "two"]},
        "avoidTDZ": true, "unknown": [1]
    }))
    .unwrap();
    assert_eq!(o.target, Target::Es2015);
    assert_eq!(o.format, Format::Cjs);
    assert!(o.minify);
    assert!(!o.sources_content);
    assert_eq!(o.source_map, SourceMap::Linked);
    assert_eq!(o.target_path.as_deref(), Some("out/x.js"));
    assert_eq!(o.jsx, Jsx::Automatic);
    assert_eq!(o.jsx_import_source.as_deref(), Some("preact"));
    assert_eq!(o.externals, ["react"]);
    assert_eq!(o.drop, Some(DropKind::Console));
    assert_eq!(
        o.defines,
        BTreeMap::from([
            ("dev".to_owned(), "false".to_owned()),
            ("env".to_owned(), "\"prod\"".to_owned()),
            ("n".to_owned(), "42".to_owned()),
        ])
    );
    assert_eq!(
        o.loaders,
        BTreeMap::from([
            (".bin".to_owned(), Loader::File),
            (".svg".to_owned(), Loader::DataUrl)
        ])
    );
    assert_eq!(o.params, Some(json!({"a": [1, "two"]})));
    assert_eq!(o.loader_for("/x/icon.svg"), Loader::DataUrl);
    assert_eq!(o.loader_for("/x/a.tsx"), Loader::Tsx);
    assert_eq!(o.loader_for("/x/a.txt"), Loader::Text);
    assert_eq!(o.loader_for("/x.y/noext"), Loader::Js);
}

#[test]
fn bad_values_are_errors() {
    let value = |option: &'static str, value: &str| OptionsError::Value {
        option,
        value: value.to_owned(),
    };
    for (opts, want) in [
        (json!({"target": "es3"}), value("target", "es3")),
        (json!({"format": "umd"}), value("format", "umd")),
        (json!({"jsx": "react"}), value("jsx", "react")),
        (json!({"platform": "deno"}), value("platform", "deno")),
        (json!({"sourceMap": "both"}), value("sourceMap", "both")),
        (json!({"loaders": {".svg": "wat"}}), value("loaders", "wat")),
        (json!({"drop": "everything"}), value("drop", "everything")),
        (json!({"minify": "maybe"}), value("minify", "maybe")),
        (
            json!({"loaders": {".x": "none"}}),
            OptionsError::NoneLoader {
                extension: ".x".to_owned(),
            },
        ),
        (
            json!({"defines": {"a=b": "1"}}),
            OptionsError::DefineKey("a=b".to_owned()),
        ),
        (
            json!({"minify": ["x"]}),
            OptionsError::Type {
                option: "minify",
                expected: "a boolean",
                got: "a list".to_owned(),
            },
        ),
        (json!([1]), OptionsError::NotAMap("a list")),
    ] {
        assert_eq!(decode(&opts), Err(want), "{opts}");
    }
}

#[test]
fn loaders_by_media_type() {
    assert_eq!(Loader::from_media_type("text/javascript"), Some(Loader::Js));
    assert_eq!(Loader::from_media_type("text/typescript"), Some(Loader::Ts));
    assert_eq!(Loader::from_media_type("text/tsx"), Some(Loader::Tsx));
    assert_eq!(Loader::from_media_type("text/jsx"), Some(Loader::Jsx));
    assert_eq!(Loader::from_media_type("application/json"), None);
}
