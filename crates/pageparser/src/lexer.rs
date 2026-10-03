//! The page lexer: front matter delimiters, the summary divider and shortcode tags.
//!
//! The lexer works on bytes so that it can report the same token boundaries for any input,
//! including bytes that are not UTF-8 (they decode as U+FFFD, one byte each). Tokens are
//! kinds plus byte ranges; nothing is copied.
//!
//! The Go implementation's lexer is the behavioural reference: the token boundaries (including
//! the split of a text run into text and trailing indentation, and the three text pieces of an
//! escaped shortcode `{{</* x */>}}`) are checked against its 135,326 items in
//! `tests/it/lexer.rs`.

use std::collections::HashSet;
use std::fmt;
use std::ops::Range;

use ssg_base::text::{is_digit, is_letter};

use crate::token::{Delim, FrontMatterFormat, Quoting, Token, TokenKind};

const SUMMARY_DIVIDER: &[u8] = b"<!--more-->";
const SUMMARY_DIVIDER_ORG: &[u8] = b"# more";
const BYTE_ORDER_MARK: char = '\u{feff}';

/// Where lexing starts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Start {
    /// At the start of a content file: byte order marks and front matter first.
    #[default]
    Page,
    /// In the body (after the front matter, or a string without any).
    Body,
}

/// Which summary divider the lexer recognises (only its first occurrence is a divider).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SummaryDivider {
    /// `<!--more-->`; with [`Start::Page`], Org front matter switches it to `# more`.
    #[default]
    Html,
    /// `# more` (Org content).
    Org,
    /// No divider: `<!--more-->` is text.
    Off,
}

/// Lexer options.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LexOptions {
    pub start: Start,
    pub summary_divider: SummaryDivider,
}

/// The reason lexing stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LexErrorKind {
    /// A page starts with `-` or `+` that is not a `---`/`+++` delimiter line.
    InvalidDelimiter(FrontMatterFormat),
    /// No closing `---`/`+++` line.
    UnterminatedFrontMatter(FrontMatterFormat),
    /// A page starts with `{` that does not close.
    UnterminatedJson,
    /// `{{</*` without a closing `*/>}}`.
    UnclosedEscape,
    /// A shortcode tag inside an inline shortcode that does not close it.
    InlineNesting,
    /// The source ends inside a shortcode tag.
    UnclosedTag,
    /// A closing `/` before any shortcode name.
    CloseWithoutOpen,
    /// A character that cannot start anything inside a shortcode tag.
    UnexpectedChar(char),
    /// Positional and named arguments mixed in one tag.
    MixedArguments,
    /// A backslash before a backtick.
    InvalidEscape,
    /// A quoted argument without its closing quote on the same line.
    UnterminatedString,
    /// A backtick argument without its closing backtick.
    UnterminatedRawString,
    /// A `.` in a shortcode name that is not `.inline`.
    PeriodInName,
    /// A closing tag for a shortcode that was never opened.
    UnopenedClose(String),
    /// Something other than whitespace between a closing tag's name and its `>}}`.
    JunkAfterClose,
}

impl fmt::Display for LexErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDelimiter(fm) => write!(f, "invalid {fm} front matter delimiter"),
            Self::UnterminatedFrontMatter(fm) => {
                write!(f, "{fm} front matter has no closing delimiter")
            }
            Self::UnterminatedJson => f.write_str("JSON front matter is not closed"),
            Self::UnclosedEscape => f.write_str("escaped shortcode `{{</* */>}}` is not closed"),
            Self::InlineNesting => f.write_str("inline shortcodes cannot contain shortcodes"),
            Self::UnclosedTag => f.write_str("shortcode tag is not closed"),
            Self::CloseWithoutOpen => f.write_str("closing shortcode tag, but none is open"),
            Self::UnexpectedChar(c) => write!(
                f,
                "unexpected {c:?} in shortcode tag (quote arguments that are not alphanumeric)"
            ),
            Self::MixedArguments => {
                f.write_str("positional and named shortcode arguments cannot be mixed")
            }
            Self::InvalidEscape => f.write_str("invalid escape in shortcode argument"),
            Self::UnterminatedString => f.write_str("quoted shortcode argument is not closed"),
            Self::UnterminatedRawString => f.write_str("backtick shortcode argument is not closed"),
            Self::PeriodInName => {
                f.write_str("a shortcode name may contain `.` only in `name.inline`")
            }
            Self::UnopenedClose(name) => {
                write!(
                    f,
                    "closing tag for shortcode {name:?}, which was not opened"
                )
            }
            Self::JunkAfterClose => f.write_str("unexpected text in closing shortcode tag"),
        }
    }
}

