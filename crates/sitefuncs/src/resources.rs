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

/// `css | purge_css(safelist=, greedy=, blocklist=, content=, variables=, important=)`: a
/// placeholder the publisher replaces, in each page, with the rules of `css` (a resource or a
/// string) that page uses (`ssg_minify::purge`). The plan is compiled once per input and
/// options.
struct PurgeCss {
    store: Arc<ResourceStore>,
    purges: Arc<CssPurges>,
}

/// A text `purge_css` reads: a resource's content (read when the plan is compiled) or a string.
#[derive(Hash)]
enum PurgeText {
    Resource(u32),
    Text(String),
}

impl PurgeCss {
    fn source(&self, v: &Value, what: &str) -> TeraResult<PurgeText> {
        if v.as_str().is_some() {
            return Ok(PurgeText::Text(text(v, what)?));
        }
        if is_post_processed(v) {
            return Err(msg(format!(
                "{what}: a post-processed resource has no content before every page is rendered"
            )));
        }
        Ok(PurgeText::Resource(
            resource_id(&self.store, v, what)?.raw(),
        ))
    }

    fn read(&self, t: &PurgeText) -> Result<String, MinifyError> {
        match t {
            PurgeText::Text(s) => Ok(s.clone()),
            PurgeText::Resource(raw) => {
                let id = ResourceId::from_raw(*raw);
                let bytes = self
                    .store
                    .content(id)
                    .map_err(|e| MinifyError::Purge(e.to_string()))?;
                String::from_utf8(bytes.to_vec()).map_err(|_| {
                    MinifyError::Purge(format!("{} is not text", self.store.resource(id).name))
                })
            }
        }
    }
}

impl SiteFilter for PurgeCss {
    fn call(&self, v: Value, kw: &Kwargs, _: &State) -> TeraResult<Value> {
        let name = "purge_css";
        let strings = |key: &str| -> TeraResult<Vec<String>> {
            match kw.get::<Value>(key)? {
                Some(v) if !v.is_none() => list(&v, &format!("{name}: {key}"))?
                    .iter()
                    .map(|s| text(s, &format!("{name}: {key}")))
                    .collect(),
                _ => Ok(Vec::new()),
            }
        };
        let options = PurgeOptions {
            safelist: strings("safelist")?,
            greedy: strings("greedy")?,
            blocklist: strings("blocklist")?,
            variables: kw.get::<bool>("variables")?.unwrap_or(false),
            drop_important: !kw.get::<bool>("important")?.unwrap_or(true),
        };
        let css = self.source(&v, name)?;
        let content = match kw.get::<Value>("content")? {
            Some(c) if !c.is_none() => list(&c, &format!("{name}: content"))?
                .iter()
                .map(|item| self.source(item, &format!("{name}: content")))
                .collect::<TeraResult<Vec<_>>>()?,
            _ => Vec::new(),
        };
        let mut key = DefaultHasher::new();
        (&options, &css, &content).hash(&mut key);
        let placeholder = self
            .purges
            .placeholder(key.finish(), || {
                let css = self.read(&css)?;
                let content = content
                    .iter()
                    .map(|t| self.read(t))
                    .collect::<Result<Vec<_>, _>>()?;
                let content: Vec<&str> = content.iter().map(String::as_str).collect();
                let targets = self.store.config().transforms.minifier.css_targets();
                PurgePlan::compile(&css, &options, &content, targets)
            })
            .map_err(msg)?;
        Ok(Value::safe_string(&placeholder))
    }
}

/// A data format of `unmarshal`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Format {
    Json,
    Toml,
    Yaml,
    Csv,
    Xml,
}

