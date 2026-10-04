//! The .NET regular-expression parser: pattern text → node tree.
//!
//! Ported from `syntax/parser.go` and `syntax/tree.go` of github.com/dlclark/regexp2 v1.11.5
//! (MIT; a port of .NET's `RegexParser`), for the default dialect Chroma compiles with
//! (`regexp2.Compile(pattern, 0)`): no ECMAScript or RE2 mode. Capture numbering (unnamed groups
//! first, then named ones), option scoping (`(?i)`, `(?i:…)`, `(?x)` comments and blanks),
//! literal `{` when it is not a quantifier, `\<`, class subtraction (`[a-z-[aeiou]]`) and the
//! lower-casing of literals under `i` follow regexp2. The tree optimisations of `tree.go`
//! (`reduce`) are left out: they do not change what matches.

use std::collections::BTreeMap;

use super::charclass::{CharSet, category_name, is_word_char, to_lower};
use super::{Error, Options};

mod captures;
mod escapes;
mod groups;
mod scan;
mod sets;
mod tree;

/// "No upper bound" for a quantifier (Go's `math.MaxInt32`).
pub(crate) const INFINITE: u32 = i32::MAX as u32;

/// A node of the parse tree (regexp2's `regexNode`), with the options in force where it was
/// parsed (case-insensitivity and direction matter to the matcher).
#[derive(Clone, Debug)]
pub(crate) struct Node {
    pub kind: Kind,
    pub opts: Options,
}

#[derive(Clone, Debug)]
pub(crate) enum Kind {
    One(char),
    Notone(char),
    Set(CharSet),
    Multi(Vec<char>),
    /// A back-reference to a capture slot.
    Ref(usize),
    Bol,
    Eol,
    Boundary,
    Nonboundary,
    Beginning,
    Start,
    EndZ,
    End,
    Empty,
    Alternate(Vec<Node>),
    Concatenate(Vec<Node>),
    /// `*`, `+`, `?`, `{m,n}` (greedy or lazy).
    Loop {
        child: Box<Node>,
        min: u32,
        max: u32,
        lazy: bool,
    },
    /// A capture into slot `index`.
    Capture {
        index: usize,
        child: Box<Node>,
    },
    Group(Box<Node>),
    /// `(?=…)`, `(?<=…)`.
    Require(Box<Node>),
    /// `(?!…)`, `(?<!…)`.
    Prevent(Box<Node>),
    /// `(?>…)`.
    Greedy(Box<Node>),
    /// `(?(n)yes|no)`.
    Testref {
        index: usize,
        children: Vec<Node>,
    },
    /// `(?(expr)yes|no)`.
    Testgroup(Vec<Node>),
}

impl Node {
    fn new(kind: Kind, opts: Options) -> Self {
        Self { kind, opts }
    }
}

/// The parsed regular expression.
#[derive(Debug)]
pub(crate) struct Tree {
    pub root: Node,
    /// Capture slot numbers in use, sorted (slot → dense index is the position).
    pub capnumlist: Vec<usize>,
}

/// A group being built (regexp2 keeps these as `regexNode`s on a linked stack).
#[derive(Debug)]
enum GroupKind {
    Capture(usize),
    Group,
    Require,
    Prevent,
    Greedy,
    Testref(usize),
    Testgroup,
}

#[derive(Debug)]
struct Group {
    kind: GroupKind,
    opts: Options,
    children: Vec<Node>,
}

/// The parser state saved by an open parenthesis (`pushGroup`).
#[derive(Debug)]
struct Frame {
    group: Group,
    alternation: (Vec<Node>, Options),
    concatenation: (Vec<Node>, Options),
}

struct Parser<'a> {
    raw: &'a str,
    pattern: Vec<char>,
    pos: usize,
    options: Options,
    options_stack: Vec<Options>,
    ignore_next_paren: bool,

    stack: Vec<Frame>,
    group: Option<Group>,
    alternation: (Vec<Node>, Options),
    concatenation: (Vec<Node>, Options),
    unit: Option<Node>,

    autocap: usize,
    capcount: usize,
    captop: usize,
    caps: BTreeMap<usize, usize>,
    capnames: BTreeMap<String, usize>,
    capnamelist: Vec<String>,
}

/// Parses `pattern` with the top-level `options`.
pub(crate) fn parse(pattern: &str, options: Options) -> Result<Tree, Error> {
    let mut p = Parser {
        raw: pattern,
        pattern: pattern.chars().collect(),
        pos: 0,
        options,
        options_stack: Vec::new(),
        ignore_next_paren: false,
        stack: Vec::new(),
        group: None,
        alternation: (Vec::new(), options),
        concatenation: (Vec::new(), options),
        unit: None,
        autocap: 0,
        capcount: 0,
        captop: 0,
        caps: BTreeMap::new(),
        capnames: BTreeMap::new(),
        capnamelist: Vec::new(),
    };
    p.count_captures()?;
    p.reset(options);
    let root = p.scan_regex()?;
    Ok(Tree {
        root,
        capnumlist: p.caps.keys().copied().collect(),
    })
}

