//! The configuration file names: `config.*` in the project, the config directory and themes.

use super::*;

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
        (
            "config/_default/config.toml",
            "title = \"dir\"\n[environments.production.params]\nq = 2\n\
             [environments.production.languages.en]\nweight = 1\n",
        ),
        ("config/_default/params.toml", "p = 1\n"),
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
