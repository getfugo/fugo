//! Languages, URLs, menus and the language redirect.

use super::*;

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
