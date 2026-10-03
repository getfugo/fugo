//! The path parser against the Go oracle `oracle/common/paths/pathparser.json.gz`: 14,513 inputs
//! (corpus, sweeps, repository paths, Thai, odd slashes and dots) × three parsers, every
//! accessor of Go's `paths.Path` that `PathInfo` models.

use std::collections::BTreeMap;

use serde_json::{Value as J, json};
use ssg_base::paths::{ContentKey, normalize_key};
use ssg_base::{FormatId, Idx, LangIdx, PageKind};
use ssg_testkit::fixture::oracle;
use ssg_vfs::{
    BundleKind, Component, FormatSpec, LayoutRole, Parsed, PathInfo, PathParser, PathParserSpec,
};

/// A fixture parser, rebuilt from the answers Go's callbacks gave.
struct Fx {
    parser: PathParser,
    langs: BTreeMap<LangIdx, String>,
    formats: BTreeMap<FormatId, String>,
}

fn build(d: &J) -> Fx {
    let mut langs = BTreeMap::new();
    if let Some(m) = d["languageIndex"].as_object() {
        for (k, v) in m {
            let i = usize::try_from(v.as_u64().unwrap()).unwrap();
            langs.insert(LangIdx::from_index(i), k.clone());
        }
    }
    let answered = |key: &str| -> Vec<Vec<J>> {
        d[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e.as_array().unwrap().clone())
            .collect()
    };
    let yes = |e: &[J]| e.last().and_then(J::as_bool) == Some(true);
    let disabled: Vec<String> = answered("langDisabledLog")
        .iter()
        .filter(|e| yes(e))
        .map(|e| e[0].as_str().unwrap().to_owned())
        .collect();
    let content_suffixes: Vec<String> = answered("contentExtLog")
        .iter()
        .filter(|e| yes(e))
        .map(|e| e[0].as_str().unwrap().to_owned())
        .collect();
    let mut suffixes: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for e in answered("outputFormatLog").iter().filter(|e| yes(e)) {
        let exts = suffixes
            .entry(normalize_key(e[0].as_str().unwrap()))
            .or_default();
        let ext = e[1].as_str().unwrap();
        if !ext.is_empty() && !exts.contains(&normalize_key(ext)) {
            exts.push(normalize_key(ext));
        }
    }
    let formats: BTreeMap<FormatId, String> = suffixes
        .keys()
        .enumerate()
        .map(|(i, n)| (FormatId::from_index(i), n.clone()))
        .collect();
    let parser = PathParser::new(PathParserSpec {
        languages: langs.iter().map(|(i, k)| (k.clone(), *i)).collect(),
        disabled_languages: disabled,
        output_formats: formats
            .iter()
            .map(|(id, n)| FormatSpec {
                name: n.clone(),
                id: *id,
                suffixes: suffixes[n].clone(),
            })
            .collect(),
        content_suffixes,
    });
    Fx {
        parser,
        langs,
        formats,
    }
}

fn component(c: &str) -> Option<Component> {
    Component::parse(c)
}

/// Go's `paths.Type` name for a kind (layout roles aside).
fn go_kind(k: BundleKind) -> &'static str {
    match k {
        BundleKind::Single => "TypeContentSingle",
        BundleKind::Leaf => "TypeLeaf",
        BundleKind::Branch => "TypeBranch",
        BundleKind::ContentAdapter => "TypeContentData",
        BundleKind::ContentResource => "TypeContentResource",
        BundleKind::Resource => "TypeFile",
    }
}

fn go_type(p: &PathInfo) -> &'static str {
    match p.layout.as_ref().map(|l| l.role) {
        Some(LayoutRole::Baseof) => "TypeBaseof",
        Some(LayoutRole::Partial) => "TypePartial",
        Some(LayoutRole::Shortcode) => "TypeShortcode",
        Some(LayoutRole::Markup) => "TypeMarkup",
        Some(LayoutRole::Template) | None => go_kind(p.kind),
    }
}

