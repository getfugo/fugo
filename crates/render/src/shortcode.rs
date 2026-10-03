//! Shortcode execution: a page body → [`ExpandedSource`] (REWRITE_PLAN.md §3.2).
//!
//! - `{{% %}}` output is spliced into the Markdown; `{{< >}}` output becomes a page-local
//!   placeholder token swapped in after Markdown.
//! - Nested calls are executed first and their output is part of the parent's `inner`. The
//!   inner of the outermost `{{% %}}` is raw; a nested `{{% %}}` has its inner rendered as
//!   Markdown (a single line loses its `<p>`).
//! - `ordinal` counts per nesting level; `parent` is the enclosing call.
//! - A call without inner content whose tag is indented gets the indentation on every further
//!   line of its output, includes inserted first (Go's `shortcode.go`
//!   `doRenderShortcode`: the indentation applies to the template's result).
//! - Includes (`render_shortcodes` in the content phase) come back as inclusion tokens: in
//!   `{{% %}}` output (and the output of calls nested in a `{{% %}}` call) they are replaced by
//!   the included source with its placeholders renumbered into this page's table and a context
//!   span for the included page (top-level calls only). On a Markdown page the source is
//!   wrapped in context marker lines (`ssg_markup::wrap_context`, Go's context `Wrap`
//!   of `.RenderShortcodes` inside goldmark): they shape the blocks around the include the way
//!   Go's markers do. In a `{{< >}}` call's output the included source has its placeholders
//!   resolved, before the call's indentation, as Go renders those calls after Markdown.

use std::collections::BTreeMap;
use std::ops::Range;
use std::path::Path;
use std::sync::Arc;

use ssg_base::PageId;
use ssg_config::global::InlineShortcodes;
use ssg_layouts::{ShortcodeMiss, ShortcodeQuery, TemplateName};
use ssg_markup::{SourceContexts, strip_context_markers, wrap_context};
use ssg_page::Markup;
use ssg_pageparser::{
    Body, Closing, Delim, InnerUse, Scalar, Segment, ShortcodeArgs, ShortcodeCall,
};
use ssg_site::Page;
use ssg_view::views::ShortcodeView;
use ssg_view::{ContentError, ExpandedSource, RenderScope, RenderStringOptions, SCOPE_KEY};

use crate::session::Session;
use crate::summary::DIVIDER_SOURCE;
use crate::tokens::{self, INCLUSION};

/// The value of a shortcode argument.
fn scalar(s: &Scalar) -> tera::Value {
    match s {
        Scalar::String(v) => tera::Value::from(v.as_str()),
        Scalar::Int(i) => tera::Value::from(*i),
        Scalar::Float(f) => tera::Value::from(*f),
        Scalar::Bool(b) => tera::Value::from(*b),
    }
}

/// `inner` with the call's indentation removed from every line that starts with it (Go
/// `.InnerDeindent`).
fn deindent(inner: &str, indentation: &str) -> String {
    if indentation.is_empty() {
        return inner.to_owned();
    }
    inner
        .split_inclusive('\n')
        .map(|line| line.strip_prefix(indentation).unwrap_or(line))
        .collect()
}

/// `out` with `indentation` added before every line after the first (Go's
/// `text.VisitLinesAfter` loop), and the offsets of `out` where it was inserted.
fn indent(out: &str, indentation: &str) -> (String, Vec<usize>) {
    if indentation.is_empty() {
        return (out.to_owned(), Vec::new());
    }
    let mut s = String::with_capacity(out.len());
    let mut at = Vec::new();
    let mut offset = 0;
    for (i, line) in out.split_inclusive('\n').enumerate() {
        if i > 0 {
            s.push_str(indentation);
            at.push(offset);
        }
        s.push_str(line);
        offset += line.len();
    }
    (s, at)
}

/// The indentation Go adds to the further lines of `call`'s output: its tag's, when the call
/// has no inner content.
fn output_indentation<'c>(call: &ShortcodeCall<'c>) -> &'c str {
    match &call.closing {
        Closing::Closed { inner, .. } if !inner.is_empty() => "",
        _ => call.indentation,
    }
}

/// Go's cleanup of a one-line inner rendered as Markdown: `<p>x</p>\n` → `x`.
fn one_line(html: String) -> String {
    match html
        .strip_prefix("<p>")
        .and_then(|s| s.strip_suffix("</p>\n"))
    {
        Some(inner) if !inner.contains('\n') => inner.to_owned(),
        _ => html,
    }
}

