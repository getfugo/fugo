//! The elements open at a point of a page: what [`super::mark_body`] needs of HTML's parsing.

use super::BODY_ATTR;

/// Elements without content or end tag.
const VOID: &[&str] = &[
    "area", "base", "basefont", "bgsound", "br", "col", "embed", "frame", "hr", "img", "input",
    "keygen", "link", "meta", "param", "source", "track", "wbr",
];
/// Elements whose content is text up to their end tag.
const RAW: &[&str] = &[
    "iframe",
    "noembed",
    "noframes",
    "noscript",
    "plaintext",
    "script",
    "style",
    "textarea",
    "title",
    "xmp",
];
/// The start tags that close an open `p`.
const CLOSE_P: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "center",
    "dd",
    "details",
    "dialog",
    "dir",
    "div",
    "dl",
    "dt",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hgroup",
    "hr",
    "li",
    "listing",
    "main",
    "menu",
    "nav",
    "ol",
    "p",
    "pre",
    "section",
    "summary",
    "table",
    "ul",
    "xmp",
];
/// The elements an implied end does not reach past.
const SCOPE: &[&str] = &[
    "applet", "button", "caption", "html", "marquee", "object", "table", "td", "template", "th",
];
const HEADINGS: &[&str] = &["h1", "h2", "h3", "h4", "h5", "h6"];
/// The elements of SVG and MathML, where `/>` ends an element.
const FOREIGN: &[&str] = &["math", "svg"];

fn is(name: &str, set: &[&str]) -> bool {
    set.iter().any(|s| s.eq_ignore_ascii_case(name))
}

pub(super) const fn ends_name(c: u8) -> bool {
    c.is_ascii_whitespace() || c == b'/' || c == b'>'
}

/// An element open at the point the scan reached.
pub(super) struct Open<'a> {
    pub(super) name: &'a str,
    /// Where an attribute goes into its start tag: before its `>` (or `/>`).
    pub(super) attr_at: usize,
    /// The end of its start tag.
    pub(super) tag_end: usize,
    /// Its element children so far.
    pub(super) children: usize,
    /// Whether it has text of its own so far (more than whitespace).
    pub(super) text: bool,
}

/// The elements open at a point of a page, as a browser parses it (closing what a start tag
/// implies closed, but without the rest of HTML's tree building: the layouts and texts of a
/// site close what they open).
pub(super) struct Tree<'a> {
    html: &'a str,
    pos: usize,
    /// Below the elements, a root (no name) that counts the top level.
    pub(super) stack: Vec<Open<'a>>,
    /// An end tag of no open element was seen.
    pub(super) stray_end: bool,
    /// A start tag has [`BODY_ATTR`].
    marked_tag: bool,
}

/// A piece of the page.
enum Token<'a> {
    Text {
        blank: bool,
    },
    Start {
        name: &'a str,
        attr_at: usize,
        self_closing: bool,
    },
    End {
        name: &'a str,
    },
    /// A comment, a doctype or a processing instruction.
    Other,
}

impl<'a> Tree<'a> {
    pub(super) fn new(html: &'a str) -> Self {
        Self {
            html,
            pos: 0,
            stack: vec![Open {
                name: "",
                attr_at: 0,
                tag_end: 0,
                children: 0,
                text: false,
            }],
            stray_end: false,
            marked_tag: false,
        }
    }

    /// Whether an element of the whole page has [`BODY_ATTR`].
    pub(super) fn marked(mut self) -> bool {
        self.advance_to(self.html.len());
        self.marked_tag
    }

    /// The innermost open element (not the root).
    pub(super) fn parent(&self) -> Option<&Open<'a>> {
        self.stack.get(1..).and_then(<[Open<'a>]>::last)
    }

    pub(super) fn in_template(&self) -> bool {
        self.stack.iter().any(|o| is(o.name, &["template"]))
    }

    fn foreign(&self) -> bool {
        self.stack.iter().any(|o| is(o.name, FOREIGN))
    }

    /// Reads the page up to `at`; false when `at` is inside a tag, a comment or the text of a
    /// script (or the like), where the scan went past it.
    pub(super) fn advance_to(&mut self, at: usize) -> bool {
        while self.pos < at {
            let token = self.token(at);
            self.feed(&token);
        }
        self.pos == at
    }

    /// The next piece from `pos` (text stops at `limit`), with `pos` moved past it.
    fn token(&mut self, limit: usize) -> Token<'a> {
        let html = self.html;
        let b = html.as_bytes();
        let i = self.pos;
        let next = |k: usize| b.get(i + k).copied().unwrap_or(0);
        if b[i] == b'<' {
            if next(1).is_ascii_alphabetic() {
                return self.start_tag();
            }
            if next(1) == b'/' && next(2).is_ascii_alphabetic() {
                let name = self.name(i + 2);
                self.pos = i + 2 + name.len();
                self.attributes();
                return Token::End { name };
            }
            if html[i..].starts_with("<!--") {
                self.pos = html[i + 4..]
                    .find("-->")
                    .map_or(html.len(), |e| i + 4 + e + 3);
                return Token::Other;
            }
            if matches!(next(1), b'!' | b'?' | b'/') {
                self.pos = html[i..].find('>').map_or(html.len(), |e| i + e + 1);
                return Token::Other;
            }
        }
        // Text, up to the next `<` (a `<` that starts no tag is text) or `limit`.
        let end = b[i + 1..]
            .iter()
            .position(|&c| c == b'<')
            .map_or(b.len(), |e| i + 1 + e)
            .min(limit);
        self.pos = end;
        Token::Text {
            blank: html[i..end].trim().is_empty(),
        }
    }

