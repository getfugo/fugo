//! The testsite (as `ssg-build`'s skeleton test assembles it) served, edited and
//! reloaded; the edit → reload times are printed (`cargo test -p ssg-serve -- --nocapture
//! testsite`).

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use ssg_testkit::fixture::{repo_dir, repo_file};

use crate::{LiveReload, get, serve, write};

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
        write(&to.join(&name), &body);
    }
}

/// The testsite as `sites.py make testsite` writes it, with the Tera layouts of
/// `sites/testsite/layouts`.
fn testsite(dir: &Path) {
    let root = repo_dir();
    copy_tree(&repo_file("testsite"), dir);
    let txtar = fs::read_to_string(root.join("tools/rust-port/i01/testsite.txtar")).expect("txtar");
    write_txtar(&txtar, dir);
    fs::remove_dir_all(dir.join("layouts")).expect("remove Go layouts");
    copy_tree(&root.join("sites/testsite/layouts"), &dir.join("layouts"));
}

#[test]
fn testsite_is_served_and_reloads() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path().join("testsite");
    testsite(&dir);
    let (server, events) = serve(&dir, |_| {});
    let addr = server.local_addrs()[0];
    let port = addr.port();
    let base = format!("http://localhost:{port}/");
    assert_eq!(server.urls(), std::slice::from_ref(&base));

    // The pages of both languages, their formats, and the canonified links on the server.
    for (path, needle) in [
        ("/", "<title>Home</title>"),
        ("/about/", r#"<span class="raw-html" id="raw">raw</span>"#),
        ("/posts/", r#"<body class="kind-section lang-en" id="top">"#),
        ("/posts/page/2/", "<title>Posts</title>"),
        ("/posts/one/", "<title>One &amp; &lt;Two&gt;</title>"),
        ("/tags/b/", "<title>B</title>"),
        ("/nn/", "<title>Heim</title>"),
        ("/nn/om/", r#"<b class="nn-b">oss</b>"#),
        ("/index.json", r#"{"title": "Home""#),
        ("/index.xml", &format!("<link>{base}</link>")),
        ("/sitemap.xml", &format!("<loc>{base}nn/sitemap.xml</loc>")),
    ] {
        let r = get(addr, path, &[]);
        assert_eq!(r.status, 200, "{path}");
        assert!(r.text().contains(needle), "{path}: {}", r.text());
    }
    let script = format!(
        r#"<script src="/livereload.js?mindelay=10&amp;v=2&amp;port={port}&amp;path=livereload" data-no-instant defer></script>"#
    );
    let home = get(addr, "/", &[]).text();
    assert!(home.contains(&format!("<head>{script}\n")), "{home}");
    assert!(
        home.contains(&format!(
            r#"<link rel="stylesheet" href="{base}css/site.css">"#
        )),
        "{home}"
    );
    assert!(!get(addr, "/index.json", &[]).text().contains("livereload"));
    assert!(!get(addr, "/old-about/", &[]).text().contains("livereload"));
    let css = get(addr, "/css/site.css", &[]);
    assert_eq!(css.header("content-type"), Some("text/css; charset=utf-8"));
    assert_eq!(css.text(), "a { color : red }\n");
    let json = get(addr, "/index.json", &[]);
    assert_eq!(json.header("content-type"), Some("application/json"));
    for path in ["/nope/", "/nn/nope/"] {
        let nf = get(addr, path, &[]);
        assert_eq!(nf.status, 404, "{path}");
        assert!(
            nf.text()
                .starts_with(&format!("<html>{script}<body class=\"nf\">")),
            "{path}: {}",
            nf.text()
        );
    }
    assert_eq!(get(addr, "/livereload.js", &[]).status, 200);

    // Edit → reload, measured.
    let mut lr = LiveReload::connect(addr, "/livereload");
    let mut timings: Vec<(&str, Duration)> = Vec::new();
    let one = dir.join("content/posts/one.md");
    let original = fs::read_to_string(&one).expect("one.md");
    for n in 1..=3 {
        let started = Instant::now();
        write(
            &one,
            &original.replace("The rest.", &format!("The rest, edit {n}.")),
        );
        let command = lr.expect();
        timings.push(("content", started.elapsed()));
        assert!(command.contains(r#""path":"/x.js""#), "{command}");
        assert!(
            get(addr, "/posts/one/", &[])
                .text()
                .contains(&format!("The rest, edit {n}.")),
            "edit {n}"
        );
    }

    let page = dir.join("layouts/_partials/page.html");
    let layout = fs::read_to_string(&page).expect("page.html");
    let started = Instant::now();
    write(&page, &layout.replace("<main>", "<main class=\"edited\">"));
    assert!(lr.expect().contains(r#""path":"/x.js""#));
    timings.push(("layout", started.elapsed()));
    assert!(
        get(addr, "/nn/om/", &[])
            .text()
            .contains(r#"<main class="edited">"#)
    );

    let builds = events.count("built");
    let started = Instant::now();
    write(&dir.join("static/css/site.css"), "a { color : blue }\n");
    assert!(lr.expect().contains(r#""path":"/css/site.css""#));
    timings.push(("static", started.elapsed()));
    assert_eq!(
        get(addr, "/css/site.css", &[]).text(),
        "a { color : blue }\n"
    );
    assert_eq!(
        events.count("built"),
        builds,
        "a static change does not build"
    );

    for (what, t) in &timings {
        println!("testsite: {what} edit → reload {} ms", t.as_millis());
    }
    let within_2s = timings
        .iter()
        .filter(|(_, t)| *t <= Duration::from_secs(2))
        .count();
    println!(
        "testsite: {within_2s} of {} edits reloaded within 2 s",
        timings.len()
    );
    server.shutdown();
}
