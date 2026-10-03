# ssg-layouts

Layout templates (REWRITE_PLAN.md §4.1, §4.3, §4.5): the scan of v0.146 layout names, the Go
implementation's lookup scorer, base template resolution, escaping by output format and loading
into one Tera instance. The embedded templates (`embedded/**`) are T32's; the build script
(`src/build.rs`) lists whatever is there for `include_str!`. T60 renders them against testsite
views through the binary and reviews them against Go's (`fugo` crate: `tests/it/embedded.rs`,
the review table in its README).

| API | What |
|---|---|
| `LayoutStore::scan(&Vfs, &Config)` | the layouts component (project, then themes; the Vfs hides a theme file behind a project file of the same path) plus `embedded::TEMPLATES`; every file classified by its v0.146 name into a `TemplateInfo` (`TemplateRole`, scope, language, format, media type, `Escaping`, `Origin`); all problems returned at once as `TemplateError::Layouts` (sorted by position) |
| `LayoutStore::from_sources(LayoutEnv, sources)` | the same from `LayoutSource`s (rel path, origin, source) |
| `LayoutEnv::from_config` / `new` | path parser, output formats, media types, default output format |
| `LayoutStore::select(&LayoutQuery)` | layout + base of a (page, format): `Selection { layout, base, render_as }` |
| `LayoutStore::hook(&HookQuery)`, `shortcode(&ShortcodeQuery)`, `partial(name)`, `has_shortcode` | the other lookups; `ShortcodeMiss::{NotFound, Incompatible}` |
| `load(Arc<LayoutStore>, &Selections, register)` | Tera: fallback prefixes `_theme1/ … _embedded/`, `autoescape_on([.html, .htm, .xml, .svg])`, `register(&mut tera)`, then one `add_raw_templates` with every template, the escaping aliases and the `L@@B` variants the selections need |
| `go_marker(src)` | the first Go-template marker of a source and its line (the `GoTemplate` refusal; ssg-build checks content adapters with it) |
| `Templates::{tera, store, select, shortcode, shortcode_query, hook, partial, uses_variable}` | the loaded instance; `uses_variable` asks Tera's `get_template_variables` (template, parents, includes), memoised |

## Rules

- **Names.** Accepted: `_partials/**`, `[<dir>/]_shortcodes/<name>…`,
  `[<dir>/]_markup/render-<kind>[-<variant>]…`, `[<dir>/]baseof[.<layout>|.<kind>]…`,
  `[<dir>/]<home|section|taxonomy|term|single|list|all|layout>[.<lang>][.<format>].<ext>`,
  `404.html`, `rss.xml`, `sitemap.xml`, `sitemapindex.xml`, `robots.txt`, `alias.html`
  (`TemplateRole::Standalone` for all but `rss.xml`, which is a layout of format `rss`). Names
  are lower-cased (the Vfs path parser's normalisation). Dot files and `~` backups are skipped.
- **Refused, with the name to use** (`IssueKind::LegacyName`): `_default/X` → `X`,
  `partials/` → `_partials/`, `shortcodes/` → `_shortcodes/`, `<id>-baseof.<ext>` →
  `baseof.<id>.<ext>`, `taxonomy/list.*` and `term/term.*` → `term.*`, a layout named `index`
  → `home`. **Refused** (`UnknownName`): `_markup/` files not named `render-<known kind>`,
  `_internal/` (reserved) and `_server/`, suffixes with no output format or media type, two files with the same
  Tera name. **Refused** (`GoTemplate`, with the line): `{{ .`, `{{ $`, `{{ end }}`, `{{/*`,
  `{{ define|range|with|if|else|block|template|partial …` outside `{% raw %}` and comments; the
  message points to `fugo templates check` and to `docs/rust-port/template-api.md` by its
  URL on GitHub's `main` (a release archive does not ship it; the migrate tool, T73, is a
  stub).
- **Descriptor.** Kind, layout, language and format come from the file name (Vfs
  `PathParser`); the media type from the format, else the only format with that suffix, else
  the first media type with it (then the format named like its sub type decides plain text);
  partials without a format are HTML. A layout identifier equal to the format or kind is
  dropped.
- **Duplicates** of (role, key, shortcode name, descriptor): the earliest origin (project,
  theme 1, …, embedded), then the fewest identifiers, then the smallest path is indexed; every
  file is still loaded into Tera under its own (prefixed) name.
- **Scorer** (`score.rs`): Go v0.146's weights and rejections (see the module docs).
  Candidates are walked from the root key down to the query path; within a key they are
  offered in descriptor order (kind, layout, format and media type names, language, variants);
  the chooser prefers user/theme templates over embedded ones (not for hooks), a closer
  template unless its `w2`/`w3` is lower, then `w1`, then the distance, then the path.
