//! Files below the mounts: Unicode names, symlinks, static files per module and language, include
//! and exclude globs, mount languages.

use super::*;

/// On macOS file names are NFC-normalised, as Go's are on darwin; the file is still read by
/// the name it has on disk.
#[test]
fn file_names_are_nfc_on_macos() {
    let p = Project::new(&[
        ("config.toml", ""),
        ("content/cafe\u{301}/Cafe\u{301}.md", ""),
    ]);
    let files = p.vfs().walk(Component::Content).unwrap();
    let want = if cfg!(target_os = "macos") {
        "caf\u{e9}/Caf\u{e9}.md"
    } else {
        "cafe\u{301}/Cafe\u{301}.md"
    };
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].rel, want);
    assert!(files[0].abs.is_file(), "{}", files[0].abs.display());
}

#[cfg(unix)]
#[test]
fn symlinks_below_a_mount_are_skipped() {
    let p = Project::new(&[
        ("config.toml", ""),
        ("content/a.md", ""),
        ("outside/b.md", ""),
    ]);
    std::os::unix::fs::symlink(p.dir.join("outside/b.md"), p.dir.join("content/b.md")).unwrap();
    std::os::unix::fs::symlink(p.dir.join("outside"), p.dir.join("content/dir")).unwrap();
    let vfs = p.vfs();
    assert_eq!(
        p.walk(&vfs, Component::Content),
        pairs(&[("a.md", "content/a.md")])
    );
    assert_eq!(vfs.open(Component::Content, "b.md"), None);
}

/// Static: of one module's mounts the last wins, the project still wins over a theme.
#[test]
fn static_later_mount_wins_within_a_module() {
    let p = Project::new(&[
        (
            "config.toml",
            "theme = \"t\"\n\
             [[module.mounts]]\nsource = \"static\"\ntarget = \"static\"\n\
             [[module.mounts]]\nsource = \"static2\"\ntarget = \"static\"\n",
        ),
        ("static/a.txt", ""),
        ("static/only.txt", ""),
        ("static2/a.txt", ""),
        ("themes/t/static/a.txt", ""),
        ("themes/t/static/t.txt", ""),
        ("layouts/x.html", ""),
        ("themes/t/layouts/x.html", ""),
    ]);
    let vfs = p.vfs();
    assert_eq!(
        p.walk(&vfs, Component::Static),
        pairs(&[
            ("a.txt", "static2/a.txt"),
            ("only.txt", "static/only.txt"),
            ("t.txt", "themes/t/static/t.txt"),
        ])
    );
    let open = |c, rel| vfs.open(c, rel).map(|f| p.origin(&f));
    assert_eq!(
        open(Component::Static, "a.txt").as_deref(),
        Some("static2/a.txt")
    );
    // Other components keep first-mount-wins.
    assert_eq!(
        open(Component::Layouts, "x.html").as_deref(),
        Some("layouts/x.html")
    );
}

/// Multihost: a static mount without a language (a configured one, a theme's) serves every
/// language, as Go copies it to every language's directory; a language's own mount still
/// wins for that language.
#[test]
fn multihost_static_without_language_serves_every_language() {
    let p = Project::new(&[
        (
            "config.toml",
            "theme = \"t\"\n\
             [languages.en]\nbaseURL = \"https://en.example.org/\"\nweight = 1\n\
             [languages.fr]\nbaseURL = \"https://fr.example.org/\"\nweight = 2\n\
             [[module.mounts]]\nsource = \"static\"\ntarget = \"static\"\n\
             [[module.mounts]]\nsource = \"static_fr\"\ntarget = \"static\"\nlang = \"fr\"\n",
        ),
        ("static/a.txt", ""),
        ("static_fr/a.txt", ""),
        ("themes/t/static/t.txt", ""),
        ("themes/t/layouts/x.html", ""),
    ]);
    let vfs = p.vfs();
    let files: Vec<(String, Option<usize>, String)> = vfs
        .walk(Component::Static)
        .unwrap()
        .iter()
        .map(|f| (f.rel.clone(), f.mount_lang.map(Idx::index), p.origin(f)))
        .collect();
    let want =
        |rel: &str, lang: usize, origin: &str| (rel.to_owned(), Some(lang), origin.to_owned());
    assert_eq!(
        files,
        [
            want("a.txt", 1, "static_fr/a.txt"),
            want("a.txt", 0, "static/a.txt"),
            want("t.txt", 0, "themes/t/static/t.txt"),
            want("t.txt", 1, "themes/t/static/t.txt"),
        ]
    );
}

