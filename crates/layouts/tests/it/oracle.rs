//! The `tplimpl` Go-oracle fixtures, normalised to v0.146 layout files.
//!
//! The store fixture of a site (`oracle/tplimpl/store/<site>.json.gz`) dumps Go's template
//! tree after Go mapped legacy names: every entry has a key (directory), a category and a
//! descriptor (kind, layout, language, output format, media type, variants). The Go lookup
//! fixtures name their winners by these entries. The sites use legacy names (`_default/`,
//! `partials/`, `taxonomy/tag.html`, …) that this crate refuses, and Go may keep one file
//! under several keys (a legacy mapping plus the file's own place).
//!
//! **Normalisation.** Every tree entry of the categories this crate models (layouts, base
//! templates, render hooks, partials, shortcodes; user and embedded) becomes one synthesised
//! v0.146 file whose name spells its key and descriptor: `<key>/[<layout>.][<kind>.][<lang>.]
//! [<format>.]<ext>`, `<key>/baseof.…`, `<key>/_markup/render-<v1>[-<v2>]…`,
//! `_partials/<name>…`, `<key>/_shortcodes/<name>…` (the format only when the extension alone
//! does not name it). A layout whose Go entry needs a base template (`noBaseOf` false) gets the
//! source `{% extends "baseof.html" %}`. [`Site::check_round_trip`] verifies that the store
//! derives exactly Go's descriptor from every synthesised name.
//!
//! Left out (counted, see the crate README): entries whose spelling is a refused legacy name
//! (Go keeps `term/term.html` at `term/` besides mapping it), Go's internal templates (its
//! private directory and `_server/`), inline partials (`{{ define "partials/x" }}`, a
//! Go-template feature), files without a suffix (no media type: they never match, and the scan
//! refuses them), and the lookups whose winner is one of them.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde_json::Value as J;
use ssg_base::{FormatId, Idx, LangIdx, Value};
use ssg_config::output::Escaping;
use ssg_config::{MediaTypes, OutputFormats};
use ssg_layouts::{
    EmbeddedHooks, HookUse, IssueKind, LayoutEnv, LayoutSource, LayoutStore, Origin, TemplateError,
    TemplateRole,
};
use ssg_vfs::{FormatSpec, PathParser, PathParserSpec};

/// The named sites of the oracle (the integration archives come from `integration.json.gz`).
pub const SITES: &[&str] = &["docs", "testsite", "legacy", "modern", "themes"];

/// Reads a fixture of `topic` (`store`, `lookup`) for a named site.
pub fn fixture(topic: &str, site: &str) -> J {
    ssg_testkit::fixture::oracle(&format!("oracle/tplimpl/{topic}/{site}.json.gz"))
}

/// The integration archives' fixtures of `topic`, by site name (pool references resolved).
pub fn integration(topic: &str) -> BTreeMap<String, J> {
    let mut fx = fixture(topic, "integration");
    let pool = fx["pool"].as_array().cloned().unwrap_or_default();
    let mut sites = fx["sites"].take();
    unpool(&mut sites, &pool);
    match sites {
        J::Object(m) => m.into_iter().collect(),
        _ => BTreeMap::new(),
    }
}

fn unpool(v: &mut J, pool: &[J]) {
    match v {
        J::Object(m) => {
            if m.len() == 1
                && let Some(i) = m.get("$p").and_then(J::as_u64)
            {
                *v = pool[usize::try_from(i).unwrap()].clone();
                return;
            }
            m.values_mut().for_each(|x| unpool(x, pool));
        }
        J::Array(a) => a.iter_mut().for_each(|x| unpool(x, pool)),
        _ => {}
    }
}

fn s<'a>(v: &'a J, k: &str) -> &'a str {
    v[k].as_str().unwrap_or_else(|| panic!("{k} in {v}"))
}

/// A Go `TemplateDescriptor` as the oracle prints it (`%+v`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GoDesc {
    pub kind: String,
    pub layout: String,
    pub format: String,
    pub media: String,
    pub lang: String,
    pub variant1: String,
    pub variant2: String,
    pub plain: bool,
}

impl GoDesc {
    pub fn parse(s: &str) -> Self {
        let body = s.trim_start_matches('{').trim_end_matches('}');
        let mut d = Self::default();
        for field in body.split_whitespace() {
            let (k, v) = field.split_once(':').unwrap();
            let v = v.to_owned();
            match k {
                "Kind" => d.kind = v,
                "LayoutFromTemplate" => d.layout = v,
                "OutputFormat" => d.format = v,
                "MediaType" => d.media = v,
                "Lang" => d.lang = v,
                "Variant1" => d.variant1 = v,
                "Variant2" => d.variant2 = v,
                "IsPlainText" => d.plain = v == "true",
                _ => {}
            }
        }
        d
    }
}

