//! Hierarchical taxonomies (`[taxonomies.<singular>] hierarchical = true`): the terms form a
//! tree, every term above a term exists, a term lists the pages of the terms below it, and a
//! value without a `/` names the one term that ends in it.

use ssg_build::{BuildError, BuildReport, BuildRequest, SinkKind, build};
use ssg_config::CliOverrides;

use crate::support::write_files;

/// One line per page: its relations as the layouts below print them.
const LAYOUT: &str = "{{ page.kind }} {{ page.title }}
term={% if page.term %}{{ page.term.key }}|{{ page.term.term }}{% endif %}
parent={% if page.parent %}{{ page.parent.rel_permalink }}{% endif %}
ancestors={% for a in page.ancestors %}{{ a.rel_permalink }} {% endfor %}
pages={% for p in page.pages %}{{ p.rel_permalink }} {% endfor %}
sections={% for p in page.sections %}{{ p.rel_permalink }} {% endfor %}
terms={% for k, list in page.terms %}{% for t in list %}{{ t.rel_permalink }} {% endfor %}{% endfor %}
{% if page.is_home %}{% for plural, tx in site.taxonomies %}{% for key, e in tx %}{{ plural }}[{{ key }}]={{ e.count }} {% endfor %}{% endfor %}{% endif %}
";

const CONFIG: &str = r#"baseURL = "https://example.org/"
disableKinds = ["rss", "sitemap", "robotstxt", "404"]
[taxonomies]
tag = "tags"
[taxonomies.location]
plural = "locations"
hierarchical = true
"#;

fn try_build(files: &[(&str, &str)]) -> (tempfile::TempDir, Result<BuildReport, BuildError>) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let site = tmp.path().join("site");
    let mut all: Vec<(String, String)> = ["home", "list", "single", "taxonomy", "term"]
        .iter()
        .map(|l| (format!("layouts/{l}.html"), LAYOUT.to_owned()))
        .collect();
    all.extend(
        files
            .iter()
            .map(|(p, c)| ((*p).to_owned(), (*c).to_owned())),
    );
    write_files(&site, &all);
    let report = build(BuildRequest {
        source: site,
        sink: SinkKind::Memory,
        cli: CliOverrides {
            cache_dir: Some(tmp.path().join("cache")),
            ..CliOverrides::default()
        },
        ..BuildRequest::default()
    });
    (tmp, report)
}

fn build_ok(files: &[(&str, &str)]) -> BuildReport {
    let (_tmp, r) = try_build(files);
    r.unwrap_or_else(|e| panic!("build failed: {e}"))
}

/// The output line of `page` (`/locations/france/`) that starts with `field=`.
fn field(r: &BuildReport, page: &str, field: &str) -> String {
    let path = format!("{}index.html", page.trim_start_matches('/'));
    let text = r
        .memory
        .as_ref()
        .expect("memory")
        .text(&path)
        .unwrap_or_else(|| panic!("no output {path}"));
    let prefix = format!("{field}=");
    text.lines()
        .find_map(|l| l.strip_prefix(&prefix))
        .unwrap_or_else(|| panic!("no {field} in {path}:\n{text}"))
        .trim_end()
        .to_owned()
}

fn page(name: &str, front_matter: &str) -> (String, String) {
    (
        format!("content/posts/{name}.md"),
        format!("---\ntitle: {name}\n{front_matter}\n---\n"),
    )
}

fn files(pages: &[(String, String)]) -> Vec<(&str, &str)> {
    let mut v = vec![("config.toml", CONFIG)];
    v.extend(pages.iter().map(|(p, c)| (p.as_str(), c.as_str())));
    v
}

#[test]
fn every_term_above_a_term_exists_and_lists_the_pages_below() {
    let pages = [
        page("a", "locations: [France/Aquitaine/Landiras]"),
        page("b", "locations: [France/Aquitaine]\nlocations_weight: -1"),
        page("c", "locations: [France/Normandie/Rouen, Denmark]"),
    ];
    let r = build_ok(&files(&pages));

    // The taxonomy page lists the top-level terms; `sections` are the child terms.
    assert_eq!(
        field(&r, "/locations/", "pages"),
        "/locations/denmark/ /locations/france/"
    );
    assert_eq!(
        field(&r, "/locations/", "sections"),
        "/locations/denmark/ /locations/france/"
    );
    // France and Normandie are made; titles and `.Data.Term` are the segment as written.
    assert_eq!(field(&r, "/locations/france/", "term"), "france|France");
    assert_eq!(
        field(&r, "/locations/france/", "sections"),
        "/locations/france/aquitaine/ /locations/france/normandie/"
    );
    assert_eq!(
        field(&r, "/locations/france/normandie/", "term"),
        "france/normandie|Normandie"
    );
    // A term lists its pages and those below it, by weight.
    assert_eq!(
        field(&r, "/locations/france/", "pages"),
        "/posts/b/ /posts/a/ /posts/c/"
    );
    assert_eq!(
        field(&r, "/locations/france/aquitaine/", "pages"),
        "/posts/b/ /posts/a/"
    );
    // The parent and the ancestors follow the tree.
    assert_eq!(
        field(&r, "/locations/france/normandie/rouen/", "parent"),
        "/locations/france/normandie/"
    );
    assert_eq!(
        field(&r, "/locations/france/normandie/rouen/", "ancestors"),
        "/locations/france/normandie/ /locations/france/ /locations/ /"
    );
    // A page's terms are the ones it names.
    assert_eq!(
        field(&r, "/posts/a/", "terms"),
        "/locations/france/aquitaine/landiras/"
    );
    // `site.taxonomies` keys are paths below the taxonomy; counts include the terms below.
    let home = field(&r, "/", "pages");
    assert!(home.contains("/posts/"), "{home}");
    let text = r.memory.as_ref().unwrap().text("index.html").unwrap();
    for entry in [
        "locations[denmark]=1",
        "locations[france]=3",
        "locations[france/aquitaine]=2",
        "locations[france/aquitaine/landiras]=1",
        "locations[france/normandie]=1",
    ] {
        assert!(text.contains(entry), "{entry} not in:\n{text}");
    }
}

