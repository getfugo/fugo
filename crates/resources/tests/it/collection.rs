//! `Get`, `GetMatch`, `Match` and `ByType` over resource lists, against the Go oracle of
//! `resource.Resources` (`tests/it/data/collection.json`, extracted from the old port's
//! `nh-resource` fixture: 445 cases over 5 resource sets, incl. names with spaces, Thai and
//! accented letters, `./` names and invalid globs).

use serde_json::{Value as J, json};
use ssg_resources::meta::{self, Named};

struct TestRes {
    name: String,
    normalized: Option<String>,
    typ: String,
}

impl Named for TestRes {
    fn name(&self) -> &str {
        &self.name
    }

    fn name_normalized(&self) -> Option<&str> {
        self.normalized.as_deref()
    }

    fn resource_type(&self) -> &str {
        &self.typ
    }
}

fn names(rs: &[&TestRes]) -> J {
    if rs.is_empty() {
        J::Null
    } else {
        json!(rs.iter().map(|r| r.name.as_str()).collect::<Vec<_>>())
    }
}

#[test]
fn get_match_by_type() {
    let fx: J = serde_json::from_str(include_str!("data/collection.json")).unwrap();
    let sets: Vec<Vec<TestRes>> = fx["sets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            s.as_array()
                .unwrap()
                .iter()
                .map(|r| TestRes {
                    name: r["name"].as_str().unwrap().to_owned(),
                    normalized: r["normalized"].as_str().map(str::to_owned),
                    typ: r["type"].as_str().unwrap().to_owned(),
                })
                .collect()
        })
        .collect();
    let mut failures = Vec::new();
    let cases = fx["cases"].as_array().unwrap();
    for c in cases {
        let rs = &sets[usize::try_from(c["set"].as_u64().unwrap()).unwrap()];
        let arg = c["arg"].as_str().unwrap();
        let got: Result<J, String> = match c["fn"].as_str().unwrap() {
            "Get" => Ok(meta::get(rs, arg).map_or(J::Null, |r| json!(r.name))),
            "GetMatch" => meta::get_match(rs, arg)
                .map(|r| r.map_or(J::Null, |r| json!(r.name)))
                .map_err(|e| e.to_string()),
            "Match" => meta::matches(rs, arg)
                .map(|v| names(&v))
                .map_err(|e| e.to_string()),
            "ByType" => Ok(names(&meta::by_type(rs, arg))),
            other => panic!("{other}"),
        };
        match (&got, c.get("error")) {
            (Err(_), Some(_)) => {}
            (Ok(g), None) if *g == c["want"] => {}
            _ => failures.push(format!("{c}: got {got:?}")),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} failures:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
    assert_eq!(cases.len(), 445);
}