/// A lexer error and the byte range it was found in.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{kind} (bytes {}..{})", span.start, span.end)]
pub struct LexError {
    pub kind: LexErrorKind,
    pub span: Range<usize>,
}

/// The tokens of a source, and the error lexing stopped at. The tokens before the error are
/// kept; after an [`LexErrorKind::InlineNesting`] error the rest of the source follows as a
/// text token (the Go lexer does the same), so the error belongs before the first token that
/// does not end before it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lexed {
    pub tokens: Vec<Token>,
    pub error: Option<LexError>,
}

impl Lexed {
    /// The tokens, or the error.
    ///
    /// # Errors
    /// The error lexing stopped at.
    pub fn into_result(self) -> Result<Vec<Token>, LexError> {
        match self.error {
            Some(e) => Err(e),
            None => Ok(self.tokens),
        }
    }
}

/// Lexes a page body (no front matter) with the `<!--more-->` summary divider.
///
/// # Errors
/// An unterminated or malformed shortcode tag.
pub fn lex(body: &str) -> Result<Vec<Token>, LexError> {
    lex_with(
        body.as_bytes(),
        LexOptions {
            start: Start::Body,
            summary_divider: SummaryDivider::Html,
        },
    )
    .into_result()
}

/// Lexes `src` (bytes that need not be UTF-8) with the given options.
#[must_use]
pub fn lex_with(src: &[u8], opts: LexOptions) -> Lexed {
    let mut lexer = Lexer::new(src, opts.summary_divider);
    let res = match opts.start {
        Start::Page => lexer.intro().and_then(|()| lexer.main()),
        Start::Body => lexer.main(),
    };
    Lexed {
        tokens: lexer.tokens,
        error: res.err(),
    }
}

/// The outcome of lexing the start of a page (see [`crate::split_front_matter`]).
pub(crate) struct Intro {
    pub front_matter: Option<(FrontMatterFormat, Range<usize>)>,
    /// Where the body starts: after the front matter, or after any byte order marks.
    pub body_offset: usize,
}

pub(crate) fn lex_intro(src: &[u8]) -> Result<Intro, LexError> {
    let mut lexer = Lexer::new(src, SummaryDivider::Html);
    lexer.intro()?;
    Ok(Intro {
        front_matter: lexer.tokens.iter().find_map(|t| match t.kind {
            TokenKind::FrontMatter(f) => Some((f, t.span.clone())),
            _ => None,
        }),
        body_offset: lexer.start,
    })
}

/// How far the arguments of the current tag have committed.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Args {
    Unknown,
    Positional,
    Named,
}

/// Which token an argument becomes.
#[derive(Clone, Copy)]
enum ArgRole {
    Param,
    Value,
}

impl ArgRole {
    fn kind(self, q: Quoting) -> TokenKind {
        match self {
            Self::Param => TokenKind::Param(q),
            Self::Value => TokenKind::Value(q),
        }
    }
}

type Step = Result<(), LexError>;

struct Lexer<'s> {
    src: &'s [u8],
    /// The read position.
    pos: usize,
    /// The start of the pending token.
    start: usize,
    /// The byte width of the last character read (0 at the end), for `backup`.
    width: usize,
    tokens: Vec<Token>,
    /// The divider still to look for.
    divider: Option<&'static [u8]>,
    // Shortcode state, carried across tags as Go does.
    delim: Delim,
    /// Inside an inline shortcode (from its name until its closing tag).
    inline: bool,
    /// The last shortcode name lexed.
    name: Option<&'s [u8]>,
    /// Every name opened so far (a closing tag must name one of them).
    opened: HashSet<&'s [u8]>,
    /// After a `/` in the current tag.
    closing: bool,
    /// After the name in the current tag (arguments may follow).
    after_name: bool,
    args: Args,
}

