//! Per-feature measurements: native comrak 0.55 against the Go implementation's goldmark output.

use std::collections::BTreeMap;

use comrak::nodes::{AstNode, NodeValue};
use comrak::{Arena, Options, parse_document};

use super::Row;
use super::corpus::{DocsCorpus, GoCfg};
use super::engine::{go_options, to_html, to_html_passes};
use super::normalize::{Fold, Tok, elements, multiset_matches, normalize, tokens};

const FOLD: Fold = Fold {
    auto_ids: true,
    typography: false,
    footnotes: false,
};
const KEEP_IDS: Fold = Fold {
    auto_ids: false,
    typography: false,
    footnotes: false,
};

/// Byte offsets of line starts, for 1-based comrak line/column (bytes) positions.
pub struct Lines(Vec<usize>);

impl Lines {
    pub fn new(src: &str) -> Self {
        let mut v = vec![0];
        v.extend(src.match_indices('\n').map(|(i, _)| i + 1));
        Self(v)
    }

    pub fn offset(&self, line: usize, column: usize) -> Option<usize> {
        Some(self.0.get(line.checked_sub(1)?)? + column.checked_sub(1)?)
    }

    pub fn line<'s>(&self, src: &'s str, line: usize) -> &'s str {
        let start = self.0.get(line - 1).copied().unwrap_or(src.len());
        let end = self.0.get(line).map_or(src.len(), |e| e - 1);
        src.get(start..end).unwrap_or("")
    }
}

fn walk<'a>(root: &'a AstNode<'a>, mut f: impl FnMut(&'a AstNode<'a>)) {
    for n in root.descendants() {
        f(n);
    }
}

/// Lines covered by code blocks and HTML blocks (never markdown).
fn verbatim_lines<'a>(root: &'a AstNode<'a>) -> Vec<(usize, usize)> {
    let mut v = Vec::new();
    walk(root, |n| {
        let d = n.data();
        if matches!(d.value, NodeValue::CodeBlock(_) | NodeValue::HtmlBlock(_)) {
            v.push((d.sourcepos.start.line, d.sourcepos.end.line));
        }
    });
    v
}

// ───────────────────────────── whole documents ─────────────────────────────

pub fn overall_docs(c: &DocsCorpus, cfg: GoCfg) -> Row {
    let o = go_options(cfg);
    let i = cfg as usize;
    let label = format!("docs, whole page, cfg {}", cfg.name());
    let matched = c
        .docs
        .iter()
        .zip(&c.html)
        .filter(|((name, md), html)| {
            let (want, got) = (normalize(&html[i], FOLD), normalize(&to_html(md, &o), FOLD));
            show_first_difference(&label, name, &want, &got);
            want == got
        })
        .count();
    Row::new(label, "pages", matched, c.docs.len())
}

/// Whole pages once the differences owned by planned passes are folded away: HTML comments
/// dropped under `unsafe = false` (a real pass, run here), typography folded to ASCII and
/// footnote markup dropped (T22 renders both). What is left is structural.
pub fn residual_docs(c: &DocsCorpus, cfg: GoCfg) -> Row {
    let o = go_options(cfg);
    let fold = Fold {
        auto_ids: true,
        typography: true,
        footnotes: true,
    };
    let label = format!(
        "docs, whole page, cfg {}, after pass-owned folds",
        cfg.name()
    );
    let matched = c
        .docs
        .iter()
        .zip(&c.html)
        .filter(|((name, md), html)| {
            let (want, got) = (
                normalize(&html[cfg as usize], fold),
                normalize(&to_html_passes(md, &o), fold),
            );
            show_first_difference(&label, name, &want, &got);
            want == got
        })
        .count();
    Row::new(label, "pages", matched, c.docs.len())
}

/// Element-level multiset match of every `<tag>` element, in one configuration.
fn element_row(c: &DocsCorpus, cfg: GoCfg, tag: &str, label: &str, o: &Options<'_>) -> Row {
    let (mut matched, mut total, mut pages, mut pages_ok) = (0, 0, 0, 0);
    for ((name, md), html) in c.docs.iter().zip(&c.html) {
        let want = elements(&tokens(&html[cfg as usize], FOLD), tag);
        if want.is_empty() {
            continue;
        }
        let got = elements(&tokens(&to_html(md, o), FOLD), tag);
        show(label, name, &want, &got);
        let m = multiset_matches(&want, &got);
        pages += 1;
        pages_ok += usize::from(m == want.len() && got.len() == want.len());
        matched += m;
        total += want.len();
    }
    Row::new(
        label.to_owned(),
        &format!("<{tag}> elements"),
        matched,
        total,
    )
    .note(format!("pages all-equal {pages_ok}/{pages}"))
}

