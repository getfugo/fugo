//! The state machine of one tokenisation (Chroma's `LexerState`), with the emitters and mutators it
//! runs.

use super::*;

/// The state of one tokenisation (Chroma's `LexerState`).
pub(crate) struct LexerState<'a> {
    pub lexer: &'a RegexLexer,
    pub registry: &'a Registry,
    pub(super) compiled: &'a Compiled,
    pub text: Vec<char>,
    pub pos: usize,
    pub stack: Stack<String>,
    /// The state of the rule that matched.
    pub state: String,
    /// The match and its groups (`""` for a group that did not participate).
    pub groups: Vec<String>,
    /// Haxe's pre-processor stack (its `MutatorContext` entry).
    pub preproc: Stack<Stack<String>>,
    pub(super) start_state: String,
    pub(super) scratch: Scratch,
}

/// The zero-width matches at one position since the lexer arrived there (`LexerState::run`).
pub(super) struct Stall {
    pub(super) pos: usize,
    /// The state stack, Haxe's pre-processor stack and the output length before the first.
    pub(super) stack: Stack<String>,
    pub(super) preproc: Stack<Stack<String>>,
    pub(super) out_len: usize,
}

/// A configuration of the lexer at a position: its state stack and Haxe's pre-processor stack.
pub(super) type Configuration = (Stack<String>, Stack<Stack<String>>);

/// How many configurations zero-width matches may visit at one position: more means a stack
/// growing without bound (Chroma runs out of memory there).
pub(super) const MAX_STALL: usize = 1024;

impl LexerState<'_> {
    /// `LexerState.Iterator`, run to the end.
    ///
    /// Where zero-width matches bring the lexer back to a configuration it already had at the
    /// same position (or visit more than [`MAX_STALL`]), Chroma loops forever; the port undoes
    /// them and treats the position as matched by no rule (crate README, deviations). The
    /// guard costs O(1) per zero-width match: the stacks are persistent ([`Stack`]), so the
    /// configurations it keeps are shared, hashed and compared without walking them.
    pub(super) fn run(&mut self, newline_added: bool) -> Vec<Token> {
        let mut out = Vec::new();
        let compiled = self.compiled;
        let len = self.text.len();
        let end = if newline_added { len - 1 } else { len };
        let mut stall: Option<Stall> = None;
        // The configurations the zero-width matches of `stall` left (one set, cleared for
        // each position).
        let mut seen: HashSet<Configuration> = HashSet::new();
        while self.pos < end && !self.stack.is_empty() {
            self.state.clone_from(self.stack.last().expect("stack"));
            let Some(rules) = compiled.states.get(&self.state) else {
                // Chroma panics ("unknown state"): give up on the rest.
                out.push(Token::new(TokenType::Error, self.slice(self.pos, len)));
                return out;
            };
            let mut found = None;
            for rule in rules {
                if let Some(g) = rule.regex.find_at(&self.text, self.pos, &mut self.scratch) {
                    found = Some((Arc::clone(rule), g));
                    break;
                }
            }
            let Some((rule, groups)) = found else {
                self.no_match(&mut out);
                continue;
            };
            self.groups = groups
                .iter()
                .map(|g| g.map_or_else(String::new, |(a, b)| self.slice(a, b)))
                .collect();
            let (start, matched_end) = groups[0].unwrap_or((self.pos, self.pos));
            let zero_width = matched_end == start;
            if zero_width && stall.as_ref().is_none_or(|s| s.pos != self.pos) {
                stall = Some(Stall {
                    pos: self.pos,
                    stack: self.stack.clone(),
                    preproc: self.preproc.clone(),
                    out_len: out.len(),
                });
                seen.clear();
            }
            self.pos += matched_end - start;
            if let Some(m) = &rule.mutator {
                mutate(m, self);
            }
            if let Some(e) = &rule.emitter {
                let groups = std::mem::take(&mut self.groups);
                let mut tokens = Vec::new();
                emit(e, &groups, self, &mut tokens);
                self.groups = groups;
                // `Ignore` tokens are dropped.
                out.extend(tokens.into_iter().filter(|t| t.ty != TokenType::Ignore));
            }
            if zero_width && stall.is_some() {
                let config = (self.stack.clone(), self.preproc.clone());
                if !seen.insert(config) || seen.len() > MAX_STALL {
                    let s = stall.take().expect("stall");
                    self.stack = s.stack;
                    self.preproc = s.preproc;
                    out.truncate(s.out_len);
                    self.state.clone_from(self.stack.last().expect("stack"));
                    self.no_match(&mut out);
                }
            }
        }
        if self.pos != len && self.stack.is_empty() {
            out.push(Token::new(TokenType::Error, self.slice(self.pos, len)));
        }
        out
    }

    /// No rule matches at the position: an unmatched newline outside the start state resets
    /// the stack (Pygments), anything else is an `Error` token of one character.
    pub(super) fn no_match(&mut self, out: &mut Vec<Token>) {
        if self.text[self.pos] == '\n' && self.state != self.start_state {
            self.stack = Stack::from(self.start_state.clone());
            return;
        }
        self.pos += 1;
        out.push(Token::new(
            TokenType::Error,
            self.slice(self.pos - 1, self.pos),
        ));
    }

    pub(super) fn slice(&self, a: usize, b: usize) -> String {
        self.text[a..b].iter().collect()
    }
}

