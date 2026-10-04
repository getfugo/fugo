//! Unit tests of the typed front matter, the cascade, titles and permalink patterns.

use jiff::tz::TimeZone;
use ssg_base::paths::ContentKey;
use ssg_base::url::SiteUrls;
use ssg_base::{Map, PageKind, Params, Value};
use ssg_config::sections::SitemapConfig;
use ssg_config::site::TitleConfig;
use ssg_config::{DateField, DateSource, MediaTypes, OutputFormats};
use ssg_page::{
    Cascade, Cjk, DateResolver, FileCtx, GoLayout, ListMode, Markup, MarkupSource, MatchCtx,
    MetaCtx, PageError, PermalinkCtx, PermalinkPattern, RenderMode, capture_overrides,
    default_title, format_go_layout, meta_from_params,
};

mod adapters;

fn params(toml: &str) -> Params {
    let Value::Map(m) = Value::from_toml_str(toml).expect("toml") else {
        panic!("not a table");
    };
    Params::fold(&m)
}

struct Site {
    types: MediaTypes,
    formats: OutputFormats,
    sitemap: SitemapConfig,
    dates: DateResolver,
    tz: TimeZone,
}

impl Site {
    fn new() -> Self {
        let types = MediaTypes::decode(&Map::new()).expect("media types");
        let formats = OutputFormats::builtin(&types);
        let dates = DateResolver::new(&[
            (
                DateField::Date,
                vec![DateSource::Field("date".into()), DateSource::Filename],
            ),
            (
                DateField::Lastmod,
                vec![DateSource::Field("lastmod".into())],
            ),
            (
                DateField::PublishDate,
                vec![
                    DateSource::Field("publishdate".into()),
                    DateSource::Field("date".into()),
                ],
            ),
            (
                DateField::ExpiryDate,
                vec![DateSource::Field("expirydate".into())],
            ),
        ]);
        Self {
            types,
            formats,
            sitemap: SitemapConfig::default(),
            tz: TimeZone::UTC,
            dates,
        }
    }

    fn ctx<'a>(&'a self, kind: PageKind, ext: &'a str, file: Option<FileCtx<'a>>) -> MetaCtx<'a> {
        MetaCtx {
            kind,
            formats: &self.formats,
            media_types: &self.types,
            sitemap: &self.sitemap,
            cjk_default: Cjk::No,
            ext,
            dates: &self.dates,
            file,
            time_zone: &self.tz,
        }
    }
}

#[test]
fn meta_decodes_reserved_keys() {
    let site = Site::new();
    let p = params(
        r#"
        Title = "Hello"
        linkTitle = "Hi"
        slug = "-my-slug-"
        weight = "3"
        draft = "true"
        keywords = "a b"
        aliases = ["/old/"]
        outputs = ["HTML", "json"]
        headless = true
        translationKey = "k"
        markup = "html"
        date = "2024-01-02"
        tags = ["x"]
        [sitemap]
        priority = 0.4
        [params]
        tags = ["y"]
        extra = 1
        [menus.main]
        weight = 5
        parent = "docs"
        [[resources]]
        src = "images/*.jpg"
        name = "photo-:counter"
        "#,
    );
    let meta = meta_from_params(p, &site.ctx(PageKind::Page, "md", None)).expect("meta");
    assert_eq!(meta.title.as_deref(), Some("Hello"));
    assert_eq!(meta.link_title.as_deref(), Some("Hi"));
    assert_eq!(meta.slug.as_deref(), Some("my-slug"));
    assert_eq!(meta.weight, 3);
    assert!(meta.draft);
    assert_eq!(meta.keywords, ["a", "b"]);
    assert_eq!(meta.aliases, ["/old/"]);
    let names: Vec<&str> = meta
        .outputs
        .iter()
        .flatten()
        .map(|&id| site.formats.get(id).name.as_str())
        .collect();
    assert_eq!(names, ["html", "json"]);
    assert_eq!(meta.build.list, ListMode::Never);
    assert_eq!(meta.build.render, RenderMode::Never);
    assert!(meta.build.publish_resources);
    assert_eq!(meta.translation_key.as_deref(), Some("k"));
    assert_eq!(meta.markup, Markup::Html);
    assert!((meta.sitemap.priority - 0.4).abs() < f64::EPSILON);
    assert_eq!(meta.sitemap.filename, "sitemap.xml");
    assert_eq!(meta.menus.len(), 1);
    assert_eq!(meta.menus[0].menu, "main");
    assert_eq!(meta.menus[0].weight, 5);
    assert_eq!(meta.menus[0].parent, "docs");
    assert_eq!(meta.resources.len(), 1);
    assert!(meta.resources[0].matcher.is_match("images/a.jpg"));
    assert!(!meta.resources[0].matcher.is_match("images/sub/a.jpg"));
    // Normalised values stay in params; `params:` wins over the top level.
    assert_eq!(meta.params.get("slug"), Some(&Value::string("my-slug")));
    assert_eq!(meta.params.get("weight"), Some(&Value::Int(3)));
    assert_eq!(meta.params.get("draft"), Some(&Value::Bool(true)));
    assert_eq!(meta.params.get("iscjklanguage"), Some(&Value::Bool(false)));
    assert_eq!(
        meta.params
            .get_path("tags")
            .and_then(Value::as_array)
            .map(<[Value]>::len),
        Some(1)
    );
    assert_eq!(
        meta.params.get("tags"),
        Some(&Value::array(vec![Value::string("y")]))
    );
    assert_eq!(meta.params.get("extra"), Some(&Value::Int(1)));
    // Dates are resolved and written back.
    let date = meta.dates.date.as_ref().expect("date");
    assert_eq!(date.to_string(), "2024-01-02T00:00:00+00:00[UTC]");
    assert_eq!(meta.dates.publish_date.as_ref(), Some(date));
    assert!(matches!(
        meta.params.get("publishdate"),
        Some(Value::Date(_))
    ));
}

