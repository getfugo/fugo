//! Templates: `partial`, `partial_cached`, `return_value`, `template_exists`, `defer`
//! (REWRITE_PLAN.md §4.3).
//!
//! `partial(name=, …)` renders `_partials/<name>` through the render session with the kwargs
//! as top-level names, the caller's `page`, `site`, `build`, `lang` and `output_format` (unless
//! a kwarg of that name is given) and a child `__nh`: same page, format and pager, depth + 1,
//! and a new frame. `return_value(value=)` inside it writes that frame; the call returns the
//! frame's value when it was written, else the rendered text.

use std::sync::{Arc, OnceLock, Weak};

use dashmap::DashMap;
use ssg_layouts::Templates;
use ssg_view::{ContentRenderer, Deferred, DeferredRegistry, MAX_DEPTH, Phase, SCOPE_KEY};
use tera::{Kwargs, State, TeraResult, Value};

use crate::call::{Registrar, SiteFunction, chain, msg, need_scope, renderer, templates, to_json};
use crate::{Frames, Handles, PartialResult};

type RendererSlot = Arc<OnceLock<Weak<dyn ContentRenderer>>>;
type TemplatesSlot = Arc<OnceLock<Weak<Templates>>>;

/// The context names a partial inherits from its caller.
const INHERITED: [&str; 5] = ["page", "site", "build", "lang", "output_format"];

pub(crate) fn register(r: &mut Registrar<'_>, h: &Handles) {
    let partial = || Partial {
        templates: Arc::clone(&h.templates),
        renderer: Arc::clone(&h.renderer),
        frames: Arc::clone(&h.frames),
    };
    r.function("partial", partial());
    r.function(
        "partial_cached",
        PartialCached {
            partial: partial(),
            cache: Arc::clone(&h.partial_cache),
        },
    );
    r.function(
        "return_value",
        ReturnValue {
            frames: Arc::clone(&h.frames),
        },
    );
    r.function(
        "template_exists",
        TemplateExists {
            templates: Arc::clone(&h.templates),
        },
    );
    r.function(
        "defer",
        Defer {
            templates: Arc::clone(&h.templates),
            deferred: Arc::clone(&h.deferred),
        },
    );
}

/// `partial(name=, …)`.
struct Partial {
    templates: TemplatesSlot,
    renderer: RendererSlot,
    frames: Arc<Frames>,
}

impl Partial {
    /// Renders partial `name` with the kwargs other than `skip` as its context.
    fn render(
        &self,
        fname: &str,
        kw: &Kwargs,
        st: &State,
        skip: &[&str],
    ) -> TeraResult<PartialResult> {
        let name = kw.must_get::<&str>("name")?;
        let s = need_scope(st, fname)?;
        let t = templates(&self.templates, fname)?;
        let template = t
            .partial(name)
            .ok_or_else(|| msg(format!("{fname}(name=\"{name}\"): no such partial")))?;
        let mut child = s.child();
        if child.too_deep() {
            return Err(msg(format!(
                "{fname}(name=\"{name}\"): partials nested deeper than {MAX_DEPTH}"
            )));
        }
        let mut ctx = tera::Context::new();
        for key in INHERITED {
            if let Some(v) = st.get::<Value>(key)? {
                ctx.insert_value(key, v);
            }
        }
        for (k, v) in kw.iter() {
            let k = k.as_str().unwrap_or_default();
            if k != "name" && !skip.contains(&k) {
                ctx.insert_value(k.to_owned(), v.clone());
            }
        }
        let r = renderer(&self.renderer, fname)?;
        let frame = self.frames.open(&template);
        child.frame = Some(frame);
        ctx.insert_value(SCOPE_KEY, child.to_value());
        let out = r.render_template(&template, ctx, &child);
        let returned = self.frames.close(frame);
        let out = out.map_err(|e| chain(format!("{fname}(name=\"{name}\")"), e))?;
        Ok(match returned {
            Some(v) => PartialResult::Value(v),
            None => PartialResult::Text(out),
        })
    }
}

