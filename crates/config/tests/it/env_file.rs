//! The project's `.env` file (`ssg_config::env_file`): the format, the ignored override names,
//! and that its values stay out of the configuration and its printouts.

use std::path::Path;

use ssg_base::diag::Diagnostic;
use ssg_config::{CliOverrides, Config, ConfigError, EnvFile, LoadOptions, load};

fn parse(text: &str) -> (EnvFile, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let file = EnvFile::parse(text, Path::new("/p/.env"), &mut diagnostics)
        .unwrap_or_else(|e| panic!("{e}"));
    (file, diagnostics)
}

fn parse_error(text: &str) -> String {
    match EnvFile::parse(text, Path::new("/p/.env"), &mut Vec::new()) {
        Ok(f) => panic!("parsed: {f:?}"),
        Err(e) => e.to_string(),
    }
}

/// A project with `config.toml` and the given `.env` (none: no file).
fn project(env_file: Option<&str>) -> (tempfile::TempDir, Result<Config, ConfigError>) {
    let tmp = tempfile::tempdir().expect("temp dir");
    std::fs::write(tmp.path().join("config.toml"), "title = \"T\"\n").expect("config");
    if let Some(text) = env_file {
        std::fs::write(tmp.path().join(".env"), text).expect(".env");
    }
    let cfg = load(&LoadOptions {
        source: tmp.path().to_owned(),
        config_files: Vec::new(),
        cli: CliOverrides::default(),
        env: vec![(
            "XDG_CACHE_HOME".into(),
            tmp.path().join("xdg").to_string_lossy().into_owned(),
        )],
    });
    (tmp, cfg)
}

#[test]
fn values() {
    let (f, d) = parse(concat!(
        "# a comment\n",
        "\n",
        "PLAIN=abc\n",
        "  SPACED = value with spaces  \n",
        "export EXPORTED=1\n",
        "EMPTY=\n",
        "COMMENTED=abc # a comment\n",
        "HASH=a#b\n",
        "ONLY_COMMENT= # nothing\n",
        "SINGLE='a \\n # b'\n",
        "DOUBLE=\"a\\nb\\t\\\"c\\\" \\\\ \\x\" # after\n",
        "CRLF=yes\r\n",
        "DUP=first\n",
        "DUP=second\n",
        "lower_case=ok\n",
    ));
    assert!(d.is_empty(), "{d:?}");
    let get = |n: &str| f.get(n).unwrap_or_else(|| panic!("{n} missing"));
    assert_eq!(get("PLAIN"), "abc");
    assert_eq!(get("SPACED"), "value with spaces");
    assert_eq!(get("EXPORTED"), "1");
    assert_eq!(get("EMPTY"), "");
    assert_eq!(get("COMMENTED"), "abc");
    assert_eq!(get("HASH"), "a#b");
    assert_eq!(get("ONLY_COMMENT"), "");
    assert_eq!(get("SINGLE"), "a \\n # b");
    assert_eq!(get("DOUBLE"), "a\nb\t\"c\" \\ \\x");
    assert_eq!(get("CRLF"), "yes");
    assert_eq!(get("DUP"), "second");
    assert_eq!(get("lower_case"), "ok");
    assert_eq!(
        f.names().collect::<Vec<_>>(),
        [
            "COMMENTED",
            "CRLF",
            "DOUBLE",
            "DUP",
            "EMPTY",
            "EXPORTED",
            "HASH",
            "ONLY_COMMENT",
            "PLAIN",
            "SINGLE",
            "SPACED",
            "lower_case",
        ]
    );
}

#[test]
fn errors_point_at_the_line() {
    assert_eq!(
        parse_error("A=1\nNO_EQUALS\n"),
        "/p/.env:2:1: expected NAME=value"
    );
    assert!(parse_error("1BAD=x").contains("\"1BAD\" is not a variable name"));
    assert!(parse_error("  BAD-NAME=x").starts_with("/p/.env:1:3: \"BAD-NAME\""));
    assert!(parse_error("OPEN=\"abc\n").contains("unterminated \" quote"));
    assert!(parse_error("OPEN='abc\n").contains("unterminated ' quote"));
    assert!(parse_error("TAIL='a' b\n").contains("unexpected text after the closing quote"));
    assert!(parse_error("exported=1\nexport =1\n").starts_with("/p/.env:2:1:"));
}

