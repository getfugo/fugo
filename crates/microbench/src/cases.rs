//! The cases: the name of a Go benchmark and a call of the Rust function that does its work,
//! with the Go benchmark's input, checking the result where the Go benchmark checks it. The Go
//! source of each is named in its comment (`<package>/<file>`, Go tree at `44529028`); two Go
//! inputs name the Go version's former project, and use `fugo` in its place here (same length).

use std::hint::black_box;

use ssg_base::anchor::{Style, anchorize};
use ssg_base::paths::sanitize;
use ssg_base::url::{BaseUrl, SiteUrls};
use ssg_markup::text::{auto_summary, split_at_marker, strip_html, word_count};
use ssg_pageparser::{
    FrontMatterFormat, LexOptions, Start, SummaryDivider, decode_front_matter_map, lex_with,
};

/// One benchmark: its Go name, the Go package that has it at `44529028` (none when the Go
/// benchmark was removed earlier), and the work of one iteration.
pub struct Case {
    pub name: &'static str,
    pub go_package: Option<&'static str>,
    pub run: Box<dyn Fn()>,
}

fn case(name: &'static str, go_package: &'static str, run: impl Fn() + 'static) -> Case {
    Case {
        name,
        go_package: (!go_package.is_empty()).then_some(go_package),
        run: Box::new(run),
    }
}

/// The arguments of `go test` (in the Go tree at `44529028`) that run the Go benchmarks of the
/// cases: `-bench=^(…)$`, `-benchtime=…` and the packages.
#[must_use]
pub fn go_test_args() -> Vec<String> {
    let all = all();
    let mut names: Vec<&str> = all
        .iter()
        .filter(|c| c.go_package.is_some())
        .map(|c| c.name.split('/').next().unwrap_or(c.name))
        .collect();
    names.sort_unstable();
    names.dedup();
    let mut packages: Vec<String> = all
        .iter()
        .filter_map(|c| c.go_package)
        .map(|p| format!("./{p}"))
        .collect();
    packages.sort();
    packages.dedup();
    let mut args = vec![
        format!("-bench=^({})$", names.join("|")),
        "-benchtime=500ms".to_owned(),
    ];
    args.extend(packages);
    args
}

/// `helpers/content_test.go` (`tstHTMLContent`, until the benchmark was removed in 2022).
const STRIP_HTML: &str = "<!DOCTYPE html><html><head><script src=\"http://two/foobar.js\"></script></head><body><nav><ul><li fugo-nav=\"section_0\"></li><li fugo-nav=\"section_1\"></li></ul></nav><article>content <a href=\"http://two/foobar\">foobar</a>. Follow up</article><p>This is some text.<br>And some more.</p></body></html>";

/// `parser/metadecoders/decoder_test.go`.
const YAML: &str = "\na:\n  v1: 32\n  v2: 43\n  v3: \"foo\"\nb:\n  - a\n  - b\nc: \"d\"\n\n";

/// `parser/pageparser/pageparser_test.go`: front matter, a summary divider, then ten times 30
/// "this is text" and a shortcode with inner content.
fn parse_input() -> Vec<u8> {
    let start = "\n\n\n---\ntitle: \"Front Matters\"\ndescription: \"It really does\"\n---\n\nThis is some summary. This is some summary. This is some summary. This is some summary.\n\n <!--more-->\n\n\n";
    let shortcode = "{{< myshortcode >}}This is some inner content.{{< /myshortcode >}}";
    let body = format!("{}{shortcode}", "this is text".repeat(30)).repeat(10);
    format!("{start}{body}").into_bytes()
}

/// The base URL `https://base/` (`helpers/url_test.go`).
fn base_urls() -> SiteUrls {
    SiteUrls {
        base_url: BaseUrl::parse("https://base/").expect("a valid base URL"),
        language_prefix: String::new(),
        link_style: Default::default(),
        path_case: Default::default(),
        accents: Default::default(),
    }
}

/// Every case, in the order the page lists them.
#[must_use]
pub fn all() -> Vec<Case> {
    let anchor = "God is good: 神真美好";
    let urls = std::rc::Rc::new(base_urls());
    let (u1, u2, u3) = (urls.clone(), urls.clone(), urls);
    let page = parse_input();
    let words = "Fugo Rocks ".repeat(200);
    vec![
        // markup/goldmark/autoid_test.go
        case(
            "BenchmarkSanitizeAnchorName",
            "markup/goldmark",
            move || {
                assert_eq!(anchorize(black_box(anchor), Style::Github).len(), 24);
            },
        ),
        case(
            "BenchmarkSanitizeAnchorNameAsciiOnly",
            "markup/goldmark",
            move || {
                assert_eq!(anchorize(black_box(anchor), Style::GithubAscii).len(), 12);
            },
        ),
        case(
            "BenchmarkSanitizeAnchorNameBlackfriday",
            "markup/goldmark",
            move || {
                assert_eq!(anchorize(black_box(anchor), Style::Blackfriday).len(), 24);
            },
        ),
        case(
            "BenchmarkSanitizeAnchorNameString",
            "markup/goldmark",
            move || {
                assert_eq!(anchorize(black_box(anchor), Style::Github).len(), 24);
            },
        ),
        // common/paths/path_test.go
        case("BenchmarkSanitize/All_allowed", "common/paths", || {
            assert_eq!(sanitize(black_box("foo/bar")), "foo/bar");
        }),
        case("BenchmarkSanitize/Spaces", "common/paths", || {
            assert_eq!(sanitize(black_box("foo bar")), "foo-bar");
        }),
        // helpers/url_test.go
        case("BenchmarkRelURL", "helpers", move || {
            black_box(u1.rel_url(black_box("https://base/foo/bar")));
        }),
        case("BenchmarkAbsURL/relurl", "helpers", move || {
            black_box(u2.abs_url(black_box("foo/bar")));
        }),
        case("BenchmarkAbsURL/absurl", "helpers", move || {
            black_box(u3.abs_url(black_box("https://base/foo/bar")));
        }),
        // helpers/content_test.go
        case("BenchmarkTotalWords", "helpers", move || {
            assert_eq!(word_count(black_box(&words), false), 400);
        }),
        case("BenchmarkStripHTML", "", || {
            black_box(strip_html(black_box(STRIP_HTML)));
        }),
        // parser/pageparser/pageparser_test.go
        case("BenchmarkParse", "parser/pageparser", move || {
            let opts = LexOptions {
                start: Start::Page,
                summary_divider: SummaryDivider::Html,
            };
            assert!(lex_with(black_box(&page), opts).into_result().is_ok());
        }),
        // parser/metadecoders/decoder_test.go
        case("BenchmarkDecodeYAMLToMap", "parser/metadecoders", || {
            assert!(decode_front_matter_map(FrontMatterFormat::Yaml, black_box(YAML)).is_ok());
        }),
        // resources/page/page_markup_test.go
        case("BenchmarkSummaryFromHTML", "resources/page", || {
            let html = "<p>First paragraph</p><p>Second paragraph</p>";
            let s = auto_summary(black_box(html), 2, false);
            assert_eq!(s.html, "<p>First paragraph</p>");
        }),
        case(
            "BenchmarkSummaryFromHTMLWithDivider",
            "resources/page",
            || {
                let html = "<p>First paragraph</p><p>FOOO</p><p>Second paragraph</p>";
                let (summary, rest) =
                    split_at_marker(black_box(html), "FOOO").expect("the divider");
                assert_eq!(summary, "<p>First paragraph</p>");
                assert_eq!(rest, "<p>Second paragraph</p>");
            },
        ),
    ]
}