/// `FUGO_SPIKE_SHOW=<feature label substring>` prints the differing items of that feature.
pub fn show(label: &str, page: &str, want: &[String], got: &[String]) {
    let Ok(filter) = std::env::var("FUGO_SPIKE_SHOW") else {
        return;
    };
    if filter.is_empty() || !label.contains(&filter) {
        return;
    }
    for (i, w) in want.iter().enumerate() {
        if !got.contains(w) {
            let g = got.get(i).map_or("<none>", String::as_str);
            eprintln!("--- {label} | {page} #{i}\n  goldmark: {w}\n  comrak:   {g}");
        }
    }
}

fn show_first_difference(label: &str, page: &str, want: &str, got: &str) {
    if want == got {
        return;
    }
    let at = want
        .char_indices()
        .zip(got.chars())
        .find(|((_, a), b)| a != b)
        .map_or(want.len().min(got.len()), |((i, _), _)| i);
    let cut = |s: &str| -> String {
        let mut start = at.saturating_sub(60).min(s.len());
        while !s.is_char_boundary(start) {
            start -= 1;
        }
        s[start..].chars().take(160).collect()
    };
    show(label, page, &[cut(want)], &[cut(got)]);
}

// ───────────────────────────── definition lists ─────────────────────────────

pub fn deflists(c: &DocsCorpus) -> Vec<Row> {
    let o = go_options(GoCfg::Default);
    let mut rows = vec![
        element_row(c, GoCfg::Default, "dl", "definition lists", &o),
        element_row(c, GoCfg::Default, "dd", "definition details", &o),
    ];
    // Tight details: goldmark writes `<dd>text</dd>` without `<p>`.
    let (mut tight, mut tight_ok) = (0, 0);
    for ((_, md), html) in c.docs.iter().zip(&c.html) {
        let want: Vec<String> = elements(&tokens(&html[0], FOLD), "dd")
            .into_iter()
            .filter(|e| !e.starts_with("<dd><p>"))
            .collect();
        if want.is_empty() {
            continue;
        }
        let got = elements(&tokens(&to_html(md, &o), FOLD), "dd");
        tight += want.len();
        tight_ok += multiset_matches(&want, &got);
    }
    rows.push(Row::new(
        "tight definition details".into(),
        "<dd> without <p>",
        tight_ok,
        tight,
    ));
    rows
}

// ───────────────────────────── heading attributes ─────────────────────────────

/// `(open tag, normalised inner HTML)` of every `h1`–`h6`, in document order.
fn heading_tags(toks: &[Tok]) -> Vec<(String, String)> {
    let mut v = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        if let Tok::Open { name, tag } = &toks[i]
            && name.len() == 2
            && name.starts_with('h')
            && name != "hr"
        {
            let mut j = i;
            while j < toks.len() && toks[j] != Tok::Close(name.clone()) {
                j += 1;
            }
            let inner = super::normalize::serialize(&toks[i + 1..j.min(toks.len())]);
            v.push((tag.clone(), inner));
            i = j;
        }
        i += 1;
    }
    v
}

