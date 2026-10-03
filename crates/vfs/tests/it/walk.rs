//! Mount precedence, the union views, ignore rules, filters and content discovery on small
//! projects.

use std::fs;
use std::path::{Path, PathBuf};

use ssg_base::{Idx, LangIdx};
use ssg_config::{Config, LoadOptions, load};
use ssg_vfs::{BundleKind, Component, FileRef, Module, Parsed, PathParser, Vfs};

struct Project {
    _tmp: tempfile::TempDir,
    dir: PathBuf,
}

impl Project {
    fn new(files: &[(&str, &str)]) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("site");
        for (name, content) in files {
            let p = dir.join(name);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, content).unwrap();
        }
        Self { _tmp: tmp, dir }
    }

    fn config(&self) -> Config {
        load(&LoadOptions {
            source: self.dir.clone(),
            env: vec![(
                "HOME".into(),
                self.dir.join("_home").to_str().unwrap().into(),
            )],
            ..LoadOptions::default()
        })
        .unwrap()
    }

    fn vfs(&self) -> Vfs {
        Vfs::new(&self.config()).unwrap()
    }

    /// `rel` of every file with the mount's source directory relative to the project.
    fn walk(&self, vfs: &Vfs, c: Component) -> Vec<(String, String)> {
        vfs.walk(c)
            .unwrap()
            .iter()
            .map(|f| (f.rel.clone(), self.origin(f)))
            .collect()
    }

    fn origin(&self, f: &FileRef) -> String {
        f.abs
            .strip_prefix(&self.dir)
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned()
    }
}

fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
    v.iter()
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect()
}

const THEMED: &[(&str, &str)] = &[
    ("config.toml", "theme = [\"t1\", \"t2\"]\n"),
    ("layouts/single.html", "p"),
    ("layouts/_partials/head.html", "p"),
    ("themes/t1/layouts/single.html", "t1"),
    ("themes/t1/layouts/list.html", "t1"),
    ("themes/t2/layouts/list.html", "t2"),
    ("themes/t2/layouts/baseof.html", "t2"),
    ("content/p.md", ""),
    ("themes/t1/content/p.md", ""),
    ("themes/t1/content/q.md", ""),
    ("data/x.toml", "a = 1"),
    ("themes/t1/data/x.toml", "a = 2"),
    ("themes/t2/i18n/en.toml", ""),
    ("i18n/en.toml", ""),
];

#[test]
fn project_before_themes_first_wins() {
    let p = Project::new(THEMED);
    let vfs = p.vfs();
    assert_eq!(
        p.walk(&vfs, Component::Layouts),
        pairs(&[
            ("_partials/head.html", "layouts/_partials/head.html"),
            ("baseof.html", "themes/t2/layouts/baseof.html"),
            ("list.html", "themes/t1/layouts/list.html"),
            ("single.html", "layouts/single.html"),
        ])
    );
    assert_eq!(
        p.walk(&vfs, Component::Content),
        pairs(&[("p.md", "content/p.md"), ("q.md", "themes/t1/content/q.md")])
    );
    // Data and i18n keep every file, project first.
    assert_eq!(
        p.walk(&vfs, Component::Data),
        pairs(&[
            ("x.toml", "data/x.toml"),
            ("x.toml", "themes/t1/data/x.toml")
        ])
    );
    assert_eq!(
        p.walk(&vfs, Component::I18n),
        pairs(&[
            ("en.toml", "i18n/en.toml"),
            ("en.toml", "themes/t2/i18n/en.toml")
        ])
    );
    let open = |rel| vfs.open(Component::Layouts, rel).map(|f| p.origin(&f));
    assert_eq!(open("single.html").as_deref(), Some("layouts/single.html"));
    assert_eq!(
        open("/list.html").as_deref(),
        Some("themes/t1/layouts/list.html")
    );
    assert_eq!(
        open("baseof.html").as_deref(),
        Some("themes/t2/layouts/baseof.html")
    );
    assert_eq!(open("none.html"), None);
    assert_eq!(open("_partials"), None);

    let modules: Vec<Module> = vfs
        .mounts_of(Component::Layouts)
        .map(|(_, m)| m.module)
        .collect();
    assert_eq!(
        modules,
        [Module::Project, Module::Theme(0), Module::Theme(1)]
    );
}