/// Emits the tokens of `e` for `groups` (`Emitter.Emit`).
pub(crate) fn emit(e: &Emitter, groups: &[String], st: &mut LexerState<'_>, out: &mut Vec<Token>) {
    match e {
        Emitter::Token(t) => out.push(Token::new(*t, groups[0].clone())),
        Emitter::ByGroups(emitters) => {
            if emitters.len() + 1 != groups.len() {
                out.push(Token::new(TokenType::Error, groups[0].clone()));
                return;
            }
            for (e, g) in emitters.iter().zip(&groups[1..]) {
                if let Some(e) = e {
                    emit(e, std::slice::from_ref(g), st, out);
                }
            }
        }
        Emitter::Using(name) => {
            let reg = st.registry;
            match reg.get(name) {
                Some(l) => out.extend(until_eof(l.tokenise(
                    reg,
                    Some(&TokeniseOptions::nested("root")),
                    &groups[0],
                ))),
                // Chroma panics ("no such lexer").
                None => out.push(Token::new(TokenType::Error, groups[0].clone())),
            }
        }
        Emitter::UsingSelf(state) => {
            let lexer = st.lexer;
            out.extend(until_eof(lexer.tokenise_regex(
                st.registry,
                Some(&TokeniseOptions::nested(state)),
                &groups[0],
            )));
        }
        Emitter::UsingByGroup {
            name_group,
            code_group,
            emitters,
        } => {
            if emitters.len() + 1 != groups.len() {
                // Chroma panics.
                out.push(Token::new(TokenType::Error, groups[0].clone()));
                return;
            }
            let reg = st.registry;
            let sub = groups.get(*name_group).and_then(|n| reg.get(n));
            for (i, g) in groups[1..].iter().enumerate() {
                match &sub {
                    Some(l) if i + 1 == *code_group => {
                        out.extend(until_eof(l.tokenise(reg, None, &groups[*code_group])));
                    }
                    _ => emit(&emitters[i], std::slice::from_ref(g), st, out),
                }
            }
        }
        Emitter::Func(_, f) => out.extend(f(groups, st)),
    }
}

/// Applies `m` to the state (`Mutator.Mutate`).
pub(crate) fn mutate(m: &Mutator, st: &mut LexerState<'_>) {
    match m {
        Mutator::Push(states) => {
            if states.is_empty() {
                st.stack.push(st.state.clone());
            } else {
                for s in states {
                    if s == "#pop" {
                        st.stack.pop();
                    } else {
                        st.stack.push(s.clone());
                    }
                }
            }
        }
        Mutator::Pop(depth) => {
            let n = st.stack.len().saturating_sub(*depth);
            st.stack.truncate(n);
        }
        Mutator::Multi(ms) => {
            for m in ms {
                mutate(m, st);
            }
        }
        Mutator::Func(_, f) => f(st),
        // Resolved at compile time; nested in `mutators` Chroma fails ("should never reach here").
        Mutator::Include(_) | Mutator::Combined(_) => {}
    }
}