pub fn heading_attributes(c: &DocsCorpus) -> Row {
    let o = go_options(GoCfg::Default);
    let (mut total, mut matched, mut parsed, mut false_pos) = (0, 0, 0, 0);
    for ((_, md), html) in c.docs.iter().zip(&c.html) {
        let arena = Arena::new();
        let root = parse_document(&arena, md, &o);
        let lines = Lines::new(md);
        let mut ours = Vec::new();
        walk(root, |n| {
            let d = n.data();
            if let NodeValue::Heading(h) = &d.value {
                let src = lines.line(md, d.sourcepos.start.line).trim_end();
                let explicit = !h.setext && src.ends_with('}') && src.contains('{');
                ours.push((h.level, explicit, d.attrs.clone()));
            }
        });
        let want = heading_tags(&tokens(&html[0], KEEP_IDS));
        let got = heading_tags(&tokens(&to_html(md, &o), KEEP_IDS));
        for (k, (level, explicit, attrs)) in ours.iter().enumerate() {
            if !explicit {
                if attrs.is_some() {
                    false_pos += 1;
                }
                continue;
            }
            total += 1;
            parsed += usize::from(attrs.is_some());
            let (Some((want_tag, want_inner)), Some((_, got_inner))) = (want.get(k), got.get(k))
            else {
                continue;
            };
            let mut pairs: Vec<(String, String)> = Vec::new();
            if let Some(a) = attrs {
                if let Some(id) = &a.id {
                    pairs.push(("id".into(), id.clone()));
                }
                if !a.classes.is_empty() {
                    pairs.push(("class".into(), a.classes.join(" ")));
                }
                pairs.extend(a.pairs.iter().cloned());
            }
            let has_id = pairs.iter().any(|(k, _)| k == "id");
            pairs.sort();
            let mut tag = format!("<h{level}");
            for (k, v) in &pairs {
                tag.push_str(&format!(" {k}=\"{}\"", v.replace('"', "&quot;")));
            }
            tag.push('>');
            // An auto id (no `#id` given) is the heading-id pass's job, not compared here.
            let want_tag = if has_id {
                want_tag.clone()
            } else {
                strip_id(want_tag)
            };
            if want_tag == tag && want_inner == got_inner {
                matched += 1;
            }
        }
    }
    Row::new("heading attributes {#id .class k=v}".into(), "headings", matched, total).note(format!(
        "comrak parsed {parsed}/{total}; attrs where goldmark saw none: {false_pos}; attrs are AST-only (not rendered)"
    ))
}

fn strip_id(tag: &str) -> String {
    let Some(start) = tag.find(" id=\"") else {
        return tag.to_owned();
    };
    let end = tag[start + 5..]
        .find('"')
        .map_or(tag.len(), |e| start + 5 + e + 1);
    format!("{}{}", &tag[..start], &tag[end..])
}

// ───────────────────────────── block attributes ─────────────────────────────

/// A `{…}` attribute line, possibly inside blockquotes (`> {.class}`).
fn attr_line(line: &str) -> Option<&str> {
    let t = line
        .trim_start_matches(|c: char| c == '>' || c.is_whitespace())
        .trim_end();
    is_attr_line(t).then_some(t)
}

fn is_attr_line(line: &str) -> bool {
    let t = line.trim();
    t.len() > 2
        && t.starts_with('{')
        && t.ends_with('}')
        && t[1..].starts_with(|c: char| c == '#' || c == '.' || c.is_ascii_alphabetic())
        && !t.starts_with("{{")
}

pub fn block_attributes(c: &DocsCorpus) -> Row {
    let o = go_options(GoCfg::Ascii);
    let (mut total, mut goldmark_used, mut comrak_used) = (0, 0, 0);
    let mut after: BTreeMap<&'static str, usize> = BTreeMap::new();
    for ((_, md), html) in c.docs.iter().zip(&c.html) {
        let arena = Arena::new();
        let root = parse_document(&arena, md, &o);
        let verbatim = verbatim_lines(root);
        let want_text = text_of(&html[GoCfg::Ascii as usize]);
        let got_text = text_of(&to_html(md, &o));
        for (i, line) in md.lines().enumerate() {
            let n = i + 1;
            let Some(lit) = attr_line(line) else {
                continue;
            };
            if verbatim.iter().any(|&(a, b)| (a..=b).contains(&n)) {
                continue;
            }
            total += 1;
            goldmark_used += usize::from(!want_text.contains(lit));
            comrak_used += usize::from(!got_text.contains(lit));
            // Which comrak block swallowed the line?
            let mut kind = "other";
            walk(root, |node| {
                let d = node.data();
                if d.sourcepos.start.line <= n && n <= d.sourcepos.end.line {
                    kind = match d.value {
                        NodeValue::Paragraph => "paragraph",
                        NodeValue::TableRow(_) => "table row",
                        NodeValue::Heading(_) => "heading",
                        _ => kind,
                    };
                }
            });
            *after.entry(kind).or_default() += 1;
        }
    }
    Row::new(
        "block attributes (attribute.block, cfg ascii)".into(),
        "{…} lines consumed",
        comrak_used,
        total,
    )
    .note(format!(
        "goldmark consumed {goldmark_used}; comrak keeps them as {after:?}"
    ))
}

