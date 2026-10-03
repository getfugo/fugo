//! `Publisher::emit` end to end: canonify per format, minify dispatch, empty outputs, held
//! outputs and `patch_held`, and URL tokens.

use std::collections::BTreeMap;
use std::sync::Arc;

use rayon::prelude::*;
use ssg_base::diag::Diagnostics;
use ssg_base::paths::OutputPath;
use ssg_base::{FormatId, Idx, LangIdx, Sink};
use ssg_config::{Config, LoadOptions, load};
use ssg_publish::{
    DiskSink, Emitted, MemorySink, Output, PublishError, PublishSettings, Publisher,
};

struct Site {
    _dir: tempfile::TempDir,
    cfg: Config,
}

fn site(toml: &str) -> Site {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("config.toml"), toml).unwrap();
    let cfg = load(&LoadOptions {
        source: dir.path().to_owned(),
        env: vec![(
            "HOME".into(),
            dir.path().join("home").to_str().unwrap().into(),
        )],
        ..LoadOptions::default()
    })
    .unwrap();
    Site { _dir: dir, cfg }
}

fn publisher(s: &Site) -> (Publisher, Arc<MemorySink>, Arc<Diagnostics>) {
    let sink = Arc::new(MemorySink::new());
    let diags = Arc::new(Diagnostics::new(Vec::<String>::new()));
    let settings = PublishSettings::from_config(&s.cfg).unwrap();
    let p = Publisher::new(
        settings,
        Arc::clone(&sink) as Arc<dyn Sink>,
        Arc::clone(&diags),
    );
    (p, sink, diags)
}

fn format(s: &Site, name: &str) -> FormatId {
    s.cfg.output_formats.by_name(name).unwrap()
}

fn output(s: &Site, path: &str, format_name: &str, text: &str) -> Output {
    Output {
        path: OutputPath::new(path),
        text: text.to_owned(),
        format: format(s, format_name),
        lang: LangIdx::from_index(0),
        alias: false,
    }
}

const PAGE: &str = concat!(
    r#"<!doctype html><html><head><link href="/css/a.css" rel=stylesheet></head>"#,
    r#"<body><a href="/about/">About</a> <img srcset="/i/a.jpg 1x, /i/b.jpg 2x"></body></html>"#
);

