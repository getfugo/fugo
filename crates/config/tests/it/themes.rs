//! The configuration file names (`config.*`; not the Go program's configuration file name) and
//! the themes: finding them (theme lists, themes of themes, `[[module.imports]]`, `_vendor`,
//! replacements) and merging their configuration below the project's with Go's `_merge`
//! rules. Tests named after a Go test port its cases with its expected values; the others
//! check the rules the oracle groups `merge/*` and `themes/*` (in `load.rs`) exercise per case.

use std::path::{Path, PathBuf};

use ssg_base::{Map, PageKind, Value};
use ssg_config::merge::{MergeStrategy, merge_themes};
use ssg_config::theme::path_key;
use ssg_config::{Config, ConfigError, LoadOptions, ThemeMounts, load};
use ssg_testkit::fixture::GO_CONFIG_NAME;

/// A project in a temporary directory (`site/`), loaded with an optional `--config` list.
struct Project {
    tmp: tempfile::TempDir,
}

impl Project {
    fn new(files: &[(&str, &str)]) -> Self {
        let tmp = tempfile::tempdir().expect("temp dir");
        for (name, text) in files {
            let p = tmp.path().join("site").join(name);
            std::fs::create_dir_all(p.parent().expect("parent")).expect("dir");
            std::fs::write(p, text).expect("write");
        }
        Self { tmp }
    }

    fn dir(&self) -> PathBuf {
        self.tmp.path().join("site")
    }

    fn load_with(&self, config_files: &[&str]) -> Result<Config, ConfigError> {
        load(&LoadOptions {
            source: self.dir(),
            config_files: config_files.iter().map(PathBuf::from).collect(),
            env: vec![(
                "XDG_CACHE_HOME".into(),
                self.tmp.path().join("xdg").to_string_lossy().into_owned(),
            )],
            ..LoadOptions::default()
        })
    }

    fn load(&self) -> Result<Config, ConfigError> {
        self.load_with(&[])
    }

    fn ok(&self) -> Config {
        self.load().unwrap_or_else(|e| panic!("{e}"))
    }

    /// `path` relative to the project directory.
    fn rel(&self, path: &Path) -> String {
        path.strip_prefix(self.dir())
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned()
    }
}

/// A TOML document as a configuration tree.
fn toml(s: &str) -> Value {
    Value::from_toml_str(s).expect("toml")
}

/// `params` of the default language as a TOML-comparable tree.
fn params(c: &Config) -> Value {
    Value::map(c.default_site().params.as_map().clone())
}

fn warnings(c: &Config, id: &str) -> Vec<String> {
    c.diagnostics
        .iter()
        .filter(|d| d.id.as_deref() == Some(id))
        .map(ToString::to_string)
        .collect()
}

// ───────────── configuration file names ─────────────

#[test]
fn config_extensions_in_lookup_order() {
    // toml, yaml, yml, json: the first that exists is read, the others are named in a warning.
    let p = Project::new(&[
        ("config.toml", "title = \"toml\"\n"),
        ("config.yaml", "title: yaml\n"),
    ]);
    let c = p.ok();
    assert_eq!(c.default_site().title, "toml");
    let files: Vec<String> = c.config_files.iter().map(|f| p.rel(f)).collect();
    assert_eq!(files, ["config.toml"]);
    let w = warnings(&c, "config-file-ignored");
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(
        w[0].contains("config.toml: using config.toml; ignoring config.yaml"),
        "{}",
        w[0]
    );
    let p = Project::new(&[
        ("config.json", "{\"title\": \"config.json\"}"),
        ("config.yaml", "title: config.yaml\n"),
    ]);
    let c = p.ok();
    assert_eq!(c.default_site().title, "config.yaml");
    assert!(warnings(&c, "config-file-ignored")[0].contains("ignoring config.json"));
    let p = Project::new(&[
        ("config.json", "{\"title\": \"json\"}"),
        ("config.yml", "title: yml\n"),
    ]);
    assert_eq!(p.ok().default_site().title, "yml");

    // One file: no warning.
    let p = Project::new(&[("config.yaml", "title: only\n")]);
    let c = p.ok();
    assert_eq!(c.default_site().title, "only");
    assert!(warnings(&c, "config-file-ignored").is_empty());
}

