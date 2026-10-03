//! `build` through the binary: the testsite against Go's tree, flags and their camelCase
//! aliases, the environment variable that chooses the environment, error reports and exit codes.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use ssg_testkit::fixture::{go_output_as_built_here, repo_dir, repo_file};
use ssg_testkit::txtar::Archive;

use crate::{binary, site_from, stderr, stdout};

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("mkdir");
    for e in fs::read_dir(from).expect("read_dir") {
        let e = e.expect("entry");
        let name = e.file_name();
        if name.to_string_lossy().starts_with('.') || name == "public" || name == "resources" {
            continue;
        }
        let (src, dst) = (e.path(), to.join(&name));
        if e.file_type().expect("type").is_dir() {
            copy_tree(&src, &dst);
        } else {
            fs::copy(&src, &dst).expect("copy");
        }
    }
}

/// Go's txtar format: `-- name --` lines start files; every file ends with a newline.
fn write_txtar(archive: &str, to: &Path) {
    let mut files: Vec<(String, String)> = Vec::new();
    for line in archive.split('\n') {
        if line.len() > 6 && line.starts_with("-- ") && line.ends_with(" --") {
            files.push((line[3..line.len() - 3].trim().to_owned(), String::new()));
        } else if let Some((_, body)) = files.last_mut() {
            body.push_str(line);
            body.push('\n');
        }
    }
    for (name, mut body) in files {
        while body.ends_with("\n\n") {
            body.pop();
        }
        let path = to.join(&name);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::write(path, body).expect("write");
    }
}

/// The testsite as `sites.py make testsite` writes it, with the Tera layouts of
/// `sites/testsite/layouts` (as ssg-build's skeleton test builds it).
pub fn testsite(dir: &Path) {
    let root = repo_dir();
    copy_tree(&repo_file("testsite"), dir);
    let txtar = fs::read_to_string(root.join("tools/rust-port/i01/testsite.txtar")).expect("txtar");
    write_txtar(&txtar, dir);
    fs::remove_dir_all(dir.join("layouts")).expect("remove Go layouts");
    copy_tree(&root.join("sites/testsite/layouts"), &dir.join("layouts"));
}

/// Every file under `dir`, by slash-separated relative path.
pub fn tree(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).expect("read_dir") {
            let p = e.expect("entry").path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let rel = p
                    .strip_prefix(dir)
                    .expect("below")
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, fs::read(&p).expect("read"));
            }
        }
    }
    out
}

/// The command line of the old port's byte comparison (`tools/rust-port/i01/compare.sh`, removed
/// in T70; no command: `build`; run from the site directory; `--clock`, `-d`), then `build`
/// with kebab-case and camelCase spellings: the tree
/// is Go's, byte for byte (`crates/build/tests/it/testsite-go.txtar`, 55 files).
#[test]
fn testsite_matches_go() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let site = tmp.path().join("testsite");
    testsite(&site);
    let go = Archive::read(&repo_dir().join("crates/build/tests/it/testsite-go.txtar"))
        .expect("go tree");
    let want: BTreeMap<String, Vec<u8>> = go
        .files
        .iter()
        .map(|f| {
            (
                f.name.clone(),
                go_output_as_built_here(&f.data).into_bytes(),
            )
        })
        .collect();
    assert_eq!(want.len(), 55);

    let o = binary(
        &site,
        &["--clock", "2026-01-01T00:00:00Z", "-d", "../out1"],
        &[],
    );
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stdout(&o).contains("Total in"), "{}", stdout(&o));
    let got = tree(&tmp.path().join("out1"));
    let names = |m: &BTreeMap<String, Vec<u8>>| m.keys().cloned().collect::<Vec<_>>();
    assert_eq!(names(&got), names(&want));
    let differ: Vec<&String> = want
        .iter()
        .filter(|(k, v)| got[*k] != **v)
        .map(|(k, _)| k)
        .collect();
    assert!(differ.is_empty(), "bytes differ: {differ:?}");

    // The same with `build`, `--source`, `--destination` and `--cleanDestinationDir`; a stale
    // file in the publish directory stays (only static files are synchronised) and `-q` prints
    // nothing on success.
    let o = binary(
        tmp.path(),
        &[
            "build",
            "--source",
            "testsite",
            "--destination",
            "../out1",
            "--cleanDestinationDir",
            "--clock=2026-01-01T00:00:00Z",
            "-q",
        ],
        &[],
    );
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(stdout(&o), "");
    assert_eq!(names(&tree(&tmp.path().join("out1"))), names(&want));
}

const FLAGS_SITE: &str = r#"
-- config.toml --
baseURL = "https://example.org/"
title = "Config title"
disableKinds = ["taxonomy", "term", "rss", "sitemap", "robots", "404"]
-- content/_index.md --
---
title: Home
---
-- content/draft.md --
---
title: Draft
draft: true
---
-- content/future.md --
---
title: Future
date: 2030-01-01
---
-- content/expired.md --
---
title: Expired
expiryDate: 2020-01-01
---
-- layouts/home.html --
{{ site.title }}|{{ build.environment }}|{{ site.base_url }}|{{ now() | date(format="%Y") }}|{% for p in site.regular_pages | sort_by(attribute="title") %}{{ p.title }},{% endfor %}
-- layouts/single.html --
{{ page.title }}
"#;

fn home(dir: &Path) -> String {
    fs::read_to_string(dir.join("public/index.html")).expect("public/index.html")
}