/// Decoded text of the page, whitespace-collapsed (for "does this literal survive?").
/// Typography is folded so that a literal is found whether or not a typographer touched it.
fn text_of(html: &str) -> String {
    let fold = Fold {
        typography: true,
        ..FOLD
    };
    let mut s = String::new();
    for t in tokens(html, fold) {
        match t {
            Tok::Text(x) => s.push_str(&x),
            Tok::Code { text, .. } => s.push_str(&text),
            _ => s.push(' '),
        }
    }
    s.split_ascii_whitespace().collect::<Vec<_>>().join(" ")
}

// ───────────────────────────── fenced code ─────────────────────────────

pub fn fences(c: &DocsCorpus) -> Vec<Row> {
    let o = go_options(GoCfg::Site);
    let mut with_attrs = go_options(GoCfg::Site);
    with_attrs.extension.fenced_code_attributes = true;
    let (mut total, mut lang_ok, mut inner_ok) = (0, 0, 0);
    let (mut attr_total, mut attr_ranges, mut attr_comrak_ok) = (0, 0, 0);
    for ((name, md), hooks) in c.docs.iter().zip(&c.hooks) {
        let want: Vec<_> = hooks.iter().filter(|r| r.kind == "codeblock").collect();
        let arena = Arena::new();
        let got = code_blocks(parse_document(&arena, md, &o));
        let arena2 = Arena::new();
        let parsed = code_blocks(parse_document(&arena2, md, &with_attrs));
        total += want.len();
        if got.len() != want.len() {
            let w: Vec<String> = want
                .iter()
                .map(|r| r.fields.get("Type").cloned().unwrap_or_default())
                .collect();
            let g: Vec<String> = got.iter().map(|b| b.0.clone()).collect();
            show(
                "fence",
                name,
                &[format!("{} blocks {w:?}", w.len())],
                &[format!("{} blocks {g:?}", g.len())],
            );
            continue;
        }
        for ((w, (info, literal, _)), (_, _, attrs)) in want.iter().zip(&got).zip(&parsed) {
            let lang = info
                .split(|c: char| c.is_whitespace() || c == '{')
                .next()
                .unwrap_or("");
            lang_ok += usize::from(w.fields.get("Type").map(String::as_str) == Some(lang));
            // Go chomps every trailing CR/LF (`htext.Chomp`).
            let inner = literal.trim_end_matches(['\r', '\n']);
            let want_inner = w.fields.get("Inner").map_or("", String::as_str);
            inner_ok += usize::from(want_inner == inner);
            let want_type = w.fields.get("Type").map_or("", String::as_str);
            show(
                "fence",
                name,
                &[format!("{want_type} {want_inner:?}")],
                &[format!("{lang} {inner:?}")],
            );
            let Some(brace) = info.find('{') else {
                continue;
            };
            attr_total += 1;
            let want = keys(
                w.fields.get("OptionsSlice"),
                w.fields.get("AttributesSlice"),
            );
            attr_ranges += usize::from(want.iter().any(|(_, v)| v.is_none()));
            let mut got: Vec<(String, Option<String>)> = Vec::new();
            if let Some(a) = attrs {
                got.extend(
                    a.pairs
                        .iter()
                        .map(|(k, v)| (k.to_lowercase(), Some(v.clone()))),
                );
                got.extend(a.id.iter().map(|id| ("id".to_owned(), Some(id.clone()))));
                if !a.classes.is_empty() {
                    got.push(("class".into(), Some(a.classes.join(" "))));
                }
            }
            got.sort();
            // Range values (`hl_lines=[2,"5-7"]`) are typed in Go; compare their keys only.
            let same = got.len() == want.len()
                && got
                    .iter()
                    .zip(&want)
                    .all(|((gk, gv), (wk, wv))| gk == wk && (wv.is_none() || gv == wv));
            attr_comrak_ok += usize::from(same);
            if !same {
                show(
                    "fence attr",
                    name,
                    &[format!("{} {want:?}", &info[brace..])],
                    &[format!("{got:?}")],
                );
            }
        }
    }
    vec![
        Row::new(
            "fence language (info word 1)".into(),
            "code blocks",
            lang_ok,
            total,
        ),
        Row::new(
            "fence content (Inner)".into(),
            "code blocks",
            inner_ok,
            total,
        ),
        Row::new(
            "fence attributes {k=v …} (comrak parser)".into(),
            "fences with {…}",
            attr_comrak_ok,
            attr_total,
        )
        .note(format!(
            "keys and scalar values; {attr_ranges} with typed range values (key only)"
        )),
    ]
}

