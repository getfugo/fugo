//! Context spans: a hook's `inner_page` is the page of the innermost span containing the
//! node (Go's `.PageInner` for `render_shortcodes` includes).

use std::ops::Range;
use std::sync::Mutex;

use ssg_base::PageId;
use ssg_markup::{
    BlockquoteCtx, CodeBlockCtx, ExpandedMarkdown, HeadingCtx, HookEnv, HookError, HookOut, Hooks,
    LinkCtx, MarkdownOptions, SourceContexts, TableCtx, render,
};

use super::{PAGE, file};

/// Records `(kind, what, inner_page, line)` of every hook call.
#[derive(Default)]
struct Pages(Mutex<Vec<(&'static str, String, PageId, u32)>>);

impl Pages {
    fn push(&self, kind: &'static str, what: &str, env: &HookEnv) {
        self.0.lock().expect("lock").push((
            kind,
            what.to_owned(),
            env.inner_page,
            env.position.line,
        ));
    }
}

impl Hooks for Pages {
    fn link(&self, env: &HookEnv, c: &LinkCtx) -> Result<HookOut, HookError> {
        self.push("link", &c.destination, env);
        Ok(HookOut::Default)
    }
    fn image(&self, env: &HookEnv, c: &LinkCtx) -> Result<HookOut, HookError> {
        self.push("image", &c.destination, env);
        Ok(HookOut::Default)
    }
    fn heading(&self, env: &HookEnv, c: &HeadingCtx) -> Result<HookOut, HookError> {
        self.push("heading", &c.anchor, env);
        Ok(HookOut::Default)
    }
    fn code_block(&self, env: &HookEnv, c: &CodeBlockCtx) -> Result<HookOut, HookError> {
        self.push("code", &c.lang, env);
        Ok(HookOut::Default)
    }
    fn blockquote(&self, env: &HookEnv, _: &BlockquoteCtx) -> Result<HookOut, HookError> {
        self.push("blockquote", "", env);
        Ok(HookOut::Default)
    }
    fn table(&self, env: &HookEnv, _: &TableCtx) -> Result<HookOut, HookError> {
        self.push("table", "", env);
        Ok(HookOut::Default)
    }
}

fn pages(md: &str, spans: Vec<(Range<usize>, PageId)>) -> Vec<(&'static str, String, PageId, u32)> {
    let contexts = SourceContexts(spans);
    let file = file();
    let src = ExpandedMarkdown {
        text: md,
        page: PAGE,
        contexts: &contexts,
        file: &file,
    };
    let h = Pages::default();
    render(&src, &MarkdownOptions::default(), &h, None).expect("renders");
    h.0.into_inner().expect("lock")
}

fn span(md: &str, part: &str) -> Range<usize> {
    let at = md.find(part).expect("part present");
    at..at + part.len()
}

const P7: PageId = PageId::from_raw(7);
const P9: PageId = PageId::from_raw(9);

/// The hooks oracle's adversarial document with inline context markers, the Go
/// implementation's textual markers turned into spans.
#[test]
fn inner_page_of_included_source() {
    let inside = "Inside **bold** [link](http://x) ![img](i.png)\n";
    let heading = "## Heading in ctx\n";
    let md = format!("Before\n{inside}After\n\n{heading}\nOutro [l](x)\n");
    let got = pages(&md, vec![(span(&md, inside), P7), (span(&md, heading), P9)]);
    assert_eq!(
        got,
        vec![
            ("link", "http://x".to_owned(), P7, 2),
            ("image", "i.png".to_owned(), P7, 2),
            ("heading", "heading-in-ctx".to_owned(), P9, 5),
            // Go keeps the last context for what follows an include; spans do not leak.
            ("link", "x".to_owned(), PAGE, 7),
        ]
    );
}

#[test]
fn innermost_span_wins_for_blocks_and_inlines() {
    let inner = "> quote [in](a)\n";
    let outer = format!("| t |\n|---|\n| [c](b) |\n\n{inner}\n```go\nx\n```\n");
    let md = format!("# Top\n\n{outer}\n[after](z)\n");
    let got = pages(&md, vec![(span(&md, &outer), P7), (span(&md, inner), P9)]);
    let kinds: Vec<_> = got
        .iter()
        .map(|(k, w, p, _)| (*k, w.as_str(), *p))
        .collect();
    assert_eq!(
        kinds,
        vec![
            ("heading", "top", PAGE),
            ("link", "b", P7),
            ("table", "", P7),
            ("link", "a", P9),
            ("blockquote", "", P9),
            ("code", "go", P7),
            ("link", "z", PAGE),
        ]
    );
}

/// comrak reports the inlines of a paragraph that began with link reference definitions a
/// line too early; the fix must put them back so the span decides correctly.
#[test]
fn inlines_after_link_reference_definitions() {
    let included = "text [a] more ![i](p.png)\n";
    let md = format!("[a]: /x\n[b]: {{{{% y %}}}}\n{included}");
    let got = pages(&md, vec![(span(&md, included), P7)]);
    assert_eq!(
        got,
        vec![
            ("link", "/x".to_owned(), P7, 3),
            ("image", "p.png".to_owned(), P7, 3),
        ]
    );
}

/// Cells of a table goldmark makes of lazy continuation lines are parsed apart from the page;
/// their inlines keep their lines and columns, so spans and positions still apply.
#[test]
fn table_cells_keep_positions() {
    let rows = "  | [c](b) | ![i](p.png) |\n";
    let md = format!("1. Intro [a](x):\n  | h | [t](y) |\n  |---|---|\n{rows}\nAfter [z](w)\n");
    let got = pages(&md, vec![(span(&md, rows), P7)]);
    assert_eq!(
        got,
        vec![
            ("link", "x".to_owned(), PAGE, 1),
            ("link", "y".to_owned(), PAGE, 2),
            ("link", "b".to_owned(), P7, 4),
            ("image", "p.png".to_owned(), P7, 4),
            ("table", String::new(), PAGE, 2),
            ("link", "w".to_owned(), PAGE, 6),
        ]
    );
}
