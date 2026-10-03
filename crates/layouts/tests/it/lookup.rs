//! Go's lookups replayed against the store built from the normalised fixtures (see
//! [`crate::oracle`]): every layout, render hook, shortcode and partial lookup Go made while
//! building the oracle sites, plus Go's grid lookups (every kind × format × front-matter layout
//! × language). The winner must be the synthesised file of Go's winning entry; for layouts
//! that Go wrapped in a base template, the base must be Go's too.

use std::collections::BTreeMap;

use serde_json::Value as J;
use ssg_base::PageKind;
use ssg_base::paths::ContentKey;
use ssg_layouts::{
    EmbeddedHooks, HookKind, HookQuery, HookUse, LayoutQuery, ShortcodeMiss, ShortcodeQuery,
    TemplateName,
};

use crate::oracle::{self, GoDesc, Site};

#[derive(Default, Debug)]
struct Tally {
    /// Per op: (matched, total).
    ops: BTreeMap<String, (usize, usize)>,
    /// Lookups left out by the normalisation (winner not modelled).
    left_out: usize,
    failures: Vec<String>,
}

impl Tally {
    fn record(&mut self, op: &str, ok: bool, detail: impl FnOnce() -> String) {
        let e = self.ops.entry(op.to_owned()).or_default();
        e.1 += 1;
        if ok {
            e.0 += 1;
        } else if self.failures.len() < 25 {
            self.failures.push(detail());
        }
    }

    fn total(&self) -> (usize, usize) {
        self.ops
            .values()
            .fold((0, 0), |(a, b), (m, t)| (a + m, b + t))
    }
}

fn s<'a>(v: &'a J, k: &str) -> &'a str {
    v[k].as_str().unwrap_or_else(|| panic!("{k} in {v}"))
}

/// The main-tree id of a variant's overlay and the (key, descriptor) of its base.
fn split_variant(id: &str) -> Option<(String, String, String)> {
    let rest = id.strip_prefix("variant|")?;
    let (overlay, base) = rest.split_once("|base=")?;
    let mut o = overlay.splitn(3, '|');
    let (key, path, desc) = (o.next()?, o.next()?, o.next()?);
    let (bkey, bdesc) = base.split_once('|')?;
    Some((
        format!("main|{key}|CategoryLayout|{path}|{desc}"),
        bkey.to_owned(),
        bdesc.to_owned(),
    ))
}

fn name(n: Option<&TemplateName>) -> String {
    n.map(ToString::to_string).unwrap_or_default()
}

