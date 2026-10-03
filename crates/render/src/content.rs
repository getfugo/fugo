//! The content phase (C1, REWRITE_PLAN.md §3.2): per page and hook variant, (a) shortcodes →
//! [`ExpandedSource`]; (b) fragments, parse only; (c) Markdown with render hooks (HTML content
//! is passed through); (d) placeholders swapped; summary, plain text, counts, TOC.
//!
//! Every stage is a memo cell ([`crate::memo`]): cross-page requests (`page_content`,
//! `page_fragments`, `render_shortcodes`) compute what they need in the caller's scope,
//! without blocking, and a request that comes back to a cell on its own chain is a cycle.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use ssg_base::PageId;
use ssg_config::site::{EmojiPolicy, SiteConfig};
use ssg_highlight::Highlight;
use ssg_markup::{
    CodeFences, ExpandedMarkdown, Fragments, Heading, HighlightOptions, Highlighter, HookError,
    MarkdownOptions, SourceContexts,
};
use ssg_page::{Cjk, Markup};
use ssg_site::{Page, SourceFile};
use ssg_view::{
    ContentError, ExpandedSource, HookVariant, RenderScope, RenderStringOptions, RenderedContent,
    Stage,
};

use crate::hooks::TeraHooks;
use crate::memo::{Place, get_or_compute};
use crate::session::{Session, lookup_path};
use crate::shortcode::Expander;
use crate::summary::{self, DIVIDER};
use crate::tokens;

/// The Markdown options of a language.
pub(crate) fn markdown_options(site: &SiteConfig) -> MarkdownOptions {
    MarkdownOptions::from_config(&site.markup, site.emoji == EmojiPolicy::Enabled)
}

/// The highlighter of a language, built at its first fence.
struct LazyHighlight<'a> {
    cell: &'a OnceLock<Arc<Highlight>>,
    site: &'a SiteConfig,
}

impl Highlighter for LazyHighlight<'_> {
    fn highlight(&self, code: &str, lang: &str, o: &HighlightOptions) -> Result<String, HookError> {
        let h = self
            .cell
            .get_or_init(|| Arc::new(Highlight::new(&self.site.markup.highlight)));
        Highlighter::highlight(h.as_ref(), code, lang, o)
    }
}

/// `body` with its shortcode calls removed (the lexer's text only).
fn unexpanded(body: &str) -> String {
    let Ok(tokens) = ssg_pageparser::lex(body) else {
        return body.to_owned();
    };
    tokens
        .iter()
        .filter(|t| t.kind == ssg_pageparser::TokenKind::Text)
        .map(|t| t.text(body))
        .collect()
}

/// Swaps placeholders in the headings of `f`.
fn swap_headings(hs: &mut [Heading], outputs: &[Arc<str>]) {
    for h in hs {
        h.html = tokens::swap(&h.html, outputs);
        h.plain = tokens::swap(&h.plain, outputs);
        swap_headings(&mut h.children, outputs);
    }
}

fn source_of(page: &Page) -> Result<&SourceFile, ContentError> {
    page.source.as_ref().ok_or(ContentError::NoContent(page.id))
}

fn file_of(src: &SourceFile) -> Arc<Path> {
    Arc::from(src.file.abs.as_path())
}

impl Session {
    /// The scope place of a computation for `page` in variant `v`.
    fn place(&self, page: &Page, v: HookVariant) -> Place {
        Place {
            lang: page.lang,
            format: self.variant_format(v),
            variant: v,
        }
    }

    fn cycle_message(&self, chain: &[(PageId, Stage)], key: (PageId, Stage)) -> String {
        let name = |(p, s): (PageId, Stage)| {
            let page = &self.model().pages[p];
            let what = match s {
                Stage::Expand => "shortcodes".to_owned(),
                Stage::Fragments => "fragments".to_owned(),
                Stage::Content(v) => format!("content ({})", self.variant_name(v)),
            };
            let file = page
                .source
                .as_ref()
                .map_or_else(|| page.key.to_path(), |s| s.file.abs.display().to_string());
            format!("{file} {what}")
        };
        let from = chain.iter().position(|&k| k == key).unwrap_or(0);
        let path: Vec<String> = chain[from..]
            .iter()
            .copied()
            .chain(std::iter::once(key))
            .map(name)
            .collect();
        format!(
            "{} is needed while it is being computed: {}",
            name(key),
            path.join(" → ")
        )
    }

