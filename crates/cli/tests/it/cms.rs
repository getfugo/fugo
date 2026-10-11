//! `[cms]`: a production build writes the editor, its index and the API Worker; a development
//! build of the same project does not.

use crate::{binary, stderr, stdout};

const SITE: &str = "baseURL = \"https://example.org/\"\ntitle = \"Site\"\n\
                    disableKinds = [\"taxonomy\", \"term\"]\n";

/// `[cms]`, for production builds only.
const CMS: &str = r#"
[environments.production.cms.git]
repo = "owner/site"
[environments.production.cms.login]
team = "team"
aud = "aud"
[environments.production.cms.roles.owner]
edit = ["**"]
publish = true
"#;

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir(dir.path().join(".git")).expect(".git");
    let config = format!("{SITE}{CMS}");
    for (path, text) in [
        ("config.toml", config.as_str()),
        ("layouts/single.html", "{{ page.title }}"),
        ("layouts/home.html", "home"),
        ("layouts/list.html", "{{ page.title }}"),
        ("content/posts/a.md", "---\ntitle: A\n---\nText.\n"),
    ] {
        let p = dir.path().join(path);
        std::fs::create_dir_all(p.parent().expect("parent")).expect("mkdir");
        std::fs::write(p, text).expect("write");
    }
    dir
}

#[test]
fn production_builds_have_the_editor() {
    let dir = project();
    let o = binary(dir.path(), &["build"], &[]);
    assert!(o.status.success(), "{}{}", stderr(&o), stdout(&o));
    assert!(
        stdout(&o).contains("CMS editor at /admin/ (API Worker: _worker.js)"),
        "{}",
        stdout(&o)
    );
    assert!(!stderr(&o).contains("WARN"), "{}", stderr(&o));
    let public = dir.path().join("public");
    for f in [
        "admin/index.html",
        "admin/cms.js",
        "_headers",
        "_worker.js",
        ".assetsignore",
    ] {
        assert!(public.join(f).is_file(), "{f}");
    }
    let worker = std::fs::read_to_string(public.join("_worker.js")).expect("_worker.js");
    assert!(
        worker.contains("\\\"posts/a\\\""),
        "the index is in the Worker"
    );
    assert!(
        worker.contains("\\\"url\\\":\\\"/posts/a/\\\""),
        "with the URLs of the pages"
    );
    assert!(
        !public.join("admin/site.json").exists(),
        "the index is not public"
    );
    assert!(
        public.join("posts/a/index.html").is_file(),
        "the site is built too"
    );
}

#[test]
fn development_builds_do_not() {
    let dir = project();
    let o = binary(dir.path(), &["build", "-e", "development"], &[]);
    assert!(o.status.success(), "{}{}", stderr(&o), stdout(&o));
    assert!(!stdout(&o).contains("CMS editor"), "{}", stdout(&o));
    assert!(!dir.path().join("public/_worker.js").exists());
    assert!(!dir.path().join("public/admin").exists());
}

#[test]
fn a_wrong_setting_fails_the_build() {
    let dir = project();
    std::fs::write(
        dir.path().join("config.toml"),
        format!("{SITE}{}", CMS.replace("owner/site", "no-slash")),
    )
    .expect("write");
    let o = binary(dir.path(), &["build"], &[]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("cms.git.repo"), "{}", stderr(&o));
}