#[test]
fn missing_theme_is_an_error() {
    // The configuration finds the themes (it reads their configuration).
    let p = Project::new(&[("config.toml", "theme = \"nope\"\n")]);
    let e = load(&LoadOptions {
        source: p.dir.clone(),
        ..LoadOptions::default()
    })
    .expect_err("missing theme");
    assert!(
        matches!(e, ssg_config::ConfigError::ThemeNotFound { .. }),
        "{e}"
    );
    // A theme directory removed after loading.
    let p = Project::new(&[
        ("config.toml", "theme = \"gone\"\n"),
        ("themes/gone/layouts/x.html", ""),
    ]);
    let cfg = p.config();
    fs::remove_dir_all(p.dir.join("themes/gone")).unwrap();
    assert!(matches!(
        Vfs::new(&cfg),
        Err(ssg_vfs::VfsError::ThemeNotFound { .. })
    ));
}

/// Nested themes, `[[module.imports]]` options and a theme's own `[[module.mounts]]` (with a
/// language), and JS config files of a theme.
#[test]
fn theme_mounts_and_nested_themes() {
    let p = Project::new(&[
        (
            "config.toml",
            "theme = [\"a\", \"b\", \"A\"]\n\
             [[module.imports]]\npath = \"m\"\n\
             [[module.imports.mounts]]\nsource = \"src\"\ntarget = \"assets/m\"\n\
             [[module.imports]]\npath = \"x\"\nnoMounts = true\n",
        ),
        // `a` imports `c`: a, c, b (depth first); `A` is `a` again.
        ("themes/a/config.toml", "theme = \"c\"\n"),
        ("themes/a/layouts/single.html", "a"),
        ("themes/a/package.json", "{}"),
        ("themes/b/layouts/single.html", "b"),
        ("themes/b/layouts/list.html", "b"),
        ("themes/c/layouts/list.html", "c"),
        ("themes/c/layouts/baseof.html", "c"),
        // The import's mounts win over the theme's own.
        (
            "themes/m/config.toml",
            "[[module.mounts]]\nsource = \"layouts\"\ntarget = \"layouts\"\n",
        ),
        ("themes/m/src/m.css", ""),
        ("themes/m/layouts/single.html", "m"),
        ("themes/x/layouts/single.html", "x"),
    ]);
    let cfg = p.config();
    let paths: Vec<&str> = cfg.themes.iter().map(|t| t.path.as_str()).collect();
    assert_eq!(paths, ["m", "x", "a", "c", "b"]);
    let vfs = Vfs::new(&cfg).unwrap();
    assert_eq!(
        p.walk(&vfs, Component::Layouts),
        pairs(&[
            ("baseof.html", "themes/c/layouts/baseof.html"),
            ("list.html", "themes/c/layouts/list.html"),
            ("single.html", "themes/a/layouts/single.html"),
        ]),
        "m mounts only src (its import's mounts), x nothing; c before b"
    );
    assert_eq!(
        p.walk(&vfs, Component::Assets),
        pairs(&[
            ("_jsconfig/package.json", "themes/a/package.json"),
            ("m/m.css", "themes/m/src/m.css"),
        ])
    );

    // A theme's own mounts, with a language.
    let p = Project::new(&[
        (
            "config.toml",
            "theme = \"t\"\n[languages.en]\nweight = 1\n[languages.nn]\nweight = 2\n",
        ),
        (
            "themes/t/config.toml",
            "[[module.mounts]]\nsource = \"content/nn\"\ntarget = \"content\"\nlang = \"nn\"\n\
             [[module.mounts]]\nsource = \"missing\"\ntarget = \"static\"\n",
        ),
        ("themes/t/content/nn/p.md", ""),
        ("themes/t/layouts/single.html", "not mounted"),
    ]);
    let vfs = p.vfs();
    let files = vfs.walk(Component::Content).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].rel, "p.md");
    assert_eq!(files[0].mount_lang, Some(LangIdx::from_index(1)));
    assert!(vfs.walk(Component::Layouts).unwrap().is_empty());
}