#[test]
fn meta_rejects_bad_values() {
    let site = Site::new();
    let ctx = site.ctx(PageKind::Page, "md", None);
    let err = |toml: &str| meta_from_params(params(toml), &ctx).expect_err(toml);
    assert!(matches!(
        err(r#"url = "https://example.org/x/""#),
        PageError::AbsoluteUrl(_)
    ));
    assert!(matches!(
        err(r#"outputs = ["nope"]"#),
        PageError::OutputFormat(_)
    ));
    assert!(matches!(
        err(r#"markup = "asciidoc""#),
        PageError::UnsupportedMarkup(_)
    ));
    assert!(matches!(
        err(r#"cascade = { title = "x" }"#),
        PageError::Cascade(_)
    ));
    assert!(matches!(
        err(r#"build = { publishResources = "maybe" }"#),
        PageError::Field { .. }
    ));
    // A branch page may cascade.
    let branch = site.ctx(PageKind::Section, "md", None);
    let meta = meta_from_params(params(r#"cascade = { title = "x" }"#), &branch).expect("branch");
    assert_eq!(meta.cascade.rules().len(), 1);
}

/// Go's undocumented `published: <bool>` and the legacy `_build` key.
#[test]
fn published_and_legacy_build() {
    let site = Site::new();
    let ctx = site.ctx(PageKind::Page, "md", None);
    let meta = |toml: &str| meta_from_params(params(toml), &ctx).expect(toml);
    assert!(meta("published = false").draft);
    assert!(!meta("published = true").draft);
    assert!(meta(r#"published = "false""#).draft);
    // `draft` wins over `published`.
    assert!(!meta("published = false\ndraft = false").draft);
    assert!(meta("published = true\ndraft = true").draft);
    // A `published` date is a date source, not a draft flag.
    let dated = meta("published = 2024-03-04");
    assert!(!dated.draft);
    assert_eq!(dated.params.get("draft"), Some(&Value::Bool(false)));

    let legacy = meta(r#"_build = { list = "never", render = "link" }"#);
    assert_eq!(legacy.build.list, ListMode::Never);
    assert_eq!(legacy.build.render, RenderMode::Link);
    assert!(legacy.params.get("_build").is_some());
    assert!(legacy.params.get("build").is_none());
    // `_build` wins over `build`, as in Go.
    let both = meta("_build = { list = \"never\" }\nbuild = { list = \"local\" }");
    assert_eq!(both.build.list, ListMode::Never);
    assert!(matches!(
        meta_from_params(params(r#"_build = { publishResources = "maybe" }"#), &ctx),
        Err(PageError::Field { .. })
    ));
}

#[test]
fn filename_dates_supply_the_slug() {
    let site = Site::new();
    let file = FileCtx {
        base_filename: "2024-03-04-hello-world.md",
        mod_time: None,
        git_author_date: None,
    };
    let meta = meta_from_params(
        Params::default(),
        &site.ctx(PageKind::Page, "md", Some(file)),
    )
    .expect("meta");
    assert_eq!(meta.slug.as_deref(), Some("hello-world"));
    assert_eq!(
        meta.dates.date.as_ref().map(ToString::to_string).as_deref(),
        Some("2024-03-04T00:00:00+00:00[UTC]")
    );
    // A front matter slug wins.
    let meta = meta_from_params(
        params(r#"slug = "mine""#),
        &site.ctx(PageKind::Page, "md", Some(file)),
    )
    .expect("meta");
    assert_eq!(meta.slug.as_deref(), Some("mine"));
    // Unparsable dates are reported, not fatal.
    let meta = meta_from_params(
        params(r#"date = "soon""#),
        &site.ctx(PageKind::Page, "md", None),
    )
    .expect("meta");
    assert_eq!(meta.unparsable_dates, ["date"]);
    assert!(meta.dates.date.is_none());
}

#[test]
fn capture_overrides_are_typed() {
    let o = capture_overrides(&params(
        r#"kind = "Section"
lang = "TH"
path = "/Docs/My Page""#,
    ))
    .expect("overrides");
    assert_eq!(o.kind, Some(PageKind::Section));
    assert_eq!(o.lang.as_deref(), Some("th"));
    assert_eq!(o.path, Some(ContentKey::from_source("docs/my-page")));
    assert!(capture_overrides(&params(r#"kind = "404""#)).is_err());
    assert!(capture_overrides(&params("kind = 3")).is_err());
    assert_eq!(
        capture_overrides(&Params::default()).expect("none"),
        Default::default()
    );
}

#[test]
fn markup_detection_order() {
    let types = MediaTypes::decode(&Map::new()).expect("media types");
    let detect = |media_type, markup, ext| {
        Markup::detect(
            MarkupSource {
                media_type,
                markup,
                ext,
            },
            &types,
        )
    };
    assert_eq!(detect(None, None, "md").ok(), Some(Markup::Markdown));
    assert_eq!(detect(None, None, "htm").ok(), Some(Markup::Html));
    assert_eq!(detect(None, None, "").ok(), Some(Markup::Markdown));
    assert_eq!(
        detect(None, Some("goldmark"), "html").ok(),
        Some(Markup::Markdown)
    );
    assert_eq!(
        detect(Some("text/html"), Some("md"), "md").ok(),
        Some(Markup::Html)
    );
    assert!(matches!(
        detect(None, None, "adoc"),
        Err(PageError::UnsupportedMarkup(_))
    ));
    assert!(matches!(
        detect(None, None, "nope"),
        Err(PageError::UnknownMarkup(_))
    ));
}

#[test]
fn cascade_fills_missing_keys_nearest_first() {
    let root = Cascade::decode(
        &Value::from_toml_str(
            r#"
            [[cascade]]
            title = "root"
            layout = "root"
            [cascade.params]
            color = "red"
            [[cascade]]
            [cascade.target]
            kind = "page"
            path = "/Blog/**"
            [cascade.params]
            banner = "blog"
            "#,
        )
        .expect("toml")
        .as_map()
        .expect("map")
        .get("cascade")
        .expect("cascade")
        .clone(),
    )
    .expect("root");
    let own =
        Cascade::decode(&Value::from_toml_str(r#"layout = "own""#).expect("toml")).expect("own");
    let merged = Cascade::inherit(&root, &own);
    let page = MatchCtx {
        kind: PageKind::Page,
        path: "/blog/post",
        lang: "en",
        environment: "production",
    };
    let mut p = params(r#"title = "Mine""#);
    merged.apply(&page, &mut p);
    assert_eq!(p.get("title"), Some(&Value::string("Mine")));
    assert_eq!(p.get("layout"), Some(&Value::string("own")));
    assert_eq!(p.get("color"), Some(&Value::string("red")));
    assert_eq!(p.get("banner"), Some(&Value::string("blog")));
    let section = MatchCtx {
        kind: PageKind::Section,
        path: "/blog",
        ..page
    };
    let mut p = Params::default();
    merged.apply(&section, &mut p);
    assert_eq!(p.get("banner"), None);
    assert!(Cascade::decode(&Value::from_toml_str(r#"kind = "page""#).expect("toml")).is_err());
}

#[test]
fn default_titles() {
    let cfg = TitleConfig::default();
    assert_eq!(
        default_title(PageKind::Section, "biscuit-roll", &cfg),
        "Biscuit-Rolls"
    );
    assert_eq!(default_title(PageKind::Section, "candy", &cfg), "Candies");
    assert_eq!(
        default_title(PageKind::Taxonomy, "brand-names", &cfg),
        "Brand Names"
    );
    assert_eq!(
        default_title(PageKind::Term, "no salt/low salt chips", &cfg),
        "No Salt/Low Salt Chips"
    );
    assert_eq!(
        default_title(PageKind::NotFound, "", &cfg),
        "404 Page not found"
    );
    assert_eq!(default_title(PageKind::Home, "Site", &cfg), "Site");
    let plain = TitleConfig {
        pluralize: false,
        capitalize: false,
        ..cfg
    };
    assert_eq!(default_title(PageKind::Section, "candy", &plain), "candy");
}

#[test]
fn go_layouts_become_strftime() {
    let urls = SiteUrls::default();
    let date = jiff::civil::date(2018, 2, 8)
        .at(15, 4, 5, 0)
        .to_zoned(TimeZone::UTC)
        .expect("date");
    let ctx = PermalinkCtx {
        date: Some(&date),
        title: "A Title",
        slug: "",
        section: "blog",
        sections: &["blog", "sub"],
        file: None,
        content_base_name: "A Title",
        urls: &urls,
    };
    let expand = |p: &str| PermalinkPattern::parse(p).expect(p).expand(&ctx).expect(p);
    // `:15:04` is two attributes.
    assert_eq!(expand("/:2006/:01/:02/:15:04/"), "/2018/02/08/1504/");
    assert_eq!(
        expand("/:January-:Jan-:Monday-:Mon/"),
        "/February-Feb-Thursday-Thu/"
    );
    assert_eq!(expand("/:_2/:2/:1/:3PM/:002/"), "/ 8/8/2/3PM/039/");
    assert_eq!(expand("/:06:MST/"), "/18UTC/");
    assert_eq!(
        expand("/:sections[1:]/:sections[last]/:title/"),
        "/sub/sub/a-title/"
    );
    assert!(PermalinkPattern::parse("/:nothing/").is_err());
}

#[test]
fn front_matter_menu_scalars() {
    let site = Site::new();
    let menus = |toml: &str| {
        meta_from_params(params(toml), &site.ctx(PageKind::Page, "md", None))
            .expect("meta")
            .menus
    };
    let m = menus("menu = 42");
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].menu, "42");
    assert_eq!(menus("menus = [\"main\", 7]")[1].menu, "7");
    // A boolean in the entry's text fields reads as `1`/`0`.
    let m = menus("[menus.main]\npre = true\npost = false\nname = \"n\"");
    assert_eq!(
        (m[0].pre.as_str(), m[0].post.as_str(), m[0].name.as_str()),
        ("1", "0", "n")
    );
}

#[test]
fn go_layouts_format() {
    let d: jiff::Zoned = "2024-03-05T14:07:09+07:00[Asia/Bangkok]"
        .parse()
        .expect("date");
    assert_eq!(format_go_layout(&d, "2006"), "2024");
    assert_eq!(format_go_layout(&d, "2006-01-02"), "2024-03-05");
    assert_eq!(format_go_layout(&d, "Jan 2, 06 3:04PM"), "Mar 5, 24 2:07PM");
    assert_eq!(
        format_go_layout(&d, "Monday 15:04:05 -07:00"),
        "Tuesday 14:07:09 +07:00"
    );
    assert_eq!(format_go_layout(&d, "x_2y"), "x 5y");
    assert_eq!(format_go_layout(&d, "-0700 Z07:00 -07"), "+0700 +07:00 +07");
    // `1` is the month; `%` is text.
    assert_eq!(format_go_layout(&d, "100%"), "300%");
    assert_eq!(format_go_layout(&d, "2006 %d"), "2024 %d");
    let utc: jiff::Zoned = "0001-01-01T00:00:00+00:00[UTC]".parse().expect("date");
    assert_eq!(format_go_layout(&utc, "2006 Z0700 -0700"), "0001 Z +0000");
    assert!(GoLayout::parse("text").is_none());
}
