# ssg-render

The render session (REWRITE_PLAN.md §2.6, §3.2–3.4, §4.2–4.4). **State: T34** (the content
engine) **+ T36 wiring**: the site functions are T35's `ssg_sitefuncs::register` (the T38
stubs are gone), and `ssg-build` runs the phases with the jobs of `wave1`/`wave2`, the
targets of `target` and the deferred templates of `render_deferred`.

## API

```rust
pub use ssg_nav::AliasPlan;
pub enum Job { Alias(AliasPlan), Page { page, format }, Pager { page, format, number },
               PagerAlias { page, format }, Standalone { page, format }, LanguageRedirect }
pub struct JobOrder { lang, format_rank, key_rank, sub }        // new(), lang()
pub struct Output { pub path, pub text, pub format, pub lang, pub is_html, pub order }
pub struct Project { pub vfs: Arc<Vfs>, pub layouts: Arc<LayoutStore> }
pub struct RenderOptions { pub clock: Clock }
impl Session {
    pub fn new(project, model: Arc<Model>, o: &RenderOptions) -> Result<Arc<Self>, RenderError>;
    /// T34: `new` plus extra functions registered after the site functions (tests).
    pub fn with_functions(project, model, o, extra: &dyn Fn(&mut tera::Tera, &Handles)) -> …;
    pub fn render_content(&self) -> Result<(), RenderError>;                  // C1, every variant
    pub fn freeze_views(&self) -> Result<(), RenderError>;                    // D
    pub fn render_job(&self, job: &Job) -> Result<Vec<Output>, RenderError>;  // E, pure: no I/O
    pub fn variants(&self) -> &[HookVariant]; pub fn page_stores(&self) -> &Arc<PageStores>;
    pub fn templates(&self) -> &Templates; // + model, views, diagnostics, order, wave1, wave2
    /// T36: the file a job writes without rendering it (`None`: it writes nothing).
    pub fn target(&self, job: &Job) -> Result<Option<OutputPath>, RenderError>;
    /// T36, phase E5: a `defer(...)` template with `data`, `site`, `fugo`, `__nh` (phase Deferred).
    pub fn render_deferred(&self, key: &str, d: &Deferred) -> Result<String, RenderError>;
    /// Content adapters: renders a `_content.html` source for `lang` as run `run` of
    /// `Handles::adapters` (phase Adapter; `site` without its page lists, `fugo`, `lang`).
    pub fn render_adapter(&self, path: &str, source: &str, lang: LangIdx, run: u32) -> Result<(), RenderError>;
    pub fn handles(&self) -> &Handles; // T36: the store, image queue, deferred registry, … for E4–E6
}
impl ContentRenderer for Session { content, fragments, render_shortcodes, render_markdown, render_template }
pub mod summary { manual, auto, plain, counts, unwrap_paragraph, Split, DIVIDER, DIVIDER_SOURCE }
pub enum RenderError { …, Content { page, source: Box<ContentError> } /* T34 */,
                       I18n, Vfs, Io, Deferred { key, template, source } /* T36 */,
                       Adapter { path, source } }
```

