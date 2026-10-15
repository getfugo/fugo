//! A page resource's own copyright notice and author (its `copyright` and `author` params) in
//! place of `[imaging]`'s: in the published photo and in every image processed from it; a
//! sibling photo keeps the site's notice; a two-language term bundle publishes its photo once,
//! with the first language's credit, though only that page gives it; a value that cannot be
//! written stops the build, naming the page's content file and the resource. `translations`:
//! the copies of a file each language publishes, and the resources adapters of both languages
//! add from one file.

use std::path::Path;

use ssg_build::{BuildError, BuildReport};

use super::*;

mod translations;

const CONFIG: &str = r#"baseURL = "https://example.org/"
title = "Images"
timeZone = "Asia/Bangkok"
defaultContentLanguage = "en"
[languages.en]
weight = 1
[languages.th]
weight = 2
[taxonomies]
flavor = "flavors"
[imaging]
copyright = "© 2019–{year} by Site"
author = "Site"
titleFromFileName = true
"#;

/// Each photo of the page, and a processed copy.
const SINGLE: &str = r#"{% for name in ["own.jpg", "site.jpg"] -%}
{% set pic = page.resources | get_resource(name=name) -%}
{% set small = pic | resize(width=100) -%}
<img src="{{ pic.rel_permalink }}"><img src="{{ small.rel_permalink }}">
{% endfor %}"#;

/// The term's photo and a processed copy; the Thai page processes one more.
const LIST: &str = r#"{% set pic = page.resources | get_resource(name="salted-duck-eggs.jpg") -%}
{% if pic -%}
{% set small = pic | resize(width=100) -%}
<img src="{{ pic.rel_permalink }}"><img src="{{ small.rel_permalink }}">
{%- if lang == "th" %}{% set thumb = pic | fill(width=40, height=40) %}<img src="{{ thumb.rel_permalink }}">{% endif %}
{%- endif %}"#;

const TERM_EN: &str = r#"---
title: Salted egg
image: salted-duck-eggs.jpg
resources:
  - src: salted-duck-eggs.jpg
    params:
      author: Takeaway
      copyright: "© Takeaway, CC BY-SA 3.0 (https://creativecommons.org/licenses/by-sa/3.0/)"
      license: CC BY-SA 3.0
      source: https://commons.wikimedia.org/wiki/File:Salted_duck_eggs.jpg
---
"#;

const ADA: [&str; 2] = ["Ada", "© 2032 Ada, CC BY 4.0"];
const SITE: [&str; 2] = ["Site", "© 2019–2032 by Site"];
const TAKEAWAY: [&str; 2] = [
    "Takeaway",
    "© Takeaway, CC BY-SA 3.0 (https://creativecommons.org/licenses/by-sa/3.0/)",
];

/// The photos of [`files`].
const PHOTOS: [&str; 3] = [
    "content/p/own.jpg",
    "content/p/site.jpg",
    "content/flavors/salted-egg/salted-duck-eggs.jpg",
];

