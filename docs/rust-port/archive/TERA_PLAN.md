# Tera templates on the Go implementation's page model

> **Superseded.** The current plan is [`REWRITE_PLAN.md`](../REWRITE_PLAN.md) (full idiomatic-Rust
> rewrite in `rust/`, Tera 2 templates). This file was `crates/TERA_PLAN.md`; T00 moved it here
> for history when `crates/` was deleted (the old port is at commit `be02933a`, local tag
> `go-parity-final`). Paths and crate names below refer to that old port.

Status: superseded by `docs/rust-port/REWRITE_PLAN.md` (full idiomatic-Rust rewrite, Tera 2 templates). Kept for history.

## 1. Decision

The Go version keeps the Go implementation's site model and page generation, and renders
layouts with [Tera](https://keats.github.io/tera/) (Jinja2-style, the engine Zola uses) instead
of Go's `text/template` + `html/template`.

- **Stays as in Go:** content tree, bundles, page kinds, front matter, cascade, permalinks, output
  formats, pagination, aliases, menus, taxonomies, multilingual, i18n bundles, asset pipes,
  image processing, goldmark rendering, the `layouts/` directory structure and the Go
  implementation's layout lookup order (`TemplateQuery` / `lookup_pages_layout` in nh-tplimpl).
- **Changes:** the template language, what a template can see (plain data plus functions,
  as in Zola, instead of Go objects with methods) and escaping (Tera's single HTML escaper
  instead of Go's contextual escaper).

Engine: **Tera 2** (`tera = { version = "2.4", features = ["fast", "preserve_order"] }`).
Its `Value` shares arrays and maps through `Arc`, so a page list built once per site can be
handed to every template without copying. It also has typed components, which fit shortcodes.

## 2. What the switch touches

Only five nh-* crates import `gotemplate` (nh-tpl, nh-tplimpl, nh-page, nh-markup, nh-i18n).
Everything the site-build crate renders itself (pages, pagers, aliases, render hooks, shortcodes)
goes through one seam, its `template_exec::execute` → `TemplateStore::execute_with_context`.

| crate | change |
|---|---|
| **nh-tera** (new) | Tera instance per site, template loading, the function/filter/test registry, `Value` builders |
| nh-tplimpl | keep the store, lookup, descriptors and the embedded-template list; parse with Tera; Tera versions of the embedded templates |
| nh-tpl | engine-neutral `Template` / `TplContext` (drop the gotemplate types from the public API) |
| the site-build crate | build the page/site context values; render content before layouts (§4.3) |
| nh-tplfuncs | reused as the implementation behind Tera functions and filters; the Go-reflection glue goes |
| nh-markup, nh-i18n, nh-page | replace their gotemplate imports with nh-tera types |
| gotemplate, go-value in templates | kept as the `templateEngine = "go"` path until §7 phase 6, then removed |

## 3. Layout files

The directory structure and lookup order stay the Go implementation's, including the new-style layout names
the in-repo `docs/` site already uses (`_partials/`, `_shortcodes/`, `_markup/`).

| Go | Tera |
|---|---|
| `baseof.html` + `{{ define "main" }}` | `{% extends "baseof.html" %}` + `{% block main %}`. The store resolves the name `baseof.html` with the Go implementation's baseof lookup for the page being rendered, so section/type-specific baseof files keep working |
| `{{ block "x" . }}default{{ end }}` | `{% block x %}default{% endblock %}` |
| `{{ partial "x.html" . }}` | `{% include "_partials/x.html" %}` (shares the context), or a macro/component when the partial takes arguments (`{{ partial "x" (dict ...) }}`) |
| `{{ partialCached ... }}` | `{% include %}`; caching becomes an engine concern, not a template feature |
| `{{ template "_internal/opengraph.html" . }}` | `{% include "_internal/opengraph.html" %}` (embedded, rewritten in Tera) |
| shortcode `{{ .Get 0 }}`, `.Inner`, `.Page` | `_shortcodes/name.html` rendered with `args` (positional), `params` (named), `inner`, `page`, `site`, `name`, `ordinal`, `parent` |
| render hooks `_markup/render-link.html` | same file names; context `destination`, `text`, `title`, `plain_text`, `page`, `ordinal`, `attributes` (per hook type, as the Go implementation's hook context) |
| output formats (`index.xml`, `list.json`, …) | same lookup; the file extension picks autoescape (`.html`, `.xml` on; `.json`, `.txt`, `.js` off) |

Mixing engines inside one site is not supported: a site is either all Go templates or all
Tera templates (`templateEngine` in the site config, default `tera` after phase 6).

## 4. Template context

### 4.1 Top-level names

| name | contents |
|---|---|
| `page` | the page being rendered (§4.2) |
| `site` | `title`, `base_url`, `language` (`lang`, `name`, `direction`, `weight`), `params`, `data`, `menus`, `taxonomies`, `home`, `pages`, `regular_pages`, `sections`, `languages`, `copyright`, `last_mod`, `config` (the exposed subset) |
| `paginator` | on paginated list pages: `pages`, `page_number`, `total_pages`, `total_items`, `first`, `last`, `prev`, `next`, `has_prev`, `has_next`, `pagers` |
| the project-named object (today's `build`) | `version`, `environment`, `is_production`, `is_development`, `generator` |
| `output_format` | `name`, `media_type`, `rel`, `permalink` of the output being rendered |

`section`, `taxonomy` and `term` are aliases of `page` on those kinds, as in Zola.

### 4.2 Page values

Snake_case fields, one map per page, built once per site and shared through `Arc`:

`kind`, `type`, `section`, `layout`, `title`, `link_title`, `description`, `summary`,
`truncated`, `content`, `plain`, `table_of_contents` (HTML) and `toc` (structured),
`word_count`, `reading_time`, `fuzzy_word_count`, `date`, `publish_date`, `expiry_date`,
`last_mod`, `draft`, `weight`, `params`, `keywords`, `aliases`, `permalink`,
`rel_permalink`, `path`, `slug`, `file` (`path`, `dir`, `base_name`, `ext`, `lang`),
`lang`, `translations`, `all_translations`, `output_formats`, `alternative_output_formats`,
`resources` (name, title, params, media type, permalink, rel_permalink; image fields for
images), `taxonomies` (term lists), `menus`, `is_home`, `is_section`, `is_page`,
`is_node`, `is_translated`, `parent_path`, `ancestors_paths`.

Lists (`pages`, `regular_pages`, `sections`) hold **shallow** pages: the same map without
`content`, `plain`, `summary` or the lists themselves, which keeps the graph acyclic and
cheap. Templates reach anything deeper through functions (§5), like Zola's `get_page`.

Go-style method calls that take arguments become functions or filters:
`.Paginate $pages 10` → front matter/config `paginate` plus `paginator`, `.GetPage "x"` →
`get_page(path="x")`, `.Resources.GetMatch "*.jpg"` → `page.resources | resource_match(pattern="*.jpg")`,
`.Scratch` / `.Store` → `{% set %}` / `{% set_global %}` (no cross-template mutable state).

### 4.3 Render order

The Go implementation renders content lazily, on the first `.Content` call from a template. With
plain data, the site-build crate renders every page's content (markdown, shortcodes, render hooks, summary, TOC)
**before** any layout runs, then builds the page values, then renders layouts. Shortcodes and
hooks only see page data built from front matter and content-independent fields; a
shortcode that reads another page's `content` is an error, as in Zola.

## 5. Functions, filters, tests

Registered by nh-tera, implemented on top of nh-tplfuncs / nh-resources / nh-i18n.

- **Lookups:** `get_page(path, lang?)`, `get_section(path, lang?)`, `get_taxonomy(kind, lang?)`,
  `get_taxonomy_term(kind, term)`, `get_menu(name)`, `get_data(path)`,
  `get_resource(path)` (`resources.Get`), `get_remote(url, options?)`, `page_by_path`.
- **URLs:** filters `abs_url`, `rel_url`, `abs_lang_url`, `rel_lang_url`, `url_parse`,
  functions `ref(path)`, `rel_ref(path)`, `url_join(parts)`.
- **Text/content:** filters `markdownify`, `plainify`, `emojify`, `humanize`, `truncate`
  (the Go implementation's HTML-aware version), `safe_html`/`safe_css`/`safe_js`/`safe_url` (all = `safe`),
  `jsonify`, `unmarshal`, `highlight(lang, options?)`, `to_math`, `html_escape`,
  `html_unescape`, `urlize`, `anchorize`, `trim`, `title_case`, `pluralize`, `singularize`.
- **i18n:** function `trans(key, count?, data?)` (alias `i18n`), filter `lang_format_number`,
  `lang_format_date`.
- **Collections:** Tera's built-ins (`sort`, `group_by`, `filter`, `map`, `first`, `last`,
  `slice`, `length`, `reverse`, `unique`, `concat`) plus the Go implementation's `where` with operators,
  `sort_by(key, order)`, `after`, `shuffle`, `union`, `intersect`, `complement`, `seq`, `dict`.
- **Resources pipeline:** filters `minify`, `fingerprint(algo?)`, `to_css(options?)`
  (Sass), `postcss(options?)`, `js_build(options?)`, `resource_concat(name)`,
  `execute_as_template(target, data)`, `resize`, `fill`, `fit`, `crop`, `image_filter(...)`;
  each returns a resource map whose `permalink` publishes the file on use.
- **Dates/format:** Tera's `date` plus the Go implementation's `format(layout)` with Go layouts (kept, so
  front matter and config date layouts keep working), `time(value)`, `now()`.
- **Tests:** `is_page`, `is_section`, `in(list)`, `has_prefix`, `has_suffix`.

## 6. Pass criteria

The Go build stays the oracle for everything that is not layout output:

1. Same output file set (paths) as the Go build of the same site with equivalent layouts.
2. Byte-identical files that templates do not produce: static files, processed resources
   (CSS, JS, images), the stats file, rendered markdown fragments (`page.content`) and
   permalinks/URLs.
3. For layout output: for the in-repo test sites whose layouts are converted 1:1 (§7 phase 5),
   the HTML DOM after whitespace normalisation equals the Go build's, except for listed
   escaping differences (§4.3 / Tera's escaper).
4. The existing Rust test suites keep passing; new ones cover nh-tera's functions and the
   context builders.

## 7. Phases

1. **Engine seam.** Engine-neutral `Template`/`TplContext` in nh-tpl; `templateEngine`
   config; nh-tera skeleton; the Go path unchanged and still passing I01.
2. **Context.** Page/site/paginator value builders in the site-build crate; render-order change
   (content before layouts) behind the Tera engine only.
3. **Functions and filters** (§5), each with unit tests against the nh-tplfuncs result.
4. **Shortcodes, render hooks, embedded templates** (RSS, sitemap, robots, alias,
   opengraph, twitter cards, schema, pagination, google analytics) in Tera.
5. **Convert the test sites:** this repo's `docs/layouts` (76 files), `testsite`,
   the I01 small sites; compare per §6.
6. **The private site:** convert its layouts in its repository (needs that repo attached
   to the session), build with Tera, compare per §6; then default `templateEngine` to `tera`.
7. **Remove the Go template path:** gotemplate, the Go-reflection parts of nh-tplfuncs and
   nh-tplimpl's gotemplate parsing, once phase 6 passes.

## 8. Risks

- Tera 2 is a rewrite of Tera 1; Zola's syntax docs mostly apply, but its function
  signatures (kwargs only) and some filters differ. Pin the minor version.
- Content-before-layout rendering (§4.3) changes what shortcodes can see.
- Tera's escaper is not context-aware: URLs in attributes and inline JS/CSS need explicit
  filters (`safe_url`, `jsonify`), which the conversion of each layout must check.