    /// The tag name at `at`.
    fn name(&self, at: usize) -> &'a str {
        let b = self.html.as_bytes();
        let len = b[at..].iter().take_while(|&&c| !ends_name(c)).count();
        &self.html[at..at + len]
    }

    fn start_tag(&mut self) -> Token<'a> {
        let name = self.name(self.pos + 1);
        self.pos += 1 + name.len();
        let (attr_at, self_closing) = self.attributes();
        Token::Start {
            name,
            attr_at,
            self_closing,
        }
    }

    /// Reads a tag's attributes up to its end: where its `>` (or `/>`) is, and whether it ends
    /// with `/>`. Notes a [`BODY_ATTR`].
    fn attributes(&mut self) -> (usize, bool) {
        let html = self.html;
        let b = html.as_bytes();
        let mut i = self.pos;
        loop {
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            match b.get(i) {
                None => {
                    self.pos = b.len();
                    return (b.len(), false);
                }
                Some(b'>') => {
                    self.pos = i + 1;
                    return (i, false);
                }
                Some(b'/') if b.get(i + 1) == Some(&b'>') => {
                    self.pos = i + 2;
                    return (i, true);
                }
                Some(b'/') => i += 1,
                Some(_) => {
                    let len = b[i..]
                        .iter()
                        .take_while(|&&c| !ends_name(c) && c != b'=')
                        .count()
                        .max(1);
                    if b[i..i + len].eq_ignore_ascii_case(BODY_ATTR.as_bytes()) {
                        self.marked_tag = true;
                    }
                    i += len;
                    while i < b.len() && b[i].is_ascii_whitespace() {
                        i += 1;
                    }
                    if b.get(i) == Some(&b'=') {
                        i += 1;
                        while i < b.len() && b[i].is_ascii_whitespace() {
                            i += 1;
                        }
                        i = match b.get(i) {
                            Some(&q @ (b'"' | b'\'')) => b[i + 1..]
                                .iter()
                                .position(|&c| c == q)
                                .map_or(b.len(), |e| i + 1 + e + 1),
                            _ => {
                                i + b[i..]
                                    .iter()
                                    .take_while(|&&c| !c.is_ascii_whitespace() && c != b'>')
                                    .count()
                            }
                        };
                    }
                }
            }
        }
    }

    /// Moves `pos` past the end tag of raw-text element `name` (to the end without one).
    fn skip_raw(&mut self, name: &str) {
        let html = self.html;
        let b = html.as_bytes();
        let n = name.len();
        let close = html[self.pos..]
            .match_indices("</")
            .map(|(k, _)| self.pos + k)
            .find(|&k| {
                b.get(k + 2..k + 2 + n)
                    .is_some_and(|t| t.eq_ignore_ascii_case(name.as_bytes()))
                    && b.get(k + 2 + n).is_none_or(|&c| ends_name(c))
            });
        self.pos = match close {
            Some(k) if !name.eq_ignore_ascii_case("plaintext") => {
                html[k..].find('>').map_or(html.len(), |e| k + e + 1)
            }
            _ => html.len(),
        };
    }

    fn feed(&mut self, token: &Token<'a>) {
        match *token {
            Token::Text { blank } => {
                if !blank && let Some(top) = self.stack.last_mut() {
                    top.text = true;
                }
            }
            Token::Start {
                name,
                attr_at,
                self_closing,
            } => {
                self.implied_ends(name);
                if let Some(top) = self.stack.last_mut() {
                    top.children += 1;
                }
                let foreign = self.foreign();
                if is(name, VOID) || (self_closing && (foreign || is(name, FOREIGN))) {
                    // No content.
                } else if is(name, RAW) && !foreign {
                    self.skip_raw(name);
                } else {
                    self.stack.push(Open {
                        name,
                        attr_at,
                        tag_end: self.pos,
                        children: 0,
                        text: false,
                    });
                }
            }
            Token::End { name } => match self.open_index(name) {
                Some(k) => self.stack.truncate(k),
                None => self.stray_end = true,
            },
            Token::Other => {}
        }
    }

    /// The index of the innermost open element `name`.
    fn open_index(&self, name: &str) -> Option<usize> {
        (1..self.stack.len())
            .rev()
            .find(|&k| self.stack[k].name.eq_ignore_ascii_case(name))
    }

    /// Closes the innermost open element `name` (and what is open in it), unless an element
    /// of [`SCOPE`] or of `scope` is open in it.
    fn close_in_scope(&mut self, name: &str, scope: &[&str]) {
        for k in (1..self.stack.len()).rev() {
            let open = self.stack[k].name;
            if open.eq_ignore_ascii_case(name) {
                self.stack.truncate(k);
                return;
            }
            if is(open, SCOPE) || is(open, scope) {
                return;
            }
        }
    }

    /// Closes what start tag `name` closes: an open `p`, an open list item, a heading.
    fn implied_ends(&mut self, name: &str) {
        if is(name, CLOSE_P) {
            self.close_in_scope("p", &[]);
        }
        if is(name, &["li"]) {
            self.close_in_scope("li", &["ol", "ul"]);
        }
        if is(name, &["dd", "dt"]) {
            self.close_in_scope("dd", &["dl"]);
            self.close_in_scope("dt", &["dl"]);
        }
        // The root has no name, so it is neither.
        let top = self.stack.last().map_or("", |o| o.name);
        if (is(name, HEADINGS) && is(top, HEADINGS))
            || (is(name, &["option"]) && is(top, &["option"]))
        {
            self.stack.pop();
        }
    }
}