/// The accessors of Go's `paths.Path` that `PathInfo` models, by fixture key.
fn accessor(fx: &Fx, p: &PathInfo, key: &str) -> Option<J> {
    let s = |v: &str| Some(J::String(v.to_owned()));
    match key {
        "Base" => s(&p.key.to_path()),
        "BaseNameNoIdentifier" => s(&p.name),
        "Path" => s(&p.path),
        "Dir" => s(p.dir()),
        "Section" => s(&p.section),
        "Ext" => s(&p.ext),
        "Type" => s(go_type(p)),
        "Lang" => s(p.lang.map_or("", |l| fx.langs[&l].as_str())),
        "OutputFormat" => s(p.format.map_or("", |f| fx.formats[&f].as_str())),
        "Layout" => s(p
            .layout
            .as_ref()
            .and_then(|l| l.layout.as_deref())
            .unwrap_or_default()),
        "Kind" => Some(json!(
            p.layout.as_ref().and_then(|l| l.kind).map(PageKind::as_str)
        )),
        "IsContent" => Some(J::Bool(p.kind.is_content())),
        "IsBundle" => Some(J::Bool(p.kind.is_bundle())),
        "IsLeafBundle" => Some(J::Bool(p.kind == BundleKind::Leaf)),
        "IsBranchBundle" => Some(J::Bool(p.kind == BundleKind::Branch)),
        "IsContentData" => Some(J::Bool(p.kind == BundleKind::ContentAdapter)),
        "Disabled" => Some(J::Bool(false)),
        _ => None,
    }
}

/// Go's value in the form `accessor` gives (the kind identifier becomes the kind it names).
fn want_form(key: &str, w: &J) -> J {
    match key {
        "Kind" => json!(
            w.as_str()
                .and_then(PageKind::parse)
                .filter(|k| matches!(
                    k,
                    PageKind::Home
                        | PageKind::Page
                        | PageKind::Section
                        | PageKind::Taxonomy
                        | PageKind::Term
                ))
                .map(PageKind::as_str)
        ),
        "OutputFormat" => json!(normalize_key(w.as_str().unwrap_or_default())),
        _ => w.clone(),
    }
}

#[derive(Default)]
struct Tally {
    cases: usize,
    checks: usize,
    /// Go's `TypeShortcode` outside layouts (a `/_shortcodes/` path in content): a plain file.
    shortcode_as_file: usize,
    /// A Go key with an empty segment or a trailing slash (`a//`, `/tags//_index.md`, a page
    /// file named `.md`): a [`ContentKey`] has neither. Walks never produce such paths (empty
    /// segments do not exist, and content names starting with `.` are ignored).
    key_slashes: usize,
    /// Go's unnormalised parse looked identifiers up case-sensitively and so found a different
    /// structure (`Index.EN.md`); `original` keeps the normalised structure.
    case_sensitive_lookup: usize,
    bad: Vec<String>,
}

#[test]
fn pathparser_matches_go() {
    let f: J = oracle("oracle/common/paths/pathparser.json.gz");
    let fxs: BTreeMap<String, Fx> = f["parsers"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, d)| (k.clone(), build(d)))
        .collect();
    let keys = |k: &str| -> Vec<String> {
        f["keys"][k]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect()
    };
    let (out_keys, mini_keys, mod_keys) = (keys("out"), keys("mini"), keys("mod"));
    let mini_sets = keys("miniSets");
    let cases = f["cases"].as_array().unwrap();
    assert!(cases.len() > 14_000, "{} cases", cases.len());

    let mut t = Tally::default();
    for c in cases {
        t.cases += 1;
        let fx = &fxs[c["p"].as_str().unwrap()];
        let input = c["in"].as_str().unwrap();
        let comp_name = c["c"].as_str().unwrap();
        let mut errs = Vec::new();

        t.checks += 1;
        if json!(normalize_key(input)) != c["norm"] {
            errs.push(format!("norm: want {}", c["norm"]));
        }

        // Go's parser also takes "" as a component; nothing then depends on it.
        let comp = component(comp_name).unwrap_or(Component::Assets);
        let ks = if mini_sets.iter().any(|s| s == c["set"].as_str().unwrap()) {
            &mini_keys
        } else {
            &out_keys
        };
        let want: BTreeMap<&str, &J> = ks
            .iter()
            .map(String::as_str)
            .zip(c["out"].as_array().unwrap())
            .collect();

        match fx.parser.parse(comp, input) {
            Parsed::DisabledLanguage => {
                t.checks += 1;
                if want.get("Disabled").and_then(|v| v.as_bool()) != Some(true) {
                    errs.push("parsed as a disabled language".to_owned());
                }
            }
            Parsed::File(p) => {
                check_out(&mut t, &mut errs, fx, &p, &want, comp);
                check_original(&mut t, &mut errs, &p, c, ks, &want);
                if let Some(m) = c.get("mod") {
                    let b = (*p).clone().into_bundled();
                    let got = [
                        json!(go_kind(b.kind)),
                        json!(b.kind.is_content()),
                        json!(b.key.to_path()),
                        json!(b.name),
                    ];
                    for (i, g) in got.iter().enumerate() {
                        t.checks += 1;
                        if i == 2
                            && &m[i] != g
                            && m[i].as_str().is_some_and(|w| {
                                ContentKey::from_source(w).to_path() == g.as_str().unwrap()
                            })
                        {
                            t.key_slashes += 1;
                            continue;
                        }
                        if &m[i] != g {
                            errs.push(format!("mod.{}: want {} got {g}", mod_keys[i], m[i]));
                        }
                    }
                }
            }
        }
        if !errs.is_empty() {
            t.bad.push(format!(
                "{} {comp_name} {input:?} ({}):\n    {}",
                c["p"],
                c["set"],
                errs.join("\n    ")
            ));
        }
    }
    assert!(
        t.bad.is_empty(),
        "{} of {} cases differ:\n{}",
        t.bad.len(),
        t.cases,
        t.bad[..t.bad.len().min(40)].join("\n")
    );
    eprintln!(
        "pathparser: {} cases, {} checks, 0 differences; accepted: {} Go TypeShortcode outside \
         layouts (a plain file), {} unnormalised case-sensitive lookups, {} keys with empty \
         segments",
        t.cases, t.checks, t.shortcode_as_file, t.case_sensitive_lookup, t.key_slashes
    );
    assert!(t.key_slashes <= 162, "{}", t.key_slashes);
    assert!(t.shortcode_as_file <= 144, "{}", t.shortcode_as_file);
    assert!(
        t.case_sensitive_lookup <= 192,
        "{}",
        t.case_sensitive_lookup
    );
}

