//! EXIF metadata against the `nh-images/exif` oracle (the capture date and the GPS position;
//! tag names are reported, values are not compared: README, deviations).

use std::collections::BTreeSet;

use serde_json::{Value as J, json};
use ssg_base::Date;
use ssg_config::ImagingConfig;
use ssg_images::{ExifSettings, Imaging, exif};
use ssg_testkit::fixture::{oracle, repo_file};

use crate::common::source_path;

fn settings(name: &str) -> ExifSettings {
    let exif = match name {
        "default" => json!({}),
        "none" => json!({"excludeFields": ".*"}),
        "all" => json!({"includeFields": ".*"}),
        "nodate" => json!({"includeFields": ".*", "disableDate": true, "disableLatLong": true}),
        "incl" => json!({"includeFields": "Orientation|Date|GPS|Model", "excludeFields": "Offset"}),
        other => panic!("config {other}"),
    };
    let cfg: ImagingConfig =
        serde_json::from_value(json!({ "exif": exif })).expect("imaging config");
    Imaging::from_config(&cfg).expect("imaging").exif
}

fn date_string(d: &Date) -> String {
    match d {
        Date::Local(dt) => format!("{}Z", dt.strftime("%Y-%m-%dT%H:%M:%S")),
        Date::Zoned(z) => format!(
            "{}Z",
            z.with_time_zone(jiff::tz::TimeZone::UTC)
                .strftime("%Y-%m-%dT%H:%M:%S")
        ),
    }
}

fn go_float(hex: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(hex, 16).expect("float bits"))
}

fn base64(s: &str) -> Vec<u8> {
    let val = |c: u8| -> u32 {
        match c {
            b'A'..=b'Z' => u32::from(c - b'A'),
            b'a'..=b'z' => u32::from(c - b'a') + 26,
            b'0'..=b'9' => u32::from(c - b'0') + 52,
            b'+' => 62,
            b'/' => 63,
            _ => panic!("base64 {c}"),
        }
    };
    let digits: Vec<u8> = s.bytes().filter(|&c| c != b'=').collect();
    let mut out = Vec::new();
    for chunk in digits.chunks(4) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, &c)| n | val(c) << (18 - 6 * i));
        out.extend_from_slice(&n.to_be_bytes()[1..chunk.len()]);
    }
    out
}

#[test]
fn dates_and_positions_match_the_exif_oracle() {
    let doc: J = oracle("oracle/images/exif/exif.json.gz");
    let configs: Vec<(&str, ExifSettings)> = ["default", "none", "all", "nodate", "incl"]
        .into_iter()
        .map(|n| (n, settings(n)))
        .collect();
    let (mut compared, mut matched, mut with_data) = (0, 0, 0);
    let (mut tag_total, mut tag_found) = (0usize, 0usize);
    let mut failures = Vec::new();
    let mut missing = std::collections::BTreeMap::new();
    let mut unavailable = Vec::new();
    for c in doc["cases"].as_array().expect("cases") {
        let src = c["src"].as_str().expect("src");
        let bytes = match c.get("b") {
            Some(b) => base64(b.as_str().expect("b")),
            None => match source_path(src) {
                Some(p) => std::fs::read(p).expect("read"),
                None => {
                    unavailable.push(src);
                    continue;
                }
            },
        };
        for (name, s) in &configs {
            let want = &c["res"][*name];
            if want.get("date").is_none() {
                // Go failed on this input (a broken EXIF block).
                continue;
            }
            compared += 1;
            let got = exif::read(&bytes, s);
            let want_date = want["date"].as_str().expect("date");
            let got_date = got
                .as_ref()
                .and_then(|e| e.date.as_ref())
                .map_or_else(|| "0001-01-01T00:00:00Z".to_owned(), date_string);
            let want_pos = (
                go_float(want["lat"].as_str().expect("lat")),
                go_float(want["long"].as_str().expect("long")),
            );
            let got_pos = got.as_ref().and_then(|e| e.lat_long).unwrap_or((0.0, 0.0));
            let close = |a: f64, b: f64| (a - b).abs() <= 1e-9 * a.abs().max(1.0);
            if want_date != "0001-01-01T00:00:00Z" || want_pos != (0.0, 0.0) {
                with_data += 1;
            }
            if got_date == want_date && close(got_pos.0, want_pos.0) && close(got_pos.1, want_pos.1)
            {
                matched += 1;
            } else {
                failures.push(format!(
                    "{src} [{name}]: got {got_date} {got_pos:?}, want {want_date} {want_pos:?}"
                ));
            }
            if *name == "all"
                && let Some(tags) = want["tags"].as_array()
            {
                let ours: BTreeSet<&str> = got
                    .as_ref()
                    .map(|e| e.tags.keys().map(String::as_str).collect())
                    .unwrap_or_default();
                for t in tags {
                    tag_total += 1;
                    let tag = t[0].as_str().expect("tag name");
                    if ours.contains(tag) {
                        tag_found += 1;
                    } else if src.starts_with("file:") {
                        *missing.entry(tag.to_owned()).or_insert(0usize) += 1;
                    }
                }
            }
        }
    }
    eprintln!(
        "exif oracle: {matched}/{compared} date+position match ({with_data} with data); \
         tag names found: {tag_found}/{tag_total}; not found in image files: {missing:?}"
    );
    let allowed = crate::common::expected_diffs("exif");
    let unexpected: Vec<&String> = failures
        .iter()
        .filter(|f| !allowed.keys().any(|k| f.starts_with(k.as_str())))
        .collect();
    assert!(
        unexpected.is_empty(),
        "{} unexpected mismatches:\n{}",
        unexpected.len(),
        unexpected
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        unavailable.is_empty(),
        "oracle sources not found: {unavailable:?}"
    );
    assert!(compared >= 2_335, "only {compared} cases compared");
    assert!(
        with_data > 50,
        "only {with_data} cases with a date or position"
    );
}

#[test]
fn orientation_is_read() {
    let bytes = std::fs::read(repo_file("resources/testdata/exif/orientation6.jpg")).expect("read");
    assert_eq!(exif::orientation(&bytes), Some(6));
    let e = exif::read(&bytes, &ExifSettings::default()).expect("exif");
    // The default settings exclude GPS and Exif* tags but keep Orientation.
    assert!(e.tags.contains_key("Orientation"), "{:?}", e.tags.keys());
    assert!(!e.tags.keys().any(|k| k.starts_with("GPS")));
}