/// The Go program's configuration file is not a configuration file: neither read nor named in
/// warnings, and in a configuration directory an ordinary file that places its keys under its
/// base name.
#[test]
fn go_config_file_is_not_read() {
    let go_file = format!("{GO_CONFIG_NAME}.toml");
    let p = Project::new(&[
        ("config.toml", "title = \"config\"\n"),
        (&go_file, "title = \"go\"\n"),
    ]);
    let c = p.ok();
    assert_eq!(c.default_site().title, "config");
    assert!(warnings(&c, "config-file-ignored").is_empty());
    let p = Project::new(&[(&go_file, "title = \"go\"\n")]);
    let e = p
        .load()
        .expect_err("the Go configuration file alone is no configuration");
    assert!(matches!(e, ConfigError::NotFound { .. }), "{e}");
    let in_dir = format!("config/_default/{go_file}");
    let p = Project::new(&[
        ("config.toml", "title = \"t\"\n"),
        (&in_dir, "title = \"go\"\n"),
    ]);
    let c = p.ok();
    assert_eq!(c.default_site().title, "t");
    assert!(c.raw.get(GO_CONFIG_NAME).is_some());
}

#[test]
fn explicit_config_files_are_unchanged() {
    let p = Project::new(&[
        ("config.toml", "title = \"config\"\n"),
        ("site.toml", "title = \"site\"\n[params]\nfrom = \"site\"\n"),
        ("extra.toml", "[params]\nextra = true\nfrom = \"extra\"\n"),
    ]);
    let c = p.load_with(&["site.toml"]).expect("load");
    assert_eq!(c.default_site().title, "site");
    assert!(warnings(&c, "config-file-ignored").is_empty());
    // The first file wins.
    let c = p.load_with(&["extra", "site.toml"]).expect("load");
    assert_eq!(
        params(&c),
        toml("extra = true\nfrom = \"extra\"\n"),
        "`extra` finds extra.toml"
    );
}

#[test]
fn config_in_the_config_directory_is_a_root_file() {
    let p = Project::new(&[
        ("config/_default/config.toml", "title = \"dir\"\n"),
        ("config/_default/params.toml", "p = 1\n"),
        (
            "config/production/config.toml",
            "[params]\nq = 2\n[languages.en]\nweight = 1\n",
        ),
    ]);
    let c = p.ok();
    assert_eq!(c.default_site().title, "dir");
    assert_eq!(params(&c), toml("p = 1\nq = 2\n"));
    assert!(c.raw.get("config").is_none());
}

#[test]
fn no_configuration_names_config_toml() {
    let p = Project::new(&[("content/_index.md", "")]);
    let e = p.load().expect_err("no configuration");
    assert!(matches!(e, ConfigError::NotFound { .. }), "{e}");
    let msg = e.to_string();
    assert!(
        msg.contains("config.toml") && !msg.contains(&format!(" {GO_CONFIG_NAME}.")),
        "{msg}"
    );
}

#[test]
fn theme_configuration_file_names() {
    let p = Project::new(&[
        ("config.toml", "theme = \"t\"\n"),
        (
            "themes/t/config.toml",
            "[params]\nfrom = \"config\"\nconfigOnly = 1\n",
        ),
        (
            "themes/t/config.yaml",
            "params:\n  from: yaml\n  yamlOnly: 1\n",
        ),
        (
            &format!("themes/t/{GO_CONFIG_NAME}.toml"),
            "[params]\nfrom = \"go\"\ngoOnly = 1\n",
        ),
        (
            "themes/t/config/_default/config.yaml",
            "params:\n  dir: 1\n",
        ),
    ]);
    let c = p.ok();
    assert_eq!(
        params(&c),
        toml("from = \"config\"\nconfigonly = 1\ndir = 1\n")
    );
    assert_eq!(c.themes[0].config_files.len(), 2);
    let w = warnings(&c, "config-file-ignored");
    assert_eq!(w.len(), 1, "{w:?}");
    assert!(
        w[0].contains("themes/t/config.toml: using config.toml; ignoring config.yaml"),
        "{}",
        w[0]
    );
    // Lowest precedence first: the theme's files, then the project's.
    let files: Vec<String> = c.config_files.iter().map(|f| p.rel(f)).collect();
    assert_eq!(
        files,
        [
            "themes/t/config.toml",
            "themes/t/config/_default/config.yaml",
            "config.toml"
        ]
    );
}

// ───────────── Go ports ─────────────

/// Go's `config_test.go` `TestLoadConfigFromThemes`: the project's and the theme's
/// configuration (the Go test's `mainConfigTemplate` and `themeConfig`).
const MAIN_CONFIG: &str = r#"
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

const THEME_CONFIG: &str = r#"
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

fn with_theme(main: &str, theme: &str) -> Config {
    Project::new(&[
        ("config.toml", main),
        ("themes/test-theme/config.toml", theme),
    ])
    .ok()
}