    /// Page `id`'s source after its shortcodes ran, in variant `v`.
    pub(crate) fn expanded(
        &self,
        id: PageId,
        v: HookVariant,
        scope: &RenderScope,
    ) -> Result<Arc<ExpandedSource>, ContentError> {
        let page = &self.model().pages[id];
        let src = source_of(page)?;
        let v = self.known_variant(v);
        let key = (id, Stage::Expand);
        get_or_compute(
            &self.cells().expanded[&v][id],
            key,
            scope,
            self.place(page, v),
            self.stores(),
            |chain| self.cycle_message(chain, key),
            |child| self.compute_expanded(page, src, child).map(Arc::new),
        )
    }

    fn compute_expanded(
        &self,
        page: &Page,
        src: &SourceFile,
        scope: &RenderScope,
    ) -> Result<ExpandedSource, ContentError> {
        let file = file_of(src);
        let path = lookup_path(page);
        let ex = Expander::new(
            self,
            page,
            &file,
            &src.text,
            src.body_offset,
            &path,
            scope.format,
            scope,
        );
        let body = ssg_pageparser::parse_body(src.body(), &|name: &str| ex.inner_use(name))
            .map_err(|e| {
                let (line, col) =
                    ssg_pageparser::line_col(&src.text, src.body_offset + e.span().start);
                ContentError::Render(format!("{}:{line}:{col}: {e}", file.display()))
            })?;
        ex.run(&body)
    }

    /// Page `id`'s headings and identifiers (parse only, from the `Html` expansion).
    pub(crate) fn fragments_of(
        &self,
        id: PageId,
        scope: &RenderScope,
    ) -> Result<Arc<Fragments>, ContentError> {
        let page = &self.model().pages[id];
        let src = source_of(page)?;
        if scope.chain.contains(&(id, Stage::Expand)) && page.meta.markup == Markup::Markdown {
            // Asked while the page's own shortcodes run (a TOC shortcode): the headings of the
            // body with the calls left out, not memoised.
            return self.unexpanded_fragments(page, src).map(Arc::new);
        }
        let key = (id, Stage::Fragments);
        get_or_compute(
            &self.cells().frags[id],
            key,
            scope,
            self.place(page, HookVariant::Html),
            self.stores(),
            |chain| self.cycle_message(chain, key),
            |child| {
                if page.meta.markup == Markup::Html {
                    return Ok(Arc::new(Fragments::default()));
                }
                let expanded = self.expanded(id, HookVariant::Html, child)?;
                let file = file_of(src);
                let mut f = ssg_markup::fragments(
                    &ExpandedMarkdown {
                        text: &expanded.markdown,
                        page: id,
                        contexts: &expanded.contexts,
                        file: &file,
                    },
                    self.markdown(page.lang),
                )
                .map_err(|e| ContentError::Render(e.to_string()))?;
                swap_headings(&mut f.headings, &expanded.placeholders);
                Ok(Arc::new(f))
            },
        )
    }

    /// The fragments of `page`'s body with its shortcode calls left out.
    fn unexpanded_fragments(
        &self,
        page: &Page,
        src: &SourceFile,
    ) -> Result<Fragments, ContentError> {
        let file = file_of(src);
        let text = unexpanded(src.body());
        ssg_markup::fragments(
            &ExpandedMarkdown {
                text: &text,
                page: page.id,
                contexts: &SourceContexts::default(),
                file: &file,
            },
            self.markdown(page.lang),
        )
        .map_err(|e| ContentError::Render(e.to_string()))
    }

    /// Page `id`'s rendered content in variant `v`.
    pub(crate) fn content_of(
        &self,
        id: PageId,
        v: HookVariant,
        scope: &RenderScope,
    ) -> Result<Arc<RenderedContent>, ContentError> {
        let page = &self.model().pages[id];
        let src = source_of(page)?;
        let v = self.known_variant(v);
        let key = (id, Stage::Content(v));
        get_or_compute(
            &self.cells().content[&v][id],
            key,
            scope,
            self.place(page, v),
            self.stores(),
            |chain| self.cycle_message(chain, key),
            |child| self.compute_content(page, src, v, child).map(Arc::new),
        )
    }

