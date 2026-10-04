//! Definition lists, heading attributes and block attributes.

use super::*;

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
pub(super) fn heading_tags(toks: &[Tok]) -> Vec<(String, String)> {
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
            let inner = super::super::normalize::serialize(&toks[i + 1..j.min(toks.len())]);
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

pub(super) fn strip_id(tag: &str) -> String {
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
pub(super) fn attr_line(line: &str) -> Option<&str> {
    let t = line
        .trim_start_matches(|c: char| c == '>' || c.is_whitespace())
        .trim_end();
    is_attr_line(t).then_some(t)
}

pub(super) fn is_attr_line(line: &str) -> bool {
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
pub(super) fn text_of(html: &str) -> String {
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
