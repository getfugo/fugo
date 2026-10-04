//! What `ssg_cms::publish` writes: the editor, its index, the Worker and `.assetsignore`.

use serde_json::Value;
use ssg_base::Sink;
use ssg_base::paths::OutputPath;

use crate::support::{TestSink, cms_toml, index_of, load, settings_of, write_files};

const CONFIG: &str = r#"
baseURL = "https://snack.example/"
title = "Snacks & <Co>"
defaultContentLanguage = "en"
[languages.en]
weight = 1
languageName = "English"
[languages.th]
weight = 2
languageName = "ไทย"
[taxonomies]
brand = "brands"
[taxonomies.category]
plural = "categories"
hierarchical = true
"#;

fn project(extra_cms: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir(dir.path().join(".git")).expect(".git");
    write_files(
        dir.path(),
        &[
            ("config.toml", &format!("{CONFIG}{}", cms_toml(extra_cms))),
            ("content/_index.md", "---\ntitle: Home\n---\n"),
            ("content/almonds/_index.en.md", "---\ntitle: Almonds\n---\n"),
            (
                "content/almonds/honey/index.en.md",
                "---\ntitle: Honey Almond\ncategories:\n  - almonds/roasted\nbrands: Tom\nrating:\n  sweet: 4\nwhen: 2020-10-25T12:00:07.092Z\n---\nText.\n",
            ),
            (
                "content/almonds/honey/index.th.md",
                "---\ntitle: อัลมอนด์\ncategories: [almonds]\nbrands: [ทอม]\n---\n",
            ),
            ("content/almonds/honey/1.jpg", "jpg"),
            ("content/almonds/honey/img/2.png", "png"),
            (
                "content/almonds/salted/index.en.md",
                "---\ntitle: Salted\n---\n",
            ),
            (
                "content/blog/post.md",
                "+++\ntitle = \"A post\"\ndraft = true\n+++\n",
            ),
            ("content/blog/broken.md", "---\ntitle: [unclosed\n---\n"),
            ("content/books/_content.html", "{# adapter #}"),
            ("content/.hidden.md", "---\ntitle: hidden\n---\n"),
            ("static/images/uploads/a.jpg", "jpg"),
        ],
    );
    dir
}

fn publish(dir: &std::path::Path, env: &str) -> (TestSink, Option<ssg_cms::Published>) {
    let sink = TestSink::default();
    let out = ssg_cms::publish(&load(dir, env), &sink).expect("publish");
    (sink, out)
}

#[test]
fn writes_the_editor_the_index_and_the_worker() {
    let dir = project("[cms.roles.translator]\nedit = [\"content/**/index.th.md\"]\n");
    let (sink, out) = publish(dir.path(), "production");
    let out = out.expect("cms");
    assert_eq!(out.path, "/admin/");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let paths = sink.paths();
    for p in [
        ".assetsignore",
        "_worker.js",
        "admin/cms.css",
        "admin/cms.js",
        "admin/index.html",
        "_headers",
    ] {
        assert!(paths.contains(&p.to_owned()), "{p} not in {paths:?}");
    }
    let html = sink.text("admin/index.html");
    assert!(
        html.contains("<title>Snacks &amp; &lt;Co&gt;</title>"),
        "{html}"
    );
    assert_eq!(sink.text(".assetsignore"), "_worker.js\n.assetsignore\n");
    assert!(
        !paths.contains(&"admin/site.json".to_owned()),
        "the index is not public"
    );
    assert_eq!(
        sink.text("_headers"),
        "/admin/*\n  X-Frame-Options: DENY\n  Content-Security-Policy: frame-ancestors 'none'\n  Referrer-Policy: same-origin\n"
    );
}

