//! Gate A-T (REWRITE_PLAN.md §7.3, T60): the testsite built by the binary against
//! the Go build of the same site (`crates/build/tests/it/testsite-go.txtar`).
//!
//! The gate lives here, not in `ssg-build`, because only this crate's tests can run the
//! binary (`CARGO_BIN_EXE_fugo`): the command line, the disk sink and the static copy are part
//! of what is compared.
//!
//! - **L1** paths: the file set, after §7.2's normalisation, equals Go's 55 files in `public`
//!   (Go also writes its stats file next to its configuration; this port writes none).
//! - **L2** links: per HTML file the `<title>`, `<link rel=canonical|alternate>` and the set of
//!   internal `href`/`src`/`srcset` URLs (percent-decoded); the alias → target map; the
//!   `<link>`/`<loc>`/`<guid>` lists of RSS and sitemaps; the URL leaves of JSON; link
//!   integrity (an internal link that resolves to none of our files is dangling in Go's
//!   output too: the testsite's layouts link to files that do not exist). Byte equality of
//!   every file is checked as well; it implies the rest while it holds.
//! - **L3** text: per HTML file the visible text (tags, comments, `script` and `style`
//!   removed, entities decoded, typographic characters mapped to ASCII, whitespace collapsed)
//!   and the heading-ID list.
//! - **Structure oracle**: the build's own dump (`FUGO_STRUCTURE_OUT`, `ssg-build`'s
//!   `structure.rs`) against Go's (`testdata/golden/testsite/structure.json`, T01), with
//!   the facts `cargo dev structdiff` compares: per (lang, page, kind, format) the
//!   target, `.RelPermalink`, `.Permalink`, template and base template (an embedded one marked
//!   as such), `written` and `pagers`; per alias file and `page/1/` alias the page, format,
//!   kind and permalink; per bundle resource the link, file and `publish`; per page its output
//!   formats.
//!
//! Accepted deviations are listed per level below; every list is empty. A structure fact may
//! differ only when the ratchet's baseline (`testdata/baselines/testsite.json`, T03)
//! accepts it (`accepted-deviation`); it accepts none.
//!
//! The full output tree (every file's content) is an insta snapshot: `snapshots/it__parity__testsite_output.snap`.

use std::collections::{BTreeMap, BTreeSet};

use ssg_testkit::fixture::{go_output_as_built_here, repo_dir};
use ssg_testkit::txtar::Archive;

use crate::build::{testsite, tree};
use crate::{binary, stderr};

mod levels;
mod scan;

use levels::*;
use scan::*;

/// Files of the Go build whose bytes may differ, with the reason (L2 bytes).
const ACCEPTED_BYTE_DIFFS: &[(&str, &str)] = &[];
/// Files whose L2 link sets may differ, with the reason.
const ACCEPTED_LINK_DIFFS: &[(&str, &str)] = &[];
/// Files whose L3 visible text or heading IDs may differ, with the reason.
const ACCEPTED_TEXT_DIFFS: &[(&str, &str)] = &[];

/// Go's output: the 55 files of `public`.
fn go_public() -> BTreeMap<String, Vec<u8>> {
    let go = Archive::read(&repo_dir().join("crates/build/tests/it/testsite-go.txtar"))
        .expect("go tree");
    go.files
        .into_iter()
        .map(|f| (f.name, go_output_as_built_here(&f.data).into_bytes()))
        .collect()
}

/// The testsite built by the binary: `public`.
struct Built {
    _tmp: tempfile::TempDir,
    public: BTreeMap<String, Vec<u8>>,
    /// The structure dump of the build.
    structure: serde_json::Value,
}

fn build_testsite() -> Built {
    let tmp = tempfile::tempdir().expect("tempdir");
    let site = tmp.path().join("testsite");
    testsite(&site);
    let dump = tmp.path().join("structure.json");
    let dump_env = dump.to_string_lossy().into_owned();
    let o = binary(
        &site,
        &["--clock", "2026-01-01T00:00:00Z"],
        &[(ssg_build::STRUCTURE_ENV, dump_env.as_str())],
    );
    assert!(o.status.success(), "{}", stderr(&o));
    let public = tree(&site.join("public"));
    let structure = ssg_testkit::fixture::read_json(&dump).expect("the structure dump");
    Built {
        _tmp: tmp,
        public,
        structure,
    }
}

// ── L1 ───────────────────────────────────────────────────────────────────────────────────────

/// §7.2's path normalisation: fingerprints `.[0-9a-f]{16,64}.` → `.H.`, processed-image
/// hashes `_hu_[0-9a-f]+` → `_hu_H`.
fn normalize_path(path: &str) -> String {
    let hex = |s: &str| {
        s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    };
    let mut parts: Vec<String> = path.split('.').map(str::to_owned).collect();
    let last = parts.len().saturating_sub(1);
    for (i, p) in parts.iter_mut().enumerate() {
        if i > 0 && i < last && (16..=64).contains(&p.len()) && hex(p) {
            *p = "H".to_owned();
        }
    }
    let mut out = parts.join(".");
    let mut from = 0;
    while let Some(i) = out[from..].find("_hu_").map(|i| i + from) {
        let rest = &out[i + 4..];
        let n = rest.bytes().take_while(u8::is_ascii_hexdigit).count();
        if n > 0 {
            out = format!("{}_hu_H{}", &out[..i], &rest[n..]);
        }
        from = i + 4;
    }
    out
}

