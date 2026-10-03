//! The input: the 959 docs bodies of the `markup/convert` and `markup/hooks` oracles (the Go
//! implementation's goldmark converter, six configurations).

use std::collections::BTreeMap;

use serde::Deserialize;
use ssg_testkit::fixture::{GoString, oracle};

/// The Go implementation's markup configurations of the convert oracle, in fixture order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum GoCfg {
    Default,
    Site,
    Ascii,
    Blackfriday,
    Cjk,
    Noattr,
}

impl GoCfg {
    pub const ALL: [Self; 6] = [
        Self::Default,
        Self::Site,
        Self::Ascii,
        Self::Blackfriday,
        Self::Cjk,
        Self::Noattr,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Site => "site",
            Self::Ascii => "ascii",
            Self::Blackfriday => "blackfriday",
            Self::Cjk => "cjk",
            Self::Noattr => "noattr",
        }
    }
}

pub struct DocsCorpus {
    /// `(name, markdown body)` of the 959 `docs/content` files.
    pub docs: Vec<(String, String)>,
    /// The Go implementation's HTML per `[doc][cfg]`.
    pub html: Vec<[String; 6]>,
    /// Decoded hook records of the `site` configuration, per doc.
    pub hooks: Vec<Vec<HookRecord>>,
}

/// One hook invocation of the hooks oracle (`kind` plus its `Field` lines).
pub struct HookRecord {
    pub kind: String,
    pub fields: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct Doc {
    name: String,
    src: GoString,
}

#[derive(Deserialize)]
struct Convert {
    docs: Vec<Doc>,
    results: Vec<ConvertResult>,
}

#[derive(Deserialize)]
struct ConvertResult {
    doc: usize,
    cfg: usize,
    html: Option<GoString>,
    html_same: Option<usize>,
}

#[derive(Deserialize)]
struct Hooks {
    docs: Vec<Doc>,
    results: Vec<HooksResult>,
}

#[derive(Deserialize)]
struct HooksResult {
    doc: usize,
    cfg: usize,
    records: Option<Vec<GoString>>,
}

fn text(s: &GoString) -> String {
    String::from_utf8_lossy(&s.0).into_owned()
}

pub fn docs() -> DocsCorpus {
    let convert: Convert = oracle("oracle/markup/convert/convert.json.gz");
    let keep: Vec<usize> = (0..convert.docs.len())
        .filter(|&i| convert.docs[i].name.starts_with("docs/content/"))
        .collect();
    let mut html: Vec<[String; 6]> = vec![Default::default(); convert.docs.len()];
    let mut same = Vec::new();
    for r in &convert.results {
        match (&r.html, r.html_same) {
            (Some(h), _) => html[r.doc][r.cfg] = text(h),
            (None, Some(k)) => same.push((r.doc, r.cfg, k)),
            (None, None) => {}
        }
    }
    for (doc, cfg, k) in same {
        html[doc][cfg] = html[doc][k].clone();
    }

    let hooks_fixture: Hooks = oracle("oracle/markup/hooks/hooks.json.gz");
    let mut hooks: Vec<Vec<HookRecord>> = (0..convert.docs.len()).map(|_| Vec::new()).collect();
    for r in hooks_fixture.results {
        let site = r.cfg == 1
            && convert
                .docs
                .get(r.doc)
                .is_some_and(|d| d.name == hooks_fixture.docs[r.doc].name);
        if site {
            hooks[r.doc] = r
                .records
                .unwrap_or_default()
                .iter()
                .map(|g| record(&text(g)))
                .collect();
        }
    }

    DocsCorpus {
        docs: keep
            .iter()
            .map(|&i| (convert.docs[i].name.clone(), text(&convert.docs[i].src)))
            .collect(),
        html: keep.iter().map(|&i| html[i].clone()).collect(),
        hooks: keep
            .iter()
            .map(|&i| std::mem::take(&mut hooks[i]))
            .collect(),
    }
}

/// Parses the oracle's `name:len:value` field dump (values may span lines; `len` is in bytes).
fn record(s: &str) -> HookRecord {
    let mut fields = BTreeMap::new();
    let mut rest = s;
    while let Some((name, tail)) = rest.split_once(':') {
        let Some((len, tail)) = tail.split_once(':') else {
            break;
        };
        let Ok(len) = len.parse::<usize>() else {
            break;
        };
        let value = tail.get(..len).unwrap_or(tail);
        fields.insert(name.to_owned(), value.to_owned());
        rest = tail
            .get(len..)
            .unwrap_or("")
            .strip_prefix('\n')
            .unwrap_or("");
    }
    HookRecord {
        kind: fields.remove("kind").unwrap_or_default(),
        fields,
    }
}