#[test]
fn the_index_lists_pages_translations_resources_and_terms() {
    let dir = project("");
    let (sink, _) = publish(dir.path(), "production");
    let index = index_of(&sink.text("_worker.js"));
    assert_eq!(index["path"], "/admin/");
    assert_eq!(index["api"], "/admin/api/");
    assert_eq!(index["default_language"], "en");
    assert_eq!(index["content_dir"], "content");
    assert_eq!(index["languages"][1]["name"], "ไทย");
    let entries = index["entries"].as_array().expect("entries");
    let keys: Vec<&str> = entries
        .iter()
        .map(|e| e["key"].as_str().expect("key"))
        .collect();
    assert_eq!(
        keys,
        [
            "_index",
            "almonds/_index",
            "almonds/honey",
            "almonds/salted",
            "blog/broken",
            "blog/post"
        ]
    );
    let honey = &entries[2];
    assert_eq!(honey["kind"], "page");
    assert_eq!(honey["bundle"], true);
    assert_eq!(honey["section"], "almonds");
    assert_eq!(honey["title"], "Honey Almond");
    assert_eq!(honey["files"][0]["lang"], "en");
    assert_eq!(
        honey["files"][0]["path"],
        "content/almonds/honey/index.en.md"
    );
    assert_eq!(honey["files"][1]["lang"], "th");
    assert_eq!(honey["files"][1]["title"], "อัลมอนด์");
    assert_eq!(
        honey["resources"],
        serde_json::json!([
            "content/almonds/honey/1.jpg",
            "content/almonds/honey/img/2.png"
        ])
    );
    assert_eq!(entries[1]["kind"], "section");
    assert_eq!(entries[0]["kind"], "home");
    let post = &entries[5];
    assert_eq!(post["files"][0]["format"], "toml");
    assert_eq!(post["files"][0]["draft"], true);
    assert_eq!(entries[4]["files"][0]["format"], "yaml");

    let taxonomies = index["taxonomies"].as_array().expect("taxonomies");
    let terms = |plural: &str| {
        taxonomies
            .iter()
            .find(|t| t["plural"] == plural)
            .map(|t| t["terms"].clone())
            .expect(plural)
    };
    assert_eq!(
        terms("categories"),
        serde_json::json!(["almonds", "almonds/roasted"])
    );
    assert_eq!(terms("brands"), serde_json::json!(["Tom", "ทอม"]));

    let almonds = index["sections"]
        .as_array()
        .expect("sections")
        .iter()
        .find(|s| s["key"] == "almonds")
        .expect("almonds");
    assert_eq!(almonds["title"], "Almonds");
    assert_eq!(almonds["count"], 2);
    assert_eq!(almonds["style"]["bundle"], true);
    assert_eq!(almonds["style"]["lang_suffix"], true);
    assert_eq!(almonds["style"]["format"], "yaml");
    let kinds: Vec<(String, String)> = almonds["keys"]
        .as_array()
        .expect("keys")
        .iter()
        .map(|k| {
            (
                k["key"].as_str().expect("k").to_owned(),
                k["kind"].as_str().expect("kind").to_owned(),
            )
        })
        .collect();
    for want in [
        ("rating", "map"),
        ("when", "date"),
        ("categories", "list"),
        ("title", "string"),
    ] {
        assert!(
            kinds.contains(&(want.0.to_owned(), want.1.to_owned())),
            "{want:?} not in {kinds:?}"
        );
    }
}

