//! The content oracles of `testdata/oracle/sitebuild/content`: Go's `.Content`,
//! `.Summary`, `.Truncated`, `.Plain`, `.WordCount`, `.FuzzyWordCount` and `.ReadingTime` of
//! every page per (site, output format), for the `content` site (shortcodes, hooks, summaries,
//! CJK, HTML content, a bundled content page, a JSON table hook, an RSS heading hook) and the
//! shortcode syntax matrix (`shortcodes`).
//!
//! The Go layouts of the fixtures are replaced by their Tera conversions below; content files
//! keep Go's shortcode syntax (the one inline shortcode body is Tera, as inline shortcode
//! bodies are templates). Differences are listed in [`EXPECTED`] with their reason.

use std::collections::BTreeMap;

use serde_json::Value as J;
use ssg_base::FormatId;
use ssg_view::{ContentRenderer, HookVariant, RenderedContent};

use crate::support::{Site, content_scope, session_in, write};

/// `{"head":…,"body":…}` of a table, cells as `"<alignment>:<text>"` (Go's `printf "%s:%s"`,
/// where alignment none prints nothing).
const JSON_TABLE: &str = r#"{"head":[{% for r in thead %}{% if not loop.first %},{% endif %}[{% for c in r %}{% if not loop.first %},{% endif %}{% if c.alignment == "none" %}{{ (":" ~ c.text) | jsonify }}{% else %}{{ (c.alignment ~ ":" ~ c.text) | jsonify }}{% endif %}{% endfor %}]{% endfor %}],"body":[{% for r in tbody %}{% if not loop.first %},{% endif %}[{% for c in r %}{% if not loop.first %},{% endif %}{% if c.alignment == "none" %}{{ (":" ~ c.text) | jsonify }}{% else %}{{ (c.alignment ~ ":" ~ c.text) | jsonify }}{% endif %}{% endfor %}]{% endfor %}]}"#;

const CONTENT_LAYOUTS: &[(&str, &str)] = &[
    (
        "layouts/_markup/render-blockquote.html",
        r#"<blockquote class="{{ type }}">{{ text }}</blockquote>"#,
    ),
    (
        "layouts/_markup/render-codeblock.html",
        r#"<pre class="cb" data-lang="{{ type }}"><code>{{ inner }}</code></pre>"#,
    ),
    (
        "layouts/_markup/render-heading.html",
        r#"<h{{ level }} id="{{ anchor }}">{{ text }} #{{ level }}</h{{ level }}>"#,
    ),
    (
        "layouts/_markup/render-heading.rss.xml",
        r"<h{{ level }}>{{ text }}</h{{ level }}>",
    ),
    (
        "layouts/_markup/render-image.html",
        r#"<img src="{{ destination | safe }}" alt="{{ plain_text }}">"#,
    ),
    (
        "layouts/_markup/render-link.html",
        r#"<a href="{{ destination | safe }}"{% if title %} title="{{ title }}"{% endif %}>{{ text }}</a>"#,
    ),
    ("layouts/_markup/render-table.json.json", JSON_TABLE),
    (
        "layouts/_shortcodes/deindent.html",
        "<pre>{{ inner_deindent }}</pre>\n<p>second\nline</p>",
    ),
    ("layouts/_shortcodes/fmt.html", "<b>html fmt</b>"),
    ("layouts/_shortcodes/fmt.rss.xml", "rss fmt"),
    ("layouts/_shortcodes/hl.html", "<mark>{{ inner }}</mark>"),
    (
        "layouts/_shortcodes/inner.html",
        r#"<span data-parent="{% if shortcode.parent %}{{ shortcode.parent.name }}{{ shortcode.parent.ordinal }}{% endif %}" data-ord="{{ shortcode.ordinal }}">{{ inner }}</span>"#,
    ),
    ("layouts/_shortcodes/md.html", "{{ inner }}"),
    (
        "layouts/_shortcodes/named.html",
        r#"[{{ shortcode | arg(name="a", default="") }}-{{ shortcode | arg(name="b", default="") }}-{{ shortcode | arg(index=0, default="") }}-{{ shortcode.is_named_params }}]"#,
    ),
    (
        "layouts/_shortcodes/outer.html",
        r#"<div class="outer" data-ord="{{ shortcode.ordinal }}">{{ inner }}</div>"#,
    ),
    (
        "layouts/_shortcodes/pos.html",
        r#"[{{ shortcode | arg(index=0, default="") }}|{{ shortcode | arg(index=1, default="") }}|{{ shortcode | arg(index=9, default="") }}|{{ shortcode.is_named_params }}|{{ shortcode.ordinal }}]"#,
    ),
    ("layouts/_shortcodes/title.html", "{{ page.title }}"),
    (
        "layouts/_shortcodes/toc.html",
        r#"<nav class="toc">{{ page_toc(page=page) }}</nav>"#,
    ),
    ("layouts/_shortcodes/v1.html", "<em>{{ inner }}</em>"),
];

const SHORTCODES_LAYOUTS: &[(&str, &str)] = &[
    (
        "layouts/_shortcodes/inner.html",
        "{% if inner %}{{ inner }}{% endif %}",
    ),
    ("layouts/_shortcodes/inner2.html", "{{ inner }}"),
    ("layouts/_shortcodes/inner3.html", "{{ inner }}"),
    ("layouts/_shortcodes/legacytag.html", "tag"),
    ("layouts/_shortcodes/sc1.html", "sc1"),
    ("layouts/_shortcodes/sc2.html", "sc2"),
    ("layouts/_shortcodes/tag.html", "tag"),
];

