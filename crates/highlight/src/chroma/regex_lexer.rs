//! The regex lexer: rules, emitters, mutators, compilation and the state machine.
//!
//! Chroma's `regexp.go` (`RegexLexer`, `LexerState.Iterator`, `matchRules`), `emitters.go`,
//! `mutators.go` and `types.go` (a token type is an emitter), rewritten. Tokens are produced
//! eagerly; Chroma's lazy iterators give the same sequence (nested lexers are independent and
//! the emitters read only the match), a nested iterator ending at its first `EOF`
//! ([`until_eof`]).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::sync::{Arc, OnceLock};

use super::stack::Stack;
use super::{Config, Lexer, Registry, Token, TokeniseOptions, ensure_lf, until_eof};
use crate::regexp2::{Regex, Scratch};
use crate::token::TokenType;

mod state;
pub(crate) use state::*;

/// A lexer's states and their rules, before compilation.
pub(crate) type Rules = BTreeMap<String, Vec<Rule>>;

/// A rule: a pattern, what it emits and how it changes the state stack.
#[derive(Clone, Debug, Default)]
pub(crate) struct Rule {
    pub pattern: String,
    pub emitter: Option<Emitter>,
    pub mutator: Option<Mutator>,
}

/// A Go function standing in for a Chroma `EmitterFunc`.
pub(crate) type EmitFn = fn(&[String], &mut LexerState<'_>) -> Vec<Token>;
/// A Go function standing in for a Chroma `MutatorFunc`.
pub(crate) type MutateFn = fn(&mut LexerState<'_>);

/// What a matching rule emits.
#[derive(Clone)]
pub(crate) enum Emitter {
    /// One token of the whole match.
    Token(TokenType),
    /// One emitter per group (`None`: the group emits nothing).
    ByGroups(Vec<Option<Emitter>>),
    /// The match tokenised by another lexer, by name.
    Using(String),
    /// The match tokenised by this lexer from a state.
    UsingSelf(String),
    /// The `code_group` tokenised by the lexer named by `name_group`, the other groups by
    /// `emitters`.
    UsingByGroup {
        name_group: usize,
        code_group: usize,
        emitters: Vec<Emitter>,
    },
    /// A lexer's own emitter (Go code in Chroma).
    Func(&'static str, EmitFn),
}

impl fmt::Debug for Emitter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Token(t) => write!(f, "Token({t:?})"),
            Self::ByGroups(e) => f.debug_tuple("ByGroups").field(e).finish(),
            Self::Using(l) => write!(f, "Using({l:?})"),
            Self::UsingSelf(s) => write!(f, "UsingSelf({s:?})"),
            Self::UsingByGroup {
                name_group,
                code_group,
                emitters,
            } => write!(f, "UsingByGroup({name_group}, {code_group}, {emitters:?})"),
            Self::Func(name, _) => write!(f, "Func({name})"),
        }
    }
}

/// How a matching rule changes the state stack.
#[derive(Clone)]
pub(crate) enum Mutator {
    /// The rules of another state, in place of this rule (resolved at compile time).
    Include(String),
    /// Push a new state made of these states' rules (resolved at compile time).
    Combined(Vec<String>),
    /// Push states (`#pop` pops); none: push the current state again.
    Push(Vec<String>),
    Pop(usize),
    Multi(Vec<Mutator>),
    /// A lexer's own mutator (Go code in Chroma).
    Func(&'static str, MutateFn),
}

impl fmt::Debug for Mutator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Include(s) => write!(f, "Include({s:?})"),
            Self::Combined(s) => write!(f, "Combined({s:?})"),
            Self::Push(s) => write!(f, "Push({s:?})"),
            Self::Pop(n) => write!(f, "Pop({n})"),
            Self::Multi(m) => f.debug_tuple("Multi").field(m).finish(),
            Self::Func(name, _) => write!(f, "Func({name})"),
        }
    }
}

