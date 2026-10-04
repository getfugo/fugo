use super::*;

fn js(loader: CssLoader, source: &str) -> String {
    css_module_js(
        loader,
        source,
        Path::new("/site/assets/a.module.css"),
        &mut CssNames::default(),
    )
    .expect("css_module_js")
}

fn err(source: &str) -> LowerError {
    css_module_js(
        CssLoader::LocalCss,
        source,
        Path::new("/site/assets/a.module.css"),
        &mut CssNames::default(),
    )
    .expect_err("an error")
}

#[test]
fn plain_css_is_an_empty_object() {
    assert_eq!(
        js(CssLoader::Css, ".a { composes: b } :local(.c) {}"),
        EMPTY_MODULE
    );
    assert_eq!(js(CssLoader::GlobalCss, ".a {} #b {}"), EMPTY_MODULE);
    assert_eq!(js(CssLoader::LocalCss, ":global(.a) {}"), EMPTY_MODULE);
}

#[test]
fn local_names_in_order_of_appearance() {
    let out = js(
        CssLoader::LocalCss,
        ".btn:hover .icon {} #main {} .my-class {} .default {} @keyframes spin {}",
    );
    assert_eq!(
        out,
        "var c0 = \"a_btn\";\nvar c1 = \"a_icon\";\nvar c2 = \"a_main\";\n\
         var c3 = \"a_my-class\";\nvar c4 = \"a_default\";\nvar c5 = \"a_spin\";\n\
         export default { \"btn\": c0, \"icon\": c1, \"main\": c2, \"my-class\": c3, \
         \"default\": c4, \"spin\": c5 };\n\
         export { c0 as btn, c1 as icon, c2 as main, c3 as \"my-class\", c5 as spin };\n"
    );
}

#[test]
fn global_and_local() {
    let out = js(
        CssLoader::LocalCss,
        ":global(.g) .l {} :global(.h .i) {} .j:not(:global(.k)) {}",
    );
    assert!(
        out.contains("export default { \"l\": c0, \"j\": c1 };"),
        "{out}"
    );
    let out = js(
        CssLoader::GlobalCss,
        ".g :local(.l) { animation: k } :local(#m) {}",
    );
    assert!(
        out.contains("export default { \"l\": c0, \"m\": c1 };"),
        "{out}"
    );
}

#[test]
fn keyframes_and_animations() {
    let out = js(
        CssLoader::LocalCss,
        ".a { animation: fade 1s ease-in infinite, none 2s } @keyframes \"slide\" {} \
         .b { animation-name: inherit, grow; -webkit-animation: prefixed 1s } \
         .c { animation: spin var(--t) linear }",
    );
    assert!(
        out.contains(
            "export default { \"a\": c0, \"fade\": c1, \"slide\": c2, \"b\": c3, \
             \"grow\": c4, \"c\": c5, \"spin\": c6 };"
        ),
        "{out}"
    );
}

#[test]
fn composes_in_the_same_file() {
    let out = js(
        CssLoader::LocalCss,
        ".a { composes: b c } .b { composes: c } .c {} .d { composes: g from global } \
         .e .f { composes: c }",
    );
    assert!(out.contains("var c0 = \"a_c a_b a_a\";"), "{out}");
    assert!(out.contains("var c1 = \"a_c a_b\";"), "{out}");
    assert!(out.contains("var c3 = \"g a_d\";"), "{out}");
    // Not a single class selector: ignored, like esbuild (which warns).
    assert!(out.contains("var c5 = \"a_f\";"), "{out}");
}

#[test]
fn composes_from_another_file() {
    let out = js(
        CssLoader::LocalCss,
        ".a { composes: x my-y from \"./b.module.css\"; composes: c } .c {} \
         .d { composes: z from 'https://example.com/c.css' }",
    );
    assert!(out.starts_with(
        "import { x as i0 } from \"./b.module.css\";\n\
         import { \"my-y\" as i1 } from \"./b.module.css\";\n"
    ));
    assert!(
        out.contains("var c0 = __ssg_compose(\"a_a\", [i0, i1, \"a_c\"]);"),
        "{out}"
    );
    assert!(out.contains("var c2 = \"a_d\";"), "{out}");
    assert!(out.contains("function __ssg_compose(own, parts)"), "{out}");
}

#[test]
fn file_identifiers() {
    for (path, ident) in [
        ("/x/a.module.css", "a"),
        ("/x/my-comp.module.css", "my_comp"),
        ("/x/1x.css", "x"),
        ("/x/a.b.c.css", "a_b_c"),
        ("/x/a.module.module.css", "a_module"),
        ("/x/comp/index.module.css", "comp"),
        ("/x/v1.2/index.css", "v1"),
        ("/x/é.css", "_"),
        ("/x/$d.css", "d"),
    ] {
        assert_eq!(file_identifier(Path::new(path)), ident, "{path}");
    }
}

#[test]
fn names_are_unique_per_build() {
    let mut names = CssNames::default();
    let a = Path::new("/site/x/s.module.css");
    let b = Path::new("/site/y/s.module.css");
    css_module_js(CssLoader::LocalCss, ".root {}", a, &mut names).expect("first");
    css_module_js(CssLoader::LocalCss, ".root {}", a, &mut names).expect("same file again");
    css_module_js(CssLoader::LocalCss, ".other {}", b, &mut names).expect("other names");
    let e = css_module_js(CssLoader::LocalCss, "\n  .other, .root {}", b, &mut names)
        .expect_err("collision");
    assert_eq!((e.line, e.column), (2, 2));
    assert!(e.message.starts_with(
        "the CSS-module name \"s_root\" would be generated for both /site/x/s.module.css \
         and /site/y/s.module.css"
    ));
}

#[test]
fn bare_global_is_an_error_with_its_position() {
    let e = err(".a {}\n.日本 :global .c {}");
    assert_eq!((e.line, e.column), (2, 9));
    assert!(e.message.starts_with("a bare \":global\""), "{e:?}");
}

#[test]
fn strings_and_export_names() {
    assert_eq!(
        js_string("a\"b\\c\n\u{2028}\u{1}"),
        "\"a\\\"b\\\\c\\n\\u2028\\x01\""
    );
    assert_eq!(export_name("btn"), "btn");
    assert_eq!(export_name("class"), "class");
    assert_eq!(export_name("my-x"), "\"my-x\"");
    assert_eq!(export_name("日本"), "\"日本\"");
}
