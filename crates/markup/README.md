# ssg-markup

Markdown for fugo: comrak behind an engine-neutral API (REWRITE_PLAN.md §2.4), plus the Go
implementation's passes. The first part of this file describes the crate (T22); the second
records the **T04 comrak spike** that chose the engine.

## API (T22)

```rust
pub fn render(src: &ExpandedMarkdown, o: &MarkdownOptions, h: &dyn Hooks, hl: Option<&dyn Highlighter>)
    -> Result<RenderedMarkdown, MarkupError>;               // { html, toc: Toc, fragments: Fragments }
pub fn fragments(src: &ExpandedMarkdown, o: &MarkdownOptions) -> Result<Fragments, MarkupError>;  // parse only, no hooks
pub struct ExpandedMarkdown<'a> { text, page: PageId, contexts: &SourceContexts, file: &Arc<Path> }
pub struct SourceContexts(pub Vec<(Range<usize>, PageId)>);       // innermost span → HookEnv::inner_page
pub fn wrap_context(md) -> (String, Range<usize>);  // Go's include wrapper: CONTEXT_OPEN line, md, CONTEXT_CLOSE line
pub fn strip_context_markers(s) -> Cow<str>;        // the marker lines removed (includes outside Markdown)
pub const CONTEXT_OPEN: &str, CONTEXT_CLOSE: &str;  // "{{NHCTXO}}" / "{{NHCTXC}}"
pub trait Hooks: Sync { link, image, heading, code_block, blockquote, table, passthrough }  // all default to HookOut::Default
pub struct HookEnv { page, inner_page, ordinal /* per kind, call order */, position }
pub struct HighlightOptions { options, attributes, ordinal /* the fence's code block ordinal */ }
pub trait Highlighter: Send + Sync { fn highlight(&self, code, lang, &HighlightOptions) -> Result<String, HookError>; }
pub struct Toc { headings } / Fragments { headings, identifiers } / Heading { id, level, html, plain, children }
pub mod text { strip_html, word_count, auto_summary -> Summary { html, truncated }, split_at_marker }
MarkdownOptions::from_config(&MarkupConfig, enable_emoji)
```

Contexts (`LinkCtx`/`ImageCtx`, `HeadingCtx`, `CodeBlockCtx`, `BlockquoteCtx`, `TableCtx`,
`PassthroughCtx`) are `Serialize` with the field names of `ssg_funcs::spec::HOOK_FIELDS`
(`ordinal` and `position` come from `HookEnv`). Enums replace Go's strings:
`BlockquoteKind`, `AlertSign`, `Alignment`, `PassthroughKind`, and in the options `RawHtml`,
`CodeFences`, `LineBreaks`, `TagStyle`, `StandaloneImages`, `LinkifyProtocol`.

### Pipeline

1. **Prepare** (only when needed): a first comrak parse finds code and raw HTML; outside them
   block-attribute lines are blanked (same length, positions unchanged) and passthrough spans
   replaced by `NHPT<n>X` tokens; an edit list maps parsed offsets back to the expanded source.
2. **Parse** with comrak (`strikethrough`, `tasklist`, `description_lists`, `footnotes`,
   `shortcodes` per options; `escaped_char_spans`; everything the Go implementation owns is
   off, **tables too**).
