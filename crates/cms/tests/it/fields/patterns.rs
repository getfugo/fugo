//! Patterns of keys: the settings of `[cms.fields."rating.*"]` reach every key the pattern
//! matches, under the settings of a longer pattern and of the key itself.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use super::{json_of, read};

const SETTINGS: &str = r#"
[cms.fields."rating.*"]
min = 0
max = 5
step = 1
[cms.fields."rating.sweet"]
label = "Sweetness"
max = 10
[cms.fields."nutrition.**.percentage"]
label = "% Daily Value"
[cms.fields."**.percentage"]
label = "Share (%)"
help = "Of the daily value"
[cms.fields."rating.smell"]
label = "Smell"
"#;

fn fields() -> BTreeMap<String, Value> {
    json_of(read(
        SETTINGS,
        &[
            (
                "content/snacks/a.md",
                "---\ntitle: A\nrating:\n  sweet: 4\n  sour: 2\n  Crispy:\n    score: 3\nnutrition:\n  fat:\n    total: 9\n    percentage: 12\n    saturated:\n      percentage: 5\n---\n",
            ),
            (
                "content/snacks/b.md",
                "---\ntitle: B\nrating:\n  sour: 1\nparts:\n  - name: Rice\n    percentage: 60\n---\n",
            ),
        ],
    ))
}

#[test]
fn a_pattern_reaches_every_key_it_matches() {
    let f = fields();
    assert_eq!(
        f["rating.sour"],
        json!({"label": "Sour", "widget": "number", "kind": "number", "min": 0, "max": 5, "step": 1})
    );
    assert_eq!(
        f["rating.crispy"],
        json!({"label": "Crispy", "kind": "map", "min": 0, "max": 5, "step": 1, "name": "Crispy"}),
        "ignoring case"
    );
    assert_eq!(
        f["rating.crispy.score"],
        json!({"label": "Score", "widget": "number", "kind": "number"}),
        "`*` is one key"
    );
    assert_eq!(
        f["rating"],
        json!({"label": "Rating", "kind": "map"}),
        "the table the keys are in is not one of them"
    );
    assert_eq!(
        f["nutrition.fat.saturated.percentage"],
        json!({"label": "% Daily Value", "widget": "number", "kind": "number", "help": "Of the daily value"}),
        "`**` crosses keys"
    );
}

#[test]
fn the_longer_pattern_and_the_key_itself_go_over_it() {
    let f = fields();
    assert_eq!(
        f["rating.sweet"],
        json!({"label": "Sweetness", "widget": "number", "kind": "number", "min": 0, "max": 10, "step": 1}),
        "setting by setting"
    );
    assert_eq!(
        f["nutrition.fat.percentage"],
        json!({"label": "% Daily Value", "widget": "number", "kind": "number", "help": "Of the daily value"})
    );
    assert_eq!(
        f["parts.percentage"],
        json!({"label": "Share (%)", "widget": "number", "kind": "number", "help": "Of the daily value"})
    );
}

#[test]
fn a_pattern_names_no_key_but_reaches_those_the_settings_name() {
    let f = fields();
    assert!(
        f.keys().all(|k| !k.contains('*')),
        "{:?}",
        f.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        f["rating.smell"],
        json!({"label": "Smell", "kind": "string", "min": 0, "max": 5, "step": 1, "unused": true})
    );
}