#[test]
fn content_is_merged_per_language() {
    let p = Project::new(&[
        (
            "config.toml",
            "defaultContentLanguage = \"en\"\n\
             [languages.en]\nweight = 1\n\
             [languages.th]\nweight = 2\ncontentDir = \"content_th\"\n\
             [[module.mounts]]\nsource = \"content\"\ntarget = \"content\"\n\
             [[module.mounts]]\nsource = \"extra\"\ntarget = \"content\"\n\
             [[module.mounts]]\nsource = \"content_th\"\ntarget = \"content\"\nlang = \"th\"\n",
        ),
        ("content/p.md", ""),
        ("extra/p.md", ""),
        ("extra/x.md", ""),
        ("content_th/p.md", ""),
    ]);
    let vfs = p.vfs();
    // The same path in another language is kept; in the same language the first mount wins.
    assert_eq!(
        p.walk(&vfs, Component::Content),
        pairs(&[
            ("p.md", "content/p.md"),
            ("p.md", "content_th/p.md"),
            ("x.md", "extra/x.md")
        ])
    );
    let langs: Vec<Option<LangIdx>> = vfs
        .walk(Component::Content)
        .unwrap()
        .iter()
        .map(|f| f.mount_lang)
        .collect();
    assert_eq!(langs, [None, Some(LangIdx::from_index(1)), None]);
}

#[test]
fn mounts_below_a_component_and_single_files() {
    let p = Project::new(&[
        (
            "config.toml",
            "[[module.mounts]]\nsource = \"assets\"\ntarget = \"assets\"\n\
             [[module.mounts]]\nsource = \"node_modules/lib\"\ntarget = \"assets/vendor/lib\"\n\
             [[module.mounts]]\nsource = \"generated.json\"\n\
             target = \"assets/notwatching/generated.json\"\n\
             [[module.mounts]]\nsource = \"missing.json\"\ntarget = \"assets/missing.json\"\n",
        ),
        ("assets/main.css", ""),
        ("generated.json", "{}"),
        ("node_modules/lib/dist/lib.js", ""),
        ("package.json", "{}"),
        ("tailwind.config.js", ""),
        ("package.config.json", "{}"),
    ]);
    let vfs = p.vfs();
    assert_eq!(
        p.walk(&vfs, Component::Assets),
        pairs(&[
            ("_jsconfig/package.config.json", "package.config.json"),
            ("_jsconfig/package.json", "package.json"),
            ("main.css", "assets/main.css"),
            ("notwatching/generated.json", "generated.json"),
            ("vendor/lib/dist/lib.js", "node_modules/lib/dist/lib.js"),
        ])
    );
    let open = |rel| vfs.open(Component::Assets, rel).map(|f| p.origin(&f));
    assert_eq!(
        open("vendor/lib/dist/lib.js").as_deref(),
        Some("node_modules/lib/dist/lib.js")
    );
    assert_eq!(open("vendor/lib"), None);
    // A single file mounts like a directory; a missing source is skipped.
    assert_eq!(
        open("notwatching/generated.json").as_deref(),
        Some("generated.json")
    );
    assert!(
        !vfs.mounts()
            .iter()
            .any(|m| m.target == "assets/missing.json")
    );
}

#[test]
fn ignore_rules_per_component() {
    let p = Project::new(&[
        (
            "config.toml",
            "ignoreFiles = [\"\\\\.draft\\\\.md$\", \"/private/\"]\n",
        ),
        ("content/a.md", ""),
        ("content/.hidden.md", ""),
        ("content/#autosave.md", ""),
        ("content/b.md~", ""),
        ("content/c.draft.md", ""),
        ("content/.git/d.md", ""),
        ("content/private/e.md", ""),
        ("layouts/single.html", ""),
        ("layouts/.single.html", ""),
        ("layouts/list.html~", ""),
        ("layouts/.dir/x.html", ""),
        ("layouts/#x.html", ""),
        ("static/.well-known/a.txt", ""),
        ("static/.DS_Store", ""),
        ("data/.x.toml", ""),
        ("data/y.toml", ""),
    ]);
    let vfs = p.vfs();
    let rels = |c| -> Vec<String> { p.walk(&vfs, c).into_iter().map(|(r, _)| r).collect() };
    assert_eq!(rels(Component::Content), ["a.md"]);
    assert_eq!(
        rels(Component::Layouts),
        ["#x.html", ".dir/x.html", "single.html"]
    );
    assert_eq!(rels(Component::Static), [".DS_Store", ".well-known/a.txt"]);
    assert_eq!(rels(Component::Data), ["y.toml"]);
    assert_eq!(vfs.open(Component::Content, "c.draft.md"), None);
    assert_eq!(vfs.open(Component::Content, ".git/d.md"), None);
    assert!(vfs.open(Component::Content, "a.md").is_some());
}

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

/// (rel, key, kind, language key) of a discovered file.
type Found = (String, String, BundleKind, String);

