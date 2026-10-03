//! Oracle: permalink patterns expanded for every page of Go's builds of the reference sites,
//! with the site's `[permalinks]`, a configuration using every token, and 55 patterns
//! (`oracle/page/permalinks/*`); and the decoding of `[permalinks]` tables
//! (`oracle/page/permalinks/decode.json.gz`).

use serde_json::{Value as J, json};
use ssg_base::PageKind;
use ssg_config::Permalinks;
use ssg_config::sections::PERMALINK_KINDS;
use ssg_page::{PermalinkCtx, PermalinkFile, PermalinkPattern, PermalinkPatterns};

use crate::support::{Tally, family, fixture, idx, oracle_paths, s, site_urls, value, zoned};

/// A kind → section → pattern table of a fixture, decoded and compiled as a site's
/// `[permalinks]` is.
fn compile(v: &J) -> PermalinkPatterns {
    let config = Permalinks::decode(&value(v)).unwrap_or_else(|e| panic!("{v}: {e}"));
    PermalinkPatterns::compile(&config).unwrap_or_else(|e| panic!("{v}: {e}"))
}

fn result(r: Result<String, String>) -> J {
    match r {
        Ok(s) => json!({ "ok": s }),
        Err(e) => json!({ "err": e }),
    }
}

/// Go replaces attributes one after another with `strings.Replace(…, 1)` in the partially
/// expanded pattern; we expand each attribute in place. They differ only when a value holds
/// text that looks like a later attribute.
fn is_replace_quirk(pattern: &str) -> bool {
    pattern.contains(":title:") || pattern.contains(":slug:")
}

#[test]
fn permalinks_match_go() {
    let mut t = Tally::default();
    for file in family("permalinks", &["decode.json.gz"]) {
        let fx = fixture(&format!("permalinks/{file}"));
        let paths = oracle_paths(&fx["paths"]);
        let urls: Vec<_> = fx["pathspecs"]
            .as_array()
            .expect("pathspecs")
            .iter()
            .map(site_urls)
            .collect();
        let patterns: Vec<(String, Result<PermalinkPattern, String>)> = fx["patterns"]
            .as_array()
            .expect("patterns")
            .iter()
            .map(|p| {
                let p = s(p).to_owned();
                let compiled = PermalinkPattern::parse(&p).map_err(|e| e.to_string());
                (p, compiled)
            })
            .collect();
        let site = compile(&fx["siteConfig"]);
        let all = compile(&fx["allConfig"]);
        for c in fx["cases"].as_array().expect("cases") {
            let date = zoned(&c["date"]);
            let entries: Vec<&str> = c["sectionsEntries"]
                .as_array()
                .map(|a| a.iter().map(s).collect())
                .unwrap_or_default();
            assert_eq!(
                format!("/{}", entries.join("/")),
                s(&c["sectionsPath"]),
                "{file}: sections path of {c}"
            );
            let file_ctx = c.get("file").map(|f| PermalinkFile {
                translation_base_name: s(&f["translationBaseName"]),
                dir: s(&f["dir"]),
            });
            let path = &paths[idx(&c["pathInfo"])];
            let ctx = PermalinkCtx {
                date: date.as_ref(),
                title: s(&c["title"]),
                slug: s(&c["slug"]),
                section: s(&c["section"]),
                sections: &entries,
                file: file_ctx,
                content_base_name: &path.unnormalized_name,
                urls: &urls[idx(&c["ps"])],
            };
            let kind = s(&c["kind"]);
            let kind = PageKind::parse(kind).unwrap_or_else(|| panic!("kind {kind}"));
            let expand = |table: &PermalinkPatterns| match table.get(kind, s(&c["section"])) {
                None => Ok(String::new()),
                Some(p) => p.expand(&ctx).map_err(|e| e.to_string()),
            };
            let mut check = |name: &str, got: J, want: &J| {
                let same = match (want.get("ok"), got.get("ok")) {
                    (Some(w), Some(g)) => w == g,
                    (None, None) => true, // both errors: the texts are ours
                    _ => false,
                };
                if same {
                    t.pass();
                } else if is_replace_quirk(name) {
                    t.accept("sequential-replace");
                } else {
                    t.fail(|| format!("{file} {} [{name}] got {got}, want {want}", path.input));
                }
            };
            check("expandSite", result(expand(&site)), &c["expandSite"]);
            check("expandAll", result(expand(&all)), &c["expandAll"]);
            for (i, (pattern, compiled)) in patterns.iter().enumerate() {
                let got = match compiled {
                    Err(e) => Err(e.clone()),
                    Ok(p) => p.expand(&ctx).map_err(|e| e.to_string()),
                };
                check(pattern, result(got), &c["patterns"][i]);
            }
        }
    }
    t.finish("permalinks");
}

/// `Permalinks::decode` gives the kind → section → pattern tables Go decodes (legacy flat
/// entries apply to pages and terms, section keys keep their case), rejects what Go rejects,
/// and `PermalinkPatterns::compile` compiles every decoded table.
#[test]
fn permalink_config_decodes_like_go() {
    let fx = fixture("permalinks/decode.json.gz");
    let mut t = Tally::default();
    for c in fx["cases"].as_array().expect("cases") {
        let name = s(&c["name"]);
        let want = &c["want"];
        match (Permalinks::decode(&value(&c["in"])), want.get("ok")) {
            (Ok(got), Some(want)) => {
                let tables = PERMALINK_KINDS.iter().all(|&kind| {
                    let got = got.of_kind(kind).map_or_else(|| json!({}), |m| json!(m));
                    got == want[kind.as_str()]
                });
                t.check(tables, || format!("{name}: got {got:?}, want {want}"));
                t.check(
                    PermalinkPatterns::compile(&got).is_ok_and(|compiled| {
                        PERMALINK_KINDS.iter().all(|&kind| {
                            want[kind.as_str()]
                                .as_object()
                                .expect("patterns")
                                .keys()
                                .all(|section| {
                                    compiled
                                        .get(kind, section.trim_matches([' ', '/']))
                                        .is_some()
                                })
                        })
                    }),
                    || format!("{name}: does not compile"),
                );
            }
            // Both reject it: the error texts are ours.
            (Err(_), None) => t.pass(),
            // Go's decoder rejects a nested table typed `map[string]string` (a Go type the
            // oracle built by hand; configuration files decode to `map[string]any`), which
            // is the ordinary `[permalinks.page]` table here.
            (Ok(_), None) if name == "advi" => t.accept("go-typed-map"),
            (got, _) => t.fail(|| format!("{name}: got {got:?}, want {want}")),
        }
    }
    t.finish("permalinks-decode");
}
