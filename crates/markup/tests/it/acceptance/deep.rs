//! Deeply nested documents (the `deep` part of the convert oracle) render on a small stack.

use serde::Deserialize;
use ssg_markup::NoHooks;
use ssg_testkit::fixture::{GoString, oracle};

use super::super::comrak_spike::normalize::{Fold, normalize};
use super::{GoCfg, Row, options, print, render_with, text};

/// The Go oracle's `deepDocs` (tools/go-oracle/nh-markup/convert at 44529028).
fn deep_docs() -> Vec<String> {
    vec![
        "> ".repeat(3000) + "deep *quote*\n",
        "- ".repeat(3000) + "item\n",
        "*".repeat(5000) + "x" + &"*".repeat(5000) + "\n",
        "## ".to_owned() + &"**a ".repeat(2000) + &"**".repeat(2000) + "\n",
        "![".repeat(1000) + "x" + &"](i)".repeat(1000) + "\n",
        "[".repeat(3000) + "x" + &"](u)".repeat(3000) + "\n",
        "# ".to_owned()
            + &"[a ".repeat(1500)
            + &"](u)".repeat(1500)
            + "\n\n"
            + &"> ".repeat(500)
            + "| a |\n"
            + &"> ".repeat(500)
            + "|---|\n",
    ]
}

#[derive(Deserialize)]
struct Deep {
    html: GoString,
}

#[derive(Deserialize)]
struct Convert {
    deep: Vec<Deep>,
}

#[test]
fn deep_nesting_on_a_small_stack() {
    let fx: Convert = oracle("oracle/markup/convert/convert.json.gz");
    let want: Vec<String> = fx.deep.iter().map(|d| text(&d.html)).collect();
    let got = std::thread::Builder::new()
        .stack_size(2 << 20)
        .spawn(|| {
            let o = options(GoCfg::Site);
            deep_docs()
                .iter()
                .map(|d| render_with(d, &o, &NoHooks).html)
                .collect::<Vec<_>>()
        })
        .expect("spawn")
        .join()
        .expect("no stack overflow");
    let fold = Fold::default();
    let ok = want
        .iter()
        .zip(&got)
        .filter(|(w, g)| {
            let (w, g) = (normalize(w, fold), normalize(g, fold));
            super::show("deep", "doc", &w, &g);
            w == g
        })
        .count();
    print(
        "Deep nesting (2 MiB stack)",
        &[Row::new("documents", ok, want.len())],
    );
    // comrak limits list and bracket nesting where goldmark does not; the point here is
    // that nothing recurses on the document depth.
    assert!(ok >= 3, "{ok}");
}
