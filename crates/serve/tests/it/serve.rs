//! The server on a small site: what it serves, and what each kind of change does.

use std::fs;

use ssg_serve::{HttpCache, LiveReloadOptions, Target, Watch};

use crate::{LiveReload, PATIENCE, get, request, serve, site, write};

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

/// A configuration change reloads the configuration, rebuilds and reloads fully; a
/// configuration that does not load pauses the site until it does.
#[test]
fn config_change_reloads_the_configuration() {
    let dir = site(SITE);
    let (server, events) = serve(dir.path(), |_| {});
    let addr = server.local_addrs()[0];
    let mut lr = LiveReload::connect(addr, "/livereload");
    let config = dir.path().join("config.toml");
    let text = fs::read_to_string(&config).expect("config");
    write(
        &config,
        &text.replace("title = \"Serve\"", "title = \"Served\""),
    );
    assert!(lr.expect().contains(r#""path":"/x.js""#));
    assert!(get(addr, "/", &[]).text().contains("<title>Served</title>"));

    write(&config, "baseURL = [\n");
    events.wait_for("config failed", 1);
    // Paused: content changes wait for a configuration that loads.
    write(
        &dir.path().join("content/about.md"),
        "---\ntitle: About\n---\nPaused.\n",
    );
    assert_eq!(lr.next(std::time::Duration::from_millis(1500)), None);
    write(&config, &text);
    assert!(lr.expect().contains(r#""path":"/x.js""#));
    assert!(get(addr, "/about/", &[]).text().contains("Paused."));
    assert!(get(addr, "/", &[]).text().contains("<title>Serve</title>"));
    server.shutdown();
}

/// The project's `.env` files are watched like configuration files: a change reloads the
/// configuration, and the templates read the new values.
#[test]
fn env_file_change_reloads() {
    let dir = site(concat!(
        "-- config.toml --\n",
        "baseURL = \"https://example.org/\"\n",
        "disableKinds = [\"taxonomy\", \"term\", \"sitemap\", \"rss\", \"robotsTXT\"]\n",
        "-- layouts/home.html --\n",
        "<html><body>[{{ get_env(name=\"SERVE_ENV_FILE_GREETING\") }}]</body></html>\n",
        "-- .env --\n",
        "SERVE_ENV_FILE_GREETING=hello\n",
    ));
    let (server, _events) = serve(dir.path(), |_| {});
    let addr = server.local_addrs()[0];
    let mut lr = LiveReload::connect(addr, "/livereload");
    assert!(get(addr, "/", &[]).text().contains("[hello]"));
    write(&dir.path().join(".env"), "SERVE_ENV_FILE_GREETING=bye\n");
    assert!(lr.expect().contains(r#""path":"/x.js""#));
    assert!(get(addr, "/", &[]).text().contains("[bye]"));
    // The server's environment is `development`: `.env.development` wins over `.env`.
    write(
        &dir.path().join(".env.development"),
        "SERVE_ENV_FILE_GREETING=dev\n",
    );
    assert!(lr.expect().contains(r#""path":"/x.js""#));
    assert!(get(addr, "/", &[]).text().contains("[dev]"));
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

/// `--render-to-disk`: the build writes the publish directory and the server reads it;
/// static changes are copied there.
#[test]
fn render_to_disk() {
    let dir = site(SITE);
    let (server, events) = serve(dir.path(), |o| o.target = Target::Disk);
    let addr = server.local_addrs()[0];
    let public = dir.path().join("public");
    let on_disk = fs::read_to_string(public.join("about/index.html")).expect("about");
    assert!(on_disk.contains("livereload.js"), "{on_disk}");
    assert_eq!(get(addr, "/about/", &[]).text(), on_disk);
    let mut lr = LiveReload::connect(addr, "/livereload");
    write(&dir.path().join("static/css/a.css"), "a { color: teal }\n");
    assert!(lr.expect().contains(r#""path":"/css/a.css""#));
    assert_eq!(
        fs::read_to_string(public.join("css/a.css")).expect("css"),
        "a { color: teal }\n"
    );
    write(
        &dir.path().join("content/about.md"),
        "---\ntitle: About\n---\nOn disk.\n",
    );
    assert!(lr.expect().contains(r#""path":"/x.js""#));
    assert!(get(addr, "/about/", &[]).text().contains("On disk."));
    assert_eq!(events.count("built"), 2);
    server.shutdown();
}

/// A base URL with a path serves the site below it; `--noHTTPCache` headers; a site built
/// without live reload has no script and no LiveReload endpoints.
#[test]
fn base_path_caching_and_no_live_reload() {
    let dir = site(&SITE.replace(
        "baseURL = \"https://example.org/\"",
        "baseURL = \"https://example.org/docs/\"",
    ));
    let (server, _events) = serve(dir.path(), |o| {
        o.live_reload = None;
        o.http_cache = HttpCache::Disabled;
        o.watch = Watch::Off;
    });
    let addr = server.local_addrs()[0];
    assert_eq!(
        server.urls(),
        [format!("http://localhost:{}/docs/", addr.port())]
    );
    let home = get(addr, "/docs/", &[]);
    assert_eq!(home.status, 200);
    assert!(!home.text().contains("livereload"), "{}", home.text());
    assert_eq!(
        home.header("cache-control"),
        Some("no-store, no-cache, must-revalidate, max-age=0")
    );
    assert_eq!(home.header("pragma"), Some("no-cache"));
    assert!(home.text().contains(r#"<a href="/docs/about/">About</a>"#));
    let r = get(addr, "/docs", &[]);
    assert_eq!((r.status, r.header("location")), (301, Some("/docs/")));
    assert_eq!(get(addr, "/about/", &[]).text(), "404 page not found\n");
    assert_eq!(get(addr, "/docs/livereload.js", &[]).status, 404);
    assert!(get(addr, "/docs/nope/", &[]).text().contains("custom 404"));
    server.shutdown();
}

/// A multihost site: one listener per language, each serving its language's directory with
/// its own port in the base URL and the LiveReload script.
#[test]
fn multihost_sites_get_a_listener_each() {
    let dir = site(concat!(
        "-- config.toml --\n",
        "defaultContentLanguage = \"en\"\n",
        "disableKinds = [\"taxonomy\", \"term\", \"sitemap\", \"rss\", \"robotsTXT\"]\n",
        "[languages.en]\nbaseURL = \"https://en.example.org/\"\ntitle = \"English\"\nweight = 1\n",
        "[languages.fr]\nbaseURL = \"https://fr.example.org/\"\ntitle = \"Français\"\nweight = 2\n",
        "-- layouts/home.html --\n",
        "<html><head></head><body>{{ site.title }} {{ site.base_url }}</body></html>\n",
        "-- layouts/404.html --\n",
        "<html><body>404 {{ site.title }}</body></html>\n",
        "-- content/_index.md --\n---\ntitle: Home\n---\n",
        "-- content/_index.fr.md --\n---\ntitle: Accueil\n---\n",
    ));
    let (server, _events) = serve(dir.path(), |_| {});
    let addrs = server.local_addrs().to_vec();
    assert_eq!(addrs.len(), 2);
    for (addr, title) in addrs.iter().zip(["English", "Français"]) {
        let home = get(*addr, "/", &[]);
        assert_eq!(home.status, 200, "{title}");
        let text = home.text();
        assert!(
            text.contains(&format!("{title} http://localhost:{}/", addr.port())),
            "{text}"
        );
        assert!(
            text.contains(&format!("port={}&amp;path=livereload", addr.port())),
            "{text}"
        );
        assert!(
            get(*addr, "/nope/", &[])
                .text()
                .contains(&format!("404 {title}"))
        );
        LiveReload::connect(*addr, "/livereload");
    }
    server.shutdown();
}

/// A multilingual site on one host: a miss below a language's directory gets that
/// language's 404 page, any other miss the default language's.
#[test]
fn per_language_404_pages() {
    let dir = site(concat!(
        "-- config.toml --\n",
        "baseURL = \"https://example.org/\"\n",
        "disableKinds = [\"taxonomy\", \"term\", \"sitemap\", \"rss\", \"robotsTXT\"]\n",
        "[languages.en]\nweight = 1\n[languages.nn]\nweight = 2\n",
        "-- layouts/home.html --\n<html><body>{{ page.lang }}</body></html>\n",
        "-- layouts/404.html --\n<html><body>404 {{ page.lang }}</body></html>\n",
        "-- content/_index.md --\n---\ntitle: Home\n---\n",
        "-- content/_index.nn.md --\n---\ntitle: Heim\n---\n",
    ));
    let (server, _events) = serve(dir.path(), |o| o.watch = Watch::Off);
    let addr = server.local_addrs()[0];
    assert!(get(addr, "/nn/", &[]).text().contains("<body>nn</body>"));
    for (path, lang) in [
        ("/nope/", "en"),
        ("/nn/nope/", "nn"),
        ("/nn", "nn"),
        ("/nnx/", "en"),
    ] {
        let r = get(addr, path, &[]);
        assert_eq!(r.status, if path == "/nn" { 301 } else { 404 }, "{path}");
        if r.status == 404 {
            assert!(
                r.text().contains(&format!("404 {lang}")),
                "{path}: {}",
                r.text()
            );
        }
    }
    server.shutdown();
}

/// A component directory created while serving (the site had no `static/`) is copied and
/// watched from then on.
#[test]
fn a_new_static_directory_is_watched() {
    let dir = site(SITE);
    fs::remove_dir_all(dir.path().join("static")).expect("remove static");
    let (server, events) = serve(dir.path(), |_| {});
    let addr = server.local_addrs()[0];
    let mut lr = LiveReload::connect(addr, "/livereload");
    assert_eq!(get(addr, "/new.txt", &[]).status, 404);
    write(&dir.path().join("static/new.txt"), "one\n");
    assert!(lr.expect().contains(r#""path":"/new.txt""#));
    assert_eq!(get(addr, "/new.txt", &[]).text(), "one\n");
    // The new directory is watched itself now.
    write(&dir.path().join("static/new.txt"), "two\n");
    assert!(lr.expect().contains(r#""path":"/new.txt""#));
    assert_eq!(get(addr, "/new.txt", &[]).text(), "two\n");
    assert_eq!(events.count("built"), 1, "{:#?}", events.lines());
    server.shutdown();
}

/// A theme's layouts and configuration are watched; an edit of the project's `config.toml` is a
/// configuration change.
#[test]
fn theme_and_new_config_files_are_watched() {
    let dir = site(concat!(
        "-- config.toml --\n",
        "baseURL = \"https://example.org/\"\ntitle = \"Site\"\ntheme = \"t\"\n",
        "disableKinds = [\"taxonomy\", \"term\", \"sitemap\", \"rss\", \"robotsTXT\", \"404\"]\n",
        "-- themes/t/config.toml --\n[params]\ncolor = \"red\"\n",
        "-- themes/t/layouts/home.html --\n",
        "<html><head></head><body>{{ site.title }} {{ site.params.color }}</body></html>\n",
        "-- content/_index.md --\n---\ntitle: Home\n---\n",
    ));
    let (server, _events) = serve(dir.path(), |_| {});
    let addr = server.local_addrs()[0];
    let mut lr = LiveReload::connect(addr, "/livereload");
    assert!(get(addr, "/", &[]).text().contains("Site red"));

    write(
        &dir.path().join("themes/t/layouts/home.html"),
        "<html><head></head><body>theme {{ site.title }} {{ site.params.color }}</body></html>\n",
    );
    assert!(lr.expect().contains(r#""path":"/index.html""#));
    assert!(get(addr, "/", &[]).text().contains("theme Site red"));

    write(
        &dir.path().join("themes/t/config.toml"),
        "[params]\ncolor = \"blue\"\n",
    );
    assert!(lr.expect().contains(r#""path":"/x.js""#));
    assert!(get(addr, "/", &[]).text().contains("theme Site blue"));

    let text = fs::read_to_string(dir.path().join("config.toml")).expect("config.toml");
    write(
        &dir.path().join("config.toml"),
        &text.replace("title = \"Site\"", "title = \"Neo\""),
    );
    assert!(lr.expect().contains(r#""path":"/x.js""#));
    assert!(get(addr, "/", &[]).text().contains("theme Neo blue"));
    server.shutdown();
}

/// Polling notices changes too.
#[test]
fn polling_watcher() {
    let dir = site(SITE);
    let (server, _events) = serve(dir.path(), |o| {
        o.watch = Watch::Poll(std::time::Duration::from_millis(100));
    });
    let addr = server.local_addrs()[0];
    let mut lr = LiveReload::connect(addr, "/livereload");
    // Let the poller take its first snapshot.
    std::thread::sleep(std::time::Duration::from_millis(300));
    write(
        &dir.path().join("content/about.md"),
        "---\ntitle: About\n---\nPolled.\n",
    );
    assert!(lr.next(PATIENCE).is_some_and(|c| c.contains("/x.js")));
    assert!(get(addr, "/about/", &[]).text().contains("Polled."));
    server.shutdown();
}

/// `build.is_server` is true and `site.server_port` is the listener's port in the server
/// (T70); a `build` of the same site has `false` and its base URL's port (none: 0).
#[test]
fn build_is_server_and_site_server_port() {
    let dir = site(concat!(
        "-- config.toml --\n",
        "baseURL = \"https://example.org/\"\n",
        "disableKinds = [\"taxonomy\", \"term\", \"sitemap\", \"rss\", \"robotsTXT\", \"404\"]\n",
        "-- layouts/home.html --\n",
        "<html><head></head><body>server={{ build.is_server }} port={{ site.server_port }}</body></html>\n",
    ));
    let (server, _events) = serve(dir.path(), |o| o.watch = Watch::Off);
    let addr = server.local_addrs()[0];
    let home = get(addr, "/", &[]).text();
    let want = format!("server=true port={}", addr.port());
    assert!(home.contains(&want), "{home}");
    server.shutdown();

    let report = ssg_build::build(ssg_build::BuildRequest {
        source: dir.path().to_owned(),
        sink: ssg_build::SinkKind::Memory,
        ..ssg_build::BuildRequest::default()
    })
    .expect("build");
    let memory = report.memory.expect("memory sink");
    let home = memory.text("index.html").expect("index.html");
    assert!(home.contains("server=false port=0"), "{home}");
}
