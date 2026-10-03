//! Registration with spec checks, and what every call needs: the render scope, page and
//! resource arguments, value conversions, errors.

use std::fmt::Display;
use std::sync::{Arc, OnceLock, Weak};

use ssg_base::{FormatId, Idx, LangIdx, PageId, ResourceId};
use ssg_funcs::check_kwargs;
use ssg_funcs::spec::{self, FuncSpec, NameKind};
use ssg_layouts::Templates;
use ssg_resources::ResourceStore;
use ssg_site::Model;
use ssg_view::{ContentRenderer, RenderScope, SCOPE_KEY, ViewCache, ViewGeneration};
use tera::value::Key;
use tera::{ArgFromValue, ErrorKind, Kwargs, State, Tera, TeraResult, Value};

/// A site-bound function: kwargs and the render state.
pub(crate) trait SiteFunction: Send + Sync + 'static {
    fn call(&self, kw: &Kwargs, st: &State) -> TeraResult<Value>;
}

/// A site-bound filter: the input, kwargs and the render state.
pub(crate) trait SiteFilter: Send + Sync + 'static {
    fn call(&self, v: Value, kw: &Kwargs, st: &State) -> TeraResult<Value>;
}

/// A registered implementation with its spec entry: checks the kwargs, and takes its safety
/// from the spec.
struct Checked<T> {
    spec: &'static FuncSpec,
    f: T,
}

impl<T> Checked<T> {
    fn check(&self, kw: &Kwargs) -> TeraResult<()> {
        check_kwargs(self.spec, kw.iter().map(|(k, _)| k.as_str().unwrap_or("")))
            .map_err(tera::Error::message)
    }
}

impl<T: SiteFunction> tera::Function<TeraResult<Value>> for Checked<T> {
    fn call(&self, kwargs: Kwargs, state: &State) -> TeraResult<Value> {
        self.check(&kwargs)?;
        self.f
            .call(&kwargs, state)
            .map_err(|e| named(self.spec.name, e))
    }

    fn is_safe(&self) -> bool {
        self.spec.safe
    }
}

/// A filter (the wrapper is a separate type so that one struct can be both).
struct CheckedFilter<T>(Checked<T>);

impl<T: SiteFilter> tera::Filter<Value, TeraResult<Value>> for CheckedFilter<T> {
    fn call(&self, value: Value, kwargs: Kwargs, state: &State) -> TeraResult<Value> {
        self.0.check(&kwargs)?;
        self.0
            .f
            .call(value, &kwargs, state)
            .map_err(|e| named(self.0.spec.name, e))
    }

    fn is_safe(&self) -> bool {
        self.0.spec.safe
    }
}

/// Registers implementations under their spec names.
pub(crate) struct Registrar<'t> {
    tera: &'t mut Tera,
}

/// The site-bound spec entry `name` of `kind`.
///
/// # Panics
/// When the spec has no such entry (a programming error; the registration test catches it).
fn spec_of(name: &str, kind: NameKind) -> &'static FuncSpec {
    spec::FUNCS
        .iter()
        .find(|f| f.name == name && f.kind == kind && f.site_bound)
        .unwrap_or_else(|| panic!("`{name}` is not a site-bound {kind:?} of spec::FUNCS"))
}

impl<'t> Registrar<'t> {
    pub(crate) fn new(tera: &'t mut Tera) -> Self {
        Self { tera }
    }

    pub(crate) fn function(&mut self, name: &'static str, f: impl SiteFunction) {
        let spec = spec_of(name, NameKind::Function);
        self.tera.register_function(name, Checked { spec, f });
    }

    pub(crate) fn filter(&mut self, name: &'static str, f: impl SiteFilter) {
        let spec = spec_of(name, NameKind::Filter);
        self.tera
            .register_filter(name, CheckedFilter(Checked { spec, f }));
    }
}

// ── errors ──────────────────────────────────────────────────────────────────────────────────

/// An error with a cause: the cause's text is in the message (Tera reports only the message).
pub(crate) fn chain<E>(what: impl Display, e: E) -> tera::Error
where
    E: std::error::Error + Send + Sync + 'static,
{
    tera::Error::chain(format!("{what}: {e}"), e)
}

pub(crate) fn msg(m: impl Display) -> tera::Error {
    tera::Error::message(m)
}

/// Tera's own argument errors (a kwarg of the wrong type, a missing or out-of-range one) name
/// neither the function nor the argument, and Tera points a filter's at its input: they get
/// the function's name, as a message (which Tera points at the call).
fn named(name: &str, e: tera::Error) -> tera::Error {
    match e.kind() {
        ErrorKind::InvalidArgument { .. }
        | ErrorKind::MissingArgument { .. }
        | ErrorKind::OutOfRangeArgument { .. } => msg(format!("{name}: {e}")),
        _ => e,
    }
}