/// Content-file rewrites: inline shortcode bodies are Tera templates.
const CONTENT_REWRITES: &[(&str, &str, &str)] = &[(
    "content/posts/shortcodes.md",
    "{{ .Page.Title }}",
    "{{ page.title }}",
)];

/// Accepted differences: (fixture, lang, path, format or `*`, field or `*`, reason).
const EXPECTED: &[(&str, &str, &str, &str, &str, &str)] = &[
    (
        "content",
        "en",
        "/posts/manual-lead",
        "*",
        "truncated",
        "a divider as the first text of a body is a divider (pageparser `divider_at_start`): \
         the summary is manual and empty, so truncated; Go reads it as text after its \
         front matter lexer and reports a front matter summary",
    ),
    (
        "shortcodes",
        "en",
        "/p02",
        "*",
        "*",
        "Go's legacy shortcode version 1 is not reproduced (D5): `{{% legacytag %}}` output is \
         Markdown like any `{{% %}}` output",
    ),
];

/// The fields compared, with their name in the oracle.
fn fields(c: &RenderedContent) -> Vec<(&'static str, J)> {
    vec![
        ("content", J::from(c.html.as_str())),
        ("summary", J::from(c.summary.as_str())),
        ("truncated", J::from(c.truncated)),
        ("plain", J::from(c.plain.as_str())),
        ("wordCount", J::from(c.word_count)),
        ("fuzzyWordCount", J::from(c.fuzzy_word_count)),
        ("readingTime", J::from(c.reading_time)),
    ]
}

fn expected(fixture: &str, lang: &str, path: &str, format: &str, field: &str) -> bool {
    EXPECTED.iter().any(|&(fx, l, p, f, k, _)| {
        fx == fixture
            && l == lang
            && p == path
            && (f == "*" || f == format)
            && (k == "*" || k == field)
    })
}

struct Tally {
    compared: usize,
    equal: usize,
    accepted: usize,
    unexpected: Vec<String>,
}

fn run(fixture: &str, layouts: &[(&str, &str)]) -> Tally {
    let fx: J =
        ssg_testkit::fixture::oracle(&format!("oracle/sitebuild/content/{fixture}.json.gz"));
    let site = &fx["site"];
    let mut files = vec![(
        "config.toml".to_owned(),
        site["toml"].as_str().expect("toml").to_owned(),
    )];
    for f in site["files"].as_array().expect("files") {
        let path = f["path"].as_str().expect("path");
        if path.starts_with("layouts/") {
            continue;
        }
        let mut content = f["content"].as_str().unwrap_or_default().to_owned();
        for (p, from, to) in CONTENT_REWRITES {
            if *p == path {
                content = content.replace(from, to);
            }
        }
        files.push((path.to_owned(), content));
    }
    files.extend(
        layouts
            .iter()
            .map(|(p, t)| ((*p).to_owned(), (*t).to_owned())),
    );
    let dir = write(&files);
    let s = Site {
        session: session_in(dir.path(), &|_, _| {}),
        dir,
    };
    let session = &s.session;
    session.render_content().expect("content phase");
    let model = session.model();
    let dump = &fx["dump"];
    let pages = dump["pages"].as_array().expect("pages");
    let mut t = Tally {
        compared: 0,
        equal: 0,
        accepted: 0,
        unexpected: Vec::new(),
    };
    for step in dump["steps"].as_array().expect("steps") {
        let format_name = step["format"].as_str().expect("format");
        let fid: FormatId = model
            .config
            .output_formats
            .by_name(format_name)
            .expect("format");
        let variant = if session.variants().contains(&HookVariant::Format(fid)) {
            HookVariant::Format(fid)
        } else {
            HookVariant::Html
        };
        for v in step["values"].as_array().expect("values") {
            let page = &pages[usize::try_from(v["page"].as_u64().expect("page")).expect("idx")];
            let path = page["path"].as_str().expect("path");
            let lang = page["lang"].as_str().expect("lang");
            let li = model
                .config
                .sites
                .iter()
                .position(|s| s.language.key == lang)
                .expect("lang");
            let id = s.page(path, li);
            let got = session.content(id, variant, &content_scope(session, id, variant));
            let got = match got {
                Ok(c) => c,
                Err(e) => {
                    t.compared += 1;
                    if !expected(fixture, lang, path, format_name, "*") {
                        t.unexpected.push(format!(
                            "{fixture} {lang} {path} [{format_name}]: error {e}"
                        ));
                    } else {
                        t.accepted += 1;
                    }
                    continue;
                }
            };
            for (field, value) in fields(&got) {
                t.compared += 1;
                if v[field] == value {
                    t.equal += 1;
                } else if expected(fixture, lang, path, format_name, field) {
                    t.accepted += 1;
                } else {
                    t.unexpected.push(format!(
                        "{fixture} {lang} {path} [{format_name}] {field}:\n   got  {value}\n   want {}",
                        v[field]
                    ));
                }
            }
        }
    }
    t
}

#[test]
fn content_oracles() {
    let mut all = Vec::new();
    let mut table = BTreeMap::new();
    for (fixture, layouts) in [
        ("content", CONTENT_LAYOUTS),
        ("shortcodes", SHORTCODES_LAYOUTS),
    ] {
        let t = run(fixture, layouts);
        table.insert(
            fixture,
            (t.compared, t.equal, t.accepted, t.unexpected.len()),
        );
        all.extend(t.unexpected);
    }
    for (fx, (n, eq, acc, bad)) in &table {
        eprintln!("content oracle {fx}: {n} values, {eq} equal, {acc} accepted, {bad} unexpected");
    }
    for u in all.iter().take(40) {
        eprintln!("{u}");
    }
    assert!(all.is_empty(), "{} unexpected differences", all.len());
}