// ── character categories of the pattern syntax (`_category`) ──

const Q: u8 = 5; // quantifier
const S: u8 = 4; // ordinary stopper
const Z: u8 = 3; // ScanBlank stopper
const X: u8 = 2; // whitespace

fn category(ch: char) -> u8 {
    match ch {
        '\t' | '\n' | '\u{B}' | '\u{C}' | '\r' | ' ' => X,
        '#' => Z,
        '$' | '(' | ')' | '.' | '[' | '\\' | '^' | '|' => S,
        '*' | '+' | '?' | '{' => Q,
        _ => 0,
    }
}

fn is_space(ch: char) -> bool {
    ch <= ' ' && category(ch) == X
}

fn is_special(ch: char) -> bool {
    ch <= '|' && category(ch) >= S
}

fn is_stopper_x(ch: char) -> bool {
    ch <= '|' && category(ch) >= X
}

fn is_quantifier(ch: char) -> bool {
    ch <= '{' && category(ch) >= Q
}

fn option_from_code(ch: char) -> Options {
    match ch.to_ascii_lowercase() {
        'i' => Options::IGNORE_CASE,
        'r' => Options::RIGHT_TO_LEFT,
        'm' => Options::MULTILINE,
        'n' => Options::EXPLICIT_CAPTURE,
        's' => Options::SINGLELINE,
        'x' => Options::IGNORE_PATTERN_WHITESPACE,
        'd' => Options::DEBUG,
        'e' => Options::ECMASCRIPT,
        'u' => Options::UNICODE,
        _ => Options::empty(),
    }
}

fn hex_digit(ch: char) -> Option<u32> {
    ch.to_digit(16)
}

impl Parser<'_> {
    fn err(&self, what: impl Into<String>) -> Error {
        Error {
            message: what.into(),
            pattern: self.raw.to_owned(),
        }
    }

    // ── cursor ──

    fn chars_right(&self) -> usize {
        self.pattern.len() - self.pos
    }

    fn right_char(&self, i: usize) -> char {
        self.pattern[self.pos + i]
    }

    fn move_right_get_char(&mut self) -> char {
        let c = self.pattern[self.pos];
        self.pos += 1;
        c
    }

    fn move_right(&mut self, i: usize) {
        self.pos += i;
    }

    fn move_left(&mut self) {
        self.pos -= 1;
    }

    // ── options ──

    fn use_i(&self) -> bool {
        self.options.contains(Options::IGNORE_CASE)
    }

    fn use_m(&self) -> bool {
        self.options.contains(Options::MULTILINE)
    }

    fn use_n(&self) -> bool {
        self.options.contains(Options::EXPLICIT_CAPTURE)
    }

    fn use_s(&self) -> bool {
        self.options.contains(Options::SINGLELINE)
    }

    fn use_x(&self) -> bool {
        self.options.contains(Options::IGNORE_PATTERN_WHITESPACE)
    }

    fn push_options(&mut self) {
        self.options_stack.push(self.options);
    }

    fn pop_options(&mut self) {
        if let Some(o) = self.options_stack.pop() {
            self.options = o;
        }
    }

    fn pop_keep_options(&mut self) {
        self.options_stack.pop();
    }
}

fn clamp(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(INFINITE).min(INFINITE)
}

/// A set node, or a single (or all-but-one) character when the set is that (`reduceSet`;
/// kept because a single character with `i` compares lower-cased input with it).
fn set_node(set: CharSet, opts: Options) -> Node {
    if let Some(c) = set.singleton() {
        Node::new(Kind::One(c), opts)
    } else if let Some(c) = set.singleton_inverse() {
        Node::new(Kind::Notone(c), opts)
    } else {
        Node::new(Kind::Set(set), opts)
    }
}

/// `makeQuantifier`: `{0}` is empty, `{1}` the node itself.
fn make_quantifier(node: Node, lazy: bool, min: u32, max: u32) -> Node {
    if min == 0 && max == 0 {
        return Node::new(Kind::Empty, node.opts);
    }
    if min == 1 && max == 1 {
        return node;
    }
    let opts = node.opts;
    Node::new(
        Kind::Loop {
            child: Box::new(node),
            min,
            max,
            lazy,
        },
        opts,
    )
}
