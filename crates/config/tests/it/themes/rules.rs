//! The rules per case: languages, theme-only settings, trees, mounts, missing and vendored themes,
//! errors.

use super::*;

/// Without `[languages]` in the project, the languages a theme defines are added (the
/// implicit language takes nothing); with `[languages]`, the project's list is kept.
#[test]
fn languages_from_a_theme() {
    let theme = "[languages.en]\ntitle = \"Theme EN\"\n[languages.en.params]\nfromTheme = true\n[languages.fr]\nweight = 2\ntitle = \"Thème\"\n[languages.fr.params]\nfr = true\n";
    let p = Project::new(&[
        ("config.toml", "theme = \"t\"\n"),
        ("themes/t/config.toml", theme),
    ]);
    let c = p.ok();
    let langs: Vec<&str> = c.sites.iter().map(|s| s.language.key.as_str()).collect();
    assert_eq!(langs, ["en", "fr"]);
    let en = c.site("en").expect("en");
    assert_eq!(en.language.title, "");
    assert!(en.params.get("fromtheme").is_none());
    let fr = c.site("fr").expect("fr");
    assert_eq!(fr.title, "Thème");
    assert_eq!(fr.params.get("fr"), Some(&Value::Bool(true)));

    // A project language takes the theme's params for it; the list stays the project's.
    let p = Project::new(&[
        ("config.toml", "theme = \"t\"\n[languages.en]\nweight = 1\n"),
        ("themes/t/config.toml", theme),
    ]);
    let c = p.ok();
    assert_eq!(c.sites.len(), 1);
    assert_eq!(
        c.default_site().params.get("fromtheme"),
        Some(&Value::Bool(true))
    );
    assert_eq!(c.default_site().language.title, "");

    // No languages anywhere: the implicit language is not a configured language.
    let p = Project::new(&[
        ("config.toml", "theme = \"t\"\n"),
        ("themes/t/config.toml", "[params]\np = 1\n"),
    ]);
    let c = p.ok();
    assert!(c.raw.get("languages").is_none());
}

/// A theme cannot move the themes directory, add themes to the project or change the
/// project's mounts, even with the root `deep`.
#[test]
fn theme_only_settings() {
    let p = Project::new(&[
        ("config.toml", "theme = \"t\"\n_merge = \"deep\"\n"),
        (
            "themes/t/config.toml",
            "themesDir = \"elsewhere\"\ntheme = \"u\"\n[[module.mounts]]\nsource = \"layouts\"\ntarget = \"layouts\"\n[[module.imports]]\npath = \"u\"\n[params]\np = 1\n",
        ),
        ("themes/t/layouts/x.html", ""),
        ("themes/u/layouts/y.html", ""),
    ]);
    let c = p.ok();
    assert_eq!(c.dirs.themes, Path::new("themes"));
    assert!(c.mounts.is_empty());
    let graph: Vec<(Option<&str>, &str)> = c
        .themes
        .iter()
        .map(|t| (t.owner.as_deref(), t.path.as_str()))
        .collect();
    assert_eq!(graph, [(None, "t"), (Some("t"), "u")]);
    assert_eq!(params(&c), toml("p = 1\n"));
}

#[test]
fn the_merge_on_trees() {
    let Value::Map(project) = toml(
        "[params]\n_merge = \"shallow\"\na = 1\n[params.sub]\nb = 1\n[markup.goldmark]\nx = 1\n",
    ) else {
        unreachable!()
    };
    let Value::Map(theme) = toml(
        "[params]\nc = 3\n[params.sub]\nd = 4\n[markup.goldmark]\ny = 2\n[markup.other]\nz = 3\n",
    ) else {
        unreachable!()
    };
    let mut root: Map = (*project).clone();
    merge_themes(&mut root, [&*theme]);
    let strip = |v: &Value| ssg_config::tree::strip_merge(v);
    assert_eq!(
        strip(&Value::map(root)),
        toml("[params]\na = 1\nc = 3\n[params.sub]\nb = 1\n[markup.goldmark]\nx = 1\n")
    );
    assert_eq!(
        MergeStrategy::parse(&Value::string("NONE")),
        MergeStrategy::None
    );
    assert_eq!(
        MergeStrategy::parse(&Value::string("sideways")),
        MergeStrategy::Deep
    );
    assert_eq!(
        MergeStrategy::default_for(&["languages", "en", "menus"], Some(MergeStrategy::None)),
        MergeStrategy::Shallow
    );
    assert_eq!(
        MergeStrategy::default_for(&["markup"], None),
        MergeStrategy::None
    );
    assert_eq!(
        MergeStrategy::default_for(&["markup"], Some(MergeStrategy::Deep)),
        MergeStrategy::Deep
    );
}

