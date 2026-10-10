//! The server on a small site: what it serves, and what each kind of change does.

use std::fs;

use ssg_serve::{HttpCache, LiveReloadOptions, Target, Watch};

use crate::{LiveReload, PATIENCE, get, request, serve, site, write};

mod cms;
mod sites;
mod watching;

const SITE: &str = r#"
-- config.toml --
baseURL = "https://example.org/"
title = "Serve"
disableKinds = ["taxonomy", "term", "sitemap", "rss", "robotsTXT"]
-- layouts/home.html --
<!DOCTYPE html>
<html><head><title>{{ site.title }}</title>{% set css = get_asset(path="css/main.css") %}<link rel="stylesheet" href="{{ css.rel_permalink }}"></head>
<body class="home">{{ page.content }}{% for p in site.regular_pages %}<a href="{{ p.rel_permalink }}">{{ p.title }}</a>{% endfor %}</body></html>
-- layouts/single.html --
<!DOCTYPE html>
<html><head><title>{{ page.title }}</title></head><body class="single">{{ page.content }}</body></html>
-- layouts/404.html --
<html><body class="not-found">custom 404</body></html>
-- content/_index.md --
---
title: Home
---
Home text.
-- content/about.md --
---
title: About
aliases: [/old-about/]
---
About text.
-- content/posts/one/index.md --
---
title: One
---
One text.
-- assets/css/main.css --
body { color: red }
-- static/css/a.css --
a { color: blue }
-- static/media/clip.bin --
0123456789
"#;

