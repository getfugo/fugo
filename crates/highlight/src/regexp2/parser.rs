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

    // ── captures ──

    fn note_capture_slot(&mut self, i: usize, pos: usize) {
        if let std::collections::btree_map::Entry::Vacant(e) = self.caps.entry(i) {
            e.insert(pos);
            self.capcount += 1;
            if self.captop <= i {
                self.captop = i + 1;
            }
        }
    }

    fn note_capture_name(&mut self, name: String, pos: usize) {
        if !self.capnames.contains_key(&name) {
            self.capnames.insert(name.clone(), pos);
            self.capnamelist.push(name);
        }
    }

    fn is_capture_slot(&self, i: usize) -> bool {
        self.caps.contains_key(&i)
    }

    fn is_capture_name(&self, name: &str) -> bool {
        self.capnames.contains_key(name)
    }

    fn capture_slot_from_name(&self, name: &str) -> usize {
        self.capnames.get(name).copied().unwrap_or(0)
    }

    fn consume_autocap(&mut self) -> usize {
        let r = self.autocap;
        self.autocap += 1;
        r
    }

    /// Named groups take the slots after the numbered ones (`assignNameSlots`).
    fn assign_name_slots(&mut self) {
        let names = self.capnamelist.clone();
        for name in names {
            while self.is_capture_slot(self.autocap) {
                self.autocap += 1;
            }
            let pos = self.capnames[&name];
            self.capnames.insert(name, self.autocap);
            self.note_capture_slot(self.autocap, pos);
            self.autocap += 1;
        }
    }

    /// The pre-scan that numbers the capture groups (`countCaptures`).
    fn count_captures(&mut self) -> Result<(), Error> {
        self.note_capture_slot(0, 0);
        self.autocap = 1;
        while self.chars_right() > 0 {
            let pos = self.pos;
            let ch = self.move_right_get_char();
            match ch {
                '\\' => {
                    if self.chars_right() > 0 {
                        self.scan_backslash(true)?;
                    }
                }
                '#' => {
                    if self.use_x() {
                        self.move_left();
                        self.scan_blank()?;
                    }
                }
                '[' => {
                    self.scan_char_set(false, true)?;
                }
                ')' => {
                    if !self.options_stack.is_empty() {
                        self.pop_options();
                    }
                }
                '(' => {
                    if self.chars_right() >= 2
                        && self.right_char(1) == '#'
                        && self.right_char(0) == '?'
                    {
                        self.move_left();
                        self.scan_blank()?;
                    } else {
                        self.push_options();
                        if self.chars_right() > 0 && self.right_char(0) == '?' {
                            self.move_right(1);
                            if self.chars_right() > 1
                                && (self.right_char(0) == '<' || self.right_char(0) == '\'')
                            {
                                self.move_right(1);
                                let ch = self.right_char(0);
                                if ch != '0' && is_word_char(ch) {
                                    if ch.is_ascii_digit() {
                                        let dec = self.scan_decimal()?;
                                        self.note_capture_slot(dec, pos);
                                    } else {
                                        let name = self.scan_capname();
                                        self.note_capture_name(name, pos);
                                    }
                                }
                            } else {
                                self.scan_options();
                                if self.chars_right() > 0 {
                                    if self.right_char(0) == ')' {
                                        self.move_right(1);
                                        self.pop_keep_options();
                                    } else if self.right_char(0) == '(' {
                                        self.ignore_next_paren = true;
                                        continue;
                                    }
                                }
                            }
                        } else if !self.use_n() && !self.ignore_next_paren {
                            let n = self.consume_autocap();
                            self.note_capture_slot(n, pos);
                        }
                    }
                    self.ignore_next_paren = false;
                }
                _ => {}
            }
        }
        self.assign_name_slots();
        Ok(())
    }

    fn reset(&mut self, options: Options) {
        self.pos = 0;
        self.autocap = 1;
        self.ignore_next_paren = false;
        self.options_stack.clear();
        self.options = options;
        self.stack.clear();
    }

    // ── tree building ──

    fn start_group(&mut self, group: Group) {
        self.group = Some(group);
        self.alternation = (Vec::new(), self.options);
        self.concatenation = (Vec::new(), self.options);
    }

    fn push_group(&mut self) {
        let group = self.group.take().expect("group");
        self.stack.push(Frame {
            group,
            alternation: std::mem::take(&mut self.alternation),
            concatenation: std::mem::take(&mut self.concatenation),
        });
    }

    fn pop_group(&mut self) -> Result<(), Error> {
        let frame = self.stack.pop().expect("stack");
        self.group = Some(frame.group);
        self.alternation = frame.alternation;
        self.concatenation = frame.concatenation;
        let group = self.group.as_mut().expect("group");
        if matches!(group.kind, GroupKind::Testgroup) && group.children.is_empty() {
            let Some(unit) = self.unit.take() else {
                return Err(self.err("illegal conditional (?(...)) expression"));
            };
            self.group.as_mut().expect("group").children.push(unit);
        }
        Ok(())
    }

    /// The finished concatenation, reversed for right-to-left matching (`reverseLeft`).
    fn take_concatenation(&mut self) -> Node {
        let (mut children, opts) =
            std::mem::replace(&mut self.concatenation, (Vec::new(), self.options));
        if opts.contains(Options::RIGHT_TO_LEFT) {
            children.reverse();
        }
        Node::new(Kind::Concatenate(children), opts)
    }

    fn add_group(&mut self) -> Result<(), Error> {
        let concat = self.take_concatenation();
        let mut group = self.group.take().expect("group");
        match group.kind {
            GroupKind::Testgroup | GroupKind::Testref(_) => {
                group.children.push(concat);
                let max = if matches!(group.kind, GroupKind::Testref(_)) {
                    2
                } else {
                    3
                };
                if group.children.len() > max {
                    return Err(self.err("too many | in (?()|)"));
                }
            }
            _ => {
                let (mut alts, opts) = std::mem::take(&mut self.alternation);
                alts.push(concat);
                group.children.push(Node::new(Kind::Alternate(alts), opts));
            }
        }
        let opts = group.opts;
        let mut children = group.children;
        let only = |children: &mut Vec<Node>| Box::new(children.pop().expect("child"));
        let kind = match group.kind {
            GroupKind::Capture(index) => Kind::Capture {
                index,
                child: only(&mut children),
            },
            GroupKind::Group => Kind::Group(only(&mut children)),
            GroupKind::Require => Kind::Require(only(&mut children)),
            GroupKind::Prevent => Kind::Prevent(only(&mut children)),
            GroupKind::Greedy => Kind::Greedy(only(&mut children)),
            GroupKind::Testref(index) => Kind::Testref { index, children },
            GroupKind::Testgroup => Kind::Testgroup(children),
        };
        self.unit = Some(Node::new(kind, opts));
        Ok(())
    }

    fn add_alternate(&mut self) {
        let concat = self.take_concatenation();
        let group = self.group.as_mut().expect("group");
        if matches!(group.kind, GroupKind::Testgroup | GroupKind::Testref(_)) {
            group.children.push(concat);
        } else {
            self.alternation.0.push(concat);
        }
    }

    fn add_concatenate(&mut self) {
        if let Some(unit) = self.unit.take() {
            self.concatenation.0.push(unit);
        }
    }

    fn add_concatenate_quantified(&mut self, lazy: bool, min: u32, max: u32) {
        if let Some(unit) = self.unit.take() {
            let q = make_quantifier(unit, lazy, min, max);
            self.concatenation.0.push(q);
        }
    }

    fn add_unit_one(&mut self, mut ch: char) {
        if self.use_i() {
            ch = to_lower(ch);
        }
        self.unit = Some(Node::new(Kind::One(ch), self.options));
    }

    fn add_unit_notone(&mut self, mut ch: char) {
        if self.use_i() {
            ch = to_lower(ch);
        }
        self.unit = Some(Node::new(Kind::Notone(ch), self.options));
    }

    fn add_unit_set(&mut self, set: CharSet) {
        self.unit = Some(set_node(set, self.options));
    }

    fn add_unit_type(&mut self, kind: Kind) {
        self.unit = Some(Node::new(kind, self.options));
    }

    /// Adds the literal run `pattern[pos..pos+n]` (`addToConcatenate`).
    fn add_to_concatenate(&mut self, pos: usize, n: usize) {
        if n == 0 {
            return;
        }
        let i = self.use_i();
        let node = if n > 1 {
            let s = self.pattern[pos..pos + n]
                .iter()
                .map(|&c| if i { to_lower(c) } else { c })
                .collect();
            Node::new(Kind::Multi(s), self.options)
        } else {
            let c = self.pattern[pos];
            Node::new(Kind::One(if i { to_lower(c) } else { c }), self.options)
        };
        self.concatenation.0.push(node);
    }

    // ── the main scan (`scanRegex`) ──

    #[expect(
        clippy::too_many_lines,
        reason = "a port of regexp2's scanRegex, kept in one piece"
    )]
    fn scan_regex(&mut self) -> Result<Node, Error> {
        let mut is_quant = false;
        self.start_group(Group {
            kind: GroupKind::Capture(0),
            opts: self.options,
            children: Vec::new(),
        });

        'outer: while self.chars_right() > 0 {
            let was_prev_quantifier = is_quant;
            is_quant = false;
            self.scan_blank()?;
            let startpos = self.pos;

            // Move past all of the normal characters.
            if self.use_x() {
                while self.chars_right() > 0 {
                    let ch = self.right_char(0);
                    if is_stopper_x(ch) && (ch != '{' || self.is_true_quantifier()) {
                        break;
                    }
                    self.move_right(1);
                }
            } else {
                while self.chars_right() > 0 {
                    let ch = self.right_char(0);
                    if is_special(ch) && (ch != '{' || self.is_true_quantifier()) {
                        break;
                    }
                    self.move_right(1);
                }
            }
            let endpos = self.pos;
            self.scan_blank()?;

            let ch = if self.chars_right() == 0 {
                '!' // at end
            } else {
                let c = self.right_char(0);
                if is_special(c) {
                    is_quant = is_quantifier(c);
                    self.move_right(1);
                    c
                } else {
                    ' ' // at an ordinary character
                }
            };

            let mut was_prev_quantifier = was_prev_quantifier;
            if startpos < endpos {
                let mut unquantified = endpos - startpos;
                if is_quant {
                    unquantified -= 1;
                }
                was_prev_quantifier = false;
                if unquantified > 0 {
                    self.add_to_concatenate(startpos, unquantified);
                }
                if is_quant {
                    self.add_unit_one(self.pattern[endpos - 1]);
                }
            }

            match ch {
                '!' => break 'outer,
                ' ' => continue 'outer,
                '[' => {
                    let set = self.scan_char_set(self.use_i(), false)?;
                    self.add_unit_set(set.expect("set"));
                }
                '(' => {
                    self.push_options();
                    match self.scan_group_open()? {
                        None => self.pop_keep_options(),
                        Some(group) => {
                            self.push_group();
                            self.start_group(group);
                        }
                    }
                    continue 'outer;
                }
                '|' => {
                    self.add_alternate();
                    continue 'outer;
                }
                ')' => {
                    if self.stack.is_empty() {
                        return Err(self.err("unexpected )"));
                    }
                    self.add_group()?;
                    self.pop_group()?;
                    self.pop_options();
                    if self.unit.is_none() {
                        continue 'outer;
                    }
                }
                '\\' => {
                    let node = self.scan_backslash(false)?;
                    self.unit = node;
                }
                '^' => {
                    if self.use_m() {
                        self.add_unit_type(Kind::Bol);
                    } else {
                        self.add_unit_type(Kind::Beginning);
                    }
                }
                '$' => {
                    if self.use_m() {
                        self.add_unit_type(Kind::Eol);
                    } else {
                        self.add_unit_type(Kind::EndZ);
                    }
                }
                '.' => {
                    if self.use_s() {
                        self.add_unit_set(CharSet::any());
                    } else {
                        self.add_unit_notone('\n');
                    }
                }
                '{' | '*' | '+' | '?' => {
                    if self.unit.is_none() {
                        return Err(self.err(if was_prev_quantifier {
                            "invalid nested repetition operator"
                        } else {
                            "missing argument to repetition operator"
                        }));
                    }
                    self.move_left();
                }
                _ => return Err(self.err("internal error")),
            }

            self.scan_blank()?;
            if self.chars_right() > 0 {
                is_quant = self.is_true_quantifier();
            }
            if self.chars_right() == 0 || !is_quant {
                self.add_concatenate();
                continue 'outer;
            }

            let mut ch = self.move_right_get_char();
            // Handle quantifiers.
            while self.unit.is_some() {
                let (min, max) = match ch {
                    '*' => (0, INFINITE),
                    '?' => (0, 1),
                    '+' => (1, INFINITE),
                    '{' => {
                        let startpos = self.pos;
                        let min = self.scan_decimal()?;
                        let mut max = min;
                        if startpos < self.pos
                            && self.chars_right() > 0
                            && self.right_char(0) == ','
                        {
                            self.move_right(1);
                            if self.chars_right() == 0 || self.right_char(0) == '}' {
                                max = INFINITE as usize;
                            } else {
                                max = self.scan_decimal()?;
                            }
                        }
                        if startpos == self.pos
                            || self.chars_right() == 0
                            || self.move_right_get_char() != '}'
                        {
                            self.add_concatenate();
                            self.pos = startpos - 1;
                            continue 'outer;
                        }
                        (clamp(min), clamp(max))
                    }
                    _ => return Err(self.err("internal error")),
                };
                self.scan_blank()?;
                let lazy = if self.chars_right() == 0 || self.right_char(0) != '?' {
                    false
                } else {
                    self.move_right(1);
                    true
                };
                if min > max {
                    return Err(self.err("invalid repeat count"));
                }
                self.add_concatenate_quantified(lazy, min, max);
                ch = '\0';
            }
        }

        if !self.stack.is_empty() {
            return Err(self.err("missing closing )"));
        }
        self.add_group()?;
        Ok(self.unit.take().expect("root"))
    }

    /// Whether a quantifier starts here (`{n}`, `{n,}`, `{n,m}` or `*+?`).
    fn is_true_quantifier(&self) -> bool {
        let mut n = self.chars_right();
        if n == 0 {
            return false;
        }
        let start = self.pos;
        let mut ch = self.pattern[start];
        if ch != '{' {
            return is_quantifier(ch);
        }
        let mut pos = start;
        loop {
            n -= 1;
            if n == 0 {
                break;
            }
            pos += 1;
            ch = self.pattern[pos];
            if !ch.is_ascii_digit() {
                break;
            }
        }
        if n == 0 || pos - start == 1 {
            return false;
        }
        if ch == '}' {
            return true;
        }
        if ch != ',' {
            return false;
        }
        loop {
            n -= 1;
            if n == 0 {
                break;
            }
            pos += 1;
            ch = self.pattern[pos];
            if !ch.is_ascii_digit() {
                break;
            }
        }
        n > 0 && ch == '}'
    }

    /// The group after `(` (`scanGroupOpen`); `None` for an option setting or comment.
    #[expect(clippy::too_many_lines, reason = "a port of regexp2's scanGroupOpen")]
    fn scan_group_open(&mut self) -> Result<Option<Group>, Error> {
        let start = self.pos;
        let mut close = '>';

        if self.chars_right() == 0
            || self.right_char(0) != '?'
            || (self.chars_right() > 1 && self.right_char(1) == ')')
        {
            if self.use_n() || self.ignore_next_paren {
                self.ignore_next_paren = false;
                return Ok(Some(self.group(GroupKind::Group)));
            }
            let n = self.consume_autocap();
            return Ok(Some(self.group(GroupKind::Capture(n))));
        }
        self.move_right(1);

        let unrecognized = |p: &Self| {
            let text: String = p.pattern[start..p.pos].iter().collect();
            p.err(format!("unrecognized grouping construct: ({text}"))
        };

        if self.chars_right() == 0 {
            return Err(unrecognized(self));
        }
        let kind = match self.move_right_get_char() {
            ':' => GroupKind::Group,
            '=' => {
                self.options.remove(Options::RIGHT_TO_LEFT);
                GroupKind::Require
            }
            '!' => {
                self.options.remove(Options::RIGHT_TO_LEFT);
                GroupKind::Prevent
            }
            '>' => GroupKind::Greedy,
            c @ ('\'' | '<') => {
                if c == '\'' {
                    close = '\'';
                }
                if self.chars_right() == 0 {
                    return Err(unrecognized(self));
                }
                match self.move_right_get_char() {
                    '=' if close != '\'' => {
                        self.options.insert(Options::RIGHT_TO_LEFT);
                        GroupKind::Require
                    }
                    '!' if close != '\'' => {
                        self.options.insert(Options::RIGHT_TO_LEFT);
                        GroupKind::Prevent
                    }
                    '=' | '!' => return Err(unrecognized(self)),
                    ch => {
                        self.move_left();
                        let mut capnum: Option<usize> = None;
                        let mut proceed = false;
                        if ch.is_ascii_digit() {
                            let n = self.scan_decimal()?;
                            capnum = self.is_capture_slot(n).then_some(n);
                            if self.chars_right() > 0
                                && !(self.right_char(0) == close || self.right_char(0) == '-')
                            {
                                return Err(self.err("invalid group name"));
                            }
                            if n == 0 {
                                return Err(self.err("capture number cannot be zero"));
                            }
                        } else if is_word_char(ch) {
                            let name = self.scan_capname();
                            if self.is_capture_name(&name) {
                                capnum = Some(self.capture_slot_from_name(&name));
                            }
                            if self.chars_right() > 0
                                && !(self.right_char(0) == close || self.right_char(0) == '-')
                            {
                                return Err(self.err("invalid group name"));
                            }
                        } else if ch == '-' {
                            proceed = true;
                        } else {
                            return Err(self.err("invalid group name"));
                        }
                        if (capnum.is_some() || proceed)
                            && self.chars_right() > 0
                            && self.right_char(0) == '-'
                        {
                            // Balancing groups (`(?<a-b>…)`): not used by Chroma's lexers.
                            return Err(self.err("balancing groups are not supported"));
                        }
                        if let Some(n) = capnum
                            && self.chars_right() > 0
                            && self.move_right_get_char() == close
                        {
                            return Ok(Some(self.group(GroupKind::Capture(n))));
                        }
                        return Err(unrecognized(self));
                    }
                }
            }
            '(' => {
                let paren_pos = self.pos;
                if self.chars_right() > 0 {
                    let ch = self.right_char(0);
                    if ch.is_ascii_digit() {
                        let n = self.scan_decimal()?;
                        if self.chars_right() > 0 && self.move_right_get_char() == ')' {
                            if self.is_capture_slot(n) {
                                return Ok(Some(self.group(GroupKind::Testref(n))));
                            }
                            return Err(self.err(format!("(?({n}) ) reference to undefined group")));
                        }
                        return Err(self.err(format!("(?({n}) ) malformed")));
                    } else if is_word_char(ch) {
                        let name = self.scan_capname();
                        if self.is_capture_name(&name)
                            && self.chars_right() > 0
                            && self.move_right_get_char() == ')'
                        {
                            let n = self.capture_slot_from_name(&name);
                            return Ok(Some(self.group(GroupKind::Testref(n))));
                        }
                    }
                }
                self.pos = paren_pos - 1;
                self.ignore_next_paren = true;
                let n = self.chars_right();
                if n >= 3 && self.right_char(1) == '?' {
                    let c2 = self.right_char(2);
                    if c2 == '#' {
                        return Err(self.err("alternation conditions cannot be comments"));
                    }
                    if c2 == '\''
                        || (n >= 4
                            && c2 == '<'
                            && self.right_char(3) != '!'
                            && self.right_char(3) != '=')
                    {
                        return Err(
                            self.err("alternation conditions do not capture and cannot be named")
                        );
                    }
                }
                GroupKind::Testgroup
            }
            _ => {
                self.move_left();
                if !matches!(
                    self.group.as_ref().map(|g| &g.kind),
                    Some(GroupKind::Testgroup)
                ) {
                    self.scan_options();
                }
                if self.chars_right() == 0 {
                    return Err(unrecognized(self));
                }
                match self.move_right_get_char() {
                    ')' => return Ok(None),
                    ':' => GroupKind::Group,
                    _ => return Err(unrecognized(self)),
                }
            }
        };
        Ok(Some(self.group(kind)))
    }

    fn group(&self, kind: GroupKind) -> Group {
        Group {
            kind,
            opts: self.options,
            children: Vec::new(),
        }
    }

    /// Backslash specials and basics (`scanBackslash`).
    fn scan_backslash(&mut self, scan_only: bool) -> Result<Option<Node>, Error> {
        if self.chars_right() == 0 {
            return Err(self.err("illegal \\ at end of pattern"));
        }
        let ch = self.right_char(0);
        let o = self.options;
        let kind = match ch {
            'b' => Kind::Boundary,
            'B' => Kind::Nonboundary,
            'A' => Kind::Beginning,
            'G' => Kind::Start,
            'Z' => Kind::EndZ,
            'z' => Kind::End,
            'w' | 'W' => Kind::Set(CharSet::word(ch == 'W')),
            's' | 'S' => Kind::Set(CharSet::space(ch == 'S')),
            'd' | 'D' => Kind::Set(CharSet::digit(ch == 'D')),
            'p' | 'P' => {
                self.move_right(1);
                let prop = self.parse_property()?;
                let mut cc = CharSet::default();
                cc.add_category(prop, ch != 'p', self.use_i());
                if self.use_i() {
                    cc.add_lowercase();
                }
                return Ok(Some(set_node(cc, o)));
            }
            _ => return self.scan_basic_backslash(scan_only),
        };
        self.move_right(1);
        Ok(Some(match kind {
            Kind::Set(set) => set_node(set, o),
            k => Node::new(k, o),
        }))
    }

    /// Back-references and character escapes (`scanBasicBackslash`).
    fn scan_basic_backslash(&mut self, scan_only: bool) -> Result<Option<Node>, Error> {
        if self.chars_right() == 0 {
            return Err(self.err("illegal \\ at end of pattern"));
        }
        let mut angled = false;
        let mut k = false;
        let mut close = '\0';
        let backpos = self.pos;
        let mut ch = self.right_char(0);

        if ch == 'k' {
            if self.chars_right() >= 2 {
                self.move_right(1);
                ch = self.move_right_get_char();
                if ch == '<' || ch == '\'' {
                    angled = true;
                    close = if ch == '\'' { '\'' } else { '>' };
                }
            }
            if !angled || self.chars_right() == 0 {
                return Err(self.err("malformed \\k<...> named back reference"));
            }
            ch = self.right_char(0);
            k = true;
        } else if (ch == '<' || ch == '\'') && self.chars_right() > 1 {
            angled = true;
            close = if ch == '\'' { '\'' } else { '>' };
            self.move_right(1);
            ch = self.right_char(0);
        }

        if angled && ch.is_ascii_digit() {
            let n = self.scan_decimal()?;
            if self.chars_right() > 0 && self.move_right_get_char() == close {
                if self.is_capture_slot(n) {
                    return Ok(Some(Node::new(Kind::Ref(n), self.options)));
                }
                return Err(self.err(format!("reference to undefined group number {n}")));
            }
        } else if !angled && ('1'..='9').contains(&ch) {
            let n = self.scan_decimal()?;
            if scan_only {
                return Ok(None);
            }
            if self.is_capture_slot(n) {
                return Ok(Some(Node::new(Kind::Ref(n), self.options)));
            }
            if n <= 9 {
                return Err(self.err(format!("reference to undefined group number {n}")));
            }
        } else if angled {
            let name = self.scan_capname();
            if !name.is_empty() && self.chars_right() > 0 && self.move_right_get_char() == close {
                if scan_only {
                    return Ok(None);
                }
                if self.is_capture_name(&name) {
                    let n = self.capture_slot_from_name(&name);
                    return Ok(Some(Node::new(Kind::Ref(n), self.options)));
                }
                return Err(self.err(format!("reference to undefined group name {name}")));
            } else if k {
                return Err(self.err("malformed \\k<...> named back reference"));
            }
        }

        // Not a back-reference: a character escape.
        self.pos = backpos;
        let mut ch = self.scan_char_escape()?;
        if scan_only {
            return Ok(None);
        }
        if self.use_i() {
            ch = to_lower(ch);
        }
        Ok(Some(Node::new(Kind::One(ch), self.options)))
    }

    /// `X` of `\p{X}` / `\pX` (`parseProperty`).
    fn parse_property(&mut self) -> Result<&'static str, Error> {
        if self.chars_right() >= 1 && self.right_char(0) != '{' {
            let ch = self.move_right_get_char().to_string();
            return category_name(&ch).ok_or_else(|| {
                self.err(format!(
                    "unknown unicode category, script, or property '{ch}'"
                ))
            });
        }
        if self.chars_right() < 3 {
            return Err(self.err("incomplete \\p{X} character escape"));
        }
        if self.move_right_get_char() != '{' {
            return Err(self.err("malformed \\p{X} character escape"));
        }
        let start = self.pos;
        while self.chars_right() > 0 {
            let ch = self.move_right_get_char();
            if !(is_word_char(ch) || ch == '-') {
                self.move_left();
                break;
            }
        }
        let name: String = self.pattern[start..self.pos].iter().collect();
        if self.chars_right() == 0 || self.move_right_get_char() != '}' {
            return Err(self.err("incomplete \\p{X} character escape"));
        }
        category_name(&name).ok_or_else(|| {
            self.err(format!(
                "unknown unicode category, script, or property '{name}'"
            ))
        })
    }

    /// Blanks and comments: `(?#…)` always, whitespace and `#…` lines with `x` (`scanBlank`).
    fn scan_blank(&mut self) -> Result<(), Error> {
        if self.use_x() {
            loop {
                while self.chars_right() > 0 && is_space(self.right_char(0)) {
                    self.move_right(1);
                }
                if self.chars_right() == 0 {
                    break;
                }
                if self.right_char(0) == '#' {
                    while self.chars_right() > 0 && self.right_char(0) != '\n' {
                        self.move_right(1);
                    }
                } else if self.chars_right() >= 3
                    && self.right_char(2) == '#'
                    && self.right_char(1) == '?'
                    && self.right_char(0) == '('
                {
                    self.skip_comment()?;
                } else {
                    break;
                }
            }
        } else {
            while self.chars_right() >= 3
                && self.right_char(2) == '#'
                && self.right_char(1) == '?'
                && self.right_char(0) == '('
            {
                self.skip_comment()?;
            }
        }
        Ok(())
    }

    fn skip_comment(&mut self) -> Result<(), Error> {
        while self.chars_right() > 0 && self.right_char(0) != ')' {
            self.move_right(1);
        }
        if self.chars_right() == 0 {
            return Err(self.err("unterminated comment"));
        }
        self.move_right(1);
        Ok(())
    }

    fn scan_capname(&mut self) -> String {
        let start = self.pos;
        while self.chars_right() > 0 {
            if !is_word_char(self.move_right_get_char()) {
                self.move_left();
                break;
            }
        }
        self.pattern[start..self.pos].iter().collect()
    }

    /// The contents of `[…]` (`scanCharSet`); `None` when only scanning.
    #[expect(clippy::too_many_lines, reason = "a port of regexp2's scanCharSet")]
    fn scan_char_set(
        &mut self,
        ignore_case: bool,
        scan_only: bool,
    ) -> Result<Option<CharSet>, Error> {
        let mut ch_prev = '\0';
        let mut in_range = false;
        let mut first_char = true;
        let mut closed = false;
        let mut cc = CharSet::default();

        if self.chars_right() > 0 && self.right_char(0) == '^' {
            self.move_right(1);
            cc.negate = true;
        }

        while self.chars_right() > 0 {
            let mut translated = false;
            let mut ch = self.move_right_get_char();
            let mut skip = false;
            if ch == ']' {
                if !first_char {
                    closed = true;
                    break;
                }
            } else if ch == '\\' && self.chars_right() > 0 {
                ch = self.move_right_get_char();
                match ch {
                    'D' | 'd' => {
                        if !scan_only {
                            if in_range {
                                return Err(self.err(format!(
                                    "cannot include class \\{ch} in character range"
                                )));
                            }
                            cc.add_digit(ch == 'D');
                        }
                        skip = true;
                    }
                    'S' | 's' => {
                        if !scan_only {
                            if in_range {
                                return Err(self.err(format!(
                                    "cannot include class \\{ch} in character range"
                                )));
                            }
                            cc.add_space(ch == 'S');
                        }
                        skip = true;
                    }
                    'W' | 'w' => {
                        if !scan_only {
                            if in_range {
                                return Err(self.err(format!(
                                    "cannot include class \\{ch} in character range"
                                )));
                            }
                            cc.add_word(ch == 'W');
                        }
                        skip = true;
                    }
                    'p' | 'P' => {
                        if !scan_only {
                            if in_range {
                                return Err(self.err(format!(
                                    "cannot include class \\{ch} in character range"
                                )));
                            }
                            let prop = self.parse_property()?;
                            cc.add_category(prop, ch != 'p', ignore_case);
                        } else {
                            let _ = self.parse_property();
                        }
                        skip = true;
                    }
                    '-' => {
                        if !scan_only {
                            cc.add_char(ch);
                        }
                        skip = true;
                    }
                    _ => {
                        self.move_left();
                        ch = self.scan_char_escape()?;
                        translated = true;
                    }
                }
            } else if ch == '[' {
                // POSIX-style `[:name:]`: skipped (regexp2 only acts on it in RE2 mode).
                if self.chars_right() > 0 && self.right_char(0) == ':' && !in_range {
                    let save = self.pos;
                    self.move_right(1);
                    if self.chars_right() > 1 && self.right_char(0) == '^' {
                        self.move_right(1);
                    }
                    self.scan_capname();
                    if self.chars_right() < 2
                        || self.move_right_get_char() != ':'
                        || self.move_right_get_char() != ']'
                    {
                        self.pos = save;
                    }
                }
            }
            if skip {
                first_char = false;
                continue;
            }

            if in_range {
                in_range = false;
                if !scan_only {
                    if ch == '[' && !translated && !first_char {
                        // A subtraction after a character: `[a-[b]]`.
                        cc.add_char(ch_prev);
                        let sub = self.scan_char_set(ignore_case, false)?.expect("set");
                        cc.add_subtraction(sub);
                        if self.chars_right() > 0 && self.right_char(0) != ']' {
                            return Err(self.err(
                                "a subtraction must be the last element in a character class",
                            ));
                        }
                    } else {
                        if ch_prev > ch {
                            return Err(
                                self.err(format!("[{ch_prev}-{ch}] range in reverse order"))
                            );
                        }
                        cc.add_range(ch_prev as u32, ch as u32);
                    }
                }
            } else if self.chars_right() >= 2
                && self.right_char(0) == '-'
                && self.right_char(1) != ']'
            {
                ch_prev = ch;
                in_range = true;
                self.move_right(1);
            } else if self.chars_right() >= 1
                && ch == '-'
                && !translated
                && self.right_char(0) == '['
                && !first_char
            {
                // A subtraction after a range: `[a-z-[b]]`.
                self.move_right(1);
                if scan_only {
                    self.scan_char_set(ignore_case, true)?;
                } else {
                    let sub = self.scan_char_set(ignore_case, false)?.expect("set");
                    cc.add_subtraction(sub);
                    if self.chars_right() > 0 && self.right_char(0) != ']' {
                        return Err(
                            self.err("a subtraction must be the last element in a character class")
                        );
                    }
                }
            } else if !scan_only {
                cc.add_range(ch as u32, ch as u32);
            }
            first_char = false;
        }

        if !closed {
            return Err(self.err("unterminated [] set"));
        }
        if scan_only {
            return Ok(None);
        }
        if ignore_case {
            cc.add_lowercase();
        }
        Ok(Some(cc))
    }

    /// Decimal digits, pegged at `i32::MAX` (`scanDecimal`).
    fn scan_decimal(&mut self) -> Result<usize, Error> {
        let mut i: usize = 0;
        while self.chars_right() > 0 {
            let Some(d) = self.right_char(0).to_digit(10) else {
                break;
            };
            self.move_right(1);
            let max = i32::MAX as usize;
            if i > max / 10 || (i == max / 10 && d as usize > max % 10) {
                return Err(self.err("capture group number out of range"));
            }
            i = i * 10 + d as usize;
        }
        Ok(i)
    }

    /// `cimsx-cimsx` up to the first unrecognised character (`scanOptions`).
    fn scan_options(&mut self) {
        let mut off = false;
        while self.chars_right() > 0 {
            let ch = self.right_char(0);
            if ch == '-' {
                off = true;
            } else if ch == '+' {
                off = false;
            } else {
                let o = option_from_code(ch);
                if o.is_empty() || o == Options::RIGHT_TO_LEFT || o == Options::ECMASCRIPT {
                    return;
                }
                if off {
                    self.options.remove(o);
                } else {
                    self.options.insert(o);
                }
            }
            self.move_right(1);
        }
    }

    /// A `\` escape for one character (`scanCharEscape`).
    fn scan_char_escape(&mut self) -> Result<char, Error> {
        let ch = self.move_right_get_char();
        if ('0'..='7').contains(&ch) {
            self.move_left();
            return Ok(self.scan_octal());
        }
        let c = match ch {
            'x' => {
                if self.chars_right() > 0 && self.right_char(0) == '{' {
                    self.move_right(1);
                    return self.scan_hex_until_brace();
                }
                self.scan_hex(2)?
            }
            'u' => self.scan_hex(4)?,
            'a' => '\u{7}',
            'b' => '\u{8}',
            'e' => '\u{1B}',
            'f' => '\u{C}',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            'v' => '\u{B}',
            'c' => self.scan_control()?,
            _ => {
                if is_word_char(ch) {
                    return Err(self.err(format!("unrecognized escape sequence \\{ch}")));
                }
                ch
            }
        };
        Ok(c)
    }

    fn scan_control(&mut self) -> Result<char, Error> {
        if self.chars_right() == 0 {
            return Err(self.err("missing control character"));
        }
        let mut ch = self.move_right_get_char() as u32;
        if (u32::from('a')..=u32::from('z')).contains(&ch) {
            ch -= u32::from('a') - u32::from('A');
        }
        match ch.checked_sub(u32::from('@')) {
            Some(c) if c < u32::from(' ') => Ok(char::from_u32(c).unwrap_or('\0')),
            _ => Err(self.err("unrecognized control character")),
        }
    }

    fn scan_hex_until_brace(&mut self) -> Result<char, Error> {
        let mut i: u32 = 0;
        let mut has_content = false;
        while self.chars_right() > 0 {
            let ch = self.move_right_get_char();
            if ch == '}' {
                if !has_content {
                    return Err(self.err("insufficient hexadecimal digits"));
                }
                return char::from_u32(i)
                    .ok_or_else(|| self.err("hex values may not be larger than 0x10FFFF"));
            }
            has_content = true;
            let Some(d) = hex_digit(ch) else {
                return Err(self.err("missing closing }"));
            };
            i = i * 16 + d;
            if i > 0x10_FFFF {
                return Err(self.err("hex values may not be larger than 0x10FFFF"));
            }
        }
        Err(self.err("missing closing }"))
    }

    fn scan_hex(&mut self, mut c: usize) -> Result<char, Error> {
        let mut i: u32 = 0;
        if self.chars_right() >= c {
            while c > 0 {
                let Some(d) = hex_digit(self.move_right_get_char()) else {
                    break;
                };
                i = i * 16 + d;
                c -= 1;
            }
        }
        if c > 0 {
            return Err(self.err("insufficient hexadecimal digits"));
        }
        Ok(char::from_u32(i).unwrap_or('\u{FFFD}'))
    }

    /// Up to three octal digits, truncated to 8 bits (`scanOctal`).
    fn scan_octal(&mut self) -> char {
        let mut c = 3.min(self.chars_right());
        let mut i: u32 = 0;
        let mut d = self.right_char(0) as u32;
        while c > 0 && (u32::from('0')..=u32::from('7')).contains(&d) {
            i = i * 8 + (d - u32::from('0'));
            c -= 1;
            self.move_right(1);
            if self.chars_right() > 0 {
                d = self.right_char(0) as u32;
            } else {
                break;
            }
        }
        char::from_u32(i & 0xFF).unwrap_or('\0')
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
