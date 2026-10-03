# ssg-site

Capture, assembly and structure of the `Model` (REWRITE_PLAN.md §2.4, phases A4–B5): T23a
(capture, tree, cascade, meta, filter) and T23b (made pages, URLs, relations, taxonomies,
translations, bundle resources, page references).

| Piece | API |
|---|---|
| Entry point | `load_model(Arc<Config>, &Vfs, &LoadModelOptions { clock, content: ContentFilter }) -> Result<Model, ModelError>` (no content adapters run); `LoadModelOptions::from_config(&cfg, clock)` |
| Content adapters | `capture_content(&Config, &Vfs) -> Captured` (phase A4; `Captured::adapters() -> &[ContentAdapter { file, info, lang }]`), then `assemble(Arc<Config>, Captured, Added { pages: Vec<AddedPage { adapter, lang, page: Arc<AdapterPage> }>, resources: Vec<AddedResource { adapter, lang, path, name, title, params, content: AddedContent::{Text, Resource} }> }, &LoadModelOptions)`; ssg-build runs the adapters in between |
| Model | `Model { config, pages: IdVec<PageId, Page>, sites: IdVec<LangIdx, SiteModel>, bundle_resources: IdVec<ResourceId, BundleResource>, data: Arc<Map>, diagnostics }`; `page(id)`, `bundle_owner(id)`, `page_name(id)` (`.Name`), `is_ancestor(a, b)`, `pager_paths(id, format, "/page/2")` |
| Page | `id, lang, kind, role: PageRole, key: ContentKey, source: Option<SourceFile>, path_info: PathInfo, meta: PageMeta`; T23b: `title, link_title, section, type, taxonomy: Option<TaxonomyIdx>, term: Option<TermIdx>, standalone: Option<FormatId>, formats: Vec<FormatId>, urls: Vec<PageUrl { format, paths: TargetPaths, links: Option<Links> }>, parent, ancestors, current_section, first_section, pages, regular_pages, sections, translations (= .AllTranslations), terms: Vec<(TaxonomyIdx, TermIdx)>, resources: Vec<ResourceId>`; `path()` (`.Path`), `name()`, `listed(ListScope::{Local, Global})`, `linked()`, `rendered()`, `url(format)`, `links()`, `dir_key()` |
| SiteModel | `lang, tree: SiteTree, resources: BTreeMap<ContentKey, ResourceId>, cascade`; T23b: `home, pages, regular_pages (.Site.Pages/.RegularPages), regular_pages_local (regular pages listed locally, default order: `.RegularPagesRecursive` of home and sections), taxonomies: IdVec<TaxonomyIdx, Taxonomy>, main_sections, last_mod, permalinks: PermalinkPatterns` |
| Taxonomies | `Taxonomy { def, page: Option<PageId>, terms: IdVec<TermIdx, Term> }` (terms by key), `listed_terms(&Model)` (`.Site.Taxonomies`: listed terms with members); `Term { key, term (.Data.Term), page, members: Vec<WeightedPage { page, weight, ordinal }>, parent, children }` (weight, then default order; hierarchical: members include the terms below), `key_of(&Term)` |
| BundleResource | `key, lang, file, info, page, adapter: Option<Arc<AddedResource>>`; T23b: `copy_of` (a `duplicateResourceFiles` copy), `owner`, `name` (as written below the owner), `name_normalized`, `target_base: Option<ResourceBase>`, `publish`; `target()`, `link()`. Feeds `ssg_resources::BundleResource { lang, file: file.abs, name, dir: target_base.link, policy: publish ? Eager : OnReference }` |
| References | `get_page(lang, ref, from)` (`.GetPage`), `site_get_page(lang, &[args])` (legacy kind-first `.Site.GetPage`), `ref_page(lang, ref, from)`, `ref_link(lang, &RefArgs { path, lang, output_format }, from, RefLink::{Permalink, RelPermalink})` → `Result<_, RefError>` (the caller maps errors to `refLinks`) |
| Trees | `SiteTree::{get, insert, remove, longest_prefix, descendants, iter}` (segment-wise) |
| Cascade | `CascadeIndex::{new, add_branch, received(key), in_force(key, own), branches}`; `meta::cascaded_params(cfg, index, lang, kind, key)` |
| Data | `data::load(&Vfs) -> Result<Data { map, diagnostics }, DataError>` |

## Phases

