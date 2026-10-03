//! The hooks oracle: every document rendered with recording hooks for all hook kinds; the
//! sequence of hook invocations and the fields each hook saw must match Go's.
//!
//! The recorder mirrors the oracle's (`mdoracle.Recorder`): each hook writes a marker
//! (`[L3|text]`, `[I4]`, `<bq5>…</bq>`, …) that nested hooks see in their `text`.

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Mutex;

use serde::Deserialize;
use ssg_base::{Map, PageId, Value};
use ssg_markup::{
    BlockquoteCtx, Cell, CodeBlockCtx, ExpandedMarkdown, HeadingCtx, HookEnv, HookError, HookOut,
    Hooks, ImageCtx, LinkCtx, SourceContexts, TableCtx, render, wrap_context,
};
use ssg_testkit::fixture::{GoString, oracle};

use super::{GoCfg, PAGE, Row, file, options, print, show, text};

#[derive(Deserialize)]
struct Doc {
    name: String,
    src: GoString,
}

#[derive(Deserialize)]
struct Result {
    doc: usize,
    cfg: usize,
    wrap: bool,
    page: String,
    records: Vec<GoString>,
    err: Option<String>,
    panic: Option<String>,
}

#[derive(Deserialize)]
struct Fixture {
    docs: Vec<Doc>,
    results: Vec<Result>,
}

type Record = (String, BTreeMap<String, String>);

/// Parses a `name:len:value` dump.
fn parse_record(s: &str) -> Record {
    let mut fields = BTreeMap::new();
    let mut rest = s;
    while let Some((name, tail)) = rest.split_once(':') {
        let Some((len, tail)) = tail.split_once(':') else {
            break;
        };
        let Ok(len) = len.parse::<usize>() else { break };
        let value = tail.get(..len).unwrap_or(tail);
        fields.insert(name.to_owned(), value.to_owned());
        rest = tail
            .get(len..)
            .unwrap_or("")
            .strip_prefix('\n')
            .unwrap_or("");
    }
    let kind = fields.remove("kind").unwrap_or_default();
    (kind, fields)
}

fn field(out: &mut String, name: &str, v: &str) {
    out.push_str(&format!("{name}:{}:{v}\n", v.len()));
}

fn dump(v: &Value) -> String {
    match v {
        Value::Null => "r:nil".into(),
        Value::Bool(b) => format!("b:{b}"),
        Value::Int(i) => format!("i:{i}"),
        Value::Float(f) => format!("f:{f}"),
        Value::String(s) => format!("s:{s}"),
        Value::Array(a) => {
            let mut o = String::from("r:");
            for r in a.iter() {
                if let Value::Array(p) = r {
                    let n: Vec<String> = p.iter().map(|x| dump(x)[2..].to_owned()).collect();
                    o.push_str(&format!("[{}]", n.join(",")));
                }
            }
            o
        }
        Value::Map(m) => dump_map(m),
        Value::Date(d) => format!("t:{d}"),
    }
}

fn dump_map(m: &Map) -> String {
    let mut o = String::from("m{");
    for (k, v) in m.iter() {
        field(&mut o, k, &dump(v));
    }
    o.push('}');
    o
}

fn dump_rows(rows: &[Vec<Cell>]) -> String {
    if rows.is_empty() {
        return "nil".into();
    }
    let mut o = String::new();
    for row in rows {
        o.push('[');
        for c in row {
            field(&mut o, c.alignment.as_str(), &c.text);
        }
        o.push(']');
    }
    o
}

struct Recorder {
    page: String,
    seq: Mutex<u32>,
    records: Mutex<Vec<Record>>,
}

impl Recorder {
    fn new(page: &str) -> Self {
        Self {
            page: page.to_owned(),
            seq: Mutex::new(0),
            records: Mutex::new(Vec::new()),
        }
    }

    fn record(&self, kind: &str, env: &HookEnv, fields: &[(&str, String)]) -> u32 {
        let mut seq = self.seq.lock().expect("seq");
        *seq += 1;
        let mut map: BTreeMap<String, String> = fields
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect();
        let inner = if env.inner_page == PAGE {
            format!("s:{}", self.page)
        } else {
            format!("s:inner:{}", env.inner_page)
        };
        map.insert("PageInner".into(), inner);
        self.records
            .lock()
            .expect("records")
            .push((kind.to_owned(), map));
        *seq
    }
}

fn link_fields(ctx: &LinkCtx) -> Vec<(&'static str, String)> {
    vec![
        ("Destination", ctx.destination.clone()),
        ("Title", ctx.title.clone()),
        ("Text", ctx.text.clone()),
        ("PlainText", ctx.plain_text.clone()),
        ("Attributes", dump_map(&ctx.attributes)),
    ]
}

fn err(msg: &str) -> HookError {
    HookError::new(msg.to_owned())
}

impl Hooks for Recorder {
    fn link(&self, env: &HookEnv, ctx: &LinkCtx) -> std::result::Result<HookOut, HookError> {
        let seq = self.record("link", env, &link_fields(ctx));
        if ctx.destination == "hook-error" {
            return Err(err("link hook failed"));
        }
        Ok(HookOut::Html(format!("[L{seq}|{}]", ctx.text)))
    }

