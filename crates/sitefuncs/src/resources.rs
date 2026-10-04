//! Resources: assets, remote resources, named targets, the pipes, `resource_content`,
//! `publish`, `post_process`, `execute_as_template`, `purge_css`, and `unmarshal`.
//!
//! Every result is a resource view (`ssg_view::resource_view`); transforms are lazy (the
//! store computes them on `.Content` or when publishing), and a view whose links are only
//! known in phase E5 carries post-process placeholders.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::{Arc, OnceLock, Weak};

use ssg_base::diag::{Diagnostic, Diagnostics};
use ssg_base::{PageId, ResourceId};
use ssg_layouts::Templates;
use ssg_minify::{CssPurges, MinifyError, PurgeOptions, PurgePlan};
use ssg_resources::pipes::has_placeholder;
use ssg_resources::pipes::{JsBuildSpec, ToCssOptions};
use ssg_resources::{
    CallSite, HashAlgo, PipeError, PpField, RemoteOptions, ResourceStore, TemplateExecutor,
    Transform,
};
use ssg_view::{ContentRenderer, SCOPE_KEY, ViewCache, post_processed_view, resource_view};
use tera::{Kwargs, State, TeraResult, Value};

use crate::Handles;
use crate::call::{
    Registrar, SiteFilter, SiteFunction, chain, field, list, msg, render_lang, renderer,
    resource_id, scope, templates, text, to_data_map, to_json,
};

mod purge;
mod unmarshal;

use purge::*;
use unmarshal::*;

type RendererSlot = Arc<OnceLock<Weak<dyn ContentRenderer>>>;

pub(crate) fn register(r: &mut Registrar<'_>, h: &Handles) {
    let assets = |kind| Assets {
        views: Arc::clone(&h.views),
        store: Arc::clone(&h.store),
        kind,
    };
    r.function("get_asset", assets(AssetCall::Get));
    r.function("find_asset", assets(AssetCall::FindOne));
    r.function("find_assets", assets(AssetCall::FindAll));
    r.function("concat_assets", assets(AssetCall::Concat));
    r.function("asset_from_string", assets(AssetCall::FromString));
    r.function(
        "get_remote",
        GetRemote {
            views: Arc::clone(&h.views),
            store: Arc::clone(&h.store),
            diagnostics: Arc::clone(&h.diagnostics),
        },
    );
    for (name, pipe) in [
        ("fingerprint", Pipe::Fingerprint),
        ("minify", Pipe::Minify),
        ("to_css", Pipe::ToCss),
        ("js_build", Pipe::JsBuild),
    ] {
        r.filter(
            name,
            PipeFilter {
                store: Arc::clone(&h.store),
                name,
                pipe,
            },
        );
    }
    r.filter(
        "publish",
        Publish {
            store: Arc::clone(&h.store),
        },
    );
    r.filter(
        "post_process",
        PostProcess {
            store: Arc::clone(&h.store),
        },
    );
    r.filter(
        "resource_content",
        ResourceContent {
            views: Arc::clone(&h.views),
            store: Arc::clone(&h.store),
            renderer: Arc::clone(&h.renderer),
        },
    );
    r.filter(
        "execute_as_template",
        ExecuteAsTemplate {
            views: Arc::clone(&h.views),
            store: Arc::clone(&h.store),
            templates: Arc::clone(&h.templates),
        },
    );
    r.filter(
        "purge_css",
        PurgeCss {
            store: Arc::clone(&h.store),
            purges: Arc::clone(&h.css_purges),
        },
    );
    r.filter(
        "unmarshal",
        Unmarshal {
            store: Arc::clone(&h.store),
        },
    );
}

/// A resource view value of store resource `id`.
pub(crate) fn view_value(store: &ResourceStore, id: ResourceId) -> Value {
    Value::from_serializable(&resource_view(store, id))
}

pub(crate) fn call_site(views: &ViewCache, st: &State) -> TeraResult<CallSite> {
    Ok(CallSite::in_lang(render_lang(views.model(), st)?))
}

#[derive(Clone, Copy)]
enum AssetCall {
    Get,
    FindOne,
    FindAll,
    Concat,
    FromString,
}

/// `get_asset(path=)`, `find_asset(pattern=)`, `find_assets(pattern=)` (the assets of the
/// render's language), `concat_assets(target=, items=)`, `asset_from_string(target=,
/// content=)`.
struct Assets {
    views: Arc<ViewCache>,
    store: Arc<ResourceStore>,
    kind: AssetCall,
}

