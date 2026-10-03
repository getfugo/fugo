//! The `allconfig/load` oracle: site trees recreated on disk, loaded, and compared with Go's
//! decoded configuration (per-language `config` command dumps, compiled values, language list).
//!
//! Required at 100%: `repo/docs`, `repo/docs-development` and `repo/testsite`. The other groups are run for coverage; their
//! differences must be listed in `expected_diffs.toml` with a reason.

use std::collections::BTreeMap;

use serde_json::Value as J;
use ssg_config::Config;

use crate::support::{Site, Tally, dump, fixture, materialize, strip};

#[derive(serde::Deserialize)]
struct ExpectedDiffs {
    #[serde(default)]
    load: BTreeMap<String, String>,
}

fn expected_diffs() -> BTreeMap<String, String> {
    let path = ssg_testkit::fixture::repo_dir().join("crates/config/expected_diffs.toml");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let d: ExpectedDiffs = toml::from_str(&text).expect("expected_diffs.toml");
    d.load
}

/// Compares one loaded case with the oracle result; returns the differences.
fn compare(site: &Site, c: &Config, want: &J, tally: &mut Tally, case: &str) {
    let langs: Vec<&str> = c.sites.iter().map(|s| s.language.key.as_str()).collect();
    let want_langs: Vec<&str> = want["languagesDefaultFirst"]
        .as_array()
        .expect("languages")
        .iter()
        .filter_map(J::as_str)
        .collect();
    tally.check(langs == want_langs, || {
        format!("{case}: languages {langs:?}, want {want_langs:?}")
    });
    tally.check(Some(c.multihost) == want["isMultihost"].as_bool(), || {
        format!("{case}: multihost {}", c.multihost)
    });
    let timeout = want["base"]["compiled"]["timeout"].as_f64().unwrap_or(0.0);
    tally.check((c.timeout.as_nanos() as f64 - timeout).abs() < 1.0, || {
        format!("{case}: timeout {:?}, want {timeout}ns", c.timeout)
    });

    // Compiled cache directories.
    for (name, w) in want["base"]["cachesCompiled"]
        .as_object()
        .into_iter()
        .flatten()
    {
        let mine = c.caches.get(name).expect("cache");
        let dir = site.expand(w["dirCompiled"].as_str().unwrap_or_default());
        let want_path = if w["isResourceDir"].as_bool() == Some(true) {
            c.project_dir.join(&c.dirs.resources).join(&dir)
        } else {
            dir.into()
        };
        tally.check(
            mine.path == want_path
                && mine.in_resource_dir == w["isResourceDir"].as_bool().unwrap_or(false),
            || format!("{case}: cache {name}: {:?}, want {want_path:?}", mine.path),
        );
    }

    for s in &c.sites {
        let key = &s.language.key;
        let Some(cl) = want["configLangs"]
            .as_array()
            .expect("configLangs")
            .iter()
            .find(|l| l["lang"].as_str() == Some(key))
        else {
            tally.check(false, || {
                format!("{case} [{key}]: not a language of the oracle")
            });
            continue;
        };
        let lang = &cl["language"];
        let facts = [
            (
                "baseURL",
                J::from(s.base_url.as_str()),
                cl["baseURL"].clone(),
            ),
            (
                "languagePrefix",
                J::from(s.language.url_prefix.as_str()),
                cl["languagePrefix"].clone(),
            ),
            (
                "languageName",
                J::from(s.language.name.as_str()),
                lang["languageName"].clone(),
            ),
            ("weight", J::from(s.language.weight), lang["weight"].clone()),
            (
                "title",
                J::from(s.language.title.as_str()),
                lang["title"].clone(),
            ),
            (
                "location",
                J::from(s.language.time_zone.iana_name().unwrap_or("UTC")),
                lang["location"].clone(),
            ),
            (
                "rtl",
                J::from(s.language.direction == ssg_config::Direction::Rtl),
                J::from(lang["languageDirection"].as_str() == Some("rtl")),
            ),
        ];
        for (what, mine, theirs) in facts {
            tally.check(strip(&mine) == strip(&theirs), || {
                format!("{case} [{key}] {what}: {mine}, want {theirs}")
            });
        }

        let compiled = &want["languageConfigs"][key.as_str()]["compiled"];
        // Kind outputs (content kinds; disabled kinds have none).
        let mut outputs = BTreeMap::new();
        for (kind, ids) in s.outputs.iter() {
            if kind.is_content() && !ids.is_empty() {
                let names: Vec<&str> = ids
                    .iter()
                    .map(|&id| c.output_formats.get(id).name.as_str())
                    .collect();
                outputs.insert(kind.as_str().to_owned(), names);
            }
        }
        let mut want_outputs = compiled["kindOutputFormats"].clone();
        if let Some(o) = want_outputs.as_object_mut() {
            o.remove("rss");
            o.retain(|_, v| v.as_array().is_some_and(|a| !a.is_empty()));
        }
        let mine_outputs = serde_json::to_value(&outputs).expect("json");
        tally.check(strip(&mine_outputs) == strip(&want_outputs), || {
            format!("{case} [{key}] outputs: {mine_outputs}, want {want_outputs}")
        });
        let disabled: Vec<String> = s
            .disable_kinds
            .iter()
            .map(|k| format!("{}=true", k.as_str()))
            .collect();
        let want_disabled: Vec<String> = compiled["disabledKinds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(J::as_str)
            .filter(|k| *k != "rss=true")
            .map(str::to_owned)
            .collect();
        let (mut a, mut b) = (disabled.clone(), want_disabled.clone());
        a.sort();
        b.sort();
        tally.check(a == b, || {
            format!("{case} [{key}] disabledKinds: {a:?}, want {b:?}")
        });

        let Some(text) = want["configDumps"][format!("{key}|false")].as_str() else {
            continue;
        };
        let mut theirs: J = serde_json::from_str(&site.expand(text)).expect("dump");
        if let Some(cd) = theirs.get_mut("cachedir")
            && let Some(s) = cd.as_str()
        {
            *cd = J::from(s.trim_end_matches('/'));
        }
        if let Some(sd) = theirs.get_mut("staticdir")
            && let Some(a) = sd.as_array_mut()
        {
            let mut seen = Vec::new();
            a.retain(|x| {
                let keep = !seen.contains(x);
                seen.push(x.clone());
                keep
            });
        }
        if let Some(m) = theirs.get_mut("minify").and_then(J::as_object_mut) {
            m.remove("tdewolff");
        }
        // The Go build's stats file (`[build.buildStats]`) is not written here.
        if let Some(b) = theirs.get_mut("build").and_then(J::as_object_mut) {
            b.remove("buildstats");
        }
        // The deprecated `enableDefault` is compared through `useEmbedded`.
        for hook in ["image", "link"] {
            if let Some(h) = theirs.pointer_mut(&format!("/markup/goldmark/renderhooks/{hook}"))
                && let Some(h) = h.as_object_mut()
            {
                h.remove("enabledefault");
            }
        }
        for (k, mut mine) in dump(c, s) {
            let theirs_k = theirs.get(&k).cloned().unwrap_or(J::Null);
            if k == "mediatypes" {
                // Go's dump shows a configured media type as written: without `delimiter`
                // when the configuration has none (the decoded delimiter is `.`).
                for (name, entry) in theirs_k.as_object().into_iter().flatten() {
                    if entry.get("delimiter").is_none()
                        && let Some(e) = mine.get_mut(name).and_then(J::as_object_mut)
                        && e.get("delimiter") == Some(&J::from("."))
                    {
                        e.remove("delimiter");
                    }
                }
            }
            let (m, t) = (strip(&mine), strip(&theirs_k));
            tally.check(m == t, || {
                format!(
                    "{case} [{key}] {k}:\n    got  {}\n    want {}",
                    m.unwrap_or(J::Null),
                    t.unwrap_or(J::Null)
                )
            });
        }
    }
}

/// Cases whose typed configuration is also kept as an insta snapshot.
const SNAPSHOT_CASES: &[&str] = &["repo/docs", "repo/testsite"];

struct GroupResult {
    cases: usize,
    not_applicable: Vec<String>,
    tally: Tally,
}

fn run_group(name: &str) -> GroupResult {
    let fx = fixture(&format!("oracle/allconfig/load/{name}.json.gz"));
    let site_dir = "site";
    let mut out = GroupResult {
        cases: 0,
        not_applicable: Vec::new(),
        tally: Tally::default(),
    };
    for row in fx["cases"].as_array().expect("cases") {
        let case = &row["case"];
        let case_name = case["name"].as_str().expect("name");
        out.cases += 1;
        let site = match materialize(case, site_dir) {
            Ok(s) => s,
            Err(why) => {
                out.not_applicable.push(format!("{case_name}: {why}"));
                continue;
            }
        };
        let result = ssg_config::load(&site.options);
        let want = &row["result"];
        let want_err = want.get("err").and_then(J::as_str);
        match (&result, want_err) {
            (Err(_), Some(_)) => out.tally.check(true, String::new),
            (Ok(_), Some(e)) => out
                .tally
                .check(false, || format!("{case_name}: loaded, want error {e:?}")),
            (Err(e), None) => out.tally.check(false, || format!("{case_name}: error {e}")),
            (Ok(c), None) => {
                compare(&site, c, want, &mut out.tally, case_name);
                if SNAPSHOT_CASES.contains(&case_name) {
                    let name = case_name.replace('/', "-");
                    ssg_testkit::snapshot::settings().bind(|| {
                        insta::assert_yaml_snapshot!(name, crate::sites::snapshot(c));
                    });
                }
            }
        }
    }
    out
}

/// Runs the groups; failures not listed in `expected_diffs.toml` fail the test. Keys of the
/// list are `<case>` (every difference of that case) or the difference's first line.
fn check_groups(groups: &[&str], strict: bool) {
    let accepted = expected_diffs();
    let mut unexpected = Vec::new();
    for g in groups {
        let r = run_group(g);
        let mut deviations = 0;
        for f in &r.tally.failures {
            let first = f.lines().next().unwrap_or_default();
            let case = first.split([':', ' ']).next().unwrap_or_default();
            let listed = !strict && (accepted.contains_key(case) || accepted.contains_key(first));
            if listed {
                deviations += 1;
            } else {
                unexpected.push(f.clone());
            }
        }
        eprintln!(
            "allconfig/load/{g}: {} cases, {} checks, {} passed, {} accepted deviations, {} not applicable",
            r.cases,
            r.tally.checks,
            r.tally.passed,
            deviations,
            r.not_applicable.len()
        );
        for n in &r.not_applicable {
            eprintln!("  not applicable: {n}");
        }
    }
    assert!(
        unexpected.is_empty(),
        "{} unexpected difference(s):\n{}",
        unexpected.len(),
        unexpected.join("\n")
    );
}

/// The acceptance sites: the docs and testsite projects.
#[test]
fn acceptance_sites() {
    check_groups(&["repo"], true);
}

#[test]
fn other_groups() {
    check_groups(
        &[
            "basic",
            "configdir",
            "languages",
            "merge",
            "mounts",
            "sections",
            "themes",
        ],
        false,
    );
}
