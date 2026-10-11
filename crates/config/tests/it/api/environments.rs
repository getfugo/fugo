//! `[environments.<name>]`: the build environment's table merged over the configuration, and the
//! folders of a configuration directory other than `_default` (an error).

use super::errors::position;
use super::*;

const SITE: &str = r#"
baseURL = "https://example.org/"
title = "Site"
[params]
color = "red"
size = "L"

[environments.production]
title = "Production"
[environments.production.params]
color = "green"
[environments.production.minify]
minifyOutput = true

[environments.development]
buildDrafts = true
buildFuture = true
[environments.development.params]
color = "blue"
"#;

fn env(name: &str) -> CliOverrides {
    CliOverrides {
        environment: Some(name.into()),
        ..CliOverrides::default()
    }
}

fn load_env(p: &Project, name: &str) -> Config {
    p.load(env(name), &[]).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn the_environment_s_table_is_merged_over_the_configuration() {
    let p = Project::new(&[("config.toml", SITE)]);
    let c = p.ok();
    let s = c.default_site();
    assert_eq!(s.title, "Production");
    assert_eq!(s.params.get("color"), Some(&Value::string("green")));
    assert_eq!(
        s.params.get("size"),
        Some(&Value::string("L")),
        "tables merge key by key"
    );
    assert!(c.minify.minify_output && !c.content.drafts);
    assert!(
        c.raw.get("environments").is_none(),
        "the tables are not settings"
    );

    let c = load_env(&p, "development");
    let s = c.default_site();
    assert_eq!(s.title, "Site");
    assert_eq!(s.params.get("color"), Some(&Value::string("blue")));
    assert!(c.content.drafts && c.content.future && !c.minify.minify_output);

    // An environment without a table has the configuration as written; names ignore case.
    assert_eq!(load_env(&p, "staging").default_site().title, "Site");
    assert!(load_env(&p, "Development").content.drafts);
}

#[test]
fn the_table_wins_over_the_directory_and_flags_over_the_table() {
    let p = Project::new(&[
        (
            "config.toml",
            "title = \"file\"\n[environments.production]\ntitle = \"table\"\n\
             baseURL = \"https://table.example/\"\n",
        ),
        (
            "config/_default/config.toml",
            "title = \"dir\"\ncopyright = \"dir\"\n",
        ),
    ]);
    let c = p.ok();
    assert_eq!(c.default_site().title, "table");
    assert_eq!(c.default_site().copyright, "dir");
    let cli = CliOverrides {
        base_url: Some("https://cli.example/".into()),
        ..CliOverrides::default()
    };
    let c = p.load(cli, &[]).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(c.default_site().base_url.as_str(), "https://cli.example/");
}

/// The table is merged as a later file would be: `_merge = "none"` replaces, `menu` is
/// `menus`, and legacy keys are migrated.
#[test]
fn the_table_is_merged_like_a_later_file() {
    let p = Project::new(&[(
        "config.toml",
        r#"
[params]
a = 1
[[menus.main]]
name = "Home"
url = "/"

[environments.production]
paginate = 5
[environments.production.params]
_merge = "none"
b = 2
[[environments.production.menu.main]]
name = "Shop"
url = "/shop/"
"#,
    )]);
    let c = p.ok();
    let s = c.default_site();
    assert_eq!(s.params.get("a"), None);
    assert_eq!(s.params.get("b"), Some(&Value::Int(2)));
    let menus: Vec<&str> = s.menus.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(menus, ["Shop"]);
    assert_eq!(s.pagination.pager_size, 5);
}

#[test]
fn errors_in_a_table_point_at_it() {
    let p = Project::new(&[(
        "config.toml",
        "title = \"x\"\n[environments.production.markup.goldmark.parser]\n\
         autoHeadingIDType = \"nope\"\n",
    )]);
    let e = p.load(CliOverrides::default(), &[]).expect_err("typed");
    let ConfigError::Invalid { key, .. } = &e else {
        panic!("{e}")
    };
    assert_eq!(
        key,
        "environments.production.markup.goldmark.parser.autoHeadingIDType"
    );
    assert_eq!(position(&e), ("config.toml".into(), 3, 1));

    for (text, key) in [
        (
            "[environments]\nproduction = \"fast\"\n",
            "environments.production",
        ),
        ("environments = 1\n", "environments"),
    ] {
        let p = Project::new(&[("config.toml", text)]);
        let e = p
            .load(CliOverrides::default(), &[])
            .expect_err("not a table");
        let ConfigError::Invalid { key: k, .. } = &e else {
            panic!("{e}")
        };
        assert_eq!(k, key);
        assert_eq!(position(&e).0, "config.toml");
    }
}

#[test]
fn environment_folders_are_an_error() {
    let p = Project::new(&[
        ("config.toml", "title = \"x\"\n"),
        ("config/_default/params.toml", "a = 1\n"),
        ("config/production/params.toml", "a = 2\n"),
    ]);
    // In every environment: the folder's settings would be lost without a word.
    for name in ["production", "development"] {
        let e = p.load(env(name), &[]).expect_err("a folder");
        assert!(
            matches!(&e, ConfigError::EnvironmentFolder { environment, .. } if environment == "production"),
            "{e}"
        );
        let msg = e.to_string();
        assert!(
            msg.contains("[environments.production]")
                && msg.contains("[environments.production.params]"),
            "{msg}"
        );
    }

    // A folder without configuration files is no error.
    Project::new(&[
        ("config.toml", "title = \"x\"\n"),
        ("config/notes/readme.txt", "notes\n"),
    ])
    .ok();

    // A theme's folder is one too.
    let p = Project::new(&[
        ("config.toml", "theme = \"t\"\n"),
        ("themes/t/config/staging/params.toml", "a = 1\n"),
    ]);
    let e = p.load(CliOverrides::default(), &[]).expect_err("a folder");
    assert!(
        matches!(&e, ConfigError::EnvironmentFolder { environment, .. } if environment == "staging"),
        "{e}"
    );
}

#[test]
fn a_theme_s_table_is_merged_over_its_configuration() {
    let p = Project::new(&[
        ("config.toml", "theme = \"t\"\n[params]\nmine = 1\n"),
        (
            "themes/t/config.toml",
            "[params]\nshade = \"light\"\n[environments.development.params]\nshade = \"dark\"\n",
        ),
    ]);
    assert_eq!(
        p.ok().default_site().params.get("shade"),
        Some(&Value::string("light"))
    );
    let c = load_env(&p, "development");
    assert_eq!(
        c.default_site().params.get("shade"),
        Some(&Value::string("dark"))
    );
    assert!(c.raw.get("environments").is_none());
}
