//! `time::parse_date` against the date layouts of the `common/cast` oracle.

use jiff::tz::{Offset, TimeZone};
use serde_json::Value as J;
use ssg_base::time::parse_date;

use crate::support::{Tally, fixture};

fn zones(f: &J) -> Vec<TimeZone> {
    f["zones"]
        .as_array()
        .unwrap()
        .iter()
        .map(|z| match z["kind"].as_str().unwrap() {
            "utc" => TimeZone::UTC,
            "fixed" => TimeZone::fixed(
                Offset::from_seconds(i32::try_from(z["offset"].as_i64().unwrap()).unwrap())
                    .unwrap(),
            ),
            "tzdata" => {
                let hex = z["tzdata"].as_str().unwrap();
                let bytes: Vec<u8> = (0..hex.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                    .collect();
                TimeZone::tzif(z["name"].as_str().unwrap(), &bytes).unwrap()
            }
            k => panic!("zone kind {k}"),
        })
        .collect()
}

/// `(unix seconds, nanoseconds, UTC offset in seconds)` of an oracle time; the offset is read
/// from Go's `String()` form (`2006-01-02 15:04:05.999999999 -0700 MST`).
fn instant(v: &J) -> (i64, i64, i32) {
    let s = v["str"].as_str().unwrap();
    let off = s
        .split(' ')
        .find(|t| t.len() == 5 && (t.starts_with('+') || t.starts_with('-')))
        .unwrap();
    let sign = if off.starts_with('-') { -1 } else { 1 };
    let hh: i32 = off[1..3].parse().unwrap();
    let mm: i32 = off[3..5].parse().unwrap();
    (
        v["unix"].as_i64().unwrap(),
        v["nsec"].as_i64().unwrap(),
        sign * (hh * 3600 + mm * 60),
    )
}

#[test]
fn cast_date_layouts_oracle() {
    let f = fixture("oracle/common/cast/cast.json");
    let zones = zones(&f);
    let mut t = Tally::new("cast/dates");
    for c in f["cases"].as_array().unwrap() {
        let Some(s) = c["in"].as_str() else {
            // Numbers, times and durations are converted by the value layer, not parsed.
            t.skip(|| format!("{}: not a string", c["in"]));
            continue;
        };
        for (field, tz) in [
            ("ToTimeE", &zones[0]),
            ("ToTimeInDefaultLocationE:1", &zones[1]),
            ("ToTimeInDefaultLocationE:3", &zones[3]),
        ] {
            let want = &c[field];
            if want["err"]
                .as_str()
                .is_some_and(|e| e.starts_with("unable to cast"))
            {
                t.skip(|| format!("{s:?}: a typed Go string (template.HTML, …)"));
                continue;
            }
            let got = parse_date(s, tz);
            match (want.get("ok"), &got) {
                (Some(w), Ok(z)) => {
                    let (unix, nsec, off) = instant(w);
                    // Go's convention: floored seconds, non-negative nanoseconds; Go prints
                    // the offset in whole minutes (LMT -4:56:02 is "-0456").
                    let ns = z.timestamp().as_nanosecond();
                    let ok = ns.div_euclid(1_000_000_000) == i128::from(unix)
                        && ns.rem_euclid(1_000_000_000) == i128::from(nsec)
                        && z.offset().seconds() / 60 == off / 60;
                    t.check(ok, || format!("{field}({s:?}): want {}, got {z}", w["str"]));
                }
                (None, Err(_)) => t.check(true, String::new),
                (Some(w), Err(_))
                    if w["unix"].as_i64().unwrap() > jiff::Timestamp::MAX.as_second() =>
                {
                    t.deviation(|| format!("{field}({s:?}): after jiff's last instant"));
                }
                _ => t.check(false, || {
                    format!("{field}({s:?}): want {want}, got {got:?}")
                }),
            }
        }
    }
    t.finish();
}

#[test]
fn date_examples() {
    let bkk = TimeZone::fixed(Offset::from_seconds(7 * 3600).unwrap());
    let d = parse_date("2024-07-14", &bkk).unwrap();
    assert_eq!(d.to_string(), "2024-07-14T00:00:00+07:00[+07:00]");
    let d = parse_date("2024-07-14T17:31:59.5Z", &bkk).unwrap();
    assert_eq!(d.offset().seconds(), 0);
    assert_eq!(d.timestamp().subsec_nanosecond(), 500_000_000);
    let d = parse_date("Sun, 14 Jul 2024 17:31:59 ICT", &TimeZone::UTC).unwrap();
    assert_eq!(d.offset().seconds(), 0);
    assert!(parse_date("2024-02-30", &bkk).is_err());
    assert!(parse_date("not a date", &bkk).is_err());
}