impl<'s> Lexer<'s> {
    fn new(src: &'s [u8], divider: SummaryDivider) -> Self {
        Self {
            src,
            pos: 0,
            start: 0,
            width: 0,
            tokens: Vec::new(),
            divider: match divider {
                SummaryDivider::Html => Some(SUMMARY_DIVIDER),
                SummaryDivider::Org => Some(SUMMARY_DIVIDER_ORG),
                SummaryDivider::Off => None,
            },
            delim: Delim::Html,
            inline: false,
            name: None,
            opened: HashSet::new(),
            closing: false,
            after_name: false,
            args: Args::Unknown,
        }
    }

    // ── cursor ──

    fn rest(&self) -> &'s [u8] {
        &self.src[self.pos..]
    }

    fn next(&mut self) -> Option<char> {
        match decode(self.rest()) {
            Some((c, w)) => {
                self.width = w;
                self.pos += w;
                Some(c)
            }
            None => {
                self.width = 0;
                None
            }
        }
    }

    fn backup(&mut self) {
        self.pos -= self.width;
    }

    fn peek(&mut self) -> Option<char> {
        let c = self.next();
        self.backup();
        c
    }

    fn at(&self, prefix: &[u8]) -> bool {
        self.rest().starts_with(prefix)
    }

    fn ignore(&mut self) {
        self.start = self.pos;
    }

    fn emit(&mut self, kind: TokenKind) {
        self.tokens.push(Token {
            kind,
            span: self.start..self.pos,
        });
        self.start = self.pos;
    }

    /// Emits the pending text, splitting off trailing indentation: the horizontal whitespace
    /// after the last newline, or the whole run when it is whitespace without a newline.
    fn emit_text(&mut self) {
        let text = &self.src[self.start..self.pos];
        let trailing_ws = text
            .iter()
            .rev()
            .take_while(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
            .count();
        let ws = &text[text.len() - trailing_ws..];
        let split = match ws.iter().rposition(|&b| b == b'\n') {
            // Whitespace after the last newline.
            Some(nl) if nl + 1 < ws.len() => Some(text.len() - ws.len() + nl + 1),
            Some(_) => None,
            // A text run of horizontal whitespace only.
            None if !text.is_empty() && trailing_ws == text.len() => Some(0),
            None => None,
        };
        match split {
            Some(0) => self.emit(TokenKind::Indentation),
            Some(at) => {
                let end = self.pos;
                self.pos = self.start + at;
                self.emit(TokenKind::Text);
                self.pos = end;
                self.emit(TokenKind::Indentation);
            }
            None => self.emit(TokenKind::Text),
        }
    }

    fn error(&self, kind: LexErrorKind) -> LexError {
        LexError {
            kind,
            span: self.start..self.pos,
        }
    }

    fn consume_crlf(&mut self) -> bool {
        let mut consumed = false;
        for want in ['\r', '\n'] {
            if self.next() == Some(want) {
                consumed = true;
            } else {
                self.backup();
            }
        }
        consumed
    }

    fn consume_space(&mut self) {
        while self.next().is_some_and(char::is_whitespace) {}
        self.backup();
    }

    fn consume_to_space(&mut self) {
        while self.next().is_some_and(|c| !c.is_whitespace()) {}
        self.backup();
    }

    // ── page start ──

    fn intro(&mut self) -> Step {
        while let Some(c) = self.next() {
            match c {
                '+' => return self.delimited_front_matter(FrontMatterFormat::Toml, '+'),
                '-' => return self.delimited_front_matter(FrontMatterFormat::Yaml, '-'),
                '{' => return self.json_front_matter(),
                '#' => {
                    self.org_front_matter();
                    return Ok(());
                }
                BYTE_ORDER_MARK => self.emit(TokenKind::ByteOrderMark),
                ' ' | '\t' | '\r' | '\n' => {}
                _ => break,
            }
        }
        Ok(())
    }

    fn delimited_front_matter(&mut self, format: FrontMatterFormat, c: char) -> Step {
        for _ in 0..2 {
            if self.next() != Some(c) {
                return Err(self.error(LexErrorKind::InvalidDelimiter(format)));
            }
        }
        let delim: &[u8] = if c == '+' { b"+++" } else { b"---" };
        let mut at_line_start = self.consume_crlf();
        self.ignore();
        loop {
            let eol = at_line_start || {
                let Some(c) = self.next() else {
                    return Err(self.error(LexErrorKind::UnterminatedFrontMatter(format)));
                };
                matches!(c, '\r' | '\n')
            };
            if eol && self.at(delim) {
                self.emit(TokenKind::FrontMatter(format));
                self.pos += delim.len();
                self.consume_crlf();
                self.ignore();
                return Ok(());
            }
            at_line_start = false;
        }
    }

    /// A JSON object: braces are balanced outside strings; the newline after it is included.
    fn json_front_matter(&mut self) -> Step {
        self.backup();
        let mut in_string = false;
        let mut depth = 0_usize;
        loop {
            match self.next() {
                None => return Err(self.error(LexErrorKind::UnterminatedJson)),
                Some('{') if !in_string => depth += 1,
                Some('}') if !in_string => depth = depth.saturating_sub(1),
                Some('"') => in_string = !in_string,
                Some('\\') => {
                    self.next();
                }
                Some(_) => {}
            }
            if depth == 0 {
                break;
            }
        }
        self.consume_crlf();
        self.emit(TokenKind::FrontMatter(FrontMatterFormat::Json));
        Ok(())
    }

    /// `#+KEY: value` lines; a lone `#` is content.
    fn org_front_matter(&mut self) {
        self.backup();
        if !self.at(b"#+") {
            return;
        }
        if self.divider.is_some() {
            self.divider = Some(SUMMARY_DIVIDER_ORG);
        }
        while let Some(c) = self.next() {
            if c == '\n' && !self.at(b"#+") {
                break;
            }
        }
        self.emit(TokenKind::FrontMatter(FrontMatterFormat::Org));
    }

    // ── body ──

    fn main(&mut self) -> Step {
        while self.pos < self.src.len() {
            let rest = self.rest();
            let tag = memchr::memmem::find(rest, b"{{");
            let divider = self.divider.and_then(|d| memchr::memmem::find(rest, d));
            let Some(skip) = [tag, divider].into_iter().flatten().min() else {
                self.pos = self.src.len();
                break;
            };
            self.pos += skip;
            if self.pos > self.start {
                self.emit_text();
            }
            if self.at(Delim::Html.left()) || self.at(Delim::Markdown.left()) {
                if let Err(e) = self.shortcode() {
                    if e.kind == LexErrorKind::InlineNesting {
                        // The Go lexer reads the rest of the source as text after this error.
                        self.pos = self.src.len();
                        if self.pos > self.start {
                            self.emit_text();
                        }
                    }
                    return Err(e);
                }
            } else if let Some(d) = self.divider.filter(|d| self.at(d)) {
                self.divider = None;
                self.pos += d.len();
                self.consume_space();
                self.emit(TokenKind::SummaryDivider);
            } else {
                // `{{` that is not a tag.
                self.pos += 1;
            }
        }
        if self.pos > self.start {
            self.emit_text();
        }
        Ok(())
    }

    /// A tag, from its left delimiter to its right delimiter.
    fn shortcode(&mut self) -> Step {
        if self.inline && !self.closes_inline() {
            return Err(self.error(LexErrorKind::InlineNesting));
        }
        self.delim = if self.at(Delim::Markdown.left()) {
            Delim::Markdown
        } else {
            Delim::Html
        };
        self.pos += 3;
        if self.at(b"/*") {
            return self.escaped_shortcode();
        }
        self.emit(TokenKind::LeftDelim(self.delim));
        self.after_name = false;
        self.args = Args::Unknown;
        self.inside_tag()
    }

    /// Whether the tag at `pos` is `{{< / name …`, the closing tag of the current inline
    /// shortcode.
    fn closes_inline(&self) -> bool {
        let after = &self.rest()[3..];
        let lead = after.len() - trim_start_space(after).len();
        let Some(tail) = after[lead..].strip_prefix(b"/") else {
            return false;
        };
        let tail = trim_end_space(trim_start_space(tail));
        let name = self.name.unwrap_or_default();
        tail.starts_with(name) && tail.get(name.len()) == Some(&b' ')
    }

    /// `{{</* x */>}}`: three text tokens (`{{<`, ` x `, `>}}`), the comment markers dropped.
    fn escaped_shortcode(&mut self) -> Step {
        let mut close = b"*/".to_vec();
        close.extend_from_slice(self.delim.right());
        let Some(end) = memchr::memmem::find(self.rest(), &close).filter(|&i| i > 1) else {
            return Err(self.error(LexErrorKind::UnclosedEscape));
        };
        self.emit_text();
        self.pos += 2;
        self.ignore();
        self.pos += end - 2;
        self.emit_text();
        self.pos += 2;
        self.ignore();
        self.pos += 3;
        self.emit_text();
        Ok(())
    }

    fn inside_tag(&mut self) -> Step {
        loop {
            if self.at(self.delim.right()) {
                return self.right_delim();
            }
            match self.next() {
                None => return Err(self.error(LexErrorKind::UnclosedTag)),
                Some(' ' | '\t' | '\r' | '\n') => self.ignore(),
                Some('=') => {
                    self.consume_space();
                    self.ignore();
                    match self.peek() {
                        Some('"') => self.quoted(true, ArgRole::Value)?,
                        Some('\\') => self.quoted(false, ArgRole::Value)?,
                        Some('`') => self.backtick(ArgRole::Value)?,
                        _ => {
                            self.consume_to_space();
                            self.emit(TokenKind::Value(Quoting::Bare));
                        }
                    }
                }
                Some('/') => {
                    if self.name.is_none() {
                        return Err(self.error(LexErrorKind::CloseWithoutOpen));
                    }
                    self.closing = true;
                    self.inline = false;
                    self.after_name = false;
                    self.emit(TokenKind::Close);
                }
                Some('\\') => {
                    self.ignore();
                    if matches!(self.peek(), Some('"' | '`')) {
                        self.param(true)?;
                    }
                }
                Some(c) if self.after_name && (is_word_or_hyphen(c) || c == '"' || c == '`') => {
                    self.backup();
                    self.param(false)?;
                }
                Some(c) if is_word(c) => {
                    self.backup();
                    if self.name_token()? {
                        return self.end_of_closing_tag();
                    }
                }
                Some(c) => return Err(self.error(LexErrorKind::UnexpectedChar(c))),
            }
        }
    }

    fn right_delim(&mut self) -> Step {
        self.closing = false;
        self.pos += 3;
        self.emit(TokenKind::RightDelim(self.delim));
        Ok(())
    }

    /// A positional argument or the name of a named one; `escaped` after a backslash.
    fn param(&mut self, escaped: bool) -> Step {
        let first = self.next();
        if first == Some('"') || (first == Some('`') && !escaped) {
            if self.args == Args::Named {
                return Err(self.error(LexErrorKind::MixedArguments));
            }
            self.args = Args::Positional;
            self.backup();
            return if first == Some('"') {
                self.quoted(!escaped, ArgRole::Param)
            } else {
                self.backtick(ArgRole::Param)
            };
        }
        if first == Some('`') {
            return Err(self.error(LexErrorKind::InvalidEscape));
        }
        let mut named = false;
        let mut c = first;
        loop {
            if !c.is_some_and(|c| is_word_or_hyphen(c) || c == '.') {
                self.backup();
                break;
            }
            c = self.next();
            if c == Some('=') {
                self.backup();
                named = true;
                break;
            }
        }
        self.args = match (self.args, named) {
            (Args::Unknown | Args::Positional, false) => Args::Positional,
            (Args::Unknown | Args::Named, true) => Args::Named,
            _ => return Err(self.error(LexErrorKind::MixedArguments)),
        };
        self.emit(TokenKind::Param(Quoting::Bare));
        Ok(())
    }

    /// A `"…"` argument. `escapes`: whether `\"` inside it is an escaped quote (otherwise it
    /// ends the string, as in `k=\"v\"`).
    fn quoted(&mut self, escapes: bool, role: ArgRole) -> Step {
        let mut open = false;
        let mut has_escapes = false;
        let mut after_escape = false;
        loop {
            match self.next() {
                Some('\\') => match self.peek() {
                    Some('"') if open && !escapes => {
                        self.backup();
                        break;
                    }
                    Some('"') if open => {
                        has_escapes = true;
                        after_escape = true;
                    }
                    Some('`') => return Err(self.error(LexErrorKind::InvalidEscape)),
                    _ => {}
                },
                None | Some('\n') => return Err(self.error(LexErrorKind::UnterminatedString)),
                Some('"') if after_escape => after_escape = false,
                Some('"') if open => {
                    self.backup();
                    break;
                }
                Some('"') => {
                    open = true;
                    self.ignore();
                }
                Some(_) => {}
            }
        }
        if !has_escapes {
            self.emit(role.kind(Quoting::Quoted));
        } else if self.src[self.start..self.pos].iter().any(|&b| b != b'\\') {
            self.emit(role.kind(Quoting::Escaped));
        } else {
            self.ignore();
        }
        match self.next() {
            Some('\\') => {
                if self.peek() == Some('"') {
                    self.ignore();
                    self.next();
                    self.ignore();
                }
            }
            Some('"') => self.ignore(),
            _ => self.backup(),
        }
        Ok(())
    }

    /// A `` `…` `` argument.
    fn backtick(&mut self, role: ArgRole) -> Step {
        let mut open = false;
        loop {
            match self.next() {
                Some('`') if open => {
                    self.backup();
                    break;
                }
                Some('`') => {
                    open = true;
                    self.ignore();
                }
                None => return Err(self.error(LexErrorKind::UnterminatedRawString)),
                Some(_) => {}
            }
        }
        self.emit(role.kind(Quoting::Backtick));
        self.next();
        self.ignore();
        Ok(())
    }

    /// A shortcode name. Returns whether it is the name of a closing tag.
    fn name_token(&mut self) -> Result<bool, LexError> {
        loop {
            match self.next() {
                Some(c) if is_word_or_hyphen(c) || c == '/' => {}
                Some('.') => {
                    self.inline = self.at(b"inline ");
                    if !self.inline {
                        return Err(self.error(LexErrorKind::PeriodInName));
                    }
                }
                _ => {
                    self.backup();
                    break;
                }
            }
        }
        let word = &self.src[self.start..self.pos];
        let closing = self.closing;
        if closing && !self.opened.contains(word) {
            let name = String::from_utf8_lossy(word).into_owned();
            return Err(self.error(LexErrorKind::UnopenedClose(name)));
        }
        self.closing = false;
        self.name = Some(word);
        self.opened.insert(word);
        self.after_name = true;
        self.emit(if self.inline {
            TokenKind::InlineName
        } else {
            TokenKind::Name
        });
        Ok(closing)
    }

    fn end_of_closing_tag(&mut self) -> Step {
        loop {
            self.inline = false;
            if self.at(self.delim.right()) {
                return self.right_delim();
            }
            match self.next() {
                Some(' ' | '\t') => self.ignore(),
                _ => return Err(self.error(LexErrorKind::JunkAfterClose)),
            }
        }
    }
}

