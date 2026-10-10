//! The API of the CMS editor of `[cms]`, which the server answers from the git repository the
//! site is in (`ssg_cms::local`, whose tests cover the requests themselves).

use std::path::Path;
use std::process::Command;

use crate::{get, request, serve, site};

const SITE: &str = r#"
-- config.toml --
baseURL = "https://example.org/"
title = "Editor"
disableKinds = ["taxonomy", "term", "sitemap", "rss", "robotsTXT"]
[cms.git]
repo = "owner/site"
[cms.login]
provider = "github"
[cms.roles.owner]
edit = ["**"]
publish = true
-- layouts/home.html --
<html><body>{{ page.title }}</body></html>
-- content/_index.md --
---
title: Home
---
"#;

/// git in `dir`, or `false` without git on `PATH`.
fn git(dir: &Path, args: &[&str]) -> bool {
    let Ok(out) = Command::new("git").arg("-C").arg(dir).args(args).output() else {
        return false;
    };
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    true
}

#[test]
fn answers_the_editor_s_api_from_the_repository() {
    let dir = site(SITE);
    if !git(dir.path(), &["init", "-q", "-b", "main"]) {
        eprintln!("SKIPPED the editor's API: no git on PATH");
        return;
    }
    for (key, value) in [
        ("user.name", "Ann"),
        ("user.email", "ann@example.org"),
        ("commit.gpgsign", "false"),
    ] {
        git(dir.path(), &["config", key, value]);
    }
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-qm", "init"]);
    let (server, _events) = serve(dir.path(), |_| {});
    let addr = server.local_addrs()[0];

    assert!(get(addr, "/admin/", &[]).text().contains("cms.js"));
    let me = get(addr, "/admin/api/me", &[]);
    assert_eq!(me.status, 200, "{}", me.text());
    assert_eq!(
        me.header("content-type"),
        Some("application/json; charset=utf-8")
    );
    assert_eq!(me.header("cache-control"), Some("no-store"));
    assert!(me.text().contains(r#""login":"local""#), "{}", me.text());
    let file = get(addr, "/admin/api/file?path=content%2F_index.md", &[]);
    assert!(file.text().contains(r#""sha":"#), "{}", file.text());
    // A POST from another page (no Origin) changes nothing.
    let post = request(
        addr,
        "POST",
        "/admin/api/save",
        &[("Content-Type", "application/json")],
    );
    assert_eq!(post.status, 403, "{}", post.text());
    server.shutdown();
}
