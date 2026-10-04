//! The values of pages and the site: relations, resources, recursive page lists and image sizes.

use super::*;

#[test]
fn relations_and_site_values() {
    let s = load(FILES);
    freeze(&s);
    let m = &s.model;
    let g = s.views.generation(Phase::Layout, HookVariant::Html);
    let str_of = |v: &tera::Value, k: &str| get(v, k).as_str().unwrap_or_default().to_owned();
    let titles = |v: &tera::Value| -> Vec<String> {
        v.as_array()
            .expect("list")
            .iter()
            .map(|p| str_of(p, "title"))
            .collect()
    };
    let one = g.full(page(m, PageKind::Page, "/posts/one", 0));
    assert_eq!(str_of(get(&one, "parent"), "title"), "Posts");
    assert_eq!(titles(get(&one, "translations")), ["Un"]);
    assert_eq!(titles(get(&one, "all_translations")), ["One", "Un"]);
    assert_eq!(str_of(get(&one, "file"), "path"), "posts/one.md");
    assert_eq!(str_of(get(&one, "file"), "dir"), "posts/");
    assert_eq!(
        str_of(get(&one, "date"), "rfc3339"),
        "2021-02-01T10:00:00+00:00"
    );
    assert_eq!(
        titles(get(get(&one, "terms"), "tags"))
            .into_iter()
            .collect::<Vec<_>>(),
        ["A", "B"]
    );
    // Newest first: bundle, two, one. Prev is the older neighbour.
    let two = g.full(page(m, PageKind::Page, "/posts/two", 0));
    assert_eq!(str_of(get(&two, "prev_in_section"), "title"), "One");
    assert_eq!(str_of(get(&two, "next_in_section"), "title"), "Bundle");
    assert_eq!(str_of(get(&two, "prev"), "title"), "One");
    let posts = g.full(page(m, PageKind::Section, "/posts", 0));
    assert_eq!(
        titles(get(&posts, "regular_pages")),
        ["Bundle", "Two", "One"]
    );
    assert_eq!(
        titles(get(&posts, "regular_pages_recursive")),
        ["Bundle", "Two", "One"]
    );
    let home = g.full(page(m, PageKind::Home, "/", 0));
    let alternatives: Vec<String> = get(&home, "alternative_output_formats")
        .as_array()
        .expect("list")
        .iter()
        .map(|f| str_of(f, "name"))
        .collect();
    // `headers` is `notAlternative`.
    assert_eq!(alternatives, ["rss", "json"]);
    let tags = g.full(page(m, PageKind::Taxonomy, "/tags", 0));
    let taxonomy = get(&tags, "taxonomy");
    assert_eq!(str_of(taxonomy, "plural"), "tags");
    let terms: Vec<(String, u64)> = get(taxonomy, "terms")
        .as_array()
        .expect("terms")
        .iter()
        .map(|t| (str_of(t, "key"), get(t, "count").as_u64().expect("count")))
        .collect();
    assert_eq!(terms, [("a".to_owned(), 2), ("b".to_owned(), 1)]);
    assert!(get(&tags, "term").is_none());
    let a = g.full(page(m, PageKind::Term, "/tags/a", 0));
    assert_eq!(str_of(get(&a, "term"), "singular"), "tag");
    assert_eq!(str_of(get(&a, "term"), "key"), "a");
    assert!(get(&a, "taxonomy").is_none());

    let en = &g.sites[ssg_base::LangIdx::from_index(0)];
    assert_eq!(str_of(en, "language_code"), "en");
    assert_eq!(str_of(get(en, "language"), "name"), "English");
    assert_eq!(get(en, "languages").as_array().map(<[_]>::len), Some(2));
    assert_eq!(get(en, "is_multilingual").as_bool(), Some(true));
    assert_eq!(str_of(get(get(en, "data"), "team"), "Lead"), "Ann");
    assert_eq!(
        str_of(en, "sitemap_abs_url"),
        "https://example.org/en/sitemap.xml"
    );
    let main = get(get(en, "menus"), "main").as_array().expect("main");
    assert_eq!(str_of(&main[0], "name"), "Posts");
    assert_eq!(str_of(&main[0], "url"), "/posts/");
    assert_eq!(str_of(get(&main[0], "page"), "path"), "/posts");
    let child = &get(&main[0], "children").as_array().expect("children")[0];
    assert_eq!(str_of(child, "name"), "One");
    assert!(get(child, "pre").is_safe());
    assert_eq!(get(&main[0], "has_children").as_bool(), Some(true));
    let fr = &g.sites[ssg_base::LangIdx::from_index(1)];
    assert_eq!(titles(get(fr, "regular_pages")), ["Un"]);
    assert_eq!(
        get(fr, "all_pages").as_array().map(<[_]>::len),
        get(en, "all_pages").as_array().map(<[_]>::len)
    );
}