/// The project-wide (root) `params` of `c`, without the language's own.
fn root_params(c: &Config) -> Value {
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

/// `TestLoadConfigModules`: the module graph of `[[module.imports]]`, old-style `theme` lists
/// in a theme's `config.toml`, and `theme.toml` metadata (not configuration).
#[test]
fn load_config_modules() {
    let mut files = vec![(
        "config.toml",
        "[module]\n[[module.imports]]\npath=\"n1\"\n[[module.imports]]\npath=\"n4\"\n",
    )];
    files.extend([
        (
            "themes/n1/config.toml",
            "title = \"Component n1\"\n\n[module]\ndescription = \"Component n1 description\"\n[[module.imports]]\npath=\"o1\"\n[[module.imports]]\npath=\"n3\"\n",
        ),
        ("themes/n2/config.toml", "title = \"Component n2\"\n"),
        ("themes/n3/config.toml", "title = \"Component n3\"\n"),
        ("themes/n4/config.toml", "title = \"Component n4\"\n"),
        ("themes/o1/config.toml", "theme = [\"n2\"]\n"),
        (
            "themes/o1/theme.toml",
            "name = \"Component o1\"\nlicense = \"MIT\"\nmin_version = 0.38\n",
        ),
    ]);
    let p = Project::new(&files);
    for n in ["n1", "n2", "n3", "n4", "o1"] {
        let data = p.dir().join(format!("themes/{n}/data"));
        std::fs::create_dir_all(&data).expect("dir");
        std::fs::write(data.join("module.toml"), format!("name={n:?}")).expect("write");
    }
    let c = p.ok();
    let graph: Vec<String> = c
        .themes
        .iter()
        .map(|t| format!("{} {}", t.owner.as_deref().unwrap_or("project"), t.path))
        .collect();
    assert_eq!(
        graph,
        ["project n1", "n1 o1", "o1 n2", "n1 n3", "project n4"]
    );
    // Root values of themes are not merged by default.
    assert_eq!(c.default_site().title, "");
}

/// `modules/config_test.go` `TestDecodeConfigBothOldAndNewProvided` and
/// `TestDecodeConfigTheme`: `[[module.imports]]` first, then `theme`.
#[test]
fn imports_then_theme() {
    let p = Project::new(&[
        (
            "config.toml",
            "theme = [\"b\", \"c\"]\n\n[module]\n[[module.imports]]\npath=\"a\"\n",
        ),
        ("themes/a/layouts/a.html", ""),
        ("themes/b/layouts/b.html", ""),
        ("themes/c/layouts/c.html", ""),
    ]);
    let paths: Vec<String> = p.ok().themes.into_iter().map(|t| t.path).collect();
    assert_eq!(paths, ["a", "b", "c"]);
    let p = Project::new(&[
        ("config.toml", "theme = [\"a\", \"b\"]\n"),
        ("themes/a/layouts/a.html", ""),
        ("themes/b/layouts/b.html", ""),
    ]);
    let paths: Vec<String> = p.ok().themes.into_iter().map(|t| t.path).collect();
    assert_eq!(paths, ["a", "b"]);
}

/// `TestDecodeConfig` "Replacements": a comma-separated string or a list; the replaced path is
/// read from the themes directory.
#[test]
fn module_replacements() {
    for replacements in [
        "replacements=\"a->b,github.com/bep/mycomponent->c\"",
        "replacements=[\"a->b\",\"github.com/bep/mycomponent->c\"]",
    ] {
        let p = Project::new(&[
            (
                "config.toml",
                &format!(
                    "[module]\n{replacements}\n[[module.imports]]\npath=\"github.com/bep/mycomponent\"\n"
                ),
            ),
            ("themes/c/config.toml", "[params]\nfromC = true\n"),
        ]);
        let c = p.ok();
        assert_eq!(c.themes.len(), 1, "{replacements}");
        assert_eq!(c.themes[0].path, "c");
        assert_eq!(c.themes[0].dir, p.dir().join("themes/c"));
        assert_eq!(params(&c), toml("fromc = true\n"));
    }
    // Not "old -> new".
    let p = Project::new(&[(
        "config.toml",
        "[module]\nreplacements = [\"github.com/a/b\"]\n",
    )]);
    let e = p.load().expect_err("invalid replacement");
    assert!(e.to_string().contains("module.replacements"), "{e}");
}

/// `modules/collect_test.go` `TestPathKey`.
#[test]
fn import_path_identity() {
    for (path, key) in [
        ("github.com/foo", "github.com/foo"),
        ("github.com/foo/v2", "github.com/foo"),
        ("github.com/foo/v12", "github.com/foo"),
        ("github.com/foo/v3d", "github.com/foo/v3d"),
        ("MyTheme", "mytheme"),
    ] {
        assert_eq!(path_key(path), key, "{path}");
    }
}

// ───────────── the merge rules ─────────────

#[test]
fn theme_list_precedence() {
    let p = Project::new(&[
        (
            "config.toml",
            "theme = [\"a\", \"b\"]\n[params]\nmine = 1\n",
        ),
        (
            "themes/a/config.toml",
            "[params]\nx = \"a\"\n[params.deep]\nfromA = 1\n",
        ),
        (
            "themes/b/config.toml",
            "[params]\nx = \"b\"\ny = \"b\"\n[params.deep]\nfromA = 2\nfromB = 2\n",
        ),
    ]);
    assert_eq!(
        params(&p.ok()),
        toml("mine = 1\nx = \"a\"\ny = \"b\"\n[deep]\nfroma = 1\nfromb = 2\n")
    );
}

/// A theme's `_merge` applies to the tables it brings in when a later theme is merged into
/// them; on a table the project has, the project's strategy decides.
#[test]
fn merge_strategy_written_in_a_theme() {
    let p = Project::new(&[
        (
            "config.toml",
            "theme = [\"a\", \"b\"]\n[params.own]\nkept = 1\n",
        ),
        (
            "themes/a/config.toml",
            "[params.locked]\n_merge = \"none\"\nfromA = 1\n[params.own]\n_merge = \"none\"\nfromA = 1\n",
        ),
        (
            "themes/b/config.toml",
            "[params.locked]\nfromB = 2\n[params.own]\nfromB = 2\n",
        ),
    ]);
    assert_eq!(
        params(&p.ok()),
        toml("[locked]\nfroma = 1\n[own]\nkept = 1\nfroma = 1\nfromb = 2\n")
    );
}

#[test]
fn per_key_defaults_and_overrides() {
    let theme = r#"
title = "theme"
[params]
tp = 1
[taxonomies]
theme = "themes"
[permalinks.page]
posts = "/:year/:slug/"
[outputs]
home = ["html", "json"]
[menus]
[[menus.main]]
name = "Theme"
url = "/t/"
[[menus.footer]]
name = "Footer"
url = "/f/"
[languages.en.menus]
[[languages.en.menus.side]]
name = "Side"
url = "/s/"
[[languages.en.menus.extra]]
name = "Extra"
url = "/e/"
"#;
    // Defaults: `taxonomies`, `permalinks`, `outputs` are `none`; `menus` `shallow`; root
    // values not merged.
    let p = Project::new(&[
        (
            "config.toml",
            "theme = \"t\"\n[[menus.main]]\nname = \"Mine\"\nurl = \"/\"\n[languages.en]\nweight = 1\n[[languages.en.menus.side]]\nname = \"MySide\"\nurl = \"/ms/\"\n",
        ),
        ("themes/t/config.toml", theme),
    ]);
    let c = p.ok();
    let s = c.default_site();
    assert_eq!(s.title, "");
    let taxonomies: Vec<&str> = s.taxonomies.iter().map(|t| t.plural.as_str()).collect();
    assert_eq!(taxonomies, ["categories", "tags"]);
    assert_eq!(s.permalinks.get(PageKind::Page, "posts"), None);
    let home: Vec<&str> = s
        .outputs
        .get(PageKind::Home)
        .iter()
        .map(|&id| c.output_formats.get(id).name.as_str())
        .collect();
    assert_eq!(home, ["html", "rss"]);
    let mut menus: Vec<(&str, &str)> = s
        .menus
        .iter()
        .map(|e| (e.menu.as_str(), e.name.as_str()))
        .collect();
    menus.sort_unstable();
    // The language's own menus replace the root's; the theme adds `extra` to them.
    assert_eq!(menus, [("extra", "Extra"), ("side", "MySide")]);

    // `_merge` in the project opens (or closes) a table.
    let p = Project::new(&[
        (
            "config.toml",
            "theme = \"t\"\n[taxonomies]\n_merge = \"deep\"\ntag = \"tags\"\n[permalinks]\n_merge = \"deep\"\n[outputs]\n_merge = \"shallow\"\n[menus]\n_merge = \"none\"\n[[menus.main]]\nname = \"Mine\"\nurl = \"/\"\n",
        ),
        ("themes/t/config.toml", theme),
    ]);
    let c = p.ok();
    let s = c.default_site();
    let mut taxonomies: Vec<&str> = s.taxonomies.iter().map(|t| t.plural.as_str()).collect();
    taxonomies.sort_unstable();
    assert_eq!(taxonomies, ["tags", "themes"]);
    assert_eq!(
        s.permalinks.get(PageKind::Page, "posts"),
        Some("/:year/:slug/")
    );
    let home: Vec<&str> = s
        .outputs
        .get(PageKind::Home)
        .iter()
        .map(|&id| c.output_formats.get(id).name.as_str())
        .collect();
    assert_eq!(home, ["html", "json"]);
    let menus: Vec<&str> = s.menus.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(menus, ["Mine"]);

    // `_merge = "none"` at the root: nothing from the theme.
    let p = Project::new(&[
        ("config.toml", "theme = \"t\"\n_merge = \"none\"\n"),
        ("themes/t/config.toml", theme),
    ]);
    let c = p.ok();
    assert!(c.default_site().params.is_empty());
    assert!(c.default_site().menus.is_empty());
}

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

// ───────────── finding themes ─────────────

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
