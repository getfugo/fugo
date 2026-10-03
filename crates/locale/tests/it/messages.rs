//! The message evaluator and bundle lookup on the reconstruction's i18n files and on
//! hand-written cases.

use std::path::Path;

use pretty_assertions::assert_eq;
use ssg_base::{Idx as _, LangIdx, Map, Value};
use ssg_locale::{
    Args, EvalError, I18nError, MessageProblem, NO_VALUE, Piece, PluralCount, Template,
    TranslateError, Translation, Translations, TranslationsBuilder,
};

fn map(entries: &[(&str, Value)]) -> Value {
    let mut m = Map::new();
    for (k, v) in entries {
        m.insert(*k, v.clone());
    }
    Value::map(m)
}

/// An English and a Thai translation file of a review site.
const EN: &str = r#"review = "Review"
latest = "Latest snacks"
home = "Home"
related = "Related snacks"
rating = "Rating"
notFound = "Page not found"
welcome = "Welcome to {{ .Name }}"
[words]
one = "word"
other = "words"
[reviews]
one = "{{ .Count }} review"
other = "{{ .Count }} reviews"
[comments]
one = "One comment"
other = "{{ .Count }} comments"
"#;
const TH: &str = r#"review = "รีวิว"
latest = "ขนมล่าสุด"
home = "หน้าแรก"
related = "ขนมที่เกี่ยวข้อง"
rating = "คะแนน"
welcome = "ยินดีต้อนรับสู่ {{ .Name }}"
[words]
other = "คำ"
[reviews]
other = "{{ .Count }} รีวิว"
[comments]
other = "{{ .Count }} ความคิดเห็น"
"#;

/// en and th of a review site.
fn review_site() -> Translations {
    let mut b = TranslationsBuilder::new("en");
    for (name, text) in [("i18n/en.toml", EN), ("i18n/th.toml", TH)] {
        b.add_file(Path::new(name), text).unwrap();
    }
    b.build(["en", "th"])
}

fn lang(i: usize) -> LangIdx {
    LangIdx::from_index(i)
}

#[test]
fn welcome_reviews_comments() {
    let t = review_site();
    let (en, th) = (lang(0), lang(1));
    let site = map(&[("Name", Value::string("Snacks"))]);
    let tr = |l, key: &str, arg: &Value| t.translate(l, key, &Args::from_value(arg)).unwrap();
    // {{ i18n "welcome" (dict "Name" .Site.Title) }}
    assert_eq!(tr(en, "welcome", &site), "Welcome to Snacks");
    assert_eq!(tr(th, "welcome", &site), "ยินดีต้อนรับสู่ Snacks");
    // {{ i18n "reviews" 1 }} / {{ i18n "reviews" 5 }}
    assert_eq!(tr(en, "reviews", &Value::Int(1)), "1 review");
    assert_eq!(tr(en, "reviews", &Value::Int(5)), "5 reviews");
    assert_eq!(tr(th, "reviews", &Value::Int(1)), "1 รีวิว");
    assert_eq!(tr(th, "reviews", &Value::Int(5)), "5 รีวิว");
    // {{ i18n "comments" (len .) }}
    assert_eq!(tr(en, "comments", &Value::Int(1)), "One comment");
    assert_eq!(tr(en, "comments", &Value::Int(3)), "3 comments");
    assert_eq!(tr(th, "comments", &Value::Int(3)), "3 ความคิดเห็น");
    // th lacks `notFound`: the default language's text
    assert_eq!(
        t.lookup(th, "notFound", &Args::default()).unwrap(),
        Translation::Fallback("Page not found".into())
    );
    assert_eq!(
        t.lookup(th, "nope", &Args::default()).unwrap(),
        Translation::Missing
    );
}

#[test]
fn the_tera_api_passes_count_and_data_separately() {
    let t = review_site();
    // i18n(key="reviews", count=1)
    let one = Args {
        count: Some(PluralCount::from_int(1)),
        data: None,
    };
    assert_eq!(t.translate(lang(0), "reviews", &one).unwrap(), "1 review");
    // i18n(key="welcome", data={"Name": …}, count=2): the count picks the form only
    let data = map(&[("Name", Value::string("S"))]);
    let args = Args {
        count: Some(PluralCount::from_int(2)),
        data: Some(&data),
    };
    assert_eq!(
        t.translate(lang(0), "welcome", &args).unwrap(),
        "Welcome to S"
    );
}

