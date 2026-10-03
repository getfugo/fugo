//! Content adapters (`content/**/_content.html`, phase `Adapter`): `add_page`, `add_resource`,
//! `enable_all_languages` and the adapter's store (the Go implementation's
//! `pagesfromdata/pagesfromgotmpl.go`, `pagesFromDataTemplateContext`).
//!
//! The build starts one [`AdapterRun`] per execution of an adapter in a language
//! ([`ContentAdapters::begin`]); the render's scope names it (`__nh.adapter`), so calls inside
//! partials add to the same run. A map is checked when it is added (Go reports these errors
//! at the `AddPage` call): an `add_page` map is decoded with [`AdapterPage::decode`], an
//! `add_resource` map here. A run that adds a path again replaces what it added there before
//! (Go inserts into its page and resource trees, where the last insert wins), with a warning;
//! a map of the run's paths finds the earlier item, so a run of n calls takes linear time.
//! The store (`store_set`/`store_get` without `page=`) belongs to the adapter file: its runs
//! for every language share it.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::{Arc, Mutex, PoisonError};

use ssg_base::diag::{Diagnostic, Diagnostics};
use ssg_base::paths::{self, ContentKey};
use ssg_base::{LangIdx, Params};
use ssg_page::AdapterPage;
use ssg_resources::{Body, ResourceStore};
use ssg_site::{AddedBody, AddedContent, AddedPage, AddedResource};
use ssg_view::{Phase, RenderScope, ViewCache};
use tera::{Kwargs, State, TeraResult, Value};

use crate::Handles;
use crate::call::{Registrar, SiteFunction, field, msg, need_scope, resource_id, text, to_data};

/// One execution of a content adapter for one language, and what it added.
#[derive(Clone, Debug)]
pub struct AdapterRun {
    /// The adapter (an index into `ssg_site::Captured::adapters`).
    pub adapter: usize,
    pub lang: LangIdx,
    /// The adapter's directory: paths of added pages and resources are relative to it.
    pub dir: ContentKey,
    /// The pages added, one per path (a later `add_page` of a path replaces the earlier one
    /// in its place).
    pub pages: Vec<AddedPage>,
    /// The resources added, one per path (as `pages`).
    pub resources: Vec<AddedResource>,
    /// `enable_all_languages()` was called.
    pub all_languages: bool,
}

/// A run with the place of each path it added in `pages` and `resources`, so that adding a
/// path again finds the earlier item without a scan.
#[derive(Debug)]
struct RunState {
    run: AdapterRun,
    page_at: HashMap<String, usize>,
    resource_at: HashMap<String, usize>,
}

