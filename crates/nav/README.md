# ssg-nav

Menus, pagination arithmetic and pager URLs, the related-content index and the alias plan
(REWRITE_PLAN.md §2.4, §3.3). The crate holds no state: the render session records the first
`paginator()`/`paginate()` call per (page, format) (`ssg-view`) and caches related indices
per candidate list.

Everything reads the site through the `NavModel` trait (final titles, links, relations, the
pages' output formats). The site `Model` implements it once T23b adds URLs and relations; the
oracle tests implement it over the Go fixtures' page dumps.

| Piece | API |
|---|---|
| Seam | `trait NavModel { page(id) -> PageFacts, tree_pages(lang), resolve_page_ref(lang, ref), is_ancestor(a, p), pages(p), regular_pages(p), site_regular_pages(lang), home(lang) }`; `PageFacts` (kind, section, titles, weight, dates, keywords, params, front matter menus, rel permalink, fragments, outputs `[(FormatId, TargetPaths)]`, `Rendering`, `ListMode`) |
| Menus | `build_menus(&m, &Config) -> (Menus, Vec<Diagnostic>)`, `build_site_menus(&m, lang, &MenuOptions { entries, section_pages_menu, urls })`; `Menus(IdVec<LangIdx, SiteMenus>)`; `SiteMenus { menus: BTreeMap<name, Vec<MenuEntry>>, page_menus }`, `menu(name)`, `is_menu_current(page, menu, &entry)`, `has_menu_current(&m, page, menu, &entry)`; `MenuEntry { identifier, name, title, url, page_ref, page, weight, parent, pre, post, params, children }`, `key_name()`, `has_children()`; `page_menu_entries(&PageFacts)`; `menu_order`, `compare_names`, `sort_by_name` |
| Pagination | `Pagination::new(PaginationItems::{Pages, Groups}, size)`, `total_items`, `total_pages` (0 for an empty list), `pager_count` (≥ 1), `pager(n)`, `pagers()`, `pages_of(&Pager)`; `Pager { number, slice: PagerSlice::{Pages(Range), Groups(Vec<GroupSlice>)} }`; `pager_paths(&UrlInputs, &PaginationConfig, n) -> TargetPaths`; `pager_alias(&UrlInputs, &OutputFormat, &PaginationConfig) -> Option<OutputPath>`; `default_pagination_list(&m, page)`; `resolve_pager_size(&[Value], configured)` |
| Related | `RelatedIndex::{new(&RelatedConfig), add(&m, page), finalize(), build(&m, &cfg, candidates), search(&m, &RelatedQuery { document, indices, fragments, named })} -> Related { pages, heading_filter }`; `related(&m, &cfg, candidates, page)` |
| Aliases | `alias_plan(&m, &Config) -> Vec<AliasPlan { from: OutputPath, to: PageId, format: FormatId, kind: AliasKind::{FrontMatter, LanguageRedirect} }>`; `page_aliases(&m, &cfg, lang)`, `language_redirect(&m, &cfg)`, `alias_target(alias, allow_root)` |

Rules, briefly: menu entries sort by weight (0 last), then name (case-folded, then bytes), then
identifier; configured entries are placed first (`pageRef` resolved, `/`-rooted URLs urlized
with the base path), then `sectionPagesMenu` entries, then the pages' own entries in tree order
(a key already in the menu is dropped with a warning); a missing parent is created with the
parent's name; a parent cycle loses the closing edge. Related matches rank by summed index
weight, then newest publish date, then name, then candidate order (Go leaves full ties random);
the normalised score and threshold use exact integer arithmetic. `page/1` redirects come from
`pager_alias`, because only the templates know what paginates.

## Acceptance (tests/it)

The oracle tests replay `testdata/oracle/page/{menus,pagination,related}` and the alias
files of `testdata/oracle/sitebuild/build/*` and print pass rates; every difference is exact
or a reviewed class of `expected_diffs.toml` (two oracle cases that call Go's decoders on
tables Go's configuration loader never produces):

| Family | Checks | Exact | Accepted |
|---|---|---|---|
| menus (assembled trees, page entries, IsMenuCurrent/HasMenuCurrent, sorts) | 1,927 | 100 % | – |
| menus config decode (ssg-config over `decode.json.gz`) | 9 | 88.9 % | 1 `config-menu-name-folded` (oracle artifact) |
| pagination (every build paginator, other sizes, groups, pager URLs) | 1,516 | 100 % | – |
| pager-size option | 15 | 100 % | – |
| related (Related, option maps, Add, Search, SearchNamed) | 13,672 | 100 % | – |
| related config decode (ssg-config) | 15 | 93.3 % | 1 `config-related-empty-accepted` (oracle artifact) |
| aliases (front matter + main-language redirect of the 25 Go builds that wrote redirects) | 60 | 100 % | – |

Not this crate's (skipped, counted): 24 pagination and 156 related cases that test template
argument conversion (a string or mixed list as `paginate`'s pages, an option map with a
non-page `document`), which belong to `ssg-sitefuncs`.

**Pending.** The `NavModel` implementation of the site `Model` (T23b); until then the alias test
takes the pages' output formats and links from the recorded renders, and `pageRef`s resolve by
path in the tests. The structure-oracle alias check (`structure.rs`) reads the alias rows of
`testdata/golden/<site>/structure.json[.gz]` (frozen at 44529028) but checks none until T23b
lands (it prints `PENDING T23b`).
