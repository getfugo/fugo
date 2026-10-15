//! The mark of the element that holds a page's text (`data-cms-body`), which the editor's
//! preview puts the text being edited into.

use ssg_cms::body::mark_body;

const TEXT: &str = "<h2 id=\"a\">A</h2>\n<p>One.</p>\n<p>Two.</p>\n";

/// The page `layout` prints, with `{}` for the text, marked.
fn marked(layout: &str) -> Option<String> {
    mark_body(&layout.replace("{}", TEXT), TEXT)
}

/// The page `layout` prints, with `{}` for the text, and `mark` in place of `@`.
fn expected(layout: &str, mark: &str) -> String {
    layout.replace("{}", TEXT).replace('@', mark)
}

#[test]
fn an_element_that_holds_the_text_alone_is_marked() {
    let layout = "<!doctype html><html><head><title>T</title></head><body>\
                  <main><h1>T</h1><div class=\"content\"@>\n{}\n</div></main></body></html>";
    assert_eq!(
        marked(&layout.replace('@', "")),
        Some(expected(layout, " data-cms-body"))
    );
}

#[test]
fn text_among_other_elements_is_marked_by_its_children() {
    // A title before it, a void element, a comment and a footer after it.
    let layout = "<body><article@><h1>T</h1><hr><!-- text -->{}<footer>F</footer></article></body>";
    assert_eq!(
        marked(&layout.replace('@', "")),
        Some(expected(layout, " data-cms-body=\"2..5\""))
    );
}

#[test]
fn what_a_hook_adds_to_the_text_does_not_matter() {
    // A heading hook that adds an anchor link: the text as printed is the text.
    let text = "<h2 id=\"t\">Title <a href=\"#t\">#</a></h2>\n<p>Body.</p>\n";
    let page = format!("<main><div class=\"content\">{text}</div></main>");
    assert_eq!(
        mark_body(&page, text).as_deref(),
        Some(format!("<main><div class=\"content\" data-cms-body>{text}</div></main>").as_str())
    );
}

#[test]
fn the_text_in_an_attribute_a_script_a_comment_or_a_template_is_passed_over() {
    let text = "<p>Hello, world.</p>";
    for before in [
        "<script type=\"application/ld+json\">{\"t\": \"<p>Hello, world.</p>\"}</script>",
        "<!-- <p>Hello, world.</p> -->",
        "<template><div><p>Hello, world.</p></div></template>",
        "<textarea><p>Hello, world.</p></textarea>",
        "<div title=\"<p>Hello, world.</p>\"></div>",
    ] {
        let page = format!("<body>{before}<section>{text}</section></body>");
        assert_eq!(
            mark_body(&page, text),
            Some(format!(
                "<body>{before}<section data-cms-body>{text}</section></body>"
            )),
            "{before}"
        );
    }
}

#[test]
fn plain_text_is_found_where_it_is_text() {
    let text = "Just words, no tags.";
    let page = format!("<head><meta name=\"description\" content=\"{text}\"></head><p>{text}</p>");
    assert_eq!(
        mark_body(&page, text),
        Some(format!(
            "<head><meta name=\"description\" content=\"{text}\"></head><p data-cms-body>{text}</p>"
        ))
    );
}

#[test]
fn start_tags_close_what_a_browser_closes() {
    // `<h2>` closes the open `p`, so the text's paragraphs are the article's children 2 and 3.
    let text = "<p>x</p><p>y</p>";
    let page = format!("<article><p>Lead<h2>T</h2>{text}<footer>F</footer></article>");
    assert_eq!(
        mark_body(&page, text),
        Some(format!(
            "<article data-cms-body=\"2..4\"><p>Lead<h2>T</h2>{text}<footer>F</footer></article>"
        ))
    );
    // Unclosed list items close each other.
    let page = format!("<ul><li>a<li>b</ul><div class=c>{text}</div>");
    assert_eq!(
        mark_body(&page, text),
        Some(format!(
            "<ul><li>a<li>b</ul><div class=c data-cms-body>{text}</div>"
        ))
    );
}

#[test]
fn tags_are_read_whatever_their_case_and_attributes() {
    let text = "<p>T</p>";
    for tag in [
        "<DIV CLASS=content>",
        "<div data-x='a > b'>",
        "<div class=\"a\"\n  id=b>",
        "<svg><path d=\"M0 0\"/></svg><div>",
    ] {
        let page = format!("{tag}{text}</div>");
        let marked = mark_body(&page, text).unwrap_or_default();
        assert!(marked.contains(" data-cms-body>"), "{tag}: {marked}");
        assert_eq!(marked.replace(" data-cms-body", ""), page, "{tag}");
    }
    // `/>` ends an SVG element, not an HTML one.
    let page = format!("<div><svg/>{text}</div>");
    assert_eq!(
        mark_body(&page, text),
        Some(format!("<div data-cms-body=\"1..2\"><svg/>{text}</div>"))
    );
}

#[test]
fn text_beyond_ascii_is_found() {
    let text = "<p>Crème brûlée, 日本語のテキスト.</p>";
    let page = format!("<p>Ça va ?</p><div>{text}</div>");
    assert_eq!(
        mark_body(&page, text),
        Some(format!("<p>Ça va ?</p><div data-cms-body>{text}</div>"))
    );
}

#[test]
fn nothing_is_marked_when_the_mark_would_be_a_guess() {
    for (page, text, why) in [
        (
            "<div><p>One!</p></div>",
            "<p>One.</p>",
            "the layout changed the text",
        ),
        (
            "<div data-cms-body>x</div><div><p>One.</p></div>",
            "<p>One.</p>",
            "the page marks an element",
        ),
        (
            "<div><b>Lead:</b> words <em>more</em></div>",
            "words <em>more</em>",
            "words that share their element",
        ),
        (
            "<p>One.</p><p>more</p>",
            "<p>One.</p>",
            "no element holds it",
        ),
        ("<div><p>x</p></div>", "", "no text"),
        (
            "<div>a <i>b</div>",
            "a <i>b",
            "text that opens what it does not close",
        ),
        (
            "<div>a</p> b</div>",
            "a</p> b",
            "text that closes what it did not open",
        ),
    ] {
        assert_eq!(mark_body(page, text), None, "{why}");
    }
}

#[test]
fn elements_beside_words_are_marked_by_their_children() {
    let text = "<p>One.</p>";
    let page = format!("<div><b>Lead:</b> words {text}</div>");
    assert_eq!(
        mark_body(&page, text),
        Some(format!(
            "<div data-cms-body=\"1..2\"><b>Lead:</b> words {text}</div>"
        ))
    );
}

#[test]
fn text_beside_words_is_marked_when_it_holds_the_element_alone() {
    // Text with words at its top level can still be the whole of its element.
    let text = "Words, then <em>more</em>.";
    let page = format!("<p class=lead>\n  {text}\n</p>");
    assert_eq!(
        mark_body(&page, text),
        Some(format!("<p class=lead data-cms-body>\n  {text}\n</p>"))
    );
}
