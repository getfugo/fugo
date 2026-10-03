//! The lexer's output: typed tokens with byte ranges into the lexed source.

use std::borrow::Cow;
use std::fmt;
use std::ops::Range;

/// The format of a page's front matter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FrontMatterFormat {
    /// Between `---` lines.
    Yaml,
    /// Between `+++` lines.
    Toml,
    /// A JSON object at the start of the page.
    Json,
    /// Emacs Org `#+KEY: value` lines. Lexed, but not decoded (COULD).
    Org,
}

impl FrontMatterFormat {
    /// The lower-case name (`yaml`, `toml`, `json`, `org`).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Yaml => "yaml",
            Self::Toml => "toml",
            Self::Json => "json",
            Self::Org => "org",
        }
    }
}

impl fmt::Display for FrontMatterFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The delimiter pair of a shortcode tag.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Delim {
    /// `{{< >}}`: the output is inserted after markdown rendering.
    Html,
    /// `{{% %}}`: the output is part of the markdown source.
    Markdown,
}

impl Delim {
    pub(crate) fn left(self) -> &'static [u8] {
        match self {
            Self::Html => b"{{<",
            Self::Markdown => b"{{%",
        }
    }

    pub(crate) fn right(self) -> &'static [u8] {
        match self {
            Self::Html => b">}}",
            Self::Markdown => b"%}}",
        }
    }
}

/// How a shortcode argument was written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Quoting {
    /// Unquoted (`42`, `true`, `abc-def`); typed by [`Token::scalar`].
    Bare,
    /// `"text"`; the span is the text between the quotes.
    Quoted,
    /// `"a \"b\""`: a quoted string with escaped quotes. The value is the span with every
    /// backslash removed.
    Escaped,
    /// `` `raw` ``; the span is the text between the backticks.
    Backtick,
}

/// What a token is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TokenKind {
    /// A byte order mark before the front matter (with any whitespace before it).
    ByteOrderMark,
    /// The front matter text, without its delimiter lines (JSON: the whole object).
    FrontMatter(FrontMatterFormat),
    /// The first `<!--more-->` (Org: `# more`) and the whitespace after it.
    SummaryDivider,
    /// Content text.
    Text,
    /// The whitespace after the last newline of a text run (or a text run that is only
    /// horizontal whitespace): the indentation of a shortcode that follows it.
    Indentation,
    /// `{{<` or `{{%`.
    LeftDelim(Delim),
    /// `>}}` or `%}}`.
    RightDelim(Delim),
    /// The `/` of a closing tag (`{{< /name >}}`) or a self-closing one (`{{< name />}}`).
    Close,
    /// A shortcode name (`/` allowed for namespaces).
    Name,
    /// The name of an inline shortcode (`name.inline`).
    InlineName,
    /// A positional argument, or the name of a named one.
    Param(Quoting),
    /// The value of a named argument.
    Value(Quoting),
}

/// A token: its kind and its byte range in the lexed source.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Range<usize>,
}

impl Token {
    /// The raw source text of the token.
    ///
    /// # Panics
    /// When `src` is not the source the token was lexed from.
    #[must_use]
    pub fn text<'s>(&self, src: &'s str) -> &'s str {
        &src[self.span.clone()]
    }

    /// The token's value: its text, without the backslashes of an [`Quoting::Escaped`] string.
    ///
    /// # Panics
    /// When `src` is not the source the token was lexed from.
    #[must_use]
    pub fn value<'s>(&self, src: &'s str) -> Cow<'s, str> {
        let text = self.text(src);
        match self.kind {
            TokenKind::Param(Quoting::Escaped) | TokenKind::Value(Quoting::Escaped) => {
                Cow::Owned(text.replace('\\', ""))
            }
            _ => Cow::Borrowed(text),
        }
    }

    /// The quoting of an argument token, `None` for other tokens.
    #[must_use]
    pub fn quoting(&self) -> Option<Quoting> {
        match self.kind {
            TokenKind::Param(q) | TokenKind::Value(q) => Some(q),
            _ => None,
        }
    }

    /// The typed value of an argument token (see [`Scalar::parse`]); quoted strings are
    /// always strings.
    ///
    /// # Panics
    /// When `src` is not the source the token was lexed from.
    #[must_use]
    pub fn scalar(&self, src: &str) -> Scalar {
        let v = self.value(src);
        match self.quoting() {
            Some(Quoting::Bare) | None => Scalar::parse(&v),
            Some(_) => Scalar::String(v.into_owned()),
        }
    }
}

/// A typed shortcode argument.
#[derive(Clone, Debug, PartialEq)]
pub enum Scalar {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

impl Scalar {
    /// Types an unquoted argument: `true`/`false`, `[+-]digits` (an `i64`; out of range stays a
    /// string), `[+-]digits.digits` or `[+-].digits` (an `f64`), anything else a string
    /// (`1e3`, `0.125.0`, `1.`).
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s {
            "true" => return Self::Bool(true),
            "false" => return Self::Bool(false),
            _ => {}
        }
        let unsigned = s.strip_prefix(['+', '-']).unwrap_or(s);
        let digits = |t: &str| t.bytes().all(|b| b.is_ascii_digit());
        if !unsigned.is_empty() && digits(unsigned) {
            return s
                .parse()
                .map_or_else(|_| Self::String(s.to_owned()), Self::Int);
        }
        if let Some((int, frac)) = unsigned.split_once('.')
            && digits(int)
            && !frac.is_empty()
            && digits(frac)
        {
            return s
                .parse()
                .map_or_else(|_| Self::String(s.to_owned()), Self::Float);
        }
        Self::String(s.to_owned())
    }
}

impl fmt::Display for Scalar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String(s) => f.write_str(s),
            Self::Int(i) => write!(f, "{i}"),
            Self::Float(x) => write!(f, "{x}"),
            Self::Bool(b) => write!(f, "{b}"),
        }
    }
}
