//! What the Go binary gives for the same adapters: slugs, sitemap priorities, dates and paths added
//! twice; two adapters on one path; many pages and resources.

use super::*;

/// What the Go binary at 44529028 gives for the same adapter: the slug as given
/// (front matter slugs lose their `-`, adapter slugs do not), the sitemap priority from zero
/// printed as Go prints a float (`0`, `1`), the dates chained as `[frontmatter]` says, and a
/// path added twice by one adapter is the last `add_page` (a resource the last
/// `add_resource`), with a warning.
#[test]
fn slugs_priorities_dates_and_paths_added_twice() {
    let (_tmp, r) = build_ok(&[
        (
            "config.toml",
            "baseURL = \"https://example.com/\"\n\
             disableKinds = [\"taxonomy\", \"term\", \"rss\", \"section\"]\n",
        ),
        (
            "content/_content.html",
            r#"{{ add_page(page={"path": "p1", "title": "p1", "slug": "-sl-"}) }}
{{ add_page(page={"path": "p2", "title": "p2", "slug": "--a b--", "sitemap": {"priority": 1, "changefreq": "daily"} }) }}
{{ add_page(page={"path": "p3", "title": "first", "dates": {"lastmod": "2024-01-05T00:00:00Z" | to_date} }) }}
{{ add_resource(resource={"path": "p3/r.txt", "content": {"value": "one"} }) }}
{{ add_page(page={"path": "P3", "title": "second", "kind": "page", "dates": {"lastmod": "2024-01-05T00:00:00Z" | to_date} }) }}
{{ add_resource(resource={"path": "p3/r.txt", "content": {"value": "two"} }) }}"#,
        ),
        (
            "layouts/home.html",
            "{% for p in site.regular_pages %}{{ p.title }}|{{ p.rel_permalink }}|{% if p.date %}{{ p.date | date(format=\"%Y-%m-%d\") }}{% endif %}|{% if p.publish_date %}{{ p.publish_date | date(format=\"%Y-%m-%d\") }}{% endif %}|{% for x in p.resources %}{{ x.name }}={{ x | resource_content }}{% endfor %}\n{% endfor %}",
        ),
        ("layouts/single.html", "{{ page.title }}"),
    ]);
    assert_eq!(
        text(&r, "index.html"),
        "second|/p3/|2024-01-05|2024-01-05|r.txt=two\np1|/-sl-/|||\np2|/--a-b--/|||\n"
    );
    assert_eq!(text(&r, "p3/r.txt"), "two");
    let sitemap: String = text(&r, "sitemap.xml").split_whitespace().collect();
    assert_contains(
        &sitemap,
        &[
            "<loc>https://example.com/-sl-/</loc><priority>0</priority>",
            "<loc>https://example.com/--a-b--/</loc><changefreq>daily</changefreq><priority>1</priority>",
        ],
    );
    let warnings: Vec<&str> = r
        .diagnostics
        .iter()
        .filter_map(|d| d.id.as_deref())
        .filter(|id| id.starts_with("duplicate-"))
        .collect();
    assert_eq!(
        warnings,
        ["duplicate-content-path", "duplicate-resource-path"]
    );
}

/// A path two adapters add is the later adapter's (the adapter of a subdirectory runs after
/// the one above it), and a path a content file has stays the file's, with warnings: the
/// Go binary at 44529028 with one collector worker (its worker multiplier variable set to 1)
/// gives the same pages and resources.
#[test]
fn two_adapters_on_one_path() {
    let (_tmp, r) = build_ok(&[
        (
            "config.toml",
            "baseURL = \"https://example.com/\"\n\
             disableKinds = [\"taxonomy\", \"term\", \"rss\", \"sitemap\"]\n",
        ),
        (
            "content/_content.html",
            r#"{{ add_page(page={"path": "books/x", "title": "X from root"}) }}
{{ add_resource(resource={"path": "books/x/r.txt", "content": {"value": "root r"}, "title": "R root"}) }}
{{ add_page(page={"path": "books/y", "title": "Y from root"}) }}"#,
        ),
        (
            "content/books/_content.html",
            r#"{{ add_page(page={"path": "x", "title": "X from books"}) }}
{{ add_resource(resource={"path": "x/r.txt", "content": {"value": "books r"}, "title": "R books"}) }}
{{ add_page(page={"path": "dup", "title": "Dup from adapter"}) }}"#,
        ),
        (
            "content/books/sub/_content.html",
            r#"{{ add_page(page={"path": "../y", "title": "Y from sub"}) }}"#,
        ),
        ("content/books/dup.md", "---\ntitle: Dup from file\n---\n"),
        (
            "layouts/home.html",
            "{% for p in site.regular_pages %}{{ p.title }}|{{ p.rel_permalink }}|{% for x in p.resources %}{{ x.title }}={{ x | resource_content }};{% endfor %}\n{% endfor %}",
        ),
        ("layouts/single.html", "{{ page.title }}"),
        ("layouts/list.html", "{{ page.title }}"),
    ]);
    assert_eq!(
        text(&r, "index.html"),
        "Dup from file|/books/dup/|\nX from books|/books/x/|R books=books r;\n\
         Y from sub|/books/y/|\n"
    );
    assert_eq!(text(&r, "books/x/r.txt"), "books r");
    // One diagnostic per id is kept (Go's `Warnidf`).
    let warnings: Vec<&str> = r
        .diagnostics
        .iter()
        .filter_map(|d| d.id.as_deref())
        .filter(|id| id.starts_with("duplicate-"))
        .collect();
    assert_eq!(
        warnings,
        ["duplicate-content-path", "duplicate-resource-path"]
    );
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.message.contains("the one that runs later wins")
                && d.message.contains("books/_content.html is used")),
        "{:?}",
        r.diagnostics
    );
}

