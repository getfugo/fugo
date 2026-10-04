//! Sites served differently: rendered to disk, under a base path, multihost, with per-language 404
//! pages, and `build.is_server`.

use super::*;

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
