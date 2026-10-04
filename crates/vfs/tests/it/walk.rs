//! Mount precedence, the union views, ignore rules, filters and content discovery on small
//! projects.

use std::fs;
use std::path::{Path, PathBuf};

use ssg_base::{Idx, LangIdx};
use ssg_config::{Config, LoadOptions, load};
use ssg_vfs::{BundleKind, Component, FileRef, Module, Parsed, PathParser, Vfs};

mod discover;
mod files;

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