/// Adapters exist to make many pages from data: one adapter adds 20,000 pages and 20,000
/// resources, then a few of the paths again. Each repeat replaces the earlier page (resource)
/// with a warning, a content file keeps its path, and a later adapter wins over this one.
/// `add_page`, `add_resource` and the model's assembly find an earlier item of a path through
/// maps of the paths: with scans of the items added so far (quadratic) the model phase of this
/// build took 13–16 s in a test build, with the maps 1.5 s; the bound only catches the former.
#[test]
fn many_pages_and_resources() {
    const N: usize = 20_000;
    let adapter = format!(
        r#"{{%- for i in range(end={N}) %}}
{{{{- add_page(page={{"path": "p" ~ i, "title": "t" ~ i, "build": {{"render": "never"}} }}) }}}}
{{{{- add_resource(resource={{"path": "p" ~ i ~ "/r.txt", "content": {{"value": "r" ~ i}} }}) }}}}
{{%- endfor %}}
{{{{- add_page(page={{"path": "P7", "title": "t7 again", "build": {{"render": "never"}} }}) }}}}
{{{{- add_page(page={{"path": "p{last}", "title": "last again", "build": {{"render": "never"}} }}) }}}}
{{{{- add_page(page={{"path": "p5", "title": "t5 again"}}) }}}}
{{{{- add_resource(resource={{"path": "p7/r.txt", "content": {{"value": "r7 again"}} }}) }}}}
{{{{- add_resource(resource={{"path": "p5/r.txt", "content": {{"value": "r5 again"}} }}) }}}}"#,
        last = N - 1
    );
    let list = format!(
        r#"{{%- if page.path == "/books" %}}{{{{ page.pages | length }}}} pages
{{%- for path in ["p0", "p5", "p7", "p8", "p{last}"] %}}
{{%- set p = get_page(path="/books/" ~ path) %}}
{{{{ path }}}}: {{{{ p.title }}}}|{{%- for x in p.resources %}}{{{{ x.name }}}}={{{{ x | resource_content }}}};{{%- endfor %}}
{{%- endfor %}}{{%- endif %}}"#,
        last = N - 1
    );
    let (_tmp, r) = build_ok(&[
        (
            "config.toml",
            "baseURL = \"https://example.com/\"\n\
             disableKinds = [\"taxonomy\", \"term\", \"rss\", \"sitemap\"]\n",
        ),
        ("content/books/_index.md", "---\ntitle: Books\n---\n"),
        ("content/books/_content.html", &adapter),
        (
            "content/books/sub/_content.html",
            r#"{{ add_page(page={"path": "../p8", "title": "t8 from sub", "build": {"render": "never"} }) }}
{{ add_resource(resource={"path": "../p8/r.txt", "content": {"value": "r8 from sub"} }) }}"#,
        ),
        (
            "content/books/p5/index.md",
            "---\ntitle: t5 from file\n---\n",
        ),
        ("content/books/p5/r.txt", "r5 from file"),
        ("layouts/list.html", &list),
        ("layouts/single.html", "{{ page.title }}"),
    ]);
    assert_eq!(
        text(&r, "books/index.html"),
        format!(
            "{N} pages\np0: t0|r.txt=r0;\np5: t5 from file|r.txt=r5 from file;\n\
             p7: t7 again|r.txt=r7 again;\np8: t8 from sub|r.txt=r8 from sub;\n\
             p{last}: last again|r.txt=r{last};",
            last = N - 1
        )
    );
    // Only the file's page is rendered; the adapter's `p5` (rendered) lost to it.
    assert_eq!(text(&r, "books/p5/index.html"), "t5 from file");
    assert!(!exists(&r, "books/p7/index.html"));
    // The report keeps one warning per id (Go's `Warnidf`).
    let warnings: Vec<&str> = r
        .diagnostics
        .iter()
        .filter_map(|d| d.id.as_deref())
        .filter(|id| id.starts_with("duplicate-"))
        .collect();
    assert_eq!(
        warnings,
        ["duplicate-content-path", "duplicate-resource-path"]
    );
    // The model phase runs the adapters and assembles the model.
    let model = r
        .timings
        .iter()
        .find_map(|&(lap, d)| (lap == "model").then_some(d))
        .expect("model lap");
    assert!(
        model < std::time::Duration::from_secs(8),
        "{N} pages and resources: the model phase took {model:?}"
    );
}