impl Format {
    fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "json" => Self::Json,
            "toml" => Self::Toml,
            "yaml" | "yml" => Self::Yaml,
            "csv" => Self::Csv,
            "xml" => Self::Xml,
            _ => return None,
        })
    }

    /// The format of a document: XML when it starts with `<`, else the first of `{`/`[`
    /// (JSON), `:` (YAML) and `=` (TOML), else CSV when it has a comma.
    fn detect(s: &str) -> Option<Self> {
        let t = s.trim_start();
        if t.starts_with('<') {
            return Some(Self::Xml);
        }
        let first = |c: char| s.find(c).unwrap_or(usize::MAX);
        let json = if t.starts_with(['{', '[']) {
            0
        } else {
            usize::MAX
        };
        let (yaml, toml) = (first(':'), first('='));
        let min = json.min(yaml).min(toml);
        if min == usize::MAX {
            return s.contains(',').then_some(Self::Csv);
        }
        Some(if min == json {
            Self::Json
        } else if min == toml {
            Self::Toml
        } else {
            Self::Yaml
        })
    }

    fn decode(self, s: &str) -> Result<ssg_base::Value, String> {
        use ssg_base::Value as Data;
        match self {
            Self::Json => Data::from_json_str(s).map_err(|e| e.to_string()),
            Self::Toml => Data::from_toml_str(s).map_err(|e| e.to_string()),
            Self::Yaml => Data::from_yaml_str(s).map_err(|e| e.to_string()),
            Self::Csv => csv_rows(s).map_err(|e| e.to_string()),
            Self::Xml => xml_root(s).map_err(|e| e.to_string()),
        }
    }
}

/// CSV (comma separated, rows of any length) as a list of lists of strings.
fn csv_rows(text: &str) -> Result<ssg_base::Value, csv::Error> {
    use ssg_base::Value as Data;
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes());
    let mut rows = Vec::new();
    for record in reader.records() {
        rows.push(Data::array(record?.iter().map(Data::string).collect()));
    }
    Ok(Data::array(rows))
}

/// An XML document as the value of its root element (attributes as `-name`, text beside
/// children or attributes as `#text`, repeated elements as lists), as `site.data` reads XML.
fn xml_root(text: &str) -> Result<ssg_base::Value, roxmltree::Error> {
    let doc = roxmltree::Document::parse(text)?;
    Ok(xml_element(doc.root_element()))
}

fn xml_element(e: roxmltree::Node<'_, '_>) -> ssg_base::Value {
    use ssg_base::Value as Data;
    let mut m = ssg_base::Map::new();
    for a in e.attributes() {
        m.insert(format!("-{}", a.name()), Data::string(a.value()));
    }
    let mut text = String::new();
    for child in e.children() {
        if child.is_element() {
            let name = child.tag_name().name();
            let v = xml_element(child);
            match m.get_mut(name) {
                None => {
                    m.insert(name, v);
                }
                Some(Data::Array(items)) => Arc::make_mut(items).push(v),
                Some(existing) => {
                    let first = std::mem::take(existing);
                    *existing = Data::array(vec![first, v]);
                }
            }
        } else if let Some(t) = child.text() {
            text.push_str(t);
        }
    }
    let text = text.trim();
    if m.is_empty() {
        return Data::string(text);
    }
    if !text.is_empty() {
        m.insert("#text", Data::string(text));
    }
    Data::map(m)
}

/// `x | unmarshal(format=?)`: a string or a resource's text parsed as JSON, TOML, YAML, CSV or
/// XML (`format`, else the resource's media type, else detected); map keys sorted.
struct Unmarshal {
    store: Arc<ResourceStore>,
}

impl SiteFilter for Unmarshal {
    fn call(&self, v: Value, kw: &Kwargs, _: &State) -> TeraResult<Value> {
        let explicit = match kw.get::<&str>("format")? {
            Some(f) => Some(Format::parse(f).ok_or_else(|| {
                msg(format!(
                    "unmarshal(format=\"{f}\"): expected json, toml, yaml, csv or xml"
                ))
            })?),
            None => None,
        };
        let (source, typed) = if field(&v, "__rid").is_some() {
            let id = resource_id(&self.store, &v, "unmarshal")?;
            let r = self.store.resource(id);
            let bytes = self.store.content(id).map_err(|e| chain("unmarshal", e))?;
            let s = String::from_utf8(bytes.to_vec())
                .map_err(|e| msg(format!("unmarshal: {} is not text: {e}", r.name)))?;
            let by_type = Format::parse(&r.media_type.sub).or_else(|| {
                r.name
                    .rsplit_once('.')
                    .and_then(|(_, ext)| Format::parse(ext))
            });
            (s, by_type)
        } else {
            (text(&v, "unmarshal")?, None)
        };
        if source.trim().is_empty() {
            return Ok(Value::from(tera::Map::new()));
        }
        let format = explicit
            .or(typed)
            .or_else(|| Format::detect(&source))
            .ok_or_else(|| msg("unmarshal: cannot detect the format of the input"))?;
        let data = format
            .decode(&source)
            .map_err(|e| msg(format!("unmarshal: {e}")))?;
        Ok(data.to_tera())
    }
}