/// Every discovered file, and the dropped duplicates.
fn discover(p: &Project) -> (Vec<Found>, Vec<PathBuf>) {
    let cfg = p.config();
    let found = p
        .vfs()
        .discover_content(&PathParser::from_config(&cfg))
        .unwrap();
    let files = found
        .files
        .iter()
        .map(|f| {
            (
                f.file.rel.clone(),
                f.info.key.as_str().to_owned(),
                f.info.kind,
                cfg.sites[f.lang].language.key.clone(),
            )
        })
        .collect();
    let dropped = found
        .duplicates
        .iter()
        .map(|d| d.dropped.strip_prefix(&p.dir).unwrap().to_path_buf())
        .collect();
    (files, dropped)
}

fn rec(rel: &str, key: &str, kind: BundleKind, lang: &str) -> Found {
    (rel.to_owned(), key.to_owned(), kind, lang.to_owned())
}

#[test]
fn leaf_bundles_and_duplicates() {
    use BundleKind::{Branch, ContentResource, Leaf, Resource, Single};
    let p = Project::new(&[
        (
            "config.toml",
            "defaultContentLanguage = \"en\"\n[languages.en]\nweight = 1\n\
             [languages.th]\nweight = 2\n",
        ),
        ("content/_index.md", ""),
        ("content/b/index.md", ""),
        ("content/b/index.th.md", ""),
        ("content/b/_index.md", ""),
        ("content/b/notes.th.md", ""),
        ("content/b/img.jpg", ""),
        ("content/b/sub/index.md", ""),
        ("content/b/sub/data.json", ""),
        ("content/s/_index.md", ""),
        ("content/s/index.md", ""),
        ("content/s/p.md", ""),
        ("content/s/p.html", ""),
        ("content/s/x.md", ""),
        ("content/s/x/_index.md", ""),
        ("content/s/y.th.md", ""),
        ("content/s/y.md", ""),
        ("content/s/img.png", ""),
    ]);
    let (files, dropped) = discover(&p);
    assert_eq!(
        files,
        [
            rec("_index.md", "", Branch, "en"),
            rec("b/index.md", "b", Leaf, "en"),
            rec("b/index.th.md", "b", Leaf, "th"),
            rec("b/_index.md", "b/_index.md", ContentResource, "en"),
            rec("b/img.jpg", "b/img.jpg", Resource, "en"),
            rec("b/notes.th.md", "b/notes.md", ContentResource, "th"),
            rec("b/sub/data.json", "b/sub/data.json", Resource, "en"),
            rec("b/sub/index.md", "b/sub/index.md", ContentResource, "en"),
            rec("s/_index.md", "s", Branch, "en"),
            rec("s/img.png", "s/img.png", Resource, "en"),
            rec("s/p.md", "s/p", Single, "en"),
            rec("s/x/_index.md", "s/x", Branch, "en"),
            rec("s/y.md", "s/y", Single, "en"),
            rec("s/y.th.md", "s/y", Single, "th"),
        ]
    );
    // `_index` before `index` and a bundle before a single page; `md` before `html`.
    assert_eq!(
        dropped,
        [
            PathBuf::from("content/s/index.md"),
            PathBuf::from("content/s/p.html"),
            PathBuf::from("content/s/x.md"),
        ]
    );
}

#[test]
fn a_leaf_bundle_at_the_root_owns_everything() {
    let p = Project::new(&[
        ("config.toml", ""),
        ("content/index.md", ""),
        ("content/a.md", ""),
        ("content/s/_index.md", ""),
    ]);
    let (files, _) = discover(&p);
    let kinds: Vec<(&str, BundleKind)> = files.iter().map(|f| (f.1.as_str(), f.2)).collect();
    assert_eq!(
        kinds,
        [
            ("", BundleKind::Leaf),
            ("a.md", BundleKind::ContentResource),
            ("s/_index.md", BundleKind::ContentResource),
        ]
    );
}

