//! T38 walking skeleton: the testsite (`tools/rust-port/i01/sites.py make testsite` with the
//! Tera layouts of `sites/testsite`) built into a `MemorySink`, and its file list (L1) and
//! bytes compared with the Go build's (`tests/it/testsite-go.txtar`).

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use ssg_base::url::BaseUrl;
use ssg_build::{BuildRequest, LiveReload, SinkKind, build};
use ssg_testkit::fixture::{go_output_as_built_here, repo_dir, repo_file};
use ssg_testkit::txtar::Archive;

/// Every file of the Go build of the same site (built with `-d public`), 55 files (Go also wrote
/// its stats file next to its configuration; fugo writes none).
fn go_build() -> Archive {
    let path = repo_dir().join("crates/build/tests/it/testsite-go.txtar");
    Archive::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Files whose bytes still differ from Go's, with the reason (none since F3).
const KNOWN_BYTE_DIFFS: &[(&str, &str)] = &[];

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("mkdir");
    for e in fs::read_dir(from).expect("read_dir") {
        let e = e.expect("entry");
        let name = e.file_name();
        if name.to_string_lossy().starts_with('.') || name == "public" || name == "resources" {
            continue;
        }
        let (src, dst) = (e.path(), to.join(&name));
        if e.file_type().expect("type").is_dir() {
            copy_tree(&src, &dst);
        } else {
            fs::copy(&src, &dst).expect("copy");
        }
    }
}

/// Go's txtar format: `-- name --` lines start files; every file ends with a newline.
fn write_txtar(archive: &str, to: &Path) {
    let mut files: Vec<(String, String)> = Vec::new();
    for line in archive.split('\n') {
        if line.len() > 6 && line.starts_with("-- ") && line.ends_with(" --") {
            files.push((line[3..line.len() - 3].trim().to_owned(), String::new()));
        } else if let Some((_, body)) = files.last_mut() {
            body.push_str(line);
            body.push('\n');
        }
    }
    for (name, mut body) in files {
        while body.ends_with("\n\n") {
            body.pop();
        }
        let path = to.join(&name);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(path, body).expect("write");
    }
}

/// The testsite as `sites.py make testsite` writes it, with the Go layouts replaced by the Tera
/// layouts of `sites/testsite/layouts`.
fn testsite(dir: &Path) {
    let root = repo_dir();
    copy_tree(&repo_file("testsite"), dir);
    let txtar = fs::read_to_string(root.join("tools/rust-port/i01/testsite.txtar")).expect("txtar");
    write_txtar(&txtar, dir);
    fs::remove_dir_all(dir.join("layouts")).expect("remove Go layouts");
    copy_tree(&root.join("sites/testsite/layouts"), &dir.join("layouts"));
}

fn build_testsite() -> (tempfile::TempDir, ssg_build::BuildReport) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let site = tmp.path().join("testsite");
    testsite(&site);
    let report = build(BuildRequest {
        source: site,
        sink: SinkKind::Memory,
        clock: Some("2026-01-01T00:00:00Z".parse().expect("clock")),
        ..BuildRequest::default()
    })
    .unwrap_or_else(|e| panic!("testsite build: {e}"));
    (tmp, report)
}

#[test]
fn testsite_l1() {
    let (_tmp, report) = build_testsite();
    let mem = report.memory.as_ref().expect("memory sink");
    let got: BTreeSet<String> = mem
        .paths()
        .iter()
        .map(|p| p.relative().to_owned())
        .collect();
    let want: BTreeSet<String> = go_build().files.into_iter().map(|f| f.name).collect();
    let missing: Vec<&String> = want.difference(&got).collect();
    let extra: Vec<&String> = got.difference(&want).collect();
    println!(
        "L1 testsite: {} files (Go {}); {} equal, missing {missing:?}, extra {extra:?}",
        got.len(),
        want.len(),
        got.intersection(&want).count()
    );
    println!(
        "pages {}, outputs {}, aliases {}, collisions {:?}",
        report.pages, report.outputs, report.aliases, report.collisions
    );
    for d in &report.diagnostics {
        println!("diagnostic: {d:?}");
    }
    // For a manual diff against the Go build: FUGO_T38_OUT=<dir>.
    if let Some(dir) = std::env::var_os("FUGO_T38_OUT") {
        mem.write_to(Path::new(&dir))
            .expect("write the memory sink");
    }
    assert_eq!(got, want, "L1 file list differs from Go's");
}

/// L2 on the testsite: the bytes of every file equal the Go build's.
#[test]
fn testsite_bytes() {
    let (_tmp, report) = build_testsite();
    let mem = report.memory.as_ref().expect("memory sink");
    let go = go_build();
    let differ: Vec<&str> = go
        .files
        .iter()
        .filter(|f| {
            mem.get(&f.name)
                .is_none_or(|got| *got != *go_output_as_built_here(&f.data).as_bytes())
        })
        .map(|f| f.name.as_str())
        .collect();
    println!(
        "L2 testsite: {} of {} files byte-identical to Go's; differ: {differ:?}",
        go.files.len() - differ.len(),
        go.files.len()
    );
    let known: Vec<&str> = KNOWN_BYTE_DIFFS.iter().map(|(p, _)| *p).collect();
    assert_eq!(
        differ, known,
        "files whose bytes differ from Go's (FUGO_T38_OUT=<dir> writes ours for a diff)"
    );
}

