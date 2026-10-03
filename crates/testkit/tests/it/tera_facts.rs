//! Tera 2.4.0 behaviours the template model (REWRITE_PLAN.md §4) relies on, verified by T02.
//!
//! Each test names the tera-2.4.0 source that implements the behaviour; the same facts are
//! recorded in `docs/rust-port/template-api.md` ("Tera facts").

use ssg_testkit::tera_value;
use tera::{Context, Tera};

fn ctx() -> Context {
    let mut c = Context::new();
    c.insert(
        "page",
        &tera_value(&serde_json::json!({
            "title": "T",
            "params": { "a": { "b": 1 }, "flag": false },
            "parent": null,
        })),
    );
    c.insert(
        "__nh",
        &tera_value(&serde_json::json!({ "page": 7, "depth": 0 })),
    );
    c
}

fn render(src: &str) -> Result<String, String> {
    Tera::one_off(src, &ctx(), true).map_err(|e| format!("{e:#}"))
}

fn render_err(src: &str) -> String {
    match render(src) {
        Ok(out) => panic!("expected an error for {src:?}, got {out:?}"),
        Err(e) => e,
    }
}

/// `__nh` is an ordinary identifier (lexer.rs:611-622 accepts `_` at any position) and `@__nh`
/// an implicit component parameter (parser.rs:1331-1336), looked up in the caller's scope chain
/// (state.rs:146-157). Without the declaration a component cannot see it (components are
/// hygienic).
#[test]
fn implicit_nh_scope_in_components() {
    let mut tera = Tera::default();
    tera.add_raw_templates([
        (
            "c.html",
            "{% component with_scope(x, @__nh) %}{{ x }}:{{ __nh.page }}{% endcomponent with_scope %}\
             {% component without_scope(x) %}{{ x }}:{{ __nh is defined }}{% endcomponent without_scope %}",
        ),
        ("t.html", "{{ <with_scope x={1} /> }} {{ <without_scope x={2} /> }}"),
    ])
    .unwrap();
    assert_eq!(tera.render("t.html", &ctx()).unwrap(), "1:7 2:false");
}

/// Component call arguments are `name={expr}`, `name="literal"` or the shorthand `name`;
/// `name=expr` without braces is a syntax error (the plan's `<breadcrumbs page=page />` must be
/// written `<breadcrumbs page={page} />` or `<breadcrumbs page />`).
#[test]
fn component_call_argument_syntax() {
    let comp = "{% component bc(page, sep=\"/\") %}{{ page.title }}{{ sep }}{% endcomponent bc %}";
    for (call, ok) in [
        ("{{ <bc page={page} /> }}", true),
        ("{{ <bc page /> }}", true),
        ("{{ <bc page={page} sep=\">\" /> }}", true),
        ("{{ <bc page=page /> }}", false),
    ] {
        let mut tera = Tera::default();
        let res = tera.add_raw_templates([("c.html", comp), ("t.html", call)]);
        assert_eq!(res.is_ok(), ok, "{call}: {res:?}");
    }
}

/// `==`/`!=` never fail on an undefined *final* segment: the path load pushes `undefined`
/// (interpreter.rs:800-826) and `Value`'s `PartialEq` (value/mod.rs:273-300) makes undefined
/// equal only to undefined, so `missing == none` is **false**. A missing *non-final* segment is
/// an error even inside `if` (interpreter.rs:809-817).
#[test]
fn equality_with_undefined_final_segment() {
    assert_eq!(
        render("{{ page.params.missing == \"a\" }}").unwrap(),
        "false"
    );
    assert_eq!(
        render("{{ page.params.missing != \"a\" }}").unwrap(),
        "true"
    );
    assert_eq!(
        render("{{ page.params.missing == none }}").unwrap(),
        "false"
    );
    assert_eq!(render("{{ page.parent == none }}").unwrap(), "true");
    assert_eq!(
        render("{{ page.params.missing is undefined }}").unwrap(),
        "true"
    );
    assert_eq!(
        render("{% if page.params.missing %}y{% else %}n{% endif %}").unwrap(),
        "n"
    );
    let e = render_err("{% if page.params.missing.deeper == 1 %}y{% endif %}");
    assert!(e.contains("missing"), "{e}");
    // Printing an undefined value is an error; `or ""` is the conversion idiom.
    render_err("{{ page.params.missing }}");
    assert_eq!(render("[{{ page.params.missing or \"\" }}]").unwrap(), "[]");
    // `or` returns a bool for bools, which is why Go's `default` maps to `default_if_empty`.
    assert_eq!(render("{{ page.params.flag or true }}").unwrap(), "true");
}

/// `?.` (lexer.rs:567) compiles to `LoadAttrOpt` (compiler.rs:174-183): an undefined or `none`
/// receiver yields undefined instead of an error (interpreter.rs:222-236). The result is still
/// undefined, so it must not be printed bare.
#[test]
fn optional_chaining() {
    assert_eq!(render("{{ page.parent?.title or \"-\" }}").unwrap(), "-");
    assert_eq!(render("{{ page.params.x?.b or \"-\" }}").unwrap(), "-");
    assert_eq!(render("{{ page.params.a?.b }}").unwrap(), "1");
    assert_eq!(
        render("{{ page.params.x?.y?.z is undefined }}").unwrap(),
        "true"
    );
    assert_eq!(render("{{ page.params?.a?.c == 1 }}").unwrap(), "false");
    render_err("{{ page.parent.title }}");
    render_err("{{ page.parent?.title }}");
}

/// `split(pat=)` (filters.rs:442-449), `nth(n=)` (filters.rs:516-519) and `replace(from=, to=)`
/// (filters.rs:199-213). Kwargs are not validated when a template is added: an unknown or
/// missing kwarg is a render-time error (filters.rs `kwargs.must_get`), which is why the
/// contract test checks kwargs itself.
#[test]
fn split_nth_replace_kwargs() {
    assert_eq!(
        render("{{ \"a/b/c\" | split(pat=\"/\") | nth(n=1) }}").unwrap(),
        "b"
    );
    assert_eq!(
        render("{{ \"a-b\" | replace(from=\"-\", to=\"+\") }}").unwrap(),
        "a+b"
    );
    let mut tera = Tera::default();
    tera.add_raw_template("t", "{{ \"a/b\" | split(sep=\"/\") }}")
        .expect("unknown kwargs pass at load time");
    let e = format!("{:#}", tera.render("t", &ctx()).unwrap_err());
    assert!(e.contains("pat"), "{e}");
}

/// Unknown filters, tests and functions are load-time errors (tera.rs `validate_template_references`).
#[test]
fn unknown_names_fail_at_load() {
    for src in [
        "{{ x | no_such_filter }}",
        "{{ no_such_fn() }}",
        "{{ x is no_such_test }}",
    ] {
        let mut tera = Tera::default();
        assert!(tera.add_raw_template("t", src).is_err(), "{src}");
    }
}