/// Override names are configuration, which `.env` is not: they are ignored, with a warning
/// `ignoreLogs` can silence.
#[test]
fn override_names_are_ignored_with_a_warning() {
    let prefixed = ssg_base::env_var!("PARAMS_API_KEY");
    let (f, d) = parse(&format!("{prefixed}=k\nKEY=v\n"));
    assert_eq!(f.get(prefixed), None);
    assert_eq!(f.get("KEY"), Some("v"));
    assert_eq!(d.len(), 1, "{d:?}");
    assert!(
        d[0].message.starts_with(&format!("{prefixed} is ignored")),
        "{}",
        d[0].message
    );
    assert_eq!(d[0].id.as_deref(), Some("env-file-prefix"));
    assert_eq!(d[0].position.as_ref().map(|p| p.line), Some(1));
}

/// The configuration carries the file's variables, but neither the configuration tree nor
/// `config`'s printout (`Serialize`) nor `Debug` shows a value.
#[test]
fn a_project_s_env_file_is_read_beside_the_configuration() {
    let (tmp, cfg) = project(Some("API_KEY=s3cr3t-value\n"));
    let cfg = cfg.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(cfg.env_file.get("API_KEY"), Some("s3cr3t-value"));
    assert_eq!(cfg.env_file.paths(), [tmp.path().join(".env")]);
    assert_eq!(cfg.raw.get("api_key"), None);
    let json = serde_json::to_string(&cfg).expect("serialize");
    assert!(!json.contains("s3cr3t-value"), "printed by config");
    assert!(
        !format!("{cfg:?}").contains("s3cr3t-value"),
        "printed by Debug"
    );
    assert!(format!("{:?}", cfg.env_file).contains("API_KEY"));
}

#[test]
fn a_project_without_env_file_has_no_variables() {
    let (_tmp, cfg) = project(None);
    let cfg = cfg.unwrap_or_else(|e| panic!("{e}"));
    assert!(cfg.env_file.is_empty());
    assert!(cfg.env_file.paths().is_empty());
}

#[test]
fn a_broken_env_file_fails_the_load() {
    let (tmp, cfg) = project(Some("OK=1\noops\n"));
    let err = cfg.expect_err("loaded");
    let pos = err.position().expect("a position");
    assert_eq!(&*pos.file, tmp.path().join(".env").as_path());
    assert_eq!(pos.line, 2);
}

/// `.env.<environment>` is read after `.env` and its lines win; another environment's file is
/// not read.
#[test]
fn the_environment_s_file_wins() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let write = |name: &str, text: &str| std::fs::write(tmp.path().join(name), text).expect(name);
    write("config.toml", "title = \"T\"\n");
    write(".env", "KEY=base\nONLY_BASE=1\n");
    write(".env.production", "KEY=production\n");
    write(".env.staging", "KEY=staging\nONLY_STAGING=1\n");
    let load = |environment: Option<&str>| {
        load(&LoadOptions {
            source: tmp.path().to_owned(),
            config_files: Vec::new(),
            cli: CliOverrides {
                environment: environment.map(str::to_owned),
                ..CliOverrides::default()
            },
            env: Vec::new(),
        })
        .unwrap_or_else(|e| panic!("{e}"))
    };
    let production = load(None);
    assert_eq!(production.env_file.get("KEY"), Some("production"));
    assert_eq!(production.env_file.get("ONLY_BASE"), Some("1"));
    assert_eq!(production.env_file.get("ONLY_STAGING"), None);
    assert_eq!(
        production.env_file.paths(),
        [tmp.path().join(".env"), tmp.path().join(".env.production")]
    );
    let staging = load(Some("staging"));
    assert_eq!(staging.env_file.get("KEY"), Some("staging"));
    assert_eq!(staging.env_file.get("ONLY_STAGING"), Some("1"));
    let dev = load(Some("development"));
    assert_eq!(dev.env_file.get("KEY"), Some("base"));
}
