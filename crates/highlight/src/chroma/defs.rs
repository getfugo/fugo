//! The lexer definitions: Chroma's lexers as Rust data (crate README, "Lexer and style
//! files"). [`super::lexers`] holds its XML lexers and [`super::golexers`]'s `exported` the
//! configuration and rules of its lexers written in Go, both converted from Chroma's XML by
//! `tests/it/xml2rust.rs`; a rule is written with the builders below
//! (`rule(r"\s+").token(T::Text)`). A definition becomes a lexer's [`Config`] when it is
//! registered and its [`Rules`] when it is first used.

use super::regex_lexer::{Emitter, Mutator, Rule, Rules};
use super::{AnalyseConfig, Config};
use crate::token::TokenType;

/// What the lexer files use.
pub(crate) mod prelude {
    pub(crate) use super::{
        AnalyseDef, ConfigDef, EmitterDef as E, LexerDef, MutatorDef as M, include, rule,
    };
    pub(crate) use crate::token::TokenType as T;
}

/// A lexer: Chroma's XML `<lexer>`.
pub(crate) struct LexerDef {
    /// Chroma's file name, without `.xml` (`c#`).
    pub file: &'static str,
    pub config: ConfigDef,
    /// The states and their rules, in the file's order.
    pub states: &'static [(&'static str, &'static [RuleDef])],
}

/// Chroma's `Config` (`<config>`).
#[expect(
    clippy::struct_excessive_bools,
    reason = "Chroma's `Config` fields, as in its lexer files"
)]
pub(crate) struct ConfigDef {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub filenames: &'static [&'static str],
    pub alias_filenames: &'static [&'static str],
    pub mime_types: &'static [&'static str],
    pub case_insensitive: bool,
    pub dot_all: bool,
    pub not_multiline: bool,
    pub ensure_nl: bool,
    /// 0 counts as 1.
    pub priority: f32,
    pub analyse: Option<AnalyseDef>,
}

impl ConfigDef {
    /// No name, nothing set: what `..ConfigDef::EMPTY` fills in.
    pub const EMPTY: Self = Self {
        name: "",
        aliases: &[],
        filenames: &[],
        alias_filenames: &[],
        mime_types: &[],
        case_insensitive: false,
        dot_all: false,
        not_multiline: false,
        ensure_nl: false,
        priority: 0.0,
        analyse: None,
    };
}

/// Content analysis by regular expressions (`<analyse>`): `(pattern, score)`.
pub(crate) struct AnalyseDef {
    /// The first matching regex decides, instead of the sum.
    pub first: bool,
    pub regexes: &'static [(&'static str, f32)],
}

/// A rule: a pattern (`""`: the empty pattern), at most one emitter and one mutator.
pub(crate) struct RuleDef {
    pub pattern: &'static str,
    pub emitter: Option<EmitterDef>,
    pub mutator: Option<MutatorDef>,
}