- **Base resolution.** A layout that begins (after whitespace and comments) with
  `{% extends "baseof.html" %}` requests it. Its candidates are the base templates whose kind
  fits the layout (`baseof.page` never wraps `list`, other kinds never `single`) and whose
  descriptor matches the layout's; per query, the best candidate on the query path wins (no
  candidate on the path: no selection, as in Go). When the winner is not what the literal
  `"baseof.html"` resolves to in Tera, `render_as` is `<layout>@@<base>` and `load` registers
  that variant with the literal rewritten. An explicit `{% extends "other" %}` is left alone.
- **Hooks.** `useEmbedded` (`EmbeddedHooks`, `HookUse`): `Always` considers only embedded link/
  image hooks, `Never` (and `auto` outside multilingual single-host sites) excludes them,
  `Fallback` considers all. A candidate with another variant is not considered.
- **Escaping.** A template escapes as its output format says (`Escaping::Plain` for plain-text
  formats). When the file suffix says otherwise, the template is also registered as
  `<name>@@plain` (plain format, escaped suffix: `feed.plainxml.xml`) or `<name>@@escaped.html`
  (HTML format, other suffix), and `render_name()`/`render_as` use the alias. Includes follow
  the caller's mode (Tera).

## Acceptance evidence

`cargo test -p ssg-layouts` (20 integration tests, 2 unit tests):

- `lookup_*`: `oracle/tplimpl/lookup` replayed on the normalised `oracle/tplimpl/store` trees
  (below): **327,192 of 327,192 lookups give Go's winner** (docs 16,314; testsite 11,389;
  legacy 40,503; modern 37,302; themes 38,263; the 255 integration archives 183,421), including
  the base template of every layout Go wrapped in one, shortcode errors (not found vs
  incompatible), partial and by-name lookups. 312 records are left out by the normalisation.
  Every synthesised file round-trips to Go's key and descriptor.
- `structure_oracle_template_and_baseof`: compares template + baseof for every (page, format)
  of `testdata/golden/<site>[-<variant>]/structure.json[.gz]` against the converted
  layouts of `sites/<site>`; a label without a dump is skipped with a message (the golden
  data, frozen at 44529028, has dumps for testsite, docs-i01, docs-reduced and
  mini; schema in `tests/it/structure.rs`); `structure_reader_self_test` exercises the reader.
- `scan`: legacy names with hints, Go markers with lines, unknown names, roles, descriptors,
  theme prefixes and shadowing, all through `Vfs` + `Config`.
- `load`: fallback prefixes (user → theme 1 → theme 2 → embedded), escaping by format and
  aliases, base variants, `uses_variable` through includes and parents, Tera load errors,
  and every `sites/*/layouts` tree loading with the contract's placeholders.

### Normalisation of the tplimpl fixtures

The fixtures were recorded from sites with legacy names, and Go's tree may hold one file under
several keys (its own place plus legacy mappings). Each tree entry of the modelled categories
(layouts, base templates, hooks, partials, shortcodes; user and embedded) becomes one synthesised
v0.146 file whose name spells the entry's key and descriptor (`tests/it/oracle.rs`); a layout Go
wrapped in a base gets `{% extends "baseof.html" %}`. Left out, with the lookups they win: Go's
internal templates (its private directory and `_server/`), inline partials
(`{{ define "partials/x" }}`), files without a suffix, and entries whose spelling is a refused
legacy name (Go keeps `term/term.html` at `term/` too). Grid lookups Go made without a candidate
filter are replayed with `HookUse::Fallback`.

## Accepted deviations

| What | Why |
|---|---|
| Only v0.146 names; legacy ones are errors | §4.1: the legacy → new mapping lives in T01's normaliser and `ssg-migrate` |
| `index.*` refused (→ `home.*`) | not in §4.1's list, but Go maps it silently; a v0.146 site would otherwise lose its home template without notice |
| `taxonomy/<singular>.html` and `taxonomy/<singular>.terms.html` are ordinary layouts of type `taxonomy` | §4.1 lists only `taxonomy/list` and `term/term`; the other old spellings are valid v0.146 names |
| `Score` is not `Ord` | Go's chooser is not a total order (a closer template wins despite a lower `w1`); `Best::offer` implements it |
| Base variants are registered for the selections passed to `load` only | a pair nobody renders may fail to load in Tera (a child block its parent lacks) although Go would accept it |
| A user template does not beat a more specific theme template | Go's rule (a theme loses only to the same key and descriptor); §4.3's "user beats theme" holds for equal candidates. `lookup_themes` confirms |
| `partial("x.json")` resolves to `x.html` when that is the partial's only file | Go's partial lookup takes the best file of the named partial even when no file fits the format |
| `LayoutQuery` has `lang: Option<LangIdx>` and `exact_layout` instead of `default_lang: bool` | the default language is `LangIdx` 0; standalone pages have no language; `exact_layout` is Go's `LayoutFromUserMustMatch` |
| `Templates::shortcode`/`hook` assume a regular page; `LayoutStore::shortcode`/`hook` take the kind | the kind never changes a hook or shortcode winner in the oracle, but the full query is available |
| `useEmbedded: always` considers only embedded link/image hooks | the Go implementation's documented meaning; the oracle has no case that distinguishes it |
