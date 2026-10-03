//! The `media` oracle: the built-in media type and output format tables, and `[mediaTypes]` /
//! `[outputFormats]` decoding.
//!
//! The tables must be equal. Decoding is compared on every case whose input is a table; the
//! named cases must match except those in [`NAMED_DEVIATIONS`]; the generated (`random:N`)
//! cases are tallied, and their mismatches must fall into the reviewed categories of
//! [`classify`].

use serde_json::{Value as J, json};
use ssg_base::{Map, Value};
use ssg_config::media::{BUILTIN, DEFAULT_CONTENT_TYPES};
use ssg_config::output::{Escaping, LinkPolicy, Listing, Placement, UglyPolicy};
use ssg_config::{MediaTypes, OutputFormats, tree};

use crate::support::fixture;

/// Named cases whose Go result depends on mapstructure's weak decoding of struct fields that
/// This port does not have (a media type entry may only set `suffixes` and `delimiter`; an
/// output format entry cannot set its name), or on a configuration map that is not
/// normalised first (`viaProvider: false` with upper-case keys).
const NAMED_DEVIATIONS: &[(&str, &str)] = &[
    (
        "case-mixed",
        "keys are normalised (lower case) before decoding",
    ),
    ("upper-dup", "keys are normalised; `RSS` and `rss` collide"),
    (
        "override-fields",
        "mainType/subType/type/firstSuffix of an entry are not settable",
    ),
    (
        "suffix-mixed-list",
        "suffixes that are not strings are rejected",
    ),
    ("suffix-blank", "blank suffixes are dropped"),
    ("key-empty-parts", "a type needs a main and a sub type"),
    ("input-string", "the section must be a table"),
    ("input-list", "the section must be a table"),
    ("weight-float", "a fractional weight is rejected"),
    ("no-mediatype", "an output format needs a media type"),
    ("unknown-field", "an output format needs a media type"),
    ("value-string-map", "an output format needs a media type"),
    ("value-nil", "an output format needs a media type"),
];

fn media_json(t: &ssg_config::MediaType) -> J {
    json!({"type": t.to_string(), "suffixes": t.suffixes.join(","), "delimiter": t.delimiter})
}

fn go_media(t: &J) -> J {
    json!({"type": t["Type"], "suffixes": t["SuffixesCSV"], "delimiter": t["Delimiter"]})
}

fn input_map(v: &J) -> Option<Map> {
    match Value::from_json(v.clone()) {
        Value::Map(m) => Some(tree::normalize_keys(&m)),
        Value::Null => Some(Map::new()),
        _ => None,
    }
}

#[test]
fn builtin_media_tables() {
    let fx = fixture("oracle/media/media/media.json.gz");
    let cases = fx["cases"].as_array().expect("cases");
    let find = |op: &str| cases.iter().find(|c| c["op"] == op).expect(op);

    // Builtin: the named types.
    let mut go: Vec<J> = find("Builtin")["fields"]
        .as_array()
        .expect("fields")
        .iter()
        .map(|f| go_media(&f[1]))
        .collect();
    let mut mine: Vec<J> = BUILTIN
        .iter()
        .map(|(t, s)| json!({"type": t, "suffixes": s.join(","), "delimiter": "."}))
        .collect();
    let key = |v: &J| v["type"].as_str().unwrap_or_default().to_owned();
    go.sort_by_key(key);
    go.dedup();
    mine.sort_by_key(key);
    // Go's Builtin struct also names types that are not in the default table.
    for t in &mine {
        assert!(go.contains(t), "built-in {t} is not one of Go's");
    }

    // DefaultTypes: the sorted default table, field by field.
    let go: Vec<J> = find("DefaultTypes")["types"]
        .as_array()
        .expect("types")
        .iter()
        .map(|t| {
            json!({"type": t["Type"], "main": t["MainType"], "sub": t["SubType"],
                   "mime": t["mimeSuffix"], "suffixes": t["SuffixesCSV"],
                   "delimiter": t["Delimiter"], "first": t["FirstSuffix"]["Suffix"],
                   "full": t["FirstSuffix"]["FullSuffix"]})
        })
        .collect();
    let mine: Vec<J> = MediaTypes::default()
        .iter()
        .map(|(_, t)| {
            json!({"type": t.to_string(), "main": t.main, "sub": t.sub, "mime": t.mime_suffix,
                   "suffixes": t.suffixes.join(","), "delimiter": t.delimiter,
                   "first": t.first_suffix(), "full": t.full_suffix()})
        })
        .collect();
    assert_eq!(mine, go, "default media types");

    // DefaultContentTypes.
    let mut go: Vec<String> = find("DefaultContentTypes")["out"]
        .as_object()
        .expect("out")
        .values()
        .filter_map(|t| t["Type"].as_str().map(str::to_owned))
        .collect();
    go.sort();
    let mut mine: Vec<String> = DEFAULT_CONTENT_TYPES
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    mine.sort();
    assert_eq!(mine, go, "default content types");
    eprintln!(
        "media tables: {} built-in types, {} content types equal",
        BUILTIN.len(),
        mine.len()
    );
}

