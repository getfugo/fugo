//! The command line itself: `version`, `--help`, usage errors, flags before the command, the Go
//! build's logging and housekeeping flags, `config`.

use std::ffi::OsString;

use ::ssg_cli::Cli;
use ::ssg_cli::args::command_first;
use ::ssg_cli::version::BuildInfo;
use clap::Parser as _;

use ssg_base::APP_NAME;

use crate::{binary, site_from, stderr, stdout};

#[test]
fn version_help_and_usage_errors() {
    let dir = tempfile::tempdir().expect("tempdir");
    let o = binary(dir.path(), &["version"], &[]);
    assert_eq!(o.status.code(), Some(0));
    let line = format!("{}\n", BuildInfo::CURRENT);
    assert_eq!(stdout(&o), line);
    let prefix = format!("{APP_NAME} v{}", ssg_base::VERSION);
    assert!(line.starts_with(&prefix), "{line}");
    assert!(line.contains(" BuildDate="), "{line}");
    let o = binary(dir.path(), &["--version"], &[]);
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(stdout(&o), line);
    let o = binary(dir.path(), &["--help"], &[]);
    assert_eq!(o.status.code(), Some(0));
    for cmd in ["build", "server", "templates", "config", "version"] {
        assert!(stdout(&o).contains(cmd), "{cmd}: {}", stdout(&o));
    }
    let o = binary(dir.path(), &["templates", "check", "--help"], &[]);
    assert!(stdout(&o).contains("--coverage"), "{}", stdout(&o));
    for bad in [
        &["--nope"][..],
        &["build", "--clock", "yesterday"],
        &["templates"],
        &["templates", "check", "--coverage", "some"],
        &["config", "--format", "xml"],
    ] {
        let o = binary(dir.path(), bad, &[]);
        assert_eq!(o.status.code(), Some(2), "{bad:?}");
        assert!(stderr(&o).contains("error"), "{bad:?}: {}", stderr(&o));
    }
}

/// `version` prints Go's `BuildVersionString` (its `version.go` at 44529028).
#[test]
fn version_line_has_the_go_format() {
    let mut info = BuildInfo {
        version: "0.150.0",
        commit: None,
        os: "linux",
        arch: "amd64",
        date: None,
        vendor: None,
    };
    assert_eq!(
        info.to_string(),
        format!("{APP_NAME} v0.150.0 linux/amd64 BuildDate=unknown")
    );
    info.commit = Some("0123456789abcdef0123456789abcdef01234567");
    info.os = "darwin";
    info.arch = "arm64";
    info.date = Some("2026-10-01T12:34:56Z");
    info.vendor = Some(APP_NAME);
    assert_eq!(
        info.to_string(),
        format!(
            "{APP_NAME} v0.150.0-0123456789abcdef0123456789abcdef01234567 darwin/arm64 \
             BuildDate=2026-10-01T12:34:56Z VendorInfo={APP_NAME}"
        )
    );

    // Go's GOOS/GOARCH names of the release targets.
    let current = BuildInfo::CURRENT;
    let expected = if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some(("linux", "amd64"))
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Some(("linux", "arm64"))
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Some(("darwin", "amd64"))
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some(("darwin", "arm64"))
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Some(("windows", "amd64"))
    } else {
        None
    };
    if let Some(os_arch) = expected {
        assert_eq!((current.os, current.arch), os_arch);
    }
    assert_eq!(current.version, ssg_base::VERSION);
}

#[test]
fn config_prints_the_resolved_configuration() {
    let s = site_from(
        "-- config.toml --\nbaseURL = \"https://e.org/\"\ntitle = \"T\"\n-- config/production/params.toml --\ncolor = \"red\"\n",
    );
    let o = binary(
        s.path(),
        &["config"],
        &[(ssg_base::env_var!("PARAMS_SIZE"), "9")],
    );
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let v: serde_json::Value = serde_json::from_str(&stdout(&o)).expect("json");
    assert_eq!(v["environment"], "production");
    let site = &v["sites"][0];
    assert_eq!(site["title"], "T", "{site}");
    let text = stdout(&o);
    assert!(text.contains("\"color\": \"red\""), "{text}");
    assert!(
        !text.contains("\"size\""),
        "an environment variable is not a setting: {text}"
    );

    let o = binary(
        s.path(),
        &["config", "-e", "dev", "--base-url", "https://b.org/"],
        &[],
    );
    let text = stdout(&o);
    assert!(text.contains("https://b.org/"), "{text}");
    assert!(!text.contains("\"color\""), "{text}");

    let o = binary(s.path(), &["config", "--format", "toml"], &[]);
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    assert!(
        stdout(&o).contains("environment = \"production\""),
        "{}",
        stdout(&o)
    );
}