#[test]
fn resource_values() {
    let s = load(FILES);
    let m = &s.model;
    let g = s.views.meta();
    let bundle = page(m, PageKind::Page, "/posts/bundle", 0);
    let rs = get(&g.summaries[bundle], "resources")
        .as_array()
        .expect("resources")
        .to_vec();
    let field = |r: &tera::Value, k: &str| get(r, k).as_str().unwrap_or_default().to_owned();
    let names: Vec<String> = rs.iter().map(|r| field(r, "name")).collect();
    assert_eq!(names, ["cover", "data.txt", "notes.md"]);
    // Metadata applied; the store holds the renamed resource under the value's id.
    assert_eq!(field(&rs[0], "title"), "Cover");
    assert_eq!(field(get(&rs[0], "params"), "credit"), "me");
    assert_eq!(field(&rs[0], "rel_permalink"), "/posts/bundle/img.png");
    assert_eq!(field(&rs[0], "resource_type"), "image");
    assert_eq!(field(get(&rs[0], "media_type"), "type"), "image/png");
    let rid = |r: &tera::Value| {
        ssg_base::ResourceId::from_raw(
            u32::try_from(get(r, "__rid").as_u64().expect("rid")).expect("u32"),
        )
    };
    assert_eq!(s.store.resource(rid(&rs[0])).name, "cover");
    assert_eq!(
        s.views.resources(bundle),
        rs.iter().map(rid).collect::<Vec<_>>()
    );
    // The bundled content page is a `page` resource pointing at its page.
    assert_eq!(field(&rs[2], "resource_type"), "page");
    assert_eq!(
        field(get(&rs[2], "media_type"), "type"),
        "application/octet-stream"
    );
    assert_eq!(field(&rs[2], "title"), "Notes");
    assert_eq!(field(&rs[2], "rel_permalink"), "");
    let notes = get(&rs[2], "page_id").as_u64().expect("page id");
    assert_eq!(
        m.pages[PageId::from_raw(u32::try_from(notes).expect("u32"))].title,
        "Notes"
    );
    // Translations share the files of their bundle directory (none here), other pages have
    // no resources.
    let one = page(m, PageKind::Page, "/posts/one", 0);
    assert_eq!(
        get(&g.summaries[one], "resources")
            .as_array()
            .map(<[_]>::len),
        Some(0)
    );

    // A fingerprint of a pending result: placeholders for the links and the integrity.
    let call = CallSite::in_lang(ssg_base::LangIdx::from_index(0));
    let css = s
        .store
        .from_string("css/a.css", "body { color: red }", &call)
        .expect("css");
    let done = s
        .store
        .transform(css, Transform::Fingerprint(HashAlgo::Sha256))
        .expect("fp");
    let v = resource_view(&s.store, done);
    assert!(
        v.rel_permalink.starts_with("/css/a."),
        "{}",
        v.rel_permalink
    );
    assert!(
        v.data
            .integrity
            .as_deref()
            .is_some_and(|i| i.starts_with("sha256-"))
    );
    let min = s.store.transform(css, Transform::Minify).expect("minify");
    let pending = s
        .store
        .transform(min, Transform::Fingerprint(HashAlgo::Sha256))
        .expect("fp");
    let v = resource_view(&s.store, pending);
    assert!(
        v.rel_permalink.starts_with("__nh_pp_"),
        "{}",
        v.rel_permalink
    );
    assert!(v.permalink.starts_with("__nh_pp_") && v.permalink.ends_with("_permalink__"));
    assert!(
        v.data
            .integrity
            .as_deref()
            .is_some_and(|i| i.ends_with("_integrity__"))
    );
    // post_process: every late field is a placeholder.
    let v = post_processed_view(&s.store, min);
    assert!(
        v.media_type.r#type.ends_with("_media_type__"),
        "{}",
        v.media_type.r#type
    );
    assert!(v.rel_permalink.ends_with("_rel_permalink__"));
    assert_eq!(v.media_type.sub_type, "css");
}

