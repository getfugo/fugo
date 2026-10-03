//! Oracle: cascade decoding and matching, markup detection and page configuration
//! (`oracle/page/misc/misc.json.gz`). Output format construction and lookup, and the
//! template-side `NamedPageMetaValue`, belong to `ssg-config` and `ssg-view`.

use jiff::tz::TimeZone;
use serde_json::{Value as J, json};
use ssg_base::paths::ContentKey;
use ssg_base::{Map, PageKind, Params, Value};
use ssg_config::sections::SitemapConfig;
use ssg_config::{MediaTypes, OutputFormats};
use ssg_page::{
    AdapterPage, Cascade, CascadeTarget, Cjk, DateResolver, Markup, MatchCtx, MetaCtx,
    meta_from_params,
};

use crate::support::{Tally, fixture, s, value};

fn params_json(p: &Params) -> J {
    serde_json::to_value(Value::map(p.as_map().clone())).expect("params")
}

fn check_cascade(t: &mut Tally, c: &J) {
    let want = &c["want"];
    let warned = !s(&c["errors"]).is_empty();
    match Cascade::decode(&value(&c["in"])) {
        Err(e) => t.check(want.get("ok").is_none(), || {
            format!("cascade {}: {e}, want {want}", c["in"])
        }),
        Ok(cascade) => {
            let entries: Vec<J> = cascade
                .rules()
                .iter()
                .map(|r| {
                    let [kind, path, lang, environment] = r.target.sources();
                    json!({
                        "fields": params_json(&r.fields),
                        "params": params_json(&r.params),
                        "target": { "environment": environment, "kind": kind, "lang": lang, "path": path },
                    })
                })
                .collect();
            let want_entries = want["ok"]["entries"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let got_warned = cascade
                .rules()
                .iter()
                .any(|r| r.target.path_looks_like_file());
            t.check(
                want.get("ok").is_some() && entries == want_entries && got_warned == warned,
                || {
                    format!(
                        "cascade {}: got {entries:?} (warned {got_warned}), want {want}",
                        c["in"]
                    )
                },
            );
        }
    }
}

fn check_matches(t: &mut Tally, c: &J) {
    let m = &c["matcher"];
    let p = &c["page"];
    let Ok(target) = CascadeTarget::new(
        s(&m["kind"]),
        s(&m["path"]),
        s(&m["lang"]),
        s(&m["environment"]),
    ) else {
        // Go ignores a criterion whose glob does not compile; we reject the cascade entry.
        t.accept("bad-glob-rejected");
        return;
    };
    let ctx = MatchCtx {
        kind: PageKind::parse(s(&p["kind"])).expect("kind"),
        path: s(&p["path"]),
        lang: s(&p["lang"]),
        environment: s(&p["env"]),
    };
    let got = target.matches(&ctx);
    t.check(got == c["want"], || format!("matches {m} {p}: got {got}"));
}

fn markup_of_type(t: &J) -> Option<Markup> {
    match s(t) {
        "text/markdown" => Some(Markup::Markdown),
        "text/html" => Some(Markup::Html),
        _ => None,
    }
}

fn check_markup(t: &mut Tally, c: &J, types: &MediaTypes) {
    let want = markup_of_type(&c["want"]["type"]);
    let got = Markup::from_name(s(&c["in"]), types).ok();
    t.check(got == want, || {
        format!(
            "markup {}: got {got:?}, want {}",
            c["in"], c["want"]["type"]
        )
    });
}

fn check_page_config(t: &mut Tally, c: &J, formats: &OutputFormats, types: &MediaTypes) {
    let i = &c["in"];
    let want = &c["want"];
    if i["pagesFromData"] == true {
        check_adapter_page_config(t, c, types);
        return;
    }
    let mut m = match value(&i["params"]) {
        Value::Map(m) => (*m).clone(),
        _ => Map::new(),
    };
    for (k, key) in [("markup", "markup"), ("mediaType", "mediatype")] {
        if !s(&i[k]).is_empty() {
            m.insert(key, value(&i[k]));
        }
    }
    if !i["outputs"].is_null() {
        m.insert("outputs", value(&i["outputs"]));
    }
    if i["cascade"] == true {
        m.insert("cascade", value(&json!({ "params": { "x": 1 } })));
    }
    let resolver = DateResolver::new(&[]);
    let ctx = MetaCtx {
        kind: PageKind::parse(s(&i["kind"])).expect("kind"),
        formats,
        media_types: types,
        sitemap: &SitemapConfig::default(),
        cjk_default: Cjk::No,
        ext: s(&i["ext"]),
        dates: &resolver,
        file: None,
        time_zone: &TimeZone::UTC,
    };
    let want_err = want.get("compileErr").is_some() || want.get("initErr").is_some();
    let want_markup = markup_of_type(&want["contentMediaType"]["type"]);
    match meta_from_params(Params::fold(&m), &ctx) {
        Err(e) => {
            if want_err {
                t.pass();
            } else if want_markup.is_none() {
                // AsciiDoc and other external markups: Go builds them with external tools.
                t.accept("markup-not-supported");
            } else {
                t.fail(|| format!("pageConfig {i}: {e}, want {want}"));
            }
        }
        Ok(meta) => {
            let outputs = meta.outputs.as_ref().map(|o| {
                o.iter()
                    .map(|&id| formats.get(id).name.clone())
                    .collect::<Vec<_>>()
            });
            let want_outputs: Option<Vec<String>> = want["outputs"]
                .as_array()
                .map(|a| a.iter().map(|v| s(v).to_owned()).collect());
            let ok = !want_err && Some(meta.markup) == want_markup && outputs == want_outputs;
            t.check(ok, || {
                format!(
                    "pageConfig {i}: got {:?} {outputs:?}, want {want}",
                    meta.markup
                )
            });
        }
    }
}

/// A page a content adapter adds (`PageConfig.Init(true)` and `CompileForPagesFromDataPre`):
/// the map's kind, path, lang and content markup, from an adapter at the content root.
fn check_adapter_page_config(t: &mut Tally, c: &J, types: &MediaTypes) {
    let i = &c["in"];
    let want = &c["want"];
    let want_err = want.get("initErr").is_some() || want.get("compileErr").is_some();
    let map = json!({
        "kind": i["kind"],
        "path": i["path"],
        "lang": i["lang"],
        "content": { "markup": i["markup"], "mediaType": i["mediaType"] },
    });
    let Value::Map(m) = value(&map) else {
        panic!("a map");
    };
    let got = AdapterPage::decode(&Params::fold(&m), &ContentKey::home(), types);
    match got {
        Err(e) => t.check(want_err, || format!("pageConfig {i}: {e}, want {want}")),
        Ok(p) => {
            let ok = !want_err
                && p.path == s(&want["path"])
                && Some(p.markup) == markup_of_type(&want["contentMediaType"]["type"]);
            t.check(ok, || {
                format!("pageConfig {i}: got {} {:?}, want {want}", p.path, p.markup)
            });
        }
    }
}

#[test]
fn misc_matches_go() {
    let fx = fixture("misc/misc.json.gz");
    let types = MediaTypes::decode(&Map::new()).expect("media types");
    let formats = OutputFormats::builtin(&types);
    let mut cascade = Tally::default();
    let mut matches = Tally::default();
    let mut markup = Tally::default();
    let mut page_config = Tally::default();
    for c in fx["cases"].as_array().expect("cases") {
        match s(&c["fn"]) {
            "cascade" => check_cascade(&mut cascade, c),
            "matches" => check_matches(&mut matches, c),
            "markupToMediaType" => {
                let supported = markup_of_type(&c["want"]["type"]).is_some();
                let unknown = s(&c["want"]["type"]).is_empty();
                if supported || unknown {
                    check_markup(&mut markup, c, &types);
                } else if Markup::from_name(s(&c["in"]), &types).is_err() {
                    markup.accept("markup-not-supported");
                } else {
                    check_markup(&mut markup, c, &types);
                }
            }
            "pageConfig" => check_page_config(&mut page_config, c, &formats, &types),
            _ => {}
        }
    }
    cascade.finish("cascade-decode");
    matches.finish("cascade-match");
    markup.finish("markup");
    page_config.finish("page-config");
}
