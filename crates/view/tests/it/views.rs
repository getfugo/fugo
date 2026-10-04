//! The view cache on a small bilingual site with every page kind: generations and variants,
//! every documented key printed for every kind, Arc sharing, resource values, snapshots.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ssg_base::{FormatId, Idx, PageId, PageKind};
use ssg_markup::Fragments;
use ssg_nav::{Pagination, PaginationItems};
use ssg_resources::{CallSite, HashAlgo, Transform};
use ssg_view::views::{
    CONTENT_KEYS, MENU_ENTRY_KEYS, PAGE_LINK_KEYS, PAGE_RELATION_KEYS, PAGE_SUMMARY_KEYS,
    PAGER_KEYS, RESOURCE_KEYS, SITE_KEYS,
};
use ssg_view::{
    ContentError, ContentRenderer, ExpandedSource, HookVariant, PaginationRecorder, Phase,
    RenderScope, RenderStringOptions, RenderedContent, pager_view, post_processed_view,
    resource_view,
};

use crate::support::{FILES, freeze, get, keys, load, load_dir, page, same};

mod values;

fn sorted(lists: &[&[&str]]) -> Vec<String> {
    let mut v: Vec<String> = lists
        .iter()
        .flat_map(|l| l.iter().map(|k| (*k).to_owned()))
        .collect();
    v.sort();
    v
}

/// Renders `{{ v[k] }}` for every key: a missing key is an error in Tera.
fn print_keys(v: &tera::Value, want: &[String]) -> String {
    let mut tera = tera::Tera::default();
    tera.add_raw_template("t", "{% for k in keys %}{{ k }}={{ v[k] }};{% endfor %}")
        .expect("template");
    let mut ctx = tera::Context::new();
    ctx.insert_value("v", v.clone());
    ctx.insert("keys", &want);
    tera.render("t", &ctx)
        .unwrap_or_else(|e| panic!("printing {want:?}: {e:?}"))
}

#[test]
fn generations_and_variants() {
    let s = load(FILES);
    let one = page(&s.model, PageKind::Page, "/posts/one", 0);
    // Before phase D every phase sees the Meta generation: no content fields.
    let meta = s.views.generation(Phase::Layout, HookVariant::Html);
    assert!(std::ptr::eq(meta, s.views.meta()));
    assert!(!keys(&meta.summaries[one]).contains(&"content".to_owned()));
    assert!(!s.views.is_frozen());

    let json = freeze(&s);
    assert!(s.views.is_frozen());
    assert_eq!(s.views.variants(), [HookVariant::Html, json]);
    let html = s.views.generation(Phase::Layout, HookVariant::Html);
    let content = |g: &ssg_view::ViewGeneration| {
        get(&g.full(one), "content")
            .as_str()
            .expect("string")
            .to_owned()
    };
    assert_eq!(content(html), "<p>html One</p>");
    assert_eq!(
        content(s.views.generation(Phase::Layout, json)),
        "<p>json One</p>"
    );
    assert_eq!(
        content(s.views.generation(Phase::Deferred, json)),
        "<p>json One</p>"
    );
    // A variant without hooks of its own falls back to Html.
    let rss = HookVariant::Format(FormatId::from_raw(99));
    assert_eq!(
        content(s.views.generation(Phase::Layout, rss)),
        "<p>html One</p>"
    );
    // The content phase sees the Meta generation, also after the freeze.
    let c = s.views.generation(Phase::Content, json);
    assert!(std::ptr::eq(c, s.views.meta()));
    assert!(get(&c.full(one), "parent").is_map());
    assert!(!keys(&c.full(one)).contains(&"content".to_owned()));
    // Content values are safe strings; a page without content has empty fields.
    assert!(get(&html.summaries[one], "content").is_safe());
    let tax = page(&s.model, PageKind::Taxonomy, "/tags", 0);
    assert_eq!(get(&html.summaries[tax], "content").as_str(), Some(""));
    assert_eq!(
        get(&html.summaries[one], "raw_content").as_str(),
        Some("# Hello\n\nOne *body*.\n")
    );
    // The source is known before rendering: the content phase reads it too (one shared value).
    assert_eq!(
        get(&c.summaries[one], "raw_content").as_str(),
        Some("# Hello\n\nOne *body*.\n")
    );
    // A second freeze is ignored.
    s.views.freeze(&BTreeMap::new());
    assert_eq!(s.views.variants().len(), 2);
}

