//! The session's phases on a small site: content, freeze, jobs of waves 1 and 2, the render
//! scope in layout contexts, recorded pagination.

use std::fs;
use std::path::Path;
use std::sync::Arc;

use ssg_base::{Clock, LangIdx, PageKind};
use ssg_config::LoadOptions;
use ssg_layouts::LayoutStore;
use ssg_render::{Job, Project, RenderError, RenderOptions, Session};
use ssg_site::LoadModelOptions;
use ssg_vfs::Vfs;

const FILES: &[(&str, &str)] = &[
    (
        "config.toml",
        "baseURL = \"https://example.org/\"\ntitle = \"Mini\"\ndisableKinds = [\"taxonomy\", \"term\", \"rss\", \"sitemap\"]\n[pagination]\npagerSize = 1\n",
    ),
    ("content/_index.md", "---\ntitle: Home\n---\n"),
    (
        "content/a.md",
        "---\ntitle: A\ndate: 2021-01-01\n---\nHello *a*.\n",
    ),
    (
        "content/b.md",
        "---\ntitle: B\ndate: 2021-01-02\n---\nHello b.\n",
    ),
    (
        "layouts/home.html",
        "{% set pager = paginator() %}{{ __nh.pager or 1 }}/{{ pager.total_pages }}:{% for p in pager.pages %}{{ p.title }}{% endfor %}",
    ),
    ("layouts/single.html", "{{ page.title }}={{ page.content }}"),
];

fn write_site(dir: &Path) {
    for (rel, text) in FILES {
        let path = dir.join(rel);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(path, text).expect("write");
    }
}

fn session(dir: &Path) -> Arc<Session> {
    let cfg = Arc::new(
        ssg_config::load(&LoadOptions {
            source: dir.to_path_buf(),
            config_files: Vec::new(),
            cli: ssg_config::CliOverrides::default(),
            env: Vec::new(),
        })
        .expect("config"),
    );
    let vfs = Arc::new(Vfs::new(&cfg).expect("vfs"));
    let clock = Clock("2026-01-01T00:00:00Z".parse().expect("clock"));
    let model = ssg_site::load_model(
        Arc::clone(&cfg),
        &vfs,
        &LoadModelOptions::from_config(&cfg, clock),
    )
    .expect("model");
    let layouts = Arc::new(LayoutStore::scan(&vfs, &cfg).expect("layouts"));
    Session::new(
        Project { vfs, layouts },
        Arc::new(model),
        &RenderOptions {
            clock,
            ..RenderOptions::default()
        },
    )
    .expect("session")
}

fn texts(s: &Session, jobs: &[Job]) -> Vec<(String, String)> {
    jobs.iter()
        .flat_map(|j| s.render_job(j).expect("render"))
        .map(|o| (o.path.to_string(), o.text))
        .collect()
}

#[test]
fn phases_jobs_and_pagination() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_site(tmp.path());
    let s = session(tmp.path());

    let wave1 = s.wave1(LangIdx::from_raw(0));
    assert!(matches!(
        s.render_job(&wave1[0]),
        Err(RenderError::Phase(_))
    ));
    assert!(matches!(s.freeze_views(), Err(RenderError::Phase(_))));
    s.render_content().expect("content");
    s.freeze_views().expect("freeze");

    let model = s.model();
    let kinds: Vec<PageKind> = wave1
        .iter()
        .map(|j| match j {
            Job::Page { page, .. } | Job::Standalone { page, .. } => model.pages[*page].kind,
            other => panic!("unexpected wave 1 job {other:?}"),
        })
        .collect();
    assert_eq!(
        kinds,
        [
            PageKind::Home,
            PageKind::Page,
            PageKind::Page,
            PageKind::NotFound
        ]
    );
    let got = texts(&s, &wave1);
    assert_eq!(got[0], ("/index.html".to_owned(), "1/2:B".to_owned()));
    assert_eq!(got[1].0, "/a/index.html");
    assert_eq!(got[1].1, "A=<p>Hello <em>a</em>.</p>\n");
    // No 404 layout: no output, no error.
    assert_eq!(got.len(), 3);

    // Wave 2: the recorded home pagination gives the page/1 alias and pager 2.
    let wave2 = s.wave2();
    assert_eq!(wave2.len(), 2, "{wave2:?}");
    let got = texts(&s, &wave2);
    assert_eq!(got[0].0, "/page/1/index.html");
    assert!(
        got[0].1.contains("url=https://example.org/"),
        "{}",
        got[0].1
    );
    assert_eq!(
        got[1],
        ("/page/2/index.html".to_owned(), "2/2:A".to_owned())
    );
}

#[test]
fn partial_return_values_and_getenv_policy() {
    let tmp = tempfile::tempdir().expect("tempdir");
    write_site(tmp.path());
    let dir = tmp.path();
    let cfg = fs::read_to_string(dir.join("config.toml")).expect("config");
    fs::write(
        dir.join("config.toml"),
        format!("{cfg}[security.funcs]\ngetenv = ['^PATH$']\n"),
    )
    .expect("write");
    fs::create_dir_all(dir.join("layouts/_partials")).expect("mkdir");
    fs::write(
        dir.join("layouts/_partials/double.html"),
        "{{ return_value(value=x * 2) }}",
    )
    .expect("write");
    fs::write(
        dir.join("layouts/single.html"),
        "{{ partial(name=\"double.html\", x=21) }}:{{ get_env(name=\"PATH\") != \"\" }}",
    )
    .expect("write");
    let s = session(dir);
    s.render_content().expect("content");
    s.freeze_views().expect("freeze");
    let got = texts(&s, &s.wave1(LangIdx::from_raw(0)));
    assert_eq!(got[1], ("/a/index.html".to_owned(), "42:true".to_owned()));
}
