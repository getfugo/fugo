//! Pages: `get_page`, `deref`, `get_terms`, `related`, `param`, the page store, menus.

use std::sync::Arc;

use ssg_base::PageId;
use ssg_nav::{MenuEntry, Menus, RelatedQuery};
use ssg_view::{PageStores, Phase, ViewCache};
use tera::{Kwargs, State, TeraResult, Value};

use crate::call::{
    Registrar, SiteFilter, SiteFunction, chain, field, generation, lang_by_key, list, msg, page_id,
    page_ids, page_scope, scope, text, to_data,
};
use crate::{ContentAdapters, Handles, RelatedCache};

pub(crate) fn register(r: &mut Registrar<'_>, h: &Handles) {
    r.function(
        "get_page",
        GetPage {
            views: Arc::clone(&h.views),
        },
    );
    r.filter(
        "deref",
        Deref {
            views: Arc::clone(&h.views),
        },
    );
    r.function(
        "get_terms",
        GetTerms {
            views: Arc::clone(&h.views),
        },
    );
    r.function(
        "related",
        Related {
            views: Arc::clone(&h.views),
            cache: Arc::clone(&h.related),
        },
    );
    r.function(
        "param",
        Param {
            views: Arc::clone(&h.views),
        },
    );
    r.function(
        "store_set",
        StoreSet {
            views: Arc::clone(&h.views),
            stores: Arc::clone(&h.stores),
            adapters: Arc::clone(&h.adapters),
        },
    );
    r.function(
        "store_get",
        StoreGet {
            views: Arc::clone(&h.views),
            stores: Arc::clone(&h.stores),
            adapters: Arc::clone(&h.adapters),
        },
    );
    r.function(
        "is_menu_current",
        MenuCurrent {
            views: Arc::clone(&h.views),
            menus: Arc::clone(&h.menus),
            has: false,
        },
    );
    r.function(
        "has_menu_current",
        MenuCurrent {
            views: Arc::clone(&h.views),
            menus: Arc::clone(&h.menus),
            has: true,
        },
    );
}

/// `get_page(path=, lang=?, page=?)`: the full value of the page at `path`, or none. A
/// relative path (`./x`, `../x`) resolves against `page`, else the render's page; an explicit
/// `page=` also enables relative lookups of plain names (`.GetPage`).
struct GetPage {
    views: Arc<ViewCache>,
}

impl SiteFunction for GetPage {
    fn call(&self, kw: &Kwargs, st: &State) -> TeraResult<Value> {
        let model = self.views.model();
        let path = kw.must_get::<&str>("path")?;
        let sc = scope(st)?;
        let explicit = match kw.get::<Value>("page")? {
            Some(p) => Some(page_id(&self.views, &p, "get_page")?),
            None => None,
        };
        let relative =
            path.starts_with("./") || path.starts_with("../") || path == "." || path == "..";
        let from = explicit.or_else(|| sc.as_ref().map(|s| s.page).filter(|_| relative));
        let lang = match kw.get::<&str>("lang")? {
            Some(l) if !l.is_empty() => lang_by_key(model, l)
                .ok_or_else(|| msg(format!("get_page(lang=\"{l}\"): no such language")))?,
            _ => match (explicit, &sc) {
                (Some(p), _) => model.pages[p].lang,
                (None, Some(s)) => s.lang,
                (None, None) => crate::call::render_lang(model, st)?,
            },
        };
        let found = model
            .get_page(lang, path, from)
            .map_err(|e| chain(format!("get_page(path=\"{path}\")"), e))?;
        Ok(found.map_or_else(Value::none, |id| {
            generation(&self.views, sc.as_ref()).full(id)
        }))
    }
}

/// `p | deref`: the full value of a listed page.
struct Deref {
    views: Arc<ViewCache>,
}

impl SiteFilter for Deref {
    fn call(&self, v: Value, _: &Kwargs, st: &State) -> TeraResult<Value> {
        if v.is_none() {
            return Ok(v);
        }
        let id = page_id(&self.views, &v, "deref")?;
        Ok(generation(&self.views, scope(st)?.as_ref()).full(id))
    }
}

