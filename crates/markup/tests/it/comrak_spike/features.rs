//! Per-feature measurements: native comrak 0.55 against the Go implementation's goldmark output.

use std::collections::BTreeMap;

use comrak::nodes::{AstNode, NodeValue};
use comrak::{Arena, Options, parse_document};

use super::Row;
use super::corpus::{DocsCorpus, GoCfg};
use super::engine::{go_options, to_html, to_html_passes};
use super::normalize::{Fold, Tok, elements, multiset_matches, normalize, tokens};

mod blocks;
mod code;
mod inline;

pub use blocks::*;
pub use code::*;
pub use inline::*;

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
