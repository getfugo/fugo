//! `register` covers every site-bound `spec::FUNCS` entry, checks kwargs against the spec and
//! takes safety from it.

use ssg_funcs::spec::{self, NameKind};

use crate::support;

/// The signatures T34 builds against (REWRITE_PLAN.md §2.6) stay as frozen.
#[test]
fn register_signature_is_frozen() {
    let f: fn(&mut tera::Tera, &ssg_sitefuncs::Handles) = ssg_sitefuncs::register;
    let _ = f;
}

/// A call of `name` as Tera would validate it.
fn call_of(name: &str, kind: NameKind) -> String {
    match kind {
        NameKind::Filter => format!("{{{{ 1 | {name} }}}}"),
        NameKind::Function => format!("{{{{ {name}() }}}}"),
        NameKind::Test => format!("{{{{ 1 is {name} }}}}"),
    }
}

#[test]
fn every_site_bound_entry_is_registered_and_nothing_else() {
    let site = support::load();
    let site_bound: Vec<_> = spec::FUNCS.iter().filter(|f| f.site_bound).collect();
    assert_eq!(site_bound.len(), 71, "site-bound FUNCS entries");

    // Only sitefuncs: Tera validates every name when a template is added.
    let mut only = tera::Tera::default();
    ssg_sitefuncs::register(&mut only, &site.handles);
    let mut missing = Vec::new();
    for f in &site_bound {
        if only
            .add_raw_template(&format!("t_{}", f.name), &call_of(f.name, f.kind))
            .is_err()
        {
            missing.push(f.name);
        }
    }
    assert!(missing.is_empty(), "not registered: {missing:?}");

    // No pure entry is registered by sitefuncs.
    let mut extra = Vec::new();
    for f in spec::FUNCS
        .iter()
        .filter(|f| !f.site_bound && f.source == spec::Source::Native)
    {
        if only
            .add_raw_template(&format!("p_{}", f.name), &call_of(f.name, f.kind))
            .is_ok()
        {
            extra.push(f.name);
        }
    }
    assert!(
        extra.is_empty(),
        "pure entries registered by sitefuncs: {extra:?}"
    );
}

#[test]
fn kwargs_are_checked_against_the_spec() {
    let site = support::load();
    let s = site.home_scope();
    let e = site
        .try_render("{{ get_page(path=\"/\", nope=1) }}", &s)
        .expect_err("unknown kwarg");
    assert!(
        e.to_string()
            .contains("unknown kwarg `nope` for `get_page`"),
        "{e}"
    );
    let e = site
        .try_render("{{ ref() }}", &s)
        .expect_err("missing kwarg");
    assert!(
        e.to_string().contains("missing required kwarg `path`"),
        "{e}"
    );
}

#[test]
fn safety_comes_from_the_spec() {
    let site = support::load();
    let s = site.home_scope();
    // `markdownify` is safe: its HTML is not escaped; `i18n` is not.
    assert_eq!(
        site.render("{{ \"<b>x</b>\" | markdownify }}", &s),
        "<b>x</b><!-- Home depth 1 -->"
    );
    assert_eq!(
        site.render("{{ i18n(key=\"<x>\") }}", &s),
        "",
        "a missing key is empty"
    );
    assert_eq!(
        site.render(
            "{{ (get_page(path=\"/posts\") | get(key=\"title\")) ~ \"<\" }}",
            &s
        ),
        "Posts&lt;"
    );
}
