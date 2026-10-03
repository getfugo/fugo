//! `Value`, `Map`, `Params`, `Date`, ids and kinds.

use pretty_assertions::assert_eq;
use ssg_base::{
    Date, FormatId, IdVec, Idx, KindSet, LangIdx, Map, PageId, PageKind, Params, Value,
};

fn docs_yaml() -> String {
    let path = ssg_testkit::fixture::legacy_docs().join("data/docs.yaml");
    std::fs::read_to_string(path).expect("testdata/legacy-docs/data/docs.yaml")
}

#[test]
fn docs_yaml_round_trips_with_key_case() {
    let v = Value::from_yaml_str(&docs_yaml()).unwrap();
    let root = v.as_map().unwrap();
    let config = root.get("config").and_then(Value::as_map).unwrap();
    assert_eq!(config.get("baseURL"), Some(&Value::string("")));
    assert!(config.get("baseurl").is_none());
    let lexers = root["chroma"].as_map().unwrap()["lexers"]
        .as_array()
        .unwrap();
    assert_eq!(lexers[0].as_map().unwrap()["Name"], Value::string("ABAP"));

    // JSON round trip keeps every key and value.
    let json = serde_json::to_string(&v).unwrap();
    assert_eq!(Value::from_json_str(&json).unwrap(), v);

    // The template value keeps key case and byte order.
    let t = v.to_tera();
    assert_eq!(
        t.get_from_path("config.baseURL")
            .and_then(tera::Value::as_str),
        Some("")
    );
    let tera_keys: Vec<String> = t
        .as_map()
        .unwrap()
        .keys()
        .map(|k| k.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        tera_keys,
        root.keys().map(str::to_owned).collect::<Vec<_>>()
    );
}

#[test]
fn front_matter_params_fold_but_arrays_keep_case() {
    let fm = "Title: Lay's Prawn\nDraft: false\nAuthor: {Name: A, SOCIAL: {X: '@a'}}\n\
              ingredients_percentage: [{Name: Potato, Value: 60}, {Name: Oil, Value: null}]\n";
    let v = Value::from_yaml_str(fm).unwrap();
    let p = Params::fold(v.as_map().unwrap());
    assert_eq!(p.get("title"), Some(&Value::string("Lay's Prawn")));
    assert_eq!(p.get("TITLE"), Some(&Value::string("Lay's Prawn")));
    assert_eq!(p.get("draft"), Some(&Value::Bool(false)));
    assert_eq!(p.get_path("Author.Social.x"), Some(&Value::string("@a")));
    assert_eq!(
        p.as_map().keys().collect::<Vec<_>>(),
        vec!["author", "draft", "ingredients_percentage", "title"]
    );
    let items = p.get("ingredients_percentage").unwrap().as_array().unwrap();
    let first = items[0].as_map().unwrap();
    assert_eq!(first.keys().collect::<Vec<_>>(), vec!["Name", "Value"]);
    assert_eq!(first["Name"], Value::string("Potato"));
    assert_eq!(first["Value"], Value::Int(60));
    assert_eq!(items[1].as_map().unwrap()["Value"], Value::Null);
}

#[test]
fn params_collisions_and_merges() {
    let m: Map = [
        ("title", Value::string("lower")),
        ("Title", Value::string("upper")),
        ("a", Value::Int(1)),
    ]
    .into_iter()
    .collect();
    let p = Params::fold(&m);
    assert_eq!(p.get("title"), Some(&Value::string("upper")));
    assert_eq!(p.len(), 2);

    let yaml = |s: &str| Params::fold(Value::from_yaml_str(s).unwrap().as_map().unwrap());
    let mut page = yaml("a: 1\nnested: {x: 1}\n");
    page.fill_missing_from(&yaml("a: 2\nb: 3\nnested: {x: 2, y: 2}\n"));
    assert_eq!(page, yaml("a: 1\nb: 3\nnested: {x: 1, y: 2}\n"));

    let mut top = yaml("a: 1\nnested: {x: 1, keep: true}\n");
    top.merge_deep(&yaml("a: 2\nnested: {x: 2}\nnew: n\n"));
    assert_eq!(top, yaml("a: 2\nnested: {x: 2, keep: true}\nnew: n\n"));

    let mut p = Params::default();
    p.insert(
        "Key",
        &Value::map([("Inner", Value::Int(1))].into_iter().collect()),
    );
    assert_eq!(p.get_path("key.inner"), Some(&Value::Int(1)));
    assert_eq!(p.remove("KEY").is_some(), true);
    assert!(p.is_empty());
}