#[test]
fn a_bare_value_names_the_one_term_that_ends_in_it() {
    let pages = [
        page("a", "locations: [Landiras, aquitaine]"),
        page("b", "locations: [France/Aquitaine/Landiras]"),
        page("c", "locations: [Stick]"),
    ];
    let mut f = files(&pages);
    // A term page in the content tree is known too; its parents are made.
    f.push((
        "content/locations/japan/tokyo/_index.md",
        "---\ntitle: Tokyo\n---\n",
    ));
    f.push((
        "content/posts/d.md",
        "---\ntitle: d\nlocations: [tokyo]\n---\n",
    ));
    let r = build_ok(&f);

    assert_eq!(
        field(&r, "/posts/a/", "terms"),
        "/locations/france/aquitaine/landiras/ /locations/france/aquitaine/"
    );
    assert_eq!(field(&r, "/posts/d/", "terms"), "/locations/japan/tokyo/");
    assert_eq!(field(&r, "/locations/japan/", "pages"), "/posts/d/");
    // An unknown bare value is a top-level term.
    assert_eq!(field(&r, "/posts/c/", "terms"), "/locations/stick/");
    assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
}

#[test]
fn an_ambiguous_bare_value_stays_top_level_with_a_warning() {
    let pages = [
        page("a", "locations: [France/Paris, USA/Texas/Paris]"),
        page("b", "locations: [Paris]"),
    ];
    let r = build_ok(&files(&pages));
    assert_eq!(field(&r, "/posts/b/", "terms"), "/locations/paris/");
    let warning = r
        .diagnostics
        .iter()
        .find(|d| d.id.as_deref() == Some("taxonomy-ambiguous-term"))
        .expect("warning");
    assert!(
        warning
            .message
            .contains("/locations/france/paris and /locations/usa/texas/paris"),
        "{}",
        warning.message
    );
}

#[test]
fn a_removed_term_page_is_skipped_by_the_tree() {
    let pages = [page("a", "locations: [France/Normandie/Rouen]")];
    let mut f = files(&pages);
    f.push((
        "content/locations/france/normandie/_index.md",
        "---\ntitle: Normandie\ndraft: true\n---\n",
    ));
    let r = build_ok(&f);
    assert_eq!(
        field(&r, "/locations/france/normandie/rouen/", "parent"),
        "/locations/france/"
    );
    assert_eq!(
        field(&r, "/locations/france/", "sections"),
        "/locations/france/normandie/rouen/"
    );
}

#[test]
fn a_flat_taxonomy_is_unchanged() {
    let pages = [page("a", "tags: [x/y]")];
    let r = build_ok(&files(&pages));
    // No term is made above `x/y`, the taxonomy lists every term, and keys are last segments.
    assert!(
        r.memory
            .as_ref()
            .unwrap()
            .text("tags/x/index.html")
            .is_none()
    );
    assert_eq!(field(&r, "/tags/", "pages"), "/tags/x/y/");
    assert_eq!(field(&r, "/tags/", "sections"), "");
    assert_eq!(field(&r, "/tags/x/y/", "term"), "y|x/y");
}

#[test]
fn flat_term_urls_keep_the_key_and_the_tree() {
    let config =
        format!("{CONFIG}[permalinks.term]\nlocations = \"/locations/:sections[last]/\"\n");
    let p = page("a", "locations: [\"France/Cheddar & Sour Cream\"]");
    let r = build_ok(&[("config.toml", &config), (&p.0, &p.1)]);
    assert_eq!(
        field(&r, "/locations/cheddar--sour-cream/", "parent"),
        "/locations/france/"
    );
    assert_eq!(
        field(&r, "/locations/france/", "sections"),
        "/locations/cheddar--sour-cream/"
    );
}

#[test]
fn the_table_form_is_checked() {
    let (_tmp, r) = try_build(&[(
        "config.toml",
        "baseURL = \"https://example.org/\"\n[taxonomies.tag]\nplural = \"tags\"\nnested = true\n",
    )]);
    let Err(e) = r else {
        panic!("the build succeeded");
    };
    let e = e.to_string();
    assert!(e.contains("taxonomies.tag.nested"), "{e}");
}