/// Where a lexer's rules come from.
enum Source {
    /// A lexer's definition (`lexers/`, `golexers/exported/`).
    Table(&'static super::defs::LexerDef),
    /// Rules built in Rust (Chroma's Go lexers).
    Code(fn() -> Rules),
}

/// How a lexer scores a text (`AnalyseText`).
enum Analyser {
    None,
    /// The `[config.analyse]` regexes of a lexer definition, compiled on first use.
    Regexes(crate::chroma::AnalyseConfig, OnceLock<Vec<Option<Regex>>>),
    Func(fn(&str) -> f32),
}

/// A rule ready to match.
struct CompiledRule {
    regex: Regex,
    emitter: Option<Emitter>,
    mutator: Option<Mutator>,
}

/// The compiled states, `include`s expanded and `combined` states added.
struct Compiled {
    states: HashMap<String, Vec<Arc<CompiledRule>>>,
}

/// A lexer defined by rules (Chroma's `RegexLexer`).
pub(crate) struct RegexLexer {
    config: Config,
    source: Source,
    analyser: Analyser,
    compiled: OnceLock<Result<Compiled, String>>,
}

impl fmt::Debug for RegexLexer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RegexLexer")
            .field("name", &self.config.name)
            .finish_non_exhaustive()
    }
}

impl RegexLexer {
    /// A lexer from its definition (Chroma's `NewXMLLexer`).
    pub fn from_table(def: &'static super::defs::LexerDef) -> Self {
        let config = def.config();
        let analyser = match &config.analyse {
            Some(a) => Analyser::Regexes(a.clone(), OnceLock::new()),
            None => Analyser::None,
        };
        Self {
            config,
            source: Source::Table(def),
            analyser,
            compiled: OnceLock::new(),
        }
    }

    /// A lexer with rules built in Rust (`MustNewLexer`).
    pub fn from_code(config: Config, rules: fn() -> Rules) -> Self {
        Self {
            config,
            source: Source::Code(rules),
            analyser: Analyser::None,
            compiled: OnceLock::new(),
        }
    }