`Session::new` follows §2.6: the renderer slot (`Arc<OnceLock<Weak<dyn ContentRenderer>>>`) is
created empty → `Handles` (the session's named mutable state: page stores, pagination
recorder, deferred registry, frames, partial cache, related cache; **one `ImageQueue` with the
`[caches.images]` file cache, shared with `StoreConfig::from_config`**; **the translations of
every i18n file** of the project and its themes, lowest precedence first; **one `Highlight`**,
also used for the fences of every language whose `[markup.highlight]` is the default site's) →
`register_placeholders`, `register_pure`, `ssg_sitefuncs::register`, `extra` →
`layouts::load` → `Arc::new(Session)` → the templates slot and the renderer slot are set.

**Jobs (T36).** `wave1(lang)`: front matter aliases, pages × formats, standalone pages
(robots.txt and the sitemap index with the first language). `wave2()`: from the recorded
paginations the `page/1/` aliases (HTML formats, unless `pagination.disableAliases`) and pagers
2..N, then the language redirect of `ssg_nav::language_redirect` (`/en/` → `/`, or `/` →
`/en/` with `defaultContentLanguageInSubdir`; none with `disableDefaultLanguageRedirect`).

## Content phase (C1)

Per page with a content file (bundled content pages included) and per **hook variant**
(`Html`, plus `Format(F)` for each output format F with a `_markup/*.<F>.*` hook; a layout job
for F uses `Format(F)` if it exists):

1. **Expand** (`shortcode.rs`): `ssg_pageparser::parse_body` with the template store as
   `InnerOracle` (`uses_variable(tpl, "inner" | "inner_deindent")`), then every call is run
   through its Tera template (looked up with the variant's format, so `fmt.rss.xml` serves the
   RSS variant) with `page` (Meta full value), `site`, `fugo`, `lang`, `shortcode`
   (`ShortcodeView`: typed `args`, `params` list or map, `is_named_params`, `ordinal` per
   nesting level, `parent`, `position` — `"file:line:col"`, quoted as Go's `.Position` prints),
   `inner` / `inner_deindent` (safe) and `__nh`. Render hooks get `position` in the same form.
   - `{{% %}}` output is spliced into the Markdown; `{{< >}}` output becomes `NHSC<n>X`.
   - Nested calls run first, into the parent's `inner`. The inner of the outermost `{{% %}}`
     is raw; a nested `{{% %}}` inner is rendered as Markdown (a one-line inner loses its
     `<p>`). Go's legacy shortcode version 1 is not reproduced (D5).
   - A call without inner content whose tag is indented gets the indentation on its further
     output lines, included sources inserted first (Go indents the template's result);
     `inner_deindent` removes the call's indentation from inner lines.
   - Inline shortcodes (`security.enableInlineShortcodes`): the body is a Tera template
     (`render_str`), reused by later self-closed calls; disabled, they print nothing.
   - The summary divider becomes its own paragraph (`summary::DIVIDER_SOURCE`).
2. **Fragments** (its own memo stage): `ssg_markup::fragments` of the `Html` expansion,
   parse only, no hooks. A page's own fragments asked while its shortcodes run (a TOC
   shortcode) are parsed from the body with the calls left out and are not memoised, so
   `{{< toc >}}` on its own page is not a cycle.
3. **Render**: comrak through `ssg_markup::render` with `TeraHooks` (`hooks.rs`), HTML
   content passed through. Hooks: `_markup/render-<kind>[-<variant>]` of the variant's format,
   else the HTML format's; the embedded table hook is left to markup's native output; context
   `page`, `page_inner` (from the source-context spans), `site`, `fugo`, `lang`, `__nh` and the
   `HOOK_FIELDS` flattened (`text` and cell texts safe; `alert_sign` as `+`/`-`/``). Under
   `CodeFences::Hooked`, a fence no hook handles goes to `ssg_highlight::Highlight`
   (built per language at its first fence).
4. **Swap and derive**: placeholders swapped (a `<p>` holding only a placeholder is removed),
   in the HTML, the TOC and the fragments; summary (manual divider, else front matter
   `summary` rendered like `markdownify`, else automatic), `.Plain`, word count, fuzzy word
   count, reading time (`summary.rs`).

### Memo cells (`memo.rs`)

`ContentStore { expanded: variant → IdVec<PageId, Memo>, frags: IdVec<PageId, Memo>,
content: variant → IdVec<PageId, Memo> }`, created with the variants known after template
load. `get_or_compute(cell, key, scope, place, stores, cycle, f)`:

- a set cell returns its value; **it never blocks** (`OnceLock::get`, then `set`);
- a key already in `scope.chain` is `ContentError::Cycle`, naming the chain's files and
  stages from the repeated key back to itself (`a.md content (html) → b.md shortcodes → …`);
- otherwise the caller computes with a child scope: page = the key's page, its language, the
  variant's format, phase `Content`, `chain + key`, and a **new page-store transaction**. The
  first `set` wins and **commits** its transaction; a loser returns the winner's value and
  **discards** its writes; a failing computation discards its writes.

Cross-page requests (`page_content`, `page_fragments`, `render_shortcodes`, `markdownify` with
`page=`) all go through the `ContentRenderer` with the caller's scope.

### `render_shortcodes` contract (for T35)

`ContentRenderer::render_shortcodes(q, scope)` returns an `ExpandedSource` whose `markdown` the
site function prints as is (safe):

- in the **content phase** it is an inclusion token `NHRS<n>X`. The expanding page replaces it
  in `{{% %}}` output by q's expanded Markdown, **appends q's placeholders to its own table and
  renumbers** q's tokens, shifts q's context spans and adds a span for the included text (so
  hooks there get `page_inner = q`). On a Markdown page the included text sits between the
  context marker lines of `ssg_markup::wrap_context` (Go's context `Wrap`, which
  `.RenderShortcodes` applies inside goldmark): they end a definition list before an include,
  keep an indented include inside its container and leave goldmark's newline before
  `</dd>`/`</li>` after an include ending in a tight item, as in Go. A call nested in a
  `{{% %}}` call gets q's text the same way (Go renders the whole call before Markdown) but
  no span. In `{{< >}}` output (and a call nested in one), hook output or `markdownify` input it
  becomes q's text with q's placeholders resolved (and no markers): Go renders a `{{< >}}`
  call after Markdown, with q's `{{< >}}` outputs in place, so an indented call indents them
  too, while in `{{% %}}` output they stay placeholders and keep their lines.
- elsewhere (layouts) it is q's source with the shortcode outputs in place (no markers).

## Tests (`cargo test -p ssg-render -- --nocapture`)

The suites run with **test doubles** of the T35 functions they need (`tests/it/fakes.rs`:
`markdownify`, `page_content`, `page_toc`, `render_shortcodes`, `store_set`/`store_get`,
`ref`/`rel_ref`, `get_page`, `get_resource`, `get_asset`), registered with
`Session::with_functions`.

| Acceptance (T34 row of §8.2) | Evidence |
|---|---|
| per-page placeholder renumbering through `render_shortcodes` | `engine::includes_renumber_placeholders_and_set_page_inner` (A's `{{< >}}` before and after an include of B, B's own placeholders) |
| `{{% %}}` includes as in Go: context markers, indentation after the include | `engine::includes_are_wrapped_and_indented` (an indented include in a definition, an include after a definition list, includes ending in a tight list item and a tight definition): byte-equal to Go's goldmark converter at 44529028 on the same Markdown; `engine::includes_indent_like_go` (indented `{{< >}}` and `{{% %}}` includes of a page with multi-line `{{< >}}` output, top-level and nested in `{{< >}}` and `{{% %}}` calls): byte-equal to the Go build at 44529028 on the same site |
| `page_inner` spans | the same test: a link hook prints `page_inner.title` and `page.title` for A's own and B's included links |
| cross-page memo: cycle test, forced two-thread no-deadlock test | `engine::cycles_are_errors` (A ↔ B through `page_content`: an error naming both files; own-page TOC is not a cycle); `engine::two_threads_never_deadlock_and_commit_once` (a `Barrier` in a shortcode holds two threads inside A's and B's computations while each needs the other's fragments; both finish, with a 60 s watchdog) |
| buffered store writes committed once | the same test with both threads computing A: pointer-equal results, a counter written from the shortcode is 1; `engine::store_writes_follow_the_winner` (nothing before C1, committed after, layout writes direct) |
| hooks via Tera; JSON variant | content oracle `content` (blockquote, codeblock, heading, image, link hooks, `render-table.json.json` → `Format(json)`, `render-heading.rss.xml` → `Format(rss)`, `fmt.rss.xml` in the RSS variant): **480/483 equal, 3 accepted**; `engine::json_variant_through_a_layout_job` (a JSON layout job prints the `Format(json)` content) |
| HTML content; bundled content resources | oracle pages `/posts/markup-html`, `/posts/html-page`, `/blog/html-page` (shortcodes, divider, auto summary in HTML) and `/bundle/sub.md`, `…/sub/index.md`, `…/notes.md` (bundled pages); `engine::c1_renders_bundled_pages_and_html_content` (C1 → frozen Full value) |
| summary oracle | `summary::summary_oracle`: `oracle/page/summary/{build,adversarial}` (Go's summary of the rendered HTML of every page of the Go builds, variants and 6,000 adversarial calls): **11,537/11,537** Markdown/HTML cases equal (`.Summary`, `.Content`, `.Truncated`); 7,196 cases of external markups (AsciiDoc, RST, Pandoc, Org), non-UTF-8 input or Go panics not applicable |

`skeleton::testsite_bytes` of `ssg-build` stays **55/55 byte-identical**.

### Accepted differences (reviewed in `tests/it/oracle.rs`)

- A summary divider as the first text of a body is a divider (pageparser's `divider_at_start`):
  manual, empty summary, truncated (Go: front matter type, not truncated). 2 pages × 3
  formats.
- Go's legacy shortcode version 1 (`legacytag`, p02) is not reproduced (D5): its `{{% %}}`
  output is Markdown.

### Go rules kept

The automatic summary follows Go's counting (it decides where real summaries end): words that
look like tags or attributes do not count; a paragraph is counted without its last character
and with the `>` of the previous `</p>`; `.Truncated` is true when anything, even a newline,
follows the cut. The manual divider grows to its paragraph (walking back over white space and
`<div>` wrappers to the `<p…>` start tag) and takes one following newline.

## Notes and limits

- An include nested in another call's inner content has no context span (the enclosing
  template can put its inner content anywhere): hooks in it see the including page as
  `page_inner`. Go's markers carry the page there. An include in `{{% %}}` output nested in
  a `{{< >}}` call has no markers either (Go prints its context marker lines as text).
- Store writes of a page are made once per variant computation (each variant's cell commits
  its own transaction): a counter incremented by a shortcode counts the variants.
- C1 renders every page with a content file, also pages with `build.render = never`; a content
  error there fails the build where Go, rendering lazily, would not notice.
- The deferred template of a key renders with the default language's `site` (a `Deferred`
  records no language).