/// Flags before the command, as the Go build (cobra) reads them: `command_first` moves the
/// command to the front, a flag keeps its value.
#[test]
fn command_first_moves_the_command_before_the_flags() {
    let first = |args: &[&str]| -> Vec<String> {
        command_first(args.iter().map(OsString::from).collect())
            .into_iter()
            .map(|a| a.into_string().expect("UTF-8"))
            .collect()
    };
    for (given, want) in [
        (
            &[APP_NAME, "-s", "site", "server", "-D"][..],
            &[APP_NAME, "server", "-s", "site", "-D"][..],
        ),
        (
            &[APP_NAME, "--environment=production", "--minify", "build"],
            &[APP_NAME, "build", "--environment=production", "--minify"],
        ),
        (
            &[
                APP_NAME,
                "-e",
                "production",
                "templates",
                "-s",
                "x",
                "check",
            ],
            &[
                APP_NAME,
                "templates",
                "check",
                "-e",
                "production",
                "-s",
                "x",
            ],
        ),
        // Short clusters: a short that takes a value takes the rest or the next argument.
        (
            &[APP_NAME, "-DEs", "server", "config"],
            &[APP_NAME, "config", "-DEs", "server"],
        ),
        (
            &[APP_NAME, "-sserver", "config"],
            &[APP_NAME, "config", "-sserver"],
        ),
        // `=` ends a cluster: what follows is a value, not shorts (`t` and `e` take values).
        (
            &[APP_NAME, "-D=t", "-E=True", "server"],
            &[APP_NAME, "server", "-D=t", "-E=True"],
        ),
        // A flag of a command (`server`'s `--port`) keeps its value too.
        (
            &[APP_NAME, "--port", "1314", "serve"],
            &[APP_NAME, "serve", "--port", "1314"],
        ),
        // pflag's explicit values of the boolean flags that set no configuration key: `=true` is
        // the flag, `=false` none. The others (`-D`, `--minify`, `--watch`, …) take `=BOOL`
        // themselves.
        (
            &[
                APP_NAME,
                "--navigate-to-changed=true",
                "server",
                "-M=false",
                "--quiet=1",
                "-D=false",
                "--minify=0",
                "--watch=false",
            ],
            &[
                APP_NAME,
                "server",
                "--navigate-to-changed",
                "--quiet",
                "-D=false",
                "--minify=0",
                "--watch=false",
            ],
        ),
    ] {
        assert_eq!(first(given), want, "{given:?}");
    }
    // Unchanged: a value that names a command, `=`-only values, no command, `--`, an unknown
    // command, a command after a command without subcommands.
    for args in [
        &[APP_NAME, "-e", "server"][..],
        &[APP_NAME, "server", "--append-port", "false"],
        &[APP_NAME, "-s", "site", "-D"],
        &[APP_NAME, "--", "server"],
        &[APP_NAME, "-s", "site", "nope"],
        &[APP_NAME, "build", "--minify", "server"],
        &[APP_NAME, "--minify=maybe", "-e=x"],
    ] {
        assert_eq!(first(args), args, "{args:?}");
    }
}

/// The binary takes flags before the command, and every command takes the Go build's persistent
/// flags (`-s`, `-d`, `-e`, `--config`, `--config-dir`, `--themes-dir`, `--clock`, `-q`, `-M`).
#[test]
fn persistent_flags_anywhere() {
    let s = site_from("-- config.toml --\ntitle = \"T\"\n");
    let o = binary(s.path(), &["-s", ".", "-e", "staging", "config"], &[]);
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let v: serde_json::Value = serde_json::from_str(&stdout(&o)).expect("json");
    assert_eq!(v["environment"], "staging");
    for args in [
        &[
            "config",
            "-s",
            ".",
            "--quiet",
            "-d",
            "out",
            "-M",
            "--clock",
            "2026-01-01T00:00:00Z",
        ][..],
        &[
            "--config",
            "config.toml",
            "--config-dir",
            "config",
            "templates",
            "check",
            "--coverage",
            "none",
        ],
        &["-q", "version", "-s", ".", "--themes-dir", "themes"],
    ] {
        let o = binary(s.path(), args, &[]);
        assert_eq!(o.status.code(), Some(0), "{args:?}: {}", stderr(&o));
    }
    // A flag the command does not take is still an error, before the command or after it.
    for args in [&["--minify", "version"][..], &["version", "--minify"]] {
        let o = binary(s.path(), args, &[]);
        assert_eq!(o.status.code(), Some(2), "{args:?}");
        assert!(stderr(&o).contains("--minify"), "{args:?}: {}", stderr(&o));
    }
}