/// The category of a mismatch of a generated case (reviewed; see the crate README).
fn classify(input: &J, go_err: bool) -> Option<&'static str> {
    let text = input.to_string();
    let lower_keys = input
        .as_object()
        .is_some_and(|o| o.keys().any(|k| k.chars().any(char::is_uppercase)));
    if go_err {
        // Go rejects what this port decodes: weakly-typed struct fields Go cannot convert.
        return Some("go rejects a field value this port ignores or converts");
    }
    if lower_keys {
        return Some("keys are normalised before decoding");
    }
    for field in [
        "mainType",
        "subType",
        "\"type\"",
        "firstSuffix",
        "suffixesCSV",
        "mimeSuffix",
    ] {
        if text.contains(field) {
            return Some("mainType/subType/type/firstSuffix fields are not settable");
        }
    }
    if text.contains(";") {
        return Some("parameters after ';' are dropped");
    }
    Some("a scalar where a table or string list is expected is rejected")
}

#[test]
fn decode_media_types() {
    let fx = fixture("oracle/media/media/media.json.gz");
    let (mut checks, mut passed, mut named_dev, mut random_dev) = (0, 0, 0, 0);
    let mut failures = Vec::new();
    let mut categories = std::collections::BTreeMap::<&str, usize>::new();
    for c in fx["cases"].as_array().expect("cases") {
        if c["op"] != "DecodeTypes" || c.get("nondet").is_some() {
            continue;
        }
        let name = c["name"].as_str().unwrap_or_default();
        checks += 1;
        let mine = input_map(&c["in"])
            .ok_or(())
            .and_then(|m| MediaTypes::decode(&m).map_err(|_| ()));
        let go = c.get("ok").map(|ok| {
            ok["types"]
                .as_array()
                .map(|a| a.iter().map(go_media).collect::<Vec<_>>())
                .unwrap_or_default()
        });
        let same = match (&mine, &go) {
            (Ok(m), Some(g)) => m.iter().map(|(_, t)| media_json(t)).collect::<Vec<_>>() == *g,
            (Err(()), None) => true,
            _ => false,
        };
        if same {
            passed += 1;
        } else if name.starts_with("random:") {
            random_dev += 1;
            *categories
                .entry(classify(&c["in"], go.is_none()).expect("category"))
                .or_default() += 1;
        } else if NAMED_DEVIATIONS.iter().any(|(n, _)| *n == name) {
            named_dev += 1;
        } else {
            failures.push(format!("{name}: {:?}", c["in"]));
        }
    }
    eprintln!(
        "media/DecodeTypes: {checks} cases, {passed} equal, {named_dev} named and {random_dev} generated accepted deviations {categories:?}"
    );
    assert!(failures.is_empty(), "unexpected:\n{}", failures.join("\n"));
}