    fn compute_content(
        &self,
        page: &Page,
        src: &SourceFile,
        v: HookVariant,
        scope: &RenderScope,
    ) -> Result<RenderedContent, ContentError> {
        let expanded = self.expanded(page.id, v, scope)?;
        let site = &self.model().config.sites[page.lang];
        let o = self.markdown(page.lang);
        let (html, toc, fragments) = match page.meta.markup {
            Markup::Markdown => {
                let file = file_of(src);
                let path = lookup_path(page);
                let hooks = TeraHooks {
                    session: self,
                    page,
                    path: &path,
                    format: scope.format,
                    scope,
                };
                let hl = LazyHighlight {
                    cell: self.highlighter(page.lang),
                    site,
                };
                let hl: Option<&dyn Highlighter> =
                    (o.code_fences == CodeFences::Hooked).then_some(&hl);
                let r = ssg_markup::render(
                    &ExpandedMarkdown {
                        text: &expanded.markdown,
                        page: page.id,
                        contexts: &expanded.contexts,
                        file: &file,
                    },
                    o,
                    &hooks,
                    hl,
                )
                .map_err(|e| ContentError::Render(e.to_string()))?;
                (r.html, r.toc.to_html(&o.toc), r.fragments)
            }
            // Go builds a TOC only for converted markup: HTML content has none (not even
            // the empty `nav` a markdown page without headings gets).
            Markup::Html => (
                expanded.markdown.clone(),
                String::new(),
                Fragments::default(),
            ),
        };
        let html = self
            .inclusions()
            .resolve_all(&tokens::swap(&html, &expanded.placeholders));
        let toc = tokens::swap(&toc, &expanded.placeholders);
        let mut fragments = fragments;
        swap_headings(&mut fragments.headings, &expanded.placeholders);

        let cjk = page.meta.cjk == Cjk::Yes;
        let split = match summary::manual(&html, DIVIDER, page.meta.markup == Markup::Markdown) {
            Some(s) => s,
            None => match page.meta.summary.as_deref().filter(|s| !s.is_empty()) {
                Some(fm) => summary::Split {
                    summary: self.markdown_in_scope(
                        fm,
                        RenderStringOptions {
                            display_block: false,
                        },
                        scope,
                    )?,
                    content: html,
                    truncated: false,
                },
                None => summary::auto(&html, site.summary_length, cjk),
            },
        };
        let plain = summary::plain(&split.content);
        let (word_count, fuzzy_word_count, reading_time) = summary::counts(&plain, cjk);
        Ok(RenderedContent {
            html: split.content,
            summary: split.summary,
            truncated: split.truncated,
            plain,
            word_count,
            fuzzy_word_count,
            reading_time,
            table_of_contents: toc,
            fragments: Arc::new(fragments),
        })
    }

    /// Markdown rendered with the hooks of the scope's page (`markdownify`, `render_string`,
    /// the inner of a nested `{{% %}}`, a front matter summary). No shortcodes run.
    pub(crate) fn markdown_in_scope(
        &self,
        md: &str,
        o: RenderStringOptions,
        scope: &RenderScope,
    ) -> Result<String, ContentError> {
        let page = &self.model().pages[scope.page];
        let site = &self.model().config.sites[page.lang];
        let opts = self.markdown(page.lang);
        let file: Arc<Path> = page
            .source
            .as_ref()
            .map_or_else(|| Arc::from(Path::new("")), file_of);
        let path = lookup_path(page);
        let hooks = TeraHooks {
            session: self,
            page,
            path: &path,
            format: scope.format,
            scope,
        };
        let hl = LazyHighlight {
            cell: self.highlighter(page.lang),
            site,
        };
        let hl: Option<&dyn Highlighter> = (opts.code_fences == CodeFences::Hooked).then_some(&hl);
        let contexts = SourceContexts::default();
        let r = ssg_markup::render(
            &ExpandedMarkdown {
                text: md,
                page: page.id,
                contexts: &contexts,
                file: &file,
            },
            opts,
            &hooks,
            hl,
        )
        .map_err(|e| ContentError::Render(e.to_string()))?;
        Ok(if o.display_block {
            r.html
        } else {
            summary::unwrap_paragraph(&r.html)
        })
    }

    /// `render_shortcodes(page=id)`: in the content phase an inclusion token the expanding
    /// page replaces by `id`'s expanded source (placeholders renumbered, a context span for
    /// `id`); elsewhere the source with its placeholders resolved.
    pub(crate) fn shortcodes_of(
        &self,
        id: PageId,
        scope: &RenderScope,
    ) -> Result<Arc<ExpandedSource>, ContentError> {
        let src = self.expanded(id, scope.variant, scope)?;
        let markdown = if scope.phase == ssg_view::Phase::Content {
            self.inclusions().token(id, Arc::clone(&src))
        } else {
            tokens::resolve(&src)
        };
        Ok(Arc::new(ExpandedSource {
            markdown,
            placeholders: Vec::new(),
            contexts: SourceContexts::default(),
        }))
    }
}
