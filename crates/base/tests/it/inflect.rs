//! `inflect` and `title` against the `common/flect` and `common/prose` oracles.

use serde_json::Value as J;
use ssg_base::inflect::{self, CustomInflections, Inflector};
use ssg_base::title::{self, Style};

use crate::support::{Tally, fixture, text};

include!("../../../../testdata/oracle/common/flect/upstream_tables.rs");

/// A Go panic (Go reports a template error) is recorded as `{"panic": …}`.
fn is_panic(v: &J) -> bool {
    v.get("panic").is_some()
}

#[test]
fn flect_corpus_oracle() {
    let f = fixture("oracle/common/flect/flect.json");
    let mut t = Tally::new("flect");
    for c in f["cases"].as_array().unwrap() {
        let Some(s) = text(&c["in"]) else {
            t.skip(|| format!("{}: input is not UTF-8", c["in"]));
            continue;
        };
        let checks: [(&str, String); 6] = [
            ("pluralize", inflect::pluralize(s)),
            ("singularize", inflect::singularize(s)),
            ("humanize", inflect::humanize(s)),
            ("titleize", inflect::titleize(s)),
            ("ordinalize", inflect::ordinalize_str(s)),
            ("inflect_humanize", inflect::humanize_text(s)),
        ];
        for (name, got) in checks {
            let want = &c[name];
            if is_panic(want) {
                // Go fails on a non-blank string without words; we return a string.
                t.deviation(|| format!("{name}({s:?}): Go fails, we return {got:?}"));
                continue;
            }
            t.check(*want == got.as_str(), || {
                format!("{name}({s:?}): want {want}, got {got:?}")
            });
        }
    }
    t.finish();
}

fn run(inf: &Inflector, op: &str, s: &str) -> String {
    match op {
        "pluralize" => inf.pluralize(s),
        "singularize" => inf.singularize(s),
        "humanize" => inf.humanize(s),
        "titleize" => inf.titleize(s),
        "capitalize" => inf.capitalize(s),
        "ordinalize" => inflect::ordinalize_str(s),
        other => panic!("unknown op {other}"),
    }
}

#[test]
fn flect_custom_data_oracle() {
    let f = fixture("oracle/common/flect/flect.json");
    let mut t = Tally::new("flect/custom");
    for cfg in f["custom"].as_array().unwrap() {
        let name = cfg["name"].as_str().unwrap();
        let custom =
            CustomInflections::from_json(cfg["inflections"].as_str(), cfg["acronyms"].as_str());
        let failed_to_load = !cfg["printed"].as_array().unwrap().is_empty();
        t.check(custom.is_err() == failed_to_load, || {
            format!(
                "{name}: load error {:?}, Go printed {}",
                custom.as_ref().err(),
                cfg["printed"]
            )
        });
        let custom = custom.unwrap_or_default();
        let inflector = Inflector::with_custom(&custom);
        if cfg.get("init_panic").is_some() {
            t.check(inflector.is_err(), || {
                format!("{name}: want a duplicate-word error")
            });
            continue;
        }
        let inflector = inflector.unwrap();
        for (key, want) in cfg["results"].as_object().unwrap() {
            let (op, input) = key.split_once(':').unwrap();
            let got = run(&inflector, op, input);
            if is_panic(want) {
                t.deviation(|| format!("{name} {op}({input:?}): Go fails, we return {got:?}"));
                continue;
            }
            t.check(*want == got.as_str(), || {
                format!("{name} {op}({input:?}): want {want}, got {got:?}")
            });
        }
    }
    t.finish();
}