type Block = (String, String, Option<Box<comrak::nodes::Attributes>>);

fn code_blocks<'a>(root: &'a AstNode<'a>) -> Vec<Block> {
    let mut v = Vec::new();
    walk(root, |n| {
        let d = n.data();
        // Go's code-block hook sees fenced blocks only; indented code is rendered directly.
        if let NodeValue::CodeBlock(cb) = &d.value
            && cb.fenced
        {
            v.push((cb.info.clone(), cb.literal.clone(), d.attrs.clone()));
        }
    });
    v
}

/// Key names of the oracle's `OptionsSlice`/`AttributesSlice` dumps (`name:len:value` lines).
/// `(key, scalar value)`; `None` for typed values without a textual form (`r:` ranges).
fn keys(options: Option<&String>, attrs: Option<&String>) -> Vec<(String, Option<String>)> {
    let mut k = Vec::new();
    for dump in [options, attrs].into_iter().flatten() {
        let mut rest = dump.as_str();
        while let Some((name, tail)) = rest.split_once(':') {
            let Some((len, tail)) = tail.split_once(':') else {
                break;
            };
            let Ok(len) = len.parse::<usize>() else {
                break;
            };
            let value = tail.get(..len).unwrap_or("");
            let scalar = ["s:", "b:", "i:", "f:"]
                .iter()
                .find_map(|p| value.strip_prefix(p))
                .map(str::to_owned);
            k.push((name.to_lowercase(), scalar));
            rest = tail.get(len..).unwrap_or("").trim_start_matches('\n');
        }
    }
    k.sort();
    k.dedup_by(|a, b| a.0 == b.0);
    k
}

// ───────────────────────────── math / passthrough ─────────────────────────────

/// A passthrough span in the source (the legacy docs site's delimiters).
struct Span {
    kind: &'static str,
    raw: String,
}

const DELIMS: [(&str, &str, &str); 3] = [
    ("block \\[ \\]", "\\[", "\\]"),
    ("block $$ $$", "$$", "$$"),
    ("inline \\( \\)", "\\(", "\\)"),
];

/// Finds delimited spans outside fenced/indented code, HTML blocks and code spans.
fn passthrough_spans(md: &str) -> Vec<Span> {
    let arena = Arena::new();
    let o = go_options(GoCfg::Default);
    let root = parse_document(&arena, md, &o);
    let verbatim = verbatim_lines(root);
    let lines = Lines::new(md);
    let mut blocked = vec![false; md.len()];
    for (a, b) in verbatim {
        let s = lines.offset(a, 1).unwrap_or(md.len()).min(md.len());
        let e = lines.offset(b + 1, 1).unwrap_or(md.len()).min(md.len());
        blocked[s..e].iter_mut().for_each(|x| *x = true);
    }
    walk(root, |n| {
        let d = n.data();
        if matches!(d.value, NodeValue::Code(_)) {
            let s = lines.offset(d.sourcepos.start.line, d.sourcepos.start.column);
            let e = lines.offset(d.sourcepos.end.line, d.sourcepos.end.column);
            if let (Some(s), Some(e)) = (s, e) {
                blocked[s.min(md.len())..(e + 1).min(md.len())]
                    .iter_mut()
                    .for_each(|x| *x = true);
            }
        }
    });
    let mut spans = Vec::new();
    let mut i = 0;
    while i < md.len() {
        let hit = DELIMS
            .iter()
            .find(|(_, open, _)| !blocked[i] && md[i..].starts_with(open));
        let Some(&(kind, open, close)) = hit else {
            i += md[i..].chars().next().map_or(1, char::len_utf8);
            continue;
        };
        let body = i + open.len();
        match md[body..].find(close) {
            Some(e) if !md[body..body + e].contains("\n\n") => {
                let end = body + e + close.len();
                spans.push(Span {
                    kind,
                    raw: md[i..end].to_owned(),
                });
                i = end;
            }
            _ => i = body,
        }
    }
    spans
}

