//! Assembling the token stream of a body into text and shortcode calls.

use std::ops::Range;

use crate::lexer::{LexError, lex};
use crate::token::{Delim, Scalar, Token, TokenKind};

/// Whether a shortcode's template uses its inner content. The Go parser depends on it: a
/// shortcode that uses `inner` collects everything up to its closing tag (and must be closed or
/// self-closed); one that does not ends at the `>}}` of its opening tag and must not be closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InnerUse {
    /// The template uses `inner` (or `inner_deindent`).
    Required,
    /// The template does not use `inner`.
    Unused,
    /// There is no template for this name.
    UnknownShortcode,
}

/// Answers [`InnerUse`] per shortcode name (in the build: the template store).
pub trait InnerOracle {
    fn inner_use(&self, shortcode: &str) -> InnerUse;
}

impl<F: Fn(&str) -> InnerUse> InnerOracle for F {
    fn inner_use(&self, shortcode: &str) -> InnerUse {
        self(shortcode)
    }
}

/// A body: text and shortcode calls in source order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Body<'a> {
    pub segments: Vec<Segment<'a>>,
    /// The index in `segments` of the first segment after the summary divider (the segments
    /// before it are the summary), if the body has one.
    pub summary_divider: Option<usize>,
}

/// A piece of a body or of a shortcode's inner content.
#[derive(Clone, Debug, PartialEq)]
pub enum Segment<'a> {
    /// Source text. An escaped shortcode `{{</* x */>}}` is the text `{{< x >}}` (three
    /// segments; the comment markers are dropped).
    Text(&'a str),
    Shortcode(ShortcodeCall<'a>),
}

/// How a shortcode call ends.
#[derive(Clone, Debug, PartialEq)]
pub enum Closing<'a> {
    /// `{{< name />}}`: no inner content.
    SelfClosed,
    /// `{{< name >}}…{{< /name >}}`.
    Closed {
        inner: Vec<Segment<'a>>,
        /// The source range of the inner content (between the tags).
        span: Range<usize>,
    },
    /// `{{< name >}}` of a shortcode that does not use `inner`: the call is the opening tag.
    Open,
}

/// The arguments of a call. A call has positional or named arguments, never both.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum ShortcodeArgs {
    #[default]
    None,
    Positional(Vec<Scalar>),
    /// In source order; a repeated name keeps its first position and its last value.
    Named(Vec<(String, Scalar)>),
}

/// One shortcode call.
#[derive(Clone, Debug, PartialEq)]
pub struct ShortcodeCall<'a> {
    /// The name as written (`name.inline` for an inline shortcode).
    pub name: &'a str,
    /// The delimiter of the opening tag.
    pub delim: Delim,
    /// An inline shortcode (`{{< name.inline >}}`): its inner content is its template.
    pub inline: bool,
    pub args: ShortcodeArgs,
    pub closing: Closing<'a>,
    /// The horizontal whitespace before the call on its line (only when the call is the first
    /// thing on the line).
    pub indentation: &'a str,
    /// The index of the call among its siblings: top-level calls of the body, or the calls in
    /// the parent's inner content.
    pub ordinal: usize,
    /// From the opening tag's `{{<` to the last `>}}` of the call.
    pub span: Range<usize>,
}

/// A body that could not be parsed. Positions are byte ranges in the body.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    #[error(transparent)]
    Lex(#[from] LexError),
    #[error("shortcode tag without a name")]
    NoName { span: Range<usize> },
    #[error("no template for shortcode {name:?}")]
    UnknownShortcode { name: String, span: Range<usize> },
    #[error("shortcode {name:?} does not use its inner content, but has a closing tag")]
    ClosingNotAllowed { name: String, span: Range<usize> },
    #[error("shortcode {name:?} uses its inner content and must be closed or self-closed")]
    Unclosed { name: String, span: Range<usize> },
    #[error("closing tag {found:?} does not match the open shortcode {name:?}")]
    MismatchedClose {
        name: String,
        found: String,
        span: Range<usize>,
    },
    #[error("closing tag {name:?} without an open shortcode")]
    UnexpectedClose { name: String, span: Range<usize> },
    #[error("unexpected token in shortcode tag")]
    Malformed { span: Range<usize> },
}

