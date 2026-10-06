//! What the pages share: a value that is the same in every language of a page (edited once for
//! all of them), and the value every page of a section gives a key (which new pages start with).
//! Also the settings of the checks before saving.

use serde_json::json;

use super::{json_of, read};

const SETTINGS: &str = r#"
[cms.fields.rating]
min = 0
max = 5
required = true
[cms.fields.taste]
shared = true
[cms.fields.brand]
shared = false
[cms.fields.author]
default = "Kitchen"
"#;

fn index() -> ssg_cms::index::Index {
    read(
        SETTINGS,
        &[
            (
                "content/snacks/a.md",
                "---\ntitle: A\nbrand: Tom\nimage: a.jpg\nrating: 4\ntaste: Sweet\ntype: snack\nauthor: Ann\nfacts:\n  fat: 9\n  sugar: 2\n---\n",
            ),
            (
                "content/snacks/a.th.md",
                "---\ntitle: เอ\nbrand: Tom\nimage: a.jpg\nrating: 4\ntaste: หวาน\ntype: snack\nfacts:\n  sugar: 2\n  fat: 9\n  salt: null\n  fiber: {}\n---\n",
            ),
            (
                "content/snacks/b.md",
                "---\ntitle: B\nbrand: Lee\nimage: b.jpg\nrating: 5\ntype: snack\nauthor: Ann\nfacts:\n  fat: 1\n---\n",
            ),
            (
                "content/snacks/b.th.md",
                "---\ntitle: บี\nbrand: Lee\nimage: b.jpg\nrating: 5\ntype: snack\nfacts:\n  fat: 1\n---\n",
            ),
            (
                "content/snacks/c.md",
                "---\ntitle: C\ntype: snack\nauthor: Bob\n---\n",
            ),
        ],
    )
}

#[test]
fn a_value_the_same_in_every_language_is_one_for_all() {
    let f = json_of(index());
    assert_eq!(
        f["image"],
        json!({"label": "Image", "widget": "image", "kind": "string", "shared": true})
    );
    assert!(f["title"].get("shared").is_none(), "{}", f["title"]);
    assert_eq!(
        f["facts"]["shared"],
        json!(true),
        "the order of a table's keys and its empty values do not count"
    );
    assert_eq!(f["taste"]["shared"], json!(true), "the settings say so");
    assert_eq!(f["brand"]["shared"], json!(false), "the settings say not");
}

#[test]
fn checks_and_defaults_are_settings() {
    let f = json_of(index());
    assert_eq!(
        f["rating"],
        json!({"label": "Rating", "widget": "number", "kind": "number", "shared": true, "min": 0, "max": 5, "required": true})
    );
    assert_eq!(f["author"]["default"], json!("Kitchen"));
}

#[test]
fn new_pages_of_a_section_start_with_the_value_its_pages_share() {
    let index = index();
    let snacks = index
        .sections
        .iter()
        .find(|s| s.key == "snacks")
        .expect("snacks");
    let defaults: Vec<(String, serde_json::Value)> = snacks
        .keys
        .iter()
        .filter_map(|k| {
            Some((
                k.key.clone(),
                serde_json::to_value(k.default.as_ref()?).ok()?,
            ))
        })
        .collect();
    assert_eq!(defaults, vec![("type".to_owned(), json!("snack"))]);
}
