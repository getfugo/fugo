//! The template API as data: every filter, function and test a template may call
//! ([`FUNCS`]), the top-level names of every render context ([`CONTEXTS`], [`HOOK_FIELDS`]), the
//! Go-template constructs that became Tera syntax ([`SYNTAX`]), the conversion rules
//! ([`CONVERSION_RULES`]) and the embedded templates ([`EMBEDDED_TEMPLATES`]).
//!
//! This module is the single source of truth (REWRITE_PLAN.md §4.6): `docs/rust-port/template-api.md`
//! is generated from it by [`template_api_markdown`] and checked by ssg-testkit's contract
//! test, and `register_placeholders` registers a kwargs-checking stub for every entry. It depends
//! on nothing, so it is available without the crate's `runtime` feature.

use std::fmt::Write as _;

mod contexts;
mod funcs;
mod output;
mod syntax;

pub use contexts::*;
pub use funcs::*;
pub use output::*;
pub use syntax::*;

/// Whether a name is called as a filter (`x | name`), a function (`name()`) or a test (`x is name`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NameKind {
    Filter,
    Function,
    Test,
}

/// Who implements a name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Source {
    /// A Tera 2.4.0 built-in; never overridden.
    Builtin,
    /// tera-contrib 0.3, registered by `register_pure`.
    Contrib,
    /// This workspace: `ssg-funcs` when pure, `ssg-sitefuncs` when site-bound.
    Native,
}

/// The render phases in which a name is available (REWRITE_PLAN.md §4.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PhaseAvail {
    /// Shortcodes, render hooks, `markdownify` / `render_string` only.
    Content,
    /// Layout jobs, `partial()`, `defer` and `execute_as_template` only.
    Layout,
    /// Content adapters (`_content.html`) and the partials they call only.
    Adapter,
    Both,
}

/// The documented type of a keyword argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ArgType {
    Any,
    String,
    Int,
    Number,
    Bool,
    Array,
    Map,
    /// A page value (summary, link or full); read through `ssg_funcs::PageArg`.
    Page,
    /// A resource view; read through `ssg_funcs::ResourceArg`.
    Resource,
}

impl ArgType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Any => "any",
            Self::String => "string",
            Self::Int => "int",
            Self::Number => "number",
            Self::Bool => "bool",
            Self::Array => "array",
            Self::Map => "map",
            Self::Page => "page",
            Self::Resource => "resource",
        }
    }
}

/// Doc sections of `template-api.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Group {
    Logic,
    Collections,
    Pages,
    Strings,
    Encoding,
    Urls,
    Dates,
    Locale,
    Resources,
    Images,
    Templates,
    System,
    Tests,
}

impl Group {
    pub const ALL: [Self; 13] = [
        Self::Logic,
        Self::Collections,
        Self::Pages,
        Self::Strings,
        Self::Encoding,
        Self::Urls,
        Self::Dates,
        Self::Locale,
        Self::Resources,
        Self::Images,
        Self::Templates,
        Self::System,
        Self::Tests,
    ];

    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Logic => "Logic, math and errors",
            Self::Collections => "Collections and maps",
            Self::Pages => "Pages, taxonomies, menus and pagination",
            Self::Strings => "Strings",
            Self::Encoding => "Encoding, escaping and hashing",
            Self::Urls => "URLs and paths",
            Self::Dates => "Dates",
            Self::Locale => "Language",
            Self::Resources => "Resources and assets",
            Self::Images => "Images",
            Self::Templates => "Templates",
            Self::System => "Environment, files and debugging",
            Self::Tests => "Tests",
        }
    }
}

/// One keyword argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kwarg {
    pub name: &'static str,
    pub ty: ArgType,
    pub required: bool,
}

/// One filter, function or test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FuncSpec {
    pub name: &'static str,
    pub kind: NameKind,
    pub source: Source,
    pub kwargs: &'static [Kwarg],
    /// Accepts keyword arguments beyond `kwargs` (`partial(name=…, any=…)`).
    pub rest_kwargs: bool,
    pub phase: PhaseAvail,
    /// The output is marked safe (never autoescaped).
    pub safe: bool,
    /// Needs the site model or the render scope `__nh` (ssg-sitefuncs).
    pub site_bound: bool,
    pub group: Group,
    /// The Go-template functions or methods this replaces ("" when it has no Go counterpart).
    pub go: &'static str,
    pub doc: &'static str,
}

impl FuncSpec {
    /// The kind code of REWRITE_PLAN.md §4.6: `bi`, `tc`, `F`, `fn` or `T`, plus ` (s)` when site-bound.
    #[must_use]
    pub fn code(&self) -> String {
        let base = match (self.source, self.kind) {
            (Source::Builtin, _) => "bi",
            (Source::Contrib, _) => "tc",
            (Source::Native, NameKind::Filter) => "F",
            (Source::Native, NameKind::Function) => "fn",
            (Source::Native, NameKind::Test) => "T",
        };
        if self.site_bound {
            format!("{base} (s)")
        } else {
            base.to_owned()
        }
    }

    /// The keyword argument called `name`, if declared.
    #[must_use]
    pub fn kwarg(&self, name: &str) -> Option<&'static Kwarg> {
        self.kwargs.iter().find(|k| k.name == name)
    }

    /// The call signature as written in a template, e.g. `x | sort_by(attribute=, reverse=?)`.
    #[must_use]
    pub fn signature(&self) -> String {
        let mut args: Vec<String> = self
            .kwargs
            .iter()
            .map(|k| {
                if k.required {
                    format!("{}=", k.name)
                } else {
                    format!("{}=?", k.name)
                }
            })
            .collect();
        if self.rest_kwargs {
            args.push("…".to_owned());
        }
        let parens = if args.is_empty() && self.kind != NameKind::Function {
            String::new()
        } else {
            format!("({})", args.join(", "))
        };
        match self.kind {
            NameKind::Filter => format!("x | {}{parens}", self.name),
            NameKind::Function => format!("{}{parens}", self.name),
            NameKind::Test => format!("x is {}{parens}", self.name),
        }
    }

    const fn new(
        kind: NameKind,
        group: Group,
        name: &'static str,
        go: &'static str,
        doc: &'static str,
    ) -> Self {
        Self {
            name,
            kind,
            source: Source::Native,
            kwargs: &[],
            rest_kwargs: false,
            phase: PhaseAvail::Both,
            safe: false,
            site_bound: false,
            group,
            go,
            doc,
        }
    }
    const fn args(mut self, kwargs: &'static [Kwarg]) -> Self {
        self.kwargs = kwargs;
        self
    }
    const fn rest(mut self) -> Self {
        self.rest_kwargs = true;
        self
    }
    const fn builtin(mut self) -> Self {
        self.source = Source::Builtin;
        self
    }
    const fn contrib(mut self) -> Self {
        self.source = Source::Contrib;
        self
    }
    const fn site(mut self) -> Self {
        self.site_bound = true;
        self
    }
    const fn safe(mut self) -> Self {
        self.safe = true;
        self
    }
    const fn only(mut self, phase: PhaseAvail) -> Self {
        self.phase = phase;
        self
    }
}
