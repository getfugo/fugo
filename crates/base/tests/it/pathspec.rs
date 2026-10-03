//! `url::SiteUrls` and `anchor` against the `helpers/pathspec` oracle.

use std::collections::BTreeMap;

use serde_json::Value as J;
use ssg_base::anchor::{self, Style};
use ssg_base::url::{self, Accents, BaseUrl, LinkStyle, PathCase, SiteUrls};

use crate::support::{Tally, fixture, text};

fn site_urls(lang: &J) -> SiteUrls {
    let base_url = BaseUrl::parse(lang["baseURL"].as_str().unwrap()).unwrap();
    assert_eq!(base_url.as_str(), lang["withPath"]);
    assert_eq!(base_url.without_path(), lang["withoutPath"]);
    assert_eq!(base_url.base_path(), lang["basePath"]);
    assert_eq!(
        base_url.base_path_no_trailing_slash(),
        lang["basePathNoTrailingSlash"]
    );
    let flag = |k: &str| lang[k].as_bool().unwrap();
    let urls = SiteUrls {
        base_url,
        language_prefix: lang["languagePrefix"].as_str().unwrap().to_owned(),
        link_style: if flag("canonifyURLs") {
            LinkStyle::Canonify
        } else {
            LinkStyle::Relative
        },
        path_case: if flag("disablePathToLower") {
            PathCase::Preserve
        } else {
            PathCase::Lower
        },
        accents: if flag("removePathAccents") {
            Accents::Remove
        } else {
            Accents::Keep
        },
    };
    assert_eq!(urls.base_path(false), lang["getBasePath"]);
    assert_eq!(urls.base_path(true), lang["getBasePathRel"]);
    urls
}

fn run(u: &SiteUrls, s: &str) -> Vec<Result<String, ()>> {
    vec![
        Ok(u.make_path(s)),
        Ok(u.make_path_sanitized(s)),
        Ok(u.urlize(s)),
        url::url_escape(s).map_err(drop),
        url::url_escape(s).map_err(drop),
        Ok(u.abs_url(s)),
        Ok(u.abs_lang_url(s)),
        Ok(u.rel_url(s)),
        Ok(u.rel_lang_url(s)),
        Ok(u.prepend_base_path(s)),
        Ok(u.prepend_base_path_abs(s)),
        Ok(match url::is_abs_url(s) {
            Ok(b) => b.to_string(),
            Err(_) => "err".to_owned(),
        }),
        Ok(SiteUrls::permalink_for_base_url(s, u.base_url.as_str())),
    ]
}

fn expected(v: &J) -> Result<String, ()> {
    match v {
        J::String(s) if s.starts_with("err: ") => Ok("err".to_owned()),
        J::String(s) => Ok(s.clone()),
        J::Object(o) if o.contains_key("panic") => Err(()),
        other => panic!("unexpected fixture value {other}"),
    }
}

#[test]
fn pathspec_oracle() {
    let f = fixture("oracle/helpers/pathspec/pathspec.json.gz");
    let funcs: Vec<&str> = f["funcs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    let mut setups: BTreeMap<(String, String), SiteUrls> = BTreeMap::new();
    for s in f["setups"].as_array().unwrap() {
        for l in s["langs"].as_array().unwrap() {
            setups.insert(
                (
                    s["name"].as_str().unwrap().to_owned(),
                    l["lang"].as_str().unwrap().to_owned(),
                ),
                site_urls(l),
            );
        }
    }
    let mut t = Tally::new("pathspec");
    let mut anchors = Tally::new("pathspec/SanitizeAnchorName");
    for c in f["cases"].as_array().unwrap() {
        let Some(s) = text(&c["in"]) else {
            t.skip(|| format!("{}: input is not UTF-8", c["in"]));
            continue;
        };
        match c.get("cs").and_then(J::as_str) {
            Some("SanitizeAnchorName") => {
                let got = anchorize(s, Style::Github);
                anchors.check(c["r"] == got.as_str(), || {
                    format!("anchorize({s:?}): want {}, got {got:?}", c["r"])
                });
                continue;
            }
            // Markup detection and TrimShortHTML belong to ssg-page / ssg-markup.
            Some(_) => continue,
            None => {}
        }
        let key = (
            c["setup"].as_str().unwrap().to_owned(),
            c["lang"].as_str().unwrap().to_owned(),
        );
        let u = &setups[&key];
        for ((got, want), name) in run(u, s).iter().zip(c["r"].as_array().unwrap()).zip(&funcs) {
            let w = expected(want);
            t.check(*got == w, || {
                format!("{key:?} {name}({s:?}): want {want}, got {got:?}")
            });
        }
    }
    t.finish();
    anchors.finish();
}

#[test]
fn anchor_styles_and_dedupe() {
    assert_eq!(anchorize("Hello, World!", Style::Github), "hello-world");
    assert_eq!(anchorize("  a  b  ", Style::Github), "a--b");
    assert_eq!(
        anchorize("Crème Brûlée", Style::GithubAscii),
        "creme-brulee"
    );
    assert_eq!(anchorize("Crème Brûlée", Style::Github), "crème-brûlée");
    assert_eq!(
        anchorize("Hello, World!", Style::Blackfriday),
        "hello-world"
    );
    assert_eq!(anchorize("--a b--", Style::Blackfriday), "a-b");
    assert_eq!(anchorize("ภาษาไทย", Style::Github), "ภาษาไทย");

    let mut d = anchor::Deduper::new();
    d.reserve("intro-1");
    assert_eq!(d.unique("intro".into(), "heading"), "intro");
    assert_eq!(d.unique("intro".into(), "heading"), "intro-2");
    assert_eq!(d.unique(String::new(), "heading"), "heading");
    assert_eq!(d.unique(String::new(), "heading"), "heading-1");
}

use anchor::anchorize;
