//! `[minify.tdewolff]` decoding.

use std::sync::Arc;

use ssg_base::{Map, Value};
use ssg_config::MinifyConfig;
use ssg_minify::options::{HtmlComments, IgnoreReason, TemplateSyntax, XmlComments, XmlWhitespace};
use ssg_minify::{IgnoredOption, Minifier, MinifyError, MinifyTarget, Options};

fn table(entries: &[(&str, &[(&str, Value)])]) -> Map {
    let mut m = Map::new();
    for (section, keys) in entries {
        let mut s = Map::new();
        for (k, v) in *keys {
            s.insert(*k, v.clone());
        }
        m.insert(*section, Value::Map(Arc::new(s)));
    }
    m
}

fn strings(items: &[&str]) -> Value {
    Value::Array(Arc::new(
        items.iter().map(|s| Value::String((*s).into())).collect(),
    ))
}

#[test]
fn defaults_are_gos() {
    let o = Options::default();
    assert_eq!(o.html.comments, HtmlComments::KeepSpecial);
    assert!(o.html.keep_end_tags && o.html.keep_document_tags && o.html.keep_default_attr_vals);
    assert_eq!(o.html.templates, TemplateSyntax::None);
    assert!(o.css.keep_css2);
    assert!(!o.js.keep_var_names);
    assert_eq!(o.svg.comments, XmlComments::Remove);
    assert_eq!(o.xml.whitespace, XmlWhitespace::Collapse);
    let decoded = Options::from_tdewolff(&Map::new()).unwrap();
    assert_eq!(decoded.options, o);
    assert!(decoded.ignored.is_empty());
}

#[test]
fn honoured_keys_any_case() {
    let t = table(&[
        (
            "html",
            &[
                ("keepComments", Value::Bool(true)),
                ("KEEPENDTAGS", Value::Bool(false)),
                ("keepdocumenttags", Value::Bool(false)),
                ("keepDefaultAttrVals", Value::Bool(false)),
                ("templateDelims", strings(&["{{", "}}"])),
            ],
        ),
        ("css", &[("keepCSS2", Value::Bool(false))]),
        ("js", &[("keepVarNames", Value::Bool(true))]),
        ("svg", &[("keepComments", Value::Bool(true))]),
        ("XML", &[("keepWhitespace", Value::Bool(true))]),
    ]);
    let d = Options::from_tdewolff(&t).unwrap();
    let o = d.options;
    assert_eq!(o.html.comments, HtmlComments::KeepAll);
    assert!(!o.html.keep_end_tags && !o.html.keep_document_tags && !o.html.keep_default_attr_vals);
    assert_eq!(o.html.templates, TemplateSyntax::Braces);
    assert!(!o.css.keep_css2);
    assert!(o.js.keep_var_names);
    assert_eq!(o.svg.comments, XmlComments::Keep);
    assert_eq!(o.xml.whitespace, XmlWhitespace::Keep);
    assert!(d.ignored.is_empty(), "{:?}", d.ignored);
}

#[test]
fn special_comments_and_legacy_alias() {
    let comments = |keys: &[(&str, Value)]| {
        Options::from_tdewolff(&table(&[("html", keys)]))
            .unwrap()
            .options
            .html
            .comments
    };
    assert_eq!(
        comments(&[("keepSpecialComments", Value::Bool(false))]),
        HtmlComments::Remove
    );
    assert_eq!(
        comments(&[("keepConditionalComments", Value::Bool(false))]),
        HtmlComments::Remove
    );
    // The new key wins over the legacy one.
    assert_eq!(
        comments(&[
            ("keepConditionalComments", Value::Bool(false)),
            ("keepSpecialComments", Value::Bool(true)),
        ]),
        HtmlComments::KeepSpecial
    );
    assert_eq!(
        Options::from_tdewolff(&table(&[(
            "html",
            &[("templateDelims", strings(&["<%", "%>"]))]
        )]))
        .unwrap()
        .options
        .html
        .templates,
        TemplateSyntax::ChevronPercent
    );
}

#[test]
fn ignored_keys_are_reported() {
    let t = table(&[
        (
            "html",
            &[
                ("keepQuotes", Value::Bool(true)),
                ("keepWhitespace", Value::Bool(true)),
                ("templateDelims", strings(&["[[", "]]"])),
                ("bogus", Value::Bool(true)),
            ],
        ),
        (
            "css",
            &[("precision", Value::Int(3)), ("decimal", Value::Int(2))],
        ),
        (
            "js",
            &[("version", Value::Int(2022)), ("precision", Value::Int(0))],
        ),
        (
            "json",
            &[
                ("keepNumbers", Value::Bool(true)),
                ("precision", Value::Int(1)),
            ],
        ),
        ("svg", &[("precision", Value::Int(2))]),
        ("yaml", &[]),
    ]);
    let d = Options::from_tdewolff(&t).unwrap();
    assert_eq!(d.options, Options::default());
    let got: Vec<(&str, IgnoreReason)> = d
        .ignored
        .iter()
        .map(|i| (i.key.as_str(), i.reason))
        .collect();
    use IgnoreReason::{NoEquivalent as N, Unknown as U};
    assert_eq!(
        got,
        [
            ("css.decimal", N),
            ("css.precision", N),
            ("html.bogus", U),
            ("html.keepQuotes", N),
            ("html.keepWhitespace", N),
            ("html.templateDelims", N),
            ("js.precision", N),
            ("js.version", N),
            ("json.keepNumbers", N),
            ("json.precision", N),
            ("svg.precision", N),
            ("yaml", U),
        ]
    );
}

#[test]
fn wrong_types_are_errors() {
    let cases: [(&str, &[(&str, Value)]); 3] = [
        ("html", &[("keepComments", Value::String("yes".into()))]),
        ("html", &[("templateDelims", Value::String("{{ }}".into()))]),
        ("xml", &[("keepWhitespace", Value::Int(1))]),
    ];
    for (section, keys) in cases {
        let err = Options::from_tdewolff(&table(&[(section, keys)])).unwrap_err();
        assert!(matches!(err, MinifyError::Option { .. }), "{err}");
    }
    let mut m = Map::new();
    m.insert("html", Value::Bool(true));
    let err = Options::from_tdewolff(&m).unwrap_err();
    assert_eq!(err.to_string(), "[minify.tdewolff] html: expected a table");
}

#[test]
fn minifier_from_config() {
    let config = MinifyConfig {
        minify_output: true,
        disabled: vec![MinifyTarget::Json],
        options: table(&[(
            "html",
            &[
                ("keepWhitespace", Value::Bool(false)),
                ("keepEndTags", Value::Bool(false)),
            ],
        )]),
    };
    let m = Minifier::new(&config).unwrap();
    assert!(!m.options().html.keep_end_tags);
    assert!(!m.is_enabled(MinifyTarget::Json));
    assert!(m.is_enabled(MinifyTarget::Html));
    assert_eq!(
        m.ignored_options(),
        [IgnoredOption {
            key: "html.keepWhitespace".to_owned(),
            reason: IgnoreReason::NoEquivalent,
        }]
    );
    assert_eq!(m.minify(MinifyTarget::Json, "{ }").unwrap(), "{ }");
}