#[test]
fn placeholders_replace_fallbacks_and_missing_keys() {
    let mut b = TranslationsBuilder::new("en").missing_placeholders(true);
    b.add_file(Path::new("en.toml"), "hello = 'Hello'\n")
        .unwrap();
    b.add_file(Path::new("es.toml"), "bye = 'Adiós'\n").unwrap();
    let t = b.build(["en", "es"]);
    let none = Args::default();
    assert_eq!(t.translate(lang(1), "bye", &none).unwrap(), "Adiós");
    assert_eq!(
        t.translate(lang(1), "hello", &none).unwrap(),
        "[i18n] hello"
    );
    assert_eq!(t.translate(lang(0), "x", &none).unwrap(), "[i18n] x");
}

#[test]
fn unsupported_syntax_is_a_load_error_with_file_and_key() {
    for (src, action) in [
        ("{{ if .Count }}x{{ end }}", "{{ if .Count }}"),
        ("{{ printf \"%d\" .Count }}", "{{ printf \"%d\" .Count }}"),
        ("{{ .Count | upper }}", "{{ .Count | upper }}"),
        ("{{/* note */}}", "{{/* note */}}"),
        ("{{ $x }}", "{{ $x }}"),
        ("a {{ .Count", "{{ .Count"),
    ] {
        let content = format!("[readingTime]\none = 'ok'\nother = '{src}'\n");
        let mut b = TranslationsBuilder::new("en");
        let err = b
            .add_file(Path::new("themes/t/i18n/en.toml"), &content)
            .unwrap_err();
        let I18nError::Message {
            path, key, problem, ..
        } = &err
        else {
            panic!("{src}: {err}");
        };
        assert_eq!(path, Path::new("themes/t/i18n/en.toml"));
        assert_eq!(key, "readingTime");
        let MessageProblem::Syntax { source, .. } = problem else {
            panic!("{src}: {problem}");
        };
        assert_eq!(source.action, action, "{src}");
        let text = err.to_string();
        assert!(
            text.contains("themes/t/i18n/en.toml") && text.contains("readingTime"),
            "{text}"
        );
    }
}

#[test]
fn pieces_and_trim_markers() {
    let t = Template::parse("a {{- .Count -}} b {{ . }}{{.X.Y}}").unwrap();
    assert_eq!(
        t.pieces(),
        [
            Piece::Text("a".into()),
            Piece::Field(vec!["Count".into()]),
            Piece::Text("b ".into()),
            Piece::Dot,
            Piece::Field(vec!["X".into(), "Y".into()]),
        ]
    );
    // `{{-3}}` is not a trim marker in Go either (it needs white space)
    assert!(Template::parse("{{-3}}").is_err());
    let custom = Template::parse_with("<< .Count >> {{ .Count }}", "<<", ">>").unwrap();
    assert_eq!(
        custom
            .render(
                &Value::Int(2),
                PluralCount::from_value(&Value::Int(2)).as_ref()
            )
            .unwrap(),
        "2 {{ .Count }}"
    );
}

