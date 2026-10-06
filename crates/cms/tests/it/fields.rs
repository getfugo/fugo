//! The editor's fields in the index: what the build works out from the content, with the
//! settings of `[cms.fields]` over it. The keys inside tables are in `fields/tables.rs`, what
//! the languages of a page and the pages of a section share in `fields/pages.rs`.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::support::{cms_toml, load, write_files};

mod pages;
mod tables;

/// The index of a site with `files` (and `config.toml` of `CONFIG` with `settings`).
pub(crate) fn read(settings: &str, files: &[(&str, &str)]) -> ssg_cms::index::Index {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir(dir.path().join(".git")).expect(".git");
    let config = format!("{CONFIG}{}", cms_toml(settings));
    write_files(dir.path(), &[("config.toml", config.as_str())]);
    write_files(dir.path(), files);
    let cfg = load(dir.path(), "production");
    let cms = ssg_cms::settings(&cfg).expect("settings").expect("[cms]");
    ssg_cms::index::read(&cfg, &cms, &BTreeMap::new()).expect("index")
}

/// The fields of an index, as the editor gets them.
pub(crate) fn json_of(index: ssg_cms::index::Index) -> BTreeMap<String, Value> {
    index
        .fields
        .into_iter()
        .map(|(k, f)| (k, serde_json::to_value(f).expect("json")))
        .collect()
}

const CONFIG: &str = r#"
baseURL = "https://snack.example/"
defaultContentLanguage = "en"
[languages.en]
weight = 1
[languages.th]
weight = 2
[taxonomies]
tag = "tags"
"#;

const SETTINGS: &str = r#"
[cms.fields.summary]
help = "Shown in lists"
[cms.fields.sizes]
options = ["S", "M", "L"]
[cms.fields.flavours]
label = "Flavours"
options = ["sweet", "salty"]
multiple = true
[cms.fields.secret]
widget = "hidden"
"#;

/// The fields of the index of a site with four snacks (one with a translation of its own).
fn fields() -> BTreeMap<String, Value> {
    json_of(read(
        SETTINGS,
        &[
            (
                "content/snacks/a.md",
                "---\ntitle: A\nbrand: Tom\nimage_preview: a.jpg\nimage_alt: A bag\nsummary: Short\nrating: 4\nreleased: 2024-01-02\nprice: 25\nsizes: [S]\ntags: [crisp]\ntranslationKey: a\n---\n",
            ),
            (
                "content/snacks/a-th.th.md",
                "---\ntitle: เอ\nbrand: Tom\ntranslationKey: a\n---\n",
            ),
            (
                "content/snacks/b.md",
                "---\ntitle: B\nbrand: Tom\nrating: 5\nreleased: \"\"\nprice: \"฿30\"\n---\n",
            ),
            (
                "content/snacks/c.md",
                "---\ntitle: C\nbrand: Lee\nsizes: [M]\n---\n",
            ),
            ("content/snacks/d.md", "---\ntitle: D\nbrand: Lee\n---\n"),
        ],
    ))
}

#[test]
fn every_key_has_a_label_and_the_widget_its_values_and_name_suggest() {
    let f = fields();
    assert_eq!(
        f["image_preview"],
        json!({"label": "Image preview", "widget": "image", "kind": "string"})
    );
    assert_eq!(
        f["image_alt"],
        json!({"label": "Image alt", "widget": "text", "kind": "string"}),
        "the text of an image is not an image"
    );
    assert_eq!(
        f["rating"],
        json!({"label": "Rating", "widget": "number", "kind": "number"})
    );
    assert_eq!(
        f["released"],
        json!({"label": "Released", "widget": "date", "kind": "date"}),
        "an empty value does not count"
    );
    assert_eq!(
        f["price"],
        json!({"label": "Price", "kind": "mixed"}),
        "values of different kinds each show as they are"
    );
    assert_eq!(
        f["tags"],
        json!({"label": "Tags", "kind": "list"}),
        "a taxonomy suggests its terms"
    );
    assert_eq!(
        f["translationkey"],
        json!({"label": "Translation key", "widget": "text", "kind": "string", "name": "translationKey"}),
        "translations share it, yet it names one page"
    );
}

#[test]
fn values_that_pages_share_are_suggested() {
    let f = fields();
    assert_eq!(
        f["brand"],
        json!({"label": "Brand", "widget": "text", "kind": "string", "suggestions": ["Lee", "Tom"]})
    );
    assert!(f["title"].get("suggestions").is_none(), "{}", f["title"]);
}

#[test]
fn the_settings_go_over_it_one_by_one() {
    let f = fields();
    assert_eq!(
        f["summary"],
        json!({"label": "Summary", "widget": "textarea", "kind": "string", "help": "Shown in lists"})
    );
    assert_eq!(
        f["sizes"],
        json!({"label": "Sizes", "widget": "select", "kind": "list", "options": ["S", "M", "L"], "multiple": true}),
        "options make a select, of several options for a list"
    );
}

#[test]
fn keys_only_the_settings_name_can_be_added() {
    let f = fields();
    assert_eq!(
        f["flavours"],
        json!({"label": "Flavours", "widget": "select", "kind": "list", "options": ["sweet", "salty"], "multiple": true, "unused": true})
    );
    assert_eq!(
        f["secret"],
        json!({"label": "Secret", "widget": "hidden", "kind": "string", "unused": true})
    );
}