#[cfg(unix)]
#[test]
fn static_follows_symlinks() {
    use std::os::unix::fs::symlink;
    let p = Project::new(&[
        ("config.toml", ""),
        ("static/real.txt", ""),
        ("outside/o.txt", ""),
        ("outside/dir/d.txt", ""),
    ]);
    symlink("real.txt", p.dir.join("static/link.txt")).unwrap();
    symlink("../outside/o.txt", p.dir.join("static/out.txt")).unwrap();
    symlink("../outside/dir", p.dir.join("static/dir")).unwrap();
    symlink("nope.txt", p.dir.join("static/broken")).unwrap();
    // Loops: to the mount root and to the directory itself.
    symlink("..", p.dir.join("outside/dir/up")).unwrap();
    symlink(".", p.dir.join("static/self")).unwrap();
    let vfs = p.vfs();
    assert_eq!(
        p.walk(&vfs, Component::Static),
        pairs(&[
            ("dir/d.txt", "static/dir/d.txt"),
            // `dir/up` is `outside`, not yet on the path, so it is walked; its `dir` is.
            ("dir/up/o.txt", "static/dir/up/o.txt"),
            ("link.txt", "static/link.txt"),
            ("out.txt", "static/out.txt"),
            ("real.txt", "static/real.txt"),
        ])
    );
    let open = |rel| vfs.open(Component::Static, rel).map(|f| p.origin(&f));
    assert_eq!(open("dir/d.txt").as_deref(), Some("static/dir/d.txt"));
    assert_eq!(open("broken"), None);
}

#[test]
fn include_and_exclude_files() {
    let p = Project::new(&[
        (
            "config.toml",
            "[[module.mounts]]\nsource = \"content\"\ntarget = \"content\"\n\
             excludeFiles = [\"**/drafts/**\", \"*.tmp\"]\n\
             [[module.mounts]]\nsource = \"docs\"\ntarget = \"content/docs\"\n\
             includeFiles = \"guide/**.md\"\n",
        ),
        ("content/a.md", ""),
        ("content/a.tmp", ""),
        ("content/s/b.tmp", ""),
        ("content/s/drafts/c.md", ""),
        ("docs/guide/one.md", ""),
        ("docs/guide/deep/two.md", ""),
        ("docs/guide/img.png", ""),
        ("docs/other/three.md", ""),
        ("docs/top.md", ""),
    ]);
    let vfs = p.vfs();
    let rels: Vec<String> = p
        .walk(&vfs, Component::Content)
        .into_iter()
        .map(|(r, _)| r)
        .collect();
    // A directory is walked when it matches an inclusion or leads to one (`/`, `/guide`):
    // `guide/**.md` does not open `guide/deep` (Go's rule; `guide/**` would).
    assert_eq!(rels, ["a.md", "docs/guide/one.md", "s/b.tmp"]);
}

#[test]
fn disabled_and_unknown_mount_languages() {
    let langs = "defaultContentLanguage = \"en\"\ndisableLanguages = [\"fr\"]\n\
                 [languages.en]\nweight = 1\n[languages.fr]\nweight = 2\n";
    let p = Project::new(&[
        (
            "config.toml",
            &format!(
                "{langs}[[module.mounts]]\nsource = \"content\"\ntarget = \"content\"\n\
                 [[module.mounts]]\nsource = \"content_fr\"\ntarget = \"content\"\nlang = \"fr\"\n"
            ),
        ),
        ("content/a.md", ""),
        ("content/a.fr.md", ""),
        ("content_fr/b.md", ""),
    ]);
    let vfs = p.vfs();
    let cfg = p.config();
    assert!(vfs.mounts().iter().any(ssg_vfs::Mount::is_disabled));
    // The disabled mount contributes nothing; a disabled language in the name drops the file.
    assert_eq!(
        p.walk(&vfs, Component::Content),
        pairs(&[("a.fr.md", "content/a.fr.md"), ("a.md", "content/a.md")])
    );
    let found = vfs
        .discover_content(&PathParser::from_config(&cfg))
        .unwrap();
    let keys: Vec<&str> = found.files.iter().map(|f| f.info.key.as_str()).collect();
    assert_eq!(keys, ["a"]);

    let p = Project::new(&[(
        "config.toml",
        "[[module.mounts]]\nsource = \"content\"\ntarget = \"content\"\nlang = \"xx\"\n",
    )]);
    fs::create_dir_all(p.dir.join("content")).unwrap();
    assert!(Vfs::new(&p.config()).is_err());
}
