//! Oracle: target paths, links and permalinks of every (page, format) descriptor Go's builds
//! of the reference sites created, plus adversarial variants (`oracle/page/paths/*`).

use serde_json::{Value as J, json};
use ssg_base::url::PathCase;
use ssg_base::{FormatId, Idx, LangIdx, PageKind};
use ssg_page::{LangPrefix, SourcePath, UrlInputs, links, target_paths};
use ssg_vfs::{Component, FormatSpec, Parsed, PathParser, PathParserSpec};

use crate::support::{Tally, family, fixture, idx, oracle_paths, output_format, s, site_urls};

/// `""` for the root and for "no resources" (Go writes both as the empty string).
fn dir_str(p: &str) -> &str {
    if p == "/" { "" } else { p }
}

fn got(c: &J, cx: &Ctx) -> J {
    let urls = &cx.urls[idx(&c["ps"])];
    let (format, suffix) = &cx.formats[idx(&c["type"])];
    let path = &cx.paths[idx(&c["path"])];
    let section = c["section"]
        .as_i64()
        .and_then(|i| usize::try_from(i).ok())
        .map_or("/", |i| cx.paths[i].base.as_str());
    let opt = |k: &str| Some(s(&c[k])).filter(|v| !v.is_empty());
    let source: &SourcePath = &path.source;
    let inputs = UrlInputs {
        kind: PageKind::parse(s(&c["kind"])).expect("kind"),
        format,
        suffix,
        source,
        section,
        base_name: s(&c["baseName"]),
        prefix: LangPrefix {
            file: s(&c["prefixFilePath"]),
            link: s(&c["prefixLink"]),
            force: c["forcePrefix"].as_bool().expect("forcePrefix"),
        },
        url: opt("url"),
        pager: opt("addends"),
        permalink: opt("expandedPermalink"),
        site_ugly: c["uglyURLs"].as_bool().expect("uglyURLs"),
        urls,
    };
    match target_paths(&inputs) {
        Err(e) => json!({ "panic": e.to_string() }),
        Ok(tp) => {
            let (rel, perma) = match links(&tp, urls, format) {
                Ok(l) => (l.rel_permalink.escaped(), l.permalink.to_string()),
                Err(_) => (String::new(), String::new()),
            };
            let (res_target, res_link) =
                tp.resources
                    .as_ref()
                    .map_or((String::new(), String::new()), |r| {
                        (
                            dir_str(r.target.as_str()).to_owned(),
                            dir_str(r.link.as_str()).to_owned(),
                        )
                    });
            json!({ "ok": {
                "targetFilename": tp.target.as_str(),
                "subResourceBaseTarget": res_target,
                "subResourceBaseLink": res_link,
                "link": tp.link.escaped(),
                "relPermalink": rel,
                "permalink": perma,
            }})
        }
    }
}

struct Ctx {
    urls: Vec<ssg_base::url::SiteUrls>,
    formats: Vec<(ssg_config::OutputFormat, String)>,
    paths: Vec<crate::support::OraclePath>,
}

/// The documented deviation a difference falls under, if any.
fn classify(c: &J, want: &J, got: &J) -> Option<&'static str> {
    let url = s(&c["url"]);
    if want.get("panic").is_some() || got.get("panic").is_some() {
        // Go panics on URLs it cannot parse; we return an error. Both sides failing is a match
        // (checked before); one side failing is a URL Go accepts and we do not, or vice versa.
        return None;
    }
    let (w, g) = (&want["ok"], &got["ok"]);
    let only_escaping = [
        "targetFilename",
        "subResourceBaseTarget",
        "subResourceBaseLink",
    ]
    .iter()
    .all(|k| w[k] == g[k]);
    if only_escaping && (url.contains('?') || url.contains('%')) {
        // Front matter `url` with a query or percent escapes: we keep the URL as a path
        // (decoded, then escaped once), Go keeps the query and the raw escapes.
        return Some("url-query-or-escape");
    }
    None
}

