//! The `resources` oracle (`testdata/oracle/resources/resources/{synth,docs}.json.gz`):
//! every asset and bundle resource of the synthetic site and the docs site, with the page's
//! `resources` front matter metadata applied; name, title, normalized name, media and resource
//! type, data, params, content and links compared with Go; then the files published when every
//! resource's link appears in an output.
//!
//! Accepted deviations (README): Go's `Key` and image sizes are not modelled here; Go's metadata
//! writes the params into the *shared* base resource, so a translation sharing the file sees
//! the other language's params — this store keeps resources immutable, so params of records
//! that reuse another language's resource are not compared.

use std::path::{Path, PathBuf};

use serde_json::Value as J;
use ssg_base::{Idx, LangIdx, Value};
use ssg_resources::meta::ResourceMeta;
use ssg_resources::{BundleResource, PublishPolicy, ResourceStore};
use ssg_testkit::fixture::repo_file;

use crate::support::{MemSink, diff, rec, repo_dir, rule, store, synth_site, want};

/// The Go program's JS package file as Go mounted it in `assets/_jsconfig` (the recorded Go
/// data's name); this port mounts `package.config.json` instead.
const GO_JS_PACKAGE: &str = "_jsconfig/package.hugo.json";

fn file_of(site: &Path, rel: &str) -> PathBuf {
    match rel.strip_prefix("crates/nh-resources/tests/fixtures/site/") {
        Some(rest) => site.join(rest),
        None => repo_file(rel),
    }
}

/// The metadata entries up to the first invalid one (Go stops there and keeps what it
/// applied).
fn metadata(v: &J) -> ResourceMeta {
    let items = v.as_array().unwrap();
    for n in (0..=items.len()).rev() {
        let prefix = Value::from_json(J::Array(items[..n].to_vec()));
        if let Ok(m) = ResourceMeta::parse(&prefix) {
            return m;
        }
    }
    unreachable!()
}

fn run(name: &str, dir: &Path) -> usize {
    rule("resources", "shared_params");
    rule("resources", "key_and_size");
    let fx: J = ssg_testkit::fixture::oracle(&format!("oracle/resources/resources/{name}.json.gz"));
    let home = tempfile::tempdir().unwrap();
    let store: ResourceStore = store(dir, home.path());
    let mut failures = Vec::new();
    let records = fx["records"].as_array().unwrap();
    let mut links = Vec::new();
    for (i, rc) in records.iter().enumerate() {
        // The docs fixture has only Go's file, which this port does not mount.
        if rc["where"] == GO_JS_PACKAGE {
            continue;
        }
        let what = format!("{name}[{i}] {} {}", rc["where"], rc["rd"]["nameOriginal"]);
        let lang = LangIdx::from_index(usize::try_from(rc["lang"].as_u64().unwrap()).unwrap());
        let rd = &rc["rd"];
        let id = match rc["kind"].as_str().unwrap() {
            "asset" => store
                .get_asset(lang, rc["where"].as_str().unwrap())
                .unwrap()
                .unwrap_or_else(|| panic!("{what}: no asset")),
            "bundle" => store.register_bundle(&BundleResource {
                lang,
                file: file_of(dir, rd["file"].as_str().unwrap()),
                name: rd["nameOriginal"].as_str().unwrap().to_owned(),
                dir: rd["basePathTargetPath"].as_str().unwrap().to_owned(),
                policy: if rd["lazyPublish"].as_bool().unwrap() {
                    PublishPolicy::OnReference
                } else {
                    PublishPolicy::Eager
                },
            }),
            other => panic!("{other}"),
        };
        let shared = rc.get("same").is_some();
        let skip: &[&str] = if shared { &["params"] } else { &[] };
        let r = store.resource(id);
        links.push(r.rel_permalink.clone());
        failures.extend(diff(
            &format!("{what} base"),
            &want(&rc["base"], skip),
            &rec(&store, &r, Some(&rc["base"])),
        ));
        if let Some(m) = rc.get("meta").filter(|m| m.is_array()) {
            let id2 = store.apply_meta(id, &metadata(m));
            let r2 = store.resource(id2);
            failures.extend(diff(
                &format!("{what} final"),
                &want(&rc["final"], skip),
                &rec(&store, &r2, None),
            ));
            assert_eq!(r2.target, r.target, "{what}: metadata keeps the target");
        }
    }

    let sink = MemSink::default();
    store
        .publish(links.iter().map(String::as_str), &sink)
        .unwrap();
    let mut want_published = fx["published"].clone();
    if let Some(w) = want_published.as_object_mut() {
        w.remove(GO_JS_PACKAGE);
    }
    failures.extend(diff(
        &format!("{name} published"),
        &want_published,
        &sink.files(),
    ));
    if let (Some(w), Some(g)) = (want_published.as_object(), sink.files().as_object()) {
        for k in g.keys().filter(|k| !w.contains_key(*k)) {
            failures.push(format!("{name} published: unexpected {k}"));
        }
    }

    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
    records.len()
}

#[test]
fn synth() {
    let tmp = tempfile::tempdir().unwrap();
    let n = run("synth", &synth_site(tmp.path()));
    assert_eq!(n, 43);
}

#[test]
fn docs() {
    let n = run("docs", &repo_dir().join("testdata/legacy-docs"));
    assert_eq!(n, 89);
}