/// `get_terms(taxonomy=, page=?)`: the page's term links of `taxonomy` (`[]` when it is not a
/// taxonomy).
struct GetTerms {
    views: Arc<ViewCache>,
}

impl SiteFunction for GetTerms {
    fn call(&self, kw: &Kwargs, st: &State) -> TeraResult<Value> {
        let taxonomy = kw.must_get::<&str>("taxonomy")?;
        let s = page_scope(&self.views, st, kw, "get_terms")?;
        let summary = &generation(&self.views, Some(&s)).summaries[s.page];
        Ok(field(summary, "terms")
            .and_then(|t| field(t, &taxonomy.to_lowercase()).cloned())
            .unwrap_or_else(|| Value::from(Vec::<Value>::new())))
    }
}

/// `related(pages=, page=?, indices=?, limit=?)`: the pages of `pages` related to `page`, best
/// first (`[related]` of the page's language; one index per candidate list).
struct Related {
    views: Arc<ViewCache>,
    cache: Arc<RelatedCache>,
}

impl SiteFunction for Related {
    fn call(&self, kw: &Kwargs, st: &State) -> TeraResult<Value> {
        let s = page_scope(&self.views, st, kw, "related")?;
        let candidates = page_ids(
            &self.views,
            &kw.must_get::<Value>("pages")?,
            "related(pages=)",
        )?;
        let g = generation(&self.views, Some(&s));
        if candidates.is_empty() {
            return Ok(g.list(&[]));
        }
        let indices: Vec<String> = match kw.get::<Value>("indices")? {
            Some(v) if !v.is_none() => list(&v, "related(indices=)")?
                .iter()
                .map(|i| text(i, "related(indices=)"))
                .collect::<TeraResult<_>>()?,
            _ => Vec::new(),
        };
        let index = self
            .cache
            .index(&self.views, s.lang, &candidates)
            .map_err(|e| chain("related", e))?;
        let found = index
            .search(
                self.views.nav(),
                &RelatedQuery {
                    document: Some(s.page),
                    indices: &indices,
                    ..RelatedQuery::default()
                },
            )
            .map_err(|e| chain("related", e))?;
        let mut pages = found.pages;
        if let Some(limit) = kw.get::<i64>("limit")? {
            pages.truncate(usize::try_from(limit).unwrap_or(0));
        }
        Ok(g.list(&pages))
    }
}

/// `param(key=, page=?)`: the page param `key` (a dotted path), else the site param.
struct Param {
    views: Arc<ViewCache>,
}

impl SiteFunction for Param {
    fn call(&self, kw: &Kwargs, st: &State) -> TeraResult<Value> {
        let key = kw.must_get::<&str>("key")?.to_lowercase();
        let s = page_scope(&self.views, st, kw, "param")?;
        let model = self.views.model();
        let p = &model.pages[s.page];
        let site = &model.config.sites[p.lang].params;
        let found = p
            .params()
            .get(&key)
            .or_else(|| p.params().get_path(&key))
            .or_else(|| site.get(&key))
            .or_else(|| site.get_path(&key));
        Ok(found.map_or_else(Value::none, ssg_base::Value::to_tera))
    }
}

/// The content adapter run whose store a `store_*` call without `page=` uses (Go's `.Store`
/// of a `_content.gotmpl`).
fn adapter_store_run(st: &State, kw: &Kwargs) -> TeraResult<Option<u32>> {
    if kw.get::<Value>("page")?.is_some() {
        return Ok(None);
    }
    Ok(scope(st)?.and_then(|s| (s.phase == Phase::Adapter).then_some(s.adapter).flatten()))
}

/// `store_set(key=, value=, page=?)`: prints nothing. In a content adapter without `page=`,
/// the adapter's store.
struct StoreSet {
    views: Arc<ViewCache>,
    stores: Arc<PageStores>,
    adapters: Arc<ContentAdapters>,
}

