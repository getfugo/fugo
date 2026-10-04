//! Behaviour tests of the pipeline: the legacy-key table on a synthetic real-site-style
//! configuration, environment variables (never settings), `CliOverrides`, `[caches]`
//! placeholders, `[privacy]`, and error positions.

use std::path::{Path, PathBuf};
use std::time::Duration;

use ssg_base::Value;
use ssg_config::global::MaxAge;
use ssg_config::{CliOverrides, Config, ConfigError, LoadOptions, load};

mod errors;
mod languages;
mod settings;

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
