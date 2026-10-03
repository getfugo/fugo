//! A .NET-dialect backtracking regular-expression engine, as Chroma uses it.
//!
//! Chroma compiles every lexer rule with github.com/dlclark/regexp2 (a port of .NET's regex
//! engine): look-behind of any length, back-references, `\G`, Unicode `\w`/`\s`/`\d`, class
//! subtraction, inline options, and .NET's own loop semantics. Which match a rule finds decides
//! the tokens, so this is a port of regexp2 v1.11.5 (MIT; `THIRD_PARTY/regexp2/`): its parser
//! and character classes ([`parser`], [`charclass`]), and a matcher ([`vm`]) that runs the
//! parse tree with regexp2's backtracking order (alternation and quantifier priority, `*`/`+`
//! stopping on an empty iteration, counted loops, atomic look-around, captures restored on
//! backtracking, right-to-left matching inside look-behind).
//!
//! Texts are `&[char]` and positions are character indices, as regexp2 works on runes.

mod charclass;
mod parser;
mod vm;

pub(crate) use vm::State as Scratch;

use std::fmt;

/// Regex options (regexp2's `RegexOptions`; the inline letters `imnsx`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Options(u32);

impl Options {
    pub const IGNORE_CASE: Self = Self(0x0001);
    pub const MULTILINE: Self = Self(0x0002);
    pub const EXPLICIT_CAPTURE: Self = Self(0x0004);
    pub const SINGLELINE: Self = Self(0x0010);
    pub const IGNORE_PATTERN_WHITESPACE: Self = Self(0x0020);
    pub const RIGHT_TO_LEFT: Self = Self(0x0040);
    pub const DEBUG: Self = Self(0x0080);
    pub const ECMASCRIPT: Self = Self(0x0100);
    pub const UNICODE: Self = Self(0x0400);

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub const fn contains(self, o: Self) -> bool {
        self.0 & o.0 == o.0 && o.0 != 0
    }

    pub fn insert(&mut self, o: Self) {
        self.0 |= o.0;
    }

    pub fn remove(&mut self, o: Self) {
        self.0 &= !o.0;
    }
}

/// A pattern that does not compile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Error {
    pub message: String,
    pub pattern: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "error parsing regexp: {} in `{}`",
            self.message, self.pattern
        )
    }
}

impl std::error::Error for Error {}

/// A compiled regular expression.
#[derive(Debug)]
pub(crate) struct Regex {
    program: vm::Program,
}

/// The groups of a match by group number (regexp2's `Match.Groups()` order): `(start, end)`
/// character positions, `None` for a group that did not participate.
pub(crate) type Groups = Vec<Option<(usize, usize)>>;

impl Regex {
    /// Compiles `pattern` (regexp2's `Compile(pattern, 0)`).
    pub fn new(pattern: &str) -> Result<Self, Error> {
        let tree = parser::parse(pattern, Options::empty())?;
        Ok(Self {
            program: vm::Program::compile(&tree),
        })
    }

    /// The match that starts the search at `start` (regexp2's `FindRunesMatchStartingAt`): with
    /// a leading `\G` only a match at `start` is possible.
    /// `scratch` is the matcher's working memory, reused between calls.
    pub fn find_at(&self, text: &[char], start: usize, scratch: &mut Scratch) -> Option<Groups> {
        self.program.find(text, start, scratch)
    }

