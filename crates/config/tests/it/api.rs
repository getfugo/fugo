//! Behaviour tests of the pipeline: the legacy-key table on a synthetic real-site-style
//! configuration, environment variables (never settings), `CliOverrides`, `[caches]`
//! placeholders, `[privacy]`, and error positions.

use std::path::{Path, PathBuf};
use std::time::Duration;

use ssg_base::Value;
use ssg_config::global::MaxAge;
use ssg_config::{CliOverrides, Config, ConfigError, LoadOptions, load};

/// A project with the given files, loaded with `cli` and `env`.
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

    fn options(&self, cli: CliOverrides, env: &[(&str, &str)]) -> LoadOptions {
        let mut env: Vec<(String, String)> = env
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        env.push((
            "XDG_CACHE_HOME".into(),
            self.tmp.path().join("xdg").to_string_lossy().into_owned(),
        ));
        LoadOptions {
            source: self.dir(),
            config_files: Vec::new(),
            cli,
            env,
        }
    }

    fn load(&self, cli: CliOverrides, env: &[(&str, &str)]) -> Result<Config, ConfigError> {
        load(&self.options(cli, env))
    }

    fn ok(&self) -> Config {
        self.load(CliOverrides::default(), &[])
            .unwrap_or_else(|e| panic!("{e}"))
    }
}

/// A real-site-style configuration written with every legacy key the pipeline migrates.
const LEGACY: &str = r#"
baseURL = "https://snacks.example/"
title = "Snacks"
paginate = 12
paginatePath = "seite"
rssLimit = 10
ignoreErrors = ["error-remote-getjson"]
footnoteReturnLinkContents = "↩"
pygmentsStyle = "dracula"
pygmentsCodeFences = false
pygmentsCodefencesGuessSyntax = true
pygmentsUseClasses = true
disqusShortname = "snacks"
googleAnalytics = "G-LEGACY"
minify = true
logI18nWarnings = true

[indexes]
brand = "brands"
company = "companies"

[[menu.main]]
name = "Brands"
url = "/brands/"
weight = 1

[privacy.twitter]
enableDNT = true

[services.twitter]
disableInlineCSS = true

[build]
writeStats = true

[languages.en]
weight = 1
[languages.th]
weight = 2
paginate = 6
[languages.th.params]
description = "ไดอารี่"
"#;

#[test]
fn legacy_keys() {
    let p = Project::new(&[("config.toml", LEGACY)]);
    let c = p.ok();
    let en = c.site("en").expect("en");
    let th = c.site("th").expect("th");
    assert_eq!(en.pagination.pager_size, 12);
    assert_eq!(en.pagination.path, "seite");
    assert_eq!(
        th.pagination.pager_size, 6,
        "a legacy key inside a language table"
    );
    assert_eq!(en.services.rss.limit, 10);
    assert_eq!(c.ignore_logs, ["error-remote-getjson"]);
    assert_eq!(en.markup.goldmark.extensions.footnote.backlink_html, "↩");
    assert!(en.markup.goldmark.extensions.footnote.enable);
    assert_eq!(en.markup.highlight.style, "dracula");
    assert!(!en.markup.highlight.code_fences);
    assert!(en.markup.highlight.guess_syntax);
    assert!(
        !en.markup.highlight.no_classes,
        "pygmentsUseClasses = true means CSS classes"
    );
    assert_eq!(en.services.disqus.shortname, "snacks");
    assert_eq!(en.services.google_analytics.id, "G-LEGACY");
    assert!(c.minify.minify_output);
    assert_eq!(
        en.taxonomies
            .iter()
            .map(|t| (t.singular.as_str(), t.plural.as_str()))
            .collect::<Vec<_>>(),
        [("brand", "brands"), ("company", "companies")]
    );
    assert_eq!(en.menus.len(), 1);
    assert_eq!(en.menus[0].menu, "main");
    assert!(c.privacy.x.enable_dnt && c.privacy.twitter.enable_dnt);
    assert!(en.services.x.disable_inline_css);
    assert_eq!(c.raw.get("printi18nwarnings"), Some(&Value::Bool(true)));

    let mut notices: Vec<String> = c.diagnostics.iter().map(|d| d.message.clone()).collect();
    notices.sort();
    notices.dedup();
    ssg_testkit::snapshot::settings().bind(|| {
        insta::assert_yaml_snapshot!("legacy-keys-notices", notices);
    });
}

#[test]
fn legacy_keys_lose_to_current_keys() {
    let p = Project::new(&[("config.toml", "paginate = 3\n[pagination]\npagerSize = 7\n")]);
    assert_eq!(p.ok().default_site().pagination.pager_size, 7);
}