pub fn math(c: &DocsCorpus) -> Vec<Row> {
    let plain = go_options(GoCfg::Default);
    let mut dollars = go_options(GoCfg::Default);
    dollars.extension.math_dollars = true;
    let mut counts: BTreeMap<&str, (usize, usize, usize)> = BTreeMap::new();
    for (_, md) in &c.docs {
        let spans = passthrough_spans(md);
        if spans.is_empty() {
            continue;
        }
        let collapse = |s: &str| s.split_ascii_whitespace().collect::<Vec<_>>().join(" ");
        let a = text_of(&to_html(md, &plain));
        let b = math_text(md, &dollars);
        for s in spans {
            let e = counts.entry(s.kind).or_default();
            e.0 += 1;
            e.1 += usize::from(a.contains(&collapse(&s.raw)));
            e.2 += usize::from(b.contains(&collapse(&s.raw)));
        }
    }
    counts
        .into_iter()
        .map(|(kind, (total, plain_ok, dollar_ok))| {
            Row::new(
                format!("math passthrough {kind}"),
                "spans verbatim",
                plain_ok.max(dollar_ok),
                total,
            )
            .note(format!("plain {plain_ok}, with math_dollars {dollar_ok}"))
        })
        .collect()
}

/// Page text where comrak math nodes are written back with their `$`/`$$` delimiters.
fn math_text(md: &str, o: &Options<'_>) -> String {
    let arena = Arena::new();
    let root = parse_document(&arena, md, o);
    for n in root.descendants() {
        let replacement = match &n.data().value {
            NodeValue::Math(m) if m.display_math => format!("$${}$$", m.literal),
            NodeValue::Math(m) => format!("${}$", m.literal),
            _ => continue,
        };
        n.data_mut().value = NodeValue::Text(replacement.into());
    }
    let mut html = String::new();
    comrak::format_html(root, o, &mut html).expect("fmt::Write to String");
    text_of(&html)
}

// ───────────────────────────── alerts ─────────────────────────────

pub fn alerts(c: &DocsCorpus) -> Vec<Row> {
    let mut o = go_options(GoCfg::Site);
    o.extension.alerts = true;
    let (mut alert_total, mut alert_ok, mut regular_total, mut regular_ok) = (0, 0, 0, 0);
    let (mut titled, mut signed) = (0, 0);
    for ((_, md), hooks) in c.docs.iter().zip(&c.hooks) {
        let want: Vec<String> = hooks
            .iter()
            .filter(|r| r.kind == "blockquote")
            .map(|r| {
                let f = |k: &str| r.fields.get(k).cloned().unwrap_or_default();
                titled += usize::from(!f("AlertTitle").is_empty());
                signed += usize::from(!f("AlertSign").is_empty());
                format!(
                    "{}|{}|{}|{}",
                    f("Type"),
                    f("AlertType"),
                    f("AlertTitle"),
                    f("AlertSign")
                )
            })
            .collect();
        if want.is_empty() {
            continue;
        }
        let arena = Arena::new();
        let root = parse_document(&arena, md, &o);
        let mut got = Vec::new();
        walk(root, |n| match &n.data().value {
            NodeValue::BlockQuote => got.push("regular|||".to_owned()),
            NodeValue::Alert(a) => got.push(format!(
                "alert|{}|{}|",
                a.alert_type.default_title().to_lowercase(),
                a.title.clone().unwrap_or_default()
            )),
            _ => {}
        });
        let (alerts, regular): (Vec<String>, Vec<String>) =
            want.into_iter().partition(|w| w.starts_with("alert"));
        alert_total += alerts.len();
        regular_total += regular.len();
        alert_ok += multiset_matches(&alerts, &got);
        regular_ok += multiset_matches(&regular, &got);
    }
    vec![
        Row::new(
            "GitHub alerts (type, title, sign)".into(),
            "alert blockquotes",
            alert_ok,
            alert_total,
        )
        .note(format!(
            "with title {titled}, with sign {signed}; comrak: 5 fixed types, no sign"
        )),
        Row::new(
            "regular blockquotes (alerts on)".into(),
            "blockquotes",
            regular_ok,
            regular_total,
        ),
    ]
}

// ───────────────────────────── emoji ─────────────────────────────

