//! The pure filters, functions and tests (every `spec::FUNCS` entry that is neither a Tera
//! built-in nor site-bound) and the tera-contrib subset.
//!
//! "Pure" means: no site model and no render scope. A pure entry may still read the render
//! context's `lang` (the page language: collation for `sort_by`, names for `date`, separators
//! for `format_number`) and the build-wide [`PureEnv`].

mod collections;
mod dates;
mod encoding;
#[cfg(feature = "goat")]
mod goat;
mod html;
#[cfg(feature = "math")]
mod katex;
mod marshal;
#[cfg(feature = "math")]
mod math;
mod strings;
mod system;
mod urls;
mod value;
mod views;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use ssg_base::diag::Diagnostics;
use ssg_base::url::{Accents, PathCase};
use ssg_base::{Clock, anchor, title};
use ssg_locale::Locale;
use tera::{Filter, Function, Kwargs, State, Tera, TeraResult, Test, Value};

use crate::check_kwargs;
use crate::spec::{self, FuncSpec, NameKind, Source};

pub use dates::date_value;

/// Pure entries whose implementation is not compiled into this build: `to_math` without the
/// `math` feature, `diagrams_goat` without the `goat` feature. [`register_pure`] registers
/// them as stubs that fail when called, so templates that name them still load.
pub const NOT_COMPILED: &[&str] = &[
    #[cfg(not(feature = "math"))]
    "to_math",
    #[cfg(not(feature = "goat"))]
    "diagrams_goat",
];

/// The `spec::FUNCS` entries that [`register_pure`] registers: neither Tera built-ins nor
/// site-bound.
pub fn pure_specs() -> impl Iterator<Item = &'static FuncSpec> {
    spec::FUNCS
        .iter()
        .filter(|f| f.source != Source::Builtin && !f.site_bound)
}

/// The locales of the site languages, built once (ICU data) and shared.
#[derive(Clone)]
pub struct Locales {
    default: Arc<Locale>,
    by_key: BTreeMap<String, Arc<Locale>>,
}

impl Locales {
    /// The locales of `default` (used when a render has no `lang`) and `others`.
    #[must_use]
    pub fn new<'a>(default: &str, others: impl IntoIterator<Item = &'a str>) -> Self {
        let default = Arc::new(Locale::new(default));
        let mut by_key = BTreeMap::new();
        by_key.insert(default.key().to_owned(), Arc::clone(&default));
        for key in others {
            let locale = Locale::new(key);
            by_key
                .entry(locale.key().to_owned())
                .or_insert_with(|| Arc::new(locale));
        }
        Self { default, by_key }
    }

    /// The locale of language key `key` (built on the fly for a key that is not a site
    /// language, e.g. an explicit `locale=`).
    #[must_use]
    pub fn get(&self, key: &str) -> Arc<Locale> {
        let key = key.to_ascii_lowercase();
        self.by_key
            .get(&key)
            .cloned()
            .unwrap_or_else(|| Arc::new(Locale::new(&key)))
    }

    /// The locale of the render's `lang`, else the default language's.
    #[must_use]
    pub fn current(&self, state: &State) -> Arc<Locale> {
        match state.get::<Value>("lang") {
            Ok(Some(lang)) if lang.as_str().is_some_and(|l| !l.is_empty()) => {
                self.get(lang.as_str().unwrap_or_default())
            }
            _ => Arc::clone(&self.default),
        }
    }
}

/// The environment variables `get_env` may read (`security.funcs.getenv`: regular expressions).
#[derive(Clone, Debug)]
pub struct EnvAllowlist(Vec<regex::Regex>);

impl EnvAllowlist {
    /// Compiles the patterns.
    ///
    /// # Errors
    /// A pattern that is not a regular expression.
    pub fn new<I, S>(patterns: I) -> Result<Self, regex::Error>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        patterns
            .into_iter()
            .map(|p| regex::Regex::new(p.as_ref()))
            .collect::<Result<_, _>>()
            .map(Self)
    }

    /// Whether `name` may be read.
    #[must_use]
    pub fn allows(&self, name: &str) -> bool {
        self.0.iter().any(|r| r.is_match(name))
    }
}

impl Default for EnvAllowlist {
    /// The default policy: `^FUGO_` and `^CI$` (the Go program's environment variables are not
    /// allowed).
    fn default() -> Self {
        Self::new([concat!("^", ssg_base::env_var!("")), "^CI$"]).expect("valid patterns")
    }
}

