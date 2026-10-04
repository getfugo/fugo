//! Fenced code, math and passthrough.

use super::*;

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

pub(super) type Block = (String, String, Option<Box<comrak::nodes::Attributes>>);

pub(super) fn code_blocks<'a>(root: &'a AstNode<'a>) -> Vec<Block> {
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
pub(super) fn keys(
    options: Option<&String>,
    attrs: Option<&String>,
) -> Vec<(String, Option<String>)> {
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
pub(super) struct Span {
    pub(super) kind: &'static str,
    pub(super) raw: String,
}

pub(super) const DELIMS: [(&str, &str, &str); 3] = [
    ("block \\[ \\]", "\\[", "\\]"),
    ("block $$ $$", "$$", "$$"),
    ("inline \\( \\)", "\\(", "\\)"),
];

/// Finds delimited spans outside fenced/indented code, HTML blocks and code spans.
pub(super) fn passthrough_spans(md: &str) -> Vec<Span> {
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
pub(super) fn math_text(md: &str, o: &Options<'_>) -> String {
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