/// `:name:` candidates in text nodes (not code), with comrak's shortcode result.
pub fn emoji(c: &DocsCorpus, goldmark_emoji: Option<&BTreeMap<String, String>>) -> Row {
    let mut o = go_options(GoCfg::Default);
    o.extension.shortcodes = true;
    let (mut candidates, mut comrak_hits, mut agree, mut goldmark_hits) = (0, 0, 0, 0);
    let mut disagree = Vec::new();
    for (_, md) in &c.docs {
        let arena = Arena::new();
        let root = parse_document(&arena, md, &go_options(GoCfg::Default));
        let mut names = Vec::new();
        walk(root, |n| {
            if let NodeValue::Text(t) = &n.data().value {
                names.extend(shortcode_candidates(t));
            }
        });
        if names.is_empty() {
            continue;
        }
        let arena2 = Arena::new();
        let root2 = parse_document(&arena2, md, &o);
        let mut resolved: BTreeMap<String, String> = BTreeMap::new();
        walk(root2, |n| {
            if let NodeValue::ShortCode(sc) = &n.data().value {
                resolved.insert(sc.code.clone(), sc.emoji.clone());
            }
        });
        for name in names {
            candidates += 1;
            let ours = resolved.get(&name);
            comrak_hits += usize::from(ours.is_some());
            if let Some(table) = goldmark_emoji {
                let theirs = table.get(&name);
                goldmark_hits += usize::from(theirs.is_some());
                if ours == theirs {
                    agree += 1;
                } else if disagree.len() < 8 {
                    disagree.push(name);
                }
            }
        }
    }
    let row = Row::new(
        "emoji shortcodes :name:".into(),
        "candidates",
        comrak_hits,
        candidates,
    );
    match goldmark_emoji {
        Some(_) => row.note(format!(
            "goldmark-emoji resolves {goldmark_hits}; same result {agree}/{candidates}; e.g. differ {disagree:?}"
        )),
        None => row.note("goldmark-emoji table not given (FUGO_GOLDMARK_EMOJI_TSV)".into()),
    }
}

fn shortcode_candidates(t: &str) -> Vec<String> {
    let mut v = Vec::new();
    let parts: Vec<&str> = t.split(':').collect();
    let mut i = 1;
    while i + 1 < parts.len() {
        let p = parts[i];
        if !p.is_empty()
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || "_+-".contains(c))
        {
            v.push(p.to_owned());
            i += 2;
        } else {
            i += 1;
        }
    }
    v
}

// ───────────────────────────── linkify, typographer, raw HTML ─────────────────────────────

pub fn links(c: &DocsCorpus) -> Vec<Row> {
    let o = go_options(GoCfg::Default);
    let mut rows = vec![element_row(c, GoCfg::Default, "a", "links (all <a>)", &o)];
    let (mut total, mut ok, mut extra) = (0, 0, 0);
    for ((name, md), html) in c.docs.iter().zip(&c.html) {
        let want: Vec<String> = elements(&tokens(&html[0], FOLD), "a")
            .into_iter()
            .filter(|a| is_bare(a))
            .collect();
        let got: Vec<String> = elements(&tokens(&to_html(md, &o), FOLD), "a")
            .into_iter()
            .filter(|a| is_bare(a))
            .collect();
        show("linkify", name, &want, &got);
        total += want.len();
        let m = multiset_matches(&want, &got);
        ok += m;
        extra += got.len() - m;
    }
    rows.push(
        Row::new(
            "linkify (bare URL/www/email anchors)".into(),
            "anchors",
            ok,
            total,
        )
        .note(format!("comrak-only bare anchors: {extra}")),
    );
    rows
}

/// `<a href="X">X'</a>` where X' is X minus a scheme/`mailto:` (an autolink or linkify result).
fn is_bare(a: &str) -> bool {
    let Some(rest) = a.strip_prefix("<a href=\"") else {
        return false;
    };
    let Some((href, tail)) = rest.split_once("\">") else {
        return false;
    };
    let Some(text) = tail.strip_suffix("</a>") else {
        return false;
    };
    let href = href.replace("&quot;", "\"");
    !text.contains('<')
        && (href == text
            || href.strip_prefix("https://") == Some(text)
            || href.strip_prefix("http://") == Some(text)
            || href.strip_prefix("mailto:") == Some(text))
}

const SMART: &[char] = &['‘', '’', '“', '”', '–', '—', '…', '«', '»'];