0. **Content adapters** (`_content.html`; Go's `_content.gotmpl`). Capture lists them
   (`Captured::adapters`; a `.gotmpl` adapter is `ModelError::GoContentAdapter`, with the hint
   to port it to Tera); ssg-build renders them with a model of the content files and
   [`assemble`] adds what they added. Each `add_page` map becomes a captured page after the
   files, with the adapter as its file (`.File`, `IsContentAdapter`), the path
   `/<path>/index.<suffix>` (`_index` for branch kinds), no front matter and `content.value` as
   its body; its meta is `ssg_page::meta_from_adapter` with the cascade's fields filling the
   map and its params the params (Go's `setMetaPost` for adapter pages). Each `add_resource`
   map becomes a bundle resource at its path, owned like a file; its bytes and metadata come
   from the adapter (`BundleResource::adapter`).

   Duplicates: a key (or resource path) a content file and an adapter both claim is the
   file's; one two adapters claim is the later adapter's, in the earlier one's place; both are
   `duplicate-content-path` (`duplicate-resource-path`) warnings, found through maps of
   (language, key), so assembling n added pages and resources takes linear time. A path one
   adapter run adds twice is already its last `add_page` (see ssg-sitefuncs). This is
   Go's last insert in the order of one collector worker, which queues a directory's
   adapters before its files and subdirectories; with several workers Go's result depends
   on scheduling. One case differs from that order: a `../` path onto a key that a file in an
   ancestor's (or an earlier sibling's) directory holds stays the file's here.

1. **Capture** (A4; rayon over files): `Vfs::discover_content` (its duplicates become
   `duplicate-content-path` warnings), read, `split_front_matter`, `decode_front_matter`,
   `capture_overrides`, the page's own `cascade`. Front matter `lang` moves a page only to an
   enabled language; `kind` is normalised; `path` re-parses the page's path. `data::load` runs
   alongside.
2. **Tree** (B1): kind from the override, else home, taxonomy (`/<plural>`), term (below it,
   segment-wise), section or page; pages of a disabled `page` kind are dropped. A key claimed
   twice keeps the first file and warns. Content files inside leaf bundles are
   `PageRole::Bundled` pages registered as bundle resources; other bundle files are
   `BundleResource`s of their language.
3. **Cascade → meta → dates** (B2, pages in parallel), then the **filter**: drafts, future and
   expired content, disabled kinds. Home, section and taxonomy pages stay switched off; other
   pages are removed with the bundle files below them (their dates still count for node dates,
   as in Go, which aggregates before it removes).
4. **Made pages** (B3, `nodes.rs`), per language in Go's order: a taxonomy page per
   configured taxonomy (unless taxonomy and term pages are both disabled); a section page per
   root section that has pages (named as its first page writes it: `/Upper Case/` → *Upper
   Cases*); the home page; the standalone pages, in the tree under Go's keys (`404`,
   `_robots`, `_sitemap`, `_sitemapindex`; `.Path` `/404`, `/_robots.txt`, …): 404 and a
   sitemap per language, robots.txt and the sitemap index once (default language) unless
   multihost, the index only for multilingual or subdirectory sites. Made pages get the
   cascade in force at their key and go through the filter.