#[test]
fn flect_upstream_tables() {
    let inf = Inflector::standard();
    for &(singular, plural, do_singularize, do_pluralize) in SINGLE_PLURAL_ASSERTIONS {
        if do_pluralize {
            assert_eq!(inf.pluralize(singular), plural, "pluralize {singular}");
            assert_eq!(inf.pluralize(plural), plural, "pluralize {plural}");
        }
        if do_singularize {
            assert_eq!(inf.singularize(plural), singular, "singularize {plural}");
            assert_eq!(
                inf.singularize(singular),
                singular,
                "singularize {singular}"
            );
        }
    }
    type Table = &'static [(&'static str, &'static str)];
    type Case = (&'static str, fn(&str) -> String, Table);
    let tables: [Case; 4] = [
        ("humanize", inflect::humanize, HUMANIZE),
        ("titleize", inflect::titleize, TITLEIZE),
        (
            "capitalize",
            |s| Inflector::standard().capitalize(s),
            CAPITALIZE,
        ),
        ("ordinalize", inflect::ordinalize_str, ORDINALIZE),
    ];
    for (name, f, table) in tables {
        for &(act, exp) in table {
            assert_eq!(f(act), exp, "{name}({act:?})");
            assert_eq!(f(exp), exp, "{name}({exp:?})");
        }
    }
    for &(original, parts) in IDENT_NEW {
        assert_eq!(inf.words(original), parts, "words({original:?})");
    }
    assert_eq!(inflect::ordinalize(-22), "-22nd");
    assert_eq!(inflect::ordinalize(111), "111th");
}

#[test]
fn flect_custom_upstream() {
    let custom =
        CustomInflections::from_json(Some(r#"{"baby":"bebe","xyz":"zyx"}"#), None).unwrap();
    let inf = Inflector::with_custom(&custom).unwrap();
    for (s, p) in [("baby", "bebe"), ("xyz", "zyx")] {
        assert_eq!(inf.pluralize(s), p);
        assert_eq!(inf.pluralize(p), p);
        assert_eq!(inf.singularize(s), s);
        assert_eq!(inf.singularize(p), s);
    }
    for bad in [r#"{"a file":"files"}"#, r#"{"beatle":"the beatles"}"#] {
        assert!(CustomInflections::from_json(Some(bad), None).is_err());
    }
    let custom = CustomInflections::from_json(None, Some(r#"["ACC","TLC","LSA"]"#)).unwrap();
    let inf = Inflector::with_custom(&custom).unwrap();
    for a in ["ACC", "TLC", "LSA"] {
        assert_eq!(inf.titleize(&a.to_lowercase()), a);
    }
}

#[test]
fn prose_corpus_oracle() {
    let f = fixture("oracle/common/prose/title.json");
    let mut t = Tally::new("prose");
    for c in f["cases"].as_array().unwrap() {
        let Some(s) = text(&c["in"]) else {
            t.skip(|| format!("{}: input is not UTF-8", c["in"]));
            continue;
        };
        let checks = [
            ("ap", title::title_case(s, Style::Ap)),
            ("chicago", title::title_case(s, Style::Chicago)),
            ("create_title", title::title_case(s, Style::parse(""))),
            (
                "ap_pluralize",
                title::title_case(&inflect::pluralize(s), Style::Ap),
            ),
        ];
        for (name, got) in checks {
            let want = &c[name];
            t.check(*want == got.as_str(), || {
                format!("{name}({s:?}): want {want}, got {got:?}")
            });
        }
    }
    for c in f["upstream"].as_array().unwrap() {
        let s = c["in"].as_str().unwrap();
        let got = title::title_case(s, Style::Ap);
        t.check(c["expect"] == got.as_str(), || {
            format!("upstream ap({s:?}): want {}, got {got:?}", c["expect"])
        });
    }
    t.finish();
}

/// Section and taxonomy names of a snack review site that the oracle corpora do not contain;
/// expectations derived by hand from the rules.
#[test]
fn site_names() {
    let ap = |s: &str| title::title_case(s, Style::Ap);
    let section = |s: &str| ap(&inflect::pluralize(s));
    for (name, plural_title) in [
        ("potato-chips", "Potato-Chips"),
        ("biscuit", "Biscuits"),
        ("seafood", "Seafoods"),
        ("candy", "Candies"),
        ("bread-pan", "Bread-Pans"),
        ("pretzels", "Pretzels"),
        ("cookies", "Cookies"),
        ("snacks", "Snacks"),
        ("companies", "Companies"),
        ("categories", "Categories"),
        ("brands", "Brands"),
        ("countries", "Countries"),
        ("ingredients", "Ingredients"),
        ("tags", "Tags"),
    ] {
        assert_eq!(section(name), plural_title, "section title of {name:?}");
    }
    for (singular, plural) in [
        ("category", "categories"),
        ("brand", "brands"),
        ("company", "companies"),
        ("country", "countries"),
        ("ingredient", "ingredients"),
        ("tag", "tags"),
    ] {
        assert_eq!(inflect::pluralize(singular), plural);
        assert_eq!(inflect::singularize(plural), singular);
    }
    for (term, title) in [
        ("Lay's", "Lay's"),
        ("Tao Kae Noi", "Tao Kae Noi"),
        ("Thai Lotte Co., Ltd.", "Thai Lotte Co., Ltd."),
        ("Ezaki Glico Co., Ltd.", "Ezaki Glico Co., Ltd."),
        ("INS 322(i)", "INS 322(i)"),
        ("ins-322i", "Ins-322i"),
        ("Palm Oil", "Palm Oil"),
        ("Wheat Flour", "Wheat Flour"),
        ("crispy", "Crispy"),
        ("Le Pan Bakery", "Le Pan Bakery"),
        ("USA", "USA"),
        ("กรอบ", "กรอบ"),
        ("คริสปี้พาย", "คริสปี้พาย"),
        ("ปาร์ตี้", "ปาร์ตี้"),
    ] {
        assert_eq!(ap(term), title, "AP title of {term:?}");
    }
    assert_eq!(inflect::humanize("potato-chips"), "Potato chips");
    assert_eq!(inflect::humanize_text("companies"), "Companies");
    assert_eq!(inflect::humanize("ญี่ปุ่น"), "ญ ป น");
    assert_eq!(
        title::title_case("the lord of the rings", Style::Chicago),
        "The Lord of the Rings"
    );
    assert_eq!(
        title::title_case("hello wORLD-foo", Style::Go),
        "Hello WORLD-Foo"
    );
    assert_eq!(
        title::title_case("éclair au chocolat", Style::FirstUpper),
        "Éclair au chocolat"
    );
    assert_eq!(title::title_case("keep As is", Style::None), "keep As is");
}