fn check_out(
    t: &mut Tally,
    errs: &mut Vec<String>,
    fx: &Fx,
    p: &PathInfo,
    want: &BTreeMap<&str, &J>,
    comp: Component,
) {
    if want.get("Disabled").and_then(|v| v.as_bool()) == Some(true) {
        errs.push("Go: disabled language".to_owned());
        return;
    }
    for (k, w) in want {
        let Some(got) = accessor(fx, p, k) else {
            continue;
        };
        t.checks += 1;
        let w = want_form(k, w);
        if got == w {
            continue;
        }
        if matches!(*k, "Base" | "mod.Base")
            && w.as_str()
                .is_some_and(|w| ContentKey::from_source(w).to_path() == got)
        {
            t.key_slashes += 1;
            continue;
        }
        if *k == "Type" && w == "TypeShortcode" && got == "TypeFile" && comp != Component::Layouts {
            t.shortcode_as_file += 1;
            continue;
        }
        errs.push(format!("{k}: want {w} got {got}"));
    }
}

/// `original` against Go's unnormalised path (`"self"`: the input was already normalised).
fn check_original(
    t: &mut Tally,
    errs: &mut Vec<String>,
    p: &PathInfo,
    c: &J,
    ks: &[String],
    want: &BTreeMap<&str, &J>,
) {
    let u: BTreeMap<&str, &J> = match c["u"].as_array() {
        Some(u) => ks.iter().map(String::as_str).zip(u).collect(),
        None => want.clone(),
    };
    let got = [
        ("Path", &p.original.path),
        ("Base", &p.original.base),
        ("BaseNameNoIdentifier", &p.original.name),
        ("Section", &p.original.section),
    ];
    let differs: Vec<String> = got
        .iter()
        .filter(|(k, g)| u.get(k).and_then(|v| v.as_str()) != Some(g.as_str()))
        .map(|(k, g)| format!("u.{k}: want {} got {g:?}", u[k]))
        .collect();
    t.checks += got.len();
    if differs.is_empty() {
        return;
    }
    // Go looks the unnormalised identifiers up case-sensitively (`EN` is no language, `MD` no
    // content suffix), so its unnormalised path can have another structure.
    let same_structure = u["Type"] == want["Type"]
        && ["Identifiers", "Lang"]
            .iter()
            .all(|k| match (u.get(k), want.get(k)) {
                (Some(a), Some(b)) => {
                    normalize_key(&a.to_string()) == normalize_key(&b.to_string())
                }
                _ => true,
            });
    if same_structure {
        errs.extend(differs);
    } else {
        t.case_sensitive_lookup += 1;
    }
}
