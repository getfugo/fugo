//! Content adapters: `add_page`, `add_resource`, `enable_all_languages` and the adapter's store
//! in a render of phase `Adapter`; none of them outside one.

use ssg_base::paths::ContentKey;
use ssg_base::{LangIdx, PageKind};
use ssg_page::Markup;
use ssg_site::AddedContent;
use ssg_view::{Phase, RenderScope};

use crate::support;

#[test]
fn adapter_functions_collect_the_run() {
    let site = support::load();
    let adapters = &site.handles.adapters;
    let run = adapters.begin(0, LangIdx::from_raw(0), ContentKey::from_source("news"));
    let s = RenderScope {
        phase: Phase::Adapter,
        adapter: Some(run),
        ..site.home_scope()
    };
    let out = site.render(
        r#"{{ enable_all_languages() }}{{ store_set(key="k", value=1) }}{{ add_page(page={"path": "A B", "title": "T", "content": {"mediaType": "text/html", "value": "<b>x</b>"} }) }}{{ add_resource(resource={"path": "a-b/data.json", "content": {"value": "{}"}, "name": "d", "params": {"X": 1} }) }}{{ add_resource(resource={"path": "a-b/logo.png", "content": {"value": get_asset(path="img/logo.png")} }) }}{{ store_get(key="k") }}"#,
        &s,
    );
    assert_eq!(
        out, "1",
        "the functions print nothing; the store is the adapter's"
    );
    let r = adapters.run(run);
    assert!(r.all_languages);
    assert_eq!(r.pages.len(), 1);
    let p = &r.pages[0].page;
    assert_eq!(
        (p.kind, p.path.as_str(), p.markup, p.content.as_str()),
        (PageKind::Page, "news/a-b", Markup::Html, "<b>x</b>")
    );
    assert_eq!(r.resources.len(), 2);
    let data = &r.resources[0];
    assert_eq!(data.path, "news/a-b/data.json");
    assert_eq!(data.name.as_deref(), Some("d"));
    assert!(data.params.get("x").is_some());
    assert!(
        matches!(&data.content, AddedContent::Text { text, media_type: None } if &**text == "{}")
    );
    let logo = &r.resources[1];
    let AddedContent::Resource {
        link, media_type, ..
    } = &logo.content
    else {
        panic!("a resource value keeps the resource: {:?}", logo.content);
    };
    assert_eq!(
        (link.as_str(), media_type.as_str()),
        ("/img/logo.png", "image/png")
    );

    // The page store is untouched; another run of the same adapter shares its store.
    let home = site.home_scope();
    assert_eq!(site.render(r#"{{ store_get(key="k") }}"#, &home), "");
    let run2 = adapters.begin(0, LangIdx::from_raw(0), ContentKey::from_source("news"));
    let s2 = RenderScope {
        adapter: Some(run2),
        ..s.clone()
    };
    assert_eq!(site.render(r#"{{ store_get(key="k") }}"#, &s2), "1");

    // Errors at the call.
    let e = site
        .try_render(r#"{{ add_page(page={"title": "no path"}) }}"#, &s)
        .expect_err("no path");
    assert!(e.to_string().contains("`path` is empty"), "{e}");
    let e = site
        .try_render(
            r#"{{ add_resource(resource={"content": {"value": "x"} }) }}"#,
            &s,
        )
        .expect_err("no path");
    assert!(e.to_string().contains("`path` is required"), "{e}");
}

#[test]
fn adapter_functions_only_in_adapters() {
    let site = support::load();
    let s = site.home_scope();
    for call in [
        r#"{{ add_page(page={"path": "x"}) }}"#,
        r#"{{ add_resource(resource={"path": "x/y.txt"}) }}"#,
        "{{ enable_all_languages() }}",
    ] {
        let e = site.try_render(call, &s).expect_err(call);
        assert!(
            e.to_string().contains("only available in content adapters"),
            "{call}: {e}"
        );
    }
}

/// A run that adds a path again replaces what it added there, in place (Go inserts into its
/// trees: the last insert wins), and warns.
#[test]
fn a_path_added_again_replaces() {
    let site = support::load();
    let adapters = &site.handles.adapters;
    let run = adapters.begin(0, LangIdx::from_raw(0), ContentKey::from_source("news"));
    let s = RenderScope {
        phase: Phase::Adapter,
        adapter: Some(run),
        ..site.home_scope()
    };
    let mark = site.handles.diagnostics.len();
    site.render(
        r#"{{ add_page(page={"path": "a", "title": "one"}) }}{{ add_page(page={"path": "b"}) }}{{ add_page(page={"path": "A", "title": "two", "kind": "section"}) }}{{ add_resource(resource={"path": "a/r.txt", "content": {"value": "1"} }) }}{{ add_resource(resource={"path": "a/r.txt", "content": {"value": "2"} }) }}"#,
        &s,
    );
    let r = adapters.run(run);
    let pages: Vec<_> = r
        .pages
        .iter()
        .map(|p| (p.page.path.as_str(), p.page.kind))
        .collect();
    assert_eq!(
        pages,
        [("news/a", PageKind::Section), ("news/b", PageKind::Page)]
    );
    assert_eq!(
        r.pages[0].page.fields.get("title"),
        Some(&ssg_base::Value::string("two"))
    );
    assert_eq!(r.resources.len(), 1);
    assert!(matches!(&r.resources[0].content, AddedContent::Text { text, .. } if &**text == "2"));
    let ids: Vec<Option<String>> = site
        .handles
        .diagnostics
        .since(mark)
        .into_iter()
        .map(|d| d.id)
        .collect();
    assert_eq!(
        ids,
        [
            Some("duplicate-content-path".to_owned()),
            Some("duplicate-resource-path".to_owned())
        ]
    );

    // `take` moves the results out; the run then adds to empty lists, without warnings.
    let taken = adapters.take(run);
    assert_eq!((taken.pages.len(), taken.resources.len()), (2, 1));
    let r = adapters.run(run);
    assert!(r.pages.is_empty() && r.resources.is_empty());
    let mark = site.handles.diagnostics.len();
    site.render(r#"{{ add_page(page={"path": "a"}) }}"#, &s);
    assert_eq!(adapters.run(run).pages.len(), 1);
    assert!(site.handles.diagnostics.since(mark).is_empty());
}

/// Paths are decoded as Go does: a page path loses one leading `/` (`PageConfig.Init`), a
/// resource path none (`ResourceConfig.Compile`); neither is trimmed (spaces become `-`). A
/// `kind` must be a content kind as written, in lower case.
#[test]
fn paths_and_kinds_as_written() {
    let site = support::load();
    let adapters = &site.handles.adapters;
    let run = adapters.begin(0, LangIdx::from_raw(0), ContentKey::from_source("news"));
    let s = RenderScope {
        phase: Phase::Adapter,
        adapter: Some(run),
        ..site.home_scope()
    };
    site.render(
        r#"{{ add_page(page={"path": " spaced "}) }}{{ add_page(page={"path": "//double"}) }}{{ add_page(page={"path": "/one"}) }}{{ add_resource(resource={"path": " lead.txt", "content": {"value": "l"} }) }}{{ add_resource(resource={"path": "/abs/x.txt", "content": {"value": "a"} }) }}"#,
        &s,
    );
    let r = adapters.run(run);
    let pages: Vec<_> = r.pages.iter().map(|p| p.page.path.as_str()).collect();
    assert_eq!(pages, ["news/-spaced-", "news/double", "news/one"]);
    let resources: Vec<_> = r.resources.iter().map(|r| r.path.as_str()).collect();
    assert_eq!(resources, ["news/-lead.txt", "news/abs/x.txt"]);
    for kind in ["PAGE", "Section", "taxonomyTerm", " page", "404"] {
        let call = format!(r#"{{{{ add_page(page={{"path": "k", "kind": "{kind}"}}) }}}}"#);
        let e = site.try_render(&call, &s).expect_err(kind);
        assert!(e.to_string().contains("is not one of page"), "{kind}: {e}");
    }
}