fn check(site: &Site, go: &J, tally: &mut Tally) {
    let bad = site.check_round_trip();
    assert!(
        bad.is_empty(),
        "{}: {} synthesised names do not round-trip:\n{}",
        site.name,
        bad.len(),
        bad.join("\n")
    );
    let embedded = site.embedded_hooks();
    for r in go["records"].as_array().unwrap() {
        let op = s(r, "op");
        let want = s(r, "result");
        // The winner in synthesised-name terms: (layout, base).
        let want_names = if want.is_empty() {
            Some((String::new(), None))
        } else if let Some((overlay, bkey, bdesc)) = split_variant(want) {
            let base = site.base_by_key_desc(&bkey, &bdesc);
            match (site.by_id.get(&overlay), base) {
                (Some(l), Some(b)) => Some((l.clone(), Some(b))),
                _ => None,
            }
        } else if site.skipped.contains(want) {
            None
        } else {
            let Some(n) = site.by_id.get(want) else {
                panic!("{}: winner {want} is not in the store fixture", site.name);
            };
            Some((n.clone(), None))
        };
        let Some((want_layout, want_base)) = want_names else {
            tally.left_out += 1;
            continue;
        };
        let q = &r["query"];
        let desc = |q: &J| {
            GoDesc::parse(&format!(
                "{{Kind:{} LayoutFromTemplate: OutputFormat:{} MediaType:{} Lang:{} Variant1:{} Variant2:{} IsPlainText:{}}}",
                s(&q["desc"], "kind"),
                s(&q["desc"], "outputFormat"),
                s(&q["desc"], "mediaType"),
                s(&q["desc"], "lang"),
                s(&q["desc"], "variant1"),
                s(&q["desc"], "variant2"),
                q["desc"]["isPlainText"].as_bool().unwrap(),
            ))
        };
        match op {
            "pages" | "shortcode" => {
                let d = desc(q);
                let format = site
                    .format(&d.format)
                    .unwrap_or_else(|| panic!("{}: format {}", site.name, d.format));
                let f = site.env.formats().get(format);
                assert_eq!(
                    site.env.media_types().get(f.media_type).type_string(),
                    d.media,
                    "{}: media type of {}",
                    site.name,
                    d.format
                );
                let path = ContentKey::from_source(s(q, "path"));
                let kind = PageKind::parse(&d.kind);
                let lang = if d.lang.is_empty() {
                    None
                } else {
                    Some(site.lang(&d.lang).unwrap())
                };
                let user = s(&q["desc"], "layoutFromUser");
                let (got_layout, got_base) = match (op, s(q, "category")) {
                    ("pages", "CategoryLayout") => {
                        let sel = site.store.select(&LayoutQuery {
                            path: &path,
                            kind,
                            layout: (!user.is_empty()).then_some(user),
                            exact_layout: q["desc"]["layoutFromUserMustMatch"].as_bool().unwrap(),
                            lang,
                            format,
                        });
                        (
                            name(sel.as_ref().map(|s| &s.layout)),
                            sel.and_then(|s| s.base).map(|b| b.to_string()),
                        )
                    }
                    ("pages", "CategoryMarkup") => {
                        let hook = HookKind::parse(&d.variant1).unwrap();
                        let got = site.store.hook(&HookQuery {
                            hook,
                            variant: (!d.variant2.is_empty()).then_some(d.variant2.as_str()),
                            path: &path,
                            kind,
                            lang,
                            format,
                            // Go's grid lookups pass no candidate filter: every hook counts.
                            embedded: if r["hadConsider"].as_bool().unwrap_or(false) {
                                embedded
                            } else {
                                EmbeddedHooks::both(HookUse::Fallback)
                            },
                        });
                        (name(got.as_ref()), None)
                    }
                    ("shortcode", _) => {
                        let got = site.store.shortcode(&ShortcodeQuery {
                            name: s(q, "name"),
                            path: &path,
                            kind,
                            lang,
                            format,
                            markdown: q["desc"]["alwaysAllowPlainText"].as_bool().unwrap(),
                        });
                        let want_err = s(r, "err");
                        let err_ok = match &got {
                            Ok(_) => want_err.is_empty(),
                            Err(ShortcodeMiss::NotFound(_)) => {
                                want_err.starts_with("no template found")
                            }
                            Err(ShortcodeMiss::Incompatible { .. }) => {
                                want_err.starts_with("no compatible template")
                            }
                        };
                        tally.record("shortcode-error", err_ok, || {
                            format!(
                                "{} shortcode {q}: go err {want_err:?}, rust {got:?}",
                                site.name
                            )
                        });
                        (name(got.as_ref().ok()), None)
                    }
                    (op, c) => panic!("{op} {c}"),
                };
                let ok = got_layout == want_layout && got_base == want_base;
                let label = format!("{op}/{}", s(q, "category"));
                tally.record(&label, ok, || {
                    format!(
                        "{} {label} {q}:\n    go   {want_layout} base {want_base:?} ({want})\n    rust {got_layout} base {got_base:?}",
                        site.name
                    )
                });
            }
            "partial" => {
                let n = s(r, "name");
                let got = name(site.store.partial(n).as_ref());
                tally.record("partial", got == want_layout, || {
                    format!("{} partial {n}: go {want_layout}, rust {got}", site.name)
                });
            }
            "byName" => {
                let n = s(r, "name");
                let got = site.store.has_shortcode(n);
                tally.record("byName", got != want.is_empty(), || {
                    format!("{} byName {n}: go {want:?}, rust {got}", site.name)
                });
            }
            other => panic!("unknown op {other}"),
        }
    }
}

fn report(label: &str, t: &Tally) {
    let (m, n) = t.total();
    eprintln!(
        "{label}: {m}/{n} lookups matched ({} left out by the normalisation); {:?}",
        t.left_out, t.ops
    );
    assert!(
        t.failures.is_empty(),
        "{label}: {} of {n} lookups differ; first:\n{}",
        n - m,
        t.failures.join("\n")
    );
}

fn check_named(site: &str) {
    let s = Site::named(site);
    let go = oracle::fixture("lookup", site);
    let mut t = Tally::default();
    check(&s, &go, &mut t);
    report(site, &t);
}

#[test]
fn lookup_docs() {
    check_named("docs");
}

#[test]
fn lookup_testsite() {
    check_named("testsite");
}

#[test]
fn lookup_legacy() {
    check_named("legacy");
}

#[test]
fn lookup_modern() {
    check_named("modern");
}

#[test]
fn lookup_themes() {
    check_named("themes");
}

/// The layout trees of the Go implementation's `tplimpl` integration tests.
#[test]
fn lookup_integration() {
    let stores = oracle::integration("store");
    let mut lookups = oracle::integration("lookup");
    assert_eq!(stores.len(), lookups.len());
    let mut t = Tally::default();
    for (name, fx) in &stores {
        let s = Site::from_fixture(name, fx);
        let go = lookups.remove(name).unwrap();
        check(&s, &go, &mut t);
    }
    report("integration", &t);
}

#[test]
fn every_named_site_is_covered() {
    for site in oracle::SITES {
        let s = Site::named(site);
        assert!(s.store.templates().len() > 10, "{site}");
    }
}