#[test]
fn the_worker_has_the_settings_and_common_inline() {
    let dir = project("[cms.roles.writer]\nedit = [\"content/blog/**\"]\n");
    let (sink, _) = publish(dir.path(), "production");
    let worker = sink.text("_worker.js");
    assert!(
        !worker.lines().any(|l| l.starts_with("import ")),
        "the Worker imports nothing: it is one module"
    );
    assert!(
        worker.contains(" as compileGlob,"),
        "common.ts is bundled in"
    );
    assert!(
        worker.contains(" as default,"),
        "the module's default export is the Worker"
    );
    let settings = settings_of(&worker);
    assert_eq!(settings["api"], "/admin/api/");
    assert_eq!(settings["workflow"], "review");
    assert_eq!(settings["git"]["host"], "github");
    assert_eq!(settings["git"]["repo"], "owner/site");
    assert_eq!(settings["git"]["branch"], "main");
    assert_eq!(settings["git"]["dir"], "");
    assert_eq!(
        settings["login"]["team"],
        "https://team.cloudflareaccess.com"
    );
    assert_eq!(settings["login"]["aud"], serde_json::json!(["aud-1"]));
    assert_eq!(
        settings["roles"]["writer"]["edit"],
        serde_json::json!(["content/blog/**"])
    );
    assert_eq!(settings["deny"], serde_json::json!(["**/_content.*"]));
    let areas: Vec<(String, String)> = settings["areas"]
        .as_array()
        .expect("areas")
        .iter()
        .map(|a| {
            (
                a["kind"].as_str().expect("kind").to_owned(),
                a["glob"].as_str().expect("glob").to_owned(),
            )
        })
        .collect();
    assert_eq!(
        areas,
        [
            ("content".to_owned(), "content/**".to_owned()),
            ("data".to_owned(), "data/**".to_owned()),
            ("i18n".to_owned(), "i18n/**".to_owned()),
            (
                "config".to_owned(),
                "config/_default/{params,menus,menu}.*".to_owned()
            ),
        ]
    );
    let content_ext = settings["areas"][0]["ext"].as_array().expect("ext");
    for e in ["md", "jpg", "webp", "pdf"] {
        assert!(content_ext.contains(&Value::from(e)), "{e}");
    }
    for e in ["html", "htm", "svg", "js", "css"] {
        assert!(!content_ext.contains(&Value::from(e)), "{e}");
    }
    assert_eq!(index_of(&worker)["api"], "/admin/api/");
}

#[test]
fn html_content_files_only_when_allowed() {
    let dir = project("");
    let config = std::fs::read_to_string(dir.path().join("config.toml")).expect("config");
    std::fs::write(
        dir.path().join("config.toml"),
        config.replace("[cms]\n", "[cms]\nhtml = true\n"),
    )
    .expect("write");
    let (sink, _) = publish(dir.path(), "production");
    let worker = sink.text("_worker.js");
    let settings = settings_of(&worker);
    let content_ext = settings["areas"][0]["ext"].as_array().expect("ext");
    assert!(content_ext.contains(&Value::from("html")));
}

