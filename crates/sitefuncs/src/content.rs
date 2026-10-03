//! Content through the render session: `page_content` and friends, `render_shortcodes`,
//! `markdownify`, `render_string`; and `highlight`.

use std::sync::{Arc, OnceLock, Weak};

use ssg_highlight::{Highlight, OptionsArg};
use ssg_markup::{MarkdownOptions, Toc};
use ssg_view::views::FragmentsView;
use ssg_view::{
    ContentRenderer, ExpandedSource, Phase, RenderScope, RenderStringOptions, ViewCache,
};
use tera::{Kwargs, State, TeraResult, Value};

use crate::Handles;
use crate::call::{
    Registrar, SiteFilter, SiteFunction, chain, msg, need_scope, page_scope, renderer, text,
    to_data,
};

type Slot = Arc<OnceLock<Weak<dyn ContentRenderer>>>;

pub(crate) fn register(r: &mut Registrar<'_>, h: &Handles) {
    for (name, field) in [
        ("page_content", Field::Content),
        ("page_summary", Field::Summary),
        ("page_plain", Field::Plain),
        ("page_word_count", Field::WordCount),
        ("page_fragments", Field::Fragments),
        ("page_toc", Field::Toc),
        ("render_shortcodes", Field::Shortcodes),
    ] {
        r.function(
            name,
            PageContent {
                views: Arc::clone(&h.views),
                renderer: Arc::clone(&h.renderer),
                name,
                field,
            },
        );
    }
    r.filter(
        "markdownify",
        Markdown {
            views: Arc::clone(&h.views),
            renderer: Arc::clone(&h.renderer),
            render_string: false,
        },
    );
    r.filter(
        "render_string",
        Markdown {
            views: Arc::clone(&h.views),
            renderer: Arc::clone(&h.renderer),
            render_string: true,
        },
    );
    r.filter(
        "highlight",
        HighlightFilter {
            highlight: Arc::clone(&h.highlight),
        },
    );
}

#[derive(Clone, Copy)]
enum Field {
    Content,
    Summary,
    Plain,
    WordCount,
    Fragments,
    Toc,
    Shortcodes,
}

/// `page_content(page=)`, `page_summary`, `page_plain`, `page_word_count`, `page_fragments`,
/// `page_toc`, `render_shortcodes`: another page's content in the render's hook variant,
/// memoised and cycle-checked by the session through the scope's chain.
struct PageContent {
    views: Arc<ViewCache>,
    renderer: Slot,
    name: &'static str,
    field: Field,
}

/// The expanded source with its `{{< >}}` outputs put back in place of their page-local
/// tokens (`NHSC<n>X`), so the text can be spliced into another page's source.
fn inline_placeholders(e: &ExpandedSource) -> String {
    let mut out = e.markdown.clone();
    for (n, html) in e.placeholders.iter().enumerate().rev() {
        out = out.replace(&format!("NHSC{n}X"), html);
    }
    out
}

impl SiteFunction for PageContent {
    fn call(&self, kw: &Kwargs, st: &State) -> TeraResult<Value> {
        let s = page_scope(&self.views, st, kw, self.name)?;
        // The caller's scope travels with the request (its chain detects cycles).
        let req = crate::call::scope(st)?.unwrap_or_else(|| s.clone());
        let r = renderer(&self.renderer, self.name)?;
        let what = || {
            format!(
                "{}(page={})",
                self.name,
                self.views.model().pages[s.page].path()
            )
        };
        match self.field {
            Field::Fragments => {
                let f = r.fragments(s.page, &req).map_err(|e| chain(what(), e))?;
                Ok(Value::from_serializable(&FragmentsView::new(&f)))
            }
            Field::Shortcodes => {
                let e = r
                    .render_shortcodes(s.page, &req)
                    .map_err(|e| chain(what(), e))?;
                Ok(Value::safe_string(&inline_placeholders(&e)))
            }
            // In the content phase the TOC comes from the fragments stage: a `{{< toc >}}`
            // shortcode asks for the TOC of the page whose content it is part of, which the
            // fragments of its body (parsed without the calls) answer without a cycle.
            Field::Toc if req.phase == Phase::Content => {
                let f = r.fragments(s.page, &req).map_err(|e| chain(what(), e))?;
                let model = self.views.model();
                let site = &model.config.sites[model.pages[s.page].lang];
                let toc = Toc {
                    headings: f.headings.clone(),
                };
                let o = MarkdownOptions::from_config(&site.markup, false);
                Ok(Value::safe_string(&toc.to_html(&o.toc)))
            }
            f => {
                let c = r
                    .content(s.page, req.variant, &req)
                    .map_err(|e| chain(what(), e))?;
                Ok(match f {
                    Field::Content => Value::safe_string(&c.html),
                    Field::Summary => Value::safe_string(&c.summary),
                    Field::Plain => Value::from(c.plain.as_str()),
                    Field::WordCount => Value::from(c.word_count),
                    _ => Value::safe_string(&c.table_of_contents),
                })
            }
        }
    }
}

/// `markdownify` (the render's page; a single paragraph is unwrapped) and
/// `render_string(display=?, page=?)` (`display="block"` keeps the paragraph).
struct Markdown {
    views: Arc<ViewCache>,
    renderer: Slot,
    render_string: bool,
}

impl SiteFilter for Markdown {
    fn call(&self, v: Value, kw: &Kwargs, st: &State) -> TeraResult<Value> {
        let name = if self.render_string {
            "render_string"
        } else {
            "markdownify"
        };
        let md = text(&v, name)?;
        let s: RenderScope = if self.render_string {
            page_scope(&self.views, st, kw, name)?
        } else {
            need_scope(st, name)?
        };
        let child = s.child();
        if child.too_deep() {
            return Err(msg(format!(
                "{name}: render nesting deeper than {}",
                ssg_view::MAX_DEPTH
            )));
        }
        let display_block = match kw.get::<&str>("display")? {
            None | Some("" | "inline") => false,
            Some("block") => true,
            Some(other) => {
                return Err(msg(format!(
                    "render_string(display=\"{other}\"): expected \"inline\" or \"block\""
                )));
            }
        };
        let r = renderer(&self.renderer, name)?;
        let html = r
            .render_markdown(&md, RenderStringOptions { display_block }, &child)
            .map_err(|e| chain(name, e))?;
        Ok(Value::safe_string(&html))
    }
}

/// `code | highlight(lang=, options=?)`: the site's `[markup.highlight]` defaults overridden
/// by `options` (a string such as `"linenos=table,hl_lines=2"`, or a map).
struct HighlightFilter {
    highlight: Arc<Highlight>,
}

impl SiteFilter for HighlightFilter {
    fn call(&self, v: Value, kw: &Kwargs, _: &State) -> TeraResult<Value> {
        let code = text(&v, "highlight")?;
        let lang = kw.must_get::<&str>("lang")?;
        let options = kw.get::<Value>("options")?;
        let map;
        let opts = match &options {
            None => OptionsArg::None,
            Some(o) if o.is_none() => OptionsArg::None,
            Some(o) if o.as_str().is_some() => OptionsArg::Str(o.as_str().unwrap_or_default()),
            Some(o) => match to_data(o) {
                ssg_base::Value::Map(m) => {
                    map = m;
                    OptionsArg::Map(&map)
                }
                _ => {
                    return Err(msg(format!(
                        "highlight(options=): expected a string or a map, got {}",
                        o.name()
                    )));
                }
            },
        };
        let html = self
            .highlight
            .highlight_with(&code, lang, opts)
            .map_err(|e| chain("highlight", e))?;
        Ok(Value::safe_string(&html))
    }
}
