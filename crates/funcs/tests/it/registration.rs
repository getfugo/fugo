//! `register_pure` registers exactly the pure entries of `spec::FUNCS` (and the tera-contrib
//! subset), and checks kwargs against the spec.

use ssg_funcs::spec::{self, FuncSpec, NameKind, Source};
use ssg_funcs::{NOT_COMPILED, pure_specs};
use tera::Context;

use crate::support::Harness;

/// A template that names `f` (the parser validates every name when a template is added).
fn naming(f: &FuncSpec) -> String {
    match f.kind {
        NameKind::Filter => format!("{{{{ x | {} }}}}", f.name),
        NameKind::Function => format!("{{{{ {}() }}}}", f.name),
        NameKind::Test => format!("{{% if x is {} %}}{{% endif %}}", f.name),
    }
}

#[test]
fn every_pure_entry_is_registered() {
    let mut h = Harness::new();
    let mut missing = Vec::new();
    for f in pure_specs() {
        if h.tera.add_raw_template("probe", &naming(f)).is_err() {
            missing.push(f.name);
        }
    }
    assert!(missing.is_empty(), "not registered: {missing:?}");
}

#[test]
fn site_bound_entries_are_left_to_sitefuncs() {
    let mut h = Harness::new();
    let mut registered = Vec::new();
    for f in spec::FUNCS.iter().filter(|f| f.site_bound) {
        if h.tera.add_raw_template("probe", &naming(f)).is_ok() {
            registered.push(f.name);
        }
    }
    assert!(
        registered.is_empty(),
        "site-bound but registered: {registered:?}"
    );
}

#[test]
fn counts() {
    let pure = pure_specs().count();
    let site = spec::FUNCS.iter().filter(|f| f.site_bound).count();
    let builtin = spec::FUNCS
        .iter()
        .filter(|f| f.source == Source::Builtin)
        .count();
    let contrib = pure_specs().filter(|f| f.source == Source::Contrib).count();
    eprintln!(
        "FUNCS: {} entries; pure (funcs) {pure} ({contrib} tera-contrib, {} not compiled in), site-bound (sitefuncs) {site}, Tera built-ins {builtin}",
        spec::FUNCS.len(),
        NOT_COMPILED.len()
    );
    assert_eq!(pure + site + builtin, spec::FUNCS.len());
}

#[test]
fn not_compiled_entries_fail_when_called() {
    let h = Harness::new();
    for name in NOT_COMPILED {
        let f = spec::FUNCS
            .iter()
            .find(|f| f.name == *name)
            .expect("in spec");
        let src = match f.kind {
            NameKind::Filter => format!("{{{{ 'x' | {name} }}}}"),
            NameKind::Function => format!("{{{{ {name}(text='x') }}}}"),
            NameKind::Test => format!("{{{{ 'x' is {name} }}}}"),
        };
        let err = h.render(&src, &Context::new()).expect_err(name);
        assert!(err.contains("not available"), "{name}: {err}");
    }
}

#[test]
fn kwargs_are_checked_against_the_spec() {
    let h = Harness::new();
    let ctx = Context::new();
    let err = h
        .render("{{ 'a' | pad_start(widht=3) }}", &ctx)
        .expect_err("unknown kwarg");
    assert!(err.contains("unknown kwarg `widht`"), "{err}");
    assert!(err.contains("x | pad_start(width=)"), "{err}");
    let err = h
        .render("{{ 'a' | substr }}", &ctx)
        .expect_err("missing kwarg");
    assert!(err.contains("missing required kwarg `start`"), "{err}");
    let err = h
        .render("{{ querify() }}", &ctx)
        .expect_err("missing kwarg");
    assert!(err.contains("missing required kwarg `params`"), "{err}");
}

#[test]
fn safety_follows_the_spec() {
    let h = Harness::new();
    let ctx = Context::new();
    // safe outputs are not escaped again
    assert_eq!(
        h.render("{{ '<b>' | html_escape }}", &ctx).unwrap(),
        "&lt;b&gt;"
    );
    assert_eq!(
        h.render("{{ {'a': '<'} | jsonify }}", &ctx).unwrap(),
        "{\"a\":\"\\u003c\"}"
    );
    // html_escape escapes safe input too
    assert_eq!(
        h.render("{{ '<b>' | safe | html_escape }}", &ctx).unwrap(),
        "&lt;b&gt;"
    );
    // plain outputs are escaped by the template
    assert_eq!(
        h.render("{{ '<p>a &amp; b</p>' | plainify }}", &ctx)
            .unwrap(),
        "a &amp; b\n"
    );
    // truncate_html keeps the input's safety
    assert_eq!(
        h.render(
            "{{ '<b>bold text</b>' | safe | truncate_html(length=4) }}",
            &ctx
        )
        .unwrap(),
        "<b>bold …</b>"
    );
    assert_eq!(
        h.render("{{ 'a <b> text' | truncate_html(length=6) }}", &ctx)
            .unwrap(),
        "a &lt;b&gt; …"
    );
}
