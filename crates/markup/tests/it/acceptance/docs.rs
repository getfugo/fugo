//! The convert oracle: heading and definition-term ids, the table of contents and whole
//! pages (normalised) for every document and markup configuration.

use std::sync::LazyLock;

use regex::Regex;
use ssg_markup::{Heading, NoHooks, TocOptions};

use super::super::comrak_spike::normalize::{Fold, normalize};
use super::{
    CONVERT, GoCfg, Row, TocHeading, fragments_of, options, print, render_with, show, text,
};

static IDS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<(h[1-6]|dt)((?: [a-z-]+="[^"]*")*)>"#).expect("valid pattern"));
static ID_ATTR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#" id="([^"]*)""#).expect("valid pattern"));

/// `(tag, id)` of every heading and definition term.
fn ids(html: &str, tag: &str) -> Vec<String> {
    IDS.captures_iter(html)
        .filter(|c| c[1].starts_with(tag))
        .map(|c| {
            let id = ID_ATTR
                .captures(&c[2])
                .map_or_else(String::new, |m| m[1].to_owned());
            format!("{}#{id}", &c[1])
        })
        .collect()
}

#[test]
fn heading_and_term_ids() {
    let mut rows = Vec::new();
    for cfg in GoCfg::ALL {
        let o = options(cfg);
        let (mut docs, mut ok, mut hs, mut hs_ok, mut dts, mut dts_ok) = (0, 0, 0, 0, 0, 0);
        for case in CONVERT.iter() {
            let Some(want) = &case.html[cfg as usize] else {
                continue;
            };
            let got = render_with(&case.md, &o, &NoHooks).html;
            let (wh, gh) = (ids(want, "h"), ids(&got, "h"));
            let (wd, gd) = (ids(want, "dt"), ids(&got, "dt"));
            docs += 1;
            ok += usize::from(wh == gh && wd == gd);
            hs += wh.len();
            hs_ok += wh.iter().zip(&gh).filter(|(a, b)| a == b).count();
            dts += wd.len();
            dts_ok += wd.iter().zip(&gd).filter(|(a, b)| a == b).count();
            show(
                &format!("ids {}", cfg.name()),
                &case.name,
                &wh.join(" "),
                &gh.join(" "),
            );
        }
        rows.push(Row::new(format!("pages, cfg {}", cfg.name()), ok, docs));
        rows.push(Row::new(
            format!("heading ids, cfg {}", cfg.name()),
            hs_ok,
            hs,
        ));
        rows.push(Row::new(
            format!("term ids, cfg {}", cfg.name()),
            dts_ok,
            dts,
        ));
    }
    print("Heading and definition-term ids (convert oracle)", &rows);
    for r in &rows {
        assert_eq!(r.matched, r.total, "{}", r.what);
    }
}

fn same_tree(want: &[TocHeading], got: &[Heading]) -> bool {
    want.len() == got.len()
        && want.iter().zip(got).all(|(w, g)| {
            text(&w.id) == g.id
                && w.level == i64::from(g.level)
                && text(&w.title) == g.html
                && same_tree(&w.headings, &g.children)
        })
}

/// The five `ToHTML` variants of the oracle: the configuration's levels, then fixed ones.
fn variants(cfg: TocOptions) -> [TocOptions; 5] {
    let fixed = |start, end, ordered| TocOptions {
        start,
        end,
        ordered,
    };
    [
        cfg,
        fixed(1, None, false),
        fixed(2, Some(3), true),
        fixed(3, Some(4), false),
        fixed(0, Some(2), true),
    ]
}

#[test]
fn table_of_contents() {
    let mut rows = Vec::new();
    for cfg in GoCfg::ALL {
        let o = options(cfg);
        let (mut total, mut tree, mut idents, mut htmls) = (0, 0, 0, 0);
        for case in CONVERT.iter() {
            let Some(want) = &case.toc[cfg as usize] else {
                continue;
            };
            if !want.present {
                continue;
            }
            total += 1;
            let got = render_with(&case.md, &o, &NoHooks);
            assert_eq!(
                fragments_of(&case.md, &o),
                got.fragments,
                "parse-only fragments of {}",
                case.name
            );
            let wh = want.headings.clone().unwrap_or_default();
            if same_tree(&wh, &got.toc.headings) && same_tree(&wh, &got.fragments.headings) {
                tree += 1;
            } else {
                show(
                    &format!("toc {}", cfg.name()),
                    &case.name,
                    &format!(
                        "{:?}",
                        wh.iter().map(|h| text(&h.title)).collect::<Vec<_>>()
                    ),
                    &format!("{:?}", got.toc.headings),
                );
            }
            let wi: Vec<String> = want.identifiers.iter().flatten().map(text).collect();
            idents += usize::from(wi == got.fragments.identifiers);
            let wv: Vec<String> = want.html.iter().flatten().map(text).collect();
            let gv: Vec<String> = variants(o.toc).iter().map(|t| got.toc.to_html(t)).collect();
            if wv == gv {
                htmls += 1;
            } else {
                show(
                    &format!("toc html {}", cfg.name()),
                    &case.name,
                    &wv.join("\n"),
                    &gv.join("\n"),
                );
            }
        }
        rows.push(Row::new(format!("tree, cfg {}", cfg.name()), tree, total));
        rows.push(Row::new(
            format!("identifiers, cfg {}", cfg.name()),
            idents,
            total,
        ));
        rows.push(Row::new(
            format!("ToHTML x5, cfg {}", cfg.name()),
            htmls,
            total,
        ));
    }
    print("Table of contents (convert oracle)", &rows);
    for r in &rows {
        assert_eq!(r.matched, r.total, "{}", r.what);
    }
}

const FOLD: Fold = Fold {
    auto_ids: false,
    typography: false,
    footnotes: false,
};

#[test]
fn whole_pages() {
    let mut rows = Vec::new();
    for cfg in GoCfg::ALL {
        let o = options(cfg);
        let (mut total, mut ok) = (0, 0);
        for case in CONVERT
            .iter()
            .filter(|c| c.name.starts_with("docs/content/"))
        {
            let Some(want) = &case.html[cfg as usize] else {
                continue;
            };
            total += 1;
            let got = render_with(&case.md, &o, &NoHooks).html;
            let (w, g) = (normalize(want, FOLD), normalize(&got, FOLD));
            let label = format!("pages {}", cfg.name());
            show(&label, &case.name, &w, &g);
            ok += usize::from(w == g);
        }
        rows.push(Row::new(
            format!("docs pages (normalised), cfg {}", cfg.name()),
            ok,
            total,
        ));
    }
    print("Whole docs pages (convert oracle, normalised)", &rows);
    let floor = |cfg: &str, min: usize| {
        let r = rows
            .iter()
            .find(|r| r.what.ends_with(cfg))
            .expect("row exists");
        assert!(r.matched >= min, "{}: {} < {min}", r.what, r.matched);
    };
    // Floors at the measured values; the residual pages are listed in the crate README.
    floor("cfg default", 870);
    floor("cfg site", 871);
    floor("cfg ascii", 871);
    floor("cfg blackfriday", 872);
    floor("cfg cjk", 540);
    floor("cfg noattr", 870);
}
