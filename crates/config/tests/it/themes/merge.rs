//! Merging the themes' configuration below the project's: Go's tests of `_merge`.

use super::*;

/// Go's `config_test.go` `TestLoadConfigFromThemes`: the project's and the theme's
/// configuration (the Go test's `mainConfigTemplate` and `themeConfig`).
pub(super) const MAIN_CONFIG: &str = r#"
theme = "test-theme"
baseURL = "https://example.com/"

[frontmatter]
date = ["date","publishDate"]

[params]
MERGE_PARAMS
p1 = "p1 main"
[params.b]
b1 = "b1 main"
[params.b.c]
bc1 = "bc1 main"

[mediaTypes]
[mediaTypes."text/m1"]
suffixes = ["m1main"]

[outputFormats.o1]
mediaType = "text/m1"
baseName = "o1main"

[languages]
[languages.en]
languageName = "English"
[languages.en.params]
pl1 = "p1-en-main"
[languages.nb]
languageName = "Norsk"
[languages.nb.params]
pl1 = "p1-nb-main"

[[menu.main]]
name = "menu-main-main"

[[menu.top]]
name = "menu-top-main"
"#;

pub(super) const THEME_CONFIG: &str = r#"
baseURL = "http://bep.is/"

# Can not be set in theme.
disableKinds = ["taxonomy", "term"]

# Can not be set in theme.
[frontmatter]
expiryDate = ["date"]

[params]
p1 = "p1 theme"
p2 = "p2 theme"
[params.b]
b1 = "b1 theme"
b2 = "b2 theme"
[params.b.c]
bc1 = "bc1 theme"
bc2 = "bc2 theme"
[params.b.c.d]
bcd1 = "bcd1 theme"

[mediaTypes]
[mediaTypes."text/m1"]
suffixes = ["m1theme"]
[mediaTypes."text/m2"]
suffixes = ["m2theme"]

[outputFormats.o1]
mediaType = "text/m1"
baseName = "o1theme"
[outputFormats.o2]
mediaType = "text/m2"
baseName = "o2theme"

[languages]
[languages.en]
languageName = "English2"
[languages.en.params]
pl1 = "p1-en-theme"
pl2 = "p2-en-theme"
[[languages.en.menu.main]]
name   = "menu-lang-en-main"
[[languages.en.menu.theme]]
name   = "menu-lang-en-theme"
[languages.nb]
languageName = "Norsk2"
[languages.nb.params]
pl1 = "p1-nb-theme"
pl2 = "p2-nb-theme"
top = "top-nb-theme"
[[languages.nb.menu.main]]
name   = "menu-lang-nb-main"
[[languages.nb.menu.theme]]
name   = "menu-lang-nb-theme"
[[languages.nb.menu.top]]
name   = "menu-lang-nb-top"

[[menu.main]]
name = "menu-main-theme"

[[menu.thememenu]]
name = "menu-theme"
"#;

pub(super) fn with_theme(main: &str, theme: &str) -> Config {
    Project::new(&[
        ("config.toml", main),
        ("themes/test-theme/config.toml", theme),
    ])
    .ok()
}

/// The project-wide (root) `params` of `c`, without the language's own.
pub(super) fn root_params(c: &Config) -> Value {
    c.raw.get("params").cloned().unwrap_or_default()
}

#[test]
fn load_config_from_themes_merge_default() {
    let c = with_theme(&MAIN_CONFIG.replace("MERGE_PARAMS", ""), THEME_CONFIG);
    assert_eq!(
        root_params(&c),
        toml(
            r#"
p1 = "p1 main"
p2 = "p2 theme"
[b]
b1 = "b1 main"
b2 = "b2 theme"
[b.c]
bc1 = "bc1 main"
bc2 = "bc2 theme"
[b.c.d]
bcd1 = "bcd1 theme"
"#
        )
    );
    assert_eq!(c.default_site().base_url.as_str(), "https://example.com/");

    // What the rules give for the rest of this configuration: root values that are not tables
    // and `none` tables stay the project's …
    let en = c.site("en").expect("en");
    assert!(!en.disable_kinds.contains(PageKind::Taxonomy));
    assert!(
        c.raw
            .get("frontmatter")
            .and_then(|f| f.as_map())
            .is_some_and(|f| !f.contains_key("expirydate"))
    );
    // … the language list too (`languages` is `none`), while each language's `params` merge
    // deeply and its name stays the project's.
    let langs: Vec<&str> = c.sites.iter().map(|s| s.language.key.as_str()).collect();
    assert_eq!(langs, ["en", "nb"]);
    assert_eq!(en.language.name, "English");
    assert_eq!(en.params.get("pl1"), Some(&Value::string("p1-en-main")));
    assert_eq!(en.params.get("pl2"), Some(&Value::string("p2-en-theme")));
    let nb = c.site("nb").expect("nb");
    assert_eq!(nb.params.get("top"), Some(&Value::string("top-nb-theme")));
    // `mediaTypes` and `outputFormats` are `shallow`: new types and formats only.
    let m1 = c.media_types.by_type("text/m1").expect("text/m1");
    assert_eq!(c.media_types.get(m1).suffixes, ["m1main"]);
    assert!(c.media_types.by_type("text/m2").is_some());
    let format = |name: &str| {
        c.output_formats
            .get(c.output_formats.by_name(name).expect(name))
    };
    assert_eq!(format("o1").base_name, "o1main");
    assert_eq!(format("o2").base_name, "o2theme");
    // `menus` is `shallow`: the theme's new menu `thememenu`, not its `main` entries. The
    // languages have no menus of their own, so the theme's language menus are not taken.
    let mut menus: Vec<(&str, &str)> = en
        .menus
        .iter()
        .map(|e| (e.menu.as_str(), e.name.as_str()))
        .collect();
    menus.sort_unstable();
    assert_eq!(
        menus,
        [
            ("main", "menu-main-main"),
            ("thememenu", "menu-theme"),
            ("top", "menu-top-main")
        ]
    );
}