/// Go's `.RegularPagesRecursive` lists the regular pages below the section that are listed
/// *locally* (`build.list = "local"` included, `never` not), unlike `site.regular_pages`,
/// in the default order; the home page walks the whole language.
#[test]
fn regular_pages_recursive_lists_local_pages() {
    let mut files = FILES.to_vec();
    files.extend([
        ("content/posts/deep/_index.md", "---\ntitle: Deep\n---\n"),
        (
            "content/posts/deep/local.md",
            "---\ntitle: Local\ndate: 2021-06-01\nbuild: {list: local}\n---\n",
        ),
        (
            "content/posts/deep/never.md",
            "---\ntitle: Never\ndate: 2021-07-01\nbuild: {list: never}\n---\n",
        ),
        (
            "content/posts/deep/leaf/index.md",
            "---\ntitle: Leaf\ndate: 2020-01-01\n---\n",
        ),
    ]);
    let s = load(&files);
    freeze(&s);
    let m = &s.model;
    let g = s.views.generation(Phase::Layout, HookVariant::Html);
    let titles = |v: &tera::Value| -> Vec<String> {
        v.as_array()
            .expect("list")
            .iter()
            .map(|p| get(p, "title").as_str().unwrap_or_default().to_owned())
            .collect()
    };
    let posts = g.full(page(m, PageKind::Section, "/posts", 0));
    assert_eq!(
        titles(get(&posts, "regular_pages_recursive")),
        ["Local", "Bundle", "Two", "One", "Leaf"]
    );
    let deep = g.full(page(m, PageKind::Section, "/posts/deep", 0));
    assert_eq!(
        titles(get(&deep, "regular_pages_recursive")),
        ["Local", "Leaf"]
    );
    let home = g.full(page(m, PageKind::Home, "/", 0));
    let all = titles(get(&home, "regular_pages_recursive"));
    let site: Vec<String> = m.sites[ssg_base::LangIdx::from_index(0)]
        .regular_pages
        .iter()
        .map(|&q| m.pages[q].title.clone())
        .collect();
    assert!(all.contains(&"Local".to_owned()) && !site.contains(&"Local".to_owned()));
    assert_eq!(all.len(), site.len() + 1, "{all:?} vs {site:?}");
}

/// An image that is not processed knows its size from its header (Go `.Width`/`.Height`,
/// REWRITE_PLAN §4.6 "known immediately"); a file that is not a decodable image has none.
#[test]
fn unprocessed_image_sizes() {
    let dir = tempfile::tempdir().expect("tempdir");
    for (rel, text) in FILES {
        let path = dir.path().join(rel);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, text).expect("write");
    }
    let png = ssg_testkit::fixture::repo_file("resources/testdata/gopher-hero8.png");
    std::fs::copy(png, dir.path().join("content/posts/bundle/img.png")).expect("copy");
    std::fs::write(
        dir.path().join("content/posts/bundle/bad.jpg"),
        "not a jpeg",
    )
    .expect("bad");
    let (m, _store, views) = load_dir(dir.path());
    let g = views.meta();
    let bundle = page(&m, PageKind::Page, "/posts/bundle", 0);
    let rs = get(&g.summaries[bundle], "resources")
        .as_array()
        .expect("resources")
        .to_vec();
    let by_name = |n: &str| {
        rs.iter()
            .find(|r| get(r, "name").as_str() == Some(n))
            .unwrap_or_else(|| panic!("resource {n}"))
            .clone()
    };
    let cover = by_name("cover");
    assert_eq!(get(&cover, "width").as_u64(), Some(591));
    assert_eq!(get(&cover, "height").as_u64(), Some(612));
    let bad = by_name("bad.jpg");
    assert!(get(&bad, "width").is_none(), "{:?}", get(&bad, "width"));
    let txt = by_name("data.txt");
    assert!(get(&txt, "width").is_none());
}