#[test]
fn toml_dates_and_json_numbers() {
    let v = Value::from_toml_str(
        "a = 2024-07-14T17:31:59+07:00\nb = 2024-07-14\nc = 2024-07-14T08:00:00\nd = 07:32:00\n",
    )
    .unwrap();
    let m = v.as_map().unwrap();
    let Value::Date(Date::Zoned(z)) = &m["a"] else {
        panic!("a: {:?}", m["a"])
    };
    assert_eq!(z.offset().seconds(), 7 * 3600);
    let Value::Date(Date::Local(b)) = &m["b"] else {
        panic!()
    };
    assert_eq!(b.to_string(), "2024-07-14T00:00:00");
    let bkk = jiff::tz::TimeZone::fixed(jiff::tz::Offset::from_seconds(7 * 3600).unwrap());
    let Value::Date(c) = &m["c"] else { panic!() };
    assert_eq!(
        c.in_tz(&bkk).unwrap().timestamp().as_second(),
        1_720_918_800
    );
    assert_eq!(m["d"], Value::string("07:32:00"));
    assert_eq!(m["a"].to_tera().as_str(), Some("2024-07-14T17:31:59+07:00"));

    let j = Value::from_json_str(r#"{"big": 18446744073709551615, "n": -3, "f": 1.5}"#).unwrap();
    let j = j.as_map().unwrap();
    assert_eq!(j["n"], Value::Int(-3));
    assert!(matches!(j["big"], Value::Float(_)));
    assert_eq!(j["f"].as_f64(), Some(1.5));

    let y = Value::from_yaml_str("1: one\ntrue: yes\ndate: 2024-07-14\n").unwrap();
    let y = y.as_map().unwrap();
    assert_eq!(y["1"], Value::string("one"));
    assert_eq!(y["date"], Value::string("2024-07-14"));
}

#[test]
fn yaml_non_finite_floats_keep_their_yaml_spelling() {
    let v = Value::from_yaml_str("a: .inf\nb: -.inf\nc: .nan\nd: 1.5\n").unwrap();
    let m = v.as_map().unwrap();
    assert_eq!(m.get("a"), Some(&Value::string(".inf")));
    assert_eq!(m.get("b"), Some(&Value::string("-.inf")));
    assert_eq!(m.get("c"), Some(&Value::string(".nan")));
    assert_eq!(m.get("d"), Some(&Value::Float(1.5)));
}

#[test]
fn id_vec_and_kinds() {
    let mut pages: IdVec<PageId, &str> = IdVec::new();
    let home = pages.push("home");
    let about = pages.push("about");
    assert_eq!(home.index(), 0);
    assert_eq!(pages[about], "about");
    pages[about] = "about-us";
    assert_eq!(pages.get(PageId::from_index(1)), Some(&"about-us"));
    assert_eq!(pages.get(PageId::from_index(2)), None);
    assert_eq!(
        pages
            .iter_enumerated()
            .map(|(i, p)| (i.raw(), *p))
            .collect::<Vec<_>>(),
        vec![(0, "home"), (1, "about-us")]
    );
    assert_eq!(pages.ids().collect::<Vec<_>>(), vec![home, about]);
    let langs: IdVec<LangIdx, &str> = vec!["en", "th"].into();
    assert_eq!(langs[LangIdx::from_raw(1)], "th");

    assert_eq!(PageKind::parse("taxonomyTerm"), Some(PageKind::Taxonomy));
    assert_eq!(PageKind::parse("404"), Some(PageKind::NotFound));
    assert_eq!(PageKind::parse("RobotsTXT"), Some(PageKind::RobotsTxt));
    assert_eq!(PageKind::parse("rss"), None);
    assert_eq!(
        serde_json::to_string(&PageKind::NotFound).unwrap(),
        "\"404\""
    );
    let set: KindSet = [PageKind::Term, PageKind::Home].into_iter().collect();
    assert!(set.contains(PageKind::Home) && !set.contains(PageKind::Page));
    assert_eq!(
        set.iter().collect::<Vec<_>>(),
        vec![PageKind::Home, PageKind::Term]
    );
    assert_eq!(KindSet::ALL.len(), PageKind::ALL.len());
    assert!(PageKind::Section.is_branch() && !PageKind::Page.is_branch());
}

#[test]
#[should_panic(expected = "FormatId overflow")]
fn id_overflow_panics() {
    let _ = FormatId::from_index(256);
}

#[test]
fn diagnostics_are_sorted_and_deduplicated() {
    use ssg_base::diag::{Diagnostic, Diagnostics, Severity};
    let d = Diagnostics::new(["Ignored-ID"]);
    d.push(Diagnostic::warning("b"));
    d.push(Diagnostic::error("a").with_id("dup"));
    d.push(Diagnostic::error("a again").with_id("dup"));
    d.push(Diagnostic::warning("b"));
    d.push(Diagnostic::info("x").with_id("ignored-id"));
    let r = d.report();
    assert_eq!(
        r.iter()
            .map(|d| (d.severity, d.message.as_str()))
            .collect::<Vec<_>>(),
        vec![(Severity::Error, "a"), (Severity::Warning, "b")]
    );
    assert!(d.has_errors());
}

#[test]
fn map_params_and_id_vec_serde() {
    // `Map` deserializes from a table only, keeping key case; it iterates in byte order.
    let m: Map = serde_json::from_str(r#"{"b": 1, "A": {"C": [true]}, "a": null}"#).unwrap();
    assert_eq!(m.keys().collect::<Vec<_>>(), vec!["A", "a", "b"]);
    assert_eq!(m["b"], Value::Int(1));
    assert_eq!(
        serde_json::to_string(&m).unwrap(),
        r#"{"A":{"C":[true]},"a":null,"b":1}"#
    );
    assert!(serde_json::from_str::<Map>("[1]").is_err());

    // `Params` serializes its folded map and folds what it deserializes.
    let p: Params = serde_json::from_str(r#"{"Title": "T", "Author": {"Name": "A"}}"#).unwrap();
    assert_eq!(p.get_path("author.name"), Some(&Value::string("A")));
    assert_eq!(
        serde_json::to_string(&p).unwrap(),
        r#"{"author":{"name":"A"},"title":"T"}"#
    );

    // `IdVec` is a sequence in id order.
    let v: IdVec<LangIdx, String> = serde_json::from_str(r#"["en", "th"]"#).unwrap();
    assert_eq!(v[LangIdx::from_index(1)], "th");
    assert_eq!(serde_json::to_string(&v).unwrap(), r#"["en","th"]"#);
}