// ── Structure oracle ─────────────────────────────────────────────────────────────────────────

/// A template's identity: its v0.146 name, marked when it is an embedded one.
fn template_id(r: &serde_json::Value, name: &str, file: &str) -> String {
    let n = r[name].as_str().unwrap_or_default();
    let f = r[file].as_str().unwrap_or(n);
    if !n.is_empty() && f.starts_with("_embedded/") {
        format!("{n} (embedded)")
    } else {
        n.to_owned()
    }
}

/// Every compared fact of a structure dump by key (the keys and fields of
/// `structure_items` of `crates/dev/src/structdiff.rs`).
fn structure_facts(doc: &serde_json::Value) -> BTreeMap<String, serde_json::Value> {
    let s = |v: &serde_json::Value, k: &str| v[k].as_str().unwrap_or_default().to_owned();
    let rows = |k: &str| doc[k].as_array().cloned().unwrap_or_default();
    let mut out = BTreeMap::new();
    for r in rows("records") {
        let key = format!(
            "record {} {} {} {}",
            s(&r, "lang"),
            s(&r, "path"),
            s(&r, "kind"),
            s(&r, "format")
        );
        let v = serde_json::json!({
            "target": s(&r, "target"),
            "relPermalink": s(&r, "relPermalink"),
            "permalink": s(&r, "permalink"),
            "template": template_id(&r, "template", "templateFile"),
            "baseof": template_id(&r, "baseof", "baseofFile"),
            "written": r["written"].as_bool().unwrap_or(true),
            "pagers": r["pagers"].as_u64().unwrap_or(0),
        });
        out.insert(key, v);
    }
    for (name, section) in [("alias", "aliases"), ("pager", "pagerAliases")] {
        for a in rows(section) {
            let key = format!(
                "{name} {} {} {} {}",
                s(&a, "from"),
                s(&a, "lang"),
                s(&a, "path"),
                s(&a, "format")
            );
            let kind = if name == "alias" {
                s(&a, "kind")
            } else {
                String::new()
            };
            out.insert(key, serde_json::json!([s(&a, "permalink"), kind]));
        }
    }
    for r in rows("resources") {
        let key = format!(
            "resource {} {} {}",
            s(&r, "lang"),
            s(&r, "path"),
            s(&r, "name")
        );
        let targets = r
            .get("targets")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([s(&r, "target")]));
        let publish = r["publish"].as_bool().unwrap_or(true);
        out.insert(
            key,
            serde_json::json!([s(&r, "relPermalink"), targets, publish]),
        );
    }
    for p in rows("pages") {
        let key = format!("page {} {} {}", s(&p, "lang"), s(&p, "path"), s(&p, "kind"));
        out.insert(key, p["outputs"].clone());
    }
    out
}

/// The structure facts the baseline accepts as differing.
fn accepted_structure() -> BTreeSet<String> {
    let path = repo_dir().join("testdata/baselines/testsite.json");
    let doc: serde_json::Value =
        ssg_testkit::fixture::read_json(&path).unwrap_or_else(|e| panic!("{e}"));
    doc["structure"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(_, v)| v["S"]["class"].as_str() == Some("accepted-deviation"))
        .map(|(k, _)| k.clone())
        .collect()
}

// ── The gate ─────────────────────────────────────────────────────────────────────────────────

fn accepted(list: &[(&str, &str)]) -> Vec<String> {
    list.iter().map(|(p, _)| (*p).to_owned()).collect()
}

