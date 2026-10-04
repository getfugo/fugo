//! The pages of content adapters: their placement, values and dates.

use super::*;

/// Content adapters: the `add_page` map is placed below the adapter's directory, its fields
/// are read from the top level with the cascade's fields filled in, `.Params` are only the
/// cascade's params and the map's `params`, and the dates follow the `[frontmatter]` chains
/// over the four date fields (Go's `createContentAdapterDatesHandler`).
#[test]
fn adapter_pages() {
    use ssg_page::{AdapterPage, meta_from_adapter};

    let site = Site::new();
    let map = params(
        r#"
        kind = "page"
        path = "V0.1 Notes/Sub"
        title = "Release v0.1"
        slug = "v0.1"
        weight = 3
        keywords = ["a", "B"]
        build = { render = "never", list = "local" }
        content = { mediaType = "text/html", value = "<b>x</b>" }
        params = { Permalink = "https://example.com/v0.1", shared = "map" }
        dates = { publishDate = 2025-10-02T13:54:48Z, lastmod = "2025-10-03T00:00:00Z", pubdate = 2020-01-01T00:00:00Z }
        "#,
    );
    let base = ContentKey::from_source("news");
    let page = AdapterPage::decode(&map, &base, &site.types).expect("decode");
    assert_eq!(page.kind, PageKind::Page);
    assert_eq!(page.path, "news/v0.1-notes/sub");
    assert_eq!(page.source_path(), "/news/v0.1-notes/sub/index.html");
    assert_eq!(page.markup, Markup::Html);
    assert_eq!(page.content, "<b>x</b>");

    // The cascade fills fields the map does not set (`title` is set) and the params.
    let mut fields = page.fields.clone();
    let mut cascaded = Params::default();
    let cascade = Cascade::decode(&Value::from_toml_str(
        "title = \"cascaded\"\ndescription = \"from cascade\"\n[params]\nshared = \"cascade\"\nshow = true\n",
    ).expect("toml"))
    .expect("cascade");
    cascade.apply_split(
        &MatchCtx {
            kind: PageKind::Page,
            path: "/news/v0.1-notes/sub",
            lang: "en",
            environment: "production",
        },
        &mut fields,
        &mut cascaded,
    );
    let meta = meta_from_adapter(
        &page,
        &fields,
        cascaded,
        &site.ctx(PageKind::Page, "", None),
    )
    .expect("meta");
    assert_eq!(meta.title.as_deref(), Some("Release v0.1"));
    assert_eq!(meta.description, "from cascade");
    assert_eq!(meta.slug.as_deref(), Some("v0.1"));
    assert_eq!(meta.weight, 3);
    assert_eq!(meta.keywords, ["a", "B"]);
    assert_eq!(meta.build.render, RenderMode::Never);
    assert_eq!(meta.build.list, ListMode::Local);
    // Only the cascade's params and the map's: no reserved keys, no dates.
    let keys: Vec<&str> = meta.params.iter().map(|(k, _)| k).collect();
    assert_eq!(keys, ["permalink", "shared", "show"]);
    assert_eq!(meta.params.get("shared"), Some(&Value::string("map")));
    // `date` has no `date`; publishDate from `publishDate`; lastmod from `lastmod`; the alias
    // `pubdate` is not a source here.
    let ymd = |z: Option<&jiff::Zoned>| z.map(|z| z.strftime("%Y-%m-%dT%H:%M:%S%:z").to_string());
    assert_eq!(ymd(meta.dates.date.as_ref()), None);
    assert_eq!(
        ymd(meta.dates.publish_date.as_ref()).as_deref(),
        Some("2025-10-02T13:54:48+00:00")
    );
    assert_eq!(
        ymd(meta.dates.lastmod.as_ref()).as_deref(),
        Some("2025-10-03T00:00:00+00:00")
    );
    // The sitemap settings start from zero, not from the site's `[sitemap]`.
    assert_eq!(meta.sitemap.priority, 0.0);
    assert_eq!(meta.sitemap.filename, "");

    // A date value of the `to_date` filter (`{rfc3339, unix}`).
    let map = params(
        "path = \"p\"\n[dates.date]\nrfc3339 = \"2024-11-06T11:22:34Z\"\nunix = 1730892154\n",
    );
    let page = AdapterPage::decode(&map, &ContentKey::home(), &site.types).expect("decode");
    let meta = meta_from_adapter(
        &page,
        &page.fields,
        Params::default(),
        &site.ctx(PageKind::Page, "", None),
    )
    .expect("meta");
    assert_eq!(
        ymd(meta.dates.date.as_ref()).as_deref(),
        Some("2024-11-06T11:22:34+00:00")
    );
    assert_eq!(
        ymd(meta.dates.publish_date.as_ref()).as_deref(),
        Some("2024-11-06T11:22:34+00:00"),
        "publishDate falls back to date"
    );

    // Branch kinds are `_index` files; the home page has the empty path.
    let page = AdapterPage::decode(&params("kind = \"home\""), &ContentKey::home(), &site.types)
        .expect("home");
    assert_eq!(page.source_path(), "/_index.md");
    let page = AdapterPage::decode(
        &params("kind = \"section\"\npath = \"docs\"\ncascade = { params = { a = 1 } }"),
        &ContentKey::home(),
        &site.types,
    )
    .expect("section");
    assert_eq!(page.source_path(), "/docs/_index.md");
    assert!(!page.cascade.is_empty());

    for (toml, want) in [
        ("kind = \"page\"", "`path` is empty"),
        ("path = \"p\"\nlang = \"en\"", "`lang` cannot be set"),
        (
            "path = \"p\"\ncontent = { markup = \"md\" }",
            "`content.markup`",
        ),
        (
            "path = \"p\"\ncascade = { params = { a = 1 } }",
            "only branch pages",
        ),
        ("path = \"p\"\nkind = \"nope\"", "nope"),
        (
            "path = \"p\"\ncontent = { mediaType = \"text/nope\" }",
            "text/nope",
        ),
        (
            "path = \"p\"\ncontent = { mediaType = \"text/asciidoc\" }",
            "not supported",
        ),
    ] {
        let e =
            AdapterPage::decode(&params(toml), &ContentKey::home(), &site.types).expect_err(toml);
        assert!(
            matches!(
                e,
                PageError::Adapter(_)
                    | PageError::Kind(_)
                    | PageError::UnknownMarkup(_)
                    | PageError::UnsupportedMarkup(_)
                    | PageError::Cascade(_)
            ),
            "{toml}: {e:?}"
        );
        assert!(e.to_string().contains(want), "{toml}: {e}");
    }
}