/// The character at the start of `b` and its width; bytes that are not UTF-8 decode as
/// U+FFFD, one byte at a time. `None` at the end.
fn decode(b: &[u8]) -> Option<(char, usize)> {
    let first = *b.first()?;
    if first.is_ascii() {
        return Some((char::from(first), 1));
    }
    let width = match first {
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => return Some((char::REPLACEMENT_CHARACTER, 1)),
    };
    b.get(..width)
        .and_then(|s| std::str::from_utf8(s).ok())
        .and_then(|s| s.chars().next())
        .map_or(Some((char::REPLACEMENT_CHARACTER, 1)), |c| Some((c, width)))
}

/// The last character of `b` and its width, as [`decode`] reads forwards.
fn decode_last(b: &[u8]) -> Option<(char, usize)> {
    if b.is_empty() {
        return None;
    }
    (1..=4.min(b.len()))
        .find_map(|w| {
            let s = std::str::from_utf8(&b[b.len() - w..]).ok()?;
            let mut chars = s.chars();
            let c = chars.next()?;
            chars.next().is_none().then_some((c, w))
        })
        .or(Some((char::REPLACEMENT_CHARACTER, 1)))
}

fn trim_start_space(mut b: &[u8]) -> &[u8] {
    while let Some((c, w)) = decode(b)
        && c.is_whitespace()
    {
        b = &b[w..];
    }
    b
}