#[test]
fn testsite_contents() {
    let (_tmp, report) = build_testsite();
    let mem = report.memory.as_ref().expect("memory sink");
    let text = |p: &str| mem.text(p).unwrap_or_else(|| panic!("no {p}"));

    // Canonified links, the paginator, taxonomies and the content of a single page.
    let posts = text("posts/index.html");
    assert!(
        posts.contains(r#"<body class="kind-section lang-en" id="top">"#),
        "{posts}"
    );
    assert!(
        posts.contains(r#"<a class="next" href="https://example.org/posts/page/2/">next</a>"#),
        "{posts}"
    );
    assert!(
        posts.contains(r#"<li><a href="https://example.org/tags/b/">b (2)</a></li>"#),
        "{posts}"
    );
    assert!(
        posts.contains(r#"<span class="den">2021-01-04</span> Jan 4, 2021"#),
        "{posts}"
    );
    let page2 = text("posts/page/2/index.html");
    assert!(page2.contains("https://example.org/posts/one/"), "{page2}");
    assert!(!page2.contains(r#"class="next""#), "{page2}");
    let two = text("posts/two/index.html");
    assert!(
        two.contains(
            r#"<a class="prev" href="https://example.org/posts/one/">One &amp; &lt;Two&gt;</a>"#
        ),
        "{two}"
    );
    assert!(
        two.contains(r#"<main><p>Second <q class="quote">q</q>.</p>"#),
        "{two}"
    );
    let first = text("first-post/index.html");
    assert!(
        first.contains(r#"<a hreflang="nn" href="https://example.org/nn/first-post/">Nynorsk</a>"#),
        "{first}"
    );

    // Home in every format; the JSON home lists the regular pages newest first.
    assert_eq!(
        text("index.json").trim_end(),
        concat!(
            r#"{"title": "Home", "n": 1.50000, "pages": [{ "url" : "/posts/three/", "title": "Three" }, "#,
            r#"{ "url" : "/posts/two/", "title": "Two" }, { "url" : "/posts/one/", "title": "One \u0026 \u003cTwo\u003e" }, "#,
            r#"{ "url" : "/first-post/", "title": "My First Post" }, { "url" : "/about/", "title": "About" }]}"#
        )
    );
    let rss = text("index.xml");
    assert!(rss.contains("<link>https://example.org/</link>"), "{rss}");
    assert!(rss.contains("<title>Three</title>"), "{rss}");

    // Aliases, the language redirect, standalone pages.
    assert!(
        text("old-about/index.html").contains(r#"content="0; url=https://example.org/about/""#)
    );
    assert!(text("page/1/index.html").contains(r#"content="0; url=https://example.org/""#));
    assert!(text("en/index.html").contains(r#"content="0; url=https://example.org/""#));
    assert!(text("nn/sitemap.xml").contains("<loc>https://example.org/nn/om/</loc>"));
    assert!(text("sitemap.xml").contains("<loc>https://example.org/nn/sitemap.xml</loc>"));
    assert_eq!(text("robots.txt").trim_end(), "User-agent: *");
    assert!(text("css/site.css").contains('{'));
}

/// The `server` command's request: a configuration loaded by the caller with the base URLs
/// pointed at the server, and the LiveReload script in every HTML page but the aliases.
#[test]
fn testsite_for_the_server() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let site = tmp.path().join("testsite");
    testsite(&site);
    let mut cfg = ssg_config::load(&ssg_config::LoadOptions {
        source: site,
        ..ssg_config::LoadOptions::default()
    })
    .expect("config");
    for s in cfg.sites.iter_mut() {
        s.base_url = BaseUrl::parse("http://localhost:1313/").expect("url");
    }
    let report = build(BuildRequest {
        config: Some(Arc::new(cfg)),
        live_reload: Some(LiveReload::default()),
        sink: SinkKind::Memory,
        clock: Some("2026-01-01T00:00:00Z".parse().expect("clock")),
        ..BuildRequest::default()
    })
    .unwrap_or_else(|e| panic!("testsite build: {e}"));
    let mem = report.memory.as_ref().expect("memory sink");
    let text = |p: &str| mem.text(p).unwrap_or_else(|| panic!("no {p}"));
    let script = concat!(
        r#"<script src="/livereload.js?mindelay=10&amp;v=2&amp;port=1313&amp;path=livereload" "#,
        "data-no-instant defer></script>"
    );
    let home = text("index.html");
    assert!(
        home.starts_with(&format!(
            "<!DOCTYPE html>\n<html lang=\"en\">\n<head>{script}\n"
        )),
        "{home}"
    );
    assert!(
        home.contains(r#"<link rel="stylesheet" href="http://localhost:1313/css/site.css">"#),
        "{home}"
    );
    assert!(text("nn/om/index.html").contains(script));
    assert!(text("404.html").starts_with(&format!("<html>{script}<body class=\"nf\">")));
    for alias in ["old-about/index.html", "page/1/index.html", "en/index.html"] {
        let a = text(alias);
        assert!(!a.contains("livereload"), "{alias}: {a}");
        assert!(a.contains("http://localhost:1313/"), "{alias}: {a}");
    }
    assert!(!text("index.xml").contains("livereload"));
    assert!(!text("index.json").contains("livereload"));
    let model = report.model.as_ref().expect("model");
    assert!(model.pages.iter().any(|p| {
        p.source
            .as_ref()
            .is_some_and(|s| s.file.rel == "posts/one.md")
    }));
}