impl SiteFunction for Assets {
    fn call(&self, kw: &Kwargs, st: &State) -> TeraResult<Value> {
        let store = &self.store;
        let lang = render_lang(self.views.model(), st)?;
        let one = |id: Option<ResourceId>| id.map_or_else(Value::none, |id| view_value(store, id));
        match self.kind {
            AssetCall::Get => {
                let path = kw.must_get::<&str>("path")?;
                let id = store
                    .get_asset(lang, path)
                    .map_err(|e| chain(format!("get_asset(path=\"{path}\")"), e))?;
                Ok(one(id))
            }
            AssetCall::FindOne => {
                let pattern = kw.must_get::<&str>("pattern")?;
                let id = store
                    .find_asset(lang, pattern)
                    .map_err(|e| chain(format!("find_asset(pattern=\"{pattern}\")"), e))?;
                Ok(one(id))
            }
            AssetCall::FindAll => {
                let pattern = kw.must_get::<&str>("pattern")?;
                let ids = store
                    .find_assets(lang, pattern)
                    .map_err(|e| chain(format!("find_assets(pattern=\"{pattern}\")"), e))?;
                Ok(Value::from(
                    ids.into_iter()
                        .map(|id| view_value(store, id))
                        .collect::<Vec<_>>(),
                ))
            }
            AssetCall::Concat => {
                let target = kw.must_get::<&str>("target")?;
                let items = kw.must_get::<Value>("items")?;
                let ids = list(&items, "concat_assets(items=)")?
                    .iter()
                    .map(|i| resource_id(store, i, "concat_assets(items=)"))
                    .collect::<TeraResult<Vec<_>>>()?;
                let id = store
                    .concat(target, &ids, &call_site(&self.views, st)?)
                    .map_err(|e| chain(format!("concat_assets(target=\"{target}\")"), e))?;
                Ok(view_value(store, id))
            }
            AssetCall::FromString => {
                let target = kw.must_get::<&str>("target")?;
                let content = text(&kw.must_get::<Value>("content")?, "asset_from_string")?;
                let id = store
                    .from_string(target, &content, &call_site(&self.views, st)?)
                    .map_err(|e| chain(format!("asset_from_string(target=\"{target}\")"), e))?;
                Ok(view_value(store, id))
            }
        }
    }
}

/// `get_remote(url=, options=?, optional=?)`: a remote resource (none for a 404). A failure
/// is an error, unless `optional=true`: then none and a warning.
struct GetRemote {
    views: Arc<ViewCache>,
    store: Arc<ResourceStore>,
    diagnostics: Arc<Diagnostics>,
}