/// The editor's embedded files (`crates/cms/assets/admin/cms.js`, `cms.css`, `worker.js`) are what
/// its sources (`crates/cms/web`, TypeScript and Sass) build to, and the TypeScript type-checks
/// (`tsc`, strict). `tools/cms/build.sh` writes them; it needs the modules of
/// `tools/dev/node.sh` (the libraries and tsc), without which this prints `SKIPPED`.
#[cfg(unix)]
#[test]
fn editor_assets_are_built_from_their_sources() {
    let Some(modules) = ssg_testkit::fixture::node_tools()
        .filter(|m| m.join("typescript").is_dir() && m.join("yaml").is_dir())
    else {
        eprintln!("SKIPPED cms editor assets: no node modules (run tools/dev/node.sh)");
        return;
    };
    let repo = ssg_testkit::fixture::repo_dir().join("crates/cms");
    let tmp = tempfile::tempdir().expect("tempdir");
    let web = tmp.path().join("web");
    copy_dir(&repo.join("web"), &web);
    std::os::unix::fs::symlink(&modules, web.join("node_modules")).expect("symlink");

    let tsc = std::process::Command::new(modules.join(".bin/tsc"))
        .arg("-p")
        .arg(&web)
        .output()
        .expect("run tsc");
    assert!(
        tsc.status.success(),
        "tsc:\n{}{}",
        String::from_utf8_lossy(&tsc.stdout),
        String::from_utf8_lossy(&tsc.stderr)
    );

    let out = tmp.path().join("out");
    let o = binary(
        &web,
        &["build", "-d", out.to_str().expect("utf-8"), "--quiet"],
        &[],
    );
    assert!(o.status.success(), "{}{}", stderr(&o), stdout(&o));
    let files = [
        "admin/cms.js",
        "admin/cms.js.LEGAL.txt",
        "admin/cms.css",
        "worker.js",
    ];
    let mut written = Vec::new();
    collect_files(&out, &out, &mut written);
    written.retain(|f| f != "index.html");
    written.sort();
    let mut want: Vec<String> = files.iter().map(|f| (*f).to_owned()).collect();
    want.sort();
    assert_eq!(
        written, want,
        "the build writes other files than crates/cms embeds"
    );
    for f in files {
        let built = std::fs::read(out.join(f)).expect("built");
        let embedded = std::fs::read(repo.join("assets").join(f)).expect("embedded");
        assert!(
            built == embedded,
            "crates/cms/assets/{f} is not what crates/cms/web builds to: run tools/cms/build.sh"
        );
    }
}

#[cfg(unix)]
fn collect_files(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<String>) {
    for e in std::fs::read_dir(dir).expect("read_dir") {
        let path = e.expect("entry").path();
        if path.is_dir() {
            collect_files(root, &path, out);
        } else {
            let rel = path.strip_prefix(root).expect("under root");
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

#[cfg(unix)]
fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).expect("mkdir");
    for e in std::fs::read_dir(from).expect("read_dir") {
        let e = e.expect("entry");
        let target = to.join(e.file_name());
        if e.file_type().expect("type").is_dir() {
            copy_dir(&e.path(), &target);
        } else {
            std::fs::copy(e.path(), target).expect("copy");
        }
    }
}

#[test]
fn cms_fields_prints_the_fields_the_build_gives_the_editor() {
    let dir = project();
    let pages = [
        (
            "content/posts/b.md",
            "---\ntitle: B\ndate: 2026-01-02\ndraft: false\nimage_preview: b.jpg\ndescription: About B\nrating:\n  taste: 4\nwhenSeen: Paris\n---\n",
        ),
        (
            "config.toml",
            &format!(
                "{SITE}{CMS}[environments.production.cms.fields.description]\nlabel = \"Short text\"\nhelp = \"One \\\"line\\\"\"\n[environments.production.cms.fields.flavours]\noptions = [\"sweet\", \"salty\"]\nmultiple = true\n"
            ),
        ),
    ];
    for (path, text) in pages {
        std::fs::write(dir.path().join(path), text).expect("write");
    }
    let o = binary(dir.path(), &["cms", "fields"], &[]);
    assert!(o.status.success(), "{}{}", stderr(&o), stdout(&o));
    let out = stdout(&o);
    for want in [
        "[cms.fields.date]\n# a date; in posts\nlabel = \"Date\"\nwidget = \"date\"\n",
        "[cms.fields.draft]\n# yes or no; in posts\nlabel = \"Draft\"\nwidget = \"boolean\"\n",
        "[cms.fields.image_preview]\n# text; in posts\nlabel = \"Image preview\"\nwidget = \"image\"\n",
        "[cms.fields.rating]\n# a table; in posts\nlabel = \"Rating\"\n\n",
        // A key inside a table, by its path.
        "[cms.fields.\"rating.taste\"]\n# a number; in posts\nlabel = \"Taste\"\nwidget = \"number\"\n",
        "[cms.fields.whenseen]\n# text; in posts\nlabel = \"When seen\"\nwidget = \"text\"\n",
        // The configuration's settings go over what the build works out, one by one.
        "[cms.fields.description]\n# text; in posts\nlabel = \"Short text\"\nwidget = \"textarea\"\nhelp = \"One \\\"line\\\"\"\n",
        // A key only the configuration names.
        "[cms.fields.flavours]\n# a list; no page has it yet\nlabel = \"Flavours\"\nwidget = \"select\"\noptions = [\"sweet\", \"salty\"]\nmultiple = true\n",
    ] {
        assert!(out.contains(want), "{want}\nnot in:\n{out}");
    }

    let none = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        none.path().join("config.toml"),
        "baseURL = \"https://example.org/\"\n",
    )
    .expect("config");
    let o = binary(none.path(), &["cms", "fields"], &[]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("no [cms]"), "{}", stderr(&o));
}
