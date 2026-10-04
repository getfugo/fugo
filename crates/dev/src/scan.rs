//! What the manifest reads from a page: the HTML tokenizer of `html5gum` for HTML (the elements'
//! text states switched as a browser's tree builder would for raw-text and escapable raw-text
//! elements), `quick-xml` for feeds and sitemaps.

use std::sync::OnceLock;

use html5gum::{State, Token, Tokenizer};
use quick_xml::events::{BytesStart, Event};
use regex::Regex;

fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A `<link rel=canonical|alternate>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelLink {
    pub rel: &'static str,
    pub href: String,
    pub hreflang: String,
    pub media_type: String,
}

/// One pass over an HTML document.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Page {
    /// The text of each `<title>`, whitespace collapsed.
    pub title: Vec<String>,
    pub rel_links: Vec<RelLink>,
    /// Every `href` and `src`, and the URLs of every `srcset`, in document order.
    pub urls: Vec<String>,
    /// The target of a `<meta http-equiv=refresh>` (an alias page).
    pub refresh: Option<String>,
    /// The text outside `script` and `style`; every tag boundary is a space, except a `span`'s
    /// inside `pre` or `code` (highlighters wrap tokens in spans that touch).
    pub text: String,
    /// The `id`s of the headings `h1`–`h6`.
    pub ids: Vec<String>,
}

impl Page {
    /// The visible text: typographic quotes, dashes and the no-break space as ASCII, `…` as
    /// `...`, whitespace collapsed.
    #[must_use]
    pub fn visible_text(&self) -> String {
        let text: String = self
            .text
            .chars()
            .map(|c| match c {
                '\u{2018}' | '\u{2019}' | '\u{201a}' => '\'',
                '\u{201c}' | '\u{201d}' | '\u{201e}' | '\u{ab}' | '\u{bb}' => '"',
                '\u{2013}' | '\u{2014}' => '-',
                '\u{a0}' => ' ',
                c => c,
            })
            .collect();
        collapse(&text.replace('\u{2026}', "..."))
    }
}

/// Whitespace runs as one space, none at the ends.
#[must_use]
pub fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The text state an element's content is tokenized in, as the HTML tree builder sets it.
fn text_state(name: &str) -> Option<State> {
    match name {
        "script" => Some(State::ScriptData),
        "style" | "xmp" | "iframe" | "noembed" | "noframes" => Some(State::RawText),
        "textarea" | "title" => Some(State::RcData),
        "plaintext" => Some(State::PlainText),
        _ => None,
    }
}

/// The target of a refresh `content` (`0; url=/to/`).
fn refresh_target(content: &str) -> Option<String> {
    static URL: OnceLock<Regex> = OnceLock::new();
    let re = URL.get_or_init(|| Regex::new(r"(?i)url=([^\n]*)\n?\z").expect("a valid expression"));
    let m = re.captures(content)?;
    Some(m[1].trim().trim_matches(['\'', '"']).to_owned())
}

#[derive(Default)]
struct Scanner {
    page: Page,
    /// Open `script` and `style` elements.
    skip: usize,
    /// Open `pre` and `code` elements.
    code: usize,
    title: Option<String>,
}

impl Scanner {
    fn boundary(&mut self, name: &str) {
        if !(name == "span" && self.code > 0) {
            self.page.text.push(' ');
        }
    }

    fn start(&mut self, name: &str, attr: impl Fn(&str) -> Option<String>, self_closing: bool) {
        self.boundary(name);
        if !self_closing {
            match name {
                "pre" | "code" => self.code += 1,
                "script" | "style" => self.skip += 1,
                "title" => self.title = Some(String::new()),
                _ => {}
            }
        }
        if name == "link"
            && let Some(href) = attr("href")
        {
            let rel = attr("rel").unwrap_or_default().to_lowercase();
            for kind in ["canonical", "alternate"] {
                if rel.split_whitespace().any(|r| r == kind) {
                    self.page.rel_links.push(RelLink {
                        rel: kind,
                        href: href.clone(),
                        hreflang: attr("hreflang").unwrap_or_default(),
                        media_type: attr("type").unwrap_or_default(),
                    });
                }
            }
        }
        self.page
            .urls
            .extend(["href", "src"].iter().filter_map(|k| attr(k)));
        if let Some(srcset) = attr("srcset") {
            let candidates = srcset
                .split(',')
                .filter_map(|c| c.split_whitespace().next().map(str::to_owned));
            self.page.urls.extend(candidates);
        }
        if name == "meta"
            && attr("http-equiv").is_some_and(|v| v.eq_ignore_ascii_case("refresh"))
            && let Some(target) = refresh_target(&attr("content").unwrap_or_default())
        {
            self.page.refresh = Some(target);
        }
        if matches!(name, "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
            && let Some(id) = attr("id")
        {
            self.page.ids.push(id);
        }
    }

    fn end(&mut self, name: &str) {
        self.boundary(name);
        match name {
            "pre" | "code" => self.code = self.code.saturating_sub(1),
            "script" | "style" => self.skip = self.skip.saturating_sub(1),
            "title" => {
                if let Some(t) = self.title.take() {
                    self.page.title.push(collapse(&t));
                }
            }
            _ => {}
        }
    }

    fn text(&mut self, text: &str) {
        if let Some(t) = &mut self.title {
            t.push_str(text);
        }
        if self.skip == 0 {
            self.page.text.push_str(text);
        }
    }
}

/// Scans an HTML document.
#[must_use]
pub fn html(document: &str) -> Page {
    let mut tokenizer = Tokenizer::new(document);
    let mut s = Scanner::default();
    while let Some(Ok(token)) = tokenizer.next() {
        match token {
            Token::StartTag(tag) => {
                let name = lossy(&tag.name);
                let attr = |k: &str| tag.attributes.get(k.as_bytes()).map(|v| lossy(&v.value));
                s.start(&name, attr, tag.self_closing);
                if !tag.self_closing
                    && let Some(state) = text_state(&name)
                {
                    tokenizer.set_state(state);
                }
            }
            Token::EndTag(tag) => s.end(&lossy(&tag.name)),
            Token::String(text) => s.text(&lossy(&text)),
            _ => {}
        }
    }
    s.page
}

/// The `href` attributes of an element, as (`<name> href`, value) items.
fn hrefs(e: &BytesStart<'_>, name: &str, items: &mut Vec<(String, String)>) {
    for a in e.attributes().with_checks(false).flatten() {
        if a.key.as_ref() == "href"
            && let Ok(v) = a.normalized_value(quick_xml::XmlVersion::Implicit1_0)
            && !v.is_empty()
        {
            items.push((format!("{name} href"), v.into_owned()));
        }
    }
}

/// The items of a feed or sitemap, in order: the text of every `link`, `loc` and `guid`
/// element (`""` for an empty one), and every `href` attribute (`<element> href`). Element
/// names are qualified (`atom:link`); CDATA sections are not text. A document that stops being
/// XML ends the list.
#[must_use]
pub fn xml(document: &str) -> Vec<(String, String)> {
    const TEXT: [&str; 3] = ["link", "loc", "guid"];
    let mut reader = quick_xml::Reader::from_str(document);
    reader.config_mut().check_end_names = false;
    let mut items = Vec::new();
    let mut open: Option<(String, String)> = None;
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = e.name().as_ref().to_lowercase();
                hrefs(&e, &name, &mut items);
                if TEXT.contains(&name.as_str()) {
                    open = Some((name, String::new()));
                }
            }
            Ok(Event::Empty(e)) => {
                let name = e.name().as_ref().to_lowercase();
                hrefs(&e, &name, &mut items);
                if TEXT.contains(&name.as_str()) {
                    items.push((name, String::new()));
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name().as_ref().to_lowercase();
                if open.as_ref().is_some_and(|(n, _)| *n == name)
                    && let Some((n, text)) = open.take()
                {
                    items.push((n, text.trim().to_owned()));
                }
            }
            Ok(Event::Text(t)) => {
                if let Some((_, text)) = &mut open {
                    text.push_str(&t.xml10_content());
                }
            }
            Ok(Event::GeneralRef(r)) => {
                if let Some((_, text)) = &mut open {
                    let name = r.xml10_content();
                    match (
                        r.resolve_char_ref(),
                        quick_xml::escape::resolve_predefined_entity(&name),
                    ) {
                        (Ok(Some(c)), _) => text.push(c),
                        (_, Some(s)) => text.push_str(s),
                        _ => {
                            text.push('&');
                            text.push_str(&name);
                            text.push(';');
                        }
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            Ok(_) => {}
        }
    }
    items
}