impl SiteFunction for StoreSet {
    fn call(&self, kw: &Kwargs, st: &State) -> TeraResult<Value> {
        let key = kw.must_get::<&str>("key")?;
        let value = kw.must_get::<Value>("value")?;
        if let Some(run) = adapter_store_run(st, kw)? {
            self.adapters.store_set(run, key, value)?;
            return Ok(Value::from(""));
        }
        let s = page_scope(&self.views, st, kw, "store_set")?;
        self.stores.set(s.txn, s.page, key, value);
        Ok(Value::from(""))
    }
}

/// `store_get(key=, page=?)`: the stored value, or none. In a content adapter without
/// `page=`, the adapter's store.
struct StoreGet {
    views: Arc<ViewCache>,
    stores: Arc<PageStores>,
    adapters: Arc<ContentAdapters>,
}

impl SiteFunction for StoreGet {
    fn call(&self, kw: &Kwargs, st: &State) -> TeraResult<Value> {
        let key = kw.must_get::<&str>("key")?;
        if let Some(run) = adapter_store_run(st, kw)? {
            return Ok(self
                .adapters
                .store_get(run, key)?
                .unwrap_or_else(Value::none));
        }
        let s = page_scope(&self.views, st, kw, "store_get")?;
        Ok(self
            .stores
            .get(s.txn, s.page, key)
            .unwrap_or_else(Value::none))
    }
}

/// `is_menu_current(menu=, entry=, page=?)` and `has_menu_current(…)`.
struct MenuCurrent {
    views: Arc<ViewCache>,
    menus: Arc<Menus>,
    has: bool,
}

/// The menu entry a `MenuEntryView` value shows (what the current-entry queries compare:
/// identity, parent, page, URL and children).
fn menu_entry(views: &ViewCache, v: &Value) -> TeraResult<MenuEntry> {
    let what = "a menu entry";
    if v.as_map().is_none() {
        return Err(msg(format!("expected {what}, got {}", v.name())));
    }
    let s = |k: &str| {
        field(v, k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let page: Option<PageId> = match field(v, "page") {
        Some(p) if !p.is_none() => Some(page_id(views, p, "menu entry page")?),
        _ => None,
    };
    let children = match field(v, "children") {
        Some(c) if !c.is_none() => list(c, "menu entry children")?
            .iter()
            .map(|c| menu_entry(views, c))
            .collect::<TeraResult<_>>()?,
        _ => Vec::new(),
    };
    Ok(MenuEntry {
        identifier: s("identifier"),
        name: s("name"),
        title: s("title"),
        url: s("url"),
        page,
        weight: field(v, "weight")
            .and_then(Value::as_i64)
            .and_then(|w| i32::try_from(w).ok())
            .unwrap_or(0),
        parent: field(v, "parent")
            .and_then(Value::as_str)
            .map(str::to_owned),
        params: match to_data(field(v, "params").unwrap_or(&Value::none())) {
            ssg_base::Value::Map(m) => ssg_base::Params::fold(&m),
            _ => ssg_base::Params::default(),
        },
        children,
        ..MenuEntry::default()
    })
}

impl SiteFunction for MenuCurrent {
    fn call(&self, kw: &Kwargs, st: &State) -> TeraResult<Value> {
        let name = if self.has {
            "has_menu_current"
        } else {
            "is_menu_current"
        };
        let menu = kw.must_get::<&str>("menu")?;
        let entry = menu_entry(&self.views, &kw.must_get::<Value>("entry")?)?;
        let s = page_scope(&self.views, st, kw, name)?;
        let lang = self.views.model().pages[s.page].lang;
        let Some(site) = self.menus.0.get(lang) else {
            return Ok(Value::from(false));
        };
        let yes = if self.has {
            site.has_menu_current(self.views.nav(), s.page, menu, &entry)
        } else {
            site.is_menu_current(s.page, menu, &entry)
        };
        Ok(Value::from(yes))
    }
}