/// One entry of Go's template tree.
#[derive(Clone, Debug)]
pub struct Entry {
    pub id: String,
    pub category: String,
    pub key: String,
    pub desc: GoDesc,
    pub embedded: bool,
    /// Shortcodes: the name.
    pub sc_name: String,
    /// The file suffix of the entry's path.
    pub ext: String,
    pub needs_base: bool,
}

/// A fixture site rebuilt as a store of synthesised v0.146 files.
pub struct Site {
    pub name: String,
    pub cfg: J,
    pub env: LayoutEnv,
    pub store: LayoutStore,
    /// Go entry id → the Tera name of its synthesised file.
    pub by_id: BTreeMap<String, String>,
    /// Tera name → Go entry.
    pub entries: BTreeMap<String, Entry>,
    /// Ids of the entries left out (internal templates, inline partials).
    pub skipped: BTreeSet<String>,
    pub langs: BTreeMap<String, LangIdx>,
}

fn to_value(j: J) -> Value {
    serde_json::from_value(j).unwrap()
}

fn map(j: J) -> ssg_base::Map {
    match to_value(j) {
        Value::Map(m) => (*m).clone(),
        other => panic!("not a map: {other:?}"),
    }
}

/// The environment of a fixture's configuration dump.
pub fn env_of(cfg: &J) -> (LayoutEnv, BTreeMap<String, LangIdx>) {
    let mut media = serde_json::Map::new();
    for m in cfg["mediaTypes"].as_array().unwrap() {
        media.insert(
            s(m, "type").to_owned(),
            serde_json::json!({
                "suffixes": m["suffixes"].as_array().cloned().unwrap_or_default(),
                "delimiter": s(m, "delimiter"),
            }),
        );
    }
    let media = MediaTypes::decode(&map(J::Object(media))).unwrap();
    let mut formats = serde_json::Map::new();
    for f in cfg["outputFormats"].as_array().unwrap() {
        let mut o = f.as_object().unwrap().clone();
        let mt = s(&f["mediaType"], "type").to_owned();
        o.insert("mediaType".into(), J::String(mt));
        o.remove("name");
        formats.insert(s(f, "name").to_owned(), J::Object(o));
    }
    let formats = OutputFormats::decode(&map(J::Object(formats)), &media).unwrap();
    let dcl = s(cfg, "defaultContentLanguage");
    let mut langs: Vec<(String, usize)> = cfg["languageIndex"]
        .as_object()
        .map(|m| {
            m.iter()
                .map(|(k, v)| (k.clone(), usize::try_from(v.as_u64().unwrap()).unwrap()))
                .collect()
        })
        .unwrap_or_default();
    langs.sort_by_key(|(k, i)| (k != dcl, *i));
    let langs: BTreeMap<String, LangIdx> = langs
        .into_iter()
        .enumerate()
        .map(|(i, (k, _))| (k, LangIdx::from_index(i)))
        .collect();
    let parser = PathParser::new(PathParserSpec {
        languages: langs.iter().map(|(k, i)| (k.clone(), *i)).collect(),
        disabled_languages: cfg["disabledLanguages"]
            .as_array()
            .map(|a| a.iter().map(|v| v.as_str().unwrap().to_owned()).collect())
            .unwrap_or_default(),
        output_formats: formats
            .iter()
            .map(|(id, f)| FormatSpec {
                name: f.name.clone(),
                id,
                suffixes: media.get(f.media_type).suffixes.clone(),
            })
            .collect(),
        content_suffixes: Vec::new(),
    });
    let default_format = formats.by_name(s(cfg, "defaultOutputFormat"));
    let env = LayoutEnv::new(parser, Arc::new(formats), Arc::new(media), default_format);
    (env, langs)
}

impl Site {
    pub fn named(name: &str) -> Self {
        Self::from_fixture(name, &fixture("store", name))
    }