impl ParseError {
    /// The byte range the error refers to.
    #[must_use]
    pub fn span(&self) -> Range<usize> {
        match self {
            Self::Lex(e) => e.span.clone(),
            Self::NoName { span }
            | Self::UnknownShortcode { span, .. }
            | Self::ClosingNotAllowed { span, .. }
            | Self::Unclosed { span, .. }
            | Self::MismatchedClose { span, .. }
            | Self::UnexpectedClose { span, .. }
            | Self::Malformed { span } => span.clone(),
        }
    }
}

/// Lexes and assembles a body.
///
/// # Errors
/// As [`lex`] and [`assemble`].
pub fn parse_body<'a>(body: &'a str, oracle: &dyn InnerOracle) -> Result<Body<'a>, ParseError> {
    let tokens = lex(body)?;
    assemble(body, &tokens, oracle)
}

/// Assembles the tokens of `src` (from [`lex`]) into a [`Body`]. Byte order marks and front
/// matter tokens are skipped; a summary divider inside a shortcode's inner content is dropped.
///
/// # Errors
/// A tag without a name, an unknown shortcode, a closing tag for a shortcode that does not use
/// `inner`, a shortcode that uses `inner` but is not closed, or mismatched closing tags.
pub fn assemble<'a>(
    src: &'a str,
    tokens: &[Token],
    oracle: &dyn InnerOracle,
) -> Result<Body<'a>, ParseError> {
    let mut p = Assembler {
        src,
        tokens,
        next: 0,
        oracle,
    };
    let mut body = Body::default();
    let mut ordinal = 0;
    while let Some(t) = p.peek() {
        match t.kind {
            TokenKind::Text | TokenKind::Indentation => {
                p.next += 1;
                push_text(&mut body.segments, src, t.span.clone());
            }
            TokenKind::SummaryDivider => {
                p.next += 1;
                if body.summary_divider.is_none() {
                    body.summary_divider = Some(body.segments.len());
                }
            }
            TokenKind::ByteOrderMark | TokenKind::FrontMatter(_) => p.next += 1,
            TokenKind::LeftDelim(_) => {
                let call = p.call(ordinal)?;
                ordinal += 1;
                body.segments.push(Segment::Shortcode(call));
            }
            _ => {
                return Err(ParseError::Malformed {
                    span: t.span.clone(),
                });
            }
        }
    }
    Ok(body)
}

/// Appends text, extending the previous text segment when the two are adjacent in `src`.
fn push_text<'a>(segments: &mut Vec<Segment<'a>>, src: &'a str, span: Range<usize>) {
    if let Some(Segment::Text(prev)) = segments.last_mut() {
        let prev_end = prev.as_ptr() as usize - src.as_ptr() as usize + prev.len();
        if prev_end == span.start {
            *prev = &src[prev_end - prev.len()..span.end];
            return;
        }
    }
    segments.push(Segment::Text(&src[span]));
}

struct Assembler<'a, 't> {
    src: &'a str,
    tokens: &'t [Token],
    next: usize,
    oracle: &'t dyn InnerOracle,
}