#[test]
fn theme_mounts_and_import_options() {
    let p = Project::new(&[
        (
            "config.toml",
            "[[module.imports]]\npath = \"noconf\"\nignoreConfig = true\n\
             [[module.imports]]\npath = \"noimports\"\nignoreImports = true\n\
             [[module.imports]]\npath = \"nomounts\"\nnoMounts = true\n\
             [[module.imports]]\npath = \"disabled\"\ndisable = true\n\
             [[module.imports]]\npath = \"withmounts\"\n\
             [[module.imports.mounts]]\nsource = \"src\"\ntarget = \"assets/src\"\n\
             [[module.imports]]\npath = \"own\"\n",
        ),
        (
            "themes/noconf/config.toml",
            "theme = \"deep\"\n[params]\nignored = true\n",
        ),
        ("themes/noimports/config.toml", "theme = \"deep\"\n"),
        (
            "themes/nomounts/config.toml",
            "[params]\nfromNomounts = true\n",
        ),
        ("themes/withmounts/src/x.css", ""),
        (
            "themes/own/config.toml",
            "[[module.mounts]]\nsource = \"files\"\ntarget = \"static\"\n",
        ),
    ]);
    let c = p.ok();
    let got: Vec<(&str, &ThemeMounts)> = c
        .themes
        .iter()
        .map(|t| (t.path.as_str(), &t.mounts))
        .collect();
    let configured = |source: &str, target: &str| {
        ThemeMounts::Configured(vec![ssg_config::MountConfig {
            source: source.into(),
            target: target.into(),
            ..ssg_config::MountConfig::default()
        }])
    };
    assert_eq!(
        got,
        [
            ("noconf", &ThemeMounts::Components),
            ("noimports", &ThemeMounts::Components),
            ("nomounts", &ThemeMounts::None),
            ("withmounts", &configured("src", "assets/src")),
            ("own", &configured("files", "static")),
        ]
    );
    assert!(c.themes[0].config_files.is_empty());
    assert_eq!(params(&c), toml("fromnomounts = true\n"));
    // `config` prints the themes (JSON and TOML).
    let json = serde_json::to_value(&c).expect("json");
    assert_eq!(
        json["themes"][3]["mounts"]["Configured"][0]["source"],
        "src"
    );
    assert!(::toml::to_string(&c).expect("toml").contains("[[themes]]"));
}

#[test]
fn themes_that_are_not_found() {
    let p = Project::new(&[("config.toml", "theme = \"nothere\"\n")]);
    let e = p.load().expect_err("missing");
    assert!(matches!(e, ConfigError::ThemeNotFound { .. }), "{e}");
    assert!(e.to_string().contains("themes/nothere"), "{e}");

    // A theme of a theme outside themesDir; the project itself may import any path.
    let p = Project::new(&[
        ("config.toml", "theme = \"a\"\n"),
        (
            "themes/a/config.toml",
            "[[module.imports]]\npath = \"../../x\"\n",
        ),
        ("x/layouts/x.html", ""),
    ]);
    let e = p.load().expect_err("outside");
    assert!(
        matches!(e, ConfigError::ThemeOutsideThemesDir { .. }),
        "{e}"
    );
    let p = Project::new(&[
        ("config.toml", "[[module.imports]]\npath = \"../shared\"\n"),
        ("shared/config.toml", "[params]\nshared = true\n"),
    ]);
    assert_eq!(params(&p.ok()), toml("shared = true\n"));

    // `themesDir` from the project.
    let p = Project::new(&[
        (
            "config.toml",
            "themesDir = \"../shared-themes\"\ntheme = \"t\"\n",
        ),
        (
            "../shared-themes/t/config.toml",
            "[params]\nshared = true\n",
        ),
    ]);
    let c = p.ok();
    assert_eq!(params(&c), toml("shared = true\n"));
}