/// What the pure functions need from the build: one per build, shared by every render.
#[derive(Clone)]
pub struct PureEnv {
    /// `now()` (`--clock`).
    pub clock: Clock,
    /// The site time zone (`timeZone`): `now()`, and date strings without an offset.
    pub time_zone: jiff::tz::TimeZone,
    /// The site languages' locales.
    pub locales: Locales,
    /// `titleCaseStyle`, the default of `title_case(style=)`.
    pub title_style: title::Style,
    /// `markup.goldmark.parser.autoHeadingIDType`, the default of `anchorize(style=)`.
    pub anchor_style: anchor::Style,
    /// `disablePathToLower` and `removePathAccents`, for `urlize`.
    pub path_case: PathCase,
    pub accents: Accents,
    /// Where `log_error` and `log_warn` record (without a position: a pure function does not
    /// know the template; sitefuncs may re-register both with the render position).
    pub diagnostics: Arc<Diagnostics>,
    /// The project directory `read_file` and `file_exists` resolve against (`None`: both fail).
    pub project_dir: Option<PathBuf>,
    /// `security.funcs.getenv`.
    pub getenv: EnvAllowlist,
    /// The project's `.env` variables: `get_env` reads these names whatever `getenv` allows
    /// (the process environment wins for the same name).
    pub env_file: Arc<BTreeMap<String, String>>,
}

impl PureEnv {
    /// Defaults for a site whose only language is `default_language`: system clock, UTC, AP
    /// title case, GitHub anchors, lower-cased paths with accents kept, the default getenv policy,
    /// no project directory.
    #[must_use]
    pub fn new(default_language: &str) -> Self {
        Self {
            clock: Clock::system(),
            time_zone: jiff::tz::TimeZone::UTC,
            locales: Locales::new(default_language, []),
            title_style: title::Style::default(),
            anchor_style: anchor::Style::default(),
            path_case: PathCase::default(),
            accents: Accents::default(),
            diagnostics: Arc::new(Diagnostics::default()),
            project_dir: None,
            getenv: EnvAllowlist::default(),
            env_file: Arc::default(),
        }
    }
}

/// Registers every pure `spec::FUNCS` entry and the tera-contrib subset on `tera`. Call it
/// before adding templates (Tera validates names when a template is added).
///
/// Every registered name checks its kwargs against the spec (an unknown or a missing required
/// kwarg is an error naming the expected signature).
pub fn register_pure(tera: &mut Tera, env: &Arc<PureEnv>) {
    let mut r = Registrar { tera };
    collections::register(&mut r, env);
    strings::register(&mut r, env);
    html::register(&mut r);
    encoding::register(&mut r);
    urls::register(&mut r);
    dates::register(&mut r, env);
    system::register(&mut r, env);
    views::register(&mut r);
    #[cfg(feature = "math")]
    math::register(&mut r, env);
    #[cfg(feature = "goat")]
    goat::register(&mut r);
    for name in NOT_COMPILED {
        r.not_compiled(name);
    }
    contrib(&mut r);
}

/// The tera-contrib names of the spec, one by one (its `regex` module also has `striptags` and
/// `spaceless`, which this port does not use).
fn contrib(r: &mut Registrar<'_>) {
    use tera_contrib::{base64, filesize_format, regex, urlencode};
    r.raw_filter("regex_replace", regex::RegexReplace::default());
    r.tera.register_test(
        "matching",
        Checked::new("matching", regex::Matching::default()),
    );
    r.raw_filter("urlencode", urlencode::urlencode);
    r.raw_filter("urlencode_strict", urlencode::urlencode_strict);
    r.raw_filter("b64_encode", base64::b64_encode);
    r.raw_filter("b64_decode", base64::b64_decode);
    r.raw_filter("filesize_format", filesize_format::filesize_format);
}

/// Looks up the spec entry `name` of `kind`.
///
/// # Panics
/// When the spec has no such entry (a programming error; the registration test catches it).
fn spec_of(name: &str, kind: NameKind) -> &'static FuncSpec {
    spec::FUNCS
        .iter()
        .find(|f| f.name == name && f.kind == kind && f.source != Source::Builtin)
        .unwrap_or_else(|| panic!("`{name}` is not a non-built-in {kind:?} of spec::FUNCS"))
}

fn check(spec: &FuncSpec, kwargs: &Kwargs) -> TeraResult<()> {
    check_kwargs(spec, kwargs.iter().map(|(k, _)| k.as_str().unwrap_or("")))
        .map_err(tera::Error::message)
}

/// A registered implementation with its spec entry: checks the kwargs, and takes its safety
/// from the spec.
struct Checked<F> {
    spec: &'static FuncSpec,
    f: F,
}

