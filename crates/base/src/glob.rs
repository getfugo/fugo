//! Glob patterns, as the Go implementation matches them.
//!
//! The syntax (the Go implementation's, from gobwas/glob):
//!
//! | pattern | matches |
//! |---|---|
//! | `*` | any run of characters except `/` |
//! | `**` | any run of characters, `/` included (anywhere, not only as a whole segment) |
//! | `?` | one character except `/` |
//! | `[abc]`, `[!abc]` | one character in (not in) the list; `\` escapes inside the list |
//! | `[a-z]`, `[!a-z]` | one character in (not in) the range; a class is one range *or* one list |
//! | `{a,b}` | one of the alternatives (nesting allowed; an unclosed `{` ends at the end) |
//! | `\x` | `x` literally |
//!
//! `,` and `}` outside alternatives and `]` outside a class are literals. With
//! [`Case::Fold`] (Go's `GetGlob`) the pattern and the matched strings are lower-cased.
//!
//! A pattern compiles to an anchored regular expression over characters. (`globset` is not
//! used: it matches bytes, so `?` and classes are wrong for non-ASCII text, and it cannot
//! express `**` inside a segment or nested alternatives.)

use crate::text;

/// Whether pattern and input are lower-cased before matching.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Case {
    /// Go's globs: both are lower-cased.
    #[default]
    Fold,
    Sensitive,
}

/// Whether `/` separates path segments.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Separator {
    /// `*` and `?` do not match `/` (Go's globs).
    #[default]
    Slash,
    /// No separator: `*` matches any string.
    None,
}

/// Glob compilation options.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GlobOpts {
    pub case: Case,
    pub separator: Separator,
}

