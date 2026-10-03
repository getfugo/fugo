//! `templates check` (REWRITE_PLAN.md §4.8).

use crate::build::testsite;
use crate::{binary, site, stdout};

/// Every rule fires once on `bad-layouts.txtar`, at its position, and the check fails.
#[test]
fn bad_layouts_report_every_rule() {
    let s = site("bad-layouts.txtar");
    let o = binary(s.path(), &["templates", "check"], &[]);
    let out = stdout(&o);
    assert_eq!(o.status.code(), Some(1), "{out}");
    let expected = [
        // (1) the scan: legacy names and Go templates, left out of the rest.
        "ERROR [legacy-name] layouts/_default/list.html: legacy layout name; rename it to list.html",
        "ERROR [go-template] layouts/term.html:1: Go template syntax `{{ range`",
        // (2) Tera: syntax (explained by a lint, the snippet as its note) and names.
        "ERROR [call-attribute] layouts/section.html:2:43: Tera cannot use `.` after a call",
        "ERROR [nested-close] layouts/taxonomy.html:1:17: `}}` inside an expression ends the `{{ … }}` tag",
        "ERROR [tera-name] layouts/single.html:2:40: Unknown filter `nosuchfilter`",
        "ERROR [tera-name] layouts/home.html:8:12: Unknown template `Partials/head.html`",
        // (3) context names.
        "WARN  [context-name] layouts/posts/list.html:2:36: `pages_of_section` is not in the context of a Layout job template",
        // (4) lints.
        "ERROR [kwarg] layouts/home.html:11:8: unknown kwarg `sep` for `split`",
        "ERROR [legacy-literal] layouts/home.html:8:12: `Partials/head.html` is not a v0.146 template name; use \"_partials/head.html\"",
        "ERROR [unknown-partial] layouts/home.html:10:4: no partial `nope.html`",
        "WARN  [partial-component] layouts/home.html:9:4: `partial(name=\"card.html\")` with arguments (title)",
        "WARN  [component-scope] layouts/_partials/card.html:1:42: `ref` is site-bound and component `card`",
        "ERROR [content-field] layouts/_shortcodes/note.html:1:9: `page.content` is a content field",
        "WARN  [map-order] layouts/home.html:6:16: a template map literal is ranged in insertion order",
        "WARN  [none-compare] layouts/home.html:4:21: `== none` is false when the value is undefined",
        // (5) coverage misses.
        "ERROR [no-shortcode] content/_index.md:4:8: no template for shortcode \"missing\" (in en home /)",
    ];
    for want in expected {
        assert!(
            out.lines().any(|l| l.starts_with(want)),
            "missing {want:?} in\n{out}"
        );
    }
    // The snippet of the explained syntax error, once.
    assert_eq!(
        out.matches("{{ get_page(path=\"/posts\").title }}").count(),
        1,
        "{out}"
    );
    // Nothing else: the sorted `for … | sort_keys` loop, the hook's fields and the clean
    // templates give no diagnostic.
    let diagnostics = out
        .lines()
        .filter(|l| l.starts_with("ERROR") || l.starts_with("WARN"))
        .count();
    assert_eq!(diagnostics, expected.len(), "{out}");
    assert!(out.ends_with("11 error(s), 5 warning(s)\n"), "{out}");
    // Coverage.
    for want in [
        "  home.html + baseof.html: 1",
        "  posts/list.html + baseof.html: 1",
        "  en 404 /404 [404] → (no template: not written)",
        "  missing → NO MATCH (no template for shortcode \"missing\"): 1 page(s)",
        "  note → _shortcodes/note.html: 1 page(s)",
        "  link → _markup/render-link.html: 2 page(s)",
    ] {
        assert!(out.lines().any(|l| l == want), "missing {want:?} in\n{out}");
    }
}

/// The converted testsite layouts are clean; `--coverage full` lists every (page, format).
#[test]
fn testsite_overlay_is_clean() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let site = tmp.path().join("testsite");
    testsite(&site);
    let o = binary(tmp.path(), &["templates", "check", "-s", "testsite"], &[]);
    let out = stdout(&o);
    assert_eq!(o.status.code(), Some(0), "{out}");
    assert!(out.ends_with("0 error(s), 0 warning(s)\n"), "{out}");
    assert!(
        out.contains("layout lookups: 23 pages, 35 (page, format) queries"),
        "{out}"
    );
    assert!(out.contains("  single.html: 7\n"), "{out}");

    let o = binary(
        tmp.path(),
        &["templates", "check", "-s", "testsite", "--coverage", "full"],
        &[],
    );
    let out = stdout(&o);
    let rows = out.lines().filter(|l| l.contains("] → ")).count();
    assert_eq!(rows, 35, "{out}");
    assert!(out.contains("  en home / [html] → list.html\n"), "{out}");

    let o = binary(
        tmp.path(),
        &["templates", "check", "-s", "testsite", "--coverage", "none"],
        &[],
    );
    assert!(!stdout(&o).contains("layout lookups"), "{}", stdout(&o));
}

/// `--deny-warnings` fails on warnings alone.
#[test]
fn deny_warnings() {
    let s = crate::site_from(
        "-- config.toml --\nbaseURL = \"https://e.org/\"\ndisableKinds = [\"taxonomy\", \"term\"]\n-- layouts/home.html --\n{% if page.params.x == none %}x{% endif %}\n",
    );
    let o = binary(s.path(), &["templates", "check"], &[]);
    assert_eq!(o.status.code(), Some(0), "{}", stdout(&o));
    assert!(
        stdout(&o).ends_with("0 error(s), 1 warning(s)\n"),
        "{}",
        stdout(&o)
    );
    let o = binary(s.path(), &["templates", "check", "--deny-warnings"], &[]);
    assert_eq!(o.status.code(), Some(1), "{}", stdout(&o));
}
