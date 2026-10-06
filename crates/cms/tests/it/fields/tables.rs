//! Keys inside tables: the build counts them across the pages (so that a page can add the ones
//! it lacks), and the settings reach them by their path.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::{json_of, read};

const SETTINGS: &str = r#"
[cms.fields.nutrition]
collapsed = true
summary = "{calories} kcal"
[cms.fields."nutrition.fat.total"]
label = "Total fat (g)"
min = 0
[cms.fields."nutrition.vitamins"]
label = "Vitamins"
[cms.fields."nutrition.vitamins.c"]
label = "Vitamin C (mg)"
"#;

fn fields() -> BTreeMap<String, Value> {
    json_of(read(
        SETTINGS,
        &[
            (
                "content/snacks/a.md",
                "---\ntitle: A\nnutrition:\n  calories: 170\n  fat:\n    total: 9\nparts:\n  - name: Rice\n    share: 60\n---\n",
            ),
            (
                "content/snacks/b.md",
                "---\ntitle: B\nnutrition:\n  fat:\n    total: 3\n    saturated: 1\nparts:\n  - name: Corn\n    share: 40.5\n---\n",
            ),
        ],
    ))
}

#[test]
fn every_key_inside_the_tables_of_any_page() {
    let f = fields();
    assert_eq!(
        f["nutrition.calories"],
        json!({"label": "Calories", "widget": "number", "kind": "number"})
    );
    assert_eq!(f["nutrition.fat"], json!({"label": "Fat", "kind": "map"}));
    assert_eq!(
        f["nutrition.fat.saturated"],
        json!({"label": "Saturated", "widget": "number", "kind": "number"}),
        "one page has it: the others can add it"
    );
    assert_eq!(f["parts"], json!({"label": "Parts", "kind": "objects"}));
    assert_eq!(
        f["parts.share"],
        json!({"label": "Share", "widget": "number", "kind": "number"}),
        "the keys of a list's tables are below the list"
    );
}

#[test]
fn settings_reach_a_key_inside_a_table_by_its_path() {
    let f = fields();
    assert_eq!(
        f["nutrition"],
        json!({"label": "Nutrition", "kind": "map", "collapsed": true, "summary": "{calories} kcal"})
    );
    assert_eq!(
        f["nutrition.fat.total"],
        json!({"label": "Total fat (g)", "widget": "number", "kind": "number", "min": 0})
    );
    assert_eq!(
        f["nutrition.vitamins"],
        json!({"label": "Vitamins", "kind": "map", "unused": true}),
        "a key only the settings name, offered in the table: a table, since they name keys below it"
    );
    assert_eq!(
        f["nutrition.vitamins.c"],
        json!({"label": "Vitamin C (mg)", "kind": "string", "unused": true})
    );
}