fn trim_end_space(mut b: &[u8]) -> &[u8] {
    while let Some((c, w)) = decode_last(b)
        && c.is_whitespace()
    {
        b = &b[..b.len() - w];
    }
    b
}

/// `_`, a letter (`L*`) or a decimal digit (`Nd`).
fn is_word(c: char) -> bool {
    c == '_' || is_letter(c) || is_digit(c)
}

fn is_word_or_hyphen(c: char) -> bool {
    is_word(c) || c == '-'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<(TokenKind, &str)> {
        lex(src)
            .unwrap()
            .into_iter()
            .map(|t| (t.kind, &src[t.span]))
            .collect()
    }

    #[test]
    fn text_splits_off_indentation() {
        use TokenKind::*;
        assert_eq!(
            kinds("a\n  {{< x >}}"),
            vec![
                (Text, "a\n"),
                (Indentation, "  "),
                (LeftDelim(Delim::Html), "{{<"),
                (Name, "x"),
                (RightDelim(Delim::Html), ">}}"),
            ]
        );
        assert_eq!(kinds("  "), vec![(Indentation, "  ")]);
        assert_eq!(kinds("a  "), vec![(Text, "a  ")]);
        assert_eq!(kinds("a\n"), vec![(Text, "a\n")]);
    }

    #[test]
    fn arguments() {
        use TokenKind::*;
        let src = r#"{{% x a="b \"c\"" n=1 r=`raw` %}}"#;
        let toks = lex(src).unwrap();
        let got: Vec<_> = toks.iter().map(|t| (t.kind, t.value(src))).collect();
        assert_eq!(
            got,
            vec![
                (LeftDelim(Delim::Markdown), "{{%".into()),
                (Name, "x".into()),
                (Param(Quoting::Bare), "a".into()),
                (Value(Quoting::Escaped), "b \"c\"".into()),
                (Param(Quoting::Bare), "n".into()),
                (Value(Quoting::Bare), "1".into()),
                (Param(Quoting::Bare), "r".into()),
                (Value(Quoting::Backtick), "raw".into()),
                (RightDelim(Delim::Markdown), "%}}".into()),
            ]
        );
    }

    #[test]
    fn errors() {
        let e = lex("{{< x a b=c >}}").unwrap_err();
        assert_eq!(e.kind, LexErrorKind::MixedArguments);
        let e = lex("{{< /x >}}").unwrap_err();
        assert_eq!(e.kind, LexErrorKind::CloseWithoutOpen);
        let e = lex("{{< x").unwrap_err();
        assert_eq!(e.kind, LexErrorKind::UnclosedTag);
    }

    #[test]
    fn decoding_matches_go() {
        assert_eq!(decode(b"\xff"), Some(('\u{fffd}', 1)));
        assert_eq!(decode(b"\xc3"), Some(('\u{fffd}', 1)));
        assert_eq!(decode("é".as_bytes()), Some(('é', 2)));
        assert_eq!(decode_last(b"a\xc3"), Some(('\u{fffd}', 1)));
        assert_eq!(decode_last("aé".as_bytes()), Some(('é', 2)));
    }
}
