//! What is watched: configuration and env files, new static directories, themes and new
//! configuration files, and the polling watcher.

use super::*;

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