#[test]
fn load_config_from_themes_merge_shallow() {
    let c = with_theme(
        &MAIN_CONFIG.replace("MERGE_PARAMS", "_merge = \"shallow\""),
        THEME_CONFIG,
    );
    // Shallow merge, only add new keys to params.
    assert_eq!(
        root_params(&c),
        toml(
            r#"
p1 = "p1 main"
p2 = "p2 theme"
[b]
b1 = "b1 main"
[b.c]
bc1 = "bc1 main"
"#
        )
    );
}

#[test]
fn load_config_from_themes_no_params_in_project() {
    let c = with_theme(
        "baseURL=\"https://example.org\"\ntheme = \"test-theme\"\n",
        "[params]\np1 = \"p1 theme\"\n",
    );
    assert_eq!(root_params(&c), toml("p1 = \"p1 theme\"\n"));
}

/// Issues #8724, #13643: a `[sitemap]` of the theme with the root's `_merge` `none` (ignored)
/// or `shallow` (taken, as the project has none).
#[test]
fn load_config_from_themes_sitemap_by_root_strategy() {
    let theme = "baseURL=\"http://example.com\"\n[sitemap]\nchangefreq = \"monthly\"\nfilename = \"sitemap.xml\"\npriority = 0.5\n";
    for (strategy, freq, priority) in [("none", "", -1.0), ("shallow", "monthly", 0.5)] {
        let c = with_theme(
            &format!(
                "_merge={strategy:?}\nbaseURL=\"https://example.org\"\ntheme = \"test-theme\"\n"
            ),
            theme,
        );
        let s = &c.default_site().sitemap;
        assert_eq!(s.change_freq, freq, "{strategy}");
        assert!((s.priority - priority).abs() < f64::EPSILON, "{strategy}");
        assert_eq!(s.filename, "sitemap.xml");
        assert_eq!(c.default_site().base_url.as_str(), "https://example.org/");
    }
}

/// `TestLoadConfigFromThemeDir`: the theme's `config/_default` and `config/production` over
/// its `config.toml`; the project's `config/config.toml` is not a configuration file.
#[test]
fn load_config_from_theme_dir() {
    let p = Project::new(&[
        (
            "config.toml",
            "theme = \"test-theme\"\n\n[params]\nm1 = \"mv1\"\n",
        ),
        (
            "themes/test-theme/config.toml",
            "[params]\nt1 = \"tv1\"\nt2 = \"tv2\"\n",
        ),
        ("config/config.toml", "[params]\nm2 = \"mv2\"\n"),
        (
            "themes/test-theme/config/_default/config.toml",
            "[params]\nt2 = \"tv2d\"\nt3 = \"tv3d\"\n",
        ),
        (
            "themes/test-theme/config/production/config.toml",
            "[params]\nt3 = \"tv3p\"\n",
        ),
    ]);
    assert_eq!(
        params(&p.ok()),
        toml("t3 = \"tv3p\"\nm1 = \"mv1\"\nt1 = \"tv1\"\nt2 = \"tv2d\"\n")
    );
}

