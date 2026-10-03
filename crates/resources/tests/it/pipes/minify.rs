//! `minify` against the `transform` oracle (`oracle/resources/transform`: 14 assets × the
//! chains `minify`, `minify | fingerprint`, `fingerprint | minify`, `minify | minify`), and the
//! chains through tools Go's oracle did not have (`na:tocss`; the `na:postcss` and `na:babel`
//! chains are not applicable: this port has neither tool).
//!
//! Links, names, titles and media types must equal Go's; content equals Go's where
//! ssg-minify writes tdewolff's bytes, and is otherwise accepted when it is a fixed point
//! of the minifier (README: minified bytes differ from tdewolff's); a fingerprint after a
//! minify then names other bytes, so only its shape is checked; `to_css` is built in (grass), so
//! `na:tocss` is not an error here.

use serde_json::Value as J;
use ssg_base::{Idx as _, LangIdx};
use ssg_resources::{HashAlgo, PipeError, ResourceError, Transform};
use std::str::FromStr as _;

use crate::support::{rule, store, synth_site};

#[test]
fn minify_chains() {
    rule("transform", "minify_bytes");
    rule("transform", "tocss_builtin");
    let fx: J = ssg_testkit::fixture::oracle("oracle/resources/transform/transform.json.gz");
    let home = tempfile::tempdir().unwrap();
    let store = store(&synth_site(home.path()), home.path());
    let lang = LangIdx::from_index(0);
    let mut failures = Vec::new();
    let (mut identical, mut accepted, mut builtin) = (0, 0, 0);
    let mut no_minifier = 0;
    let mut no_tool = 0;
    for c in fx["cases"].as_array().unwrap() {
        let chain: Vec<&str> = c["chain"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap())
            .collect();
        if !chain.iter().any(|s| *s == "minify" || s.starts_with("na:")) {
            continue;
        }
        if chain.contains(&"na:postcss") || chain.contains(&"na:babel") {
            no_tool += 1;
            continue;
        }
        let asset = c["asset"].as_str().unwrap();
        let what = format!("{asset} {chain:?} {}", c["order"]);
        let mut id = store.get_asset(lang, asset).unwrap().unwrap();
        let mut minified = false;
        let mut result = Ok(());
        for step in &chain {
            let t = match step.split_once(':').unwrap_or((step, "")) {
                ("minify", _) => {
                    minified = true;
                    Transform::Minify
                }
                ("fingerprint", algo) => Transform::Fingerprint(HashAlgo::from_str(algo).unwrap()),
                ("na", "tocss") => Transform::ToCss(Default::default()),
                other => panic!("{other:?}"),
            };
            let sass = matches!(t, Transform::ToCss(_));
            match store
                .transform(id, t)
                .and_then(|n| store.realize(n).map(|_| n))
            {
                Ok(n) => id = n,
                Err(_) if sass => {
                    // grass may or may not parse the minified asset as SCSS.
                    builtin += 1;
                    result = Err(());
                    break;
                }
                Err(ResourceError::Pipe { source, .. })
                    if matches!(*source, PipeError::NoMinifier(_))
                        && c.get("contentErr").is_some() =>
                {
                    if chain.iter().all(|s| !s.starts_with("na:")) {
                        no_minifier += 1;
                    }
                    result = Err(());
                    break;
                }
                Err(e) => {
                    failures.push(format!("{what}: {e}"));
                    result = Err(());
                    break;
                }
            }
            if sass {
                builtin += 1;
            }
        }
        if result.is_err() || c.get("contentErr").is_some() {
            if result.is_ok() && !chain.contains(&"na:tocss") {
                failures.push(format!("{what}: no error, Go: {}", c["contentErr"]));
            }
            continue;
        }
        let r = store.resource(id);
        for (k, got) in [
            ("name", J::from(r.name.clone())),
            ("title", J::from(r.title.clone())),
            ("mediaType", J::from(r.media_type_string())),
            ("resourceType", J::from(r.resource_type())),
        ] {
            if c[k] != got {
                failures.push(format!("{what}: {k} {got}, want {}", c[k]));
            }
        }
        let content = store.content(id).unwrap();
        let same = c["content"]
            .as_str()
            .is_some_and(|w| w.as_bytes() == &*content);
        let fingerprinted_after = chain.last().is_some_and(|s| s.starts_with("fingerprint"));
        if same || !fingerprinted_after {
            if c["relPermalink"] != r.rel_permalink.as_str() {
                failures.push(format!(
                    "{what}: link {}, want {}",
                    r.rel_permalink, c["relPermalink"]
                ));
            }
        } else if !r.rel_permalink.starts_with(
            c["relPermalink"]
                .as_str()
                .unwrap()
                .split(".min.")
                .next()
                .unwrap(),
        ) {
            failures.push(format!("{what}: link {}", r.rel_permalink));
        }
        if same {
            identical += 1;
        } else if minified {
            // A fixed point: minifying our output again changes nothing.
            let again = store.transform(id, Transform::Minify).unwrap();
            if store.content(again).unwrap() == content {
                accepted += 1;
            } else {
                failures.push(format!("{what}: minifying again changes the output"));
            }
        } else {
            failures.push(format!("{what}: content differs"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(identical + accepted + no_minifier, 28 * 4, "minify chains");
    assert_eq!(
        no_tool,
        28 + 28,
        "chains through PostCSS or Babel (not applicable)"
    );
    eprintln!(
        "minify: {identical} chains with Go's bytes, {accepted} other minified bytes (fixed points), {no_minifier} errors for types without minifier (as Go), {builtin} to_css runs, {no_tool} PostCSS or Babel chains not applicable"
    );
}