/// Pages, directory redirects, static files, content types, the 404 page, `livereload.js`,
/// the WebSocket handshake and its origin check, `HEAD` and byte ranges.
#[test]
fn serves_the_site() {
    let dir = site(SITE);
    let (server, _events) = serve(dir.path(), |_| {});
    let addr = server.local_addrs()[0];
    let port = addr.port();
    assert_eq!(server.urls(), [format!("http://localhost:{port}/")]);

    let home = get(addr, "/", &[]);
    assert_eq!(home.status, 200);
    assert_eq!(
        home.header("content-type"),
        Some("text/html; charset=utf-8")
    );
    let script = format!(
        r#"<script src="/livereload.js?mindelay=10&amp;v=2&amp;port={port}&amp;path=livereload" data-no-instant defer></script>"#
    );
    assert!(
        home.text().starts_with(&format!(
            "<!DOCTYPE html>\n<html><head>{script}<title>Serve</title>"
        )),
        "{}",
        home.text()
    );
    assert!(home.text().contains(r#"<a href="/about/">About</a>"#));

    let about = get(addr, "/about/", &[]);
    assert_eq!(about.status, 200);
    assert!(
        about.text().contains("<p>About text.</p>"),
        "{}",
        about.text()
    );
    // Go's file server: a directory without its slash, and `index.html`, redirect.
    let r = get(addr, "/about?x=1", &[]);
    assert_eq!((r.status, r.header("location")), (301, Some("about/?x=1")));
    let r = get(addr, "/about/index.html", &[]);
    assert_eq!((r.status, r.header("location")), (301, Some("./")));
    // An alias page has no LiveReload script.
    let alias = get(addr, "/old-about/", &[]);
    assert_eq!(alias.status, 200);
    assert!(!alias.text().contains("livereload"), "{}", alias.text());
    assert!(
        alias
            .text()
            .contains(&format!("http://localhost:{port}/about/")),
        "{}",
        alias.text()
    );

    let css = get(addr, "/css/a.css", &[]);
    assert_eq!(css.status, 200);
    assert_eq!(css.header("content-type"), Some("text/css; charset=utf-8"));
    assert_eq!(css.text(), "a { color: blue }\n");
    let asset = get(addr, "/css/main.css", &[]);
    assert_eq!(asset.text(), "body { color: red }\n");

    // Misses: the site's 404 page for navigations, Go's plain 404 for anything else.
    let missing = get(addr, "/no/such/page/", &[]);
    assert_eq!(missing.status, 404);
    assert_eq!(
        missing.header("content-type"),
        Some("text/html; charset=utf-8")
    );
    assert!(
        missing.text().starts_with(&format!(
            "<html>{script}<body class=\"not-found\">custom 404"
        )),
        "{}",
        missing.text()
    );
    let missing = get(addr, "/img/nope.png", &[]);
    assert_eq!(missing.status, 404);
    assert_eq!(missing.text(), "404 page not found\n");
    let missing = get(addr, "/img/nope.png", &[("Sec-Fetch-Mode", "navigate")]);
    assert!(missing.text().contains("custom 404"));

    let js = get(
        addr,
        "/livereload.js?mindelay=10&v=2&port=1&path=livereload",
        &[],
    );
    assert_eq!(js.status, 200);
    assert_eq!(js.header("content-type"), Some("text/javascript"));
    assert!(js.text().contains("LiveReload") && js.text().contains("__ssg_navigate"));

    let mut lr = LiveReload::connect(addr, "/livereload");
    assert_eq!(lr.next(std::time::Duration::from_millis(200)), None);
    let refused = get(
        addr,
        "/livereload",
        &[
            ("Upgrade", "websocket"),
            ("Connection", "Upgrade"),
            ("Sec-WebSocket-Version", "13"),
            ("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ=="),
            ("Origin", "http://evil.example"),
        ],
    );
    assert_eq!(refused.status, 403);

    let head = request(addr, "HEAD", "/css/a.css", &[]);
    assert_eq!(head.status, 200);
    assert_eq!(head.header("content-length"), Some("18"));
    assert!(head.body.is_empty());
    let part = get(addr, "/media/clip.bin", &[("Range", "bytes=2-5")]);
    assert_eq!(part.status, 206);
    assert_eq!(part.header("content-range"), Some("bytes 2-5/11"));
    assert_eq!(part.text(), "2345");
    let part = get(addr, "/media/clip.bin", &[("Range", "bytes=40-")]);
    assert_eq!(part.status, 416);
    server.shutdown();
}

/// A content edit rebuilds the site and reloads the browsers; the new page is served.
#[test]
fn content_change_rebuilds_and_reloads() {
    let dir = site(SITE);
    let (server, events) = serve(dir.path(), |_| {});
    let addr = server.local_addrs()[0];
    let mut lr = LiveReload::connect(addr, "/livereload");
    write(
        &dir.path().join("content/about.md"),
        "---\ntitle: About\n---\nAbout, edited.\n",
    );
    let command = lr.expect();
    assert_eq!(
        command,
        r#"{"command":"reload","path":"/x.js","originalPath":"","liveCSS":true,"liveImg":true}"#
    );
    assert!(get(addr, "/about/", &[]).text().contains("About, edited."));
    assert_eq!(events.count("built"), 2, "{:#?}", events.lines());
    // The alias went with the front matter.
    assert_eq!(get(addr, "/old-about/", &[]).status, 404);
    server.shutdown();
}

/// A static edit copies the file without a build and reloads just that path; a removed
/// static file is removed.
#[test]
fn static_change_copies_without_a_rebuild() {
    let dir = site(SITE);
    let (server, events) = serve(dir.path(), |_| {});
    let addr = server.local_addrs()[0];
    let mut lr = LiveReload::connect(addr, "/livereload");
    write(&dir.path().join("static/css/a.css"), "a { color: green }\n");
    assert_eq!(
        lr.expect(),
        r#"{"command":"reload","path":"/css/a.css","originalPath":"","liveCSS":true,"liveImg":true}"#
    );
    assert_eq!(get(addr, "/css/a.css", &[]).text(), "a { color: green }\n");
    assert_eq!(events.count("built"), 1, "{:#?}", events.lines());
    assert_eq!(events.count("static synced 1"), 1);

    fs::remove_file(dir.path().join("static/css/a.css")).expect("remove");
    assert!(lr.expect().contains(r#""path":"/css/a.css""#));
    assert_eq!(get(addr, "/css/a.css", &[]).status, 404);
    write(&dir.path().join("static/new/b.txt"), "b\n");
    assert!(lr.expect().contains(r#""path":"/new/b.txt""#));
    assert_eq!(get(addr, "/new/b.txt", &[]).text(), "b\n");
    assert_eq!(events.count("built"), 1, "{:#?}", events.lines());
    server.shutdown();
}

/// Go's reload rules on what a rebuild changed: a stylesheet alone is reloaded in place, a
/// single other file by its path, several files fully, nothing not at all.
#[test]
fn reloads_follow_what_the_build_changed() {
    let dir = site(SITE);
    let (server, events) = serve(dir.path(), |_| {});
    let addr = server.local_addrs()[0];
    let mut lr = LiveReload::connect(addr, "/livereload");

    write(
        &dir.path().join("assets/css/main.css"),
        "body { color: navy }\n",
    );
    assert!(lr.expect().contains(r#""path":"/css/main.css""#));
    assert_eq!(
        get(addr, "/css/main.css", &[]).text(),
        "body { color: navy }\n"
    );

    write(
        &dir.path().join("layouts/404.html"),
        "<html><body class=\"not-found\">gone</body></html>\n",
    );
    assert!(lr.expect().contains(r#""path":"/404.html""#));

    write(
        &dir.path().join("layouts/single.html"),
        "<html><body class=\"single2\">{{ page.content }}</body></html>\n",
    );
    assert!(lr.expect().contains(r#""path":"/x.js""#));
    assert!(get(addr, "/about/", &[]).text().contains("single2"));

    // The same bytes again: a build, no reload.
    let builds = events.count("built");
    write(
        &dir.path().join("layouts/single.html"),
        "<html><body class=\"single2\">{{ page.content }}</body></html>\n",
    );
    events.wait_for("built", builds + 1);
    assert_eq!(lr.next(std::time::Duration::from_millis(300)), None);
    server.shutdown();
}

/// A failing build keeps the last good site; the fix is picked up.
#[test]
fn a_broken_build_keeps_the_last_good_site() {
    let dir = site(SITE);
    let (server, events) = serve(dir.path(), |_| {});
    let addr = server.local_addrs()[0];
    let mut lr = LiveReload::connect(addr, "/livereload");
    write(
        &dir.path().join("layouts/single.html"),
        "<html><body>{{ page.title </body></html>\n",
    );
    events.wait_for("build failed", 1);
    assert!(
        events
            .lines()
            .iter()
            .any(|l| l.starts_with("build failed") && l.contains("single.html")),
        "{:#?}",
        events.lines()
    );
    assert_eq!(lr.next(std::time::Duration::from_millis(300)), None);
    assert!(get(addr, "/about/", &[]).text().contains("About text."));
    write(
        &dir.path().join("layouts/single.html"),
        "<html><body class=\"fixed\">{{ page.content }}</body></html>\n",
    );
    assert!(lr.expect().contains(r#""path":"/x.js""#));
    assert!(get(addr, "/about/", &[]).text().contains("fixed"));
    server.shutdown();
}

/// `--navigateToChanged`: the browsers go to the changed page, on its server's port.
#[test]
fn navigate_to_the_changed_page() {
    let dir = site(SITE);
    let (server, _events) = serve(dir.path(), |o| {
        o.live_reload = Some(LiveReloadOptions {
            navigate_to_changed: true,
            ..LiveReloadOptions::default()
        });
    });
    let addr = server.local_addrs()[0];
    let mut lr = LiveReload::connect(addr, "/livereload");
    write(
        &dir.path().join("content/posts/one/index.md"),
        "---\ntitle: One\n---\nOne, edited.\n",
    );
    assert_eq!(
        lr.expect(),
        format!(
            r#"{{"command":"reload","path":"__ssg_navigate/posts/one/","originalPath":"","liveCSS":true,"liveImg":true, "overrideURL": {}}}"#,
            addr.port()
        )
    );
    server.shutdown();
}
