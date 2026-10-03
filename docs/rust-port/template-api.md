# Template API

<!-- GENERATED from ssg_funcs::spec (crates/funcs/src/spec.rs); do not edit.
     Regenerate: INSTA_UPDATE=always cargo test -p ssg-testkit contract -->

Templates are Tera 2.4.0 ([REWRITE_PLAN.md](REWRITE_PLAN.md) §4). Kind codes: `bi` Tera built-in, `tc` tera-contrib, `F` native filter, `fn` native function, `T` native test; `(s)` site-bound (needs the site model or the render scope). Phase: `both`, or the only phase the name works in. `=?` marks an optional kwarg, `…` any further kwargs.

## Render contexts

| Render | Top-level names |
|---|---|
| Layout job | `page`, `site`, `build`, `lang`, `output_format`, `__nh` |
| Shortcode | `page`, `site`, `build`, `lang`, `shortcode`, `inner`, `inner_deindent`, `__nh` |
| Render hook | `page`, `page_inner`, `site`, `build`, `lang`, `__nh`, plus the hook's fields, flattened (below) |
| `partial(name=…, …)` | `page`, `site`, `build`, `lang`, `output_format`, `__nh`, plus the call's kwargs as top-level names |
| Component | only its declared arguments; `@page`, `@site`, `@build`, `@lang` and `@__nh` may be declared as implicit arguments (looked up in the caller's scope) |
| `defer` template | `data`, `site`, `build`, `__nh` |
| `execute_as_template` | `data`, `site`, `build`, `__nh` |
| Alias | `permalink`, `page`, `site`, `build` |
| Sitemap, robots, 404 | `page`, `site`, `build`, `lang`, `__nh` |
| Sitemapindex | `page`, `site`, `build`, `lang`, `__nh`, `sites` |
| Content adapter (`content/**/_content.html`) | `site`, `build`, `lang`, `__nh`, `site` has no page lists (`home`, `pages`, `regular_pages`, `all_pages`, `sections`, `main_sections`, `taxonomies`, `menus`): the model is not built yet |

| Name | Meaning |
|---|---|
| `page` | the full page value of the Full generation |
| `site` | `SiteView` of the current language |
| `build` | `BuildView`: version, environment, generator |
| `lang` | the language code of the page |
| `output_format` | `OutputFormatView` being rendered |
| `__nh` | the render scope (`RenderScope`); read by site-bound functions |
| `shortcode` | `ShortcodeView`: name, args, params, is_named_params, ordinal, parent, position |
| `inner` | the inner content (safe) |
| `inner_deindent` | the inner content without common indentation |
| `page_inner` | the page whose source holds the hooked node (differs inside `render_shortcodes`) |
| `data` | the `data=` value of the call |
| `permalink` | the target URL |
| `sites` | `[{language, sitemap_abs_url, last_mod}]` |

Flattened render-hook fields:

| Hook | Fields |
|---|---|
| link, image | `destination` `title` `text` `plain_text` `is_block` `attributes` `ordinal` `position` |
| heading | `level` `anchor` `text` `plain_text` `attributes` |
| codeblock | `type` `inner` `options` `attributes` `ordinal` `position` |
| blockquote | `type` `alert_type` `alert_title` `alert_sign` `text` `attributes` `ordinal` |
| table | `thead` `tbody` `attributes` `ordinal` |
| passthrough | `type` `inner` `attributes` `ordinal` `position` |

## Syntax that replaces Go-template functions

| Go | Tera |
|---|---|
| `and` `or` `not` `eq` `ne` `lt` `le` `gt` `ge` | `and` `or` `not` `==` `!=` `<` `<=` `>` `>=`; pages compare by `.id`, pagers by `.page_number`, dates by `.unix` |
| `cond c a b` | `a if c else b` |
| `print`, `printf` | `~`, plus the filters `pad_start`, `pad_end`, `round`, `format_number`, `jsonify`; a literal U+00A0 character for `%c` of 160; `'"' ~ x ~ '"'` for `%q` in attributes |
| `dict`, `slice` | map and array literals `{"k": v}`, `[a, b]`; computed keys: `[[k, v]] \| from_pairs` |
| `index m k` | `m[k]`, `m.k` |
| `in`, `strings.Contains` | `x in l`, `"x" in s`; pages `p.id in [q.id for q in l]` |
| `isset m "k"` | `"k" in m`, `is defined` |
| `first N`, `last N`, `after N` | `l[:N]`, `l[-N:]`, `l[N:]` |
| `where` | `[p for p in pages if p.params.x == v]`; `in`/`intersect` via ids; `like` via `is matching(pat=)` |
| `apply l "float" "."` | `[x \| float for x in l]` |
| `newScratch`, `.Scratch.*` | `{% set %}`, `{% set_global %}`, `merge` |
| `add` `sub` `mul` `div` `mod` | `+ - * / %`; `//` for Go's integer `div` |
| `.GetTerms "tags"` | `page.terms.tags` |
| `.Data.Singular/Plural/Term/Terms` | `page.taxonomy.singular/plural/terms`, `page.term.term` |
| `.OutputFormats.Get "rss"`, `.AlternativeOutputFormats`, `.MediaType` | `page.output_formats.rss`, `page.alternative_output_formats`, `f.media_type.type` |
| The site-info object's `Version` / `Environment` / `IsProduction` / `IsDevelopment` / `IsServer` / `Generator` | `build.version` (fugo's version), `build.environment`, `build.is_production`, `build.is_development`, `build.is_server`, `build.generator` |
| `.Site.ServerPort` | `site.server_port` (the base URL's port, 0 without one) |
| `.Site.Config.Privacy.*` | `site.config.privacy.*` |
| `.Data.Integrity`, `.Width`, `.Height` | `r.data.integrity`, `r.width`, `r.height` |
| `partial "x" .` (shares the context) | `{% include "_partials/x.html" %}` |
| `partial "x" (dict …)` with a literal name | a component defined in `_partials/` (`{% component x(page, sep="/", @lang) %}`), called as `{{ <x page={page} /> }}` |
| `debug.Timer` | removed |
| `css.TailwindCSS` | removed: run Tailwind's standalone CLI next to fugo (`tailwindcss -i assets/css/in.css -o assets/css/site.css --watch`) and use its output as an asset |
| `babel`, `js.Babel` | removed: `js_build` compiles TypeScript and JSX and lowers modern JavaScript for the browser targets |
| `postCSS`, `css.PostCSS` | removed: `minify` adds vendor prefixes for the site's browserslist and minifies; `purge_css` purges per page |

## Logic, math and errors

| Call | Kind | Phase | Safe | Go | Description |
|---|---|---|---|---|---|
| `throw(message=)` | bi | both |  |  | Aborts the render at once with `message`. (message: string) |
| `log_error(message=)` | fn | both |  | `errorf`, `erroridf` | Records an error with the template position; the build fails when it ends. Prints nothing. (message: string) |
| `log_warn(message=, id=?)` | fn | both |  | `warnf`, `warnidf` | Records a warning; `id` lets `ignoreLogs` suppress it. Prints nothing. (message: string, id: string) |
| `max(values=)` | fn | both |  | `math.Max` | The largest of `values` (numbers). (values: array) |
| `min(values=)` | fn | both |  | `math.Min` | The smallest of `values` (numbers). (values: array) |
| `x \| abs` | bi | both |  | `math.Abs` | Absolute value. |
| `x \| round(method=?, precision=?)` | bi | both |  | `math.Round`, `math.Ceil`, `math.Floor` | `method` is `common` (default), `ceil` or `floor`; `precision` digits after the point. (method: string, precision: int) |
| `x \| int(base=?)` | bi | both |  | `int` | Converts to an integer; strings are parsed in `base` (default 10). (base: int) |
| `x \| float` | bi | both |  | `float` | Converts to a float. |
| `x \| str` | bi | both |  | `string` | Converts to a string. |

## Collections and maps

| Call | Kind | Phase | Safe | Go | Description |
|---|---|---|---|---|---|
| `x \| length` | bi | both |  | `len` | Length of a string (characters), array or map. |
| `x \| default(value=, boolean=?)` | bi | both |  |  | `value` when the input is undefined (only then; `boolean=true` also replaces falsy values). Not Go's `default`. (value: any, boolean: bool) |
| `x \| default_if_empty(value=)` | F | both |  | `default` | Go's `default`: `value` when the input is undefined, none, 0, "", or an empty array or map. `false` counts as set. (value: any) |
| `x \| get(key=, default=?)` | bi | both |  | `index` | The map entry `key`, else `default`, else an error. (key: string, default: any) |
| `x \| get_path(path=)` | F | both |  | `index m "a" "b"` | Walks `path` (keys and integer indices); none when a step is missing. (path: array) |
| `x \| first` | bi | both |  | `index l 0` | The first element, or none. |
| `x \| last` | bi | both |  |  | The last element, or none. |
| `x \| nth(n=)` | bi | both |  | `index l n` | The element at index `n`, or none. (n: int) |
| `x \| keys` | bi | both |  |  | The keys of a map. |
| `x \| values` | bi | both |  |  | The values of a map. |
| `x \| pairs` | bi | both |  |  | `[key, value]` pairs of a map. |
| `x \| sort_keys` | F | both |  | `(range over a map)` | The map with its keys sorted; Go ranges maps in key order, Tera literals keep insertion order. |
| `x \| from_pairs` | F | both |  | `dict $k $v (computed keys)` | A map from `[key, value]` pairs (the inverse of `pairs`), for keys a map literal cannot compute: `[[k, v]] \| from_pairs`. Keys are stringified, a later pair wins, keys sorted. |
| `x \| append(value=)` | F | both |  | `append` | The array with `value` appended. (value: any) |
| `x \| concat(with=)` | F | both |  | `append l1 l2` | The array followed by the elements of `with`. (with: array) |
| `x \| merge(with=)` | F | both |  | `merge` | Deep merge of two maps; `with` wins; keys sorted. (with: map) |
| `x \| join(sep=?)` | bi | both |  | `delimit` | Joins with `sep`. (sep: string) |
| `x \| delimit(sep=, last=?)` | F | both |  | `delimit l sep last` | Joins with `sep`, and `last` before the final element. (sep: string, last: string) |
| `x \| reverse` | bi | both |  | `.Reverse` | Reversed array or string. |
| `x \| unique` | bi | both |  | `uniq` | Removes duplicates, keeping the first. |
| `x \| sort(attribute=?)` | bi | both |  |  | Tera's sort (by value, or by `attribute` path); not locale-aware. Use `sort_by` for Go's `sort`. (attribute: string) |
| `x \| group_by(attribute=)` | bi | both |  |  | Tera's grouping by `attribute` path into a map. (attribute: string) |
| `x \| sort_by(attribute=, reverse=?)` | F | both |  | `sort` | Sorts by `attribute` (a path such as `params.weight`; `""` or `value`: the elements): collation of the render's `lang`, dates as instants, stable. (attribute: string, reverse: bool) |
| `x \| complement(without=)` | F | both |  | `complement` | Elements not in `without` (pages compared by id). (without: array) |
| `x \| union(with=)` | F | both |  | `union` | Elements of either array, first occurrence kept (pages by id). (with: array) |
| `x \| intersect(with=)` | F | both |  | `intersect` | Elements present in both (pages by id). (with: array) |
| `x \| symdiff(with=)` | F | both |  | `symdiff` | Elements present in exactly one (pages by id). (with: array) |
| `range(start=?, end=, step_by=?)` | bi | both |  | `seq` | Integers from `start` (default 0) to `end` (exclusive) by `step_by`; Go's `seq N` is `range(start=1, end=N+1)`. (start: int, end: int, step_by: int) |
| `querify(params=)` | fn | both |  | `querify` | A URL query string from `params`, keys sorted. (params: map) |

## Pages, taxonomies, menus and pagination

| Call | Kind | Phase | Safe | Go | Description |
|---|---|---|---|---|---|
| `get_page(path=, lang=?, page=?)` | fn (s) | both |  | `site.GetPage`, `.GetPage` | The full value of the page at `path` (relative paths resolve against `page`), or none. (path: string, lang: string, page: page) |
| `x \| deref` | F (s) | both |  |  | The full value (with relations) of a listed summary page. |
| `get_terms(taxonomy=, page=?)` | fn (s) | both |  | `.GetTerms` | The term links of `page` for `taxonomy`; `[]` when it is not a taxonomy. (taxonomy: string, page: page) |
| `related(pages=, page=?, indices=?, limit=?)` | fn (s) | both |  | `.Related` | Pages related to `page` among `pages`, per the `related` config. (pages: array, page: page, indices: array, limit: int) |
| `param(key=, page=?)` | fn (s) | both |  | `.Param` | The page param `key` (a dotted path), else the site param. (key: string, page: page) |
| `paginator()` | fn (s) | layout |  | `.Paginator` | The pager of the current (page, format) over its default list. Recorded: the first call wins. |
| `paginate(pages=, size=?)` | fn (s) | layout |  | `.Paginate` | The pager over `pages`. A re-call with another list or size is an error naming both positions. (pages: array, size: int) |
| `store_set(key=, value=, page=?)` | fn (s) | both |  | `.Store.Set` | Sets `key` in the page store (content-phase writes are buffered per transaction); in a content adapter without `page=`, in the adapter's store, which its runs for every language share. Prints nothing. (key: string, value: any, page: page) |
| `store_get(key=, page=?)` | fn (s) | both |  | `.Store.Get` | Reads `key` from the page store (in a content adapter without `page=`: the adapter's store), or none. (key: string, page: page) |
| `add_page(page=)` | fn (s) | adapter |  | `.AddPage (content adapter)` | Adds the page `page` describes to the adapter's directory: `path` (relative, required but for `kind: home`), `kind` (default `page`), `title`, `content` (`{mediaType, value}`, default Markdown), `dates` (`{date, lastmod, publishDate, expiryDate}`), `params`, `build`, `cascade`, `outputs` and the other front matter fields (not `lang`, `content.markup`). A path added again replaces the earlier page. Prints nothing. (page: map) |
| `add_resource(resource=)` | fn (s) | adapter |  | `.AddResource (content adapter)` | Adds the page resource `resource` describes: `path` (relative to the adapter's directory, required), `content` (`{mediaType, value}`: a string, or a resource, which keeps its own URL), `name`, `title`, `params`. A path added again replaces the earlier resource. Prints nothing. (resource: map) |
| `enable_all_languages()` | fn (s) | adapter |  | `.EnableAllLanguages (content adapter)` | Runs the adapter for every language, not only its own (the runs share the adapter's store). Prints nothing. |
| `page_content(page=)` | fn (s) | both | yes | `.Content (another page`, `content phase)` | The rendered content of `page`; memoised and cycle-checked. (page: page) |
| `page_summary(page=)` | fn (s) | both | yes | `.Summary (content phase)` | The summary of `page`. (page: page) |
| `page_plain(page=)` | fn (s) | both |  | `.Plain (content phase)` | The content of `page` as plain text. (page: page) |
| `page_word_count(page=)` | fn (s) | both |  | `.WordCount (content phase)` | The word count of `page`. (page: page) |
| `page_fragments(page=)` | fn (s) | both |  | `.Fragments (content phase)` | `{headings, identifiers}` of `page`. (page: page) |
| `page_toc(page=)` | fn (s) | both | yes | `.TableOfContents (content phase)` | The table of contents of `page`. (page: page) |
| `render_shortcodes(page=)` | fn (s) | both | yes | `.RenderShortcodes` | The source of `page` with its shortcodes expanded (placeholders renumbered). (page: page) |
| `is_menu_current(menu=, entry=, page=?)` | fn (s) | both |  | `.IsMenuCurrent` | Whether `entry` of `menu` points at `page`. (menu: string, entry: map, page: page) |
| `has_menu_current(menu=, entry=, page=?)` | fn (s) | both |  | `.HasMenuCurrent` | Whether a child of `entry` of `menu` points at `page`. (menu: string, entry: map, page: page) |
| `x \| by_title` | F (s) | both |  | `.ByTitle` | Pages by title (collation of the language). |
| `x \| by_link_title` | F (s) | both |  | `.ByLinkTitle` | Pages by link title. |
| `x \| by_date` | F (s) | both |  | `.ByDate` | Pages by date, oldest first. |
| `x \| by_publish_date` | F (s) | both |  | `.ByPublishDate` | Pages by publish date. |
| `x \| by_lastmod` | F (s) | both |  | `.ByLastmod` | Pages by last modification. |
| `x \| by_weight` | F (s) | both |  | `.ByWeight` | Pages by Go's default page order (weight, date, link title, path). |
| `x \| group_by_date(format=, attribute=?, order=?)` | F (s) | both |  | `.GroupByDate` | `[{key, pages}]` grouped by the date formatted with `format` (strftime), newest first (`order="asc"`: oldest first); no date is Go's zero date (`0001`). (format: string, attribute: string, order: string) |
| `x \| group_by_param(param=)` | F (s) | both |  | `.GroupByParam` | `[{key, pages}]` grouped by the page param `param`. (param: string) |
| `x \| by_count` | F (s) | both |  | `.ByCount` | Taxonomy terms by page count, then lower-cased name in Go's string order. |
| `x \| alphabetical` | F (s) | both |  | `.Alphabetical` | Taxonomy terms by name (collation). |

## Strings

| Call | Kind | Phase | Safe | Go | Description |
|---|---|---|---|---|---|
| `x \| lower` | bi | both |  | `lower` | Lower case. |
| `x \| upper` | bi | both |  | `upper` | Upper case. |
| `x \| capitalize` | bi | both |  |  | First character upper, the rest lower. |
| `x \| title` | bi | both |  |  | Tera's naive title case. Go's `title` is `title_case`. |
| `x \| title_case(style=?)` | F | both |  | `title`, `strings.Title` | Title case in `style` (`ap`, `chicago`, `go`, `firstupper`, `none`; default: `titleCaseStyle`). (style: string) |
| `x \| wordcount` | bi | both |  |  | Tera's word count (whitespace split). |
| `x \| trim(pat=?)` | bi | both |  | `strings.TrimSpace` | Trims whitespace, or the string `pat` repeatedly. (pat: string) |
| `x \| trim_start(pat=?)` | bi | both |  |  | Trims leading whitespace, or `pat`. (pat: string) |
| `x \| trim_end(pat=?)` | bi | both |  |  | Trims trailing whitespace, or `pat`. (pat: string) |
| `x \| trim_chars(chars=)` | F | both |  | `trim`, `strings.Trim` | Trims any of the characters in `chars` from both ends. (chars: string) |
| `x \| trim_start_chars(chars=)` | F | both |  | `strings.TrimLeft` | Trims any of `chars` from the start. (chars: string) |
| `x \| trim_end_chars(chars=)` | F | both |  | `strings.TrimRight` | Trims any of `chars` from the end. (chars: string) |
| `x \| strip_prefix(prefix=)` | F | both |  | `strings.TrimPrefix` | Removes `prefix` once. (prefix: string) |
| `x \| strip_suffix(suffix=)` | F | both |  | `strings.TrimSuffix` | Removes `suffix` once. (suffix: string) |
| `x \| replace(from=, to=)` | bi | both |  | `replace`, `strings.Replace` | Replaces every `from` with `to`. (from: string, to: string) |
| `x \| split(pat=)` | bi | both |  | `split` | Splits on `pat`. (pat: string) |
| `x \| regex_replace(pattern=, rep=)` | tc | both |  | `replaceRE` | Replaces matches of `pattern` with `rep` (`$1` groups). (pattern: string, rep: string) |
| `x \| regex_find(pattern=, limit=?)` | F | both |  | `findRE` | Matches of `pattern`, at most `limit`. (pattern: string, limit: int) |
| `x \| substr(start=, length=?)` | F | both |  | `substr` | `length` characters from `start` (negative counts from the end). (start: int, length: int) |
| `x \| truncate(length=, end=?)` | bi | both |  |  | Tera's plain-text truncate to `length` characters plus `end`. (length: int, end: string) |
| `x \| truncate_html(length=, ellipsis=?)` | F | both |  | `truncate` | Go's HTML-aware `truncate`: closes open tags, `ellipsis` default `…`. Keeps the input's safety. (length: int, ellipsis: string) |
| `x \| pad_start(width=)` | F | both |  | `printf "%5s"` | Pads on the left with spaces to `width` characters. (width: int) |
| `x \| pad_end(width=)` | F | both |  | `printf "%-35s"` | Pads on the right with spaces to `width` characters. (width: int) |
| `x \| indent(width=?, indentation=?, first=?, blank=?)` | bi | both |  |  | Tera's indent. (width: int, indentation: string, first: bool, blank: bool) |
| `x \| newlines_to_br` | bi | both |  |  | Replaces line breaks with `<br>`. |
| `x \| pluralize(singular=?, plural=?)` | bi | both |  |  | Tera's suffix pluralizer for a count (`singular`, `plural`). Go's `inflect.Pluralize` is `pluralize_word`. (singular: string, plural: string) |
| `x \| pluralize_word` | F | both |  | `inflect.Pluralize`, `pluralize` | English plural of a word. |
| `x \| singularize_word` | F | both |  | `inflect.Singularize`, `singularize` | English singular of a word. |
| `x \| humanize` | F | both |  | `humanize` | Go's `humanize` (`my-first-post` → `My first post`; numbers → ordinals). |
| `x \| ordinalize` | F | both |  | `humanize (numbers)` | `1` → `1st`. |
| `x \| urlize` | F | both |  | `urlize` | The URL-safe path form of a string, as Go's `urlize` makes it. |
| `x \| anchorize(style=?)` | F | both |  | `anchorize` | An anchor id as Go generates it; `style` `github` (default), `github-ascii` or `blackfriday`. (style: string) |
| `x \| plainify` | F | both |  | `plainify` | Strips HTML tags. |
| `x \| emojify` | F | both | yes | `emojify` | Replaces `:shortcode:` emoji. |
| `x \| markdownify` | F (s) | both | yes | `markdownify` | Renders Markdown with the current page's hooks; a single paragraph is unwrapped. |
| `x \| render_string(display=?, page=?)` | F (s) | both | yes | `.RenderString` | Renders Markdown with `page`'s hooks; `display="block"` keeps the paragraph. (display: string, page: page) |
| `x \| highlight(lang=, options=?)` | F (s) | both | yes | `highlight`, `transform.Highlight` | Syntax highlighting of the input as `lang` (Chroma classes, or inline styles per `noClasses`; needs the site's highlight configuration). (lang: string, options: any) |
| `x \| to_math(options=?, optional=?)` | F | both | yes | `transform.ToMath (+ try)` | LaTeX to MathML and/or HTML with KaTeX 0.16.22 and mhchem, as Go renders it (SHOULD; feature `math`). `options`: KaTeX's `output` (`mathml` default, `html`, `htmlAndMathml`), `displayMode`, `leqno`, `fleqn`, `errorColor`, `macros`, `minRuleThickness`, `throwOnError` (default true), `strict` (`error` default, `ignore`, `warn`: warnings). An error (a formula KaTeX rejects, invalid options) fails the render; with `optional=true` it is a warning (id `to_math`) and the result none. (options: map, optional: bool) |
| `diagrams_goat(text=)` | fn | both |  | `diagrams.Goat` | `{inner (safe SVG), width, height, wrapped}` for the ASCII diagram `text` (SHOULD; feature `goat`). (text: string) |
| `x \| format_number(precision=?)` | F | both |  | `lang.FormatNumber`, `printf "%.1f"` | The number with `precision` decimals in the format of the render's `lang`. (precision: int) |
| `x \| filesize_format(binary=?)` | tc | both |  |  | Human file size (`binary` units by default). (binary: bool) |

## Encoding, escaping and hashing

| Call | Kind | Phase | Safe | Go | Description |
|---|---|---|---|---|---|
| `x \| safe` | bi | both |  | `safeHTML`, `safeHTMLAttr`, `safeURL`, `safeJS`, `safeCSS` | Marks the value safe. |
| `x \| escape` | bi | both |  |  | Tera's escape (leaves safe input alone). Go's `html` is `html_escape`. |
| `x \| escape_html` | bi | both |  |  | Tera's HTML escape of a string. |
| `x \| escape_xml` | bi | both |  |  | Tera's XML escape (`&quot;`, `&apos;`; leaves safe input alone). Go's `transform.XMLEscape` is `xml_escape`. |
| `x \| xml_escape` | F | both | yes | `transform.XMLEscape` | Drops the characters XML forbids, then escapes `& < > " '`, tab, newline and CR (`&#34; &#39; &#x9; &#xA; &#xD;`, Go's `xml.EscapeText`) even when the input is safe; the result is safe. |
| `x \| html_escape` | F | both | yes | `html`, `htmlEscape`, `transform.HTMLEscape` | Escapes `& < > " '` even when the input is safe; the result is safe. |
| `x \| html_unescape` | F | both |  | `htmlUnescape`, `transform.HTMLUnescape` | Decodes HTML entities. |
| `x \| jsonify(indent=?)` | F | both | yes | `jsonify` | JSON with sorted keys, `<>&` escaped as `\u003c…`; `indent` pretty-prints. (indent: string) |
| `x \| unmarshal(format=?)` | F (s) | both |  | `transform.Unmarshal` | Parses a string or resource as JSON, TOML, YAML, CSV or XML (`format` overrides detection); keys sorted. (format: string) |
| `x \| remarshal(format=)` | F | both |  | `transform.Remarshal` | Re-encodes data as `format` (`toml`, `yaml`, `json`). (format: string) |
| `x \| urlencode` | tc | both |  | `urlquery` | Percent-encodes for a URL path (keeps `/`). |
| `x \| urlencode_strict` | tc | both |  | `urlquery` | Percent-encodes every non-alphanumeric character. |
| `x \| urldecode` | F | both |  | `urls.PathUnescape` | Decodes percent-encoding. |
| `x \| b64_encode(url_safe=?, padded=?)` | tc | both |  | `base64Encode` | Base64 (`url_safe`, `padded`). (url_safe: bool, padded: bool) |
| `x \| b64_decode(url_safe=?)` | tc | both |  | `base64Decode` | Decodes base64. (url_safe: bool) |
| `x \| md5` | F | both |  | `md5`, `crypto.MD5` | Hex MD5. |
| `x \| sha1` | F | both |  | `sha1` | Hex SHA-1. |
| `x \| sha256` | F | both |  | `sha256` | Hex SHA-256. |
| `x \| fnv32a` | F | both |  | `hash.FNV32a` | FNV-1a 32-bit hash as an integer. |
| `x \| xxhash` | F | both |  | `hash.XxHash` | Hex xxHash64. |

## URLs and paths

| Call | Kind | Phase | Safe | Go | Description |
|---|---|---|---|---|---|
| `x \| abs_url` | F (s) | both |  | `absURL` | Absolute URL against `baseURL` (base path kept). |
| `x \| rel_url` | F (s) | both |  | `relURL` | Root-relative URL with the base path. |
| `x \| abs_lang_url(page=?)` | F (s) | both |  | `absLangURL` | `abs_url` with the language prefix of `page`'s language. (page: page) |
| `x \| rel_lang_url(page=?)` | F (s) | both |  | `relLangURL` | `rel_url` with the language prefix of `page`'s language. (page: page) |
| `ref(path=, lang=?, output_format=?, page=?)` | fn (s) | both |  | `ref` | The permalink of the page at `path`; unresolved per `refLinksErrorLevel`. (path: string, lang: string, output_format: string, page: page) |
| `rel_ref(path=, lang=?, output_format=?, page=?)` | fn (s) | both |  | `relref` | The relative permalink of the page at `path`. (path: string, lang: string, output_format: string, page: page) |
| `x \| parse_url` | F | both |  | `urls.Parse` | `{scheme, host, path, fragment, query, is_absolute, string}`. |
| `join_url(parts=)` | fn | both |  | `urls.JoinPath` | Joins URL `parts` with single slashes. (parts: array) |
| `x \| path_ext` | F | both |  | `path.Ext` | Extension with the dot. |
| `x \| path_base` | F | both |  | `path.Base` | Last element. |
| `x \| path_base_name` | F | both |  | `path.BaseName` | Last element without extension. |
| `x \| path_dir` | F | both |  | `path.Dir` | All but the last element. |
| `x \| path_clean` | F | both |  | `path.Clean` | Lexically cleaned path. |
| `path_join(parts=)` | fn | both |  | `path.Join` | Joins `parts` and cleans the result. (parts: array) |

## Dates

| Call | Kind | Phase | Safe | Go | Description |
|---|---|---|---|---|---|
| `now()` | fn | both |  | `now` | The build time (honours `--clock`) as a date value. |
| `x \| date(format=?, style=?, locale=?)` | F | both |  | `.Format`, `time.Format`, `dateFormat` | Formats a date with strftime `format` or `style` (`short`, `medium`, `long`, `full`). A style is localized in `locale` (default: the render's `lang`; Thai uses the Gregorian calendar); a `format`'s month and weekday names are English (Go's `.Format`) unless `locale` is given (`time.Format`, `dateFormat`: `locale=lang`). Accepts a date value, a date string or Unix seconds; none prints nothing. (format: string, style: string, locale: string) |
| `x \| to_date` | F | both |  | `time.AsTime`, `time` | Parses a string or number into a date value (`{rfc3339, unix}`). |

## Language

| Call | Kind | Phase | Safe | Go | Description |
|---|---|---|---|---|---|
| `i18n(key=, count=?, data=?, page=?)` | fn (s) | both |  | `i18n`, `T` | The translation of `key` in `page`'s language; `count` picks the plural form, `data` fills `{{ .Field }}`. (key: string, count: number, data: any, page: page) |

## Resources and assets

| Call | Kind | Phase | Safe | Go | Description |
|---|---|---|---|---|---|
| `get_asset(path=)` | fn (s) | both |  | `resources.Get` | The asset at `path` under `assets/`, or none. (path: string) |
| `find_asset(pattern=)` | fn (s) | both |  | `resources.GetMatch` | The first asset matching the glob `pattern`, or none. (pattern: string) |
| `find_assets(pattern=)` | fn (s) | both |  | `resources.Match` | All assets matching `pattern`. (pattern: string) |
| `get_remote(url=, options=?, optional=?)` | fn (s) | both |  | `resources.GetRemote`, `try` | A remote resource (`options`: headers, method, body, key). Errors propagate unless `optional=true` (then none and a warning). (url: string, options: map, optional: bool) |
| `concat_assets(target=, items=)` | fn (s) | both |  | `resources.Concat` | Concatenates `items` into a resource at `target`. (target: string, items: array) |
| `asset_from_string(target=, content=)` | fn (s) | both |  | `resources.FromString` | A resource at `target` with `content`. (target: string, content: string) |
| `x \| get_resource(name=)` | F | both |  | `.Resources.Get` | The resource named `name` in a list (case-insensitive), or none. (name: string) |
| `x \| find_resource(pattern=)` | F | both |  | `.Resources.GetMatch` | The first resource matching the glob `pattern`, or none. (pattern: string) |
| `x \| find_resources(pattern=)` | F | both |  | `.Resources.Match` | All resources matching `pattern`. (pattern: string) |
| `x \| by_type(type=)` | F | both |  | `.Resources.ByType` | Resources whose type is `type` (`image`, `page`, …). (type: string) |
| `x \| fingerprint(algo=?)` | F (s) | both |  | `fingerprint` | The resource renamed with its hash (`algo`: sha256 default, sha384, sha512, md5); sets `data.integrity`. (algo: string) |
| `x \| minify` | F (s) | both |  | `minify` | The minified resource. |
| `x \| resource_content` | F (s) | both |  | `.Content (resource)` | The text of a resource; for a bundled content page, its rendered HTML (marked safe). |
| `x \| publish` | F (s) | both |  | `.Publish` | Publishes the resource and returns it. |
| `x \| to_css(options=?)` | F (s) | both |  | `toCSS`, `css.Sass` | Sass/SCSS to CSS. (options: map) |
| `x \| js_build(options=?)` | F (s) | both |  | `js.Build` | Bundles with rolldown. (options: map) |
| `x \| execute_as_template(target=, data=?)` | F (s) | both |  | `resources.ExecuteAsTemplate` | Renders the asset as a Tera template with `data`, published at `target`. (target: string, data: any) |
| `x \| post_process` | F (s) | both |  | `resources.PostProcess` | Defers the resource's fields until all pages are rendered. |
| `x \| purge_css(safelist=?, greedy=?, blocklist=?, content=?, variables=?, important=?)` | F (s) | both | yes | `PurgeCSS (PostCSS)` | A placeholder that each page's published output replaces with the rules of the CSS (a resource or string) that page uses: the tags, classes and ids of its elements, the words of its `<script>` elements. `safelist` names (or `/regex/`) count as used; `greedy` keeps any selector whose text contains the string or matches the `/regex/`; `blocklist` names drop their selectors; `content` resources or strings (scripts that add classes) count their words as used on every page; `variables=true` drops custom properties nothing kept references; `important=false` drops `!important`. Printed compactly for the project's browserslist targets. E.g. `<style>{{ css \| purge_css(content=[js]) }}</style>`. (safelist: array, greedy: array, blocklist: array, content: array, variables: bool, important: bool) |

## Images

| Call | Kind | Phase | Safe | Go | Description |
|---|---|---|---|---|---|
| `x \| resize(width=?, height=?, format=?, quality=?, filter=?, anchor=?, spec=?)` | F (s) | both |  | `.Resize` | Resizes to `width` and/or `height` (or a `spec` string in Go's syntax, e.g. `"600x400 webp q75"`). (width: int, height: int, format: string, quality: int, filter: string, anchor: string, spec: string) |
| `x \| fill(width=?, height=?, format=?, quality=?, filter=?, anchor=?, spec=?)` | F (s) | both |  | `.Fill` | Crops and resizes to fill `width`×`height` at `anchor`. (width: int, height: int, format: string, quality: int, filter: string, anchor: string, spec: string) |
| `x \| fit(width=?, height=?, format=?, quality=?, filter=?, anchor=?, spec=?)` | F (s) | both |  | `.Fit` | Downscales to fit `width`×`height`. (width: int, height: int, format: string, quality: int, filter: string, anchor: string, spec: string) |
| `x \| crop(width=?, height=?, format=?, quality=?, filter=?, anchor=?, spec=?)` | F (s) | both |  | `.Crop` | Crops to `width`×`height` at `anchor`. (width: int, height: int, format: string, quality: int, filter: string, anchor: string, spec: string) |
| `x \| process(width=?, height=?, format=?, quality=?, filter=?, anchor=?, spec=?)` | F (s) | both |  | `.Process` | Any of the above per `spec` (or the typed kwargs). (width: int, height: int, format: string, quality: int, filter: string, anchor: string, spec: string) |
| `x \| image_filter(filters=)` | F (s) | both |  | `images.Filter`, `.Filter`, `images.Text`, `images.Dither` | Applies `filters`, a list of `{"op": …}` maps, one per Go `images.*` filter: `brightness`, `color_balance`, `colorize`, `contrast`, `gamma`, `gaussian_blur`, `grayscale`, `hue`, `invert`, `saturation`, `sepia`, `sigmoid`, `unsharp_mask`, `pixelate`, `opacity`, `padding`, `overlay` and `mask` (`image`: a resource), `auto_orient`, `text` (`text`, `color`, `size`, `x`, `y`, `alignx`, `aligny`, `linespacing`, `font`: a font resource), `dither` (`colors`, `method`, `serpentine`, `strength`), `process` (`spec`). E.g. `img \| image_filter(filters=[{"op": "text", "text": page.title, "size": 40}, {"op": "dither"}])`. (filters: array) |
| `x \| exif` | F (s) | both |  | `.Exif` | EXIF data of an image, or none. |
| `x \| image_colors` | F (s) | both |  | `.Colors` | Not implemented yet: calling it is an error (Go's `.Colors` gives the dominant colours as hex strings). |
| `qr_code(text=, level=?, scale=?, target_dir=?)` | fn (s) | both |  | `images.QR` | A PNG image resource of the QR code of `text`, with Go's bytes and name (`<target_dir>/qr_<hash>.png`): `level` low, medium (default), quartile or high; `scale` pixels per module (at least 2, default 4). E.g. `qr_code(text=page.permalink, target_dir="images/qr")`. (text: string, level: string, scale: int, target_dir: string) |

## Templates

| Call | Kind | Phase | Safe | Go | Description |
|---|---|---|---|---|---|
| `super()` | bi | both |  |  | The parent block's content (inside `{% block %}` only). |
| `partial(name=, …)` | fn (s) | both | yes | `partial (dynamic name or returned value)` | Renders `_partials/<name>` with the kwargs as top-level names; returns its `return_value` or the rendered string. (name: string) |
| `partial_cached(name=, key=, …)` | fn (s) | both | yes | `partialCached` | `partial` memoised on (`name`, `key`). (name: string, key: any) |
| `return_value(value=)` | fn (s) | both |  | `return` | Sets the value the enclosing `partial()` returns. Prints nothing. (value: any) |
| `template_exists(name=)` | fn (s) | both |  | `templates.Exists` | Whether a template called `name` exists. (name: string) |
| `defer(template=, key=, data=?)` | fn (s) | layout | yes | `templates.Defer` | A placeholder; `template` is rendered once per `key` with `data` after all pages. (template: string, key: string, data: any) |
| `x \| arg(index=?, name=?, default=?)` | F | content |  | `.Get (shortcodes)` | A shortcode argument by position `index` or by `name`, else `default`: `shortcode \| arg(index=0, default="")`. (index: int, name: string, default: any) |

## Environment, files and debugging

| Call | Kind | Phase | Safe | Go | Description |
|---|---|---|---|---|---|
| `get_env(name=)` | fn | both |  | `os.Getenv` | An environment variable ("" when unset): one the project's `.env` file defines (the process environment wins), or one `security.funcs.getenv` allows; any other name is an error. (name: string) |
| `read_file(path=)` | fn | both |  | `os.ReadFile` | A file of the project (`security` rules apply). (path: string) |
| `file_exists(path=)` | fn | both |  | `os.FileExists` | Whether a project file exists. (path: string) |
| `x \| dump` | F | both |  | `debug.Dump` | Pretty-printed JSON of any value. |

## Tests

| Call | Kind | Phase | Safe | Go | Description |
|---|---|---|---|---|---|
| `x is defined` | bi | both |  | `isset` | The value is defined. |
| `x is undefined` | bi | both |  |  | The value is undefined. |
| `x is none` | bi | both |  | `.IsZero (dates)` | The value is none (zero dates serialise as none). |
| `x is string` | bi | both |  |  | A string. |
| `x is number` | bi | both |  |  | A number. |
| `x is integer` | bi | both |  |  | An integer. |
| `x is float` | bi | both |  |  | A float. |
| `x is bool` | bi | both |  |  | A bool. |
| `x is map` | bi | both |  | `reflect.IsMap` | A map. |
| `x is array` | bi | both |  | `reflect.IsSlice` | An array. |
| `x is iterable` | bi | both |  |  | An array, map or string. |
| `x is odd` | bi | both |  |  | An odd number. |
| `x is even` | bi | both |  |  | An even number. |
| `x is divisible_by(divisor=)` | bi | both |  |  | Divisible by `divisor`. (divisor: int) |
| `x is starting_with(pat=)` | bi | both |  | `strings.HasPrefix` | Starts with `pat`. (pat: string) |
| `x is ending_with(pat=)` | bi | both |  | `strings.HasSuffix` | Ends with `pat`. (pat: string) |
| `x is containing(pat=)` | bi | both |  | `in`, `strings.Contains` | Contains `pat` (substring, element or key). (pat: any) |
| `x is matching(pat=)` | tc | both |  | `findRE (as a condition)`, `where … "like"` | Matches the regex `pat`. (pat: string) |
| `x is version_at_least(version=)` | T | both |  |  | A semver at least `version`; a `-DEV` build ranks below its release. Replaces Go-template comparisons of the site-info object's `Version`. (version: string) |

## Conversion rules

**Names and paths**

- Files use v0.146 names (`_partials/`, `_shortcodes/`, `_markup/`, `home|section|taxonomy|term|single|list|all|<layout>[.<lang>][.<fmt>].<ext>`, `baseof.html`). Include and extends literals are lower-case, new-style names.
- `.Title` → `page.title`; `.Site.X` and `site.X` → `site.x`.
- Params are lower-cased: `.Site.Params.HomeTitle` → `site.params.hometitle`. Data keys keep their case: `item.Name`.
- Reserved front-matter keys stay available in params: `where … "Params.type"` → `[p for p in l if p.params.type == "snacks"]`.

**Missing values**

- Printed params that may be missing → `page.params.x or ""` (printing an undefined value is an error).
- Nested optional lookups use `?.`: `page.params.a?.b`, `page.parent?.title or ""`.
- Go's `default` → `default_if_empty(value=)`. Do not use `or` for bools.
- `x == none` is false when `x` is undefined; use `is undefined`, `is none` or truthiness instead.

**Comparisons**

- `eq $p $currentSection` → `p.id == current_section.id`. Pagers compare by `page_number`, dates by `.unix`.
- Mixed int/string comparisons get an explicit `int` or `str`.

**Control flow**

- `{{ with X }}…{{ else with Y }}` → `{% if X %}{% set x = X %}…{% elif Y %}…`.
- `range $k, $v := m` → `{% for k, v in m %}`; template map literals need `| sort_keys` first.
- `range $i, $e := l` → `{% for e in l %}` with `loop.index0` / `loop.first`.
- `where` → a list comprehension with `if`; `apply` → a comprehension; `seq N` → `range(start=1, end=N+1)`.
- Variable reassignment inside a block → `set_global` (discarded inside includes).

**Templates and partials**

- `define`/`block` in children → `{% extends "baseof.html" %}` plus `{% block %}`; delete blocks the parent does not define.
- Partials → include, component or `partial()`; component calls pass arguments as `name={expr}`, `name="literal"` or the shorthand `name`.
- `try` → `optional=true` on `get_remote` and `to_math` (none on an error), or a `none` check.

**Formatting**

- Go `printf` → `~`, `pad_start`/`pad_end`, `round`/`format_number`, `jsonify`.
- Tera 2.4 strings know only the escapes `\n` `\t` `\r` `\"` `\'` `\/` `\\`: write other characters such as U+00A0 literally (`"\u{a0}"` is an error), and double a regex backslash (`pattern="\\s+"`).
- Go date layouts → strftime: `"2006-01-02"` → `"%Y-%m-%d"`, `"Jan 2, 2006"` → `"%b %-d, %Y"`. `.Format` stays English; `time.Format` and `dateFormat` localize names, so add `locale=lang`.

**Removed Go-template idioms**

- `range .Paginator.Pages` → `{% set pager = paginator() %}{% for p in pager.pages %}`; delete a second `.Paginate` that follows `.Paginator`.
- `{{ $noop := .WordCount }}` → delete.
- Another page's `.Content` inside a shortcode → `page_content(page=p)`.
- `.Scratch` / `newScratch` → `set`, `set_global`, `merge`; the page store only for cross-template flags.

**Components**

- A component that calls site-bound functions passes `page=` or declares `@__nh`.

**Escaping**

- `html`/`htmlEscape` → `html_escape`. In `<script>`, use `jsonify | safe`. In query strings, use `urlencode`. `safeHTML` and the other `safe*` → `safe`.

**Assets and i18n**

- Assets used with `execute_as_template` are Tera templates: `{{ .api }}` → `{{ data.api }}`.
- i18n files stay in Go-template syntax, limited to `{{ . }}` and `{{ .Field }}`.

## Embedded templates

Loaded under the fallback prefix `_embedded/`; a user or theme template of the same name wins.

- `_markup/render-codeblock-goat.html`
- `_markup/render-image.html`
- `_markup/render-link.html`
- `_markup/render-table.html`
- `_partials/_funcs/get-page-images.html`
- `_partials/google_analytics.html`
- `_partials/opengraph.html`
- `_partials/pagination.html`
- `_partials/schema.html`
- `_partials/twitter_cards.html`
- `_shortcodes/details.html`
- `_shortcodes/figure.html`
- `_shortcodes/highlight.html`
- `_shortcodes/instagram.html`
- `_shortcodes/param.html`
- `_shortcodes/qr.html`
- `_shortcodes/ref.html`
- `_shortcodes/relref.html`
- `_shortcodes/vimeo.html`
- `_shortcodes/x.html`
- `_shortcodes/youtube.html`
- `alias.html`
- `robots.txt`
- `rss.xml`
- `sitemap.xml`
- `sitemapindex.xml`

## Tera facts

Verified against the tera 2.4.0 source and by the `tera_facts` tests of ssg-testkit.

- `__nh` is an ordinary identifier (a leading `_` is allowed) and `@__nh` a valid implicit component argument, resolved through the callers' scopes; a component that does not declare it cannot see it.
- Component call arguments are `name={expr}`, `name="literal"` or the shorthand `name`; `name=expr` is a syntax error.
- `==` and `!=` never fail on an undefined final path segment: the value is undefined, and undefined equals only undefined (`x == none` is false). A missing non-final segment is an error, even inside `if`.
- Printing an undefined value is an error; `x or ""` and `default(value=)` are the fallbacks.
- `?.` (and `?[`) yield undefined when the receiver is undefined or none; the result must still not be printed bare.
- Built-in kwargs: `split(pat=)`, `nth(n=)`, `replace(from=, to=)`, `trim(pat=)`, `join(sep=)`, `round(method=, precision=)`, `truncate(length=, end=)`, `default(value=, boolean=)`, `get(key=, default=)`, `range(start=, end=, step_by=)`.
- Unknown filters, tests, functions, components and include targets are errors when templates are added; kwargs are checked only when called, so the contract test checks them statically.
- tera-contrib 0.3 names: `b64_encode`/`b64_decode`, `filesize_format`, `regex_replace(pattern=, rep=)`, `matching(pat=)`, `urlencode`, `urlencode_strict`, `date(format=, locale=, timezone=)`.
