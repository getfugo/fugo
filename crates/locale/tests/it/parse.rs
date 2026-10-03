//! Message file layouts against go-i18n's parser (`oracle/i18n/parse`): the messages (key →
//! plural texts) of every file Go reads, and an error for every file Go rejects.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value as J;
use ssg_locale::MessageFile;

use crate::common::expected_diffs;

#[derive(Deserialize)]
struct Fixture {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    content: String,
    path: String,
    result: J,
}

const FORMS: [&str; 6] = ["zero", "one", "two", "few", "many", "other"];

type Messages = BTreeMap<String, BTreeMap<String, String>>;

fn go_messages(result: &J) -> Messages {
    let mut out = Messages::new();
    for m in result["messages"].as_array().unwrap() {
        let forms = FORMS
            .iter()
            .filter_map(|f| {
                let t = m[*f].as_str().unwrap_or("");
                (!t.is_empty()).then(|| ((*f).to_owned(), t.to_owned()))
            })
            .collect();
        out.insert(m["id"].as_str().unwrap().to_owned(), forms);
    }
    out
}

fn our_messages(file: &MessageFile) -> Messages {
    file.messages
        .iter()
        .map(|m| {
            let forms = m
                .forms
                .iter()
                .filter(|(_, t)| !t.is_empty())
                .map(|(f, t)| (f.as_str().to_owned(), t.clone()))
                .collect();
            (m.id.clone(), forms)
        })
        .collect()
}

#[test]
fn message_file_layouts_oracle() {
    let fixture: Fixture = ssg_testkit::fixture::oracle("oracle/i18n/parse/parse.json.gz");
    let listed = expected_diffs().parse.cases;
    let mut failures = Vec::new();
    let mut agreed = 0;
    let mut deviations = Vec::new();
    for (i, case) in fixture.cases.iter().enumerate() {
        let ours = MessageFile::read(Path::new(&case.path), &case.content);
        let go_ok = case.result.get("err").is_none();
        let same = match (&ours, go_ok) {
            (Ok(file), true) => our_messages(file) == go_messages(&case.result),
            (Err(_), false) => true,
            _ => false,
        };
        let key = i.to_string();
        match (same, listed.contains_key(&key)) {
            (true, false) => agreed += 1,
            (false, true) => deviations.push(key),
            (true, true) => failures.push(format!("case {i}: listed but agrees")),
            (false, false) => failures.push(format!(
                "case {i} {} {:?}: ours {:?}, Go {}",
                case.path,
                case.content,
                ours.map(|f| our_messages(&f)).map_err(|e| e.to_string()),
                case.result
            )),
        }
    }
    println!(
        "parse: {agreed}/{} agree, accepted deviations {deviations:?}",
        fixture.cases.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
