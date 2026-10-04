use super::*;

fn page(tags: &[&str], classes: &[&str], ids: &[&str], words: &[&str]) -> PageNames {
    let set = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect();
    PageNames {
        tags: set(tags),
        classes: set(classes),
        ids: set(ids),
        words: set(words),
    }
}

/// Printed for Safari 12 (no media query ranges).
fn plan(css: &str, options: &PurgeOptions, content: &[&str]) -> PurgePlan {
    let targets = Targets {
        browsers: Some(lightningcss::targets::Browsers {
            safari: Some(12 << 16),
            ..Default::default()
        }),
        ..Targets::default()
    };
    PurgePlan::compile(css, options, content, targets).expect("compile")
}

#[test]
fn selectors_need_their_names() {
    let p = plan(
        "body{margin:0}.a,.b{color:red}.a .c{color:blue}#x{top:0}p.a{left:0}\
         .n:not(.zz){x:y}:is(.a,.q) span{z:1}a:hover{color:green}[data-x]{w:1}",
        &PurgeOptions::default(),
        &[],
    );
    let out = plan_out(&p, &page(&["body", "p"], &["a", "n"], &[], &[]));
    assert_eq!(
        out,
        "body{margin:0}.a{color:red}p.a{left:0}.n:not(.zz){x:y}[data-x]{w:1}"
    );
    let out = plan_out(&p, &page(&["span", "a"], &["a", "b", "c"], &["x"], &[]));
    assert_eq!(
        out,
        ".a,.b{color:red}.a .c{color:#00f}#x{top:0}:is(.a,.q) span{z:1}a:hover{color:green}[data-x]{w:1}"
    );
}

fn plan_out(p: &PurgePlan, names: &PageNames) -> String {
    p.purge(names)
}

#[test]
fn groups_and_other_rules() {
    let css = "@charset \"utf-8\";@font-face{font-family:F;src:url(f.woff2)}\
               @media (min-width:768px){.a{color:red}.b{color:blue}}\
               @supports (display:grid){.b{display:grid}}\
               @keyframes k{0%{opacity:0}to{opacity:1}}";
    let p = plan(css, &PurgeOptions::default(), &[]);
    let out = p.purge(&page(&[], &["a"], &[], &[]));
    assert_eq!(
        out,
        "@font-face{font-family:F;src:url(f.woff2)}@media (min-width:768px){.a{color:red}}\
         @keyframes k{0%{opacity:0}to{opacity:1}}"
    );
}

#[test]
fn safelist_content_greedy_blocklist() {
    let options = PurgeOptions {
        safelist: vec!["s".into(), "/^re-/".into()],
        greedy: vec!["/bs-dark/".into()],
        blocklist: vec!["/^no-/".into()],
        ..PurgeOptions::default()
    };
    let p = plan(
        ".s{a:1}.re-x{a:2}.shown{a:3}.bs-dark .zz{a:4}.no-x{a:5}.other{a:6}",
        &options,
        &["el.classList.add('shown')"],
    );
    assert_eq!(
        p.purge(&page(&[], &["no-x"], &[], &[])),
        ".s{a:1}.re-x{a:2}.shown{a:3}.bs-dark .zz{a:4}"
    );
    // Script words of the page count too.
    assert!(
        p.purge(&page(&[], &[], &[], &["other"]))
            .contains(".other{a:6}")
    );
}

#[test]
fn unused_variables() {
    let css = ":root{--a:1px;--b:var(--a);--c:2px;--d:3px;--e:4px}.x{margin:var(--b)}\
               .y{padding:var(--c)}@font-face{font-family:F;font-weight:var(--e)}";
    let options = PurgeOptions {
        variables: true,
        ..PurgeOptions::default()
    };
    let p = plan(css, &options, &[]);
    let out = p.purge(&page(&[], &["x"], &[], &["--d"]));
    assert!(
        out.starts_with(":root{--a:1px;--b:var(--a);--d:3px;--e:4px}"),
        "{out}"
    );
    assert!(!out.contains("--c"), "{out}");
    // Without the option every declaration stays.
    let p = plan(css, &PurgeOptions::default(), &[]);
    assert!(p.purge(&page(&[], &["x"], &[], &[])).contains("--c:2px"));
}

#[test]
fn important_kept_or_dropped() {
    let css = ".a{color:red!important;margin:0}";
    let p = plan(css, &PurgeOptions::default(), &[]);
    assert_eq!(
        p.purge(&page(&[], &["a"], &[], &[])),
        ".a{margin:0;color:red!important}"
    );
    let options = PurgeOptions {
        drop_important: true,
        ..PurgeOptions::default()
    };
    let p = plan(css, &options, &[]);
    assert_eq!(
        p.purge(&page(&[], &["a"], &[], &[])),
        ".a{margin:0;color:red}"
    );
}

#[test]
fn fallback_declarations_survive() {
    let css = ".i{background:radial-gradient(circle at 30% 107%,#fdf497 0%,#285aeb 90%);\
               background:-webkit-radial-gradient(circle at 30% 107%,#fdf497 0%,#285aeb 90%)}";
    let p = plan(css, &PurgeOptions::default(), &[]);
    let out = p.purge(&page(&[], &["i"], &[], &[]));
    assert!(
        out.contains("background:radial-gradient(")
            && out.contains("background:-webkit-radial-gradient("),
        "{out}"
    );
}

#[test]
fn byte_order_mark_is_dropped() {
    let p = plan("\u{feff}:root{--a:1}.b{x:1}", &PurgeOptions::default(), &[]);
    assert_eq!(p.purge(&page(&[], &[], &[], &[])), ":root{--a:1}");
}

#[test]
fn rejected_rules_kept_as_written() {
    let p = plan(
        ".a{x:1}.b::before.c{content:\"x\"}@media (min-width:1px){.d{y:2}}",
        &PurgeOptions::default(),
        &[],
    );
    assert_eq!(
        p.purge(&page(&[], &["a"], &[], &[])),
        ".a{x:1}.b::before.c{content:\"x\"}"
    );
}

#[test]
fn registry_placeholders() {
    let purges = CssPurges::default();
    let compile = || {
        PurgePlan::compile(
            ".a{x:1}.b{x:2}",
            &PurgeOptions::default(),
            &[],
            Targets::default(),
        )
    };
    let ph = purges.placeholder(7, compile).unwrap();
    assert_eq!(ph, "__nh_purge_0__");
    assert_eq!(purges.placeholder(7, || unreachable!()).unwrap(), ph);
    let html = format!("<style>{ph}</style><p class=b>");
    let out = purges
        .resolve(&html, || page(&[], &["b"], &[], &[]))
        .unwrap()
        .unwrap();
    assert_eq!(out, "<style>.b{x:2}</style><p class=b>");
    assert!(purges.resolve("<p>", PageNames::default).unwrap().is_none());
    assert!(
        purges
            .resolve("__nh_purge_9__", PageNames::default)
            .is_err()
    );
}

#[test]
fn word_splitting() {
    let w: Vec<&str> = words("add('md:flex', \"w-1/2\"); --bs-x").collect();
    assert_eq!(
        w,
        [
            "add", "md:flex", "md", "flex", "w-1/2", "w-1", "2", "--bs-x"
        ]
    );
}
