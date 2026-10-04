//! Error positions, and the errors of `toc` end levels and permalinks.

use super::*;

pub(super) fn position(e: &ConfigError) -> (String, u32, u32) {
    let p = e.position().unwrap_or_else(|| panic!("no position: {e}"));
    (
        p.file
            .file_name()
            .expect("name")
            .to_string_lossy()
            .into_owned(),
        p.line,
        p.col,
    )
}

#[test]
fn error_positions() {
    // TOML syntax.
    let p = Project::new(&[("config.toml", "title = \"x\"\n[params\nfoo = 1\n")]);
    let e = p.load(CliOverrides::default(), &[]).expect_err("syntax");
    assert!(matches!(e, ConfigError::Syntax { .. }));
    assert_eq!(position(&e), ("config.toml".into(), 2, 8));

    // YAML syntax.
    let p = Project::new(&[("config.yaml", "title: x\nparams: [a\n")]);
    let e = p.load(CliOverrides::default(), &[]).expect_err("syntax");
    assert!(matches!(e, ConfigError::Syntax { .. }));
    assert_eq!(position(&e).0, "config.yaml");
    assert!(position(&e).1 >= 2, "{e}");

    // JSON syntax.
    let p = Project::new(&[("config.json", "{\n  \"title\": }\n")]);
    let e = p.load(CliOverrides::default(), &[]).expect_err("syntax");
    assert_eq!(position(&e), ("config.json".into(), 2, 12));

    // A value of the wrong type points at its key, in the file that set it.
    let p = Project::new(&[
        ("config.toml", "title = \"x\"\n"),
        (
            "config/_default/markup.toml",
            "[goldmark]\n[goldmark.parser]\nautoHeadingIDType = \"nope\"\n",
        ),
    ]);
    let e = p.load(CliOverrides::default(), &[]).expect_err("typed");
    let ConfigError::Invalid { key, .. } = &e else {
        panic!("{e}")
    };
    assert_eq!(key, "markup.goldmark.parser.autoHeadingIDType");
    assert_eq!(position(&e), ("markup.toml".into(), 3, 1));

    // In a language table.
    let p = Project::new(&[(
        "config.toml",
        "[languages.en]\nweight = 1\n[languages.th]\nweight = 2\n[languages.th.pagination]\npagerSize = \"many\"\n",
    )]);
    let e = p.load(CliOverrides::default(), &[]).expect_err("typed");
    assert_eq!(position(&e), ("config.toml".into(), 6, 1));
    assert!(
        e.to_string().contains("languages.th.pagination.pagerSize"),
        "{e}"
    );

    // An array element.
    let p = Project::new(&[(
        "config.toml",
        "[[related.indices]]\nname = \"a\"\n[[related.indices]]\nname = \"b\"\ntype = \"nope\"\n",
    )]);
    let e = p.load(CliOverrides::default(), &[]).expect_err("typed");
    assert_eq!(position(&e), ("config.toml".into(), 5, 1));

    // Language errors.
    let p = Project::new(&[(
        "config.toml",
        "defaultContentLanguage = \"fr\"\n[languages.en]\n",
    )]);
    let e = p.load(CliOverrides::default(), &[]).expect_err("language");
    assert_eq!(position(&e), ("config.toml".into(), 1, 1));
    let p = Project::new(&[(
        "config.toml",
        "[languages.en]\ntimeZone = \"Mars/Olympus\"\n",
    )]);
    let e = p.load(CliOverrides::default(), &[]).expect_err("time zone");
    assert_eq!(position(&e), ("config.toml".into(), 2, 1));

    let shown = e
        .to_string()
        .replace(&*p.tmp.path().to_string_lossy(), "$ROOT");
    ssg_testkit::snapshot::settings().bind(|| {
        insta::assert_snapshot!("error-display-time-zone", shown);
    });
}

#[test]
fn toc_end_level_and_permalink_errors() {
    // `endLevel = -1` is no end level; other levels are levels.
    let p = Project::new(&[(
        "config.toml",
        "[markup.tableOfContents]\nstartLevel = 1\nendLevel = -1\n",
    )]);
    let c = p.ok();
    let toc = &c.default_site().markup.table_of_contents;
    assert_eq!((toc.start_level, toc.end_level), (1, None));
    let p = Project::new(&[("config.toml", "[markup.tableOfContents]\nendLevel = 4\n")]);
    assert_eq!(
        p.ok().default_site().markup.table_of_contents.end_level,
        Some(4)
    );
    assert_eq!(ssg_config::markup::TocConfig::default().end_level, Some(3));
    let p = Project::new(&[("config.toml", "[markup.tableOfContents]\nendLevel = -2\n")]);
    let e = p.load(CliOverrides::default(), &[]).expect_err("end level");
    assert_eq!(position(&e), ("config.toml".into(), 2, 1));

    // Permalinks for a kind that has none, or a pattern that is not a string.
    let p = Project::new(&[("config.toml", "[permalinks.home]\na = \"/a/\"\n")]);
    let e = p.load(CliOverrides::default(), &[]).expect_err("home");
    assert_eq!(position(&e).0, "config.toml");
    assert!(e.to_string().contains("permalinks.home"), "{e}");
    let p = Project::new(&[("config.toml", "[permalinks]\nposts = 42\n")]);
    let e = p.load(CliOverrides::default(), &[]).expect_err("pattern");
    assert_eq!(position(&e), ("config.toml".into(), 2, 1));
}