    /// Replaces the configuration (`SetConfig`).
    #[must_use]
    pub fn with_config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }

    /// Replaces the analyser (`SetAnalyser`).
    #[must_use]
    pub fn with_analyser(mut self, f: fn(&str) -> f32) -> Self {
        self.analyser = Analyser::Func(f);
        self
    }

    /// The lexer's rules before compilation (`Rules`).
    pub fn rules(&self) -> Result<Rules, String> {
        match &self.source {
            Source::Table(def) => def.rules(),
            Source::Code(f) => Ok(f()),
        }
    }

    fn compiled(&self) -> &Result<Compiled, String> {
        self.compiled.get_or_init(|| self.compile())
    }

    /// Compiles the rules (`fetchRules` + `maybeCompile`): each pattern as
    /// `\G(?flags)(?:pattern)`, `include`s expanded, `combined` states created.
    fn compile(&self) -> Result<Compiled, String> {
        let raw = self.rules()?;
        if !raw.contains_key("root") {
            return Err(format!("{}: no \"root\" state", self.config.name));
        }
        let mut flags = String::new();
        if !self.config.not_multiline {
            flags.push('m');
        }
        if self.config.case_insensitive {
            flags.push('i');
        }
        if self.config.dot_all {
            flags.push('s');
        }

        /// A compiled rule, or an include still to expand.
        enum Slot {
            Rule(Arc<CompiledRule>),
            Include(String),
        }
        let mut combined: Vec<Vec<String>> = Vec::new();
        let mut slots: HashMap<String, Vec<Slot>> = HashMap::new();
        for (state, rules) in &raw {
            let mut out = Vec::with_capacity(rules.len());
            for (i, rule) in rules.iter().enumerate() {
                if let Some(Mutator::Include(s)) = &rule.mutator {
                    out.push(Slot::Include(s.clone()));
                    continue;
                }
                let pattern = if flags.is_empty() {
                    format!("\\G(?:{})", rule.pattern)
                } else {
                    format!("\\G(?{flags})(?:{})", rule.pattern)
                };
                let regex = Regex::new(&pattern).map_err(|e| {
                    format!(
                        "{}: failed to compile rule {state}.{i}: {e}",
                        self.config.name
                    )
                })?;
                let mutator = match &rule.mutator {
                    Some(Mutator::Combined(states)) => {
                        combined.push(states.clone());
                        Some(Mutator::Push(vec![combined_name(states)]))
                    }
                    m => m.clone(),
                };
                out.push(Slot::Rule(Arc::new(CompiledRule {
                    regex,
                    emitter: rule.emitter.clone(),
                    mutator,
                })));
            }
            slots.insert(state.clone(), out);
        }

        fn expand(
            state: &str,
            slots: &HashMap<String, Vec<Slot>>,
            done: &mut HashMap<String, Vec<Arc<CompiledRule>>>,
            visiting: &mut HashSet<String>,
        ) -> Result<Vec<Arc<CompiledRule>>, String> {
            if let Some(r) = done.get(state) {
                return Ok(r.clone());
            }
            let Some(list) = slots.get(state) else {
                return Err(format!("invalid include state {state:?}"));
            };
            if !visiting.insert(state.to_owned()) {
                return Ok(Vec::new());
            }
            let mut out = Vec::new();
            for slot in list {
                match slot {
                    Slot::Rule(r) => out.push(Arc::clone(r)),
                    Slot::Include(s) => out.extend(expand(s, slots, done, visiting)?),
                }
            }
            visiting.remove(state);
            done.insert(state.to_owned(), out.clone());
            Ok(out)
        }

        let mut states = HashMap::new();
        let names: Vec<String> = slots.keys().cloned().collect();
        for name in &names {
            let mut visiting = HashSet::new();
            expand(name, &slots, &mut states, &mut visiting)
                .map_err(|e| format!("{}: {e}", self.config.name))?;
        }
        for list in combined {
            let name = combined_name(&list);
            if states.contains_key(&name) {
                continue;
            }
            let mut rules = Vec::new();
            for s in &list {
                let Some(r) = states.get(s) else {
                    return Err(format!("{}: invalid combine state {s:?}", self.config.name));
                };
                rules.extend(r.iter().cloned());
            }
            states.insert(name, rules);
        }
        Ok(Compiled { states })
    }

    /// Why the lexer does not compile, if it does not (for tests).
    #[cfg(test)]
    pub fn compile_error(&self) -> Option<&str> {
        self.compiled().as_ref().err().map(String::as_str)
    }

    /// Tokenises `text` with this lexer (`RegexLexer.Tokenise`).
    pub fn tokenise_regex(
        &self,
        reg: &Registry,
        opts: Option<&TokeniseOptions>,
        text: &str,
    ) -> Vec<Token> {
        let Ok(compiled) = self.compiled() else {
            return vec![Token::new(TokenType::Error, text)];
        };
        let default;
        let opts = match opts {
            Some(o) => o,
            None => {
                default = TokeniseOptions {
                    state: "root".to_owned(),
                    nested: false,
                    ensure_lf: true,
                };
                &default
            }
        };
        let mut text = if opts.ensure_lf {
            ensure_lf(text)
        } else {
            text.to_owned()
        };
        let newline_added = !opts.nested && self.config.ensure_nl && !text.ends_with('\n');
        if newline_added {
            text.push('\n');
        }
        let mut st = LexerState {
            lexer: self,
            registry: reg,
            compiled,
            text: text.chars().collect(),
            pos: 0,
            stack: Stack::from(opts.state.clone()),
            state: String::new(),
            groups: Vec::new(),
            preproc: Stack::new(),
            start_state: opts.state.clone(),
            scratch: Scratch::default(),
        };
        st.run(newline_added)
    }
}

fn combined_name(states: &[String]) -> String {
    format!("__combined_{}", states.join("__"))
}

impl Lexer for RegexLexer {
    fn config(&self) -> &Config {
        &self.config
    }

    fn tokenise(&self, reg: &Registry, opts: Option<&TokeniseOptions>, text: &str) -> Vec<Token> {
        self.tokenise_regex(reg, opts, text)
    }

    fn analyse_text(&self, text: &str) -> f32 {
        match &self.analyser {
            Analyser::None => 0.0,
            Analyser::Func(f) => f(text),
            Analyser::Regexes(cfg, compiled) => {
                let regexes = compiled.get_or_init(|| {
                    cfg.regexes
                        .iter()
                        .map(|(p, _)| Regex::new(p).ok())
                        .collect()
                });
                let chars: Vec<char> = text.chars().collect();
                let mut score = 0.0f32;
                for (re, (_, s)) in regexes.iter().zip(&cfg.regexes) {
                    let Some(re) = re else { continue };
                    if re.is_match(&chars) {
                        if cfg.first {
                            return s.min(1.0);
                        }
                        score += s;
                    }
                }
                score.min(1.0)
            }
        }
    }
}
