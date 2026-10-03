//! Placeholder registrations for every [`spec::FUNCS`] entry that Tera does not provide itself,
//! for the contract instance (ssg-testkit) and `templates check` (REWRITE_PLAN.md §4.1, §4.8).
//!
//! A placeholder checks its kwargs against the spec when called (unknown, or a required one
//! missing) and otherwise does nothing: a filter returns its input, a function none, a test false.

use tera::{Filter, Function, Kwargs, State, Tera, TeraResult, Test, Value};

use crate::spec::{self, FuncSpec, NameKind, Source};

/// Registers a placeholder for every non-built-in [`spec::FUNCS`] entry (tera-contrib ones
/// included). Tera built-ins are never overridden.
pub fn register_placeholders(tera: &mut Tera) {
    for spec in spec::FUNCS.iter().filter(|f| f.source != Source::Builtin) {
        let stub = Placeholder(spec);
        match spec.kind {
            NameKind::Filter => tera.register_filter(spec.name, stub),
            NameKind::Function => tera.register_function(spec.name, stub),
            NameKind::Test => tera.register_test(spec.name, stub),
        }
    }
}

/// Checks call kwargs against a spec entry: every kwarg must be declared (unless the entry takes
/// further kwargs) and every required one present.
///
/// # Errors
/// A message naming the offending kwarg and the call.
pub fn check_kwargs<'a>(
    spec: &FuncSpec,
    names: impl IntoIterator<Item = &'a str>,
) -> Result<(), String> {
    let names: Vec<&str> = names.into_iter().collect();
    let unknown = names.iter().find(|n| spec.kwarg(n).is_none());
    if let (false, Some(unknown)) = (spec.rest_kwargs, unknown) {
        return Err(format!(
            "unknown kwarg `{unknown}` for `{}`; expected `{}`",
            spec.name,
            spec.signature()
        ));
    }
    if let Some(missing) = spec
        .kwargs
        .iter()
        .find(|k| k.required && !names.contains(&k.name))
    {
        return Err(format!(
            "missing required kwarg `{}` for `{}`; expected `{}`",
            missing.name,
            spec.name,
            spec.signature()
        ));
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct Placeholder(&'static FuncSpec);

impl Placeholder {
    fn check(self, kwargs: &Kwargs) -> TeraResult<()> {
        let names: Vec<&str> = kwargs
            .iter()
            .map(|(k, _)| k.as_str().unwrap_or(""))
            .collect();
        check_kwargs(self.0, names).map_err(tera::Error::message)
    }
}

impl Filter<Value, TeraResult<Value>> for Placeholder {
    fn call(&self, value: Value, kwargs: Kwargs, _: &State) -> TeraResult<Value> {
        self.check(&kwargs)?;
        Ok(value)
    }

    fn is_safe(&self) -> bool {
        self.0.safe
    }
}

impl Function<TeraResult<Value>> for Placeholder {
    fn call(&self, kwargs: Kwargs, _: &State) -> TeraResult<Value> {
        self.check(&kwargs)?;
        Ok(Value::none())
    }

    fn is_safe(&self) -> bool {
        self.0.safe
    }
}

impl Test<Value, TeraResult<bool>> for Placeholder {
    fn call(&self, _: Value, kwargs: Kwargs, _: &State) -> TeraResult<bool> {
        self.check(&kwargs)?;
        Ok(false)
    }
}
