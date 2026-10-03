//! Assets, remote resources, named targets, pipes, content, publishing, post-processing,
//! `execute_as_template`, `unmarshal`.

use ssg_base::PageKind;
use ssg_base::diag::Severity;

use crate::support;

#[test]
fn assets_and_named_targets() {
    let site = support::load();
    let s = site.home_scope();
    let r = |src: &str| site.render(src, &s);
    assert_eq!(
        r(
            "{% set a = get_asset(path=\"css/a.css\") %}{{ a.rel_permalink }}|{{ a.media_type.type }}|{{ a.name }}"
        ),
        "/sub/css/a.css|text/css|/css/a.css"
    );
    assert_eq!(r("{{ get_asset(path=\"css/none.css\") is none }}"), "true");
    assert_eq!(
        r("{{ find_asset(pattern=\"css/*.css\") | get_path(path=[\"rel_permalink\"]) }}"),
        "/sub/css/a.css"
    );
    assert_eq!(r("{{ find_assets(pattern=\"**.css\") | length }}"), "2");
    assert_eq!(
        r(
            "{{ concat_assets(target=\"css/all.css\", items=find_assets(pattern=\"css/*.css\")) | resource_content }}"
        ),
        "body { color: red; }\np { margin: 0; }\n"
    );
    assert_eq!(
        r(
            "{% set x = asset_from_string(target=\"t/x.txt\", content=\"hi\") %}{{ x.rel_permalink }}={{ x | resource_content }}"
        ),
        "/sub/t/x.txt=hi"
    );
    // A second claim of the target with other content in the same language is an error.
    let e = site
        .try_render(
            "{{ asset_from_string(target=\"t/x.txt\", content=\"other\") }}",
            &s,
        )
        .expect_err("target conflict");
    assert!(e.to_string().contains("created twice"), "{e}");
}

#[test]
fn pipes_publish_and_post_process() {
    let site = support::load();
    let s = site.home_scope();
    let r = |src: &str| site.render(src, &s);
    let fp = r(
        "{% set f = get_asset(path=\"css/a.css\") | fingerprint %}{{ f.rel_permalink }}|{{ f.data.integrity }}",
    );
    assert!(
        fp.starts_with("/sub/css/a.") && fp.contains(".css|sha256-"),
        "{fp}"
    );
    let md5 = r(
        "{{ get_asset(path=\"css/a.css\") | fingerprint(algo=\"md5\") | get_path(path=[\"data\", \"integrity\"]) }}",
    );
    assert!(md5.starts_with("md5-"), "{md5}");
    let e = site
        .try_render(
            "{{ get_asset(path=\"css/a.css\") | fingerprint(algo=\"crc\") }}",
            &s,
        )
        .expect_err("bad algo");
    assert!(e.to_string().contains("unsupported hash algorithm"), "{e}");
    assert_eq!(
        r("{{ get_asset(path=\"css/a.css\") | minify | get_path(path=[\"rel_permalink\"]) }}"),
        "/sub/css/a.min.css"
    );
    assert_eq!(
        r(
            "{{ get_asset(path=\"css/a.css\") | to_css(options={\"targetPath\": \"out/x.css\"}) | get_path(path=[\"rel_permalink\"]) }}"
        ),
        "/sub/out/x.css"
    );
    let e = site
        .try_render(
            "{{ get_asset(path=\"css/a.css\") | to_css(options={\"vars\": 3}) }}",
            &s,
        )
        .expect_err("bad option");
    assert!(e.to_string().contains("to_css(options=)"), "{e}");
    // post_process: every late field is a placeholder; resource_content is the content one.
    let pp = r(
        "{% set p = get_asset(path=\"css/b.css\") | minify | post_process %}{{ p.rel_permalink }}|{{ p.media_type.type }}|{{ p | resource_content }}",
    );
    let parts: Vec<&str> = pp.split('|').collect();
    assert!(
        parts[0].starts_with("__nh_pp_") && parts[0].ends_with("_rel_permalink__"),
        "{pp}"
    );
    assert!(parts[1].ends_with("_media_type__"), "{pp}");
    assert!(parts[2].ends_with("_content__"), "{pp}");
    let resolved = site
        .store
        .resolve_post_process(&pp)
        .expect("resolve")
        .expect("placeholders");
    assert_eq!(resolved, "/sub/css/b.min.css|text/css|p{margin:0}");
    // publish marks the resource.
    assert_eq!(
        r("{{ get_asset(path=\"css/b.css\") | publish | get_path(path=[\"rel_permalink\"]) }}"),
        "/sub/css/b.css"
    );
    let e = site
        .try_render("{{ get_page(path=\"/\") | minify }}", &s)
        .expect_err("a page is not a resource");
    assert!(
        e.to_string().contains("expected a resource, got a page"),
        "{e}"
    );
}