5. **Terms** (`taxonomy.rs`): every linked page in tree order, per taxonomy in plural order,
   names terms in front matter (a string is one term; numbers and booleans are written out;
   a list holding a list or map names none; empty values are skipped). `/<plural>/<value>` is
   read as a content path (lower case, spaces → `-`, `/` nests); the page at that key is the
   term page, made when missing (unless a content term page there was removed by the filter).
   `.Data.Term` is the last value; the weight is `<plural>_weight` (a warning when it is not
   an integer). A **hierarchical** taxonomy (fugo's own, `[taxonomies.<singular>]
   hierarchical = true`): a value without `/` that is no top-level term names the one known
   term ending in it (term pages and the paths pages write; several: warning
   `taxonomy-ambiguous-term`, top-level); every term above a term is made (named from the
   written path); terms get `parent`/`children` and list the members of the terms below
   (smallest weight); `.Data.Term` is the last segment; template keys are paths below the
   taxonomy (`Taxonomy::key_of`); `.GetTerms` keeps the named terms.
6. **Relations** (`relations.rs`): titles (a page without a file: site title, pluralised and
   title-cased section name, taxonomy plural, term, `404 Page not found`), link titles,
   sections, types; `.Parent` (nearest branch page above; a bundled page's bundle),
   `.Ancestors`, `.CurrentSection`, `.FirstSection`, `.Sections`; **node dates** (a home,
   section or taxonomy page without dates takes the latest date/lastmod and the latest passed
   publish date below it, terms excepted; then term and taxonomy pages without dates take their
   terms' and members' dates; `.Site.Lastmod`); the **lists** in the default order with the
   language's collator (home/section: pages and sections directly in it; taxonomy: its terms
   at all levels, a hierarchical one its top-level terms, which with child terms are also its
   and its terms' `.Sections`; term: its members, weighted; standalone pages: the site's pages);
   `.Site.MainSections` (configured, else the root section with the most regular pages, the
   first by name on a tie).
7. **Translations** (`translations.rs`): a `translationKey` groups pages of any language;
   otherwise the linked pages at the page's `.Path` in every language (a standalone page's
   path is not its key: sitemaps have none). Language weight, date, link title, file.
8. **URLs** (`urls.rs`, rayon over pages): formats (front matter `outputs`, else the kind's;
   the standalone format; only the first for pages not rendered); per format
   `ssg_page::target_paths` (current section, slug/standalone base name/name, language
   prefixes — multihost writes to `/<lang>`, sitemaps are always in their language's directory
   on multilingual sites —, front matter `url` expanded when it holds `:` attributes, the
   `[permalinks]` pattern of the kind and section) and `links` for pages with a link.
9. **Bundle resources** (`resources.rs`): a file belongs to the page at the longest key above
   it in any language; the owner in the file's language names it (below the owner) and places
   it (the owner's resource directory in its primary format); publish with the owner when it
   is rendered and publishes resources. With `duplicateResourceFiles` (or multihost, or
   non-Markdown owners) every translation of the owner gets a copy (`copy_of`). `.Resources`
   of a page: the files its key owns, in its language else the first language that has the
   file (bundled pages only in their own language), plus a `translationKey`'s translations'
   files; files by media main type then name, then bundled pages in the default order. Front
   matter `resources` metadata is applied by `ssg-resources` (`ResourceStore::apply_meta`),
   which then re-sorts by the new names.
10. **References** (`refs.rs`): `.GetPage`/`ref` as semantics §12.4 (`.md` added to references
    without an extension, relative to the page asking — a branch's own directory, else its
    container; above the root is the root —, then from the root, then a file or directory next
    to the page's or the home page's file, then by unique page name).

Page ids follow the capture order (key, then language), then the made pages in phase order,
so the model is the same for any number of threads.

## Acceptance (tests/it)

`cargo test -p ssg-site -- --nocapture` prints the tallies. Every difference is exact or a
reviewed class of `expected_diffs.toml` (`[[class]]`, exact counts).

| Test | Fixtures | Result |
|---|---|---|
| `capture` | `oracle/sitebuild/capture/*` (9 sites) | per language tree pages and resource trees, page configs, duplicate warnings equal (T23a) |
| `assemble` | `oracle/sitebuild/assemble/*` (17 sites) | every page with a file: 1,248 pages, 19,776 fields equal, including the aggregated dates of content branch pages (T23a + node dates) |
| `structure` | `oracle/sitebuild/assemble/*` (17 sites, 1,461 pages) | 52,928 / 52,932 checks exact, 4 accepted: page set incl. every made page (1,460), title/link title/type/section/term (1,461 each), dates of every page (1,461), params and build of made pages (212), parent/current/first section (1,436 each), `.Sections` (349), rendered formats (1,436), **target file, link and resource directory per (page, format) (1,515)**, **`.OutputFormats` rel/permalinks (1,436)**, `.Pages`/`.RegularPages` per node (2,869), site pages/regular pages/home/lastmod/main sections (26 languages each), taxonomy terms and weighted members (110), `.GetTerms` (41), `.Site.GetPage` (11,317) and page-relative `GetPage`/ref lookups (2 × 10,052), **bundle resource URLs per (page, name) (61)** |
| `calls` | `oracle/sitebuild/site/*` (19 sites; recorded template calls) | 68,153 / 68,427 exact, 274 accepted: translations (2,600), relations (3,250), lists (1,950), links and output formats (1,950), dates (2,600), names and kinds (7,150), `.Eq`/`.IsAncestor`/`.IsDescendant`/`.InSection` (12,436), page `.GetPage` (4,707), `.GetTerms` (1,882), **`ref`/`relref`/`RefFrom`/`RelRefFrom` (27,820)**, site lists/taxonomies/lastmod/main sections (261), `.Site.GetPage` (1,821) |
| `build` | `oracle/sitebuild/build/*` (26 sites incl. the 9 build-* sites) | every (language, page, format) Go rendered is rendered here (1,647), its target is a file Go wrote (1,642 non-empty), pagers 2..N via `Model::pager_paths` are files Go wrote (344) |
| `golden` | `testdata/golden/<site>/structure.json[.gz]` + `FUGO_SITES` | the structure oracle gate (targets and permalinks per (page, format), resource URLs per (page, name)); skips with a message without the dump (the golden data is frozen at 44529028) or the site; a self-test runs the reader |
| `data`, `model` | as T23a | |
| `real_sites` (ignored) | `sites.py make docs\|testsite` | `FUGO_SITES=<dir>:… cargo test -p ssg-site real_sites -- --ignored --nocapture`: page counts, made pages, terms, outputs, shared target files |

**Segment-aware prefix lookup**: every ancestor lookup (parent, sections, owners, lists, node
dates, cascades) is segment-wise; on all 17 + 19 + 26 oracle sites it gives Go's result
except `/tagsfoo` of edge-tree (below), an accepted deviation.

## Structure-oracle dump (for T01)

`testdata/golden/<site>[-<variant>]/structure.json[.gz]`, read by
`crates/layouts/tests/it/structure.rs` (T30: `config`, `records[].template/baseof`) and
`crates/site/tests/it/golden.rs` (T23b):

```json
{ "config": { /* as oracle/tplimpl/store: defaultContentLanguage, languageIndex, … */ },
  "records": [ { "path": "/posts/p1", "kind": "page", "layout": "", "lang": "en",
                 "format": "html", "template": "single.html", "baseof": "baseof.html",
                 "target": "/posts/p1/index.html",
                 "relPermalink": "/posts/p1/",
                 "permalink": "https://example.org/posts/p1/" } ],
  "aliases": [ { "path": "/posts/p1", "lang": "en", "format": "html",
                 "alias": "/old/", "target": "/old/index.html" } ],
  "resources": [ { "path": "/posts/p1", "lang": "en", "name": "cover.jpg",
                   "relPermalink": "/posts/p1/Cover.JPG", "target": "/posts/p1/Cover.JPG" } ] }
