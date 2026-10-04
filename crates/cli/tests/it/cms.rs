//! `[cms]`: a production build writes the editor, its index and the API Worker; a development
//! build of the same project does not.

use crate::{binary, stderr, stdout};

const CMS: &str = r#"
[git]
repo = "owner/site"
[login]
team = "team"
aud = "aud"
[roles.owner]
edit = ["**"]
publish = true
"#;

fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir(dir.path().join(".git")).expect(".git");
    for (path, text) in [
        (
            "config.toml",
            "baseURL = \"https://example.org/\"\ntitle = \"Site\"\ndisableKinds = [\"taxonomy\", \"term\"]\n",
        ),
        ("config/production/cms.toml", CMS),
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
        dir.path().join("config/production/cms.toml"),
        CMS.replace("owner/site", "no-slash"),
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
    for f in ["admin/cms.js", "admin/cms.css", "worker.js"] {
        let built = std::fs::read(out.join(f)).expect("built");
        let embedded = std::fs::read(repo.join("assets").join(f)).expect("embedded");
        assert!(
            built == embedded,
            "crates/cms/assets/{f} is not what crates/cms/web builds to: run tools/cms/build.sh"
        );
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