#[test]
fn every_documented_key_for_every_kind() {
    let s = load(FILES);
    freeze(&s);
    let g = s.views.generation(Phase::Layout, HookVariant::Html);
    let summary_keys = sorted(&[PAGE_SUMMARY_KEYS, CONTENT_KEYS]);
    let full_keys = sorted(&[PAGE_SUMMARY_KEYS, CONTENT_KEYS, PAGE_RELATION_KEYS]);
    let meta_full_keys = sorted(&[PAGE_SUMMARY_KEYS, PAGE_RELATION_KEYS]);
    let mut kinds = BTreeSet::new();
    for p in &s.model.pages {
        kinds.insert(p.kind);
        assert_eq!(
            keys(&g.summaries[p.id]),
            summary_keys,
            "{:?} {}",
            p.kind,
            p.path()
        );
        let full = g.full(p.id);
        assert_eq!(keys(&full), full_keys, "{:?} {}", p.kind, p.path());
        let printed = print_keys(&full, &full_keys);
        assert!(printed.starts_with("aliases="), "{printed}");
        assert_eq!(keys(&s.views.meta().full(p.id)), meta_full_keys);
        print_keys(&s.views.meta().full(p.id), &meta_full_keys);
        assert_eq!(keys(&g.links[p.id]), sorted(&[PAGE_LINK_KEYS]));
        print_keys(&g.links[p.id], &sorted(&[PAGE_LINK_KEYS]));
    }
    for k in [
        PageKind::Home,
        PageKind::Section,
        PageKind::Page,
        PageKind::Taxonomy,
        PageKind::Term,
        PageKind::NotFound,
        PageKind::Sitemap,
        PageKind::SitemapIndex,
        PageKind::RobotsTxt,
    ] {
        assert!(kinds.contains(&k), "no {k:?} page");
    }
    for site in g.sites.iter().chain(s.views.meta().sites.iter()) {
        assert_eq!(keys(site), sorted(&[SITE_KEYS]));
        print_keys(site, &sorted(&[SITE_KEYS]));
        assert_eq!(keys(get(site, "config")), ["privacy", "services"]);
        let menu = get(get(site, "menus"), "main")
            .as_array()
            .expect("main menu");
        assert_eq!(keys(&menu[0]), sorted(&[MENU_ENTRY_KEYS]));
        print_keys(&menu[0], &sorted(&[MENU_ENTRY_KEYS]));
    }
    // site.config: the snake-case fields of the embedded templates.
    let cfg = get(&g.sites[ssg_base::LangIdx::from_index(0)], "config");
    let t = |path: &str| -> String {
        let mut tera = tera::Tera::default();
        tera.add_raw_template("t", &format!("{{{{ c.{path} }}}}"))
            .expect("template");
        let mut ctx = tera::Context::new();
        ctx.insert_value("c", cfg.clone());
        tera.render("t", &ctx).expect("render")
    };
    assert_eq!(t("services.rss.limit"), "10");
    assert_eq!(t("services.google_analytics.id"), "G-1");
    assert_eq!(t("services.x.disable_inline_css"), "false");
    assert_eq!(t("privacy.youtube.privacy_enhanced"), "true");
    assert_eq!(t("privacy.x.enable_dnt"), "true");
    for service in [
        "disqus",
        "google_analytics",
        "instagram",
        "twitter",
        "vimeo",
        "x",
        "youtube",
    ] {
        for switch in [
            "disable",
            "simple",
            "enable_dnt",
            "respect_do_not_track",
            "privacy_enhanced",
        ] {
            t(&format!("privacy.{service}.{switch}"));
        }
    }
    // Resource values and term links.
    let bundle = page(&s.model, PageKind::Page, "/posts/bundle", 0);
    for r in get(&g.summaries[bundle], "resources")
        .as_array()
        .expect("resources")
    {
        assert_eq!(keys(r), sorted(&[RESOURCE_KEYS]));
        print_keys(r, &sorted(&[RESOURCE_KEYS]));
    }
    let one = page(&s.model, PageKind::Page, "/posts/one", 0);
    let tags = get(get(&g.summaries[one], "terms"), "tags");
    assert_eq!(
        keys(&tags.as_array().expect("tags")[0]),
        sorted(&[PAGE_LINK_KEYS])
    );
    // Pagers.
    let home = page(&s.model, PageKind::Home, "/", 0);
    let rec = PaginationRecorder::default().paginator(home, FormatId::from_raw(0), None, || {
        let items = PaginationItems::Pages(
            s.model.sites[s.model.pages[home].lang]
                .regular_pages
                .clone()
                .into(),
        );
        Pagination::new(items, 2).expect("pagination")
    });
    let pager = pager_view(g, &rec, 1, |n| Ok::<_, ()>(format!("/page/{n}/"))).expect("pager");
    let pager = tera::Value::from_serializable(&pager);
    assert_eq!(keys(&pager), sorted(&[PAGER_KEYS]));
    print_keys(&pager, &sorted(&[PAGER_KEYS]));
}