    pub fn from_fixture(name: &str, fx: &J) -> Self {
        let cfg = fx["config"].clone();
        let (env, langs) = env_of(&cfg);
        let mut entries = Vec::new();
        let mut skipped = BTreeSet::new();
        let store = &fx["store"];
        let all = store["main"]
            .as_array()
            .unwrap()
            .iter()
            .chain(store["shortcodes"].as_array().into_iter().flatten());
        for e in all {
            let id = s(e, "id").to_owned();
            let category = s(e, "category").to_owned();
            let sub = s(e, "subCategory");
            let modelled = matches!(
                category.as_str(),
                "CategoryLayout"
                    | "CategoryBaseof"
                    | "CategoryMarkup"
                    | "CategoryPartial"
                    | "CategoryShortcode"
            );
            // A file without a suffix has no media type and never matches; it is refused here.
            let no_media = GoDesc::parse(s(e, "desc")).media.is_empty();
            if !modelled || sub == "SubCategoryInline" || no_media {
                skipped.insert(id);
                continue;
            }
            let path = s(e, "path");
            let file = path.rsplit('/').next().unwrap_or_default();
            let ext = file.rsplit_once('.').map(|(_, x)| x).unwrap_or_default();
            entries.push(Entry {
                desc: GoDesc::parse(s(e, "desc")),
                category,
                key: s(e, "key").to_owned(),
                embedded: sub == "SubCategoryEmbedded",
                sc_name: e["scName"].as_str().unwrap_or_default().to_owned(),
                ext: ext.to_owned(),
                needs_base: !e["noBaseOf"].as_bool().unwrap_or(true),
                id,
            });
        }

        let mut sources = Vec::new();
        let mut by_id = BTreeMap::new();
        let mut by_name = BTreeMap::new();
        for e in entries {
            let rel = synthesise(&env, &e);
            let origin = if e.embedded {
                Origin::Embedded
            } else {
                Origin::User(format!("/sites/{name}/layouts/{rel}").into())
            };
            let tera_name = format!("{}{rel}", origin.prefix());
            let source = if e.needs_base && e.category == "CategoryLayout" {
                "{% extends \"baseof.html\" %}".to_owned()
            } else {
                "x".to_owned()
            };
            sources.push(LayoutSource {
                rel,
                origin,
                source,
            });
            by_id.insert(e.id.clone(), tera_name.clone());
            assert!(
                by_name.insert(tera_name.clone(), e).is_none(),
                "{name}: two entries synthesise {tera_name}"
            );
        }
        // Go keeps a legacy file at its own place too (`term/term.html` as a `term` layout of
        // type `term`); those spellings are refused here, so their entries are left out.
        let mut legacy = BTreeSet::new();
        if let Err(TemplateError::Layouts(issues)) =
            LayoutStore::from_sources(env.clone(), sources.clone())
        {
            for i in issues {
                assert!(
                    matches!(i.kind, IssueKind::LegacyName { .. }),
                    "{name}: {i}"
                );
                legacy.insert(i.position.file.to_path_buf());
            }
        }
        sources.retain(|src| {
            let drop = src.origin.file().is_some_and(|f| legacy.contains(f));
            if drop {
                let e = by_name.remove(&src.rel).unwrap();
                by_id.remove(&e.id);
                skipped.insert(e.id);
            }
            !drop
        });
        let store = LayoutStore::from_sources(env.clone(), sources)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        Self {
            name: name.to_owned(),
            cfg,
            env,
            store,
            by_id,
            entries: by_name,
            skipped,
            langs,
        }
    }

    /// Every synthesised file whose descriptor in the store differs from Go's.
    pub fn check_round_trip(&self) -> Vec<String> {
        let formats = self.env.formats();
        let mut bad = Vec::new();
        for (name, e) in &self.entries {
            let Some(t) = self.store.template(name) else {
                bad.push(format!("{name}: not in the store"));
                continue;
            };
            let (kind, layout) = match &t.role {
                TemplateRole::Layout { kind, layout } | TemplateRole::Base { kind, layout } => {
                    (*kind, layout.clone())
                }
                _ => (None, None),
            };
            let (v1, v2) = match &t.role {
                TemplateRole::Hook { kind, variant } => (
                    kind.as_str().to_owned(),
                    variant.clone().unwrap_or_default(),
                ),
                _ => (String::new(), String::new()),
            };
            let lang = t
                .lang
                .and_then(|l| self.langs.iter().find(|(_, i)| **i == l))
                .map(|(k, _)| k.clone())
                .unwrap_or_default();
            let got = GoDesc {
                kind: kind.map(|k| k.as_str().to_owned()).unwrap_or_default(),
                layout: layout.unwrap_or_default(),
                format: t
                    .format
                    .map(|f| formats.get(f).name.clone())
                    .unwrap_or_default(),
                media: t
                    .media
                    .map(|m| media_string(&self.env, m))
                    .unwrap_or_default(),
                lang,
                variant1: v1,
                variant2: v2,
                plain: t.escaping == Escaping::Plain,
            };
            let key = match &t.role {
                TemplateRole::Partial { name } => format!("/_partials/{name}"),
                _ if t.scope.is_home() => String::new(),
                _ => format!("/{}", t.scope),
            };
            // A shortcode's layout and kind are not part of its role; the lookups check them.
            let mut want = e.desc.clone();
            if e.category == "CategoryShortcode" {
                want.layout.clear();
                want.kind.clear();
            }
            if got != want || key != e.key {
                bad.push(format!(
                    "{name}: go {} {:?}\n    rust {key} {got:?}",
                    e.key, e.desc
                ));
            }
        }
        bad
    }

