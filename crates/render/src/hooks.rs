//! Render hooks through Tera (REWRITE_PLAN.md §4.2, §4.3): [`TeraHooks`] implements
//! `ssg_markup::Hooks` with the `_markup/render-<kind>[-<variant>]` templates.
//!
//! - A hook for a non-HTML output format falls back to the HTML hook.
//! - The embedded table hook's output is Go's default, which `ssg-markup` writes
//!   natively, so only a user or theme table hook is rendered.
//! - The context holds `page` and `page_inner` (the generation of the render's phase), `site`,
//!   `build`, `lang`, `__nh` and the hook's fields flattened; HTML fields (`text`, cell texts)
//!   are safe strings.

use ssg_base::FormatId;
use ssg_base::paths::ContentKey;
use ssg_layouts::{HookKind, TemplateName};
use ssg_markup::{
    BlockquoteCtx, CodeBlockCtx, HeadingCtx, HookEnv, HookError, HookOut, Hooks, LinkCtx,
    PassthroughCtx, TableCtx,
};
use ssg_site::Page;
use ssg_view::{RenderScope, SCOPE_KEY};
use tera::Value;

use crate::session::Session;
use crate::shortcode::error_chain;

/// The hooks of one page render.
pub(crate) struct TeraHooks<'a> {
    pub session: &'a Session,
    pub page: &'a Page,
    /// The lookup path (the page's key with its type as first segment).
    pub path: &'a ContentKey,
    pub format: FormatId,
    pub scope: &'a RenderScope,
}

fn cells(rows: &[Vec<ssg_markup::Cell>]) -> Value {
    Value::from(
        rows.iter()
            .map(|r| {
                Value::from(
                    r.iter()
                        .map(|c| {
                            let mut m = tera::value::Map::new();
                            m.insert("text".into(), Value::safe_string(&c.text));
                            m.insert("alignment".into(), Value::from_serializable(&c.alignment));
                            Value::from(m)
                        })
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>(),
    )
}

impl TeraHooks<'_> {
    /// The hook template for `kind`: this format's, else the HTML format's.
    fn template(&self, kind: HookKind, variant: Option<&str>) -> Option<TemplateName> {
        let s = self.session;
        let embedded = s.embedded_hooks(self.page.lang);
        let find = |f| {
            s.templates()
                .hook(kind, variant, self.path, f, self.page.lang, embedded)
        };
        find(self.format).or_else(|| {
            let html = s.html_format();
            (html != self.format).then(|| find(html)).flatten()
        })
    }

    fn run(
        &self,
        kind: HookKind,
        variant: Option<&str>,
        env: &HookEnv,
        fields: impl FnOnce(&mut tera::Context),
    ) -> Result<HookOut, HookError> {
        let Some(tpl) = self.template(kind, variant) else {
            return Ok(HookOut::Default);
        };
        if kind == HookKind::Table && self.session.is_embedded(&tpl) {
            return Ok(HookOut::Default);
        }
        let s = self.session;
        let generation = s.views().generation(self.scope.phase, self.scope.variant);
        let mut ctx = tera::Context::new();
        ctx.insert_value("page", generation.full(env.page));
        ctx.insert_value("page_inner", generation.full(env.inner_page));
        ctx.insert_value("site", generation.sites[self.page.lang].clone());
        ctx.insert_value("build", s.build_info().clone());
        ctx.insert("lang", &s.model().config.sites[self.page.lang].language.key);
        ctx.insert_value(SCOPE_KEY, self.scope.child().to_value());
        fields(&mut ctx);
        s.templates()
            .tera()
            .render(tpl.as_str(), &ctx)
            .map(HookOut::Html)
            .map_err(|e| HookError::new(format!("{tpl}: {}", error_chain(&e))))
    }

    fn link_like(&self, kind: HookKind, env: &HookEnv, c: &LinkCtx) -> Result<HookOut, HookError> {
        self.run(kind, None, env, |ctx| {
            ctx.insert("destination", &c.destination);
            ctx.insert("title", &c.title);
            ctx.insert_value("text", Value::safe_string(&c.text));
            ctx.insert("plain_text", &c.plain_text);
            ctx.insert("is_block", &c.is_block);
            ctx.insert_value("attributes", Value::from_serializable(&c.attributes));
            ctx.insert("ordinal", &env.ordinal);
            ctx.insert("position", &format!("\"{}\"", env.position));
        })
    }
}

impl Hooks for TeraHooks<'_> {
    fn link(&self, env: &HookEnv, c: &LinkCtx) -> Result<HookOut, HookError> {
        self.link_like(HookKind::Link, env, c)
    }

    fn image(&self, env: &HookEnv, c: &LinkCtx) -> Result<HookOut, HookError> {
        self.link_like(HookKind::Image, env, c)
    }

    fn heading(&self, env: &HookEnv, c: &HeadingCtx) -> Result<HookOut, HookError> {
        self.run(HookKind::Heading, None, env, |ctx| {
            ctx.insert("level", &c.level);
            ctx.insert("anchor", &c.anchor);
            ctx.insert_value("text", Value::safe_string(&c.text));
            ctx.insert("plain_text", &c.plain_text);
            ctx.insert_value("attributes", Value::from_serializable(&c.attributes));
        })
    }

    fn code_block(&self, env: &HookEnv, c: &CodeBlockCtx) -> Result<HookOut, HookError> {
        let variant = (!c.lang.is_empty()).then_some(c.lang.as_str());
        self.run(HookKind::CodeBlock, variant, env, |ctx| {
            ctx.insert("type", &c.lang);
            ctx.insert("inner", &c.inner);
            ctx.insert_value("options", Value::from_serializable(&c.options));
            ctx.insert_value("attributes", Value::from_serializable(&c.attributes));
            ctx.insert("ordinal", &env.ordinal);
            ctx.insert("position", &format!("\"{}\"", env.position));
        })
    }

    fn blockquote(&self, env: &HookEnv, c: &BlockquoteCtx) -> Result<HookOut, HookError> {
        self.run(HookKind::Blockquote, None, env, |ctx| {
            ctx.insert_value("type", Value::from_serializable(&c.kind));
            ctx.insert("alert_type", &c.alert_type);
            ctx.insert("alert_title", &c.alert_title);
            ctx.insert("alert_sign", c.alert_sign.as_str());
            ctx.insert_value("text", Value::safe_string(&c.text));
            ctx.insert_value("attributes", Value::from_serializable(&c.attributes));
            ctx.insert("ordinal", &env.ordinal);
        })
    }

    fn table(&self, env: &HookEnv, c: &TableCtx) -> Result<HookOut, HookError> {
        self.run(HookKind::Table, None, env, |ctx| {
            ctx.insert_value("thead", cells(&c.thead));
            ctx.insert_value("tbody", cells(&c.tbody));
            ctx.insert_value("attributes", Value::from_serializable(&c.attributes));
            ctx.insert("ordinal", &env.ordinal);
        })
    }

    fn passthrough(&self, env: &HookEnv, c: &PassthroughCtx) -> Result<HookOut, HookError> {
        self.run(HookKind::Passthrough, None, env, |ctx| {
            ctx.insert_value("type", Value::from_serializable(&c.kind));
            ctx.insert("inner", &c.inner);
            ctx.insert_value("attributes", Value::from_serializable(&c.attributes));
            ctx.insert("ordinal", &env.ordinal);
            ctx.insert("position", &format!("\"{}\"", env.position));
        })
    }
}