/// A pattern that is not a glob.
#[derive(Debug, thiserror::Error)]
pub enum GlobError {
    #[error("glob {0:?}: unexpected end of pattern in a character class")]
    UnexpectedEnd(String),
    #[error("glob {0:?}: a character class holds one range or one list, then ']'")]
    ExpectedClose(String),
    #[error("glob {0:?}: empty character class")]
    EmptyClass(String),
    #[error("glob {pattern:?}: range {lo:?}-{hi:?} is reversed")]
    ReversedRange { pattern: String, lo: char, hi: char },
    #[error(transparent)]
    Regex(#[from] regex::Error),
}

/// A compiled glob.
#[derive(Clone, Debug)]
pub struct Glob {
    regex: regex::Regex,
    case: Case,
}

impl Glob {
    /// Whether `s` matches the whole pattern.
    #[must_use]
    pub fn is_match(&self, s: &str) -> bool {
        match self.case {
            Case::Fold => self.regex.is_match(&text::to_lower(s)),
            Case::Sensitive => self.regex.is_match(s),
        }
    }
}

/// Compiles a glob pattern.
///
/// # Errors
/// An unterminated, reversed or empty character class.
pub fn compile(pattern: &str, o: GlobOpts) -> Result<Glob, GlobError> {
    let source = match o.case {
        Case::Fold => text::to_lower(pattern),
        Case::Sensitive => pattern.to_owned(),
    };
    let mut parser = Parser {
        chars: source.chars().collect(),
        pos: 0,
        pattern,
    };
    let seq = parser.sequence(false)?;
    let mut re = String::from("^(?s:");
    emit(&seq, o.separator, &mut re);
    re.push_str(")$");
    Ok(Glob {
        regex: regex::Regex::new(&re)?,
        case: o.case,
    })
}

#[derive(Debug)]
enum Node {
    Literal(char),
    /// `*`
    Any,
    /// `**`
    Super,
    /// `?`
    Single,
    Class {
        negated: bool,
        items: ClassItems,
    },
    Alternatives(Vec<Vec<Node>>),
}

#[derive(Debug)]
enum ClassItems {
    Range(char, char),
    List(Vec<char>),
}

struct Parser<'a> {
    chars: Vec<char>,
    pos: usize,
    pattern: &'a str,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        Some(c)
    }

    fn sequence(&mut self, in_alternatives: bool) -> Result<Vec<Node>, GlobError> {
        let mut seq = Vec::new();
        while let Some(c) = self.peek() {
            if in_alternatives && matches!(c, ',' | '}') {
                break;
            }
            self.pos += 1;
            match c {
                '{' => {
                    let mut alternatives = Vec::new();
                    loop {
                        alternatives.push(self.sequence(true)?);
                        match self.next() {
                            Some(',') => {}
                            _ => break,
                        }
                    }
                    seq.push(Node::Alternatives(alternatives));
                }
                '[' => seq.push(self.class()?),
                '?' => seq.push(Node::Single),
                '*' if self.peek() == Some('*') => {
                    self.pos += 1;
                    seq.push(Node::Super);
                }
                '*' => seq.push(Node::Any),
                '\\' => {
                    if let Some(escaped) = self.next() {
                        seq.push(Node::Literal(escaped));
                    }
                }
                c => seq.push(Node::Literal(c)),
            }
        }
        Ok(seq)
    }

    fn class(&mut self) -> Result<Node, GlobError> {
        let end = || GlobError::UnexpectedEnd(self.pattern.to_owned());
        let negated = self.peek() == Some('!');
        if negated {
            self.pos += 1;
        }
        let first = self.peek().ok_or_else(end)?;
        let items = if self.chars.get(self.pos + 1) == Some(&'-') {
            self.pos += 2;
            let hi = self.next().ok_or_else(end)?;
            match self.next() {
                Some(']') => {}
                Some(_) => return Err(GlobError::ExpectedClose(self.pattern.to_owned())),
                None => return Err(end()),
            }
            if first == '\0' || hi == '\0' {
                return Err(GlobError::EmptyClass(self.pattern.to_owned()));
            }
            if hi < first {
                return Err(GlobError::ReversedRange {
                    pattern: self.pattern.to_owned(),
                    lo: first,
                    hi,
                });
            }
            ClassItems::Range(first, hi)
        } else {
            let mut list = Vec::new();
            loop {
                match self.next().ok_or_else(end)? {
                    ']' => break,
                    '\\' => {
                        if let Some(c) = self.next() {
                            list.push(c);
                        }
                    }
                    c => list.push(c),
                }
            }
            if list.is_empty() {
                return Err(GlobError::EmptyClass(self.pattern.to_owned()));
            }
            ClassItems::List(list)
        };
        Ok(Node::Class { negated, items })
    }
}

/// Appends the regex of `seq` to `re`.
fn emit(seq: &[Node], separator: Separator, re: &mut String) {
    for node in seq {
        match node {
            Node::Literal(c) => re.push_str(&regex::escape(c.encode_utf8(&mut [0; 4]))),
            Node::Any => re.push_str(match separator {
                Separator::Slash => "[^/]*",
                Separator::None => ".*",
            }),
            Node::Single => re.push_str(match separator {
                Separator::Slash => "[^/]",
                Separator::None => ".",
            }),
            Node::Super => re.push_str(".*"),
            Node::Class { negated, items } => {
                re.push('[');
                if *negated {
                    re.push('^');
                }
                match items {
                    ClassItems::Range(lo, hi) => {
                        push_class_char(re, *lo);
                        re.push('-');
                        push_class_char(re, *hi);
                    }
                    ClassItems::List(list) => list.iter().for_each(|&c| push_class_char(re, c)),
                }
                re.push(']');
            }
            Node::Alternatives(alternatives) => {
                re.push_str("(?:");
                for (i, alt) in alternatives.iter().enumerate() {
                    if i > 0 {
                        re.push('|');
                    }
                    emit(alt, separator, re);
                }
                re.push(')');
            }
        }
    }
}

/// A character inside a regex class, escaped when the class syntax gives it a meaning.
fn push_class_char(re: &mut String, c: char) {
    if matches!(c, '\\' | ']' | '[' | '^' | '-' | '&' | '~') {
        re.push('\\');
    }
    re.push(c);
}