/// An [`Emitter`]; a function by its name.
pub(crate) enum EmitterDef {
    Token(TokenType),
    /// One token type per group (`bygroups` of token types only).
    Groups(&'static [TokenType]),
    /// One emitter per group, [`EmitterDef::Nil`] for a group that emits nothing.
    ByGroups(&'static [EmitterDef]),
    Using(&'static str),
    UsingSelf(&'static str),
    UsingByGroup {
        name_group: usize,
        code_group: usize,
        emitters: &'static [EmitterDef],
    },
    /// A lexer's own emitter, Go code in Chroma (`golexers.rs`).
    Func(&'static str),
    /// Go's `nil` emitter, in [`EmitterDef::ByGroups`] only.
    Nil,
}

/// A [`Mutator`]; a function by its name.
pub(crate) enum MutatorDef {
    Include(&'static str),
    Combined(&'static [&'static str]),
    /// States to push (`#pop` pops); none: the current state again.
    Push(&'static [&'static str]),
    Pop(usize),
    Multi(&'static [MutatorDef]),
    /// A lexer's own mutator, Go code in Chroma (`golexers.rs`).
    Func(&'static str),
}

/// A rule matching `pattern`, emitting nothing and keeping the state.
pub(crate) const fn rule(pattern: &'static str) -> RuleDef {
    RuleDef {
        pattern,
        emitter: None,
        mutator: None,
    }
}

/// The rules of `state`, in place of this rule.
pub(crate) const fn include(state: &'static str) -> RuleDef {
    rule("").include(state)
}

/// The builders: each sets the rule's emitter or its mutator, once (a second one does not
/// compile: the panic is evaluated with the table).
impl RuleDef {
    const fn emit(mut self, e: EmitterDef) -> Self {
        assert!(self.emitter.is_none(), "a rule with a second emitter");
        self.emitter = Some(e);
        self
    }

    const fn mutate(mut self, m: MutatorDef) -> Self {
        assert!(self.mutator.is_none(), "a rule with a second mutator");
        self.mutator = Some(m);
        self
    }

    pub const fn token(self, t: TokenType) -> Self {
        self.emit(EmitterDef::Token(t))
    }

    pub const fn groups(self, types: &'static [TokenType]) -> Self {
        self.emit(EmitterDef::Groups(types))
    }

    pub const fn bygroups(self, emitters: &'static [EmitterDef]) -> Self {
        self.emit(EmitterDef::ByGroups(emitters))
    }

    pub const fn using(self, lexer: &'static str) -> Self {
        self.emit(EmitterDef::Using(lexer))
    }

    pub const fn using_self(self, state: &'static str) -> Self {
        self.emit(EmitterDef::UsingSelf(state))
    }

    pub const fn using_by_group(
        self,
        name_group: usize,
        code_group: usize,
        emitters: &'static [EmitterDef],
    ) -> Self {
        self.emit(EmitterDef::UsingByGroup {
            name_group,
            code_group,
            emitters,
        })
    }

    pub const fn emit_func(self, name: &'static str) -> Self {
        self.emit(EmitterDef::Func(name))
    }

    pub const fn include(self, state: &'static str) -> Self {
        self.mutate(MutatorDef::Include(state))
    }

    pub const fn combined(self, states: &'static [&'static str]) -> Self {
        self.mutate(MutatorDef::Combined(states))
    }

    pub const fn push(self, states: &'static [&'static str]) -> Self {
        self.mutate(MutatorDef::Push(states))
    }

    pub const fn pop(self, depth: usize) -> Self {
        self.mutate(MutatorDef::Pop(depth))
    }

    pub const fn mutators(self, list: &'static [MutatorDef]) -> Self {
        self.mutate(MutatorDef::Multi(list))
    }

    pub const fn mutator_func(self, name: &'static str) -> Self {
        self.mutate(MutatorDef::Func(name))
    }
}

/// The lexer of Chroma's `lexers/embedded/<file>.xml`.
pub(crate) fn lexer(file: &str) -> Option<&'static LexerDef> {
    super::lexers::LEXERS
        .iter()
        .copied()
        .find(|l| l.file == file)
}

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

impl LexerDef {
    /// The lexer's configuration.
    pub fn config(&self) -> Config {
        let c = &self.config;
        Config {
            name: c.name.to_owned(),
            aliases: strings(c.aliases),
            filenames: strings(c.filenames),
            alias_filenames: strings(c.alias_filenames),
            mime_types: strings(c.mime_types),
            case_insensitive: c.case_insensitive,
            dot_all: c.dot_all,
            not_multiline: c.not_multiline,
            ensure_nl: c.ensure_nl,
            priority: c.priority,
            analyse: c.analyse.as_ref().map(|a| AnalyseConfig {
                regexes: a
                    .regexes
                    .iter()
                    .map(|(p, s)| ((*p).to_owned(), *s))
                    .collect(),
                first: a.first,
            }),
        }
    }

    /// The lexer's rules (a state defined twice keeps its last definition); a function no Go
    /// lexer port defines, or a `nil` emitter outside `bygroups`, is an error.
    pub fn rules(&self) -> Result<Rules, String> {
        let mut out = Rules::new();
        for (state, rules) in self.states {
            let list = rules
                .iter()
                .map(|r| {
                    Ok(Rule {
                        pattern: r.pattern.to_owned(),
                        emitter: r.emitter.as_ref().map(emitter).transpose()?,
                        mutator: r.mutator.as_ref().map(mutator).transpose()?,
                    })
                })
                .collect::<Result<_, String>>()?;
            out.insert((*state).to_owned(), list);
        }
        Ok(out)
    }
}

fn emitter(e: &EmitterDef) -> Result<Emitter, String> {
    Ok(match e {
        EmitterDef::Token(t) => Emitter::Token(*t),
        EmitterDef::Groups(types) => {
            Emitter::ByGroups(types.iter().map(|t| Some(Emitter::Token(*t))).collect())
        }
        EmitterDef::ByGroups(groups) => Emitter::ByGroups(
            groups
                .iter()
                .map(|g| match g {
                    EmitterDef::Nil => Ok(None),
                    g => emitter(g).map(Some),
                })
                .collect::<Result<_, _>>()?,
        ),
        EmitterDef::Using(lexer) => Emitter::Using((*lexer).to_owned()),
        EmitterDef::UsingSelf(state) => Emitter::UsingSelf((*state).to_owned()),
        EmitterDef::UsingByGroup {
            name_group,
            code_group,
            emitters,
        } => Emitter::UsingByGroup {
            name_group: *name_group,
            code_group: *code_group,
            emitters: emitters.iter().map(emitter).collect::<Result<_, _>>()?,
        },
        EmitterDef::Func(name) => {
            let f = super::golexers::emitter_func(name)
                .ok_or_else(|| format!("unknown emitter function {name:?}"))?;
            Emitter::Func(f.0, f.1)
        }
        EmitterDef::Nil => return Err("a nil emitter outside bygroups".into()),
    })
}

fn mutator(m: &MutatorDef) -> Result<Mutator, String> {
    Ok(match m {
        MutatorDef::Include(state) => Mutator::Include((*state).to_owned()),
        MutatorDef::Combined(states) => Mutator::Combined(strings(states)),
        MutatorDef::Push(states) => Mutator::Push(strings(states)),
        MutatorDef::Pop(depth) => Mutator::Pop(*depth),
        MutatorDef::Multi(list) => {
            Mutator::Multi(list.iter().map(mutator).collect::<Result<_, _>>()?)
        }
        MutatorDef::Func(name) => {
            let f = super::golexers::mutator_func(name)
                .ok_or_else(|| format!("unknown mutator function {name:?}"))?;
            Mutator::Func(f.0, f.1)
        }
    })
}