#[test]
fn target_paths_match_go() {
    let mut t = Tally::default();
    let mut from_builds = 0;
    for file in family("paths", &[]) {
        let fx = fixture(&format!("paths/{file}"));
        let cx = Ctx {
            urls: fx["pathspecs"]
                .as_array()
                .expect("pathspecs")
                .iter()
                .map(site_urls)
                .collect(),
            formats: fx["formats"]
                .as_array()
                .expect("formats")
                .iter()
                .map(output_format)
                .collect(),
            paths: oracle_paths(&fx["paths"]),
        };
        for c in fx["cases"].as_array().expect("cases") {
            if c["src"] == "build" {
                from_builds += 1;
            }
            let want = &c["want"];
            let got = got(c, &cx);
            let same = if want.get("panic").is_some() {
                got.get("panic").is_some()
            } else {
                // Output paths are clean: Go's `/th/section/` resource directory (a pattern
                // that expands to an empty last element) is `/th/section`.
                let mut want = want.clone();
                if let Some(J::String(d)) = want.pointer_mut("/ok/subResourceBaseTarget")
                    && d.len() > 1
                    && d.ends_with('/')
                {
                    d.pop();
                }
                got == want
            };
            if same {
                t.pass();
            } else if let Some(class) = classify(c, want, &got) {
                t.accept(class);
            } else {
                let input = &cx.paths[idx(&c["path"])].input;
                t.fail(|| format!("{file} {input} {c}\n      got {got}"));
            }
        }
    }
    assert!(
        from_builds > 5000,
        "{from_builds} descriptors from the builds"
    );
    t.finish("paths");
}

/// The path parser of an oracle site.
fn parser(fx: &J) -> PathParser {
    let p = &fx["parser"];
    PathParser::new(PathParserSpec {
        languages: p["languageIndex"]
            .as_object()
            .expect("languages")
            .iter()
            .map(|(k, v)| (k.clone(), LangIdx::from_index(idx(v))))
            .collect(),
        disabled_languages: Vec::new(),
        output_formats: fx["formats"]
            .as_array()
            .expect("formats")
            .iter()
            .enumerate()
            .map(|(i, f)| FormatSpec {
                name: s(&f["name"]).to_owned(),
                id: FormatId::from_index(i),
                suffixes: s(&f["mediaType"]["suffixesCSV"])
                    .split(',')
                    .filter(|x| !x.is_empty())
                    .map(str::to_owned)
                    .collect(),
            })
            .collect(),
        content_suffixes: p["contentExtLog"]
            .as_array()
            .expect("content")
            .iter()
            .filter(|e| e[1] == true)
            .map(|e| s(&e[0]).to_owned())
            .collect(),
    })
}

/// `SourcePath::from_path_info` over the path parser gives the directory, name and shape Go
/// derived from the same content paths.
#[test]
fn source_paths_match_go() {
    let mut t = Tally::default();
    for file in family("paths", &[]) {
        let fx = fixture(&format!("paths/{file}"));
        let parser = parser(&fx);
        let entries = fx["paths"].as_array().expect("paths");
        for (entry, oracle) in entries.iter().zip(oracle_paths(&fx["paths"])) {
            if entry["retype"] == true {
                // Content files inside leaf bundles, re-typed by the capture phase.
                continue;
            }
            let case = if entry["unnormalizedOf"] == true {
                PathCase::Preserve
            } else {
                PathCase::Lower
            };
            let Parsed::File(pi) = parser.parse(Component::Content, &oracle.input) else {
                t.fail(|| format!("{file} {}: not parsed", oracle.input));
                continue;
            };
            let got = SourcePath::from_path_info(&pi, case);
            let want = &oracle.source;
            // Go's container directory of a branch bundle is not used by any URL.
            let dir_ok = oracle.is_branch || got.dir == want.dir;
            t.check(
                dir_ok && got.name == want.name && got.shape == want.shape,
                || format!("{file} {}: got {got:?}, want {want:?}", oracle.input),
            );
        }
    }
    t.finish("source-paths");
}
