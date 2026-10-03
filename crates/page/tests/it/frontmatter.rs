//! Oracle: page dates (Go's `HandleDates`) for every page of the reference builds, replayed
//! over 8 `[frontmatter]` variants × 3 time zones × with and without a Git date; and the
//! `[frontmatter]` and `build` option decoding (`oracle/page/frontmatter/*`).

use jiff::{SignedDuration, Timestamp};
use serde_json::{Map as JMap, Value as J, json};
use ssg_base::{Date, Params, Value};
use ssg_config::{DateField, DateSource, decode_front_matter};
use ssg_page::{BuildPolicy, DateResolver, FileCtx, ListMode, RenderMode};

use ssg_testkit::fixture::Tag;

use crate::support::{Tally, family, fixture, parse_time, render_time, s, time_zone, value, zoned};

fn resolver(cfg: &J) -> DateResolver {
    let chain = |k: &str| -> Vec<DateSource> {
        cfg[k]
            .as_array()
            .map(|a| a.iter().map(|v| DateSource::parse(s(v))).collect())
            .unwrap_or_default()
    };
    DateResolver::new(&[
        (DateField::Date, chain("date")),
        (DateField::Lastmod, chain("lastmod")),
        (DateField::PublishDate, chain("publishDate")),
        (DateField::ExpiryDate, chain("expiryDate")),
    ])
}

/// A front matter value as the oracle writes it.
fn to_json(v: &Value) -> J {
    match v {
        Value::Null => J::Null,
        Value::Bool(b) => json!(b),
        Value::Int(i) => json!(i),
        Value::Float(f) => json!(f),
        Value::String(s) => json!(&**s),
        Value::Date(Date::Zoned(z)) => json!({ "$nh:time": render_time(Some(z)) }),
        Value::Date(Date::Local(dt)) => json!({ "$nh:local": dt.to_string() }),
        Value::Array(a) => J::Array(a.iter().map(to_json).collect()),
        Value::Map(m) => J::Object(m.iter().map(|(k, v)| (k.to_owned(), to_json(v))).collect()),
    }
}

struct Run {
    out: J,
    has_errors: bool,
}

/// Resolves the dates of the recorded input with `cfg` in `zone`; `git` replaces the recorded
/// Git date (replays use the modification time plus 36 hours).
fn run(before: &J, cfg: &DateResolver, zone: &str, git: Option<bool>) -> Run {
    let Value::Map(m) = value(&before["params"]) else {
        panic!("params: {before}");
    };
    let mut params = Params::fold(&m);
    let before_json = to_json(&Value::Map(m));
    let mod_time = zoned(&before["modTime"]).map(|z| z.timestamp());
    let git_date: Option<Timestamp> = match git {
        None => zoned(&before["gitAuthorDate"]).map(|z| z.timestamp()),
        Some(false) => None,
        // The oracle's synthetic Git date is the modification time plus 36 hours, also for
        // pages without a file (Go's zero time plus 36 hours).
        Some(true) => {
            let Some(Tag::Time(t)) = Tag::of(&before["modTime"]) else {
                panic!("modTime: {before}");
            };
            Some(parse_time(t).timestamp() + SignedDuration::from_hours(36))
        }
    };
    let file = FileCtx {
        base_filename: s(&before["baseFilename"]),
        mod_time,
        git_author_date: git_date,
    };
    let got = cfg.resolve(&mut params, Some(&file), &time_zone(zone));
    let mut changed = JMap::new();
    for (k, v) in params.iter() {
        let v = to_json(v);
        if before_json.get(k) != Some(&v) {
            changed.insert(k.to_owned(), v);
        }
    }
    let date = |f: DateField| json!({ "$nh:time": render_time(got.dates.get(f)) });
    let slug = got.slug.unwrap_or_else(|| s(&before["slug"]).to_owned());
    Run {
        out: json!({
            "dates": {
                "date": date(DateField::Date),
                "lastmod": date(DateField::Lastmod),
                "publishDate": date(DateField::PublishDate),
                "expiryDate": date(DateField::ExpiryDate),
            },
            "params": changed,
            "slug": slug,
        }),
        has_errors: !got.unparsable.is_empty(),
    }
}

/// Unix seconds (`date: 1136214245`) are UTC here; Go places them in the process's
/// local zone, which the oracle ran as a nameless zero offset (`+00:00`).
fn only_local_zone_differs(got: &J, want: &J) -> bool {
    let utc = want.to_string().replace("+00:00\"", "Z\"");
    serde_json::from_str::<J>(&utc).ok().as_ref() == Some(got)
}