fn value_of(r: PartialResult) -> Value {
    match r {
        PartialResult::Text(s) => Value::safe_string(&s),
        PartialResult::Value(v) => v,
    }
}

impl SiteFunction for Partial {
    fn call(&self, kw: &Kwargs, st: &State) -> TeraResult<Value> {
        self.render("partial", kw, st, &[]).map(value_of)
    }
}

/// `partial_cached(name=, key=, …)`: the whole result (text or returned value) memoised on
/// (`name`, `key`); `key` is not part of the partial's context.
struct PartialCached {
    partial: Partial,
    cache: Arc<DashMap<(String, String), PartialResult>>,
}

impl SiteFunction for PartialCached {
    fn call(&self, kw: &Kwargs, st: &State) -> TeraResult<Value> {
        let name = kw.must_get::<&str>("name")?.to_owned();
        let key = to_json(&kw.must_get::<Value>("key")?).to_string();
        let k = (name, key);
        if let Some(hit) = self.cache.get(&k) {
            return Ok(value_of(hit.clone()));
        }
        let r = self.partial.render("partial_cached", kw, st, &["key"])?;
        let r = self.cache.entry(k).or_insert(r).clone();
        Ok(value_of(r))
    }
}

/// `return_value(value=)`: the value the enclosing `partial()` returns; prints nothing.
struct ReturnValue {
    frames: Arc<Frames>,
}

impl SiteFunction for ReturnValue {
    fn call(&self, kw: &Kwargs, st: &State) -> TeraResult<Value> {
        let value = kw.must_get::<Value>("value")?;
        let s = need_scope(st, "return_value")?;
        let frame = s.frame.ok_or_else(|| {
            msg("`return_value` is only allowed in a template rendered by `partial()`")
        })?;
        self.frames.set(frame, value);
        Ok(Value::from(""))
    }
}

/// `template_exists(name=)`: a template with that Tera name or layouts path
/// (`_partials/x.html`).
struct TemplateExists {
    templates: TemplatesSlot,
}

impl SiteFunction for TemplateExists {
    fn call(&self, kw: &Kwargs, _: &State) -> TeraResult<Value> {
        let name = kw.must_get::<&str>("name")?;
        let t = templates(&self.templates, "template_exists")?;
        let store = t.store();
        let path = name.trim_start_matches('/');
        let found = store.template(name).is_some()
            || store
                .templates()
                .any(|i| i.path().trim_start_matches('/') == path);
        Ok(Value::from(found))
    }
}

/// `defer(template=, key=, data=?)`: registers `template` for phase E5 (once per `key`, the
/// first registration wins) and returns the placeholder `__nh_defer_<key>__`.
struct Defer {
    templates: TemplatesSlot,
    deferred: Arc<DeferredRegistry>,
}

impl SiteFunction for Defer {
    fn call(&self, kw: &Kwargs, st: &State) -> TeraResult<Value> {
        let template = kw.must_get::<&str>("template")?;
        let key = kw.must_get::<&str>("key")?;
        let data = kw.get::<Value>("data")?.unwrap_or_else(Value::none);
        let s = need_scope(st, "defer")?;
        if s.phase != Phase::Layout {
            return Err(msg(format!(
                "`defer` is only available in layouts (and their partials), not in {:?} renders",
                s.phase
            )));
        }
        if key.is_empty() || key.contains("__") {
            return Err(msg(format!(
                "defer(key=\"{key}\"): the key must be non-empty and must not contain `__`"
            )));
        }
        let t = templates(&self.templates, "defer")?;
        let info = t
            .store()
            .template(template)
            .ok_or_else(|| msg(format!("defer(template=\"{template}\"): no such template")))?;
        self.deferred.register(
            key,
            Deferred {
                template: Arc::from(info.render_name().as_str()),
                data,
            },
        );
        Ok(Value::safe_string(&format!("__nh_defer_{key}__")))
    }
}