    /// The format id of a format name.
    pub fn format(&self, name: &str) -> Option<FormatId> {
        self.env.formats().by_name(name)
    }

    pub fn lang(&self, key: &str) -> Option<LangIdx> {
        self.langs.get(key).copied()
    }

    /// The embedded-hook policy of the fixture's configuration (`auto` means `never` in a
    /// configuration dump: Go resolved it to `fallback` before where it applies).
    pub fn embedded_hooks(&self) -> EmbeddedHooks {
        let p = |k: &str| match s(&self.cfg, k) {
            "always" => HookUse::Always,
            "fallback" => HookUse::Fallback,
            _ => HookUse::Never,
        };
        EmbeddedHooks {
            link: p("renderHookLink"),
            image: p("renderHookImage"),
        }
    }

    /// The Tera name of the base template entry with this key and descriptor.
    pub fn base_by_key_desc(&self, key: &str, desc: &str) -> Option<String> {
        let d = GoDesc::parse(desc);
        self.entries
            .iter()
            .find(|(_, e)| e.category == "CategoryBaseof" && e.key == key && e.desc == d)
            .map(|(n, _)| n.clone())
    }
}

fn media_string(env: &LayoutEnv, m: ssg_base::MediaTypeId) -> String {
    env.media_types().get(m).type_string()
}

/// The v0.146 file name that spells a Go entry's key and descriptor.
fn synthesise(env: &LayoutEnv, e: &Entry) -> String {
    let d = &e.desc;
    let key = e.key.trim_start_matches('/');
    let dir = |k: &str| {
        if k.is_empty() {
            String::new()
        } else {
            format!("{k}/")
        }
    };
    let mut stem: Vec<String> = Vec::new();
    let (prefix, mut parts) = match e.category.as_str() {
        "CategoryLayout" => {
            if !d.layout.is_empty() {
                stem.push(d.layout.clone());
            }
            if !d.kind.is_empty() {
                stem.push(d.kind.clone());
            }
            (dir(key), stem)
        }
        "CategoryBaseof" => {
            stem.push("baseof".to_owned());
            if !d.layout.is_empty() {
                stem.push(d.layout.clone());
            }
            if !d.kind.is_empty() {
                stem.push(d.kind.clone());
            }
            (dir(key), stem)
        }
        "CategoryMarkup" => {
            let mut h = format!("render-{}", d.variant1);
            if !d.variant2.is_empty() {
                h = format!("{h}-{}", d.variant2);
            }
            stem.push(h);
            (format!("{}_markup/", dir(key)), stem)
        }
        "CategoryPartial" => {
            let (parent, file) = key.rsplit_once('/').unwrap_or(("", key));
            stem.push(file.to_owned());
            (dir(parent), stem)
        }
        "CategoryShortcode" => {
            stem.push(e.sc_name.clone());
            if !d.layout.is_empty() {
                stem.push(d.layout.clone());
            }
            if !d.kind.is_empty() {
                stem.push(d.kind.clone());
            }
            (format!("{}_shortcodes/", dir(key)), stem)
        }
        c => panic!("category {c}"),
    };
    if !d.lang.is_empty() {
        parts.push(d.lang.clone());
    }
    let ext_names_format = env
        .formats()
        .by_name(&e.ext)
        .is_some_and(|f| env.formats().get(f).name == d.format);
    if !d.format.is_empty() && (!ext_names_format || parts.is_empty()) {
        parts.push(d.format.clone());
    }
    if !e.ext.is_empty() {
        parts.push(e.ext.clone());
    }
    format!("{prefix}{}", parts.join("."))
}