/// `TestLoadConfigThemeLanguage`: the theme's `languages.en.params` merge into the project's
/// `en`; the project's language title wins.
#[test]
fn load_config_theme_language() {
    let p = Project::new(&[
        (
            "config.toml",
            r#"
baseURL = "https://example.com"
defaultContentLanguage = "en"
defaultContentLanguageInSubdir = true
theme = "mytheme"
[languages]
[languages.en]
title = "English Title"
weight = 1
[languages.sv]
weight = 2
"#,
        ),
        (
            "themes/mytheme/config.toml",
            r#"
[params]
p1 = "p1base"
[languages]
[languages.en]
title = "English Title Theme"
[languages.en.params]
p2 = "p2en"
[languages.en.params.sub]
sub1 = "sub1en"
[languages.sv]
title = "Svensk Title Theme"
"#,
        ),
    ]);
    let c = p.ok();
    let en = c.site("en").expect("en");
    assert_eq!(en.title, "English Title");
    assert_eq!(en.params.get("p1"), Some(&Value::string("p1base")));
    assert_eq!(en.params.get("p2"), Some(&Value::string("p2en")));
    assert_eq!(en.params.get("sub"), Some(&toml("sub1 = \"sub1en\"\n")));
    assert_eq!(c.site("sv").expect("sv").title, "", "`languages` is `none`");
}

/// `config/allconfig` `TestMergeDeep`: the root's `_merge = "deep"` over two themes, the second
/// configured in its `config/_default/config.toml`.
#[test]
fn merge_deep() {
    let p = Project::new(&[
        (
            "config.toml",
            "baseURL = \"https://example.com\"\ntheme = [\"theme1\", \"theme2\"]\n_merge = \"deep\"\n",
        ),
        (
            "themes/theme1/config.toml",
            "[sitemap]\nfilename = 'mysitemap.xml'\n[services]\n[services.googleAnalytics]\nid = 'foo bar'\n[taxonomies]\n  foo = 'bars'\n",
        ),
        (
            "themes/theme2/config/_default/config.toml",
            "[taxonomies]\n  bar = 'baz'\n",
        ),
    ]);
    let c = p.ok();
    let s = c.default_site();
    assert_eq!(c.environment, "production");
    assert_eq!(s.base_url.as_str(), "https://example.com/");
    assert_eq!(s.sitemap.filename, "mysitemap.xml");
    let mut taxonomies: Vec<(&str, &str)> = s
        .taxonomies
        .iter()
        .map(|t| (t.singular.as_str(), t.plural.as_str()))
        .collect();
    taxonomies.sort_unstable();
    assert_eq!(taxonomies, [("bar", "baz"), ("foo", "bars")]);
    assert_eq!(s.services.google_analytics.id, "foo bar");
}

/// `TestMergeDeepBuildStatsTheme`: with the root `deep`, the theme's `title` and `[build]`
/// (Go's test sets `[build.buildStats]`, which is gone here; `useResourceCacheWhen` stands in).
#[test]
fn merge_deep_build_stats_theme() {
    let p = Project::new(&[
        (
            "config.toml",
            "baseURL = \"https://example.com\"\n_merge = \"deep\"\ntheme = [\"theme1\"]\n",
        ),
        (
            "themes/theme1/config.toml",
            "title = \"Theme 1\"\n[build]\nuseResourceCacheWhen = \"always\"\n",
        ),
    ]);
    let c = p.ok();
    assert_eq!(c.default_site().title, "Theme 1");
    assert_eq!(c.themes.len(), 1);
    assert_eq!(c.build.use_resource_cache_when, "always");
    // `TestMergeDeepBuildStats`: the same with `[[module.imports]]` and the project's title.
    let p = Project::new(&[
        (
            "config.toml",
            "baseURL = \"https://example.com\"\ntitle = \"Theme 1\"\n_merge = \"deep\"\n[module]\n[[module.imports]]\npath = \"theme1\"\n",
        ),
        (
            "themes/theme1/config.toml",
            "[build]\nuseResourceCacheWhen = \"always\"\n",
        ),
    ]);
    let c = p.ok();
    assert_eq!(c.default_site().title, "Theme 1");
    assert_eq!(c.themes.len(), 1);
    assert_eq!(c.build.use_resource_cache_when, "always");
}

/// `TestConfigOutputFormatDefinedInTheme`: the project's `outputs` name a format only the
/// theme defines.
#[test]
fn config_output_format_defined_in_theme() {
    let p = Project::new(&[
        (
            "config.toml",
            "theme = \"mytheme\"\n[outputFormats]\n[outputFormats.myotherformat]\nbaseName = 'myotherindex'\nmediaType = 'text/html'\n[outputs]\n  home = ['myformat']\n",
        ),
        (
            "themes/mytheme/config.toml",
            "[outputFormats]\n[outputFormats.myformat]\nbaseName = 'myindex'\nmediaType = 'text/html'\n",
        ),
    ]);
    let c = p.ok();
    let home: Vec<&str> = c
        .default_site()
        .outputs
        .get(PageKind::Home)
        .iter()
        .map(|&id| c.output_formats.get(id).name.as_str())
        .collect();
    assert_eq!(home, ["myformat"]);
    let myformat = c.output_formats.by_name("myformat").expect("myformat");
    assert_eq!(c.output_formats.get(myformat).base_name, "myindex");
    assert!(c.output_formats.by_name("myotherformat").is_some());
}