fn go_format(f: &J) -> J {
    json!({
        "name": f["Name"].as_str().unwrap_or_default().to_lowercase(), "media": f["MediaType"]["Type"],
        "base": f["BaseName"], "path": f["Path"], "rel": f["Rel"], "protocol": f["Protocol"],
        "plain": f["IsPlainText"], "html": f["IsHTML"], "nougly": f["NoUgly"], "ugly": f["Ugly"],
        "notalt": f["NotAlternative"], "root": f["Root"], "perm": f["Permalinkable"],
        "weight": f["Weight"],
    })
}

fn my_formats(f: &OutputFormats, types: &MediaTypes) -> Vec<J> {
    f.iter()
        .map(|(_, f)| {
            json!({
                "name": f.name, "media": types.get(f.media_type).to_string(), "base": f.base_name,
                "path": f.path, "rel": f.rel, "protocol": f.protocol,
                "plain": f.escaping == Escaping::Plain, "html": f.is_html,
                "nougly": f.ugly == UglyPolicy::Never, "ugly": f.ugly == UglyPolicy::Always,
                "notalt": f.listing == Listing::NotAlternative,
                "root": f.placement == Placement::Root, "perm": f.links == LinkPolicy::Own,
                "weight": f.weight,
            })
        })
        .collect()
}

#[test]
fn output_formats() {
    let fx = fixture("oracle/media/output/output.json.gz");
    let cases = fx["cases"].as_array().expect("cases");
    let types = MediaTypes::default();
    let custom_types = MediaTypes::decode(&input_map(&fx["customTypesInput"]).expect("map"))
        .expect("custom types");

    // The built-in table in render order.
    let go: Vec<J> = cases
        .iter()
        .find(|c| c["op"] == "DefaultFormats")
        .expect("DefaultFormats")["formats"]
        .as_array()
        .expect("formats")
        .iter()
        .map(go_format)
        .collect();
    assert_eq!(
        my_formats(&OutputFormats::builtin(&types), &types),
        go,
        "default output formats"
    );

    let (mut checks, mut passed, mut named_dev, mut random_dev) = (0, 0, 0, 0);
    let mut failures = Vec::new();
    let mut categories = std::collections::BTreeMap::<&str, usize>::new();
    for c in cases {
        if c["op"] != "DecodeConfig" || c.get("nondet").is_some() || c.get("panic").is_some() {
            continue;
        }
        let name = c["name"].as_str().unwrap_or_default();
        checks += 1;
        let t = if c["types"] == "custom" {
            &custom_types
        } else {
            &types
        };
        let mine = input_map(&c["in"])
            .ok_or(())
            .and_then(|m| OutputFormats::decode(&m, t).map_err(|_| ()));
        let go = c.get("ok").map(|ok| {
            ok["formats"]
                .as_array()
                .map(|a| a.iter().map(go_format).collect::<Vec<_>>())
                .unwrap_or_default()
        });
        let same = match (&mine, &go) {
            (Ok(m), Some(g)) => my_formats(m, t) == *g,
            (Err(()), None) => true,
            _ => false,
        };
        if same {
            passed += 1;
        } else if name.starts_with("random:") {
            random_dev += 1;
            *categories
                .entry(classify_format(&c["in"], go.is_none()))
                .or_default() += 1;
        } else if NAMED_DEVIATIONS.iter().any(|(n, _)| *n == name) {
            named_dev += 1;
        } else {
            failures.push(format!("{name} ({}): {:?}", c["types"], c["in"]));
        }
    }
    eprintln!(
        "output/DecodeConfig: {checks} cases, {passed} equal, {named_dev} named and {random_dev} generated accepted deviations {categories:?}"
    );
    assert!(failures.is_empty(), "unexpected:\n{}", failures.join("\n"));
}

fn classify_format(input: &J, go_err: bool) -> &'static str {
    if go_err {
        return "go rejects a field value this port ignores or converts";
    }
    let upper = input
        .as_object()
        .is_some_and(|o| o.keys().any(|k| k.chars().any(char::is_uppercase)));
    if upper {
        return "keys are normalised before decoding";
    }
    "a field value this port rejects (weak decoding)"
}
