//! A-DET (REWRITE_PLAN.md §3.5, §7.1): the output tree is identical with 1 and 8 render
//! threads, and across two runs with 8 — on the `mini` site (deferred wave, post-processing,
//! URL-token publishing, `FromString` claimed by two languages) and on the edge trees with
//! target collisions (`build-collide`: 13 colliding terms and pagers, `asm-taxo`,
//! `build-aliases`: aliases against pages, minified).

use std::collections::BTreeMap;
use std::sync::Arc;

use ssg_build::{BuildRequest, SinkKind, build};
use ssg_publish::MemorySink;

use crate::edges::write_site;
use crate::mini::build_mini;

/// Every file of a memory build, by path.
fn tree(m: &MemorySink) -> BTreeMap<String, Arc<[u8]>> {
    m.paths()
        .into_iter()
        .map(|p| {
            let bytes = m.get(p.relative()).expect("file");
            (p.relative().to_owned(), bytes)
        })
        .collect()
}

fn edge_tree(name: &str, threads: usize) -> (BTreeMap<String, Arc<[u8]>>, usize) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path().join("site");
    write_site(name, &dir);
    let r = build(BuildRequest {
        source: dir,
        sink: SinkKind::Memory,
        clock: Some("2026-09-27T12:00:00Z".parse().expect("clock")),
        threads: Some(threads),
        ..BuildRequest::default()
    });
    let r = r.unwrap_or_else(|e| panic!("{name}: {e}"));
    let collisions = r.collisions.len();
    (tree(r.memory.as_ref().expect("memory")), collisions)
}

fn assert_same(name: &str, runs: &[BTreeMap<String, Arc<[u8]>>]) {
    let first = &runs[0];
    for (i, run) in runs.iter().enumerate().skip(1) {
        let differ: Vec<&String> = first
            .keys()
            .chain(run.keys())
            .filter(|k| first.get(*k) != run.get(*k))
            .collect();
        assert!(differ.is_empty(), "{name}: run {i} differs in {differ:?}");
    }
    println!(
        "{name}: {} files identical over {} runs",
        first.len(),
        runs.len()
    );
}

#[test]
fn output_does_not_depend_on_threads() {
    let runs: Vec<_> = [1, 8, 8]
        .into_iter()
        .map(|t| {
            let (_tmp, r) = build_mini(Some(t));
            tree(r.memory.as_ref().expect("memory"))
        })
        .collect();
    assert_same("mini", &runs);

    for name in ["build-collide", "asm-taxo", "build-aliases"] {
        let mut collisions = Vec::new();
        let runs: Vec<_> = [1, 8, 8]
            .into_iter()
            .map(|t| {
                let (tree, c) = edge_tree(name, t);
                collisions.push(c);
                tree
            })
            .collect();
        assert!(
            collisions.windows(2).all(|w| w[0] == w[1]),
            "{name}: {collisions:?}"
        );
        assert_same(name, &runs);
    }
}