```

- One record per (page, format) Go renders (pager 1 only; standalone pages once). `path` is
  the page's `.Path` (`/` for the home page, `/_robots.txt`), `kind` its `.Kind`, `lang` its
  language key, `format` the output format name.
- `target`: the file under `publishDir` with a leading slash (`targetPaths.TargetFilename`,
  multihost language directory included); `relPermalink`/`permalink`: `.OutputFormats.Get
  <format>` of the page (escaped, as templates print them).
- `aliases`: every alias file Go writes (front matter aliases, `page/1/`, the language
  redirect) with the page and format it points to (T24's alias plan).
- `resources`: every bundle file in a page's `.Resources` (bundled pages excepted): `name` is
  `NameNormalized` (the path below the page, lower case), `relPermalink` as templates print it,
  `target` the file under `publishDir`.

## Deviations from Go (`expected_diffs.toml`)

- **Taxonomy prefix is segment-wise**: `content/tagsfoo/_index.md` is a section, not a term of
  `tags` (Go's go-radix matches characters); edge-tree's home lists and calls that name it.
- **Bundle files belong to their owner**: a single-file page `leafy.md` does not also list the
  files of the bundle `leafy/b/`, and a file is named below its owner (`img.jpg`), not after
  the first page that walked it (`b/img.jpg`; same URL).
- **`ref`/`relref` from a bundled page** resolve (Go's bundled pages have no working site
  and return `""`).
- **Lists use the page's own language's collator** (Go: the collator of the site being
  rendered when a list is first computed); a term's `.RegularPages` are its regular members
  (Go caches `.Pages` and `.RegularPages` of a term under one key: whichever is asked first
  answers both). Neither occurs in the fixtures.
- **`.Resources` order** is a strict order (files by media main type then name, then bundled
  pages); Go's comparator is not a strict weak order when types differ.
- **Main-section ties** pick the first section by name (Go: map order).
- **YAML 1.2** data, **TOML local times** as strings, **CSV data files** as rows, data
  messages (T23a).

## For later tasks

- T24 (nav): `Page::pages`/`regular_pages`/`sections`/`translations`, `Model::pager_paths`,
  `Taxonomy::listed_terms`; next/prev and alias plans are not in the model (they derive from
  the lists and `PageUrl`s); `RegularPagesRecursive` filters `SiteModel::regular_pages_local`.
- T33 (view): `.Rel` of `.OutputFormats` (`canonical` for a page's only built-in format),
  `.Data`, `.Site.Taxonomies` keys (`to_lower(term.term)`), bundle-resource metadata and its
  re-sort, `refLinks` error level and not-found URL.