#[test]
fn lists_share_summaries() {
    let s = load(FILES);
    freeze(&s);
    let g = s.views.generation(Phase::Layout, HookVariant::Html);
    let en = ssg_base::LangIdx::from_index(0);
    let site = &g.sites[en];
    // Every list element is the generation's summary of that page (one allocation).
    let id = |v: &tera::Value| {
        PageId::from_raw(u32::try_from(get(v, "id").as_u64().expect("id")).expect("u32"))
    };
    let mut checked = 0;
    for list in ["pages", "regular_pages", "all_pages", "sections"] {
        for v in get(site, list).as_array().expect("list") {
            assert!(same(v, &g.summaries[id(v)]), "site.{list}");
            checked += 1;
        }
    }
    for p in &s.model.pages {
        let full = g.full(p.id);
        assert!(same(&full, &g.full(p.id)), "full values are built once");
        for rel in [
            "pages",
            "regular_pages",
            "sections",
            "ancestors",
            "translations",
        ] {
            for v in get(&full, rel).as_array().expect("list") {
                assert!(same(v, &g.summaries[id(v)]), "{rel} of {}", p.path());
                checked += 1;
            }
        }
        for rel in ["parent", "current_section", "first_section"] {
            let v = get(&full, rel);
            if !v.is_none() {
                assert!(same(v, &g.summaries[id(v)]));
            }
        }
        // Generation-independent parts are shared between the Meta and Full summaries.
        let meta = &s.views.meta().summaries[p.id];
        for k in ["params", "output_formats", "resources", "terms", "language"] {
            assert!(same(get(meta, k), get(&g.summaries[p.id], k)), "{k}");
        }
    }
    for (_, terms) in get(site, "taxonomies").as_map().expect("taxonomies") {
        for (_, t) in terms.as_map().expect("terms") {
            assert!(same(get(t, "page"), &g.summaries[id(get(t, "page"))]));
            for v in get(t, "pages").as_array().expect("pages") {
                assert!(same(v, &g.summaries[id(v)]));
                checked += 1;
            }
        }
    }
    assert!(same(
        get(site, "data"),
        get(&g.sites[ssg_base::LangIdx::from_index(1)], "data")
    ));
    assert!(checked > 20, "{checked}");
}

/// A renderer that knows only the content of pages with a file.
struct Fake(Arc<ssg_site::Model>);

impl ContentRenderer for Fake {
    fn content(
        &self,
        p: PageId,
        v: HookVariant,
        s: &RenderScope,
    ) -> Result<Arc<RenderedContent>, ContentError> {
        assert_eq!(s.phase, Phase::Content);
        assert_eq!(s.variant, v);
        if !crate::support::has_content(&self.0, p) {
            return Err(ContentError::NoContent(p));
        }
        Ok(Arc::new(RenderedContent {
            html: format!("<p>{:?} {}</p>", v, self.0.pages[p].title),
            ..RenderedContent::default()
        }))
    }

    fn fragments(&self, p: PageId, _: &RenderScope) -> Result<Arc<Fragments>, ContentError> {
        Err(ContentError::NoContent(p))
    }

    fn render_shortcodes(
        &self,
        p: PageId,
        _: &RenderScope,
    ) -> Result<Arc<ExpandedSource>, ContentError> {
        Err(ContentError::NoContent(p))
    }

    fn render_markdown(
        &self,
        md: &str,
        _: RenderStringOptions,
        _: &RenderScope,
    ) -> Result<String, ContentError> {
        Ok(md.to_owned())
    }

    fn render_template(
        &self,
        _: &ssg_layouts::TemplateName,
        _: tera::Context,
        _: &RenderScope,
    ) -> Result<String, ContentError> {
        Ok(String::new())
    }
}

#[test]
fn freeze_from_a_content_renderer() {
    let s = load(FILES);
    let json = HookVariant::Format(s.model.config.output_formats.by_name("json").expect("json"));
    s.views
        .freeze_from(&Fake(Arc::clone(&s.model)), &[HookVariant::Html, json])
        .expect("freeze");
    let one = page(&s.model, PageKind::Page, "/posts/one", 0);
    let content = |v| {
        get(
            &s.views.generation(Phase::Layout, v).summaries[one],
            "content",
        )
        .as_str()
        .expect("content")
        .to_owned()
    };
    assert_eq!(content(HookVariant::Html), "<p>Html One</p>");
    assert!(content(json).starts_with("<p>Format("), "{}", content(json));
}

#[test]
fn view_snapshots() {
    let s = load(FILES);
    freeze(&s);
    let m = &s.model;
    let g = s.views.generation(Phase::Layout, HookVariant::Html);
    let en = ssg_base::LangIdx::from_index(0);
    let mut site = g.sites[en].as_map().cloned().expect("site");
    // Lists of summaries are covered by the page snapshots; keep the ids.
    for k in ["pages", "regular_pages", "all_pages", "sections"] {
        let ids: Vec<tera::Value> = get(&g.sites[en], k)
            .as_array()
            .expect("list")
            .iter()
            .map(|p| get(p, "path").clone())
            .collect();
        site.insert(k.into(), tera::Value::from(ids));
    }
    site.insert(
        "home".into(),
        get(get(&g.sites[en], "home"), "path").clone(),
    );
    site.insert("taxonomies".into(), tera::Value::none());
    ssg_testkit::snapshot::settings().bind(|| {
        insta::assert_yaml_snapshot!("site_en", tera::Value::from(site));
        insta::assert_yaml_snapshot!(
            "page_bundle_full",
            g.full(page(m, PageKind::Page, "/posts/bundle", 0))
        );
        insta::assert_yaml_snapshot!(
            "term_a_summary",
            &g.summaries[page(m, PageKind::Term, "/tags/a", 0)]
        );
        insta::assert_yaml_snapshot!(
            "page_one_fr_meta_summary",
            &s.views.meta().summaries[page(m, PageKind::Page, "/posts/one", 1)]
        );
    });
}