/// The Go build's logging and housekeeping flags, and the server flags that changed nothing, are
/// usage errors; so are the camelCase spellings no Go command line used.
#[test]
fn removed_flags_are_usage_errors() {
    let s = site_from("-- config.toml --\ntitle = \"T\"\n");
    for bad in [
        &["--gc"][..],
        &["--logLevel", "info"],
        &["--log-level", "warn"],
        &["--noBuildLock"],
        &["config", "--noBuildLock"],
        &["--printI18nWarnings"],
        &["--printPathWarnings"],
        &["--printUnusedTemplates"],
        &["--templateMetrics"],
        &["--templateMetricsHints"],
        &["--baseUrl", "https://example.org/"],
        &["server", "--disableFastRender"],
        &["server", "--disableBrowserError"],
        &["server", "--renderToDisk"],
        &["server", "--noHttpCache"],
        &["-v"],
    ] {
        let o = binary(s.path(), bad, &[]);
        assert_eq!(o.status.code(), Some(2), "{bad:?}: {}", stderr(&o));
    }
    // The Go spellings that stay.
    for args in [
        &[APP_NAME, "--baseURL", "https://example.org/"][..],
        &[APP_NAME, "server", "--noHTTPCache", "--render-to-disk"],
    ] {
        let parsed = Cli::try_parse_from(command_first(args.iter().map(OsString::from).collect()));
        assert!(parsed.is_ok(), "{args:?}");
    }
}

/// Boolean flags take pflag's explicit values.
#[test]
fn explicit_boolean_values() {
    let s = site_from("-- config.toml --\ntitle = \"T\"\n");
    let o = binary(s.path(), &["--minify=true", "--quiet=TRUE"], &[]);
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    assert!(!stdout(&o).contains("Total in"), "{}", stdout(&o));
    // The flags that take `=BOOL` themselves read it with Go's `strconv.ParseBool` spellings;
    // `=false` is kept (it overrides the configuration).
    let parse = |args: &[&str]| {
        let args = command_first(args.iter().map(OsString::from).collect());
        Cli::try_parse_from(&args).unwrap_or_else(|e| panic!("{args:?}: {e}"))
    };
    let cli = parse(&[
        APP_NAME,
        "-DE=f",
        "--buildFuture=FALSE",
        "--minify=0",
        "--ignoreCache=T",
        "--cleanDestinationDir=False",
        "--noTimes=1",
        "--noChmod=t",
    ]);
    let (b, p) = (&cli.build, &cli.build.project);
    assert_eq!(
        (p.include.build_drafts, p.include.build_expired),
        (Some(true), Some(false))
    );
    assert_eq!(
        (p.include.build_future, b.minify),
        (Some(false), Some(false))
    );
    assert_eq!(
        (p.ignore_cache, b.output.clean_destination_dir),
        (Some(true), Some(false))
    );
    assert_eq!(
        (b.output.no_times, b.output.no_chmod),
        (Some(true), Some(true))
    );
    let cli = parse(&[APP_NAME]);
    assert_eq!(
        (cli.build.project.include.build_drafts, cli.build.minify),
        (None, None)
    );
    let Some(::ssg_cli::args::Command::Server(server)) =
        parse(&[APP_NAME, "server", "--watch=0", "--appendPort=F", "-DEF"]).command
    else {
        panic!("server");
    };
    assert!(!server.watch.watch && !server.listen.append_port);
    assert_eq!(server.build.project.include.build_future, Some(true));
    // A build flag of a command that does not build.
    let o = binary(s.path(), &["config", "--minify"], &[]);
    assert_eq!(o.status.code(), Some(2), "{}", stderr(&o));
}

/// `--noTimes` and `--noChmod` (config `noTimes`, `noChmod`): the static copy leaves the copies'
/// modification times and permissions alone.
#[cfg(unix)]
#[test]
fn no_times_and_no_chmod_reach_the_static_copy() {
    use std::os::unix::fs::PermissionsExt;
    use std::time::{Duration, SystemTime};

    let s = site_from("-- config.toml --\ntitle = \"T\"\n-- static/a.txt --\nhi\n");
    let src = s.path().join("static/a.txt");
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(978_307_200);
    std::fs::File::options()
        .write(true)
        .open(&src)
        .and_then(|f| f.set_modified(old))
        .expect("set mtime");
    std::fs::set_permissions(&src, std::fs::Permissions::from_mode(0o604)).expect("chmod");
    let copied = |args: &[&str]| {
        let _ = std::fs::remove_dir_all(s.path().join("public"));
        let o = binary(s.path(), args, &[]);
        assert_eq!(o.status.code(), Some(0), "{args:?}: {}", stderr(&o));
        let m = std::fs::metadata(s.path().join("public/a.txt")).expect("copied");
        (
            m.modified().expect("mtime") == old,
            m.permissions().mode() & 0o777 == 0o604,
        )
    };
    assert_eq!(copied(&["build"]), (true, true));
    assert_eq!(copied(&["build", "--noTimes", "--noChmod"]), (false, false));
    assert_eq!(copied(&["build", "--no-times"]), (false, true));
}
