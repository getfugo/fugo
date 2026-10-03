//! Localized names, date styles and numbers against the Go implementation's locales library
//! (`oracle/common/locales`, en and th). Fields that differ are listed in `expected_diffs.toml`
//! (newer CLDR in ICU4X).

use std::collections::BTreeMap;

use pretty_assertions::assert_eq;
use serde::Deserialize;
use serde_json::Value as J;
use ssg_locale::{DatePattern, DateStyle, Locale, NameWidth, format_date, format_number};

use crate::common::expected_diffs;

#[derive(Deserialize)]
struct Fixture {
    zones: Vec<Zone>,
    cases: Vec<J>,
}

#[derive(Deserialize)]
struct Zone {
    name: String,
    kind: String,
    offset: Option<i32>,
    tzdata: Option<String>,
}

fn fixture() -> Fixture {
    ssg_testkit::fixture::oracle("oracle/common/locales/locales.json")
}

fn time_zone(z: &Zone) -> jiff::tz::TimeZone {
    match z.kind.as_str() {
        "utc" => jiff::tz::TimeZone::UTC,
        "fixed" => {
            jiff::tz::TimeZone::fixed(jiff::tz::Offset::from_seconds(z.offset.unwrap()).unwrap())
        }
        _ => {
            let hex = z.tzdata.as_deref().unwrap();
            let bytes: Vec<u8> = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                .collect();
            jiff::tz::TimeZone::tzif(&z.name, &bytes).unwrap()
        }
    }
}

const WEEKDAYS: [jiff::civil::Weekday; 7] = {
    use jiff::civil::Weekday::*;
    [
        Sunday, Monday, Tuesday, Wednesday, Thursday, Friday, Saturday,
    ]
};

/// Gregorian month and weekday names, `th` included (Thai sites built by the Go
/// implementation print them with Gregorian years).
#[test]
fn month_and_weekday_names() {
    let listed = expected_diffs().locales.fields;
    for case in fixture().cases.iter().filter(|c| c["kind"] == "names") {
        let lang = case["locale"].as_str().unwrap();
        let locale = Locale::new(lang);
        let names = |field: &str| -> Vec<String> {
            case[field]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect()
        };
        let months = |w| {
            (1..=12)
                .map(|m| locale.month_name(m, w).to_owned())
                .collect::<Vec<_>>()
        };
        let weekdays = |w| {
            WEEKDAYS
                .iter()
                .map(|d| locale.weekday_name(*d, w).to_owned())
                .collect::<Vec<_>>()
        };
        for (field, ours) in [
            ("MonthWide", months(NameWidth::Wide)),
            ("MonthAbbreviated", months(NameWidth::Abbreviated)),
            ("WeekdayWide", weekdays(NameWidth::Wide)),
            ("WeekdayAbbreviated", weekdays(NameWidth::Abbreviated)),
        ] {
            let key = format!("{lang} {field}");
            if listed.contains_key(&key) {
                assert_ne!(ours, names(field), "{key} is listed but agrees");
            } else {
                assert_eq!(ours, names(field), "{key}");
            }
        }
    }
}

#[test]
fn date_styles() {
    let listed = expected_diffs().locales.fields;
    let fixture = fixture();
    let zones: Vec<_> = fixture.zones.iter().map(time_zone).collect();
    let mut tally: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut failures = Vec::new();
    for case in fixture.cases.iter().filter(|c| c["kind"] == "time") {
        let ts = jiff::Timestamp::new(
            case["unix"].as_i64().unwrap(),
            i32::try_from(case["nsec"].as_i64().unwrap()).unwrap(),
        );
        let Ok(ts) = ts else { continue };
        let zone = &zones[usize::try_from(case["zone"].as_u64().unwrap()).unwrap()];
        let zoned = ts.to_zoned(zone.clone());
        // The Go locales library misprints years after 9999, BC years, and the short style's
        // two-digit year before 1000; early AD years (Go's zero time is year 1) are compared
        // in the other styles
        if !(1..=9999).contains(&zoned.year()) {
            continue;
        }
        let early = zoned.year() < 1000;
        for lang in ["en", "th"] {
            let locale = Locale::new(lang);
            for (style, field) in [
                (DateStyle::Short, "date_short"),
                (DateStyle::Medium, "date_medium"),
                (DateStyle::Long, "date_long"),
                (DateStyle::Full, "date_full"),
            ] {
                if early && style == DateStyle::Short {
                    continue;
                }
                let want = case[lang][field].as_str().unwrap();
                let got = format_date(&zoned, DatePattern::Style(style), &locale).unwrap();
                let key = format!("{lang} {field}");
                let t = tally.entry(key.clone()).or_default();
                t.1 += 1;
                if got == want {
                    t.0 += 1;
                } else if !listed.contains_key(&key) {
                    failures.push(format!("{key} {zoned}: {got:?} vs Go {want:?}"));
                }
            }
        }
    }
    println!("date styles (agree/compared): {tally:?}");
    failures.truncate(20);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    for key in listed.keys().filter(|k| k.contains(" date_")) {
        let (agree, total) = tally[key];
        assert!(agree < total, "{key} is listed but agrees");
    }
}

#[test]
fn numbers() {
    let listed = expected_diffs().locales.fields;
    let mut tally: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    let mut failures = Vec::new();
    for case in fixture().cases.iter().filter(|c| c["kind"] == "number") {
        let bits = u64::from_str_radix(case["n"].as_str().unwrap(), 16).unwrap();
        let n = f64::from_bits(bits);
        // The Go locales library prints infinities as `+,Inf`; this port does not format them
        if !n.is_finite() {
            continue;
        }
        let Ok(precision) = u8::try_from(case["v"].as_i64().unwrap()) else {
            continue;
        };
        for lang in ["en", "th"] {
            let Some(want) = case[lang]["number"].as_str() else {
                continue;
            };
            let got = format_number(n, precision, &Locale::new(lang));
            let t = tally.entry(lang).or_default();
            t.1 += 1;
            if got == want {
                t.0 += 1;
            } else if !listed.contains_key(&format!("{lang} number")) {
                failures.push(format!("{lang} {n:?} .{precision}: {got:?} vs Go {want:?}"));
            }
        }
    }
    println!("numbers (agree/compared): {tally:?}");
    failures.truncate(30);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn strftime_uses_localized_names() {
    let d: jiff::Zoned = "2020-08-14T14:59:12+07:00[Asia/Bangkok]".parse().unwrap();
    let f = |lang: &str, p: &str| {
        format_date(&d, DatePattern::Strftime(p), &Locale::new(lang)).unwrap()
    };
    assert_eq!(f("en", "%b %-d, %Y"), "Aug 14, 2020");
    assert_eq!(f("th", "%b %-d, %Y"), "ส.ค. 14, 2020");
    assert_eq!(f("th", "%-d %B %Y"), "14 สิงหาคม 2020");
    assert_eq!(f("th", "%A"), "วันศุกร์");
    assert_eq!(f("en", "%^a %% %H:%M %:z"), "FRI % 14:59 +07:00");
    assert_eq!(f("fr", "%A %-d %B %Y"), "vendredi 14 août 2020");
}