const ENV_BASE: &str = r#"
title = "file"
[params]
count = 3
ratio = 1.5
flag = false
list = ["a", "b"]
[params.nested]
deep = "d"
[markup.goldmark.renderer]
unsafe = false
"#;

/// Settings never come from the environment: variables named like settings change nothing.
#[test]
fn environment_variables_are_not_settings() {
    let p = Project::new(&[("config.toml", ENV_BASE)]);
    let want = serde_json::to_value(p.ok()).expect("serialize");
    let c = p
        .load(
            CliOverrides::default(),
            &[
                (ssg_base::env_var!("TITLE"), "env"),
                (ssg_base::env_var!("PARAMS_COUNT"), "42"),
                (ssg_base::env_var!("BASEURL"), "https://env.example/"),
                (ssg_base::env_var!("CACHEDIR"), "/env-cache"),
                (
                    ssg_base::env_var!("MARKUP_GOLDMARK_RENDERER_UNSAFE"),
                    "true",
                ),
                (ssg_base::env_var!("TAXONOMIES"), r#"{"tag": "tags"}"#),
            ],
        )
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(serde_json::to_value(c).expect("serialize"), want);
}

#[test]
fn cli_overrides_and_precedence() {
    let p = Project::new(&[
        (
            "config.toml",
            "baseURL = \"https://file.example/\"\ntitle = \"file\"\n",
        ),
        ("config/_default/params.toml", "from = \"dir\"\n"),
        ("config/staging/config.toml", "title = \"staging\"\n"),
    ]);
    let cli = CliOverrides {
        base_url: Some("https://cli.example/".into()),
        environment: Some("staging".into()),
        destination: Some("out".into()),
        minify: Some(true),
        build_drafts: Some(true),
        build_future: Some(true),
        build_expired: Some(false),
        ..CliOverrides::default()
    };
    let c = p.load(cli.clone(), &[]).unwrap_or_else(|e| panic!("{e}"));
    let s = c.default_site();
    assert_eq!(c.environment, "staging");
    assert_eq!(
        s.title, "staging",
        "the environment directory wins over the file"
    );
    assert_eq!(s.params.get("from"), Some(&Value::string("dir")));
    assert_eq!(s.base_url.as_str(), "https://cli.example/");
    assert_eq!(c.dirs.publish, Path::new("out"));
    assert!(c.minify.minify_output);
    assert!(c.content.drafts && c.content.future && !c.content.expired);

    // The environment does not override anything: settings come from files and flags.
    let c = p
        .load(
            cli,
            &[(ssg_base::env_var!("BASEURL"), "https://env.example/")],
        )
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(c.default_site().base_url.as_str(), "https://cli.example/");

    // Without --environment it is `production`; no environment variable chooses it.
    let c = p
        .load(
            CliOverrides::default(),
            &[(ssg_base::env_var!("ENVIRONMENT"), "staging")],
        )
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(c.environment, "production");
    assert_eq!(c.default_site().title, "file");
}

#[test]
fn caches_resolve_placeholders() {
    let p = Project::new(&[(
        "config.toml",
        r#"
cacheDir = "/var/cache/nh"
[caches.images]
dir = ":cacheDir/images"
maxAge = "1440h"
[caches.getresource]
dir = ":cacheDir/:project"
maxAge = 3600
[caches.misc]
dir = ":resourceDir/_misc"
maxAge = 0
"#,
    )]);
    let c = p.ok();
    let get = |n| c.caches.get(n).expect(n);
    assert_eq!(
        get("images").path,
        Path::new("/var/cache/nh/images/filecache/images")
    );
    assert_eq!(
        get("images").max_age,
        MaxAge::For(Duration::from_secs(1440 * 3600))
    );
    assert_eq!(
        get("getresource").path,
        Path::new("/var/cache/nh/site/filecache/getresource")
    );
    assert_eq!(
        get("getresource").max_age,
        MaxAge::For(Duration::from_secs(3600))
    );
    assert_eq!(get("misc").path, p.dir().join("resources/_misc/misc"));
    assert!(get("misc").in_resource_dir);
    assert_eq!(get("misc").max_age, MaxAge::For(Duration::ZERO));
    assert_eq!(get("assets").path, p.dir().join("resources/_gen/assets"));
    assert_eq!(get("getjson").max_age, MaxAge::Forever);
    assert_eq!(
        get("modules").path,
        Path::new("/var/cache/nh/modules/filecache/modules")
    );

    // Default cache directory: $XDG_CACHE_HOME/<name>_cache.
    let p = Project::new(&[("config.toml", "title = \"x\"\n")]);
    let c = p.ok();
    assert_eq!(
        c.cache_dir,
        p.tmp
            .path()
            .join(format!("xdg/{}_cache", ssg_base::APP_NAME))
    );
    assert_eq!(
        c.caches.get("misc").expect("misc").path,
        p.tmp.path().join(format!(
            "xdg/{}_cache/site/filecache/misc",
            ssg_base::APP_NAME
        ))
    );

    // A cache directory that does not resolve to an absolute path is an error.
    let p = Project::new(&[("config.toml", "[caches.misc]\ndir = \":project/misc\"\n")]);
    let e = p
        .load(CliOverrides::default(), &[])
        .expect_err("relative cache dir");
    assert!(e.to_string().contains("caches.misc.dir"), "{e}");
}

#[test]
fn privacy() {
    let p = Project::new(&[(
        "config.toml",
        r#"
[privacy.youtube]
privacyEnhanced = true
[privacy.x]
enableDNT = true
simple = true
[privacy.googleAnalytics]
respectDoNotTrack = true
[privacy.vimeo]
disable = true
[privacy.instagram]
simple = true
[privacy.disqus]
disable = true
"#,
    )]);
    let pr = p.ok().privacy;
    assert!(pr.youtube.privacy_enhanced && !pr.youtube.disable);
    assert!(pr.x.enable_dnt && pr.x.simple && !pr.x.disable);
    assert!(pr.google_analytics.respect_do_not_track);
    assert!(pr.vimeo.disable);
    assert!(pr.instagram.simple);
    assert!(pr.disqus.disable);
    assert_eq!(
        Project::new(&[("config.toml", "")]).ok().privacy,
        Default::default()
    );
}

fn position(e: &ConfigError) -> (String, u32, u32) {
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
fn languages_and_urls() {
    let p = Project::new(&[(
        "config.toml",
        r#"
defaultContentLanguage = "th"
defaultContentLanguageInSubdir = true
disableLanguages = "fr"
[languages.en]
weight = 2
languageName = "English"
[languages.th]
weight = 1
languageCode = "th-TH"
timeZone = "Asia/Bangkok"
[languages.ar]
weight = 3
languageDirection = "rtl"
[languages.fr]
weight = 4
"#,
    )]);
    let c = p.ok();
    let keys: Vec<_> = c.sites.iter().map(|s| s.language.key.as_str()).collect();
    assert_eq!(keys, ["th", "en", "ar"]);
    assert_eq!(c.disabled_languages, ["fr"]);
    let th = c.default_site();
    assert_eq!(th.language.url_prefix, "th");
    assert_eq!(th.language.code, "th-TH");
    assert_eq!(th.language.time_zone.iana_name(), Some("Asia/Bangkok"));
    assert_eq!(c.site("en").expect("en").language.code, "en");
    assert_eq!(
        c.site("ar").expect("ar").language.direction,
        ssg_config::Direction::Rtl
    );
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

#[test]
fn menus_section_pages_menu_and_language_redirect() {
    let p = Project::new(&[(
        "config.toml",
        r#"
sectionPagesMenu = "main"
disableDefaultLanguageRedirect = true
[[menus.main]]
name = "A"
pre = true
post = 3
[languages.en]
weight = 1
[languages.de]
weight = 2
sectionPagesMenu = "sections"
"#,
    )]);
    let c = p.ok();
    assert_eq!(
        c.default_language_redirect,
        ssg_config::RedirectPolicy::Disabled
    );
    let en = c.site("en").expect("en");
    assert_eq!(en.section_pages_menu.as_deref(), Some("main"));
    assert_eq!(
        c.site("de").expect("de").section_pages_menu.as_deref(),
        Some("sections")
    );
    // A boolean `pre`/`post` reads as `1`/`0`, as in Go.
    assert_eq!(
        (en.menus[0].pre.as_str(), en.menus[0].post.as_str()),
        ("1", "3")
    );

    let c = Project::new(&[("config.toml", "title = \"x\"\n")]).ok();
    assert_eq!(
        c.default_language_redirect,
        ssg_config::RedirectPolicy::Write
    );
    assert_eq!(c.default_site().section_pages_menu, None);

    // An empty `[related]` is the empty configuration (Go's site loader: the table is
    // set, and not empty to `related.DecodeConfig` because of its merge-strategy key).
    let c = Project::new(&[("config.toml", "[related]\n")]).ok();
    assert!(c.default_site().related.indices.is_empty());
    assert_eq!(c.default_site().related.threshold, 0);

    // Menus are lists of entries; `menus` is a table; the related cardinality thresholds
    // are percentages.
    for bad in [
        "[menus.main]\nname = \"single\"\n",
        "menus = \"main\"\n",
        "[related]\nthreshold = 80\n[[related.indices]]\nname = \"tags\"\ncardinalityThreshold = 101\n",
    ] {
        let p = Project::new(&[("config.toml", bad)]);
        assert!(p.load(CliOverrides::default(), &[]).is_err(), "{bad}");
    }
}