#[test]
fn adapters_are_not_pages_in_any_case() {
    let dir = project("");
    write_files(
        dir.path(),
        &[
            ("content/books/_Content.html", "{# adapter #}"),
            ("content/books/_CONTENT.th.html", "{# adapter #}"),
        ],
    );
    let (sink, _) = publish(dir.path(), "production");
    let index = index_of(&sink.text("_worker.js"));
    let keys: Vec<&str> = index["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|e| e["key"].as_str().expect("key"))
        .collect();
    assert!(!keys.iter().any(|k| k.starts_with("books")), "{keys:?}");
}

#[test]
fn media_directory_and_its_names() {
    let dir = project("");
    let config = std::fs::read_to_string(dir.path().join("config.toml")).expect("config");
    std::fs::write(
        dir.path().join("config.toml"),
        config.replace("[cms]\n", "[cms]\nmedia = \"static/images/uploads\"\n"),
    )
    .expect("write");
    let (sink, _) = publish(dir.path(), "production");
    let index = index_of(&sink.text("_worker.js"));
    assert_eq!(index["media"], "static/images/uploads");
    assert_eq!(index["media_ref"], "/images/uploads/");
    let worker = sink.text("_worker.js");
    assert!(worker.contains("\"static/images/uploads/**\""), "{worker}");

    std::fs::write(
        dir.path().join("config.toml"),
        config.replace("[cms]\n", "[cms]\nmedia = \"layouts/x\"\n"),
    )
    .expect("write");
    let err = ssg_cms::publish(&load(dir.path(), "production"), &TestSink::default())
        .expect_err("media outside");
    assert!(err.to_string().contains("cms.media"), "{err}");
}

#[test]
fn a_role_that_reaches_no_area_is_an_error() {
    let dir = project("[cms.roles.dev]\nedit = [\"layouts/**\"]\n");
    let err = ssg_cms::publish(&load(dir.path(), "production"), &TestSink::default())
        .expect_err("layouts");
    let msg = err.to_string();
    assert!(
        msg.contains("cms.roles.dev.edit") && msg.contains("layouts/**"),
        "{msg}"
    );
}

#[test]
fn only_the_environment_with_the_table_gets_the_editor() {
    let dir = tempfile::tempdir().expect("tempdir");
    write_files(
        dir.path(),
        &[
            ("config.toml", "baseURL = \"https://example.org/\"\n"),
            (
                "config/production/cms.toml",
                &cms_toml("").replace("[cms]\n", "").replace("[cms.", "["),
            ),
            ("content/a.md", "---\ntitle: A\n---\n"),
        ],
    );
    let (_, out) = publish(dir.path(), "development");
    assert!(out.is_none(), "development has no [cms]");
    let (sink, out) = publish(dir.path(), "production");
    assert!(out.is_some(), "production has config/production/cms.toml");
    assert!(sink.paths().contains(&"_worker.js".to_owned()));
}

#[test]
fn the_project_directory_in_its_repository() {
    let repo = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir(repo.path().join(".git")).expect(".git");
    let site = repo.path().join("sites/main");
    write_files(
        &site,
        &[
            (
                "config.toml",
                &format!("baseURL = \"https://example.org/\"\n{}", cms_toml("")),
            ),
            ("content/a.md", "---\ntitle: A\n---\n"),
        ],
    );
    let (sink, out) = publish(&site, "production");
    assert!(out.expect("cms").warnings.is_empty());
    assert!(sink.text("_worker.js").contains("\"dir\": \"sites/main/\""));

    let lone = tempfile::tempdir().expect("tempdir");
    write_files(
        lone.path(),
        &[(
            "config.toml",
            &format!("baseURL = \"https://example.org/\"\n{}", cms_toml("")),
        )],
    );
    let (_, out) = publish(lone.path(), "production");
    let warnings = out.expect("cms").warnings;
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].id.as_deref(), Some("cms-no-repository"));
}

#[test]
fn keeps_a_static_assetsignore_and_warns_about_a_page_it_replaces() {
    let dir = project("");
    let sink = TestSink::default();
    sink.write(&OutputPath::new(".assetsignore"), b"drafts/*")
        .expect("write");
    sink.write(&OutputPath::new("_headers"), b"/*\n  X-Robots-Tag: noindex")
        .expect("write");
    sink.write(&OutputPath::new("admin/index.html"), b"<p>a page</p>")
        .expect("write");
    let out = ssg_cms::publish(&load(dir.path(), "production"), &sink)
        .expect("publish")
        .expect("cms");
    assert_eq!(
        sink.text(".assetsignore"),
        "drafts/*\n_worker.js\n.assetsignore\n"
    );
    assert_eq!(out.warnings.len(), 1);
    assert_eq!(out.warnings[0].id.as_deref(), Some("cms-path-taken"));

    // A second build over the first (a disk sink keeps the files) warns about nothing.
    let again = ssg_cms::publish(&load(dir.path(), "production"), &sink)
        .expect("publish")
        .expect("cms");
    assert!(again.warnings.is_empty(), "{:?}", again.warnings);
    assert_eq!(
        sink.text(".assetsignore"),
        "drafts/*\n_worker.js\n.assetsignore\n"
    );
    let headers = sink.text("_headers");
    assert!(
        headers.starts_with("/*\n  X-Robots-Tag: noindex\n/admin/*\n"),
        "{headers}"
    );
    assert_eq!(headers.matches("/admin/*").count(), 1, "{headers}");
}
