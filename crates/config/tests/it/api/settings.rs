//! Settings from outside the files: environment variables (never settings), `CliOverrides`,
//! `[caches]` placeholders and `[privacy]`.

use super::*;

pub(super) const ENV_BASE: &str = r#"
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
