//! The template contract (REWRITE_PLAN.md §4.8): the converted layouts load against `FUNCS`, their
//! calls use declared kwargs, `FUNCS` is consistent with Tera, and `template-api.md` equals the
//! spec.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use pretty_assertions::assert_eq;
use ssg_funcs::spec::{self, NameKind, Source};
use ssg_testkit::contract::{
    Call, TemplateSource, contract_instance, kwarg_findings, scan_calls, template_api_path,
    template_sets,
};
use tera::Tera;

fn tpl(name: &str, source: &str) -> TemplateSource {
    TemplateSource {
        name: name.to_owned(),
        path: PathBuf::from(name),
        source: source.to_owned(),
    }
}

/// Every template set of `sites` and `crates/layouts/embedded` loads into the contract
/// instance, and no call uses an unknown kwarg or misses a required one.
#[test]
fn contract_is_clean() {
    let sets = template_sets().unwrap();
    let testsite = sets
        .iter()
        .find(|s| s.label == "testsite")
        .expect("sites/testsite");
    let names: Vec<&str> = testsite.templates.iter().map(|t| t.name.as_str()).collect();
    for layout in [
        "404.html",
        "_partials/page.html",
        "home.json",
        "list.html",
        "single.html",
    ] {
        assert!(names.contains(&layout), "{layout} missing from {names:?}");
    }
    let mut failures = Vec::new();
    for set in &sets {
        if let Err(e) = contract_instance(&set.templates) {
            failures.push(format!("[{}] {e:#}", set.label));
        }
        for t in &set.templates {
            failures.extend(
                kwarg_findings(t)
                    .iter()
                    .map(|f| format!("[{}] {f}", set.label)),
            );
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The contract instance rejects names outside `FUNCS`; the scanner rejects unknown and missing
/// kwargs.
#[test]
fn contract_rejects_violations() {
    for src in [
        "{{ x | printf }}",
        "{{ where(a=1) }}",
        "{{ x is like }}",
        "{% include \"_partials/nope.html\" %}",
    ] {
        assert!(contract_instance(&[tpl("t.html", src)]).is_err(), "{src}");
    }
    // Embedded templates resolve through the fallback prefix even before T32 writes them.
    contract_instance(&[tpl("t.html", "{% include \"_partials/pagination.html\" %}")]).unwrap();

    let cases = [
        ("{{ x | split(sep=\"/\") }}", "unknown kwarg `sep`"),
        (
            "{{ x | sort_by(reverse=true) }}",
            "missing required kwarg `attribute`",
        ),
        ("{{ get_page(path=\"/\", pg=page) }}", "unknown kwarg `pg`"),
        (
            "{% if x is starting_with(prefix=\"a\") %}{% endif %}",
            "unknown kwarg `prefix`",
        ),
        ("{{ x | nth }}", "missing required kwarg `n`"),
        (
            "{%- filter replace(from=\"a\") %}x{% endfilter %}",
            "missing required kwarg `to`",
        ),
    ];
    for (src, want) in cases {
        let findings = kwarg_findings(&tpl("t.html", src));
        assert_eq!(findings.len(), 1, "{src}: {findings:?}");
        assert!(
            findings[0].message.contains(want),
            "{src}: {}",
            findings[0].message
        );
    }
    for ok in [
        "{{ partial(name=\"x\", page=page, anything=1) }}",
        "{{ x | split(pat=\"/\") | nth(n=1) }}",
        "{% component bc(page, sep=\"/\", @__nh) %}{{ sep }}{% endcomponent bc %}",
        "{# {{ x | nth }} #}{% raw %}{{ x | nth }}{% endraw %}",
        "{{ \"x | nth\" }}",
    ] {
        let findings = kwarg_findings(&tpl("t.html", ok));
        assert!(findings.is_empty(), "{ok}: {findings:?}");
    }
    // Placeholders check kwargs at render time too.
    let tera = contract_instance(&[tpl("t.html", "{{ x | sort_by(attr=\"a\") }}")]).unwrap();
    let mut ctx = tera::Context::new();
    ctx.insert("x", &Vec::<i32>::new());
    let err = format!("{:#}", tera.render("t.html", &ctx).unwrap_err());
    assert!(err.contains("unknown kwarg `attr`"), "{err}");
}

#[test]
fn scanner_reads_calls() {
    let src = "{% set p = get_page(path=\"/a\") %}\n{% for x in l | sort_by(attribute=\"t\", reverse=true) %}\
               {% if x is not starting_with(pat=\"a\") and (x is defined) %}{{ x | upper }}{% endif %}{% endfor %}\
               {{ <card title={f(a=[1, g(b=2)], c={\"d\": 1})} /> }}";
    let calls: Vec<(NameKind, String, Vec<String>, usize)> = scan_calls(src)
        .into_iter()
        .map(
            |Call {
                 kind,
                 name,
                 kwargs,
                 line,
                 ..
             }| (kind, name, kwargs, line),
        )
        .collect();
    let s = |v: &[&str]| v.iter().map(|x| (*x).to_owned()).collect::<Vec<_>>();
    assert_eq!(
        calls,
        vec![
            (NameKind::Function, "get_page".to_owned(), s(&["path"]), 1),
            (
                NameKind::Filter,
                "sort_by".to_owned(),
                s(&["attribute", "reverse"]),
                2
            ),
            (NameKind::Test, "starting_with".to_owned(), s(&["pat"]), 2),
            (NameKind::Test, "defined".to_owned(), s(&[]), 2),
            (NameKind::Filter, "upper".to_owned(), s(&[]), 2),
            (NameKind::Function, "f".to_owned(), s(&["a", "c"]), 2),
            (NameKind::Function, "g".to_owned(), s(&["b"]), 2),
        ]
    );
}

/// `FUNCS` is well-formed: one entry per (name, kind); unique kwarg names; built-ins are exactly
/// the names a bare Tera knows (this port never overrides one); the §4.2 scope names are present.
#[test]
fn funcs_spec_is_consistent() {
    let mut seen = BTreeSet::new();
    for f in spec::FUNCS {
        assert!(
            seen.insert((f.kind, f.name)),
            "duplicate {:?} {}",
            f.kind,
            f.name
        );
        let kw: BTreeSet<&str> = f.kwargs.iter().map(|k| k.name).collect();
        assert_eq!(kw.len(), f.kwargs.len(), "{}: duplicate kwargs", f.name);
        assert!(!f.doc.is_empty(), "{}: no doc", f.name);
        let src = match f.kind {
            NameKind::Filter => format!("{{{{ x | {} }}}}", f.name),
            NameKind::Function if f.name == "super" => continue,
            NameKind::Function => format!("{{{{ {}() }}}}", f.name),
            NameKind::Test => format!("{{{{ x is {} }}}}", f.name),
        };
        let known = Tera::default().add_raw_template("t", &src).is_ok();
        assert_eq!(
            known,
            f.source == Source::Builtin,
            "{}: Tera built-in = {known}",
            f.name
        );
    }
    assert!(
        spec::FUNCS
            .iter()
            .filter(|f| f.site_bound)
            .all(|f| f.source == Source::Native)
    );
    for role_names in spec::CONTEXTS.iter().filter(|c| !c.names.is_empty()) {
        assert!(
            role_names.names.iter().any(|n| n.name == "site"),
            "{}",
            role_names.title
        );
    }
    let mut embedded = spec::EMBEDDED_TEMPLATES.to_vec();
    embedded.sort_unstable();
    embedded.dedup();
    assert_eq!(
        embedded,
        spec::EMBEDDED_TEMPLATES,
        "EMBEDDED_TEMPLATES sorted and unique"
    );
}

/// `docs/data/template_api.json`, the documentation site's template reference, equals the spec.
/// `INSTA_UPDATE=always` rewrites it (reviewed with `git diff`).
#[test]
fn docs_template_api_json_matches_spec() {
    let path = ssg_testkit::fixture::repo_dir().join("docs/data/template_api.json");
    let want = spec::template_api_json();
    if std::env::var("INSTA_UPDATE").is_ok_and(|v| v == "always") {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, &want).unwrap();
    }
    let have = fs::read_to_string(&path).unwrap_or_default();
    assert!(
        have == want,
        "{} is stale; regenerate with INSTA_UPDATE=always cargo test -p ssg-testkit contract",
        path.display()
    );
    let v: serde_json::Value = serde_json::from_str(&want).expect("valid JSON");
    assert_eq!(
        v["funcs"].as_array().map(Vec::len),
        Some(spec::FUNCS.len()),
        "every name"
    );
}

/// `docs/rust-port/template-api.md` equals the spec. `INSTA_UPDATE=always` rewrites it (reviewed with
/// `git diff`, like every snapshot of the workspace).
#[test]
fn template_api_md_matches_spec() {
    let path = template_api_path();
    let want = spec::template_api_markdown();
    if std::env::var("INSTA_UPDATE").is_ok_and(|v| v == "always") {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, &want).unwrap();
    }
    let have = fs::read_to_string(&path).unwrap_or_default();
    assert!(
        have == want,
        "{} is stale; regenerate with INSTA_UPDATE=always cargo test -p ssg-testkit contract",
        path.display()
    );
}