/// The adapter runs of a build and the adapters' stores.
#[derive(Debug, Default)]
pub struct ContentAdapters {
    runs: Mutex<Vec<RunState>>,
    /// (adapter, key) → value.
    stores: Mutex<HashMap<(usize, String), Value>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl ContentAdapters {
    /// Starts a run of `adapter` (whose directory is `dir`) in `lang`; the render's scope
    /// carries the returned id (`RenderScope::adapter`).
    pub fn begin(&self, adapter: usize, lang: LangIdx, dir: ContentKey) -> u32 {
        let mut runs = lock(&self.runs);
        runs.push(RunState {
            run: AdapterRun {
                adapter,
                lang,
                dir,
                pages: Vec::new(),
                resources: Vec::new(),
                all_languages: false,
            },
            page_at: HashMap::new(),
            resource_at: HashMap::new(),
        });
        u32::try_from(runs.len() - 1).expect("fewer than 2^32 adapter runs")
    }

    /// What run `run` added so far.
    ///
    /// # Panics
    /// When `run` was not started with [`begin`](Self::begin).
    #[must_use]
    pub fn run(&self, run: u32) -> AdapterRun {
        lock(&self.runs)[run as usize].run.clone()
    }

    /// What run `run` added, moved out: the run keeps its adapter, language, directory and
    /// `all_languages`, and adds to empty lists afterwards.
    ///
    /// # Panics
    /// When `run` was not started with [`begin`](Self::begin).
    #[must_use]
    pub fn take(&self, run: u32) -> AdapterRun {
        let mut runs = lock(&self.runs);
        let state = &mut runs[run as usize];
        state.page_at.clear();
        state.resource_at.clear();
        AdapterRun {
            pages: std::mem::take(&mut state.run.pages),
            resources: std::mem::take(&mut state.run.resources),
            ..state.run.clone()
        }
    }

    fn with_run<R>(&self, run: u32, f: impl FnOnce(&mut RunState) -> R) -> TeraResult<R> {
        let mut runs = lock(&self.runs);
        let r = runs
            .get_mut(run as usize)
            .ok_or_else(|| msg(format!("no content adapter run {run}")))?;
        Ok(f(r))
    }

    /// The value of `key` in the store of the adapter of run `run`.
    pub(crate) fn store_get(&self, run: u32, key: &str) -> TeraResult<Option<Value>> {
        let adapter = self.with_run(run, |r| r.run.adapter)?;
        Ok(lock(&self.stores).get(&(adapter, key.to_owned())).cloned())
    }

    /// Sets `key` in the store of the adapter of run `run`.
    pub(crate) fn store_set(&self, run: u32, key: &str, value: Value) -> TeraResult<()> {
        let adapter = self.with_run(run, |r| r.run.adapter)?;
        lock(&self.stores).insert((adapter, key.to_owned()), value);
        Ok(())
    }
}

pub(crate) fn register(r: &mut Registrar<'_>, h: &Handles) {
    r.function(
        "add_page",
        AddPage {
            views: Arc::clone(&h.views),
            adapters: Arc::clone(&h.adapters),
            diagnostics: Arc::clone(&h.diagnostics),
        },
    );
    r.function(
        "add_resource",
        AddResource {
            views: Arc::clone(&h.views),
            store: Arc::clone(&h.store),
            adapters: Arc::clone(&h.adapters),
            diagnostics: Arc::clone(&h.diagnostics),
        },
    );
    r.function(
        "enable_all_languages",
        EnableAllLanguages {
            adapters: Arc::clone(&h.adapters),
        },
    );
}

/// The adapter run of the render; an error outside content adapters.
pub(crate) fn adapter_run(s: &RenderScope, name: &str) -> TeraResult<u32> {
    match (s.phase, s.adapter) {
        (Phase::Adapter, Some(run)) => Ok(run),
        _ => Err(msg(format!(
            "`{name}` is only available in content adapters (`content/**/_content.html`)"
        ))),
    }
}

/// A map argument as folded params.
fn map_arg(v: &Value, what: &str) -> TeraResult<Params> {
    match to_data(v) {
        ssg_base::Value::Map(m) => Ok(Params::fold(&m)),
        _ => Err(msg(format!("{what}: expected a map, got {}", v.name()))),
    }
}

/// The entry `key` of a map value, ignoring case (Go's map keys are case-insensitive).
fn field_fold(v: &Value, key: &str) -> Option<Value> {
    let m = v.as_map()?;
    m.iter()
        .find(|(k, _)| k.as_str().is_some_and(|k| k.eq_ignore_ascii_case(key)))
        .map(|(_, v)| v.clone())
}

/// Adds `item`, whose path is `path`, to `list`, replacing the item with the same path in its
/// place (Go's `InsertIntoValuesDimension`: the last insert of a key wins); `at` maps the
/// paths in `list` to their places. A replacement is reported as a `duplicate-<what>-path`
/// warning (`what`: `content` for pages, `resource`).
fn insert_or_replace<T>(
    (list, at): (&mut Vec<T>, &mut HashMap<String, usize>),
    path: String,
    item: T,
    (dir, what): (&ContentKey, &str),
    diagnostics: &Diagnostics,
) {
    let earlier = match at.entry(path) {
        Entry::Vacant(e) => {
            e.insert(list.len());
            list.push(item);
            return;
        }
        Entry::Occupied(e) => e,
    };
    let call = if what == "content" {
        "add_page"
    } else {
        "add_resource"
    };
    diagnostics.push(
        Diagnostic::warning(format!(
            "duplicate {what} path \"/{}\": the content adapter in {:?} adds it again; the \
             last `{call}` is used",
            earlier.key(),
            dir.to_path()
        ))
        .with_id(format!("duplicate-{what}-path")),
    );
    list[*earlier.get()] = item;
}

/// `add_page(page=)`: adds a page to the run's language, below the adapter's directory.
struct AddPage {
    views: Arc<ViewCache>,
    adapters: Arc<ContentAdapters>,
    diagnostics: Arc<Diagnostics>,
}

impl SiteFunction for AddPage {
    fn call(&self, kw: &Kwargs, st: &State) -> TeraResult<Value> {
        let s = need_scope(st, "add_page")?;
        let run = adapter_run(&s, "add_page")?;
        let map = map_arg(&kw.must_get::<Value>("page")?, "add_page(page=)")?;
        let types = &self.views.model().config.media_types;
        let (adapter, lang, dir) = self
            .adapters
            .with_run(run, |r| (r.run.adapter, r.run.lang, r.run.dir.clone()))?;
        let page =
            AdapterPage::decode(&map, &dir, types).map_err(|e| msg(format!("add_page: {e}")))?;
        self.adapters.with_run(run, |r| {
            let path = page.path.clone();
            let page = AddedPage {
                adapter,
                lang,
                page: Arc::new(page),
            };
            insert_or_replace(
                (&mut r.run.pages, &mut r.page_at),
                path,
                page,
                (&r.run.dir, "content"),
                &self.diagnostics,
            );
        })?;
        Ok(Value::from(""))
    }
}

/// `add_resource(resource=)`: adds a page resource below the adapter's directory.
struct AddResource {
    views: Arc<ViewCache>,
    store: Arc<ResourceStore>,
    adapters: Arc<ContentAdapters>,
    diagnostics: Arc<Diagnostics>,
}

impl AddResource {
    /// The resource of an `add_resource` map (Go's `ResourceConfig` with `Validate` and
    /// `Compile`): `path` joined to `dir` with `path.Join` and normalised (lower case, spaces
    /// → `-`); an empty `path` is an error (Go would put the resource at the adapter's
    /// directory, the key of its section page); a
    /// `content.value` that is a resource keeps that resource's file, link and media type.
    fn decode(
        &self,
        v: &Value,
        adapter: usize,
        lang: LangIdx,
        dir: &ContentKey,
    ) -> TeraResult<AddedResource> {
        let what = "add_resource";
        let map = map_arg(v, "add_resource(resource=)")?;
        let string = |m: &Params, key: &str| -> TeraResult<Option<String>> {
            use ssg_base::Value as V;
            match m.get(key) {
                None | Some(V::Null) => Ok(None),
                Some(V::String(s)) => Ok(Some(s.to_string())),
                Some(V::Int(i)) => Ok(Some(i.to_string())),
                Some(V::Float(f)) => Ok(Some(f.to_string())),
                Some(V::Bool(b)) => Ok(Some(b.to_string())),
                Some(_) => Err(msg(format!("{what}: `{key}` must be a string"))),
            }
        };
        // `Compile`: the path is joined as it is (no slash removed, not trimmed).
        let path = string(&map, "path")?.unwrap_or_default();
        if path.is_empty() {
            return Err(msg(format!("{what}: `path` is required")));
        }
        let joined = paths::join(&["/", &dir.to_path(), &path]);
        let path = paths::normalize_key(joined.trim_start_matches('/'));
        let params = match map.get("params") {
            None | Some(ssg_base::Value::Null) => Params::default(),
            Some(ssg_base::Value::Map(m)) => Params::fold(m),
            Some(_) => return Err(msg(format!("{what}: `params` must be a map"))),
        };
        // The content as the template passed it: a resource value is not data.
        let content = field_fold(v, "content").unwrap_or_else(Value::none);
        let cmap = if content.is_none() || content.is_undefined() {
            Params::default()
        } else {
            map_arg(&content, "add_resource: `content`")?
        };
        if string(&cmap, "markup")?.is_some_and(|m| !m.is_empty()) {
            return Err(msg(format!(
                "{what}: `content.markup` cannot be set: use `content.mediaType`"
            )));
        }
        let media_type = string(&cmap, "mediatype")?.filter(|m| !m.is_empty());
        if let Some(mt) = &media_type
            && self.views.model().config.media_types.by_type(mt).is_none()
        {
            return Err(msg(format!("{what}: unknown media type {mt:?}")));
        }
        let value = field_fold(&content, "value").unwrap_or_else(Value::none);
        let content = if field(&value, "__rid").is_some() {
            let id = resource_id(&self.store, &value, what)?;
            let r = self.store.resource(id);
            let body = match &r.body {
                Body::File(p) => AddedBody::File(p.clone()),
                _ => AddedBody::Bytes(
                    self.store
                        .content(id)
                        .map_err(|e| msg(format!("{what}: {e}")))?,
                ),
            };
            AddedContent::Resource {
                body,
                media_type: media_type.unwrap_or_else(|| r.media_type_string()),
                target: r.target.clone(),
                link: r.link.clone(),
            }
        } else {
            AddedContent::Text {
                text: Arc::from(text(&value, what)?.as_str()),
                media_type,
            }
        };
        Ok(AddedResource {
            adapter,
            lang,
            path,
            name: string(&map, "name")?.filter(|n| !n.is_empty()),
            title: string(&map, "title")?.filter(|t| !t.is_empty()),
            params,
            content,
        })
    }
}

impl SiteFunction for AddResource {
    fn call(&self, kw: &Kwargs, st: &State) -> TeraResult<Value> {
        let s = need_scope(st, "add_resource")?;
        let run = adapter_run(&s, "add_resource")?;
        let (adapter, lang, dir) = self
            .adapters
            .with_run(run, |r| (r.run.adapter, r.run.lang, r.run.dir.clone()))?;
        let r = self.decode(&kw.must_get::<Value>("resource")?, adapter, lang, &dir)?;
        self.adapters.with_run(run, |state| {
            insert_or_replace(
                (&mut state.run.resources, &mut state.resource_at),
                r.path.clone(),
                r,
                (&state.run.dir, "resource"),
                &self.diagnostics,
            );
        })?;
        Ok(Value::from(""))
    }
}

/// `enable_all_languages()`: the adapter runs for the other languages too.
struct EnableAllLanguages {
    adapters: Arc<ContentAdapters>,
}

impl SiteFunction for EnableAllLanguages {
    fn call(&self, _: &Kwargs, st: &State) -> TeraResult<Value> {
        let s = need_scope(st, "enable_all_languages")?;
        let run = adapter_run(&s, "enable_all_languages")?;
        self.adapters
            .with_run(run, |r| r.run.all_languages = true)?;
        Ok(Value::from(""))
    }
}
