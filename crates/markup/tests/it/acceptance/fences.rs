//! `CodeFences::Plain` against Go: the convert oracle's `cjk` configuration renders fences
//! with `codeFences = false`, i.e. goldmark's own `<pre><code class="language-x">`.

use ssg_base::Value;
use ssg_markup::{
    CodeFences, ExpandedMarkdown, HighlightOptions, Highlighter, HookError, MarkdownOptions,
    NoHooks, SourceContexts, render,
};

use super::{CONVERT, GoCfg, PAGE, Row, file, html, options, print};

struct Stub;

impl Highlighter for Stub {
    fn highlight(&self, code: &str, lang: &str, o: &HighlightOptions) -> Result<String, HookError> {
        if lang == "fail" {
            return Err(HookError::new("no lexer"));
        }
        let hl = o.options.get("hl_lines").cloned().unwrap_or(Value::Null);
        Ok(format!(
            "<div class=\"highlight\" data-lang=\"{lang}\" data-hl=\"{hl:?}\" data-ord=\"{}\">{code}</div>\n",
            o.ordinal
        ))
    }
}

/// Under `CodeFences::Hooked` a fence no hook takes goes to the highlighter, with its
/// options and its ordinal among the code blocks; `Plain` never calls it.
#[test]
fn highlighter_seam() {
    let md = "```go {hl_lines=[2]}\na\nb\n```\n";
    let contexts = SourceContexts::default();
    let file = file();
    let src = ExpandedMarkdown {
        text: md,
        page: PAGE,
        contexts: &contexts,
        file: &file,
    };
    let o = MarkdownOptions::default();
    let out = render(&src, &o, &NoHooks, Some(&Stub))
        .expect("renders")
        .html;
    assert_eq!(
        out,
        "<div class=\"highlight\" data-lang=\"go\" data-hl=\"Array([Array([Int(1), Int(1)])])\" data-ord=\"0\">a\nb</div>\n"
    );
    let two = ExpandedMarkdown {
        text: "```a\nx\n```\n\n```b\ny\n```\n",
        ..src
    };
    let out = render(&two, &o, &NoHooks, Some(&Stub))
        .expect("renders")
        .html;
    assert!(
        out.contains("data-lang=\"a\" data-hl=\"Null\" data-ord=\"0\"")
            && out.contains("data-lang=\"b\" data-hl=\"Null\" data-ord=\"1\""),
        "{out}"
    );
    let plain = MarkdownOptions {
        code_fences: CodeFences::Plain,
        ..MarkdownOptions::default()
    };
    let out = render(&src, &plain, &NoHooks, Some(&Stub))
        .expect("renders")
        .html;
    assert_eq!(
        out,
        "<pre><code class=\"language-go\">a\nb\n</code></pre>\n"
    );
    let failing = ExpandedMarkdown {
        text: "x\n\n```fail\ny\n```\n",
        ..src
    };
    let err = render(&failing, &o, &NoHooks, Some(&Stub)).expect_err("fails");
    assert_eq!(
        err.to_string(),
        "content/doc.md:3:1: code block render hook: no lexer"
    );
}

fn pres(html: &str) -> Vec<String> {
    let mut v = Vec::new();
    let mut rest = html;
    while let Some(s) = rest.find("<pre") {
        let e = rest[s..].find("</pre>").map_or(rest.len(), |e| s + e + 6);
        v.push(rest[s..e].to_owned());
        rest = &rest[e..];
    }
    v
}

#[test]
fn plain_fences_equal_go() {
    let o = options(GoCfg::Cjk);
    assert_eq!(o.code_fences, CodeFences::Plain);

    // The testsite's fence (tools/rust-port/i01/testsite.txtar, `content/about.md`), whose
    // Go output is goldmark's plain fence.
    let testsite = html("```html\n<div class=\"fenced\">\n```\n", &o);
    assert_eq!(
        testsite,
        "<pre><code class=\"language-html\">&lt;div class=&quot;fenced&quot;&gt;\n</code></pre>\n"
    );

    // Every fence of the docs corpus, byte-exact; the first 20 docs pages with fences are
    // the A-D1 sample the plan names.
    let (mut pages, mut first20, mut total, mut ok) = (0, 0, 0, 0);
    for case in CONVERT
        .iter()
        .filter(|c| c.name.starts_with("docs/content/"))
    {
        let Some(want) = &case.html[GoCfg::Cjk as usize] else {
            continue;
        };
        let want = pres(want);
        if want.is_empty() {
            continue;
        }
        let got = pres(&html(&case.md, &o));
        let equal = want == got;
        pages += 1;
        if pages <= 20 {
            first20 += usize::from(equal);
        }
        total += want.len();
        ok += want.iter().zip(&got).filter(|(a, b)| a == b).count();
    }
    print(
        "CodeFences::Plain (byte-exact <pre> blocks)",
        &[
            Row::new("testsite fence", 1, 1),
            Row::new("first 20 docs pages with fences", first20, 20.min(pages)),
            Row::new("all docs <pre> blocks", ok, total),
        ],
    );
    assert_eq!(first20, 20);
    assert!(ok * 100 >= total * 99, "{ok}/{total}");
}