/// One page's shortcode expansion.
pub(crate) struct Expander<'s> {
    pub session: &'s Session,
    pub page: &'s Page,
    /// The content file (the whole text, for positions).
    pub file: &'s Arc<Path>,
    pub text: &'s str,
    pub body_offset: usize,
    /// The shortcode lookup path (the page's key with its type as first segment).
    pub path: &'s ssg_base::paths::ContentKey,
    pub format: ssg_base::FormatId,
    /// The scope of the expansion's computation.
    pub scope: &'s RenderScope,
    out: String,
    placeholders: Vec<Arc<str>>,
    contexts: Vec<(Range<usize>, PageId)>,
    /// Inline shortcode templates defined so far (`name.inline`).
    inline: BTreeMap<String, String>,
    /// Whether the top-level call being expanded is a `{{% %}}` call: Go renders those (and
    /// the calls nested in them) before Markdown, so the pages they include keep their own
    /// `{{< >}}` outputs as placeholders (`page__content.go` `RenderShortcodes` with the
    /// content callback), and on a Markdown page their texts get context markers
    /// (`IsInGoldmark`).
    markdown_call: bool,
}

impl<'s> Expander<'s> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        session: &'s Session,
        page: &'s Page,
        file: &'s Arc<Path>,
        text: &'s str,
        body_offset: usize,
        path: &'s ssg_base::paths::ContentKey,
        format: ssg_base::FormatId,
        scope: &'s RenderScope,
    ) -> Self {
        Self {
            session,
            page,
            file,
            text,
            body_offset,
            path,
            format,
            scope,
            out: String::new(),
            placeholders: Vec::new(),
            contexts: Vec::new(),
            inline: BTreeMap::new(),
            markdown_call: false,
        }
    }

    fn position(&self, span: &Range<usize>) -> String {
        let (line, col) = ssg_pageparser::line_col(self.text, self.body_offset + span.start);
        format!("{}:{line}:{col}", self.file.display())
    }

    fn query(&self, name: &'s str, markdown: bool) -> ShortcodeQuery<'s> {
        ShortcodeQuery {
            name,
            path: self.path,
            kind: Some(self.page.kind),
            lang: Some(self.page.lang),
            format: self.format,
            markdown,
        }
    }

    /// Whether shortcode `name` uses its inner content.
    pub(crate) fn inner_use(&self, name: &str) -> InnerUse {
        let t = self.session.templates();
        let q = |markdown| ShortcodeQuery {
            name,
            path: self.path,
            kind: Some(self.page.kind),
            lang: Some(self.page.lang),
            format: self.format,
            markdown,
        };
        match t
            .shortcode_query(&q(false))
            .or_else(|_| t.shortcode_query(&q(true)))
        {
            Ok(tpl) => {
                if t.uses_variable(&tpl, "inner") || t.uses_variable(&tpl, "inner_deindent") {
                    InnerUse::Required
                } else {
                    InnerUse::Unused
                }
            }
            Err(ShortcodeMiss::NotFound(_)) => InnerUse::UnknownShortcode,
            Err(ShortcodeMiss::Incompatible { .. }) => InnerUse::Unused,
        }
    }

    /// Expands `body` (the page's body, parsed).
    pub(crate) fn run(mut self, body: &Body<'_>) -> Result<ExpandedSource, ContentError> {
        for (i, seg) in body.segments.iter().enumerate() {
            if body.summary_divider == Some(i) {
                self.out.push_str(DIVIDER_SOURCE);
            }
            match seg {
                Segment::Text(t) => self.out.push_str(t),
                Segment::Shortcode(call) => {
                    self.markdown_call = call.delim == Delim::Markdown;
                    let output = self.call(call, None)?;
                    let indentation = output_indentation(call);
                    match call.delim {
                        Delim::Markdown => self.splice(&output, indentation),
                        Delim::Html => {
                            // Go renders a `{{< >}}` call after Markdown, where the pages it
                            // includes have their own `{{< >}}` outputs in place: they are
                            // indented with the rest.
                            let text = self.session.inclusions().resolve_all(&output);
                            let (text, _) = indent(&text, indentation);
                            self.out
                                .push_str(&tokens::placeholder(self.placeholders.len()));
                            self.placeholders.push(Arc::from(text));
                        }
                    }
                }
            }
        }
        if body.summary_divider == Some(body.segments.len()) {
            self.out.push_str(DIVIDER_SOURCE);
        }
        Ok(ExpandedSource {
            markdown: self.out,
            placeholders: self.placeholders,
            contexts: SourceContexts(self.contexts),
        })
    }

    /// Appends `{{% %}}` output ([`Self::spliced`]) and its context spans.
    fn splice(&mut self, output: &str, indentation: &str) {
        let (text, spans) = self.spliced(output, indentation);
        let base = self.out.len();
        self.out.push_str(&text);
        self.contexts.extend(
            spans
                .into_iter()
                .map(|(r, p)| (base + r.start..base + r.end, p)),
        );
    }

    /// `{{% %}}` output (or a call's nested in one) with the inclusion tokens replaced by the
    /// included sources, their placeholders renumbered into this page's table and between
    /// context markers on a Markdown page, then indented; and the context spans of the
    /// included texts in it.
    ///
    /// The spans of a nested call's includes are dropped: the enclosing call's template can
    /// put its inner content anywhere (README, deviations).
    fn spliced(
        &mut self,
        output: &str,
        indentation: &str,
    ) -> (String, Vec<(Range<usize>, PageId)>) {
        let markdown = self.page.meta.markup == Markup::Markdown;
        let mut text = String::with_capacity(output.len());
        let mut spans: Vec<(Range<usize>, PageId)> = Vec::new();
        let mut last = 0;
        for (r, n) in tokens::find(output, INCLUSION) {
            let Some((q, src)) = self.session.inclusions().take(n) else {
                continue;
            };
            text.push_str(&output[last..r.start]);
            let offset = self.placeholders.len();
            let md = tokens::renumber(&src.markdown, offset);
            self.placeholders.extend(src.placeholders.iter().cloned());
            let (md, inner) = if markdown {
                wrap_context(&md)
            } else {
                // No markers outside Markdown (the included page's own included ones too).
                let md = strip_context_markers(&md).into_owned();
                let len = md.len();
                (md, 0..len)
            };
            let start = text.len() + inner.start;
            text.push_str(&md);
            for (span, p) in &src.contexts.0 {
                spans.push((span.start + start..span.end + start, *p));
            }
            spans.push((start..start + inner.len(), q));
            last = r.end;
        }
        text.push_str(&output[last..]);
        let (text, inserted) = indent(&text, indentation);
        let shift = |at: usize| at + indentation.len() * inserted.partition_point(|&i| i <= at);
        let spans = spans
            .into_iter()
            .map(|(r, p)| (shift(r.start)..shift(r.end), p))
            .collect();
        (text, spans)
    }

    /// The output of `call` (nested calls executed into its inner content), not yet indented
    /// ([`output_indentation`]).
    fn call(
        &mut self,
        call: &ShortcodeCall<'_>,
        parent: Option<&ShortcodeView>,
    ) -> Result<String, ContentError> {
        let position = self.position(&call.span);
        let (args, params, named) = match &call.args {
            ShortcodeArgs::None => (
                Vec::new(),
                tera::Value::from(Vec::<tera::Value>::new()),
                false,
            ),
            ShortcodeArgs::Positional(list) => {
                let args: Vec<tera::Value> = list.iter().map(scalar).collect();
                (args.clone(), tera::Value::from(args), false)
            }
            ShortcodeArgs::Named(pairs) => {
                let mut m = tera::value::Map::new();
                for (k, v) in pairs {
                    m.insert(tera::value::Key::String(k.as_str().into()), scalar(v));
                }
                (Vec::new(), tera::Value::from(m), true)
            }
        };
        let view = ShortcodeView {
            name: call.name.to_owned(),
            args,
            params,
            is_named_params: named,
            ordinal: u32::try_from(call.ordinal).unwrap_or(u32::MAX),
            parent: parent.map(|p| Box::new(p.clone())),
            // Go's `.Position` prints as `"file:line:col"` (common/text.Position).
            position: format!("\"{position}\""),
        };
        let mut inner = String::new();
        if let Closing::Closed { inner: segs, .. } = &call.closing
            && !call.inline
        {
            for seg in segs {
                match seg {
                    Segment::Text(t) => inner.push_str(t),
                    Segment::Shortcode(nested) => {
                        let out = self.call(nested, Some(&view))?;
                        let indentation = output_indentation(nested);
                        if self.markdown_call {
                            inner.push_str(&self.spliced(&out, indentation).0);
                        } else {
                            let out = self.session.inclusions().resolve_all(&out);
                            inner.push_str(&indent(&out, indentation).0);
                        }
                    }
                }
            }
            if call.delim == Delim::Markdown && parent.is_some() {
                let raw = !inner.contains('\n');
                let html = self
                    .session
                    .markdown_in_scope(
                        &inner,
                        RenderStringOptions {
                            display_block: true,
                        },
                        self.scope,
                    )
                    .map_err(|e| wrap(&position, &e))?;
                inner = if raw { one_line(html) } else { html };
            }
        }
        let deindented = deindent(&inner, call.indentation);
        let ctx = self.context(&view, &inner, &deindented);
        if call.inline {
            self.inline(call, &ctx, &position)
        } else {
            let tpl = self
                .session
                .templates()
                .shortcode_query(&self.query(call.name, call.delim == Delim::Markdown))
                .map_err(|e| ContentError::Render(format!("{position}: {e}")))?;
            self.render(&tpl, &ctx, &position)
        }
    }

    /// An inline shortcode: its inner content is its template (Tera syntax), defined by its
    /// first closed call and reused by self-closed ones.
    fn inline(
        &mut self,
        call: &ShortcodeCall<'_>,
        ctx: &tera::Context,
        position: &str,
    ) -> Result<String, ContentError> {
        if self.session.model().config.security.inline_shortcodes == InlineShortcodes::Disabled {
            return Ok(String::new());
        }
        let body = match &call.closing {
            Closing::Closed { span, .. } => {
                let body = self.text[self.body_offset + span.start..self.body_offset + span.end]
                    .to_owned();
                self.inline.insert(call.name.to_owned(), body.clone());
                body
            }
            _ => self.inline.get(call.name).cloned().ok_or_else(|| {
                ContentError::Render(format!(
                    "{position}: inline shortcode {:?} used before it is defined",
                    call.name
                ))
            })?,
        };
        self.session
            .templates()
            .tera()
            .render_str(&body, ctx, true)
            .map_err(|e| ContentError::Render(format!("{position}: {}", error_chain(&e))))
    }

    fn context(&self, view: &ShortcodeView, inner: &str, deindented: &str) -> tera::Context {
        let s = self.session;
        let generation = s.views().generation(self.scope.phase, self.scope.variant);
        let mut ctx = tera::Context::new();
        ctx.insert_value("page", generation.full(self.page.id));
        ctx.insert_value("site", generation.sites[self.page.lang].clone());
        ctx.insert_value("build", s.build_info().clone());
        ctx.insert("lang", &s.model().config.sites[self.page.lang].language.key);
        ctx.insert_value("shortcode", tera::Value::from_serializable(view));
        ctx.insert_value("inner", tera::Value::safe_string(inner));
        ctx.insert_value("inner_deindent", tera::Value::safe_string(deindented));
        ctx.insert_value(SCOPE_KEY, self.scope.child().to_value());
        ctx
    }

    fn render(
        &self,
        tpl: &TemplateName,
        ctx: &tera::Context,
        position: &str,
    ) -> Result<String, ContentError> {
        if self.scope.child().too_deep() {
            return Err(ContentError::TooDeep {
                limit: ssg_view::MAX_DEPTH,
            });
        }
        self.session
            .templates()
            .tera()
            .render(tpl.as_str(), ctx)
            .map_err(|e| {
                ContentError::Render(format!(
                    "{position}: shortcode template {tpl}: {}",
                    error_chain(&e)
                ))
            })
    }
}

/// A content error at a shortcode position.
fn wrap(position: &str, e: &ContentError) -> ContentError {
    match e {
        ContentError::Cycle(m) => ContentError::Cycle(format!("{position}: {m}")),
        _ => ContentError::Render(format!("{position}: {e}")),
    }
}

/// A Tera error with its causes.
pub(crate) fn error_chain(e: &tera::Error) -> String {
    let mut s = e.to_string();
    let mut src = std::error::Error::source(e);
    while let Some(c) = src {
        let t = c.to_string();
        if !s.contains(&t) {
            s.push_str(": ");
            s.push_str(&t);
        }
        src = c.source();
    }
    s
}