    /// Whether the pattern matches anywhere in `text` (`MatchString`).
    pub fn is_match(&self, text: &[char]) -> bool {
        self.find_at(text, 0, &mut Scratch::default()).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(pattern: &str, text: &str) -> Option<Vec<Option<String>>> {
        let re = Regex::new(pattern).unwrap_or_else(|e| panic!("{e}"));
        let chars: Vec<char> = text.chars().collect();
        re.find_at(&chars, 0, &mut Scratch::default()).map(|g| {
            g.iter()
                .map(|s| s.map(|(a, b)| chars[a..b].iter().collect()))
                .collect()
        })
    }

    fn whole(pattern: &str, text: &str) -> Option<String> {
        m(pattern, text).and_then(|g| g[0].clone())
    }

    #[test]
    fn literals_classes_and_quantifiers() {
        assert_eq!(whole("abc", "xxabcx").as_deref(), Some("abc"));
        assert_eq!(whole(r"\Gabc", "xxabc"), None);
        assert_eq!(whole("a{2,3}", "aaaa").as_deref(), Some("aaa"));
        assert_eq!(whole("a{2,3}?", "aaaa").as_deref(), Some("aa"));
        assert_eq!(whole("{{[-]?", "{{- x").as_deref(), Some("{{-"));
        assert_eq!(whole("x{,2}", "x{,2}").as_deref(), Some("x{,2}"));
        assert_eq!(whole(r"[^]]+", "ab]c").as_deref(), Some("ab"));
        assert_eq!(whole(r"[a-z-[aeiou]]+", "bcde").as_deref(), Some("bcd"));
        assert_eq!(whole(r"\w+", "héllo wörld").as_deref(), Some("héllo"));
        assert_eq!(whole(r"(?i)HELLO", "hello").as_deref(), Some("hello"));
        assert_eq!(whole(r"(?i)[A-C]+", "abcd").as_deref(), Some("abc"));
        assert_eq!(whole(r"\<a", "<a").as_deref(), Some("<a"));
        assert_eq!(whole(r"[\p{L}\p{N}]+", "ab12-").as_deref(), Some("ab12"));
        assert_eq!(whole(r"(?x) a b  # comment", "ab").as_deref(), Some("ab"));
        assert_eq!(whole(r"a(?#comment)b", "ab").as_deref(), Some("ab"));
    }

    #[test]
    fn anchors_in_multiline_and_not() {
        assert_eq!(whole(r"(?m)^b$", "a\nb\nc").as_deref(), Some("b"));
        assert_eq!(whole(r"^b", "a\nb"), None);
        assert_eq!(whole(r"a$", "a\n").as_deref(), Some("a"));
        assert_eq!(whole(r"a\z", "a\n"), None);
        assert_eq!(whole(r"\bfoo\b", "a foo b").as_deref(), Some("foo"));
    }

    #[test]
    fn groups_backreferences_and_lookaround() {
        let g = m(r"(\s)(\*|_)((?:(?!\2).)*)(\2)", " *em* x").expect("match");
        assert_eq!(g[3].as_deref(), Some("em"));
        assert_eq!(whole(r"(?<=\$)\w+", "$foo").as_deref(), Some("foo"));
        assert_eq!(whole(r"(?<!\$)\b\w+", "$foo bar").as_deref(), Some("bar"));
        assert_eq!(
            whole(r"(?<=<\s*(?:script|style)[^>]*)x", "<script  a>x").as_deref(),
            None
        );
        assert_eq!(
            whole(r"(?<=<\s*(?:script|style)[^>]*)>", "<script  a>").as_deref(),
            Some(">")
        );
        assert_eq!(whole(r"foo(?=bar)", "foobar").as_deref(), Some("foo"));
        assert_eq!(whole(r"(?>a+)b", "aaab").as_deref(), Some("aaab"));
        assert_eq!(whole(r"(?>a*)a", "aaa"), None);
        // Named groups are numbered after the unnamed ones.
        let g = m(r"(?<n>a)(b)", "ab").expect("match");
        assert_eq!(g[1].as_deref(), Some("b"));
        assert_eq!(g[2].as_deref(), Some("a"));
        // An unmatched group is empty; a back-reference to it fails.
        let g = m(r"(a)|(b)", "b").expect("match");
        assert_eq!(g[1], None);
        assert_eq!(whole(r"(a)?\1b", "b"), None);
        // The last iteration of a repeated group.
        let g = m(r"(\w)+", "abc").expect("match");
        assert_eq!(g[1].as_deref(), Some("c"));
    }

    #[test]
    fn loops_stop_on_empty_iterations() {
        assert_eq!(whole(r"(a|)*b", "aab").as_deref(), Some("aab"));
        assert_eq!(whole(r"(?:a?)*?b", "aab").as_deref(), Some("aab"));
        assert_eq!(whole(r"(?:x|y){2,}?z", "xyxz").as_deref(), Some("xyxz"));
        assert_eq!(
            whole(r"/(\\\n)?[*](.|\n)*?[*](\\\n)?/", "/* a\n b */ x").as_deref(),
            Some("/* a\n b */")
        );
        assert_eq!(
            whole(r"(\\\\|\\.|[^\\])*?;", r#"a\;b;"#).as_deref(),
            Some(r#"a\;b;"#)
        );
    }

    #[test]
    fn errors() {
        assert!(Regex::new("a)").is_err());
        assert!(Regex::new("(a").is_err());
        assert!(Regex::new("a**").is_err());
        assert!(Regex::new(r"\q").is_err());
        assert!(Regex::new(r"(?P<n>a)").is_err());
        assert!(Regex::new(r"\p{Greekish}").is_err());
    }
}