3. **Passes** (`src/passes`): the link-reference-definition sourcepos fix (paragraphs and
   setext headings); **goldmark's pipe tables** (`tables.rs`, a port of goldmark v1.7.12
   `extension/table.go`: a paragraph transformer — a delimiter line among a paragraph's lines,
   lazy continuation lines included, makes the lines from the one before it a table; a short
   header is padded, a long one means no table and stops the search; body rows are truncated
   or padded (padding without alignment); the lines before the header stay the paragraph,
   with their own inlines cut at the header row (parsed again on their own when an inline
   runs into it; hard line breaks kept, the last line's break dropped); a task item's `[ ]` is header text when the table starts on the item's first
   line; goldmark's escaped pipes (unescaped in code spans only: an autolink keeps the
   backslash); a list that only a blank line before such a table made loose is tight again
   (the table node has no blank line before it). goldmark's **`RequireParagraph` retry**
   (`tables/retry.rs`, `parser.go` `openBlocks`): a setext underline or the `:` of a list's
   first definition closes and transforms the paragraph before it; when a table takes all of
   it, the line is parsed again as a block that cannot continue a paragraph (`---` a thematic
   break, `-` an empty list item, `===` and `: x` paragraphs), otherwise the heading or term
   keeps the lines before the header and follows the table. The cells' inlines come from
   comrak: the tables are rebuilt as canonical GFM tables, one cell per goldmark cell, in a
   synthetic document followed by the page with its delimiter rows defused (for its reference
   and footnote definitions, even one right before a table), and grafted with positions
   mapped back; a table whose synthetic parse does not have the expected shape keeps
   its paragraph, and only it); **an empty
   text block for each paragraph of link reference definitions** (goldmark's
   `linkReferenceParagraphTransformer` keeps one; comrak drops the paragraph; footnote
   definitions leave none); HTML comments
   become empty nodes under `RawHtml::Omit` (Go writes nothing, but they stay siblings);
   passthrough nodes; `<…>` autolinks marked; block attributes
   (applied to the block the line follows, goldmark's rules: no blank line before, never a
   fenced code block, container depth from the `>` markers); heading attributes (the Go
   implementation's grammar, `src/attributes.rs`); goldmark definition lists (one term per
   line, per-`<dd>` tightness, only the first paragraph child unwrapped, lists split at link
   reference definitions); **context markers** (`contexts.rs`, as in the Go implementation:
   the `wrap_context` lines around an included page's text are parsed as ordinary lines, so
   they shape the blocks — a marker paragraph ends a definition list, a closing marker continues a
   paragraph lazily — then each marker becomes an empty node that takes the newline after it,
   a paragraph holding only a marker is replaced by it and the soft line break before a marker
   in a paragraph goes, except in goldmark's text blocks, which keep it: `…text\n</dd>`); block
   images; **linkify and typographer in one
   left-to-right scan** (goldmark interleaves them: a converted quote lets a link start, a
   link swallows the quote after it); heading and definition-term ids (Go's `TextPlain`
   with its first-child quirk, raw source text for entities/escapes, `base::anchor`
   `anchorize` + `Deduper`).
4. **Render** (`src/render.rs`): an iterative walk (no recursion on nesting depth) writing
   goldmark's HTML and the Go implementation's renderers (blockquote default, embedded table
   template — attributes in key order, falsy ones left out, values escaped as
   `transform.HTMLEscape` —, footnotes, alerts detected on the rendered content with the Go
   implementation's regex, code blocks, raw HTML omission). Hooks run post-order: a node that
   needs its content records the output length on entry and takes what follows on exit. Hook
   destinations and titles are the source text (as Go passes them).

### Acceptance (T22 row of §8.2), `cargo test -p ssg-markup --test it acceptance -- --nocapture`

| criterion | result |
|---|---|
| heading ids, docs corpus (convert oracle, all 6 configurations) | 1654/1654 per configuration; pages 895/895…897/897 |
| definition-term ids (`autoDefinitionTermID`, cfgs ascii, noattr) | 842/842 |
| heading ids, the Go implementation's rules | Thai, first-child quirk, entities, dedupe, setext: 16/16 (`acceptance::passes::heading_ids`); the adversarial headings of both oracles 100% |
| hook invocations (hooks oracle, 1357 conversions) | 1352/1357 identical sequences (≥ 99.6%) |
| hook fields, after typographer normalisation | 57975/58019 (99.92%; `IsBlock` 358/362, `TBody` 220/230: the dropped marker rows); `PageInner` 11249/11289 |
| TOC (tree, identifiers, 5 × `ToHTML`), all configurations | 995/995 each; `fragments()` equals `render().fragments` on every document |
| docs pages, normalised HTML | default 873/875, site 875/877, ascii 875/877, blackfriday 875/875, noattr 873/875, cjk 540/877 (the 2: typographer, below) |
| goldmark structure (`acceptance::compat`): tables (lazy lines, padding, rows, escaped pipes, tightness, task items, lines before a header, a setext underline or definition after a table, several tables), context markers, reference-definition text blocks, comments | 53/53 documents of `tests/data/compat/compat.json` byte-equal to the Go implementation's goldmark converter at 44529028 (the marker row of one dropped); the expectations are written by `tests/data/compat/mdcompat.go.txt` (recipe in its header) |
| `CodeFences::Plain` | testsite fence byte-equal; first 20 docs pages with fences 20/20; all 2036 docs `<pre>` blocks byte-equal |
| passes | deflist ids, alert title/sign, block attributes, passthrough, emoji, linkify: `acceptance::passes` |
| context spans | `acceptance::context`: includes, nesting (innermost wins) and inlines after link reference definitions |

### Accepted deviations

- **The Go implementation's context markers** shape the blocks (see the pipeline) but do not
  carry the page: spans come from `SourceContexts`. Go keeps the last context for what follows
  an include (the oracle's `Outro` link and blocks whose content contains the closing marker);
  spans do not leak. These account for the 40 `PageInner` differences.
- **The closing context marker after an include that ends with a table** is a row of empty
  cells in Go (goldmark's table transformer takes the marker line as a body row; the docs'
  `functions/resources/getmatch`, `match`, `methods/page/resources`); the row is dropped here.
  These are the 10 `TBody` differences of the hooks oracle.
- **A context marker inside code** is removed (Go would print its marker text there), and a
  URL right before an include ends there (Go's linkify takes the start of its marker into it).
  Like Go's, the markers start with `{` and end with `}`, so emphasis next to an include
  flanks the same way.
- **Tables**: the container prefixes of a task item's lines use its list's marker width
  (comrak keeps no width per task item; no docs page has a table in a task item).
- **A fence whose language has no hook and no highlighter** renders as plain
  `<pre><code class="language-x">`; Go fails the page ("no code renderer found").
- **HTML comments** are dropped under `RawHtml::Omit` (Go's behaviour); the plain goldmark
  `default` instance writes `<!-- raw HTML omitted -->` instead.
- **Typographer**: goldmark's rules are ported; 2 docs pages still differ on a closing `'`
  at the end of a line inside a paragraph (goldmark's choice there depends on state this
  port does not model).
- **Pathological nesting** (the oracle's `deep` documents: 3000 nested lists or brackets,
  1000 nested images): renders on a 2 MiB stack; 3/7 equal Go, the rest differ where comrak
  caps list and bracket nesting.
- **CJK** east-Asian line breaks and escaped spaces (cfg `cjk`) are not implemented (none of
  the three sites enables them).
- **Invalid UTF-8** reaches this crate already replaced by U+FFFD (the input is `&str`).
- **Numbers in attributes** are `Value::Int` when written without fraction or exponent
  (goldmark: always `float64`).
- **Code-block options** are always split from attributes (Chroma's option names, keys as
  written); Go does the same for its default highlighter.
- **Tables**: the embedded template writes attribute values as text (the oracle's replica
  prints `s:`-typed dumps). Table, row and cell positions are the trimmed source ranges of
  goldmark's rows and cells (goldmark's own nodes have none).
- **`TocOptions::end`** is `Option<u8>` (`None` = Go's `-1`); `fragments()` returns a
  `Result` (attribute errors); `ExpandedMarkdown` carries the content file for positions;
  `MarkdownOptions` has the extra enums above and `heading_ids: Option<Style>` (`None` =
  `autoHeadingID = false`). These are additions to the plan's sketch, not changes of meaning.
- A heading without an id has TOC level 0 (Go sets the level only with an id).

### Plan issues

- `ssg-config`'s `TocConfig::end_level` is `u8`, so Go's `endLevel = -1` (cfg
  `blackfriday` of the oracle) cannot be decoded; `TocOptions::from` maps a present value to
  `Some`. A config fix task should make it signed (or optional).

## T04 spike: decision

**comrak 0.55.0 is the engine** (workspace features `attributes`, `shortcodes`; no `bon`,
`syntect` or CLI features). pulldown-cmark stays the documented fallback behind the same API,
but it was **not evaluated**: it is neither in `Cargo.lock` nor in the offline registry cache,
and this task had no network. Nothing measured below calls for it.

Reasons:

1. **Block structure and inline parsing already match goldmark on the corpora.** Natively,
   with no fugo pass, 862/959 docs pages are equal after normalisation. Once the
   differences owned by passes that T22 writes anyway are folded (Go's comment dropping,
   typography, footnote markup), **925/959** docs pages are equal; the remaining 34 are listed
   under "residual differences" and each maps to a pass or an accepted quirk.
2. **Every feature the sites use exists in comrak or is a small AST pass**: tight definition
   lists parse (`Term\n: def`), heading attributes parse (11/11), alerts parse (278/278 on
   the docs), emoji shortcodes resolve exactly like goldmark-emoji (2007/2007 candidates),
   `codeFences = false` output is byte-identical (2036/2036 `<pre>` blocks), fenced code
   language and content equal Go's hook data (2006/2006).
3. **`sourcepos` is exact for inline nodes** (links, images, code, emphasis, raw HTML, text)
   in paragraphs, lists, tables, blockquotes, headings and definition lists — the input for
   `inner_page` spans — with one characterised comrak defect (below).
4. **Hooks fit the AST.** `NodeValue::Raw` is valid under any parent (`can_contain_type`),
   so "run the hook post-order, replace the node by raw HTML" works without tripping
   comrak's debug-build AST validation in `format_html`.
5. The gaps are all **semantic quirks of goldmark and the Go implementation** (passthrough
   delimiters, block attributes, deflist tightness, typographer and linkify heuristics), which
   any engine other than a goldmark port needs as custom passes too; switching engines would
   not remove one.

## Measurements

Inputs: the 959 `docs/content` (now `testdata/legacy-docs/content`) bodies of
`testdata/oracle/markup/convert` (the Go implementation's goldmark converter, six markup
configurations, with a stub highlighter) and `markup/hooks` (hook contexts, `site`
configuration). comrak runs natively with the options closest to each configuration
(`tests/it/comrak_spike/engine.rs`).

Normalisation (`normalize.rs`): entities decoded, attributes sorted, whitespace collapsed
outside `<pre>` and dropped next to block tags, XHTML slashes ignored, code blocks folded to
`<pre lang>text</pre>` (Chroma, the stub highlighter and `<pre><code class="language-…">`
alike), `align="x"` → `style="text-align: x"` (Go's table template), ids on `h1`–`h6`/`dt`
dropped (auto ids are a pass). Feature rows compare the feature's own elements, not pages.

### Whole documents

| input | matched / total |
|---|---|
| docs, cfg `default` (unsafe off, typographer, linkify, deflist, footnote, `attribute.title`) | 862 / 959 |
| docs, cfg `site` (same, unsafe on) | 870 / 959 |
| docs, cfg `ascii` (github-ascii ids, `attribute.block`, dt ids, image not wrapped) | 864 / 959 |
| docs, cfg `blackfriday` (xhtml, hardWraps, no typographer) | 897 / 959 |
| docs, cfg `cjk` (CJK line breaks, escaped space, custom typographer quotes — not in comrak) | 452 / 959 |
| docs, cfg `noattr` | 861 / 959 |
| docs, cfg `default`, after pass-owned folds | **925 / 959** |
| docs, cfg `site`, after pass-owned folds | **925 / 959** |

### Per feature (native comrak, no pass)

| feature | matched / total | verdict |
|---|---|---|
| definition lists `<dl>` | 145 / 175 | parse OK; **pass** for goldmark tightness and list splitting |
| definition details `<dd>` | 866 / 890 | same |
| tight `<dd>` (no `<p>`) | 866 / 889 | same (the plan's "840" is stale: 889 tight of 890) |
| heading attributes `{#id .class k=v}` | 11 / 11 | comrak parses (AST only; the renderer ignores attrs); render via heading hook/pass |
| block attributes (`attribute.block`, cfg ascii) | 0 / 7 consumed | **pass**; comrak keeps the line as paragraph text (7/7, incl. lazy continuation into a blockquote paragraph) |
| fence language | 2006 / 2006 | OK (Go hooks fenced blocks only; indented code is not hooked) |
| fence content (`Inner`, chomped) | 2006 / 2006 | OK |
| fence attributes `{k=v …}` via comrak's parser | 236 / 237 | **pass**: parse the raw info string with the Go implementation's grammar (fails on `hl_lines=[3, "6-8"]`, values untyped) |
| math `$$ … $$` verbatim | 7 / 7 | by accident only; **pass** |
| math `\[ … \]` verbatim | 1 / 6 | **pass** (`\[` is a CommonMark escape; content gets mangled) |
| math `\( … \)` verbatim | 0 / 3 | **pass** |
| GitHub alerts (type, title, sign) | 278 / 278 | docs covered; comrak knows 5 types and no `+`/`-` sign → **pass** for Go semantics |
| regular blockquotes with alerts on | 34 / 34 | OK |
| emoji `:name:` | 1965 / 2007 resolve; 2007 / 2007 same as goldmark-emoji | OK (only encoding differs: Unicode vs `&#x…;`) |
| links, all `<a>` | 4865 / 4924 | differences are footnote markup and linkify |
| linkify (bare URL / `www.` / email anchors) | 60 / 73 (+16 comrak-only) | **pass** for parity (see residuals) |
| typographer (quotes, dashes, ellipsis) | 3533 / 3567 (pages 362 / 425) | **pass** for parity (goldmark heuristics) |
| raw HTML omitted (`unsafe = false`) | 4 / 4 comments | OK, plus the comment-drop pass |
| raw HTML passed (`unsafe = true`) | 22 / 22 tags/comments | OK |
| `codeFences = false`, byte-exact `<pre>` | 2036 / 2036 | OK: `CodeFences::Plain` = comrak's default code block |

### `sourcepos` of inline nodes (docs)

| node | exact |
|---|---|
| link: paragraph / list / table / deflist / blockquote / footnote | 1586/1586, 552/552, 2045/2045, 498/498, 180/180, 5/5 |
| image | 31 / 31 |
| code span, emphasis, raw inline HTML | 7606/7606, 478/478, 8/8 |
| text (no escapes/entities/typography) | 19597 / 19597 |
| **inlines of a paragraph that began with link reference definitions** | links 0 / 15, text 20 / 81 |

Columns are UTF-8 byte columns (1-based, end inclusive), so `line_start[line-1] + col - 1` is
the byte offset; no tab or CRLF case occurs in the corpora. **Defect:** when comrak strips
link reference definitions from the start of a paragraph and text remains (e.g. an invalid
definition such as `[x]: {{% eturl … %}}`), the remaining inlines keep lines relative to the
paragraph start: they are shifted **up by the number of removed lines** (columns correct).
Probe: `"[a]: /x\n[b]: {{% y %}}\ntext [a] more\n"` reports `text` at 2:1 instead of 3:1.
T22 must correct this (re-scan the paragraph's source for leading definitions and add the
line count) or upstream a fix; everywhere else positions can be trusted.

### Residual differences (the 34 pages after folds, cfg `default`, by first difference)

| cause | pages | pass / action |
|---|---|---|
| deflist: only the **first** paragraph of a tight `<dd>` is unwrapped (goldmark `ReplaceChild` loop stops after one); tightness = "no blank line before the `:` line" per `<dd>` | 12 | deflist pass |
| deflist: goldmark ends the `<dl>` at a link reference definition line; comrak continues it | 6 | deflist pass (split where a non-blank, node-less line sits between items) |
| linkify: goldmark keeps a trailing `'`/`"` in the URL (`https://example.org/'`) | 9 | linkify pass |
| linkify: goldmark does not linkify after `:` (`WORK:jsmith@example.org`) | 1 | linkify pass |
| table: cells padded for a short row get alignment none | 3 | table hook data |
| table: goldmark accepts a delimiter row with more cells than the header (QR.md) | 1 | accept (GFM-correct in comrak) or pre-pass |
| table as a lazy-continuation line inside a list item (21yunbox) | 1 | accept |
| list tightness (codeberg: blank line before an indented table in the last item) | 1 | accept, or goldmark tightness recompute |

## Custom passes T22 must write

Passes run on the comrak AST between `parse_document` and `format_html`; hooked nodes are
replaced by `NodeValue::Raw`.

1. **Heading ids and fragments**: the Go implementation's `github` / `github-ascii` /
   `blackfriday` anchorize, per-document dedupe, the `TextPlain` first-child quirk (spec
   markdown.md §7), TOC. Do not use comrak's `header_id_prefix` (different Unicode tables,
   emits an `<a class="anchor">`).
2. **Definition-term ids** (`autoDefinitionTermID`, cfgs ascii/noattr and the docs site).
3. **Definition lists, goldmark semantics**: per-`<dd>` tightness from the blank line before
   the `:` marker; unwrap only the first paragraph of a tight `<dd>` (comrak's tight render also
   glues a tight `<dd>`'s paragraphs without a separator, e.g. `texta|b`); split a list that
   comrak continued across link reference definitions.
4. **Block attributes** (`attribute.block`): a trailing `{…}` line of a paragraph (also a
   lazy-continuation line inside a blockquote) is removed and applied to the paragraph or the
   preceding block; after a GFM table the line would become a table row. The Go
   implementation's attribute grammar, not comrak's.
5. **Fence info → options/attributes**: parse the raw `NodeCodeBlock::info` (keep
   `fenced_code_attributes` off) with the Go implementation's grammar: commas, arrays, typed
   values, options vs attributes split.
6. **Passthrough / math**: a pre-parse scan for the configured delimiters (`\[…\]`, `$$…$$`,
   `\(…\)`) outside code, replacing each span by a same-length placeholder so `sourcepos`
   stays valid, then a post-parse swap to a passthrough node / hook. Keep `math_dollars` off
   (it also turns `$…$` into math, which Go does not).
7. **Alerts, Go semantics**: keep comrak `alerts` off; detect `[!type]`, sign and title on
   the first paragraph of a blockquote (the Go implementation's regex), lowercase the type,
   and drop the first line from the hook text. Without a blockquote hook the output is a plain
   blockquote (what the oracle shows).
8. **Typographer (goldmark rules)**: `parse.smart` off and goldmark's typographer over text
   nodes (spec §8.4, per-block quote counters, configurable substitutions incl. empty ones,
   entity output). Needed for the 63 docs pages
   whose substitution sequence differs.
9. **Linkify (goldmark rules)**: `extension.autolink` off and goldmark's linkify over text
   nodes (spec §8.5): trigger characters, trailing-punctuation trimming, `linkifyProtocol`
   for `www.` (comrak writes `http://`, Go's default is `https`).
10. **Raw HTML under `unsafe = false`**: drop HTML comments (block and inline) instead of
    `<!-- raw HTML omitted -->` (the Go implementation's context-marker renderer). Prototyped
    in `engine::to_html_passes`.
11. **`sourcepos` fix** for paragraphs that began with link reference definitions (above).
12. **Renderers the Go implementation owns** (hooks or default templates, not passes on the
    tree): tables (thead/tbody data, alignment none for padded cells), blockquote default
    (`…</p></blockquote>`), footnotes (`fn:1`/`fnref:1` ids, `↩︎`, `div.footnotes`),
    emoji as `&#x…;` entities if byte parity matters, `wrapStandAloneImageWithinParagraph =
    false` (block images), HTML5 void tags (comrak writes `<br />`, `<img … />`).
13. Not needed by the sites: CJK east-Asian line breaks and escaped space (cfg `cjk`).

## Risks for T22

- The `sourcepos` defect above silently gives the wrong `inner_page` for inlines of such
  paragraphs; test it explicitly.
- comrak's `format_html` validates the AST in debug builds; every pass must leave a valid
  tree (`Raw` is always allowed).
- comrak parses heading attributes with its own grammar (bare values limited to
  `[A-Za-z0-9-_:.%]`, no commas or arrays); it matched all 11 docs headings, but the Go
  implementation's grammar should be the one T22 applies (re-parse the heading's trailing
  `{…}`).
- Typographer and linkify passes are ports of goldmark heuristics (spec §8.4–8.5); without
  them 63 docs pages differ in quotes.
- The oracles cover no configuration with passthrough or `enableEmoji`, so those two verdicts
  rest on the spec and on goldmark-emoji's table, not on the Go implementation's HTML.
- comrak upgrades: the harness asserts floors at the measured values, so a regression fails
  `cargo test -p ssg-markup`.

## Re-running the harness

```sh
export CARGO_TARGET_DIR=<main checkout>/target CARGO_BUILD_JOBS=4 CARGO_INCREMENTAL=0
cargo test --offline -p ssg-markup --test it comrak_spike -- --nocapture   # ~30 s, prints the tables
FUGO_SPIKE_SHOW='definition lists' cargo test ...   # print differing items of one row (label substring)
FUGO_GOLDMARK_EMOJI_TSV=/path/goldmark-emoji.tsv cargo test ...   # enable the emoji comparison
```

The emoji table (`shortname<TAB>hex code points`) comes from goldmark-emoji v1.0.6 (the version
in the repository's `go.mod` at 44529028), resolved for every `:name:` in the docs:

```go
// go run . < names.txt > goldmark-emoji.tsv   (module requiring github.com/yuin/goldmark-emoji v1.0.6)
defs := definition.Github()
for sc := bufio.NewScanner(os.Stdin); sc.Scan(); {
    if e, ok := defs.Get(strings.TrimSpace(sc.Text())); ok { /* print name, "%x" of each rune in e.Unicode */ }
}
```

with `names.txt` from `grep -rhoE ':[a-z0-9_+-]+:' testdata/legacy-docs/content --include=*.md | tr -d : | sort -u`.
