//! One photo, published by the pages of two languages as a copy each (`duplicateResourceFiles`,
//! a host per language, a Thai page that is not Markdown), or added by the runs of a content
//! adapter for both languages (`enable_all_languages`): every copy, and every image processed
//! from any of them, carries the English credit, whether the Thai page gives none or another.

use super::*;

/// `config` with a host per language.
fn multihost(config: &str) -> String {
    config.replace(
        "[languages.th]\n",
        "baseURL = \"https://en.example.org/\"\n[languages.th]\nbaseURL = \"https://th.example.org/\"\n",
    )
}

/// The author and copyright of `path` in `report`.
fn credit(report: &BuildReport, path: &str) -> [String; 2] {
    let mem = report.memory.as_ref().expect("memory");
    let [author, copyright, _] = notice(&mem.get(path).unwrap_or_else(|| panic!("{path}")));
    [author, copyright]
}

#[test]
fn translations_own_copies_carry_the_credit() {
    let params = "      author: Ada\n      copyright: \"© {year} Ada, CC BY 4.0\"\n";
    for (setup, config, th_term) in [
        (
            "duplicateResourceFiles",
            format!("{CONFIG}[markup.goldmark]\nduplicateResourceFiles = true\n"),
            "_index.th.md",
        ),
        ("a host per language", multihost(CONFIG), "_index.th.md"),
        (
            "an HTML Thai term page",
            CONFIG.to_owned(),
            "_index.th.html",
        ),
    ] {
        let tmp = tempfile::tempdir().expect("tempdir");
        let report = build_files(tmp.path(), &files(&config, params, th_term), &PHOTOS)
            .unwrap_or_else(|e| panic!("{setup}: {e}"));
        let paths = published(&report);
        let thai: Vec<&String> = paths
            .iter()
            .filter(|p| p.starts_with("th/") && p.contains("salted-duck-eggs"))
            .collect();
        assert_eq!(
            thai.len(),
            3,
            "{setup}: the Thai copy of the term's photo, its resize and its fill: {paths:?}"
        );
        for path in paths.iter().filter(|p| p.ends_with(".jpg")) {
            let name = path.rsplit('/').next().unwrap_or_default();
            let want = if name.starts_with("own") {
                ADA
            } else if name.starts_with("site") {
                SITE
            } else {
                TAKEAWAY
            };
            assert_eq!(
                credit(&report, path),
                want.map(str::to_owned),
                "{setup}: {path}"
            );
        }
    }
}

/// The adapter: a page with `VALUE` (the asset, or an image processed from it) as its resource,
/// the English run giving Takeaway's credit, the Thai run `TH_PARAMS`.
const ADAPTER: &str = r#"{{- enable_all_languages() }}
{{- add_page(page={"kind": "page", "path": "eggs", "title": "Eggs"}) }}
{%- set photo = get_asset(path="photos/eggs.jpg") %}
{%- set value = VALUE %}
{%- if lang == "en" %}
{{- add_resource(resource={"path": "eggs/photo.jpg", "content": {"value": value}, "params": {"author": "Takeaway", "copyright": "© Takeaway, CC BY-SA 3.0 (https://creativecommons.org/licenses/by-sa/3.0/)"} }) }}
{%- else %}
{{- add_resource(resource={"path": "eggs/photo.jpg", "content": {"value": value}, "params": TH_PARAMS }) }}
{%- endif %}"#;

/// The page's photo and a processed copy; the Thai page processes one more.
const ADAPTER_SINGLE: &str = r#"{% set pic = page.resources | get_resource(name="photo.jpg") -%}
{% set small = pic | resize(width=100) -%}
<img src="{{ pic.rel_permalink }}"><img src="{{ small.rel_permalink }}">
{%- if lang == "th" %}{% set thumb = pic | fill(width=40, height=40) %}<img src="{{ thumb.rel_permalink }}">{% endif %}"#;

#[test]
fn adapter_resources_of_one_file_carry_one_credit() {
    for (hosts, value, th_params) in [
        // One place the runs' resources share.
        (1, "photo", "{}"),
        // A place each: the language's own copy of the asset.
        (2, "photo", "{}"),
        // The bytes of an image processed from the asset.
        (1, "photo | resize(width=300)", "{}"),
        (2, "photo | resize(width=300)", "{}"),
        // Both give one: the first language's.
        (
            1,
            "photo",
            r#"{"author": "Somchai", "copyright": "© Somchai"}"#,
        ),
    ] {
        let case = format!("{hosts} host(s), {value}, Thai params {th_params}");
        let config = if hosts == 1 {
            CONFIG.to_owned()
        } else {
            multihost(CONFIG)
        };
        let files: Vec<(String, String)> = [
            ("config.toml", config),
            (
                "content/foods/_content.html",
                ADAPTER
                    .replace("VALUE", value)
                    .replace("TH_PARAMS", th_params),
            ),
            ("layouts/single.html", ADAPTER_SINGLE.to_owned()),
            ("layouts/list.html", "list".to_owned()),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect();
        let tmp = tempfile::tempdir().expect("tempdir");
        let report = build_files(tmp.path(), &files, &["assets/photos/eggs.jpg"])
            .unwrap_or_else(|e| panic!("{case}: {e}"));
        let paths = published(&report);
        let photos: Vec<&String> = paths.iter().filter(|p| p.ends_with(".jpg")).collect();
        // The photo and its resize per place, and the Thai page's fill.
        let want = if hosts == 1 { 3 } else { 5 };
        assert_eq!(photos.len(), want, "{case}: {paths:?}");
        for path in photos {
            assert_eq!(
                credit(&report, path),
                TAKEAWAY.map(str::to_owned),
                "{case}: {path}"
            );
        }
    }
}
