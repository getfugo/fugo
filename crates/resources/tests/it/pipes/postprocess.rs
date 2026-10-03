//! `post_process`: per-field placeholders filled in build phase E5.
//!
//! - Against the `transform` oracle's post-processed resources (`resources.PostProcess` of
//!   14 assets' `fingerprint` chains): content, links, integrity and media type after
//!   replacement equal Go's.

use std::str::FromStr as _;

use serde_json::Value as J;
use ssg_base::{Idx as _, LangIdx};
use ssg_resources::{HashAlgo, PpField, Transform};

use crate::support::{store, synth_site};

/// Go's placeholders in `doc` and the values `replaced` holds for them, in order.
fn go_values(doc: &str, replaced: &str) -> Vec<(String, String)> {
    let mut literals = Vec::new();
    let mut placeholders = Vec::new();
    let mut rest = doc;
    while let Some(i) = rest.find("__h_pp_l1_") {
        let end = i + rest[i..].find("__e=").unwrap() + 4;
        literals.push(&rest[..i]);
        placeholders.push(rest[i..end].to_owned());
        rest = &rest[end..];
    }
    literals.push(rest);
    let mut out = Vec::new();
    let mut pos = literals[0].len();
    for (i, ph) in placeholders.into_iter().enumerate() {
        let next = literals[i + 1];
        let len = if next.is_empty() {
            replaced.len() - pos
        } else {
            replaced[pos..].find(next).unwrap()
        };
        out.push((ph, replaced[pos..pos + len].to_owned()));
        pos += len + next.len();
    }
    out
}

#[test]
fn post_process_oracle() {
    let fx: J = ssg_testkit::fixture::oracle("oracle/resources/transform/transform.json.gz");
    let go = go_values(
        fx["doc"].as_str().unwrap(),
        fx["replaced"].as_str().unwrap(),
    );
    let home = tempfile::tempdir().unwrap();
    let store = store(&synth_site(home.path()), home.path());
    let lang = LangIdx::from_index(0);
    let mut compared = 0;
    let mut failures = Vec::new();
    for c in fx["cases"].as_array().unwrap() {
        let Some(pp) = c.get("pp") else { continue };
        let chain: Vec<&str> = c["chain"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap())
            .collect();
        if !chain.iter().all(|s| s.starts_with("fingerprint:")) {
            continue; // minify bytes differ (minify.rs); the tool chains fail.
        }
        // Go's placeholder prefix of this case: `__h_pp_l1_<n>_`.
        let content_ph = pp["content"].as_str().unwrap();
        let prefix = content_ph.trim_end_matches("Content__e=");
        let mut id = store
            .get_asset(lang, c["asset"].as_str().unwrap())
            .unwrap()
            .unwrap();
        for step in &chain {
            let algo = HashAlgo::from_str(step.trim_start_matches("fingerprint:")).unwrap();
            id = store.transform(id, Transform::Fingerprint(algo)).unwrap();
        }
        let ours = store.post_process(id);
        assert_eq!(store.post_process(id), ours, "one id per resource");
        for (field, go_field) in [
            (PpField::Content, "Content"),
            (PpField::RelPermalink, "RelPermalink"),
            (PpField::Permalink, "Permalink"),
            (PpField::Integrity, "Data.Integrity"),
            (PpField::MediaType, "MediaType.Type"),
        ] {
            let go_ph = format!("{prefix}{go_field}__e=");
            let Some((_, want)) = go.iter().find(|(ph, _)| *ph == go_ph) else {
                continue;
            };
            let text = format!("[{}]", ours.placeholder(field));
            let got = store.resolve_post_process(&text).unwrap().unwrap();
            if got != format!("[{want}]") {
                failures.push(format!(
                    "{} {chain:?} {go_field}: {got} want [{want}]",
                    c["asset"]
                ));
            }
            compared += 1;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(
        compared,
        13 * 5,
        "13 post-processed fingerprint chains, 5 fields"
    );
    assert_eq!(store.resolve_post_process("no placeholder").unwrap(), None);
    assert_eq!(
        store
            .resolve_post_process("__nh_pp_999_content__ __nh_pp_1_nope__")
            .unwrap()
            .as_deref(),
        Some("__nh_pp_999_content__ __nh_pp_1_nope__"),
        "unknown placeholders stay"
    );
    eprintln!("post_process: {compared} fields equal to Go's after replacement");
}