#[test]
fn canonify_by_format() {
    let s = site("baseURL = 'https://example.org/docs/'\ncanonifyURLs = true\n");
    let (p, sink, _) = publisher(&s);
    p.emit(output(&s, "/a/index.html", "html", PAGE)).unwrap();
    p.emit(output(
        &s,
        "/index.xml",
        "rss",
        r#"<description>&lt;a href=&#34;/x/&#34;&gt; <link href="/y"/></description>"#,
    ))
    .unwrap();
    p.emit(output(&s, "/index.json", "json", r#"{"u":"/z"}"#))
        .unwrap();
    let html = sink.text("/a/index.html").unwrap();
    assert!(
        html.contains(r#"href="https://example.org/docs/about/""#),
        "{html}"
    );
    assert!(
        html.contains(
            r#"srcset="https://example.org/docs/i/a.jpg 1x, https://example.org/docs/i/b.jpg 2x""#
        ),
        "{html}"
    );
    // RSS is canonified with the entity quotes; a raw `"` is not a quote there.
    assert_eq!(
        sink.text("/index.xml").unwrap(),
        r#"<description>&lt;a href=&#34;https://example.org/docs/x/&#34;&gt; <link href="/y"/></description>"#
    );
    assert_eq!(sink.text("/index.json").unwrap(), r#"{"u":"/z"}"#);
}

#[test]
fn rss_is_canonified_without_the_setting() {
    let s = site("baseURL = 'https://example.org/'\n");
    let (p, sink, _) = publisher(&s);
    p.emit(output(&s, "/a/index.html", "html", PAGE)).unwrap();
    p.emit(output(&s, "/index.xml", "rss", "<d>src=&#34;/x&#34;</d>"))
        .unwrap();
    assert_eq!(sink.text("/a/index.html").unwrap(), PAGE);
    assert_eq!(
        sink.text("/index.xml").unwrap(),
        "<d>src=&#34;https://example.org/x&#34;</d>"
    );
}

#[test]
fn relative_urls() {
    let s = site("baseURL = 'https://example.org/'\nrelativeURLs = true\n");
    let (p, sink, _) = publisher(&s);
    p.emit(output(&s, "/a/b/index.html", "html", PAGE)).unwrap();
    let html = sink.text("/a/b/index.html").unwrap();
    assert!(html.contains(r#"href="../../about/""#), "{html}");
    // Tokens of relative URLs resolve against the output's directory.
    assert!(p.url_tokens().contains("/css/a.css"));
}

/// `serve`: each language's HTML pages get the script of its own LiveReload URL; aliases,
/// non-HTML formats and languages without a URL get none (Go's publisher).
#[test]
fn livereload_script_per_language() {
    let s = site(concat!(
        "baseURL = 'https://example.org/'\n",
        "[languages.en]\nweight = 1\n[languages.nn]\nweight = 2\n",
    ));
    let mut settings = PublishSettings::from_config(&s.cfg).unwrap();
    assert!(settings.sites.iter().all(|l| l.livereload.is_none()));
    let url = ssg_base::url::UrlRef::parse("http://localhost:1313/docs/").unwrap();
    settings.sites.iter_mut().next().unwrap().livereload = Some(url);
    let sink = Arc::new(MemorySink::new());
    let diags = Arc::new(Diagnostics::new(Vec::<String>::new()));
    let p = Publisher::new(settings, Arc::clone(&sink) as Arc<dyn Sink>, diags);
    let page = "<!doctype html><html><head><title>t</title></head></html>";
    p.emit(output(&s, "/index.html", "html", page)).unwrap();
    p.emit(Output {
        alias: true,
        ..output(&s, "/old/index.html", "html", page)
    })
    .unwrap();
    p.emit(output(&s, "/index.xml", "rss", "<rss></rss>"))
        .unwrap();
    p.emit(Output {
        lang: LangIdx::from_index(1),
        ..output(&s, "/nn/index.html", "html", page)
    })
    .unwrap();
    let script = concat!(
        r#"<script src="/docs/livereload.js?mindelay=10&amp;v=2&amp;port=1313&amp;"#,
        r#"path=docs/livereload" data-no-instant defer></script>"#
    );
    assert_eq!(
        sink.text("/index.html").unwrap(),
        format!("<!doctype html><html><head>{script}<title>t</title></head></html>")
    );
    assert_eq!(sink.text("/old/index.html").unwrap(), page);
    assert_eq!(sink.text("/index.xml").unwrap(), "<rss></rss>");
    assert_eq!(sink.text("/nn/index.html").unwrap(), page);
}

#[test]
fn minify_dispatch_and_empty_outputs() {
    let s = site("baseURL = 'https://example.org/'\nminify = true\n");
    let (p, sink, diags) = publisher(&s);
    p.emit(output(&s, "/index.html", "html", "<p>\n  a  </p>\n\n"))
        .unwrap();
    p.emit(output(&s, "/robots.txt", "robots", "User-agent: *\n\n"))
        .unwrap();
    p.emit(output(&s, "/index.json", "json", "{ \"a\" : [1, 2] }"))
        .unwrap();
    p.emit(output(&s, "/bad.json", "json", "{ not json"))
        .unwrap();
    assert_eq!(
        p.emit(output(&s, "/empty.html", "html", "")).unwrap(),
        Emitted::Empty
    );
    assert!(sink.text("/index.html").unwrap().len() < "<p>\n  a  </p>\n\n".len());
    assert_eq!(sink.text("/robots.txt").unwrap(), "User-agent: *\n\n");
    assert_eq!(sink.text("/index.json").unwrap(), r#"{"a":[1,2]}"#);
    // Invalid JSON is published as it is, with a warning.
    assert_eq!(sink.text("/bad.json").unwrap(), "{ not json");
    assert_eq!(diags.report().len(), 1);
    assert!(!sink.exists(&OutputPath::new("/empty.html")));
    assert_eq!(p.written(), 4);
}

#[test]
fn held_outputs_are_patched_and_rescanned() {
    let s = site("baseURL = 'https://example.org/'\nminify = true\n");
    let (p, sink, _) = publisher(&s);
    let page = "<html><head>__nh_defer_css__</head><body>  <p>x</p>  </body></html>";
    assert_eq!(
        p.emit(output(&s, "/index.html", "html", page)).unwrap(),
        Emitted::Held
    );
    assert_eq!(
        p.emit(output(
            &s,
            "/b/index.html",
            "html",
            "<link href=\"__nh_pp_7_rel_permalink__\">"
        ))
        .unwrap(),
        Emitted::Held
    );
    // Held outputs wait in the sink, unpatched and not minified; only their paths are kept.
    assert_eq!(sink.text("/index.html").unwrap(), page);
    assert_eq!(p.written(), 0);
    assert_eq!(p.held().len(), 2);
    assert!(!p.url_tokens().contains("/css/styles.min.css"));

    let repl = BTreeMap::from([
        (
            "__nh_defer_css__".to_owned(),
            r#"<link rel="stylesheet" href="/css/styles.min.css">"#.to_owned(),
        ),
        (
            "__nh_pp_7_rel_permalink__".to_owned(),
            "/css/pp.css".to_owned(),
        ),
    ]);
    assert_eq!(p.patch_held(&repl).unwrap(), 2);
    let html = sink.text("/index.html").unwrap();
    assert!(html.contains("/css/styles.min.css"), "{html}");
    assert!(!html.contains("  <p>"), "minified after patching: {html}");
    assert!(p.url_tokens().contains("/css/styles.min.css"));
    assert!(p.url_tokens().contains("/css/pp.css"));
    assert!(p.held().is_empty());
}

#[test]
fn patched_links_are_canonified() {
    let s = site("baseURL = 'https://example.org/'\ncanonifyURLs = true\n");
    let (p, sink, _) = publisher(&s);
    let page = r#"<a href="/a/">a</a><link href="__nh_pp_1_rel_permalink__">"#;
    assert_eq!(
        p.emit(output(&s, "/index.html", "html", page)).unwrap(),
        Emitted::Held
    );
    let repl = BTreeMap::from([(
        "__nh_pp_1_rel_permalink__".to_owned(),
        "/css/a.css".to_owned(),
    )]);
    assert_eq!(p.patch_held(&repl).unwrap(), 1);
    assert_eq!(
        sink.text("/index.html").unwrap(),
        r#"<a href="https://example.org/a/">a</a><link href="https://example.org/css/a.css">"#
    );
}

#[test]
fn unresolved_placeholder_is_an_error() {
    let s = site("baseURL = 'https://example.org/'\n");
    let (p, sink, _) = publisher(&s);
    p.emit(output(&s, "/index.html", "html", "a __nh_defer_x__ b"))
        .unwrap();
    match p.patch_held(&BTreeMap::new()) {
        Err(PublishError::UnresolvedPlaceholder { path, placeholder }) => {
            assert_eq!(path.as_str(), "/index.html");
            assert_eq!(placeholder, "__nh_defer_x__");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(sink.text("/index.html").unwrap(), "a __nh_defer_x__ b");
    assert_eq!(p.written(), 0);
}

/// A disk build: the held page waits in its file and is patched there.
#[test]
fn held_outputs_wait_on_disk() {
    let s = site(
        "baseURL = 'https://example.org/'
minify = true
",
    );
    let out = tempfile::tempdir().unwrap();
    let p = Publisher::new(
        PublishSettings::from_config(&s.cfg).unwrap(),
        Arc::new(DiskSink::new(out.path())),
        Arc::new(Diagnostics::new(Vec::<String>::new())),
    );
    let page = "<html><head>__nh_pp_1_content__</head><body>  <p>x</p>  </body></html>";
    assert_eq!(
        p.emit(output(&s, "/a/index.html", "html", page)).unwrap(),
        Emitted::Held
    );
    let file = out.path().join("a/index.html");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), page);
    let repl = BTreeMap::from([(
        "__nh_pp_1_content__".to_owned(),
        "<style>p{color:red}</style>".to_owned(),
    )]);
    assert_eq!(p.patch_held(&repl).unwrap(), 1);
    let html = std::fs::read_to_string(&file).unwrap();
    assert!(
        html.contains("<style>p{color:red}</style>")
            && !html.contains("__nh_")
            && !html.contains("  <p>"),
        "{html}"
    );
    // A held file removed before the deferred wave is an error naming it.
    p.emit(output(&s, "/b/index.html", "html", page)).unwrap();
    std::fs::remove_file(out.path().join("b/index.html")).unwrap();
    match p.patch_held(&repl) {
        Err(PublishError::Read { path, .. }) => assert_eq!(path.as_str(), "/b/index.html"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn url_tokens_of_outputs() {
    let s = site("baseURL = 'https://example.org/'\n");
    let (p, _, _) = publisher(&s);
    p.emit(output(
        &s,
        "/b/index.html",
        "html",
        r#"<img src="/b/herrs-salt-&amp;-vinegar/Lay&#39;s.jpg"><img srcset="/r/a.webp 1x,/r/b.webp 2x">"#,
    ))
    .unwrap();
    p.emit(output(
        &s,
        "/index.json",
        "json",
        r#"{"image":"https:\/\/example.org\/j\/c\u0026d.png?x=1"}"#,
    ))
    .unwrap();
    p.add_tokens_from("url(/from/template.png)");
    let t = p.url_tokens();
    for want in [
        "/b/herrs-salt-&-vinegar/Lay's.jpg",
        "/r/a.webp",
        "/r/b.webp",
        "https://example.org/j/c&d.png",
        "https://example.org/j/c&d.png?x=1",
        "/from/template.png",
    ] {
        assert!(
            t.contains(want),
            "{want}: {:?}",
            t.iter().collect::<Vec<_>>()
        );
    }
}

/// Concurrent emits give the same tokens as sequential ones.
#[test]
fn concurrent_emits_are_deterministic() {
    let s = site("baseURL = 'https://example.org/'\n");
    let pages: Vec<String> = (0..200)
        .map(|i| {
            format!(
                r#"<div class="c{}" id="i{i}"><a href="/p/{i}/">x</a></div>"#,
                i % 17
            )
        })
        .collect();
    let run = |threads: usize| {
        let (p, sink, _) = publisher(&s);
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .unwrap();
        pool.install(|| {
            pages.par_iter().enumerate().for_each(|(i, text)| {
                p.emit(output(&s, &format!("/p/{i}/index.html"), "html", text))
                    .unwrap();
            });
        });
        (p.url_tokens(), sink.paths())
    };
    assert_eq!(run(1), run(4));
}

#[test]
fn disk_sink_creates_directories() {
    let dir = tempfile::tempdir().unwrap();
    let sink = DiskSink::new(dir.path());
    let path = OutputPath::new("/a/b/index.html");
    assert!(!sink.exists(&path));
    sink.write(&path, b"x").unwrap();
    sink.write(&path, b"yz").unwrap();
    assert!(sink.exists(&path));
    assert_eq!(
        std::fs::read(dir.path().join("a/b/index.html")).unwrap(),
        b"yz"
    );

    let mem = MemorySink::new();
    mem.write(&path, b"m").unwrap();
    mem.write_to(&dir.path().join("mem")).unwrap();
    assert_eq!(
        std::fs::read(dir.path().join("mem/a/b/index.html")).unwrap(),
        b"m"
    );
    assert_eq!(mem.get("a/b/index.html").as_deref(), Some(&b"m"[..]));
}

/// `purge_css` placeholders: each page gets the rules it uses (its elements, the words of its
/// scripts, the custom properties it mentions), resolved before the page is held or written.
#[test]
fn css_purged_per_page() {
    use ssg_minify::{CssPurges, Minifier, PurgeOptions, PurgePlan};
    let s = site("baseURL = 'https://example.org/'\n");
    let purges = Arc::new(CssPurges::default());
    let sink = Arc::new(MemorySink::new());
    let p = Publisher::new(
        PublishSettings::from_config(&s.cfg).unwrap(),
        Arc::clone(&sink) as Arc<dyn Sink>,
        Arc::new(Diagnostics::new(Vec::<String>::new())),
    )
    .with_css_purges(Arc::clone(&purges));
    let css = ":root{--a:red;--b:blue}.nav{color:var(--a)}.card{color:#123}.shown{x:1}\
               .tpl{y:2}#main{z:3}table{w:4}";
    let options = PurgeOptions {
        variables: true,
        ..PurgeOptions::default()
    };
    let ph = purges
        .placeholder(1, || {
            PurgePlan::compile(css, &options, &[], Minifier::default().css_targets())
        })
        .unwrap();
    let page =
        |body: &str| format!("<html><head><style>{ph}</style></head><body>{body}</body></html>");
    p.emit(output(
        &s,
        "/a/index.html",
        "html",
        &page("<nav class=nav></nav>"),
    ))
    .unwrap();
    p.emit(output(
        &s,
        "/b/index.html",
        "html",
        &page(
            "<div id=main class=card style=\"color:var(--b)\"></div>\
             <script>el.classList.add('shown')</script>\
             <script type=x-tmpl-mustache><p class=\"tpl\">{{x}}</p></script>",
        ),
    ))
    .unwrap();
    let style = |path: &str| {
        let html = sink.text(path).unwrap();
        let start = html.find("<style>").unwrap() + 7;
        html[start..html.find("</style>").unwrap()].to_owned()
    };
    assert_eq!(style("/a/index.html"), ":root{--a:red}.nav{color:var(--a)}");
    assert_eq!(
        style("/b/index.html"),
        ":root{--b:blue}.card{color:#123}.shown{x:1}.tpl{y:2}#main{z:3}"
    );
}
