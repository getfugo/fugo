# ssg-view

Views and render state (REWRITE_PLAN.md §2.5). **State: T33.** The views are built directly
from `ssg_site::Model`, `ssg-nav` (menus, aliases, pagination lists), `ssg-resources`
(resource values) and the content the `ContentRenderer` produces (`ssg_markup` outputs).
The render-state types are final; `ssg_sitefuncs::{Handles, register}` is the frozen stub
T34 builds against.

## View cache

```rust
pub struct ViewInputs { pub model: Arc<Model>, pub store: Arc<ResourceStore>, pub menus: Arc<Menus> }
impl ViewCache {
    pub fn new(i: ViewInputs) -> Result<Self, ViewError>;          // C0: Meta generation; bundle files registered in the store
    pub fn freeze(&self, contents: &BTreeMap<HookVariant, Contents>);   // D: one Full generation per variant (second call ignored)
    pub fn freeze_from(&self, r: &dyn ContentRenderer, variants: &[HookVariant]) -> Result<(), ContentError>;
    pub fn generation(&self, phase: Phase, v: HookVariant) -> &ViewGeneration; // Content → Meta; else Full(v), Full(Html), Meta before D
    pub fn meta(&self) -> &ViewGeneration;  pub fn variants(&self) -> Vec<HookVariant>;  pub fn is_frozen(&self) -> bool;
    pub fn model(&self) -> &Arc<Model>;  pub fn nav(&self) -> &NavSite;  pub fn store(&self) -> &Arc<ResourceStore>;
    pub fn menus(&self) -> &Arc<Menus>;  pub fn resources(&self, p: PageId) -> &[ResourceId];  pub fn contains(&self, p: PageId) -> bool;
}
pub struct ViewGeneration { pub summaries: IdVec<PageId, Value>, pub links: IdVec<PageId, Value>, pub sites: IdVec<LangIdx, Value>, /* full: OnceLock per page */ }
impl ViewGeneration {
    pub fn full(&self, p: PageId) -> Value;        // summary + relations, built once (deref, get_page)
    pub fn page_value(&self, p: PageId) -> Value;  // the same value for a layout job, not kept
    pub fn list(&self, ids: &[PageId]) -> Value;  pub fn opt(&self, id: Option<PageId>) -> Value;
}
```

- **Generations.** Meta (content phase): summaries without content keys (`raw_content`, the
  source, is known after parsing, so it is a summary key in every generation). Full (one per
  hook variant, frozen in a `OnceLock` before any layout renders): summaries with the
  `ContentView` keys of that variant. Every list (relations, `site.*` lists, terms, pagers)
  holds **summary** values of its own generation: acyclic, one allocation per page shared by
  every list (the tests check pointer equality). `full`/`page_value` add `PageRelations`.
- **Sharing.** Everything content-independent (params, output formats, resources, terms,
  links, languages, menus, `site.data`, `site.config`, `raw_content`) is serialised once for
  all generations; media types, sitemap settings, equal dates and empty lists are shared
  across pages; variants whose content of a page is the same `Arc<RenderedContent>` share its
  values.
- **Keys.** `views::{PAGE_SUMMARY_KEYS, CONTENT_KEYS, PAGE_RELATION_KEYS, SITE_KEYS,
  PAGE_LINK_KEYS, RESOURCE_KEYS, MENU_ENTRY_KEYS, PAGER_KEYS}`: every documented key is always
  present (`none` when absent). Beyond §2.5: page `name` (`.Name`), `site.main_sections`, menu
  `key_name` and `parent`. `site.config` is snake case: `services.{rss.limit,
  google_analytics.id, disqus.shortname, instagram|x|twitter.disable_inline_css}`,
  `privacy.<disqus|google_analytics|instagram|twitter|vimeo|x|youtube>.{disable, simple,
  enable_dnt, respect_do_not_track, privacy_enhanced}` (every switch for every service).
- `sitemap.priority` is an integer when it is integral (`0`, `1`, `-1`), else a float: Tera
  prints the float `0.0` as `0.0`, Go's `{{ .Sitemap.Priority }}` as `0` (the embedded
  `sitemap.xml` prints it).
- `alternative_output_formats` is the list of output formats other than the page's primary
  one (a full value is per page, not per rendered format; templates that need "other than the
  current" compare with `output_format.name`).

## Resources

`page_resources(&Model, &ResourceStore)` registers every bundle file of the model in the store
(a resource a content adapter added through `ResourceStore::register_adapter_resource`, with its
bytes, name, title and params)
(publish policy from the model: eager, on reference; bundled content pages `Never`) and
applies the page's `resources` front matter (`apply_meta`; files re-sorted by type and name
when renamed, bundled pages after them as `page` resources with `page_id`). `resource_view(store,
id)` is the value of any store resource; `post_processed_view(store, id)` of `post_process`.

