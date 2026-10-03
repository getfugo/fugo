//! [`HtmlElements`] (the names `purge_css` keeps) against the Go oracle `publisher/collector`
//! (`htmlElementsCollector`, which wrote the Go build's stats file): single element strings,
//! whole documents (per document and per group through one collector) and multi-write streams,
//! under the oracle's five configurations (each leaves out some of the lists).

use std::collections::BTreeMap;

use serde::Deserialize;
use ssg_publish::HtmlElements;
use ssg_testkit::fixture::{GoString, oracle_lines};

use crate::support::{Tally, show};

#[derive(Deserialize)]
struct Record {
    t: String,
    conf: Option<String>,
    g: Option<String>,
    // `el`
    s: Option<GoString>,
    tag: Option<GoString>,
    classes: Option<Vec<GoString>>,
    ids: Option<Vec<GoString>>,
    // `doc`, `group`, `chunks`
    doc: Option<GoString>,
    chunks: Option<Vec<GoString>>,
    want: Option<Want>,
    n: Option<usize>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Want {
    Lists {
        tags: Option<Vec<GoString>>,
        classes: Option<Vec<GoString>>,
        ids: Option<Vec<GoString>>,
    },
    /// `closed` records (`isClosedByTag`, a private helper of Go's scanner).
    Bool(#[allow(dead_code, reason = "only the record type is used")] bool),
}

/// The sorted lists of the Go stats file; a left-out or empty list is `None`.
#[derive(Debug, PartialEq, Eq)]
struct StatsLists {
    tags: Option<Vec<String>>,
    classes: Option<Vec<String>>,
    ids: Option<Vec<String>>,
}

/// The lists an oracle configuration leaves out.
#[derive(Clone, Copy, Default)]
struct Disabled {
    tags: bool,
    classes: bool,
    ids: bool,
}

impl StatsLists {
    fn new(found: HtmlElements, c: Disabled) -> Self {
        let list = |set: std::collections::BTreeSet<String>, disabled: bool| {
            (!disabled && !set.is_empty()).then(|| set.into_iter().collect())
        };
        Self {
            tags: list(found.tags, c.tags),
            classes: list(found.classes, c.classes),
            ids: list(found.ids, c.ids),
        }
    }
}

fn conf(name: &str) -> Disabled {
    let mut c = Disabled::default();
    match name {
        "all" => {}
        "noids" => c.ids = true,
        "noclasses" => c.classes = true,
        "notags" => c.tags = true,
        "classesonly" => {
            c.tags = true;
            c.ids = true;
        }
        _ => panic!("conf {name}"),
    }
    c
}

fn strings(v: Option<&Vec<GoString>>) -> Option<Vec<String>> {
    v.map(|l| {
        l.iter()
            .map(|s| String::from_utf8_lossy(&s.0).into_owned())
            .collect()
    })
}

/// What the collector sees of a Go document: Go stops at the first byte that is not UTF-8 (and
/// at a literal U+FFFD).
fn text(s: &GoString) -> (String, bool) {
    let lossy = String::from_utf8_lossy(&s.0);
    let clean = std::str::from_utf8(&s.0).is_ok() && !lossy.contains('\u{fffd}');
    (lossy.into_owned(), clean)
}

fn stats(html: &str, c: Disabled) -> StatsLists {
    StatsLists::new(HtmlElements::collect(html), c)
}

/// The accepted class of a difference: the Go collector feeds each element string alone to
/// x/net/html's tree builder in a body context, so the result depends on that algorithm's
/// insertion modes rather than on the element.
fn classify(html: &str, want: &StatsLists, got: &StatsLists, clean_utf8: bool) -> &'static str {
    let lower = html.to_ascii_lowercase();
    let has = |needle: &str| lower.contains(needle);
    let tags_only_differ = want.classes == got.classes && want.ids == got.ids;
    if !clean_utf8 {
        "invalid-utf8"
    } else if has("<!") || has("<?") || has("<<") || has("</ ") || has("< ") {
        // CDATA, doctypes and processing instructions become tags (`![cdata[x]]`, `?php`) or
        // hide the rest of the document from Go's scanner.
        "markup-declarations"
    } else if [
        "<th",
        "<caption",
        "<col",
        "<frame",
        "<head",
        "<image",
        "<tbody",
        "<thead",
        "<tfoot",
        "<tr",
        "<td",
        "<select",
        "<template",
        "<svg",
        "<math",
        "<table",
    ]
    .iter()
    .any(|t| has(t))
    {
        // Elements the tree builder drops, renames or moves out of a body context.
        "tree-builder-context"
    } else if has("<pre") || has("<textarea") || has("<script") || has("<style") {
        // Go skips everything up to the closing tag by its own quote-aware scan (an unclosed
        // quote in the skipped text, or a `<prefix…>` tag such as `<preload>`, changes it).
        "raw-text-skip"
    } else if html.matches('"').count() % 2 == 1 || html.matches('\'').count() % 2 == 1 {
        // Go's scanner tracks quotes across elements and text.
        "unbalanced-quotes"
    } else if html.split('<').skip(1).any(|t| {
        t.split(|c: char| c.is_whitespace() || c == '>' || c == '/')
            .next()
            .is_some_and(|name| !name.is_ascii())
    }) {
        // Go lower-cases a tag name with Unicode rules but x/net/html with ASCII rules, and
        // then finds no element of that name: the tag is kept, its classes and ids are lost.
        "non-ascii-tag-name"
    } else if html.contains('\0') || html.contains("&#") {
        // HTML replaces NUL and invalid numeric references (`&#0;`, `&#xD800;`) with U+FFFD
        // and decodes `&#x;`-like junk differently from x/net/html.
        "nul-or-numeric-reference"
    } else if repeats_attribute(&lower) {
        // x/net/html keeps every copy of a repeated attribute; HTML keeps the first.
        "repeated-attribute"
    } else if tags_only_differ {
        "tag-name-spelling"
    } else {
        "attribute-parsing"
    }
}

/// Whether a tag of `lower` (lower-cased HTML) has `class`, `id` or a class binding twice.
fn repeats_attribute(lower: &str) -> bool {
    lower.split('<').any(|tag| {
        let tag = tag.split('>').next().unwrap_or_default();
        let names: Vec<&str> = tag
            .split(|c: char| c.is_whitespace() || c == '/')
            .map(|a| a.split('=').next().unwrap_or_default())
            .collect();
        ["class", "id"]
            .iter()
            .any(|a| names.iter().filter(|n| *n == a).count() > 1)
            || ["transition", ":class"]
                .iter()
                .any(|a| names.iter().filter(|n| n.contains(a)).count() > 1)
    })
}

/// A list as the Go stats file has it: sorted, without duplicates, `None` when empty.
fn sorted(v: Option<Vec<String>>) -> Option<Vec<String>> {
    let mut v = v?;
    v.sort();
    v.dedup();
    (!v.is_empty()).then_some(v)
}

#[test]
fn collector_oracle() {
    let recs: Vec<Record> = oracle_lines("oracle/publisher/collector/collector.jsonl.gz");
    let mut families: BTreeMap<&str, Tally> = BTreeMap::new();
    let mut group_docs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut closed = 0;
    for r in &recs {
        let c = r.conf.as_deref().map(conf);
        match r.t.as_str() {
            "el" => {
                let c = c.expect("conf");
                let (html, clean) = text(r.s.as_ref().expect("s"));
                let tag = r
                    .tag
                    .as_ref()
                    .map(|t| String::from_utf8_lossy(&t.0).into_owned());
                let want = StatsLists {
                    tags: tag.map(|t| vec![t]),
                    classes: sorted(strings(r.classes.as_ref())),
                    ids: sorted(strings(r.ids.as_ref())),
                };
                let got = stats(&html, c);
                let t = families.entry("el").or_default();
                if got == want {
                    t.pass();
                } else {
                    let class = classify(&html, &want, &got, clean);
                    debug_sample("el", class, &html, &got, &want);
                    t.accept(class);
                }
            }
            "doc" | "group" | "chunks" => {
                let c = c.expect("conf");
                let Some(Want::Lists { tags, classes, ids }) = &r.want else {
                    panic!("want")
                };
                let want = StatsLists {
                    tags: strings(tags.as_ref()),
                    classes: strings(classes.as_ref()),
                    ids: strings(ids.as_ref()),
                };
                let (html, clean) = match r.t.as_str() {
                    "doc" => {
                        let (html, clean) = text(r.doc.as_ref().expect("doc"));
                        if r.conf.as_deref() == Some("all") {
                            group_docs
                                .entry(r.g.clone().expect("g"))
                                .or_default()
                                .push(html.clone());
                        }
                        (html, clean)
                    }
                    "group" => {
                        let docs = &group_docs[r.g.as_deref().expect("g")];
                        assert_eq!(Some(docs.len()), r.n);
                        // Documents are scanned one by one; joining them with a newline keeps
                        // one document's open elements from swallowing the next.
                        let mut found = HtmlElements::default();
                        for d in docs {
                            found.add_html(d);
                        }
                        let got = StatsLists::new(found, c);
                        let t = families.entry("group").or_default();
                        if got == want {
                            t.pass();
                        } else {
                            t.accept(match r.g.as_deref() {
                                Some("upstream") => "group-upstream",
                                Some("hand") => "group-hand",
                                Some("html5lib") => "group-html5lib",
                                Some("elements") => "group-elements",
                                _ => "group-random",
                            });
                        }
                        continue;
                    }
                    _ => {
                        let chunks = r.chunks.as_deref().unwrap_or_default();
                        let bytes: Vec<u8> = chunks.iter().flat_map(|c| c.0.clone()).collect();
                        text(&GoString(bytes))
                    }
                };
                let got = stats(&html, c);
                let family = match (r.t.as_str(), r.g.as_deref()) {
                    ("doc", Some("upstream")) => "doc-upstream",
                    ("doc", Some("hand")) => "doc-hand",
                    ("doc", Some("html5lib")) => "doc-html5lib",
                    ("doc", Some("elements")) => "doc-elements",
                    ("doc", _) => "doc-random",
                    _ => "chunks",
                };
                let t = families.entry(family).or_default();
                if got == want {
                    t.pass();
                } else if family == "doc-upstream" {
                    t.fail(|| {
                        format!(
                            "{}\n    got  {got:?}\n    want {want:?}",
                            show(&GoString(html.into_bytes()))
                        )
                    });
                } else {
                    // Go's writer stops at a character split across two writes.
                    let split_char = r.chunks.as_ref().is_some_and(|chunks| {
                        chunks.iter().any(|c| std::str::from_utf8(&c.0).is_err())
                    });
                    let class = if split_char && clean {
                        "character-split-across-writes"
                    } else {
                        classify(&html, &want, &got, clean)
                    };
                    debug_sample(family, class, &html, &got, &want);
                    t.accept(class);
                }
            }
            "closed" => closed += 1,
            other => panic!("record type {other}"),
        }
    }
    // `closed` records test `isClosedByTag`, a private helper of Go's scanner with no
    // counterpart here; the skipping it implements is covered by the documents.
    assert_eq!(closed, 3022);
    for (family, t) in &families {
        t.print(&format!("collector-{family}"));
    }
    for (family, t) in &families {
        t.verify(&format!("collector-{family}"));
    }
}

/// `<PREFIX>_SAMPLES=<family>:<class>` prints the differences of one class.
fn debug_sample(family: &str, class: &str, html: &str, got: &StatsLists, want: &StatsLists) {
    if std::env::var(ssg_base::env_var!("SAMPLES")).is_ok_and(|v| v == format!("{family}:{class}"))
    {
        eprintln!("{html:?}\n    got  {got:?}\n    want {want:?}");
    }
}