impl SiteFunction for GetRemote {
    fn call(&self, kw: &Kwargs, st: &State) -> TeraResult<Value> {
        let url = kw.must_get::<&str>("url")?;
        let optional = kw.get::<bool>("optional")?.unwrap_or(false);
        let options = kw.get::<Value>("options")?;
        let map = to_data_map(options.as_ref(), "get_remote(options=)")?;
        let lang = render_lang(self.views.model(), st)?;
        let result = RemoteOptions::from_map(map.as_ref())
            .and_then(|o| self.store.get_remote(lang, url, &o));
        match result {
            Ok(id) => Ok(id.map_or_else(Value::none, |id| view_value(&self.store, id))),
            Err(e) if optional => {
                self.diagnostics.push(
                    Diagnostic::warning(format!("get_remote(url=\"{url}\"): {e}"))
                        .with_id("get_remote"),
                );
                Ok(Value::none())
            }
            Err(e) => Err(chain(format!("get_remote(url=\"{url}\")"), e)),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pipe {
    Fingerprint,
    Minify,
    ToCss,
    JsBuild,
}

/// `fingerprint(algo=?)`, `minify`, `to_css(options=?)`, `js_build`: the
/// transformed resource (computed lazily by the store).
struct PipeFilter {
    store: Arc<ResourceStore>,
    name: &'static str,
    pipe: Pipe,
}

impl PipeFilter {
    fn transform(&self, kw: &Kwargs) -> TeraResult<Transform> {
        let options = || -> TeraResult<serde_json::Value> {
            Ok(kw
                .get::<Value>("options")?
                .map_or(serde_json::Value::Null, |v| to_json(&v)))
        };
        let opt = |e: PipeError| chain(format!("{}(options=)", self.name), e);
        Ok(match self.pipe {
            Pipe::Fingerprint => {
                let algo = kw.get::<&str>("algo")?.unwrap_or("");
                Transform::Fingerprint(
                    algo.parse::<HashAlgo>()
                        .map_err(|e| chain("fingerprint(algo=)", e))?,
                )
            }
            Pipe::Minify => Transform::Minify,
            Pipe::ToCss => Transform::ToCss(ToCssOptions::from_json(&options()?).map_err(opt)?),
            Pipe::JsBuild => {
                Transform::JsBuild(Box::new(JsBuildSpec::from_json(&options()?).map_err(opt)?))
            }
        })
    }
}

impl SiteFilter for PipeFilter {
    fn call(&self, v: Value, kw: &Kwargs, _: &State) -> TeraResult<Value> {
        let id = resource_id(&self.store, &v, self.name)?;
        let t = self.transform(kw)?;
        let out = self
            .store
            .transform(id, t)
            .map_err(|e| chain(self.name, e))?;
        // A pending `fingerprint` is computed now, so its links are final and the page is
        // written at once instead of being held until E5 with placeholder links; only a chain
        // that waits for E5 (images) keeps the placeholders.
        if self.pipe == Pipe::Fingerprint && !self.store.waits_for_e5(out) {
            self.store.realize(out).map_err(|e| chain(self.name, e))?;
        }
        Ok(view_value(&self.store, out))
    }
}

/// `r | publish`: publishes `r` even if no output names its URL; returns it.
struct Publish {
    store: Arc<ResourceStore>,
}

impl SiteFilter for Publish {
    fn call(&self, v: Value, _: &Kwargs, _: &State) -> TeraResult<Value> {
        let id = resource_id(&self.store, &v, "publish")?;
        self.store.mark_published(id);
        Ok(v)
    }
}

/// `r | post_process`: the view whose links, integrity and media type are placeholders filled
/// in phase E5 (`resource_content` of it is the content placeholder).
struct PostProcess {
    store: Arc<ResourceStore>,
}

impl SiteFilter for PostProcess {
    fn call(&self, v: Value, _: &Kwargs, _: &State) -> TeraResult<Value> {
        let id = resource_id(&self.store, &v, "post_process")?;
        Ok(Value::from_serializable(&post_processed_view(
            &self.store,
            id,
        )))
    }
}

/// Whether a resource view's links are post-process placeholders.
fn is_post_processed(v: &Value) -> bool {
    field(v, "rel_permalink")
        .and_then(Value::as_str)
        .is_some_and(has_placeholder)
}

/// `r | resource_content`: the text of a resource; a bundled content page's rendered HTML
/// (safe); the content placeholder of a post-processed resource.
struct ResourceContent {
    views: Arc<ViewCache>,
    store: Arc<ResourceStore>,
    renderer: RendererSlot,
}

impl SiteFilter for ResourceContent {
    fn call(&self, v: Value, _: &Kwargs, st: &State) -> TeraResult<Value> {
        let id = resource_id(&self.store, &v, "resource_content")?;
        let page = field(&v, "page_id")
            .and_then(Value::as_u64)
            .and_then(|p| u32::try_from(p).ok())
            .map(PageId::from_raw)
            .filter(|p| self.views.contains(*p));
        if let Some(page) = page {
            let caller = match scope(st)? {
                Some(s) => s,
                None => crate::call::scope_for_page(self.views.model(), None, page),
            };
            let r = renderer(&self.renderer, "resource_content")?;
            let c = r
                .content(page, caller.variant, &caller)
                .map_err(|e| chain("resource_content", e))?;
            return Ok(Value::safe_string(&c.html));
        }
        if is_post_processed(&v) {
            let pp = self.store.post_process(id);
            return Ok(Value::from(pp.placeholder(PpField::Content)));
        }
        let bytes = self
            .store
            .content(id)
            .map_err(|e| chain("resource_content", e))?;
        let s = std::str::from_utf8(&bytes).map_err(|e| {
            msg(format!(
                "resource_content: {} is not text: {e}",
                self.store.resource(id).name
            ))
        })?;
        Ok(Value::from(s))
    }
}

/// Renders asset sources with the build's Tera instance.
struct Executor<'a> {
    tera: &'a tera::Tera,
    context: tera::Context,
    autoescape: bool,
}

impl TemplateExecutor for Executor<'_> {
    fn execute(&self, _name: &str, source: &str) -> Result<String, String> {
        self.tera
            .render_str(source, &self.context, self.autoescape)
            .map_err(|e| e.to_string())
    }
}

/// `r | execute_as_template(target=, data=?)`: the asset's text rendered as a Tera template
/// with `data`, `site`, `build` and `__nh`, as a resource at `target` (escaped when `target` is
/// HTML, XML or SVG).
struct ExecuteAsTemplate {
    views: Arc<ViewCache>,
    store: Arc<ResourceStore>,
    templates: Arc<OnceLock<Weak<Templates>>>,
}

impl SiteFilter for ExecuteAsTemplate {
    fn call(&self, v: Value, kw: &Kwargs, st: &State) -> TeraResult<Value> {
        let name = "execute_as_template";
        let id = resource_id(&self.store, &v, name)?;
        let target = kw.must_get::<&str>("target")?;
        let data = kw.get::<Value>("data")?.unwrap_or_else(Value::none);
        let t = templates(&self.templates, name)?;
        let mut context = tera::Context::new();
        for key in ["site", "build"] {
            if let Some(v) = st.get::<Value>(key)? {
                context.insert_value(key, v);
            }
        }
        if let Some(s) = scope(st)? {
            context.insert_value(SCOPE_KEY, s.child().to_value());
        }
        // The value itself, not a copy through JSON: a page as `data` is a large value, and the
        // asset is executed for every page that calls this.
        context.insert_value("data", data);
        let exec = Executor {
            tera: t.tera(),
            context,
            autoescape: ssg_layouts::AUTOESCAPE_SUFFIXES
                .iter()
                .any(|s| target.ends_with(s)),
        };
        let out = self
            .store
            .execute_as_template(id, target, &exec, &call_site(&self.views, st)?)
            .map_err(|e| chain(format!("{name}(target=\"{target}\")"), e))?;
        Ok(view_value(&self.store, out))
    }
}
