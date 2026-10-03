//! Test doubles of the T35 site functions the content engine calls back through
//! (`ssg_sitefuncs::register` is still the frozen stub): each is a few lines over the
//! `Handles` and the `ContentRenderer`, just enough for the content-phase suites. They are
//! registered with `Session::with_functions` and replaced by the real functions once T35
//! lands.

use std::sync::Arc;

use ssg_base::PageId;
use ssg_markup::{MarkdownOptions, Toc};
use ssg_site::{RefArgs, RefLink};
use ssg_sitefuncs::Handles;
use ssg_view::{ContentRenderer, RenderScope, RenderStringOptions};
use tera::{Kwargs, State, TeraResult, Value};

fn scope(st: &State) -> TeraResult<RenderScope> {
    RenderScope::from_state(st)?.ok_or_else(|| tera::Error::message("no render scope"))
}

fn renderer(
    h: &Arc<std::sync::OnceLock<std::sync::Weak<dyn ContentRenderer>>>,
) -> TeraResult<Arc<dyn ContentRenderer>> {
    h.get()
        .and_then(std::sync::Weak::upgrade)
        .ok_or_else(|| tera::Error::message("no renderer"))
}

fn page_of(kw: &Kwargs) -> TeraResult<Option<PageId>> {
    Ok(kw.get::<Value>("page")?.and_then(|v| {
        let id = v.as_map()?.get(&tera::value::Key::Str("id"))?.as_u64()?;
        u32::try_from(id).ok().map(PageId::from_raw)
    }))
}

fn chain(e: impl std::fmt::Display) -> tera::Error {
    tera::Error::message(e.to_string())
}

/// Registers the doubles on `t`.
pub fn register(t: &mut tera::Tera, h: &Handles) {
    let r = Arc::clone(&h.renderer);
    t.register_filter("markdownify", move |v: String, _: Kwargs, st: &State| {
        let s = scope(st)?;
        let html = renderer(&r)?
            .render_markdown(&v, RenderStringOptions::default(), &s)
            .map_err(chain)?;
        Ok::<_, tera::Error>(Value::safe_string(&html))
    });
    let r = Arc::clone(&h.renderer);
    t.register_function("page_content", move |kw: Kwargs, st: &State| {
        let s = scope(st)?;
        let p = page_of(&kw)?.ok_or_else(|| tera::Error::message("page_content: no page"))?;
        let c = renderer(&r)?.content(p, s.variant, &s).map_err(chain)?;
        Ok::<_, tera::Error>(Value::safe_string(&c.html))
    });
    let r = Arc::clone(&h.renderer);
    let model = Arc::clone(&h.model);
    t.register_function("page_toc", move |kw: Kwargs, st: &State| {
        let s = scope(st)?;
        let p = page_of(&kw)?.unwrap_or(s.page);
        let f = renderer(&r)?.fragments(p, &s).map_err(chain)?;
        let site = &model.config.sites[model.pages[p].lang];
        let o = MarkdownOptions::from_config(&site.markup, false);
        let toc = Toc {
            headings: f.headings.clone(),
        };
        Ok::<_, tera::Error>(Value::safe_string(&toc.to_html(&o.toc)))
    });
    let r = Arc::clone(&h.renderer);
    t.register_function("render_shortcodes", move |kw: Kwargs, st: &State| {
        let s = scope(st)?;
        let p = page_of(&kw)?.ok_or_else(|| tera::Error::message("render_shortcodes: no page"))?;
        let src = renderer(&r)?.render_shortcodes(p, &s).map_err(chain)?;
        Ok::<_, tera::Error>(Value::safe_string(&src.markdown))
    });
    let stores = Arc::clone(&h.stores);
    t.register_function("store_set", move |kw: Kwargs, st: &State| {
        let s = scope(st)?;
        let key: String = kw.must_get("key")?;
        let value: Value = kw.must_get("value")?;
        stores.set(s.txn, s.page, &key, value);
        Ok::<_, tera::Error>(Value::from(""))
    });
    let stores = Arc::clone(&h.stores);
    t.register_function("store_get", move |kw: Kwargs, st: &State| {
        let s = scope(st)?;
        let key: String = kw.must_get("key")?;
        let p = page_of(&kw)?.unwrap_or(s.page);
        Ok::<_, tera::Error>(stores.get(s.txn, p, &key).unwrap_or_else(Value::none))
    });
    for (name, kind) in [
        ("ref", RefLink::Permalink),
        ("rel_ref", RefLink::RelPermalink),
    ] {
        let model = Arc::clone(&h.model);
        t.register_function(name, move |kw: Kwargs, st: &State| {
            let s = scope(st)?;
            let args = RefArgs {
                path: kw.must_get("path")?,
                lang: kw.get("lang")?,
                output_format: kw.get("output_format")?,
            };
            let link = model
                .ref_link(s.lang, &args, Some(s.page), kind)
                .map_err(chain)?;
            Ok::<_, tera::Error>(Value::from(link))
        });
    }
    let model = Arc::clone(&h.model);
    let views = Arc::clone(&h.views);
    t.register_function("get_page", move |kw: Kwargs, st: &State| {
        let s = scope(st)?;
        let path: String = kw.must_get("path")?;
        let from = page_of(&kw)?.or(Some(s.page));
        Ok::<_, tera::Error>(match model.get_page(s.lang, &path, from).map_err(chain)? {
            Some(id) => views.generation(s.phase, s.variant).full(id),
            None => Value::none(),
        })
    });
    t.register_filter("get_resource", |_: Value, _: Kwargs, _: &State| {
        Ok::<_, tera::Error>(Value::none())
    });
    t.register_function("get_asset", |_: Kwargs, _: &State| {
        Ok::<_, tera::Error>(Value::none())
    });
}