/// Kwarg `key` of function `name` as a `T` (`None` when absent); a value of another type is
/// an error that names the function and the kwarg.
pub(crate) fn kwarg<'k, T>(kw: &'k Kwargs, key: &'k str, name: &str) -> TeraResult<Option<T>>
where
    T: ArgFromValue<'k, Output = T>,
{
    kw.get::<T>(key)
        .map_err(|e| msg(format!("{name}({key}=): {e}")))
}

fn no_scope(name: &str) -> tera::Error {
    msg(format!(
        "`{name}` needs the render scope: pass `page=`, or declare `@{SCOPE_KEY}` in the component \
         that calls it"
    ))
}

// ── the render scope ────────────────────────────────────────────────────────────────────────

/// The scope of the render, if the context has one.
pub(crate) fn scope(st: &State) -> TeraResult<Option<RenderScope>> {
    RenderScope::from_state(st)
}

/// The scope of the render; an error with a hint when the context has none (a component that
/// did not declare `@__nh`).
pub(crate) fn need_scope(st: &State, name: &str) -> TeraResult<RenderScope> {
    scope(st)?.ok_or_else(|| {
        msg(format!(
            "`{name}` needs the render scope `{SCOPE_KEY}`: declare `@{SCOPE_KEY}` in the \
             component that calls it"
        ))
    })
}

/// The format of page `id` whose links a page-relative call uses: its primary output format.
pub(crate) fn primary_format(model: &Model, id: PageId) -> FormatId {
    let p = &model.pages[id];
    p.urls
        .first()
        .map(|u| u.format)
        .or_else(|| p.formats.first().copied())
        .or_else(|| model.config.output_formats.by_name("html"))
        .unwrap_or_else(|| FormatId::from_index(0))
}

/// `base` moved to page `id` (its language and primary format); a new layout scope without a
/// base.
pub(crate) fn scope_for_page(model: &Model, base: Option<RenderScope>, id: PageId) -> RenderScope {
    let p = &model.pages[id];
    let format = primary_format(model, id);
    match base {
        Some(mut s) => {
            if s.page != id {
                s.pager = None;
            }
            s.page = id;
            s.lang = p.lang;
            s.format = format;
            s
        }
        None => RenderScope::layout(id, p.lang, format, None),
    }
}

/// The scope of a page-relative call: moved to `page=` when given, else the render's.
pub(crate) fn page_scope(
    views: &ViewCache,
    st: &State,
    kw: &Kwargs,
    name: &str,
) -> TeraResult<RenderScope> {
    let sc = scope(st)?;
    match kw.get::<Value>("page")? {
        Some(p) => {
            let id = page_id(views, &p, name)?;
            Ok(scope_for_page(views.model(), sc, id))
        }
        None => sc.ok_or_else(|| no_scope(name)),
    }
}

/// The language of the render: the scope's, else the context's `lang`, else the default
/// language.
pub(crate) fn render_lang(model: &Model, st: &State) -> TeraResult<LangIdx> {
    if let Some(s) = scope(st)? {
        return Ok(s.lang);
    }
    Ok(context_lang(model, st).unwrap_or_else(|| LangIdx::from_index(0)))
}

/// The language named by the context's `lang`.
pub(crate) fn context_lang(model: &Model, st: &State) -> Option<LangIdx> {
    let lang = st.get::<Value>("lang").ok().flatten()?;
    lang_by_key(model, lang.as_str()?)
}

/// The site language with key `key`.
pub(crate) fn lang_by_key(model: &Model, key: &str) -> Option<LangIdx> {
    model
        .config
        .sites
        .iter_enumerated()
        .find(|(_, s)| s.language.key.eq_ignore_ascii_case(key))
        .map(|(i, _)| i)
}

/// The view generation a render in `s` sees.
pub(crate) fn generation<'v>(views: &'v ViewCache, s: Option<&RenderScope>) -> &'v ViewGeneration {
    match s {
        Some(s) => views.generation(s.phase, s.variant),
        None => views.generation(ssg_view::Phase::Layout, ssg_view::HookVariant::Html),
    }
}

// ── arguments ───────────────────────────────────────────────────────────────────────────────

/// The entry `key` of a map value.
pub(crate) fn field<'v>(v: &'v Value, key: &str) -> Option<&'v Value> {
    let m = v.as_map()?;
    m.get_index_of(&Key::Str(key)).map(|i| &m[i])
}