#[test]
fn default_mounts_follow_the_dirs() {
    let p = Project::new(&[
        (
            "config.toml",
            "contentDir = \"c\"\nstaticDir = [\"s1\", \"s2\"]\n",
        ),
        ("c/a.md", ""),
        ("s1/x.txt", "1"),
        ("s2/x.txt", "2"),
        ("s2/y.txt", ""),
    ]);
    let vfs = p.vfs();
    assert_eq!(
        p.walk(&vfs, Component::Content),
        pairs(&[("a.md", "c/a.md")])
    );
    // Static: the later static dir wins (Go's static copy; this test expected `s1/x.txt`
    // while the vfs applied first-mount-wins to static too).
    assert_eq!(
        p.walk(&vfs, Component::Static),
        pairs(&[("x.txt", "s2/x.txt"), ("y.txt", "s2/y.txt")])
    );
    let open = vfs.open(Component::Static, "x.txt").map(|f| p.origin(&f));
    assert_eq!(open.as_deref(), Some("s2/x.txt"));
    let dir: &Path = &p.dir;
    assert!(vfs.mounts().iter().all(|m| m.abs.starts_with(dir)));
}

/// Discovery on whole sites: `FUGO_VFS_SITES=<dir>:<dir> cargo test -p ssg-vfs --
/// --ignored discover_sites` (sites from `tools/rust-port/i01/sites.py make <site> <dir>`).
#[test]
#[ignore = "needs FUGO_VFS_SITES"]
fn discover_sites() {
    let dirs = std::env::var("FUGO_VFS_SITES").expect("FUGO_VFS_SITES");
    for dir in dirs.split(':') {
        let cfg = load(&LoadOptions {
            source: PathBuf::from(dir),
            ..LoadOptions::default()
        })
        .unwrap();
        let vfs = Vfs::new(&cfg).unwrap();
        let found = vfs
            .discover_content(&PathParser::from_config(&cfg))
            .unwrap();
        let pages = found.files.iter().filter(|f| f.info.kind.is_page()).count();
        let walked: Vec<String> = [
            Component::Layouts,
            Component::Assets,
            Component::Data,
            Component::I18n,
            Component::Static,
        ]
        .iter()
        .map(|&c| format!("{c} {}", vfs.walk(c).unwrap().len()))
        .collect();
        eprintln!(
            "{dir}: {} mounts; content {} pages, {} resources, {} duplicates; {}",
            vfs.mounts().len(),
            pages,
            found.files.len() - pages,
            found.duplicates.len(),
            walked.join(", ")
        );
    }
}

/// Content adapters (`_content.html`, Go's `_content.gotmpl`) are listed apart from the
/// pages: they share their directory with the section's `_index.md`, one per directory and
/// language; a disabled language's adapter is dropped. `_content.html` is an adapter only in
/// the content component.
#[test]
fn content_adapters_are_apart() {
    let p = Project::new(&[
        (
            "config.toml",
            "defaultContentLanguage = \"en\"\n[languages.en]\nweight = 1\n\
             [languages.th]\nweight = 2\n[languages.fr]\nweight = 3\ndisabled = true\n",
        ),
        ("content/news/_index.md", ""),
        ("content/news/_content.html", ""),
        ("content/news/_content.gotmpl", ""),
        ("content/news/_content.th.html", ""),
        ("content/news/_content.fr.html", ""),
        ("content/_content.html", ""),
        ("content/docs/_content.md", ""),
    ]);
    let cfg = p.config();
    let found = p
        .vfs()
        .discover_content(&PathParser::from_config(&cfg))
        .unwrap();
    let adapters: Vec<(&str, &str, &str)> = found
        .adapters
        .iter()
        .map(|f| {
            assert_eq!(f.info.kind, BundleKind::ContentAdapter);
            (
                f.file.rel.as_str(),
                f.info.key.as_str(),
                cfg.sites[f.lang].language.key.as_str(),
            )
        })
        .collect();
    assert_eq!(
        adapters,
        [
            ("_content.html", "", "en"),
            ("news/_content.html", "news", "en"),
            ("news/_content.th.html", "news", "th"),
        ]
    );
    let dropped: Vec<PathBuf> = found
        .duplicates
        .iter()
        .map(|d| d.dropped.strip_prefix(&p.dir).unwrap().to_path_buf())
        .collect();
    assert_eq!(dropped, [PathBuf::from("content/news/_content.gotmpl")]);
    let pages: Vec<(&str, BundleKind)> = found
        .files
        .iter()
        .map(|f| (f.file.rel.as_str(), f.info.kind))
        .collect();
    assert_eq!(
        pages,
        [
            ("docs/_content.md", BundleKind::Single),
            ("news/_index.md", BundleKind::Branch)
        ]
    );

    let parser = PathParser::from_config(&cfg);
    let Parsed::File(layout) = parser.parse(Component::Layouts, "_content.html") else {
        panic!("a file");
    };
    assert_ne!(layout.kind, BundleKind::ContentAdapter);
}