    fn image(&self, env: &HookEnv, ctx: &ImageCtx) -> std::result::Result<HookOut, HookError> {
        let mut f = link_fields(ctx);
        f.push(("IsBlock", format!("b:{}", ctx.is_block)));
        f.push(("Ordinal", format!("i:{}", env.ordinal)));
        let seq = self.record("image", env, &f);
        Ok(HookOut::Html(format!("[I{seq}]")))
    }

    fn heading(&self, env: &HookEnv, ctx: &HeadingCtx) -> std::result::Result<HookOut, HookError> {
        self.record(
            "heading",
            env,
            &[
                ("Level", format!("i:{}", ctx.level)),
                ("Anchor", ctx.anchor.clone()),
                ("Text", ctx.text.clone()),
                ("PlainText", ctx.plain_text.clone()),
                ("Attributes", dump_map(&ctx.attributes)),
            ],
        );
        Ok(HookOut::Html(format!(
            "<h{l} id=\"{}\">{}</h{l}>\n",
            ctx.anchor,
            ctx.text,
            l = ctx.level
        )))
    }

    fn code_block(
        &self,
        env: &HookEnv,
        ctx: &CodeBlockCtx,
    ) -> std::result::Result<HookOut, HookError> {
        if ctx.lang == "nohook" {
            return Ok(HookOut::Default);
        }
        let seq = self.record(
            "codeblock",
            env,
            &[
                ("Type", ctx.lang.clone()),
                ("Inner", ctx.inner.clone()),
                ("Ordinal", format!("i:{}", env.ordinal)),
                ("Attributes", dump_map(&ctx.attributes)),
                ("Options", dump_map(&ctx.options)),
            ],
        );
        if ctx.lang == "errlang" {
            return Err(err("code block hook failed"));
        }
        Ok(HookOut::Html(format!(
            "<pre{seq} lang=\"{}\">{}</pre>\n",
            ctx.lang, ctx.inner
        )))
    }

    fn blockquote(
        &self,
        env: &HookEnv,
        ctx: &BlockquoteCtx,
    ) -> std::result::Result<HookOut, HookError> {
        let kind = match ctx.kind {
            ssg_markup::BlockquoteKind::Regular => "regular",
            ssg_markup::BlockquoteKind::Alert => "alert",
        };
        let seq = self.record(
            "blockquote",
            env,
            &[
                ("Type", kind.to_owned()),
                ("AlertType", ctx.alert_type.clone()),
                ("AlertTitle", ctx.alert_title.clone()),
                ("AlertSign", ctx.alert_sign.as_str().to_owned()),
                ("Text", ctx.text.clone()),
                ("Ordinal", format!("i:{}", env.ordinal)),
                ("Attributes", dump_map(&ctx.attributes)),
            ],
        );
        if ctx.text.contains("BQERR") {
            return Err(err("blockquote hook failed"));
        }
        Ok(HookOut::Html(format!("<bq{seq}>{}</bq>\n", ctx.text)))
    }

    fn table(&self, env: &HookEnv, ctx: &TableCtx) -> std::result::Result<HookOut, HookError> {
        self.record(
            "table",
            env,
            &[
                ("Ordinal", format!("i:{}", env.ordinal)),
                ("Attributes", dump_map(&ctx.attributes)),
                ("THead", dump_rows(&ctx.thead)),
                ("TBody", dump_rows(&ctx.tbody)),
            ],
        );
        Ok(HookOut::Default)
    }
}

/// Folds typographer output (entities and characters) and number types for comparison.
fn norm(field: &str, v: &str) -> String {
    let mut s = v.to_owned();
    for (from, to) in [
        ("&lsquo;", "'"),
        ("&rsquo;", "'"),
        ("&ldquo;", "\""),
        ("&rdquo;", "\""),
        ("&quot;", "\""),
        ("&ndash;", "--"),
        ("&mdash;", "---"),
        ("&hellip;", "..."),
        ("&laquo;", "<<"),
        ("&raquo;", ">>"),
        ("\u{2018}", "'"),
        ("\u{2019}", "'"),
        ("\u{201c}", "\""),
        ("\u{201d}", "\""),
        ("\u{2013}", "--"),
        ("\u{2014}", "---"),
        ("\u{2026}", "..."),
    ] {
        s = s.replace(from, to);
    }
    if field == "Attributes" || field == "Options" {
        // goldmark numbers are float64; `f:3` is the integer 3.
        s = s
            .split('\n')
            .map(|l| match l.rsplit_once(":f:") {
                Some((head, n)) if !n.contains(['.', 'e']) => format!("{head}:i:{n}"),
                _ => l.to_owned(),
            })
            .collect::<Vec<_>>()
            .join("\n");
    }
    s
}