impl<'a, 't> Assembler<'a, 't> {
    fn peek(&self) -> Option<&'t Token> {
        self.tokens.get(self.next)
    }

    fn peek_kind(&self, ahead: usize) -> Option<TokenKind> {
        self.tokens.get(self.next + ahead).map(|t| t.kind)
    }

    fn bump(&mut self) -> Option<&'t Token> {
        let t = self.tokens.get(self.next)?;
        self.next += 1;
        Some(t)
    }

    fn text(&self, t: &Token) -> &'a str {
        &self.src[t.span.clone()]
    }

    fn span_from(&self, start: usize) -> Range<usize> {
        let end = self
            .next
            .checked_sub(1)
            .map_or(start, |i| self.tokens[i].span.end);
        start..end.max(start)
    }

    /// Consumes the rest of a closing tag after its `{{<`: `/`, the name, `>}}`. Returns the
    /// name.
    fn closing_tag(&mut self, start: usize) -> Result<&'a str, ParseError> {
        let close = self.bump();
        debug_assert!(close.is_some_and(|t| t.kind == TokenKind::Close));
        let name = match self.bump() {
            Some(t) if matches!(t.kind, TokenKind::Name | TokenKind::InlineName) => self.text(t),
            _ => {
                return Err(ParseError::Malformed {
                    span: self.span_from(start),
                });
            }
        };
        match self.bump() {
            Some(t) if matches!(t.kind, TokenKind::RightDelim(_)) => Ok(name),
            _ => Err(ParseError::Malformed {
                span: self.span_from(start),
            }),
        }
    }

    /// A call, starting at its `{{<`.
    fn call(&mut self, ordinal: usize) -> Result<ShortcodeCall<'a>, ParseError> {
        let indentation = self
            .next
            .checked_sub(1)
            .map(|i| &self.tokens[i])
            .filter(|t| t.kind == TokenKind::Indentation)
            .map_or("", |t| self.text(t));
        let Some(Token {
            kind: TokenKind::LeftDelim(delim),
            span: open_span,
        }) = self.bump()
        else {
            let at = self.span_from(0).end;
            return Err(ParseError::Malformed { span: at..at });
        };
        let start = open_span.start;
        let delim = *delim;

        let (name, inline) = match self.peek_kind(0) {
            Some(TokenKind::Name) => (self.bump().map_or("", |t| self.text(t)), false),
            Some(TokenKind::InlineName) => (self.bump().map_or("", |t| self.text(t)), true),
            Some(TokenKind::RightDelim(_)) => {
                return Err(ParseError::NoName {
                    span: start..self.tokens[self.next].span.end,
                });
            }
            Some(TokenKind::Close) => {
                let name = self.closing_tag(start)?;
                return Err(ParseError::UnexpectedClose {
                    name: name.to_owned(),
                    span: self.span_from(start),
                });
            }
            _ => {
                return Err(ParseError::Malformed {
                    span: self.span_from(start),
                });
            }
        };
        let uses_inner = if inline {
            true
        } else {
            match self.oracle.inner_use(name) {
                InnerUse::Required => true,
                InnerUse::Unused => false,
                InnerUse::UnknownShortcode => {
                    return Err(ParseError::UnknownShortcode {
                        name: name.to_owned(),
                        span: self.span_from(start),
                    });
                }
            }
        };

        let args = self.args(start)?;

        let mut call = ShortcodeCall {
            name,
            delim,
            inline,
            args,
            closing: Closing::Open,
            indentation,
            ordinal,
            span: start..start,
        };

        match self.bump().map(|t| t.kind) {
            // `{{< name />}}`, or the odd `{{< name / name >}}`.
            Some(TokenKind::Close) => {
                if !uses_inner {
                    return Err(ParseError::ClosingNotAllowed {
                        name: name.to_owned(),
                        span: self.span_from(start),
                    });
                }
                match self.bump() {
                    Some(t) if matches!(t.kind, TokenKind::RightDelim(_)) => {
                        call.closing = Closing::SelfClosed;
                    }
                    Some(t) if matches!(t.kind, TokenKind::Name | TokenKind::InlineName) => {
                        let found = self.text(t);
                        if found != name {
                            return Err(mismatch(name, found, self.span_from(start)));
                        }
                        if !matches!(self.bump().map(|t| t.kind), Some(TokenKind::RightDelim(_))) {
                            return Err(ParseError::Malformed {
                                span: self.span_from(start),
                            });
                        }
                        let at = self.span_from(start).end;
                        call.closing = Closing::Closed {
                            inner: Vec::new(),
                            span: at..at,
                        };
                    }
                    _ => {
                        return Err(ParseError::Malformed {
                            span: self.span_from(start),
                        });
                    }
                }
            }
            Some(TokenKind::RightDelim(_)) if !uses_inner => {}
            Some(TokenKind::RightDelim(_)) => call.closing = self.inner(name, start)?,
            _ => {
                return Err(ParseError::Malformed {
                    span: self.span_from(start),
                });
            }
        }
        call.span = self.span_from(start);
        Ok(call)
    }

    /// The arguments of an opening tag, up to (not including) its `/` or `>}}`.
    fn args(&mut self, start: usize) -> Result<ShortcodeArgs, ParseError> {
        let mut args = ShortcodeArgs::None;
        while let Some(t) = self.peek() {
            let TokenKind::Param(_) = t.kind else { break };
            self.next += 1;
            if let Some(TokenKind::Value(_)) = self.peek_kind(0) {
                let key = t.value(self.src).into_owned();
                let value = self
                    .bump()
                    .map_or(Scalar::String(String::new()), |v| v.scalar(self.src));
                match &mut args {
                    ShortcodeArgs::None => args = ShortcodeArgs::Named(vec![(key, value)]),
                    ShortcodeArgs::Named(named) => {
                        match named.iter_mut().find(|(k, _)| *k == key) {
                            Some(slot) => slot.1 = value,
                            None => named.push((key, value)),
                        }
                    }
                    ShortcodeArgs::Positional(_) => {
                        return Err(ParseError::Malformed {
                            span: self.span_from(start),
                        });
                    }
                }
            } else {
                let value = t.scalar(self.src);
                match &mut args {
                    ShortcodeArgs::None => args = ShortcodeArgs::Positional(vec![value]),
                    ShortcodeArgs::Positional(list) => list.push(value),
                    ShortcodeArgs::Named(_) => {
                        return Err(ParseError::Malformed {
                            span: self.span_from(start),
                        });
                    }
                }
            }
        }
        Ok(args)
    }

    /// The inner content of `name` after its opening tag, through its closing tag.
    fn inner(&mut self, name: &str, start: usize) -> Result<Closing<'a>, ParseError> {
        let inner_start = self.span_from(start).end;
        let mut inner = Vec::new();
        let mut ordinal = 0;
        while let Some(t) = self.peek() {
            match t.kind {
                TokenKind::Text | TokenKind::Indentation => {
                    self.next += 1;
                    push_text(&mut inner, self.src, t.span.clone());
                }
                TokenKind::LeftDelim(_) if self.peek_kind(1) == Some(TokenKind::Close) => {
                    let inner_end = t.span.start;
                    self.next += 1;
                    let found = self.closing_tag(start)?;
                    if found != name {
                        return Err(mismatch(name, found, self.span_from(start)));
                    }
                    return Ok(Closing::Closed {
                        inner,
                        span: inner_start..inner_end,
                    });
                }
                TokenKind::LeftDelim(_) => {
                    let nested = self.call(ordinal)?;
                    ordinal += 1;
                    inner.push(Segment::Shortcode(nested));
                }
                // Go drops a divider inside inner content.
                TokenKind::SummaryDivider
                | TokenKind::ByteOrderMark
                | TokenKind::FrontMatter(_) => {
                    self.next += 1;
                }
                _ => {
                    return Err(ParseError::Malformed {
                        span: t.span.clone(),
                    });
                }
            }
        }
        Err(ParseError::Unclosed {
            name: name.to_owned(),
            span: start..inner_start,
        })
    }
}

fn mismatch(name: &str, found: &str, span: Range<usize>) -> ParseError {
    ParseError::MismatchedClose {
        name: name.to_owned(),
        found: found.to_owned(),
        span,
    }
}
