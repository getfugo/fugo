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

mod content;
mod shortcodes;

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

    // ── body ──
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
mod tests;