impl<F> Checked<F> {
    fn new(name: &str, f: F) -> Self {
        let spec = spec::FUNCS
            .iter()
            .find(|s| s.name == name && s.source != Source::Builtin)
            .unwrap_or_else(|| panic!("`{name}` is not in spec::FUNCS"));
        Self { spec, f }
    }
}

impl<F> Filter<Value, TeraResult<Value>> for Checked<F>
where
    F: Fn(Value, &Kwargs, &State) -> TeraResult<Value> + Send + Sync + 'static,
{
    fn call(&self, value: Value, kwargs: Kwargs, state: &State) -> TeraResult<Value> {
        check(self.spec, &kwargs)?;
        (self.f)(value, &kwargs, state)
    }

    fn is_safe(&self) -> bool {
        self.spec.safe
    }
}

/// A function: kwargs and state.
struct Func<F>(Checked<F>);

impl<F> Function<TeraResult<Value>> for Func<F>
where
    F: Fn(&Kwargs, &State) -> TeraResult<Value> + Send + Sync + 'static,
{
    fn call(&self, kwargs: Kwargs, state: &State) -> TeraResult<Value> {
        check(self.0.spec, &kwargs)?;
        (self.0.f)(&kwargs, state)
    }

    fn is_safe(&self) -> bool {
        self.0.spec.safe
    }
}

/// A test.
struct Predicate<F>(Checked<F>);

impl<F> Test<Value, TeraResult<bool>> for Predicate<F>
where
    F: Fn(Value, &Kwargs, &State) -> TeraResult<bool> + Send + Sync + 'static,
{
    fn call(&self, value: Value, kwargs: Kwargs, state: &State) -> TeraResult<bool> {
        check(self.0.spec, &kwargs)?;
        (self.0.f)(value, &kwargs, state)
    }
}

/// A tera-contrib test, kwargs checked.
impl<'a> Test<&'a str, TeraResult<bool>> for Checked<tera_contrib::regex::Matching> {
    fn call(&self, value: &'a str, kwargs: Kwargs, state: &State) -> TeraResult<bool> {
        check(self.spec, &kwargs)?;
        Test::call(&self.f, value, kwargs, state)
    }
}

/// Registers implementations under their spec names.
struct Registrar<'t> {
    tera: &'t mut Tera,
}

impl Registrar<'_> {
    fn filter<F>(&mut self, name: &'static str, f: F)
    where
        F: Fn(Value, &Kwargs, &State) -> TeraResult<Value> + Send + Sync + 'static,
    {
        let spec = spec_of(name, NameKind::Filter);
        self.tera.register_filter(name, Checked { spec, f });
    }

    fn function<F>(&mut self, name: &'static str, f: F)
    where
        F: Fn(&Kwargs, &State) -> TeraResult<Value> + Send + Sync + 'static,
    {
        let spec = spec_of(name, NameKind::Function);
        self.tera.register_function(name, Func(Checked { spec, f }));
    }

    fn test<F>(&mut self, name: &'static str, f: F)
    where
        F: Fn(Value, &Kwargs, &State) -> TeraResult<bool> + Send + Sync + 'static,
    {
        let spec = spec_of(name, NameKind::Test);
        self.tera
            .register_test(name, Predicate(Checked { spec, f }));
    }

    /// A tera-contrib filter under its own name (its kwargs are checked by tera-contrib).
    fn raw_filter<Arg, Res, F>(&mut self, name: &'static str, f: F)
    where
        F: Filter<Arg, Res> + for<'a> Filter<<Arg as tera::ArgFromValue<'a>>::Output, Res>,
        Arg: for<'a> tera::ArgFromValue<'a>,
        Res: tera::value::FunctionResult,
    {
        let _ = spec_of(name, NameKind::Filter);
        self.tera.register_filter(name, f);
    }

    /// A stub for an entry whose implementation is not compiled in.
    fn not_compiled(&mut self, name: &'static str) {
        let spec = spec::FUNCS
            .iter()
            .find(|s| s.name == name && s.source != Source::Builtin)
            .unwrap_or_else(|| panic!("`{name}` is not in spec::FUNCS"));
        let message = move || {
            tera::Error::message(format!(
                "`{name}` is not available in this build (feature not compiled in)"
            ))
        };
        match spec.kind {
            NameKind::Filter => self.filter(name, move |_, _, _| Err(message())),
            NameKind::Function => self.function(name, move |_, _| Err(message())),
            NameKind::Test => self.test(name, move |_, _, _| Err(message())),
        }
    }
}