#[test]
fn rendering_values() {
    let render = |src: &str, data: &Value| {
        Template::parse(src)
            .unwrap()
            .render(data, PluralCount::from_value(data).as_ref())
    };
    let ctx = |v: Value| map(&[("Context", v)]);
    assert_eq!(
        render("{{ .Context }}", &ctx(Value::Float(0.5))).unwrap(),
        "0.5"
    );
    assert_eq!(
        render("{{ .Context }}", &ctx(Value::Float(2.0))).unwrap(),
        "2"
    );
    assert_eq!(
        render("{{ .Context }}", &ctx(Value::Float(1e6))).unwrap(),
        "1e+06"
    );
    assert_eq!(
        render("{{ .Context }}", &ctx(Value::Float(1.234_567e6))).unwrap(),
        "1.234567e+06"
    );
    assert_eq!(
        render("{{ .Context }}", &ctx(Value::Float(1e-5))).unwrap(),
        "1e-05"
    );
    assert_eq!(
        render("{{ .Context }}", &ctx(Value::Float(0.0001))).unwrap(),
        "0.0001"
    );
    assert_eq!(
        render("{{ .Context }}", &ctx(Value::Float(-123_456.0))).unwrap(),
        "-123456"
    );
    assert_eq!(
        render("{{ .Context }}", &ctx(Value::string("1/3 (Pack) (30 g)"))).unwrap(),
        "1/3 (Pack) (30 g)"
    );
    assert_eq!(
        render("{{ .Context }}", &ctx(Value::Null)).unwrap(),
        NO_VALUE
    );
    assert_eq!(
        render("{{ .Missing }}", &ctx(Value::Int(1))).unwrap(),
        NO_VALUE
    );
    assert_eq!(render("{{ . }}", &Value::Null).unwrap(), NO_VALUE);
    assert_eq!(render("{{ .Count }}", &Value::Int(3)).unwrap(), "3");
    assert_eq!(
        render("{{ .Context }}", &Value::Int(3)),
        Err(EvalError::NoFields {
            field: "Context".into(),
            kind: "number"
        })
    );
    assert!(matches!(
        render("{{ . }}", &ctx(Value::Int(1))),
        Err(EvalError::NotPrintable { .. })
    ));
}

#[test]
fn missing_forms_are_errors() {
    let mut b = TranslationsBuilder::new("en");
    b.add_file(Path::new("en.toml"), "[onlyone]\none = 'just one'\n")
        .unwrap();
    let t = b.build(["en"]);
    let two = Args::from_value(&Value::Int(2));
    assert!(matches!(
        t.translate(lang(0), "onlyone", &two),
        Err(TranslateError::MissingForm { .. })
    ));
    assert_eq!(
        t.translate(lang(0), "onlyone", &Args::from_value(&Value::Int(1)))
            .unwrap(),
        "just one"
    );
}

#[test]
fn plural_counts_follow_go() {
    let c = |v: Value| PluralCount::from_value(&v).map(|c| c.to_string());
    assert_eq!(c(Value::Int(3)), Some("3".into()));
    assert_eq!(c(Value::Float(1.0)), Some("1.0".into()));
    assert_eq!(c(Value::Float(2.5)), Some("2.5".into()));
    assert_eq!(c(Value::string("1.50")), Some("1.50".into()));
    assert_eq!(c(Value::string("1.")), Some("1".into()));
    assert_eq!(c(Value::string("abc")), None);
    assert_eq!(c(Value::string(" 1")), None);
    assert_eq!(c(map(&[("count", Value::Int(4))])), Some("4".into()));
    assert_eq!(c(map(&[("Context", Value::Int(4))])), None);
    assert_eq!(c(Value::Bool(true)), None);
}

#[test]
fn later_files_override_and_parents_are_used() {
    let mut b = TranslationsBuilder::new("en");
    b.add_file(
        Path::new("themes/t/i18n/en.toml"),
        "a = 'theme a'\nb = 'theme b'\n",
    )
    .unwrap();
    b.add_file(Path::new("i18n/en.toml"), "a = 'project a'\n")
        .unwrap();
    b.add_file(Path::new("i18n/pt.toml"), "p = 'pt p'\n")
        .unwrap();
    b.add_file(Path::new("i18n/pt-BR.yaml"), "q: pt-BR q\n")
        .unwrap();
    let t = b.build(["en", "pt-br", "de"]);
    let none = Args::default();
    assert_eq!(t.translate(lang(0), "a", &none).unwrap(), "project a");
    assert_eq!(t.translate(lang(0), "b", &none).unwrap(), "theme b");
    let ptbr = lang(1);
    assert_eq!(
        t.lookup(ptbr, "q", &none).unwrap(),
        Translation::Found("pt-BR q".into())
    );
    assert_eq!(
        t.lookup(ptbr, "p", &none).unwrap(),
        Translation::Found("pt p".into())
    );
    assert_eq!(
        t.lookup(ptbr, "a", &none).unwrap(),
        Translation::Fallback("project a".into())
    );
    // a language without files translates as the default language
    assert_eq!(
        t.lookup(lang(2), "a", &none).unwrap(),
        Translation::Found("project a".into())
    );
}

#[test]
fn shared_across_threads() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<Translations>();
    send_sync::<ssg_locale::Locale>();
    send_sync::<ssg_locale::Collator>();
}
