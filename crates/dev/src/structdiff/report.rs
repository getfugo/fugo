//! The report: the comparison as text.

use super::*;

/// The first `n` characters of `s`.
#[must_use]
pub fn cut(s: &str, n: usize) -> &str {
    s.char_indices().nth(n).map_or(s, |(i, _)| &s[..i])
}

/// The report of a comparison.
#[must_use]
pub fn report_text(res: &Comparison, show: usize) -> String {
    let mut out = vec![format!(
        "structdiff {}: {} (reference) vs {} (candidate)",
        res.site, res.reference, res.cand
    )];
    for (pass, p) in &res.passes {
        out.push(format!(
            "  L1 {pass:<10} {}/{} files matched (candidate {} files; paths {} missing, {} extra)",
            p.matched, p.ref_files, p.cand_files, p.missing, p.extra
        ));
    }
    if !res.collision_dirs.is_empty() {
        out.push(format!(
            "     collision directories (collapsed): {}",
            res.collision_dirs.join(", ")
        ));
    }
    let level = |l| res.summary.get(&l).cloned().unwrap_or_default();
    for (l, what) in [
        (Level::L2, "files with equal links"),
        (Level::L3, "files with equal text"),
        (Level::L4, "files with equal assets"),
    ] {
        let x = level(l);
        out.push(format!("  {} {}/{} {what}", l.name(), x.ok, x.compared));
    }
    let s = level(Level::S);
    let kinds: Vec<String> = s
        .by_kind
        .iter()
        .flatten()
        .map(|(k, c)| {
            format!(
                "{k} {}/{}",
                c.get(&Status::Ok).copied().unwrap_or(0),
                c.values().sum::<usize>()
            )
        })
        .collect();
    out.push(format!(
        "  S  {}/{} structure facts equal ({})",
        s.ok,
        s.compared,
        kinds.join(", ")
    ));
    if let Some(a) = &res.a7 {
        out.push(format!(
            "  A7 {:.4} ({}/{} pages with equal visible text; mean similarity {:.4})",
            a.ratio, a.equal, a.pages, a.mean_similarity
        ));
    }
    out.extend(res.notes.iter().map(|n| format!("  note: {n}")));
    out.push(format!("  total: {} differences", res.diffs));
    if !res.classes.is_empty() {
        out.push("Top difference classes:".into());
        for c in res.classes.iter().take(25) {
            let examples = c
                .examples
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ");
            out.push(format!("  {:>5}  {:<28} e.g. {examples}", c.count, c.class));
        }
    }
    if let Some(a) = &res.a7 {
        if !a.top_hunks.is_empty() {
            out.push("Top visible-text differences (word hunks, pages that have them):".into());
            for h in &a.top_hunks {
                let examples = h
                    .examples
                    .iter()
                    .take(2)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push(format!(
                    "  {:>5}  {}  e.g. {examples}",
                    h.pages,
                    cut(&h.hunk, 150)
                ));
            }
        }
        if !a.worst.is_empty() {
            out.push(format!(
                "Worst {} pages (A7 similarity: `words` = Dice of the word multisets, `len` = length ratio):",
                a.worst.len()
            ));
            for p in &a.worst {
                let h = p.hunks.first().map_or_else(String::new, |h| {
                    format!(
                        "  {} hunks, first {}",
                        p.hunk_count.unwrap_or(0),
                        cut(h, 120)
                    )
                });
                out.push(format!("  {:.4} {:<7} {}{h}", p.ratio, p.method, p.file));
            }
        }
    }
    for level in Level::ALL {
        let section = if level == Level::S {
            &res.structure
        } else {
            &res.files
        };
        let bad: Vec<(&String, &Outcome)> = section
            .iter()
            .filter_map(|(k, v)| {
                v.get(&level)
                    .filter(|o| o.status != Status::Ok)
                    .map(|o| (k, o))
            })
            .collect();
        if bad.is_empty() {
            continue;
        }
        out.push(format!("{} differences ({}):", level.name(), bad.len()));
        for (k, o) in bad.iter().take(show) {
            let detail = if o.detail.is_empty() {
                String::new()
            } else {
                format!(": {}", cut(&o.detail, 300))
            };
            out.push(format!("  {:<7} {k}{detail}", o.status.name()));
        }
        if bad.len() > show {
            out.push(format!("  … {} more", bad.len() - show));
        }
    }
    if let Some(r) = &res.ratchet {
        out.extend(r.lines(show));
    }
    let mut text = out.join("\n");
    text.push('\n');
    text
}