/// Every configuration flag, in kebab-case and in Go's camelCase, and the environment variables:
/// the one that chooses the environment is read, those named like settings are not.
#[test]
fn flags_and_environment() {
    let s = site_from(FLAGS_SITE);
    let clock = ["--clock", "2026-06-01T00:00:00Z"];
    let run = |args: &[&str], env: &[(&str, &str)]| {
        let mut a = clock.to_vec();
        a.extend_from_slice(args);
        let o = binary(s.path(), &a, env);
        assert!(o.status.success(), "{args:?}: {}", stderr(&o));
        home(s.path())
    };
    assert_eq!(
        run(&[], &[]),
        "Config title|production|https://example.org/|2026|\n"
    );
    for (flags, pages) in [
        (&["-D", "-E", "-F"][..], "Draft,Expired,Future,"),
        (&["--build-drafts", "--build-expired"], "Draft,Expired,"),
        (&["--buildDrafts", "--buildFuture"], "Draft,Future,"),
        (&["--buildExpired"], "Expired,"),
    ] {
        let got = run(flags, &[]);
        assert!(got.ends_with(&format!("|{pages}\n")), "{flags:?}: {got}");
    }
    for flags in [
        &["-b", "https://b.org/"][..],
        &["--base-url", "https://b.org/"],
        &["--baseURL=https://b.org/"],
    ] {
        assert!(run(flags, &[]).contains("|https://b.org/|"), "{flags:?}");
    }
    assert!(run(&["-e", "staging"], &[]).contains("|staging|"));
    assert!(run(&["--environment", "staging"], &[]).contains("|staging|"));
    // No environment variable is read: not the environment (only `-e` chooses it), not
    // variables named like settings.
    let env = run(
        &[],
        &[
            (ssg_base::env_var!("ENVIRONMENT"), "env"),
            (ssg_base::env_var!("TITLE"), "Env title"),
            (ssg_base::env_var!("BASEURL"), "https://env.org/"),
        ],
    );
    assert!(env.contains("|production|"), "{env}");
    assert!(
        !env.contains("Env title") && !env.contains("https://env.org/"),
        "{env}"
    );

    // `--minify` / `-M` (`--renderToMemory`): nothing is written.
    fs::remove_dir_all(s.path().join("public")).expect("rm public");
    for flag in ["-M", "--render-to-memory", "--renderToMemory"] {
        let o = binary(s.path(), &[flag, "--minify"], &[]);
        assert!(o.status.success(), "{}", stderr(&o));
        assert!(!s.path().join("public").exists(), "{flag}");
    }
}

/// pflag's explicit `=false` overrides the configuration, as Go's flags did (`flagsToCfg`):
/// `-D=false` against `buildDrafts = true`, `--cleanDestinationDir=false` against
/// `cleanDestinationDir = true`.
#[test]
fn explicit_false_overrides_the_configuration() {
    let site = FLAGS_SITE.replacen(
        "title = \"Config title\"\n",
        "title = \"Config title\"\nbuildDrafts = true\nbuildFuture = true\ncleanDestinationDir = true\n",
        1,
    );
    let s = site_from(&site);
    let stale = s.path().join("public/stale.txt");
    let run = |args: &[&str]| {
        fs::create_dir_all(s.path().join("public")).expect("mkdir public");
        fs::write(&stale, "stale\n").expect("write stale");
        let mut a = vec!["--clock", "2026-06-01T00:00:00Z"];
        a.extend_from_slice(args);
        let o = binary(s.path(), &a, &[]);
        assert!(o.status.success(), "{args:?}: {}", stderr(&o));
        (home(s.path()), stale.exists())
    };
    for (flags, pages, kept) in [
        (&[][..], "Draft,Future,", false),
        (&["-D=false"], "Future,", false),
        (&["--buildDrafts=0", "--buildFuture=f"], "", false),
        (
            &["-D=true", "--cleanDestinationDir=false"],
            "Draft,Future,",
            true,
        ),
        (
            &["--clean-destination-dir=FALSE", "-E"],
            "Draft,Expired,Future,",
            true,
        ),
    ] {
        let (got, stale_kept) = run(flags);
        assert!(got.ends_with(&format!("|{pages}\n")), "{flags:?}: {got}");
        assert_eq!(stale_kept, kept, "{flags:?}");
    }
}

/// Errors: the message with its file, line and column, Tera's snippet, exit code 1.
#[test]
fn errors_are_reported_with_positions() {
    let s = site_from(
        "-- config.toml --\nbaseURL = \"https://e.org/\"\n-- layouts/home.html --\n<p>\n{{ page.params.nope.deeper }}\n</p>\n",
    );
    let o = binary(s.path(), &["-M"], &[]);
    assert_eq!(o.status.code(), Some(1));
    let err = stderr(&o);
    assert!(err.starts_with("ERROR build failed: "), "{err}");
    assert!(err.contains("home.html:2:16"), "{err}");
    assert!(err.contains("{{ page.params.nope.deeper }}"), "{err}");

    let s = site_from(
        "-- config.toml --\nbaseURL = \"https://e.org/\"\n-- layouts/home.html --\n{{ page.title }}{% if %}\n",
    );
    let o = binary(s.path(), &["-M"], &[]);
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("home.html:1:23"), "{}", stderr(&o));

    // Diagnostics (here a `throw`-free error: a missing shortcode) are listed, then counted.
    let s = site_from(
        "-- config.toml --\nbaseURL = \"https://e.org/\"\n-- content/_index.md --\n---\ntitle: H\n---\n{{< nope >}}\n-- layouts/home.html --\n{{ page.content }}\n",
    );
    let o = binary(s.path(), &["-M"], &[]);
    assert_eq!(o.status.code(), Some(1));
    let err = stderr(&o);
    assert!(err.contains("ERROR"), "{err}");
    assert!(err.contains("nope"), "{err}");

    // A project that does not load.
    let o = binary(s.path(), &["-s", "missing", "-M"], &[]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        stderr(&o).contains("no configuration file"),
        "{}",
        stderr(&o)
    );
}
