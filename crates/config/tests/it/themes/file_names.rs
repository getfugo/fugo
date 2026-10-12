//! The configuration file: one `config.*` in the project and in each theme; several, or a
//! configuration directory, are an error.

use super::*;

#[test]
fn one_configuration_file() {
    for name in ["config.toml", "config.yaml", "config.yml", "config.json"] {
        let text = if name.ends_with(".json") {
            "{\"title\": \"one\"}"
        } else if name.ends_with(".toml") {
            "title = \"one\"\n"
        } else {
            "title: one\n"
        };
        let p = Project::new(&[(name, text)]);
        let c = p.ok();
        assert_eq!(c.default_site().title, "one", "{name}");
        let files: Vec<String> = c.config_files.iter().map(|f| p.rel(f)).collect();
        assert_eq!(files, [name]);
    }
}

#[test]
fn several_configuration_files_are_an_error() {
    let p = Project::new(&[
        ("config.toml", "title = \"toml\"\n"),
        ("config.yaml", "title: yaml\n"),
        ("config.json", "{\"title\": \"json\"}"),
    ]);
    let e = p.load().expect_err("several files");
    let ConfigError::SeveralFiles { dir, files } = &e else {
        panic!("{e}")
    };
    assert_eq!(dir, &p.dir());
    assert_eq!(files, &["config.toml", "config.yaml", "config.json"]);
    assert!(
        e.to_string()
            .contains("several configuration files (config.toml, config.yaml, config.json)"),
        "{e}"
    );
}

/// The Go program's configuration file is not a configuration file: neither read nor counted
/// as a second one.
#[test]
fn go_config_file_is_not_read() {
    let go_file = format!("{GO_CONFIG_NAME}.toml");
    let p = Project::new(&[
        ("config.toml", "title = \"config\"\n"),
        (&go_file, "title = \"go\"\n"),
    ]);
    assert_eq!(p.ok().default_site().title, "config");
    let p = Project::new(&[(&go_file, "title = \"go\"\n")]);
    let e = p
        .load()
        .expect_err("the Go configuration file alone is no configuration");
    assert!(matches!(e, ConfigError::NotFound { .. }), "{e}");
}

#[test]
fn explicit_config_file() {
    let p = Project::new(&[
        ("config.toml", "title = \"config\"\n"),
        ("config.yaml", "title: yaml\n"),
        ("site.toml", "title = \"site\"\n"),
        ("extra.toml", "title = \"extra\"\n"),
    ]);
    // `--config` names the file: the project's `config.*` are not read.
    let c = p.load_with(Some("site.toml")).expect("load");
    assert_eq!(c.default_site().title, "site");
    let files: Vec<String> = c.config_files.iter().map(|f| p.rel(f)).collect();
    assert_eq!(files, ["site.toml"]);
    // `extra` finds extra.toml.
    let c = p.load_with(Some("extra")).expect("load");
    assert_eq!(c.default_site().title, "extra");
    let e = p.load_with(Some("missing.toml")).expect_err("no such file");
    assert!(matches!(e, ConfigError::Io { .. }), "{e}");
}

