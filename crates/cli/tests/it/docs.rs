//! Gates A-D2 and A-D3 (docs-live, [`gate_a_d3`]): the legacy docs site, the test fixture
//! `testdata/legacy-docs`. fugo's own documentation, `docs/`, is checked by [`crate::docs_site`].
//!
//! Gate A-D2 (REWRITE_PLAN.md §7.3, T66): the legacy docs site with the `reduced` docs
//! patches, built by the binary against the committed golden data of the Go build
//! (`testdata/golden/docs-reduced/`) through the acceptance harness:
//! `tools/dev/compare.sh docs-reduced --ref golden` (`sites.py make docs-reduced --overlay
//! sites/docs`, both passes, `structdiff.py` and the ratchet of
//! `testdata/baselines/docs-reduced.json`).
//!
//! The reduced variant keeps Chroma highlighting (with `hl` inline/`noClasses` and
//! `shortcodes/highlight.md`), GoAT diagrams (`diagrams_goat`), emoji, passthrough with
//! `to_math`, `remarshal` in `code-toggle`, the recorded Tailwind CSS through `defer`
//! (`sites/docs/assets/css/styles.tailwind.css`) and the real Alpine/Turbo `js_build`. The
//! gate: L1 888/888 in both passes, the structure oracle and L2 on every file, L4 on every
//! asset, every page's visible text (A7 1.0) and a clean ratchet.
//!
//! `js_build` imports the node modules of `tools/dev/node.sh`; without them the test prints
//! `SKIPPED` and passes ([`crate::acceptance`]). The Go binaries are not needed. The binary
//! must have the default features `goat` and `math`.

use crate::acceptance;

#[test]
fn gate_a_d2() {
    let Some(d) = acceptance::compare("gate_a_d2", "docs-reduced") else {
        return;
    };
    acceptance::assert_l1(&d, 888);
    acceptance::assert_equal(&d, &["L1", "L2", "L4", "S"]);
    let a7 = d["a7"]["ratio"].as_f64().expect("A7 ratio");
    assert!(
        (a7 - 1.0).abs() < f64::EPSILON,
        "A7 {a7}: a page's visible text differs"
    );
}

/// Gate A-D3: the legacy docs site as the Go build's published site has it — `docs-live`,
/// `testdata/legacy-docs` without patches (GetRemote, `images.Text`, QR codes, Dither, smart
/// crops, the `x` shortcode, the style gallery and the news content adapter all run;
/// `tools/legacy-docs/build.sh` builds it) — against the published site itself:
/// `testdata/golden/docs-live/` is the manifest of the published site's repository at a1928152,
/// the Go build of 2025-10-13 (testdata/golden/README.md). One unminified
/// pass (the site is published unminified) at that build's clock, its GetRemote responses
/// from `sites.py cache docs-live`. The gate: L1 2372/2372, L2 and L4 on every file, every
/// page's visible text (A7 1.0) and a clean ratchet (`testdata/baselines/docs-live.json`).
#[test]
fn gate_a_d3() {
    let Some(d) = acceptance::compare("gate_a_d3", "docs-live") else {
        return;
    };
    acceptance::assert_l1_passes(&d, &["unminified"], 2372);
    acceptance::assert_equal(&d, &["L1", "L2", "L4"]);
    let a7 = d["a7"]["ratio"].as_f64().expect("A7 ratio");
    assert!(
        (a7 - 1.0).abs() < f64::EPSILON,
        "A7 {a7}: a page's visible text differs"
    );
}