pub fn typographer(c: &DocsCorpus) -> Row {
    let o = go_options(GoCfg::Default);
    let (mut pages, mut pages_ok, mut chars, mut chars_ok) = (0, 0, 0, 0);
    let seq = |html: &str| -> Vec<String> {
        tokens(html, FOLD)
            .into_iter()
            .filter_map(|t| match t {
                Tok::Text(s) => Some(s),
                _ => None,
            })
            .flat_map(|s| {
                s.chars()
                    .filter(|c| SMART.contains(c))
                    .map(String::from)
                    .collect::<Vec<_>>()
            })
            .collect()
    };
    for ((_, md), html) in c.docs.iter().zip(&c.html) {
        let want = seq(&html[0]);
        if want.is_empty() {
            continue;
        }
        let got = seq(&to_html(md, &o));
        pages += 1;
        pages_ok += usize::from(want == got);
        chars += want.len();
        chars_ok += multiset_matches(&want, &got);
    }
    Row::new(
        "typographer (quotes, dashes, ellipsis)".into(),
        "substitutions",
        chars_ok,
        chars,
    )
    .note(format!(
        "pages with the identical sequence {pages_ok}/{pages}"
    ))
}

/// Markdown never emits these; anything else with a tag in unsafe mode came from raw HTML.
const MARKDOWN_TAGS: &[&str] = &[
    "a",
    "blockquote",
    "br",
    "code",
    "dd",
    "del",
    "div",
    "dl",
    "dt",
    "em",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
    "img",
    "input",
    "li",
    "ol",
    "p",
    "pre",
    "section",
    "strong",
    "sup",
    "table",
    "tbody",
    "td",
    "th",
    "thead",
    "tr",
    "ul",
];

pub fn raw_html(c: &DocsCorpus) -> Vec<Row> {
    let safe = go_options(GoCfg::Default);
    let open = go_options(GoCfg::Site);
    let (mut omitted, mut omitted_ok, mut raw, mut raw_ok) = (0, 0, 0, 0);
    let comments = |html: &str| -> Vec<String> {
        tokens(html, FOLD)
            .into_iter()
            .filter_map(|t| match t {
                Tok::Comment(s) => Some(s.trim().to_owned()),
                _ => None,
            })
            .collect()
    };
    let raw_tags = |html: &str| -> Vec<String> {
        tokens(html, FOLD)
            .into_iter()
            .filter_map(|t| match t {
                Tok::Open { name, tag } if !MARKDOWN_TAGS.contains(&name.as_str()) => Some(tag),
                Tok::Comment(s) => Some(s),
                _ => None,
            })
            .collect()
    };
    for ((_, md), html) in c.docs.iter().zip(&c.html) {
        let want = comments(&html[GoCfg::Default as usize]);
        if !want.is_empty() {
            omitted += want.len();
            omitted_ok += multiset_matches(&want, &comments(&to_html(md, &safe)));
        }
        let want = raw_tags(&html[GoCfg::Site as usize]);
        if !want.is_empty() {
            raw += want.len();
            raw_ok += multiset_matches(&want, &raw_tags(&to_html(md, &open)));
        }
    }
    vec![
        Row::new(
            "raw HTML omitted (unsafe=false)".into(),
            "omission comments",
            omitted_ok,
            omitted,
        ),
        Row::new(
            "raw HTML passed (unsafe=true)".into(),
            "raw tags/comments",
            raw_ok,
            raw,
        ),
    ]
}

// ───────────────────────────── codeFences = false ─────────────────────────────

pub fn plain_fences(c: &DocsCorpus) -> Row {
    let o = go_options(GoCfg::Cjk);
    let pres = |html: &str| -> Vec<String> {
        let mut v = Vec::new();
        let mut rest = html;
        while let Some(s) = rest.find("<pre") {
            let e = rest[s..].find("</pre>").map_or(rest.len(), |e| s + e + 6);
            v.push(rest[s..e].to_owned());
            rest = &rest[e..];
        }
        v
    };
    let (mut total, mut ok) = (0, 0);
    for ((_, md), html) in c.docs.iter().zip(&c.html) {
        let want = pres(&html[GoCfg::Cjk as usize]);
        if want.is_empty() {
            continue;
        }
        total += want.len();
        ok += multiset_matches(&want, &pres(&to_html(md, &o)));
    }
    Row::new(
        "codeFences = false (plain <pre><code>), byte-exact".into(),
        "<pre> blocks",
        ok,
        total,
    )
}