/// The Go build's configuration directory is an error that says where each file's settings go:
/// `_default/` files by their names, an environment's under `[environments.<name>]`.
#[test]
fn a_configuration_directory_is_an_error() {
    let files = [
        ("config/_default/config.toml", "title = \"dir\"\n"),
        ("config/_default/params.toml", "p = 1\n"),
        (
            "config/_default/menu.en.toml",
            "[[main]]\nname = \"Home\"\n",
        ),
        ("config/_default/Imaging.yaml", "quality: 80\n"),
        ("config/_default/notes.txt", "not configuration\n"),
        (
            "config/production/config.toml",
            "baseURL = \"https://x.example/\"\n",
        ),
        ("config/staging/nested/params.th.json", "{\"q\": 2}"),
    ];
    let want = [
        ("_default/Imaging.yaml", "imaging"),
        ("_default/config.toml", ""),
        ("_default/menu.en.toml", "languages.en.menus"),
        ("_default/params.toml", "params"),
        ("production/config.toml", "environments.production"),
        (
            "staging/nested/params.th.json",
            "environments.staging.languages.th.params",
        ),
    ];
    let with_file = [&[("config.toml", "title = \"file\"\n")][..], &files[..]].concat();
    // With a configuration file or without one (a Go project), in every environment.
    for files in [&with_file[..], &files[..]] {
        let p = Project::new(files);
        let e = p.load().expect_err("a configuration directory");
        let ConfigError::ConfigDirectory { dir, files } = &e else {
            panic!("{e}")
        };
        assert_eq!(dir, &p.dir().join("config"));
        let got: Vec<(String, &str)> = files
            .iter()
            .map(|(f, t)| (f.to_string_lossy().replace('\\', "/"), t.as_str()))
            .collect();
        let want: Vec<(String, &str)> = want.iter().map(|(f, t)| ((*f).to_owned(), *t)).collect();
        assert_eq!(got, want);
        let msg = e.to_string();
        assert!(
            msg.contains(
                "configuration directories are not read; move the settings of its files into \
                 the configuration file: _default/Imaging.yaml under [imaging], \
                 _default/config.toml at its root,"
            ),
            "{msg}"
        );
    }

    // The Go program's configuration file is an ordinary file there, under its base name.
    let p = Project::new(&[
        ("config.toml", "title = \"t\"\n"),
        (
            &format!("config/_default/{GO_CONFIG_NAME}.toml"),
            "title = \"go\"\n",
        ),
    ]);
    let e = p.load().expect_err("a configuration directory");
    let ConfigError::ConfigDirectory { files, .. } = &e else {
        panic!("{e}")
    };
    assert_eq!(files[0].1, GO_CONFIG_NAME);

    // Folders without configuration files, and files of the directory itself, are not read.
    let p = Project::new(&[
        ("config.toml", "title = \"file\"\n"),
        ("config/notes/readme.txt", "notes\n"),
        ("config/config.toml", "title = \"dir\"\n"),
    ]);
    assert_eq!(p.ok().default_site().title, "file");
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
fn theme_configuration_file() {
    let p = Project::new(&[
        ("config.toml", "theme = \"t\"\n"),
        ("themes/t/config.toml", "[params]\nfrom = \"config\"\n"),
        (
            &format!("themes/t/{GO_CONFIG_NAME}.toml"),
            "[params]\nfrom = \"go\"\ngoOnly = 1\n",
        ),
    ]);
    let c = p.ok();
    assert_eq!(params(&c), toml("from = \"config\"\n"));
    assert_eq!(
        c.themes[0].config_file.as_deref(),
        Some(p.dir().join("themes/t/config.toml").as_path())
    );
    // Lowest precedence first: the theme's file, then the project's.
    let files: Vec<String> = c.config_files.iter().map(|f| p.rel(f)).collect();
    assert_eq!(files, ["themes/t/config.toml", "config.toml"]);

    // Several files, or a configuration directory, are an error in a theme too.
    let p = Project::new(&[
        ("config.toml", "theme = \"t\"\n"),
        ("themes/t/config.toml", "[params]\nfrom = \"config\"\n"),
        ("themes/t/config.yaml", "params:\n  from: yaml\n"),
    ]);
    let e = p.load().expect_err("several files");
    assert!(
        matches!(&e, ConfigError::SeveralFiles { dir, .. } if dir.ends_with("themes/t")),
        "{e}"
    );
    let p = Project::new(&[
        ("config.toml", "theme = \"t\"\n"),
        ("themes/t/config/production/params.toml", "a = 1\n"),
    ]);
    let e = p.load().expect_err("a configuration directory");
    let ConfigError::ConfigDirectory { dir, files } = &e else {
        panic!("{e}")
    };
    assert!(dir.ends_with("themes/t/config"), "{}", dir.display());
    assert_eq!(files[0].1, "environments.production.params");
}
