//! `get_remote` against the `getremote` oracle
//! (`testdata/oracle/resource-transformers/getremote/getremote.json.gz`), without network:
//!
//! - the Go build's cache names (`go_keys`) of the oracle's 48 key vectors;
//! - the oracle's calls replayed from the 32 cache entries the Go build wrote (imported on demand
//!   into this crate's cache under its own names): links, names, media types, `.Data`, content, and
//!   errors (security policy, status, options, URLs, media type, missing entries offline).
//!
//! Accepted deviations (README): `echo-post-upper` (the same request as `echo-post` with
//! upper-case option names) is one request here, so it gets `echo-post`'s resource; `head`
//! resolves `application/json; charset=UTF-8` to the configured JSON type and gets a `.json`
//! suffix (Go keeps the parameterised type unmatched and no suffix); `vnd-accepted`'s media
//! type drops the `; charset=utf-8` parameter Go keeps in the type string.

use std::fs;
use std::path::Path;

use serde_json::{Value as J, json};
use ssg_base::{Idx, LangIdx, Map, Value};
use ssg_resources::{RemoteOptions, ResourceStore, StoreConfig, go_keys};
use ssg_testkit::fixture::testdata;

use crate::support::{config, json_doc, sha};

const SECURITY: &str = r#"baseURL = "https://example.org/sub/"
[security.http]
mediaTypes = ['^application/vnd\.t15\+json']
methods = ['(?i)GET|POST|HEAD']
urls = ['.*']
"#;

fn options_of(v: Option<&J>) -> Option<Map> {
    match v.map(|j| Value::from_json(j.clone())) {
        Some(Value::Map(m)) => Some((*m).clone()),
        _ => None,
    }
}

fn offline_store(tmp: &Path, import: &Path) -> ResourceStore {
    let site = tmp.join("site");
    fs::create_dir_all(&site).unwrap();
    fs::write(site.join("config.toml"), SECURITY).unwrap();
    let cfg = config(&site, &tmp.join("home"));
    let mut sc = StoreConfig::from_config(&cfg, None, None);
    sc.remote.cache_dir = Some(tmp.join("cache/getresource"));
    sc.remote.import_dirs = vec![import.to_owned()];
    sc.remote.network = false;
    ResourceStore::new(sc)
}

/// The oracle's record of a remote resource, for comparison.
fn rec(store: &ResourceStore, id: ssg_base::ResourceId, want: &J) -> J {
    let r = store.resource(id);
    let b = store.content(id).unwrap();
    let mut o = json!({
        "name": r.name, "title": r.title, "nameNormalized": r.name_normalized,
        "mediaType": r.media_type_string(), "resourceType": r.resource_type(),
        "data": serde_json::to_value(&r.data).unwrap(),
        "params": serde_json::to_value(r.params.as_map()).unwrap(),
        "contentLen": b.len(), "relPermalink": r.rel_permalink, "permalink": r.permalink.as_str(),
    });
    if want.get("content").is_some() {
        o["content"] = json!(String::from_utf8_lossy(&b));
    } else {
        o["contentSha"] = json!(sha(&b));
    }
    o
}

fn want(res: &J, skip: &[&str]) -> J {
    let mut o = serde_json::Map::new();
    for (k, v) in res.as_object().unwrap() {
        if matches!(
            k.as_str(),
            "key" | "mediaTypeJSON" | "width" | "height" | "whPanic"
        ) || skip.contains(&k.as_str())
        {
            continue;
        }
        let v = if matches!(k.as_str(), "data" | "params") {
            json_doc(v)
        } else {
            v.clone()
        };
        o.insert(k.clone(), v);
    }
    J::Object(o)
}

fn compare(what: &str, want: &J, got: &J, failures: &mut Vec<String>) {
    for (k, w) in want.as_object().unwrap() {
        if got.get(k) != Some(w) {
            failures.push(format!(
                "{what}.{k}: want {w}, got {}",
                got.get(k).unwrap_or(&J::Null)
            ));
        }
    }
}

#[test]
fn go_cache_names() {
    let fx: J =
        ssg_testkit::fixture::oracle("oracle/resource-transformers/getremote/getremote.json.gz");
    let calls = fx["calls"].as_array().unwrap();
    let mut n = 0;
    for k in fx["keys"].as_array().unwrap() {
        let call = calls.iter().find(|c| c["name"] == k["name"]).unwrap();
        let args = call["args"].as_array().unwrap();
        let url = args[0].as_str().unwrap();
        let got = go_keys(url, options_of(args.get(1)).as_ref());
        assert_eq!(got.0, k["userKey"].as_str().unwrap(), "{}", k["name"]);
        assert_eq!(got.1, k["optionsKey"].as_str().unwrap(), "{}", k["name"]);
        n += 1;
    }
    assert_eq!(n, 48);
}

#[test]
fn oracle_calls_from_go_cache() {
    let fx: J =
        ssg_testkit::fixture::oracle("oracle/resource-transformers/getremote/getremote.json.gz");
    let tmp = tempfile::tempdir().unwrap();
    let store = offline_store(
        tmp.path(),
        &testdata("oracle/resource-transformers/getremote/cache"),
    );
    let lang = LangIdx::from_index(0);
    let mut failures = Vec::new();
    let (mut ok, mut errors, mut skipped, mut listed) = (0, 0, 0, 0);
    let expected_diffs = crate::support::expected_diffs();
    let diffs = expected_diffs["getremote"].as_table().unwrap();
    for c in fx["calls"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let args = c["args"].as_array().unwrap();
        // Argument-count and argument-type errors are the template function's (T35).
        let url = match args.first() {
            Some(J::String(s)) => s.clone(),
            Some(J::Number(n)) => n.to_string(),
            _ => {
                skipped += 1;
                continue;
            }
        };
        if args.len() > 2 || args.get(1).is_some_and(|a| !a.is_object()) {
            skipped += 1;
            continue;
        }
        let expected = &c["cached"];
        let got = RemoteOptions::from_map(options_of(args.get(1)).as_ref())
            .and_then(|o| store.get_remote(lang, &url, &o));
        match (got, expected.get("res")) {
            (Ok(Some(id)), Some(res)) => {
                let skip: Vec<&str> = diffs
                    .get(name)
                    .and_then(|d| d["fields"].as_array())
                    .map(|f| f.iter().filter_map(|x| x.as_str()).collect())
                    .unwrap_or_default();
                let w = want(res, &skip);
                compare(name, &w, &rec(&store, id, &w), &mut failures);
                if !skip.is_empty() {
                    // The entry is still needed: the full record differs.
                    let full = want(res, &[]);
                    let mut differs = Vec::new();
                    compare(name, &full, &rec(&store, id, &full), &mut differs);
                    assert!(!differs.is_empty(), "{name}: stale expected_diffs entry");
                    listed += 1;
                }
                ok += 1;
            }
            (Err(_), None) if expected.get("err").is_some() => errors += 1,
            (Ok(None), None) if expected.get("nil").is_some() => ok += 1,
            (got, _) => failures.push(format!("{name}: want {expected}, got {got:?}")),
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!((ok, errors, skipped), (33, 15, 3));
    assert_eq!(listed, diffs.len());
    // 31 of the 32 entries of the Go build are imported under this crate's names (`echo-post-upper`
    // is `echo-post` here, so its entry is not read).
    let imported = fs::read_dir(tmp.path().join("cache/getresource"))
        .unwrap()
        .count();
    assert_eq!(imported, 31);
}