/// The site's files: `config`, the English page's front matter `resources` entry for `own.jpg`
/// giving `params` (YAML, indented by six spaces), and the Thai term page `th_term` (in
/// `content/flavors/salted-egg/`).
fn files(config: &str, params: &str, th_term: &str) -> Vec<(String, String)> {
    let page = format!(
        "---\ntitle: P\nflavors: [salted-egg]\nresources:\n  - src: own.jpg\n    params:\n{params}---\n"
    );
    let th_term = format!("content/flavors/salted-egg/{th_term}");
    [
        ("config.toml", config.to_owned()),
        ("layouts/single.html", SINGLE.to_owned()),
        ("layouts/list.html", LIST.to_owned()),
        ("content/p/index.md", page),
        (
            "content/p/index.th.md",
            "---\ntitle: P th\nflavors: [salted-egg]\n---\n".to_owned(),
        ),
        (
            "content/flavors/salted-egg/_index.en.md",
            TERM_EN.to_owned(),
        ),
        (th_term.as_str(), "---\ntitle: ไข่เค็ม\n---\n".to_owned()),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect()
}

/// Builds the site in `tmp` with the photos, at 2032-01-01T00:30 in Bangkok.
fn build_site(tmp: &Path, params: &str) -> Result<BuildReport, BuildError> {
    build_files(tmp, &files(CONFIG, params, "_index.th.md"), &PHOTOS)
}

/// Builds the site of `files` in `tmp`, with a photo at each of `photos`, at 2032-01-01T00:30
/// in Bangkok.
fn build_files(
    tmp: &Path,
    files: &[(String, String)],
    photos: &[&str],
) -> Result<BuildReport, BuildError> {
    let site = tmp.join("site");
    write_files(&site, files);
    let photo = testdata("upstream/resources/testdata/sunset.jpg");
    for rel in photos {
        let path = site.join(rel);
        fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        fs::copy(&photo, path).expect("copy the image");
    }
    build(BuildRequest {
        source: site,
        sink: SinkKind::Memory,
        cli: CliOverrides {
            cache_dir: Some(tmp.join("cache")),
            ..CliOverrides::default()
        },
        clock: Some("2031-12-31T17:30:00Z".parse().expect("clock")),
        ..BuildRequest::default()
    })
}

#[test]
fn resources_carry_their_own_notice() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let report = build_site(
        tmp.path(),
        "      author: Ada\n      copyright: \"© {year} Ada, CC BY 4.0\"\n      license: CC BY 4.0\n",
    )
    .unwrap_or_else(|e| panic!("images build: {e}"));
    let mem = report.memory.as_ref().expect("memory");
    let paths = published(&report);
    let processed = |prefix: &str| -> Vec<&String> {
        paths
            .iter()
            .filter(|p| p.starts_with(prefix) && p.contains("_hu_"))
            .collect()
    };
    let (ada, site, takeaway) = (ADA, SITE, TAKEAWAY);
    let eggs = "salted-duck-eggs";
    let term_processed = processed(&format!("flavors/salted-egg/{eggs}_hu_"));
    assert_eq!(
        term_processed.len(),
        2,
        "the languages' shared copy and Thai's own: {paths:?}"
    );
    let mut checked = vec![
        ("p/own.jpg".to_owned(), "own", ada),
        ("p/site.jpg".to_owned(), "site", site),
        (format!("flavors/salted-egg/{eggs}.jpg"), eggs, takeaway),
    ];
    for (prefix, title, want) in [("p/own_hu_", "own", ada), ("p/site_hu_", "site", site)] {
        let copies = processed(prefix);
        assert_eq!(copies.len(), 1, "both languages' copy: {paths:?}");
        checked.push((copies[0].clone(), title, want));
    }
    checked.extend(
        term_processed
            .into_iter()
            .map(|p| (p.clone(), eggs, takeaway)),
    );
    for (path, title, [author, copyright]) in &checked {
        let bytes = mem.get(path).unwrap_or_else(|| panic!("{path}"));
        assert_eq!(
            notice(&bytes),
            [*author, *copyright, *title].map(str::to_owned),
            "{path}"
        );
    }
    // One copy of the term's photo, which the Thai page links too.
    assert!(
        !paths
            .iter()
            .any(|p| p.starts_with("th/") && p.ends_with(".jpg")),
        "{paths:?}"
    );
    let th = mem
        .text("th/flavors/salted-egg/index.html")
        .expect("Thai term page");
    assert!(
        th.contains(&format!("src=\"/flavors/salted-egg/{eggs}.jpg\"")),
        "{th}"
    );
}

/// The files `report` published.
fn published(report: &BuildReport) -> Vec<String> {
    let mem = report.memory.as_ref().expect("memory");
    mem.paths()
        .iter()
        .map(|p| p.relative().to_owned())
        .collect()
}

/// EXIF `Artist` and `Copyright`, and the XMP title, of `bytes` (empty: none).
fn notice(bytes: &[u8]) -> [String; 3] {
    let all = ssg_images::ExifSettings {
        include: None,
        exclude: None,
        ..ssg_images::ExifSettings::default()
    };
    let e = ssg_images::exif::read(bytes, &all).expect("EXIF");
    assert_eq!(e.lat_long, None, "no GPS position");
    let tag = |t: &str| {
        e.tags
            .get(t)
            .and_then(ssg_base::Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let xmp = String::from_utf8_lossy(bytes);
    let title = xmp
        .split_once("<dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">")
        .and_then(|(_, rest)| rest.split_once('<'))
        .map_or_else(String::new, |(t, _)| t.to_owned());
    [tag("Artist"), tag("Copyright"), title]
}

#[test]
fn params_that_cannot_be_written_stop_the_build() {
    for (params, reason) in [
        (
            "      copyright: \"© {yaer} Ada\"\n".to_owned(),
            "params.copyright: \"{yaer}\" is not a placeholder: the only one is {year}",
        ),
        (
            "      author: \"Ada\\nLovelace\"\n".to_owned(),
            "params.author: must be at most 1000 bytes of text without control characters",
        ),
        (
            format!("      copyright: \"{}\"\n", "x".repeat(1001)),
            "params.copyright: must be at most 1000 bytes of text without control characters",
        ),
        (
            "      author: 1815\n".to_owned(),
            "params.author: expected a string, found an integer",
        ),
    ] {
        let tmp = tempfile::tempdir().expect("tempdir");
        let e = build_site(tmp.path(), &params).expect_err(&params);
        let msg = e.to_string();
        assert!(
            msg.ends_with(&format!(
                "content/p/index.md: resource \"own.jpg\": {reason}"
            )),
            "{msg}"
        );
    }
}