/// Go's `createContentAdapterDatesHandler` runs the date, lastmod, publishDate and
/// expiryDate chains one after the other on the given dates: a chain reads what the earlier
/// ones set, and a date whose chain finds nothing keeps its given value. The cases are the
/// Go binary's results (at 44529028) for the same `add_page` maps. The slug is used as
/// given (Go trims `-` from front matter slugs only).
#[test]
fn adapter_dates_follow_the_chains_in_turn() {
    use ssg_config::decode_front_matter;
    use ssg_page::{AdapterPage, meta_from_adapter};

    let ymd = |z: Option<&jiff::Zoned>| z.map(|z| z.strftime("%Y-%m-%d").to_string());
    let run = |frontmatter: &str, map: &str| {
        let mut site = Site::new();
        let Value::Map(fm) = Value::from_toml_str(frontmatter).expect("toml") else {
            panic!("not a table");
        };
        site.dates = DateResolver::new(&decode_front_matter(&fm));
        let page =
            AdapterPage::decode(&params(map), &ContentKey::home(), &site.types).expect("decode");
        let meta = meta_from_adapter(
            &page,
            &page.fields,
            Params::default(),
            &site.ctx(PageKind::Page, "", None),
        )
        .expect("meta");
        let d = &meta.dates;
        (
            [&d.date, &d.lastmod, &d.publish_date, &d.expiry_date].map(|z| ymd(z.as_ref())),
            meta.slug,
        )
    };
    let day = || Some("2024-01-05".to_owned());

    // The default chains: `date` takes `lastmod`, then `publishDate` takes that `date`.
    let (dates, _) = run(
        "",
        "path = \"p\"\ndates = { lastmod = 2024-01-05T00:00:00Z }",
    );
    assert_eq!(dates, [day(), day(), day(), None]);
    // `date = [\"lastmod\"]` finds no lastmod: the given date stays (and fills the others).
    let (dates, _) = run(
        "date = [\"lastmod\"]\npublishDate = [\"date\"]",
        "path = \"p\"\ndates = { date = 2024-01-05T00:00:00Z }",
    );
    assert_eq!(dates, [day(), day(), day(), None]);
    // An empty chain keeps the given date.
    let (dates, _) = run(
        "expiryDate = []",
        "path = \"p\"\ndates = { expiryDate = 2024-01-05T00:00:00Z }",
    );
    assert_eq!(dates, [None, None, None, day()]);

    let (_, slug) = run("", "path = \"p\"\nslug = \"-sl-\"");
    assert_eq!(slug.as_deref(), Some("-sl-"));
}
