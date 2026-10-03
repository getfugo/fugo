//! Map outputs are sorted, whatever serde_json's `Map` keeps (REWRITE_PLAN.md §8.2, T31).

use tera::{Context, Value};

use crate::support::Harness;

/// rolldown turns on serde_json's `preserve_order` and `arbitrary_precision` in every build it
/// is part of, and the workspace-hack turns them on for every member, so this test build has
/// them as the binary does. This port must not rely on `serde_json::Map`'s order: its own values
/// sort their keys.
#[test]
fn serde_json_has_the_binarys_features() {
    // Only `arbitrary_precision` keeps a number beyond f64 instead of failing.
    assert!(
        serde_json::from_str::<serde_json::Value>("1e999").is_ok(),
        "serde_json/arbitrary_precision is off: drop it from the workspace-hack"
    );
    let mut m = serde_json::Map::new();
    m.insert("b".into(), serde_json::Value::Null);
    m.insert("a".into(), serde_json::Value::Null);
    let keys: Vec<&str> = m.keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        ["b", "a"],
        "serde_json/preserve_order is off: drop it from the workspace-hack"
    );
    let v = ssg_base::Value::from_json(serde_json::Value::Object(m));
    let sorted: Vec<&str> = v.as_map().expect("a map").keys().collect();
    assert_eq!(sorted, ["a", "b"]);
}

fn keys(v: &Value) -> Vec<String> {
    v.as_map()
        .expect("a map")
        .keys()
        .map(|k| k.as_str().unwrap_or_default().to_owned())
        .collect()
}

fn assert_sorted(label: &str, v: &Value) {
    let k = keys(v);
    let mut sorted = k.clone();
    sorted.sort();
    assert_eq!(k, sorted, "{label}: keys not sorted");
    for inner in v.as_map().expect("a map").values() {
        if inner.is_map() {
            assert_sorted(label, inner);
        }
    }
}

#[test]
fn map_outputs_are_sorted() {
    let h = Harness::new();
    let ctx = Context::new();
    for (label, expr) in [
        (
            "merge",
            "{'z': 1, 'b': {'y': 1, 'x': 2} } | merge(with={'c': 1, 'a': {'q': 1} })",
        ),
        ("sort_keys", "{'z': 1, 'a': 2, 'm': 3} | sort_keys"),
        ("parse_url", "'https://example.org/x?q=1#f' | parse_url"),
        ("to_date", "'2024-01-01' | to_date"),
        ("now", "now()"),
    ] {
        let v = h
            .eval(expr, &ctx)
            .unwrap_or_else(|e| panic!("{label}: {e}"));
        assert_sorted(label, &v);
    }
    // text outputs of maps: jsonify, dump and remarshal write keys in order
    let out = h
        .render(
            "{{ {'z': 1, 'a': {'y': 1, 'b': 2} } | jsonify }}|{{ {'z': 1, 'a': 2} | remarshal(format='yaml') }}",
            &ctx,
        )
        .unwrap();
    assert_eq!(out, "{\"a\":{\"b\":2,\"y\":1},\"z\":1}|a: 2\nz: 1\n");
}