#[test]
fn resource_content_of_files_and_bundled_pages() {
    let site = support::load();
    let bundle = site.scope(site.page(PageKind::Page, "/posts/bundle", 0), None);
    assert_eq!(
        site.render(
            "{% for r in page.resources %}{{ r.name }}={% if r.resource_type == \"image\" %}img{% else %}{{ r | resource_content | trim }}{% endif %};{% endfor %}",
            &bundle
        ),
        "pic.png=img;data.txt=bundle data;notes.md=<p>content of Notes</p>;"
    );
}

#[test]
fn execute_as_template_renders_with_the_build_instance() {
    let site = support::load();
    let one = site.page(PageKind::Page, "/posts/one", 0);
    let s = site.scope(one, None);
    let out = site.render(
        "{% set t = get_asset(path=\"tpl/greet.txt\") | execute_as_template(target=\"out/greet.txt\", data={\"name\": \"Ann\"}) %}{{ t.rel_permalink }}={{ t | resource_content }}",
        &s,
    );
    assert_eq!(
        out,
        format!("/sub/out/greet.txt=Hello Ann from Funcs ({one})")
    );
}

#[test]
fn get_remote_errors_and_optional() {
    let site = support::load();
    let s = site.home_scope();
    // The network is off and the cache is empty: an error...
    let e = site
        .try_render("{{ get_remote(url=\"https://example.org/x.json\") }}", &s)
        .expect_err("offline");
    assert!(e.to_string().contains("network is disabled"), "{e}");
    let e = site
        .try_render("{{ get_remote(url=\"ftp://example.org/x\") }}", &s)
        .expect_err("scheme");
    assert!(e.to_string().contains("only http and https"), "{e}");
    // ...unless optional: none and a warning.
    assert_eq!(
        site.render(
            "{{ get_remote(url=\"https://example.org/x.json\", options={\"headers\": {\"A\": \"b\"} }, optional=true) is none }}",
            &s
        ),
        "true"
    );
    let report = site.handles.diagnostics.report();
    assert!(
        report
            .iter()
            .any(|d| d.severity == Severity::Warning && d.message.contains("x.json")),
        "{report:?}"
    );
    let e = site
        .try_render(
            "{{ get_remote(url=\"https://example.org/\", options=3) }}",
            &s,
        )
        .expect_err("options");
    assert!(e.to_string().contains("expected a map"), "{e}");
}

#[test]
fn unmarshal_strings_and_resources() {
    let site = support::load();
    let s = site.home_scope();
    let r = |src: &str| site.render(src, &s);
    assert_eq!(
        r("{{ (get_asset(path=\"data/x.json\") | unmarshal) | jsonify }}"),
        "{\"a\":[1,2],\"b\":2}"
    );
    assert_eq!(
        r("{{ get_asset(path=\"data/rows.csv\") | unmarshal | get_path(path=[1, 0]) }}"),
        "1"
    );
    assert_eq!(
        r("{{ \"a = 1\" | unmarshal | get_path(path=[\"a\"]) }}"),
        "1"
    );
    assert_eq!(
        r("{{ \"a: [x, z]\" | unmarshal | get_path(path=[\"a\", 1]) }}"),
        "z"
    );
    assert_eq!(
        r("{{ \"<r k=\\\"v\\\"><i>1</i><i>2</i></r>\" | unmarshal | get_path(path=[\"-k\"]) }}"),
        "v"
    );
    assert_eq!(
        r("{{ \"x;y\" | unmarshal(format=\"csv\") | get_path(path=[0, 0]) }}"),
        "x;y"
    );
    assert_eq!(r("{{ \"\" | unmarshal | length }}"), "0");
    let e = site
        .try_render("{{ \"x\" | unmarshal(format=\"ini\") }}", &s)
        .expect_err("format");
    assert!(
        e.to_string()
            .contains("expected json, toml, yaml, csv or xml"),
        "{e}"
    );
}
