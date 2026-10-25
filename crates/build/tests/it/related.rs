//! Related content in a build: fragments indices match the pages' heading ids, in layouts and
//! in shortcodes, and `applyFilter` leaves each result only its matched headings.

use std::fs;

use ssg_build::{BuildRequest, SinkKind, build};

use crate::support::write_files;

const CONFIG: &str = r#"baseURL = "https://example.org/"
disableKinds = ["rss", "sitemap", "taxonomy", "term"]
[related]
threshold = 0
includeNewer = true
[[related.indices]]
name = "fragmentrefs"
type = "fragments"
applyFilter = true
weight = 80
[[related.indices]]
name = "tags"
weight = 100
"#;

/// Each result: its title, then its heading ids and titles.
const SINGLE: &str = r#"{{ page.title }}:{% for p in related(pages=site.regular_pages) %} {{ p.title }}[{{ p.fragments.identifiers | join(sep=",") }}]{% endfor %}
{{ page.content }}"#;

/// The text of each page of the built site, by title (its first line, then its content).
fn built() -> Vec<String> {
    let tmp = tempfile::tempdir().expect("tempdir");
    let dir = tmp.path().join("site");
    let files = [
        ("config.toml", CONFIG),
        (
            "content/a.md",
            "---\ntitle: A\ndate: 2024-01-01\ntags: [x]\n---\n## Shared heading\n\n## Only in A\n",
        ),
        (
            "content/b.md",
            "---\ntitle: B\ndate: 2024-01-02\n---\n## Shared heading\n\n## Only in B\n\n{{< seealso >}}\n",
        ),
        (
            "content/c.md",
            "---\ntitle: C\ndate: 2024-01-03\nfragmentrefs: [shared-heading]\n---\n## Nothing shared\n",
        ),
        (
            "content/d.md",
            "---\ntitle: D\ndate: 2024-01-04\ntags: [x]\n---\nNo headings.\n",
        ),
        ("layouts/single.html", SINGLE),
        ("layouts/list.html", "{{ page.title }}"),
        (
            "layouts/_shortcodes/seealso.html",
            r#"<p>See also:{% for p in related(pages=site.regular_pages) %} {{ p.title }}{% for h in p.fragments.headings %} #{{ h.id }} {{ h.title }}{% endfor %}{% endfor %}</p>"#,
        ),
    ];
    write_files(&dir, &files.map(|(p, t)| (p.to_owned(), t.to_owned())));
    let public = tmp.path().join("public");
    let report = build(BuildRequest {
        source: dir,
        destination: Some(public.clone()),
        sink: SinkKind::Disk,
        clock: Some("2026-09-01T00:00:00Z".parse().expect("clock")),
        ..BuildRequest::default()
    })
    .expect("build");
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    ["a", "b", "c", "d"]
        .iter()
        .map(|p| fs::read_to_string(public.join(p).join("index.html")).expect("read"))
        .collect()
}

#[test]
fn fragments_indices_match_heading_ids() {
    let pages = built();
    let first = |i: usize| pages[i].lines().next().unwrap_or_default().to_owned();
    // A: D by tag (100), then C (its `fragmentrefs`) and B (the heading), both by
    // `shared-heading` (80), the newer first; every result keeps only that heading.
    assert_eq!(first(0), "A: D[] C[] B[shared-heading]");
    assert_eq!(first(1), "B: C[] A[shared-heading]");
    assert_eq!(first(2), "C: B[shared-heading] A[shared-heading]");
    // D: A by tag alone, with all its headings (no heading matched, no filter).
    assert_eq!(first(3), "D: A[only-in-a,shared-heading]");
}

#[test]
fn fragments_indices_work_in_shortcodes() {
    let pages = built();
    // B's content asks while B renders: its own headings come without the shortcode.
    assert!(
        pages[1].contains("<p>See also: C A #shared-heading Shared heading</p>"),
        "{}",
        pages[1]
    );
}
