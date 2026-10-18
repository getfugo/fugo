//! Where the sitemaps are: a multilingual site's in each language's directory and the index at
//! the root, or all at the root with `[sitemap] root`, named after their language.

use std::fs;

use ssg_build::{BuildRequest, SinkKind, build};

use crate::support::{files_below, write_files};

const LANGUAGES: &str = "[languages.en]\nweight = 1\n[languages.fr]\nweight = 2\n";

/// The sitemaps of a site of `config` with an English and a French page, by path: its XML files
/// (no RSS).
fn sitemaps(config: &str) -> Vec<(String, String)> {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path().join("site");
    let config = format!(
        "baseURL = \"https://example.org/\"\ndisableKinds = [\"rss\", \"taxonomy\", \"term\"]\n{config}"
    );
    let files = [
        ("config.toml", config.as_str()),
        ("content/about.en.md", "---\ntitle: About\n---\n"),
        ("content/about.fr.md", "---\ntitle: À propos\n---\n"),
        ("layouts/single.html", "{{ page.title }}"),
        ("layouts/list.html", "{{ page.title }}"),
    ];
    write_files(&dir, &files.map(|(p, t)| (p.to_owned(), t.to_owned())));
    let public = tmp.path().join("public");
    let report = build(BuildRequest {
        source: dir,
        destination: Some(public.clone()),
        sink: SinkKind::Disk,
        clock: Some("2026-09-01T00:00:00Z".parse().expect("clock")),
        ..BuildRequest::default()
    })
    .expect("build");
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    files_below(&public)
        .into_iter()
        .filter(|f| f.ends_with(".xml"))
        .map(|f| {
            let text = fs::read_to_string(public.join(&f)).expect("read");
            (f, text)
        })
        .collect()
}

fn paths(maps: &[(String, String)]) -> Vec<&str> {
    maps.iter().map(|(p, _)| p.as_str()).collect()
}

fn text<'a>(maps: &'a [(String, String)], path: &str) -> &'a str {
    maps.iter()
        .find(|(p, _)| p == path)
        .map_or_else(|| panic!("no {path}"), |(_, t)| t.as_str())
}

#[test]
fn multilingual_sitemaps_are_in_their_language_directories() {
    let maps = sitemaps(LANGUAGES);
    assert_eq!(
        paths(&maps),
        ["en/sitemap.xml", "fr/sitemap.xml", "sitemap.xml"]
    );
    // The default language's pages are at the root, its sitemap below /en/.
    assert!(text(&maps, "en/sitemap.xml").contains("<loc>https://example.org/about/</loc>"));
    let index = text(&maps, "sitemap.xml");
    assert!(
        index.contains("<loc>https://example.org/en/sitemap.xml</loc>"),
        "{index}"
    );
    assert!(
        index.contains("<loc>https://example.org/fr/sitemap.xml</loc>"),
        "{index}"
    );
}

#[test]
fn root_puts_every_language_sitemap_at_the_root() {
    let maps = sitemaps(&format!("[sitemap]\nroot = true\n{LANGUAGES}"));
    assert_eq!(
        paths(&maps),
        ["sitemap-en.xml", "sitemap-fr.xml", "sitemap.xml"]
    );
    let en = text(&maps, "sitemap-en.xml");
    assert!(en.contains("<loc>https://example.org/about/</loc>"), "{en}");
    assert!(!en.contains("/fr/about/</loc>"), "{en}");
    let fr = text(&maps, "sitemap-fr.xml");
    assert!(
        fr.contains("<loc>https://example.org/fr/about/</loc>"),
        "{fr}"
    );
    let index = text(&maps, "sitemap.xml");
    assert!(
        index.contains("<loc>https://example.org/sitemap-en.xml</loc>"),
        "{index}"
    );
    assert!(
        index.contains("<loc>https://example.org/sitemap-fr.xml</loc>"),
        "{index}"
    );
}

#[test]
fn root_sitemaps_are_named_after_the_filename() {
    let maps = sitemaps(&format!(
        "[sitemap]\nroot = true\nfilename = \"map.xml\"\n{LANGUAGES}"
    ));
    assert_eq!(paths(&maps), ["map-en.xml", "map-fr.xml", "map.xml"]);
    assert!(text(&maps, "map.xml").contains("<loc>https://example.org/map-fr.xml</loc>"));
}

#[test]
fn root_leaves_a_site_without_an_index_alone() {
    let maps = sitemaps("[sitemap]\nroot = true\n");
    assert_eq!(paths(&maps), ["sitemap.xml"]);
    let map = text(&maps, "sitemap.xml");
    assert!(map.contains("<urlset"), "{map}");
    assert!(
        map.contains("<loc>https://example.org/about/</loc>"),
        "{map}"
    );
}