#[test]
fn vendored_themes() {
    let p = Project::new(&[
        (
            "config.toml",
            "[[module.imports]]\npath = \"github.com/me/vtheme\"\n",
        ),
        (
            "_vendor/modules.txt",
            "# github.com/me/vtheme v1.2.3\n# github.com/me/other v0.1.0\n",
        ),
        (
            "_vendor/github.com/me/vtheme/config.toml",
            "[params]\nvendored = true\n[[module.imports]]\npath = \"github.com/me/other\"\n",
        ),
        ("_vendor/github.com/me/other/assets/a.css", ""),
    ]);
    let c = p.ok();
    let got: Vec<(&str, Option<&str>)> = c
        .themes
        .iter()
        .map(|t| (t.path.as_str(), t.vendored.as_deref()))
        .collect();
    assert_eq!(
        got,
        [
            ("github.com/me/vtheme", Some("v1.2.3")),
            ("github.com/me/other", Some("v0.1.0"))
        ]
    );
    assert_eq!(params(&c), toml("vendored = true\n"));

    // `ignoreVendorPaths`: looked up in the themes directory instead (and not found there).
    let p = Project::new(&[
        (
            "config.toml",
            "ignoreVendorPaths = \"github.com/me/*\"\n[[module.imports]]\npath = \"github.com/me/vtheme\"\n",
        ),
        ("_vendor/modules.txt", "# github.com/me/vtheme v1.2.3\n"),
        ("_vendor/github.com/me/vtheme/layouts/x.html", ""),
    ]);
    assert!(matches!(p.load(), Err(ConfigError::ThemeNotFound { .. })));

    let p = Project::new(&[
        (
            "config.toml",
            "[[module.imports]]\npath = \"github.com/me/vtheme\"\n",
        ),
        ("_vendor/modules.txt", "# github.com/me/vtheme\n"),
    ]);
    let e = p.load().expect_err("invalid modules.txt");
    assert_eq!(e.position().map(|p| p.line), Some(1), "{e}");
}

/// Errors in a theme's configuration point at the theme's file.
#[test]
fn errors_in_theme_configuration() {
    let p = Project::new(&[
        ("config.toml", "theme = \"t\"\n"),
        ("themes/t/config.toml", "[params\nbroken = 1\n"),
    ]);
    let e = p.load().expect_err("syntax");
    let pos = e.position().expect("position");
    assert!(pos.file.ends_with("themes/t/config.toml"), "{e}");

    let p = Project::new(&[
        ("config.toml", "theme = \"t\"\n"),
        (
            "themes/t/config.toml",
            "\n[outputFormats.bad]\nmediaType = \"text/nope\"\n",
        ),
    ]);
    let e = p.load().expect_err("unknown media type");
    let pos = e.position().expect("position");
    assert!(
        pos.file.ends_with("themes/t/config.toml") && pos.line > 0,
        "{e}"
    );

    let p = Project::new(&[
        ("config.toml", "theme = \"t\"\n"),
        (
            "themes/t/config.toml",
            "[[module.mounts]]\nsource = \"x\"\ntarget = \"nowhere\"\n",
        ),
    ]);
    let e = p.load().expect_err("mount target");
    assert!(e.to_string().contains("module.mounts[0].target"), "{e}");
    assert!(
        e.position()
            .is_some_and(|p| p.file.ends_with("themes/t/config.toml")),
        "{e}"
    );
}