/// The oracle's wrapped document (the Go implementation's context markers around the source as
/// page 7, then around a fixed document as page 9) with this crate's context markers: the
/// original source becomes a context span of page 7 (page 9 of the oracle has no page, so it
/// is not a span).
fn wrapped(src: &str) -> (String, Range<usize>) {
    let mut s = String::from("Intro *text*\n\n");
    let (w, inner) = wrap_context(src);
    let span = s.len() + inner.start..s.len() + inner.end;
    s.push_str(&w);
    s.push_str("\nOutro [l](x)\n\n");
    s.push_str(&wrap_context("## Nil lookup\n\n![i](n.png)\n").0);
    (s, span)
}

const COMPARED: &[&str] = &[
    "Destination",
    "Title",
    "Text",
    "PlainText",
    "Attributes",
    "IsBlock",
    "Ordinal",
    "Level",
    "Anchor",
    "Type",
    "Inner",
    "Options",
    "AlertType",
    "AlertTitle",
    "AlertSign",
    "THead",
    "TBody",
];

#[test]
fn hook_invocations_and_fields() {
    let fx: Fixture = oracle("oracle/markup/hooks/hooks.json.gz");
    let file = file();
    let mut by_cfg: BTreeMap<usize, [usize; 7]> = BTreeMap::new();
    let mut per_field: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    let (mut inner_ok, mut inner_total) = (0, 0);
    for r in &fx.results {
        let doc = &fx.docs[r.doc];
        let src = text(&doc.src);
        let cfg = GoCfg::ALL[r.cfg];
        let o = options(cfg);
        let (md, spans) = if r.wrap {
            let (md, span) = wrapped(&src);
            (md, vec![(span, PageId::from_raw(7))])
        } else {
            (src.clone(), Vec::new())
        };
        let contexts = SourceContexts(spans);
        let rec = Recorder::new(&r.page);
        let input = ExpandedMarkdown {
            text: &md,
            page: PAGE,
            contexts: &contexts,
            file: &file,
        };
        let out = render(&input, &o, &rec, None);
        let want: Vec<Record> = r.records.iter().map(|g| parse_record(&text(g))).collect();
        let got = rec.records.into_inner().expect("records");
        let c = by_cfg.entry(r.cfg).or_default();
        // [results, same invocations, records, records equal, errors expected, errors seen, _]
        c[0] += 1;
        let kinds = |v: &[Record]| v.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>();
        if kinds(&want) == kinds(&got) {
            c[1] += 1;
        } else {
            show(
                "invocations",
                &doc.name,
                &kinds(&want).join(","),
                &kinds(&got).join(","),
            );
        }
        c[2] += want.len();
        for ((wk, wf), (gk, gf)) in want.iter().zip(&got) {
            if wk != gk {
                continue;
            }
            let mut all = true;
            for name in COMPARED {
                // Only code blocks have options (always empty elsewhere in Go).
                if *name == "Options" && wk != "codeblock" {
                    continue;
                }
                let Some(w) = wf.get(*name) else { continue };
                let g = gf.get(*name).map_or("<missing>", String::as_str);
                let e = per_field.entry(name).or_default();
                e.1 += 1;
                if norm(name, w) == norm(name, g) {
                    e.0 += 1;
                } else {
                    all = false;
                    show(
                        &format!("field {wk}.{name}"),
                        &format!("{} cfg {}", doc.name, cfg.name()),
                        &norm(name, w),
                        &norm(name, g),
                    );
                }
            }
            if let (Some(w), Some(g)) = (wf.get("PageInner"), gf.get("PageInner")) {
                inner_total += 1;
                if w == g {
                    inner_ok += 1;
                } else {
                    show("PageInner", &doc.name, w, g);
                }
            }
            c[3] += usize::from(all);
        }
        if r.err.is_some() || r.panic.is_some() {
            c[4] += 1;
            c[5] += usize::from(out.is_err());
        }
    }
    let mut rows = Vec::new();
    for (cfg, c) in &by_cfg {
        let name = GoCfg::ALL[*cfg].name();
        rows.push(Row::new(
            format!("same hook invocations, cfg {name}"),
            c[1],
            c[0],
        ));
        rows.push(Row::new(
            format!("records with all fields equal, cfg {name}"),
            c[3],
            c[2],
        ));
        rows.push(Row::new(
            format!("failing conversions that fail, cfg {name}"),
            c[5],
            c[4],
        ));
    }
    let (mut fields_ok, mut fields) = (0, 0);
    for (name, (ok, total)) in &per_field {
        rows.push(Row::new(format!("field {name}"), *ok, *total));
        fields_ok += ok;
        fields += total;
    }
    rows.push(Row::new("all fields", fields_ok, fields));
    rows.push(Row::new("PageInner (context spans)", inner_ok, inner_total));
    print("Hook invocations and fields (hooks oracle)", &rows);
    let invocations: (usize, usize) = by_cfg
        .values()
        .fold((0, 0), |a, c| (a.0 + c[1], a.1 + c[0]));
    assert!(
        invocations.0 * 100 >= invocations.1 * 98,
        "invocations {invocations:?}"
    );
    assert!(
        fields_ok * 100 >= fields * 98,
        "fields {fields_ok}/{fields}"
    );
}