/// The page a page value (summary, link or full) stands for.
pub(crate) fn page_id(views: &ViewCache, v: &Value, what: &str) -> TeraResult<PageId> {
    if field(v, "__rid").is_some() {
        return Err(msg(format!("{what}: expected a page, got a resource")));
    }
    let id = field(v, "id")
        .filter(|_| field(v, "kind").is_some())
        .and_then(Value::as_u64)
        .and_then(|i| u32::try_from(i).ok())
        .map(PageId::from_raw)
        .ok_or_else(|| msg(format!("{what}: expected a page, got {}", v.name())))?;
    if !views.contains(id) {
        return Err(msg(format!("{what}: no page with id {id}")));
    }
    Ok(id)
}

/// The pages of a list of page values.
pub(crate) fn page_ids(views: &ViewCache, v: &Value, what: &str) -> TeraResult<Vec<PageId>> {
    if v.is_none() || v.is_undefined() {
        return Ok(Vec::new());
    }
    list(v, what)?
        .iter()
        .map(|p| page_id(views, p, what))
        .collect()
}

/// The elements of an array value.
pub(crate) fn list<'v>(v: &'v Value, what: &str) -> TeraResult<&'v [Value]> {
    v.as_array()
        .ok_or_else(|| msg(format!("{what}: expected a list, got {}", v.name())))
}

/// The store resource a resource view stands for.
pub(crate) fn resource_id(store: &ResourceStore, v: &Value, what: &str) -> TeraResult<ResourceId> {
    let Some(rid) = field(v, "__rid") else {
        let got = if field(v, "kind").is_some() && field(v, "id").is_some() {
            "a page"
        } else {
            v.name()
        };
        return Err(msg(format!("{what}: expected a resource, got {got}")));
    };
    let id = rid
        .as_u64()
        .and_then(|i| u32::try_from(i).ok())
        .map(ResourceId::from_raw)
        .ok_or_else(|| msg(format!("{what}: `__rid` is not a resource id")))?;
    if id.index() >= store.len() {
        return Err(msg(format!("{what}: no resource with id {}", id.raw())));
    }
    Ok(id)
}

/// A string argument or input: strings as they are, numbers and booleans printed.
pub(crate) fn text(v: &Value, what: &str) -> TeraResult<String> {
    if let Some(s) = v.as_str() {
        return Ok(s.to_owned());
    }
    if v.is_none() || v.is_undefined() {
        return Ok(String::new());
    }
    if v.is_number() || v.is_bool() {
        return Ok(v.to_string());
    }
    Err(msg(format!("{what}: expected a string, got {}", v.name())))
}

// ── conversions ─────────────────────────────────────────────────────────────────────────────

/// A template value as JSON (undefined and none are null).
pub(crate) fn to_json(v: &Value) -> serde_json::Value {
    serde_json::to_value(v).unwrap_or(serde_json::Value::Null)
}

/// A template value as a data value.
pub(crate) fn to_data(v: &Value) -> ssg_base::Value {
    ssg_base::Value::from_json(to_json(v))
}

/// A data map from a template map value (`None` for none or undefined).
pub(crate) fn to_data_map(v: Option<&Value>, what: &str) -> TeraResult<Option<ssg_base::Map>> {
    match v {
        None => Ok(None),
        Some(v) if v.is_none() || v.is_undefined() => Ok(None),
        Some(v) => match to_data(v) {
            ssg_base::Value::Map(m) => Ok(Some((*m).clone())),
            _ => Err(msg(format!("{what}: expected a map, got {}", v.name()))),
        },
    }
}

/// A new map value from entries in the given order.
pub(crate) fn map_value<'a>(entries: impl IntoIterator<Item = (&'a str, Value)>) -> Value {
    let mut m = tera::Map::new();
    for (k, v) in entries {
        m.insert(Key::String(Arc::from(k)), v);
    }
    Value::from(m)
}

// ── the late-bound services ─────────────────────────────────────────────────────────────────

/// The render session, while it exists.
pub(crate) fn renderer(
    slot: &OnceLock<Weak<dyn ContentRenderer>>,
    name: &str,
) -> TeraResult<Arc<dyn ContentRenderer>> {
    slot.get().and_then(Weak::upgrade).ok_or_else(|| {
        msg(format!(
            "`{name}`: no render session (the renderer is not set)"
        ))
    })
}

/// The loaded templates, once set.
pub(crate) fn templates(
    slot: &OnceLock<Weak<Templates>>,
    name: &str,
) -> TeraResult<Arc<Templates>> {
    slot.get()
        .and_then(Weak::upgrade)
        .ok_or_else(|| msg(format!("`{name}`: the templates are not loaded yet")))
}