**Pending transforms (T42's "for the template layer").** A pending result's links, name and
media type are final, so its view is built without computing it. A `fingerprint` of a pending
resource has provisional links and no integrity: its view carries `__nh_pp_<n>_<field>__`
placeholders for `rel_permalink`, `permalink` and `data.integrity` (`ResourceStore::post_process`),
so the output is held and patched in E5 (the Go implementation's laziness). The `fingerprint`
filter computes the others at the call (`ResourceStore::waits_for_e5`), so their views are final. `width`
and `height` are known for every image (`ResourceStore::image_size`): a processed image's
planned size, else the size in the source's header (read once per file, no pixels decoded);
none for other resources and undecodable images.

## Render state

Frozen by T38 and kept: `SCOPE_KEY`, `MAX_DEPTH`, `Phase`, `HookVariant`, `Stage`,
`RenderScope { page, lang, format, pager, phase, variant, frame, txn, depth, chain }` with
`layout`, `from_state`, `child`, `too_deep`, `to_value`, and the `ContentRenderer` trait
(`content`, `fragments`, `render_shortcodes`, `render_markdown`, `render_template`). No
thread-locals: the scope is the context value `__nh`. Content adapters (`_content.html`) added
`Phase::Adapter` (it sees the Meta generation, like the content phase) and
`RenderScope::adapter: Option<u32>`, the adapter run of such a render (not serialised when
`None`).

| Type | API |
|---|---|
| `PaginationRecorder` | `paginator(page, format, at, make) -> Arc<Recorded>` (first call records), `paginate(page, format, at, Pagination) -> Result<Arc<Recorded>, PaginationConflict>` (equal: reuse; else both positions), `get`, `recorded()`; `Recorded { pagination: Arc<ssg_nav::Pagination>, first_call: Option<Position> }`, `total_pages()` (≥ 1), `page(n)` |
| `PageStores` | `new(pages)`, `begin() -> TxnId`, `set(txn, page, key, value)` (buffered in a transaction, else direct), `get(txn, page, key)` (own writes first), `commit(txn)`, `discard(txn)` |
| `DeferredRegistry` | `register(key, Deferred { template, data })` (first wins), `entries()` |
| Pagers | `pager_view(&ViewGeneration, &Recorded, n, url) -> PagerView`, `pager_url(model, page, format, n)`, `page_target(model, page, format, pager) -> (TargetPaths, Links)` |
| `NavSite` | `ssg_nav::NavModel` over the model (menus, `page_aliases`, `default_pagination_list`, related index) |

## Tests (`cargo test -p ssg-view`)

- `views`: a bilingual site with every kind (home, section, page, leaf bundle with resources
  and a bundled page, taxonomy, term, 404, sitemap, sitemap index, robots.txt): Meta and Full
  generations for `Html` and a `json` variant (fallback to `Html`, content phase → Meta),
  every documented key printed for every kind (pages, links, sites, resources, menus, pagers,
  `site.config` switches), pointer-equal list entries, relations and site values, resource
  metadata and pending-fingerprint placeholders, `freeze_from` a `ContentRenderer`, insta
  snapshots (`tests/it/snapshots`).
- `state`: pagination recorder (first call, identical re-call, conflict with both positions),
  page-store transactions, deferred registry. `scope`: the scope through a Tera render.
- `memory::real_sites` (ignored): `FUGO_SITES=<dir>[:<dir>…] cargo test -p ssg-view
  real_sites -- --ignored --nocapture` with sites from `tools/rust-port/i01/sites.py make`:
  every page's full value in the Meta and two Full generations has every key, lists share
  summaries, and dhat measures the heap kept. Last run:

| Site | Pages | Model kept | Views kept (Meta + Full, job values) | + every full value cached | worst (3 generations, all cached) |
|---|---|---|---|---|---|
| testsite | 23 | 0.38 MB | 0.33 MB (0.86×) | 1.22× | 2.09× |
| docs | 948 | 9.49 MB | 17.6 MB (**1.85×**, asserted < 2×) | 2.42× | 3.80× |

The views copy the content strings (Tera 2 builds string values from `&str` only):
`raw_content` (1.4 MB on docs) and the rendered HTML, summary, plain text and TOC (3.8 MB).
