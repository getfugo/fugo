//! `fingerprint` and `resources.Copy` against the `transform` oracle
//! (`testdata/oracle/resources/transform/transform.json.gz`): for 14 assets of the
//! synthetic site (text, JSON, XML, SVG, PNG, no extension, unknown extension, names with
//! spaces, non-ASCII and upper case), the chains `fingerprint` (default sha256), `md5`,
//! `sha384`, `sha512` and `md5 | sha512`: target paths, links, `Data.Integrity` (SRI), media
//! type, name, title and content equal Go's; `sha1` is an error. `copy` to one target: the
//! first asset's copy equals Go's; in Go later assets copied to the same target get the first
//! one's cached result (its transformation cache key ignores the source), here they are a
//! target conflict (README).

use std::str::FromStr;

use serde_json::Value as J;
use ssg_base::{Idx, LangIdx, ResourceId};
use ssg_resources::{CallSite, HashAlgo, ResourceError, ResourceStore, Transform};

use crate::support::{MemSink, diff, rec, rule, store, synth_site, want};

fn apply(
    store: &ResourceStore,
    id: ResourceId,
    chain: &[J],
    call: &CallSite,
) -> Result<ResourceId, ResourceError> {
    let mut id = id;
    for step in chain {
        let step = step.as_str().unwrap();
        id = match step.split_once(':') {
            Some(("fingerprint", algo)) => {
                store.transform(id, Transform::Fingerprint(HashAlgo::from_str(algo)?))?
            }
            Some(("copy", target)) => store.copy(target, id, call)?,
            _ => panic!("{step}"),
        };
    }
    Ok(id)
}

#[test]
fn fingerprint_and_copy() {
    let fx: J = ssg_testkit::fixture::oracle("oracle/resources/transform/transform.json.gz");
    let home = tempfile::tempdir().unwrap();
    let store = store(&synth_site(home.path()), home.path());
    let lang = LangIdx::from_index(0);
    let call = CallSite::in_lang(lang);
    let mut failures = Vec::new();
    let (mut compared, mut errors, mut conflicts) = (0, 0, 0);
    let mut copy_owner: Option<String> = None;
    rule("transform", "copy_same_target");
    rule("transform", "pipes");
    let mut links = Vec::new();
    for c in fx["cases"].as_array().unwrap() {
        let chain = c["chain"].as_array().unwrap();
        let steps: Vec<&str> = chain.iter().map(|s| s.as_str().unwrap()).collect();
        if steps
            .iter()
            .any(|s| !s.starts_with("fingerprint:") && !s.starts_with("copy:"))
        {
            continue; // minify and the tool pipes are T42's.
        }
        let asset = c["asset"].as_str().unwrap();
        let what = format!("{asset} {steps:?} {}", c["order"]);
        let src = store.get_asset(lang, asset).unwrap().unwrap();
        let is_copy = steps[0].starts_with("copy:");
        let first_copy = is_copy && copy_owner.get_or_insert_with(|| asset.to_owned()) == asset;
        match apply(&store, src, chain, &call) {
            Err(ResourceError::UnsupportedHash(_)) if c.get("contentErr").is_some() => {
                errors += 1;
            }
            Err(ResourceError::TargetConflict { .. }) if is_copy && !first_copy => {
                conflicts += 1;
            }
            Err(e) => failures.push(format!("{what}: {e}")),
            Ok(_) if c.get("contentErr").is_some() => {
                failures.push(format!("{what}: Go fails: {}", c["contentErr"]));
            }
            Ok(id) if is_copy && !first_copy => {
                failures.push(format!("{what}: expected a target conflict, got {id:?}"));
            }
            Ok(id) => {
                let r = store.resource(id);
                links.push(r.rel_permalink.clone());
                failures.extend(diff(
                    &what,
                    &want(c, &["nameNormalized"]),
                    &rec(&store, &r, Some(c)),
                ));
                compared += 1;
            }
        }
    }
    // Every fingerprinted target is published once its link is seen, with the source's bytes.
    let sink = MemSink::default();
    let stats = store
        .publish(links.iter().map(String::as_str), &sink)
        .unwrap();
    let files = sink.0.lock().unwrap();
    for c in fx["cases"].as_array().unwrap() {
        if c["order"] != "content-first" {
            continue;
        }
        for (path, v) in c["published"].as_object().unwrap() {
            if path.starts_with("copied/") && c["asset"] != "css/a.css" {
                continue;
            }
            if let Some(b) = files.get(path) {
                let got = format!("{}:{}", b.len(), crate::support::sha(b));
                if &J::String(got.clone()) != v {
                    failures.push(format!("published {path}: want {v}, got {got}"));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(errors, 28, "sha1 errors");
    assert_eq!(conflicts, 13 * 2 * 2, "copy conflicts");
    assert_eq!(compared, 28 * 5 + 2 * 2, "compared cases");
    assert_eq!(stats.files, files.len());
    eprintln!(
        "fingerprint: {compared} cases equal Go's, {errors} errors, {conflicts} conflicts, {} files",
        files.len()
    );
}