fn judge(t: &mut Tally, got: &J, want: &J, errors_ok: bool, detail: impl FnOnce() -> String) {
    if got == want && errors_ok {
        t.pass();
    } else if errors_ok && only_local_zone_differs(got, want) {
        t.accept("unix-seconds-in-utc");
    } else {
        t.fail(detail);
    }
}

#[test]
fn dates_match_go() {
    let mut t = Tally::default();
    let mut replays = 0;
    for file in family("frontmatter", &["decode.json.gz"]) {
        let fx = fixture(&format!("frontmatter/{file}"));
        let cfgs: Vec<DateResolver> = fx["configs"]
            .as_array()
            .expect("configs")
            .iter()
            .map(resolver)
            .collect();
        let zones: Vec<&str> = fx["locations"]
            .as_array()
            .expect("locations")
            .iter()
            .map(s)
            .collect();
        for c in fx["cases"].as_array().expect("cases") {
            let before = &c["before"];
            let got = run(before, &resolver(&c["cfg"]), s(&before["location"]), None);
            let name = s(&before["pathOrTitle"]);
            judge(&mut t, &got.out, &c["after"], true, || {
                format!("{file} {name}: got {}\n      want {}", got.out, c["after"])
            });
            for v in c["variants"].as_array().into_iter().flatten() {
                replays += 1;
                let cfg = &cfgs[crate::support::idx(&v["cfg"])];
                let zone = zones[crate::support::idx(&v["loc"])];
                let got = run(before, cfg, zone, v["git"].as_bool());
                let mut want = v["out"].clone();
                let want_errors = !s(&want["errors"]).is_empty();
                want.as_object_mut().expect("out").remove("errors");
                judge(
                    &mut t,
                    &got.out,
                    &want,
                    got.has_errors == want_errors,
                    || {
                        format!(
                            "{file} {name} cfg {} {zone} git {}: got {} (errors {})\n      want {} (errors {want_errors})",
                            v["cfg"], v["git"], got.out, got.has_errors, want
                        )
                    },
                );
            }
        }
    }
    assert!(replays > 13_000, "{replays} replays");
    t.finish("frontmatter-dates");
}

/// `decode_front_matter` gives the date sources Go decodes from `[frontmatter]` (defaults,
/// `:default`, aliases, case, duplicates, scalars and empty lists).
#[test]
fn front_matter_config_decodes_like_go() {
    let fx = fixture("frontmatter/decode.json.gz");
    let mut t = Tally::default();
    for c in fx["cases"].as_array().expect("cases") {
        if c["fn"] != "frontmatter" {
            continue;
        }
        let config = match value(&c["in"]) {
            Value::Map(m) => (*m).clone(),
            Value::Null => ssg_base::Map::new(),
            other => panic!("not a table: {other:?}"),
        };
        let got: JMap<String, J> = decode_front_matter(&config)
            .into_iter()
            .map(|(field, sources)| {
                let name = match field {
                    DateField::Date => "date",
                    DateField::Lastmod => "lastmod",
                    DateField::PublishDate => "publishDate",
                    DateField::ExpiryDate => "expiryDate",
                };
                // Go's empty list is `nil`.
                let list = if sources.is_empty() {
                    J::Null
                } else {
                    sources.iter().map(|s| json!(s.as_config_str())).collect()
                };
                (name.to_owned(), list)
            })
            .collect();
        let got = J::Object(got);
        let want = &c["want"]["ok"];
        t.check(&got == want, || {
            format!("{}: got {got}, want {want}", c["in"])
        });
    }
    assert!(t.total >= 10, "{} cases", t.total);
    t.finish("frontmatter-config");
}

#[test]
fn build_options_match_go() {
    let fx = fixture("frontmatter/decode.json.gz");
    let mut t = Tally::default();
    for c in fx["cases"].as_array().expect("cases") {
        if c["fn"] != "build" {
            continue;
        }
        let want = &c["want"];
        let got = BuildPolicy::decode(&value(&c["in"]));
        let ok = match got {
            Err(_) => want.get("err").is_some(),
            Ok(b) => {
                let list = match b.list {
                    ListMode::Always => "always",
                    ListMode::Never => "never",
                    ListMode::Local => "local",
                };
                let render = match b.render {
                    RenderMode::Always => "always",
                    RenderMode::Never => "never",
                    RenderMode::Link => "link",
                };
                want.get("err").is_none()
                    && want["list"] == list
                    && want["render"] == render
                    && want["publishResources"] == b.publish_resources
            }
        };
        t.check(ok, || format!("build {}: want {want}", c["in"]));
    }
    t.finish("build-options");
}