#[test]
fn testsite_gate_a_t() {
    let go = go_public();
    assert_eq!(go.len(), 55, "Go's public directory");
    let built = build_testsite();
    let ours = &built.public;

    // L1: the 55 files of `public`.
    let norm = |m: &BTreeMap<String, Vec<u8>>| -> Vec<String> {
        let mut v: Vec<String> = m.keys().map(|k| normalize_path(k)).collect();
        v.sort();
        v
    };
    let (want, got) = (norm(&go), norm(ours));
    assert_eq!(got.len(), 55);
    assert_eq!(got, want, "L1: the file multiset differs from Go's");

    // L2, bytes.
    let differ: Vec<String> = go
        .iter()
        .filter(|(k, v)| ours.get(*k) != Some(*v))
        .map(|(k, _)| k.clone())
        .collect();
    assert_eq!(differ, accepted(ACCEPTED_BYTE_DIFFS), "L2: bytes differ");

    // L2, links.
    let link_diffs: Vec<String> = go
        .iter()
        .filter(|(k, v)| links(k, v) != links(k, &ours[*k]))
        .map(|(k, _)| k.clone())
        .collect();
    assert_eq!(
        link_diffs,
        accepted(ACCEPTED_LINK_DIFFS),
        "L2: links differ"
    );
    let aliases = ours
        .iter()
        .filter_map(|(k, v)| links(k, v).alias_target.map(|t| (k.clone(), t)))
        .count();
    assert_eq!(aliases, 15, "the alias files (incl. page/1/ and en/)");
    let (d_ours, d_go) = (dangling(ours), dangling(&go));
    assert!(
        d_ours.is_subset(&d_go),
        "L2: links dangling only in our output: {:?}",
        d_ours.difference(&d_go).collect::<Vec<_>>()
    );
    // The testsite's layouts link to files neither build has.
    assert_eq!(
        d_ours.iter().map(String::as_str).collect::<Vec<_>>(),
        [
            "/docs/a/",
            "/favicon.ico",
            "/img/404.png",
            "/img/a.png",
            "/img/a@2x.png",
            "/img/x.png",
            "/js/app.js",
            "/unquoted/",
            "/upper",
            "/x"
        ]
    );

    // L3: visible text and heading IDs of every HTML page.
    let text_diffs: Vec<String> = go
        .iter()
        .filter(|(k, _)| k.ends_with(".html"))
        .filter(|(k, v)| {
            let (g, o) = (
                String::from_utf8_lossy(v),
                String::from_utf8_lossy(&ours[*k]),
            );
            visible_text(&g) != visible_text(&o) || heading_ids(&g) != heading_ids(&o)
        })
        .map(|(k, _)| k.clone())
        .collect();
    assert_eq!(
        text_diffs,
        accepted(ACCEPTED_TEXT_DIFFS),
        "L3: text differs"
    );

    println!(
        "A-T: L1 {}/55 paths; L2 {}/55 files byte-identical, links equal, {} aliases, {} dangling \
         links (all dangling in Go's output too); L3 {} HTML pages equal; structure oracle below",
        got.len(),
        go.len() - differ.len(),
        aliases,
        d_ours.len(),
        go.keys().filter(|k| k.ends_with(".html")).count() - text_diffs.len(),
    );

    // Structure oracle: the build's dump against Go's.
    let golden = ssg_testkit::fixture::read_json(
        &repo_dir().join("testdata/golden/testsite/structure.json"),
    )
    .expect("golden structure dump");
    let (want, got) = (structure_facts(&golden), structure_facts(&built.structure));
    let structure_diffs: BTreeSet<String> = want
        .keys()
        .chain(got.keys())
        .filter(|k| want.get(*k) != got.get(*k))
        .cloned()
        .collect();
    let unaccepted: Vec<String> = structure_diffs
        .difference(&accepted_structure())
        .map(|k| format!("{k}: go {:?}, rust {:?}", want.get(k), got.get(k)))
        .collect();
    assert!(
        unaccepted.is_empty(),
        "structure oracle: {} facts differ:\n{}",
        unaccepted.len(),
        unaccepted.join("\n")
    );
    println!(
        "A-T structure oracle: {}/{} facts equal (records, aliases, page/1 aliases, resources, pages)",
        want.len() - structure_diffs.len(),
        want.len()
    );

    // The full output tree, reviewed with `INSTA_UPDATE=always` plus `git diff`.
    let mut snap = String::new();
    let mut files: Vec<(String, &[u8])> = ours
        .iter()
        .map(|(k, v)| (format!("public/{k}"), v.as_slice()))
        .collect();
    files.sort();
    for (name, bytes) in files {
        snap.push_str(&format!("-- {name} --\n"));
        snap.push_str(&String::from_utf8_lossy(bytes));
        if !snap.ends_with('\n') {
            snap.push('\n');
        }
    }
    ssg_testkit::snapshot::settings().bind(|| {
        insta::assert_snapshot!("testsite_output", snap);
    });
}

/// The scanner and normalisations on inputs the testsite does not have.
#[test]
fn parity_helpers() {
    assert_eq!(
        normalize_path("css/main.0123456789abcdef.css"),
        "css/main.H.css"
    );
    assert_eq!(normalize_path("img/a_hu_0f3a_12.png"), "img/a_hu_H_12.png");
    assert_eq!(percent_decode("/th/%E0%B8%81/"), "/th/ก/");
    assert_eq!(decode_entities("a&amp;b&#39;c&#x41;&nope;"), "a&b'cA&nope;");
    assert_eq!(
        visible_text("<p>A &ldquo;b&rdquo;</p><script>x<y</script><!-- c --><style>s</style>\n d"),
        "A \"b\" d"
    );
    assert_eq!(
        heading_ids(r#"<h2 id="a">A</h2><h7 id="no"></h7><H3 ID='b'>B</H3>"#),
        ["a", "b"]
    );
    let l = links(
        "x.html",
        br#"<title>T</title><link rel="canonical" href="https://example.org/c/"><a href=/u/>u</a><img srcset="/a.png 1x, https://example.org/b.png 2x"><a href="//cdn/x">c</a>"#,
    );
    assert_eq!(l.title, ["T"]);
    assert_eq!(
        l.rel_links,
        [("canonical".to_owned(), "https://example.org/c/".to_owned())]
    );
    assert_eq!(
        l.internal.into_iter().collect::<Vec<_>>(),
        ["/a.png", "/b.png", "/c/", "/u/"]
    );
}
