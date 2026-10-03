//! T04 comrak spike: native comrak 0.55 over the 959 docs bodies, measured per feature against
//! normalised goldmark HTML from the Go implementation.
//!
//! Re-run and print the table:
//! `cargo test -p ssg-markup --test it comrak_spike -- --nocapture`
//! (optionally with `FUGO_GOLDMARK_EMOJI_TSV=<shortcode\tcodepoints file>`, see the crate README).
//! The assertions are floors at the measured values, so a comrak upgrade that regresses a
//! feature fails here.

pub mod corpus;
mod engine;
mod features;
pub mod normalize;
mod sourcepos;

use std::collections::BTreeMap;
use std::fmt::Write as _;

use corpus::GoCfg;

pub struct Row {
    feature: String,
    unit: String,
    matched: usize,
    total: usize,
    note: String,
}

impl Row {
    fn new(feature: String, unit: &str, matched: usize, total: usize) -> Self {
        Self {
            feature,
            unit: unit.to_owned(),
            matched,
            total,
            note: String::new(),
        }
    }

    fn note(mut self, note: String) -> Self {
        self.note = note;
        self
    }
}

fn table(title: &str, rows: &[Row]) -> String {
    let mut s = format!(
        "\n## {title}\n\n| feature | matched / total | unit | notes |\n|---|---|---|---|\n"
    );
    for r in rows {
        let _ = writeln!(
            s,
            "| {} | {} / {} | {} | {} |",
            r.feature, r.matched, r.total, r.unit, r.note
        );
    }
    s
}

/// `shortcode<TAB>hex codepoints separated by spaces`, one per line.
fn goldmark_emoji() -> Option<BTreeMap<String, String>> {
    let path = std::env::var_os("FUGO_GOLDMARK_EMOJI_TSV")?;
    let text = std::fs::read_to_string(&path).ok()?;
    let table = text
        .lines()
        .filter_map(|l| {
            let (name, cps) = l.split_once('\t')?;
            let s: Option<String> = cps
                .split(' ')
                .map(|h| u32::from_str_radix(h, 16).ok().and_then(char::from_u32))
                .collect();
            Some((name.to_owned(), s?))
        })
        .collect();
    Some(table)
}

fn floor(rows: &[Row], feature: &str, min: usize) {
    let r = rows
        .iter()
        .find(|r| r.feature == feature)
        .unwrap_or_else(|| panic!("no row {feature}"));
    assert!(r.matched >= min, "{feature}: {} < floor {min}", r.matched);
}

#[test]
fn comrak_spike() {
    let docs = corpus::docs();
    assert_eq!(docs.docs.len(), 959, "docs bodies");

    let mut whole: Vec<Row> = GoCfg::ALL
        .iter()
        .map(|&c| features::overall_docs(&docs, c))
        .collect();
    whole.push(features::residual_docs(&docs, GoCfg::Default));
    whole.push(features::residual_docs(&docs, GoCfg::Site));

    let mut feat = features::deflists(&docs);
    feat.push(features::heading_attributes(&docs));
    feat.push(features::block_attributes(&docs));
    feat.extend(features::fences(&docs));
    feat.extend(features::math(&docs));
    feat.extend(features::alerts(&docs));
    feat.push(features::emoji(&docs, goldmark_emoji().as_ref()));
    feat.extend(features::links(&docs));
    feat.push(features::typographer(&docs));
    feat.extend(features::raw_html(&docs));
    feat.push(features::plain_fences(&docs));

    let pos = sourcepos::check(&docs.docs, &engine::go_options(GoCfg::Default));

    println!("{}", table("Whole documents (normalised)", &whole));
    println!(
        "{}",
        table("Features (native comrak, no custom pass)", &feat)
    );
    println!("{}", table("Inline sourcepos (docs, cfg default)", &pos));

    // Floors at the values measured by T04 (comrak 0.55.0); see the crate README.
    floor(&whole, "docs, whole page, cfg default", 862);
    floor(
        &whole,
        "docs, whole page, cfg default, after pass-owned folds",
        925,
    );
    floor(&feat, "definition details", 866);
    floor(&feat, "heading attributes {#id .class k=v}", 11);
    floor(&feat, "fence language (info word 1)", 2006);
    floor(&feat, "fence content (Inner)", 2006);
    floor(&feat, "GitHub alerts (type, title, sign)", 278);
    floor(
        &feat,
        "codeFences = false (plain <pre><code>), byte-exact",
        2036,
    );
    floor(&feat, "raw HTML passed (unsafe=true)", 22);
    floor(&feat, "typographer (quotes, dashes, ellipsis)", 3533);
    floor(&pos, "sourcepos link (paragraph)", 1586);
    floor(&pos, "sourcepos link (table)", 2045);
    floor(&pos, "sourcepos image (paragraph)", 31);
    floor(&pos, "sourcepos text", 19597);
    for r in pos
        .iter()
        .filter(|r| !r.feature.contains("after link ref defs"))
    {
        assert_eq!(
            r.matched, r.total,
            "{}: inline sourcepos regressed",
            r.feature
        );
    }
}
