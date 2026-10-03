# Completeness critique of the site-layer design (first pass)

> **Historical (old port).** Paths under `crates/` refer to the byte-parity port that T00 of
> [`REWRITE_PLAN.md`](../REWRITE_PLAN.md) deleted; it is recoverable with
> `git show go-parity-final:crates/<path>` (commit `be02933a`, local tag).

Written by an adversarial reviewer of the old port's layer design document in `crates/` ("the layer design" below), crates/WAVE_B_PLAN.json and the nh-* skeleton. The architect started a revision addressing these points; that revision was interrupted when the local session stopped, so check each point against the current layer design before relying on it.

**Site-layer design review: the layer design, WAVE_B_PLAN.json and the nh-* skeleton**

The skeleton compiles and the module ownership is clean, but it compiles only because every Wave A dependency is commented out and all 25 crates carry blanket `#![allow(unused, dead_code, …)]`. The main risks are the unpinned gotemplate contract and site-build module ownership that runs against the declared task dependencies, so several tasks cannot pass their acceptance tests in the planned order.

**What checked out:**
- **Compile:** `cargo check --offline` passes on all 25 nh-* crates with 0 errors and 0 warnings.
- **Ownership:** no module is claimed twice, and every `Owner:` line in the skeleton matches the plan.
- **Cycles:** neither the task graph nor the crate graph (from Cargo.toml) has a cycle.
- **Go file coverage:** every executed Go file of the Go version has a skeleton module, except files that do not need one:
  - the gotemplate fork (a Wave A crate);
  - `lazy/`, `para`/`rungroup` and `bufferpool`;
  - the flag-init code of unused CLI subcommands;
  - `docshelper`, `warpc` and `esbuild/batch.go`.
- **Template API:** every receiver/method pair in TI A.2 has an owner, except the one in P1-7.

---

**P0: blocking, or will stop tasks from meeting their acceptance tests**

1. **The gotemplate contract is not pinned.**
   - Evidence: `nh-tplimpl/src/engine.rs` uses placeholder types (`Arc<()>`), and `crates/gotemplate` does not exist yet. The "required engine capabilities" are written down only on the consuming side.
   - Several of the Go implementation's semantics must live inside the gotemplate engine and cannot be patched later in `engine.rs`:
     - Truthiness must go through a hook. Go's `isTrue` is `hreflect.IsTruthfulValue`, which calls `IsZero` on objects, `time.Time` and named maps such as `maps.Params`. `ExecHelper` has no `is_true` method.
     - Methods are looked up before map keys, including on `Value::List`/`Value::Map` named types (through the helper).
     - `and`/`or` return the deciding operand, not a bool.
     - Go's `evalArg`/`validateType` rules apply: ideal constants, and invalid values converted to typed zero or nil.
     - `printValue` distinguishes an invalid value (`<no value>`) from a nil pointer or nil non-empty interface (`<nil>`).
     - html/template escapers unwrap `PrintableValue`.
     - The `mainsections` special case compares map pointer identity (in Rust: `Arc::ptr_eq` with the site params).
     - The context value must reach every func and method call, and nested `execute_with_context` calls from inside method calls must work (render hooks, partials, `ExecuteAsTemplate`).
   - Fix: turn the `engine.rs` requirements into a trait or spec that the Wave A gotemplate task implements as its acceptance contract, with T13 as reviewer. Do this now, while that API is still open. Add `ExecHelper::is_true(&Value) -> bool`.

2. **Site-build module ownership inverts the declared task dependencies.**
   - T20 is declared before T21/T22/T23, but its modules call their code:
     - Inserting into the content tree needs `ContentNodeShifter`, `PageTrees` and `newPageMap`, which live in `content_map_page.rs` (T21).
     - `page__new` calls `pageMeta.parseFrontMatter` and `newCachedContent` (`page__content.go:66,129`), which are in T22's `page__content.rs`.
     - `page__meta.initLazyProviders` (EX, 884-947) calls `newPagePaths` (T23), and `newPageOutput`/`newPageContentOutput` (T22).
   - Fix:
     - Move `content_map_page.go:118-259` and `696-976` (trees, shifter, `newPageMap`) into a T20 module.
     - Move the parse half of `page__content.go` (66-128 and 278-445) into a T20 module.
     - Move `initLazyProviders`/`initPage` to T23.
     - Or merge T20–T23 into two tasks.

3. **T13 depends on nh-doctree `SimpleTree` (owned by T20), but the plan does not say so.**
   - Evidence: the template store uses `SimpleTree`; `walk_path` and `longest_prefix` are still `todo!()`. T20 sits after T09 on the critical path, so it stalls T13 → T14 → T23.
   - Fix: split nh-doctree into its own early task (it only needs nh-common and nh-resource), or give `simpletree.rs` to T13.

4. **Several acceptance tests need tasks that are not in `depends_on`.**
   - **T09** needs T12: `related.DecodeConfig` and `navigation` menu decoding live in nh-page modules owned by T12.
   - **T15 and T18** (tpl namespaces hold `Arc<Deps>`) need a constructible `Deps`, which T23 owns. Fix: T23 ships a `Deps::for_tests(conf, lang)` builder early, or the namespace functions take narrow context structs.
   - **T22** requires `.Content`/`.Plain` to match with the real hooks. `render-image.html` needs `resources.Get`, `Resize`, `images.Overlay`/`Filter`, `path.Ext` and `printf`, so T10, T14, T15, T18 and T19 are all needed. Fix: have a Go oracle record every hook call's output, keyed by (page, hook kind, ordinal, output format), and replay it through a stub `HookRenderer`. Test real hooks in I01.
   - **T24** requires publish order plus identical stats file and CSS, which is a full build. T18 is not in its dependencies. Fix: add T18, or narrow T24 to render-loop order with a stub executor and move the full-build checks to I01.
   - **T13**'s probe-site outputs need `printf`, `eq`, `jsonify`, … from T18/T19. Fix: give T13 a minimal test FuncMap, or move the probe check to I01.

**P1: fidelity gaps that the interfaces cannot express yet**

5. **Page outputs must be shared by format name.**
   - Evidence: `page__meta.go:907-916` keeps a `created := map[string]*pageOutput`. Every global render-format index with the same name (for example en/html at index 0 and th/html at index 7) gets the same `*pageOutput`, so the paginator, content output (pco) and target paths are shared.
   - The skeleton has `outputs: Vec<PageOutput>` with one independent entry per index.
   - Fix: use `Vec<Arc<PageOutput>>` with the same `Arc` per name. Standalone pages have length 1.

6. **`shiftToOutputFormat` state cannot be expressed with `OnceLock`.**
   - Evidence: `page.go:691-694` resets a built paginator whenever a page is shifted on the rendering site, but `PagePaginator.init` is a `OnceLock`.
   - For other sites, Go swaps the page's content provider to a `LazyContentProvider` and resets it on each shift (`page.go:719-743`). It restores the real pco later. `PageOutput` has only `pco: OnceLock` and no slot for the current provider.
   - Fix:
     - Make the paginator init resettable (`Mutex<Option<…>>`).
     - Add `current_provider: Mutex<Pco | Lazy>` to `PageOutput`.
     - Make the lazy provider resettable.
     - Put these in T22's type, used by T23.

7. **`.Sitemap.ChangeFreq` has no owner.**
   - Evidence: `config.SitemapConfig` is called 1,730 times in `sitemap.xml`. It is missing from the §5.2 table, and `Page::sitemap()` returns a plain Rust struct with no `Object` implementation.
   - Fix: T04 implements `Object` for it as a Struct kind with fields ChangeFreq, Priority (float64), Filename and Disable. T23 wraps it in the page method table.

8. **There is no rule for which Go nils map to which Rust value, and §4.7 attributes `%!s(<nil>)` to the wrong kind of nil.**
   - `%!s(<nil>)` comes from an untyped nil: a missing Params key passed to `printf`.
   - A method that returns a nil interface (`.Parent` on home, `resource.Resource` nil) must become `Invalid`. `jsonLd.html:389` stores `.Parent` in a Scratch and then calls `.Parent` on the `Get` result. With `TypedNil` that call errors ("nil pointer evaluating"); with `Invalid` it quietly yields nothing, as in Go.
   - `.GetPage` on a page that is not found returns `page.NilPage`, which is `(*nopPage)(nil)`: a typed nil whose `IsZero` is true.
   - The value model has no variant for a nil non-empty interface; text/template prints that as `<nil>`, not `<no value>`.
   - Fix: add an explicit mapping table to the layer design's §4.7.

9. **Render hooks get the wrong template context.**
   - The layer design's §6.2 and the `HookRendererTemplate` doc say the context is rebuilt with the page set and `in_goldmark()`.
   - Go passes the caller's context through unchanged (`site.go:1509-1530`). `IsInGoldmark` is set only for `{{% %}}` shortcodes (`shortcode.go:331`). When it is set, `RenderShortcodes` wraps its output in the context markers' `Wrap` (`page__content.go:1123`), so setting it for hooks is not harmless.
   - Fix the documentation and the skeleton.

10. **Collation must follow Go per language.**
    - T03's acceptance says "en collator for both languages", but Go calls `collate.New(th)`, and the nh-langs skeleton already uses the language tag.
    - Fix: port the per-tag collators faithfully and run the 1.13M comparison pairs with both the `en` and `th` tags. Keep the rule that the collator is the one of the site current when a list is first computed (`h.current_site`).

11. **Processed images must be decoded from encoded bytes.**
    - Evidence: images.md §9.10. Each resize or filter decodes its parent's encoded JPEG/PNG bytes, and the overlay decodes the watermark's encoded PNG. Because Rust never reads `resources/_gen`, T14 must keep an in-memory "file cache" of those encoded bytes and decode from it, never reuse pixels.
    - This rule is missing from the layer design's §4.5/§6.6 and from T14's notes.
    - Fix: add it, plus a T14 test of chained `Resize` → `Filter` through the `ImageResource` API (T10 is only driven by the repro inputs).

12. **Caches must never compute while holding a lock.**
    - `PageMap.cache_pages1/2`, the sites struct's `cache_pages`, `CachedContent.scopes` and the `shortcode_state` Mutex are plain `Mutex`es.
    - Content rendering re-enters templates and other page queries. Holding a std `Mutex`, or re-entering a `OnceLock`, would deadlock.
    - Fix: use the dynacache pattern everywhere (compute outside the lock, first writer wins) and state the rule in the layer design.

**P2: documentation errors and plan hygiene**

13. **The collision tolerance does not exist.**
    - `golden/canonical` and `canonical2` are byte-identical (`diff -rq` gives 0 differences), and both equal the single-worker `out-seq1`.
    - §11.2, §13.4 and I02 say `--alt canonical2` absorbs the 26 collision files; it absorbs nothing.
    - The port must reproduce the single-worker last writer in tree-key order exactly.

14. **§7.2 misstates term titles.** The row "`.Title` = first value" is wrong. `.Title` is AP-title-case of the *last* `m.term`; `.Name` is the first value (CM §7.3).

15. **The §11.2 end-to-end command will fail as written.**
    - It omits the environment variable naming the esbuild binary. The private site has no esbuild in `node_modules`, and there is none on PATH.
    - The only binary is in scratch (`work/resources-pipeline/esbuild/...`), which is not durable.
    - Fix: add the variable and pin the binary somewhere permanent (T16/I01).

16. **Some parent module files have no owner.** `nh-resource-transformers/src/{resource_transformers.rs, resource_transformers/{cssjs,js,tocss}.rs, tocss/{sass,scss}.rs}` and `nh-tplfuncs/src/internal.rs` are shared between T15/T16 and T15/T19. Assign them explicitly to the crate lead.

17. **No crate has `[dev-dependencies]`.**
    - Every task needs a fixture reader, but each Cargo.toml belongs to the crate lead, and several crates are shared by 2–5 tasks.
    - Fix: add `serde_json` (or a small `nh-testutil` crate) as a dev-dependency everywhere now.
    - While doing that, uncomment the Wave A dependencies whose crate directories already exist (go-unicode, go-sort, go-html, go-path, …), each only once that crate passes `cargo check`, as README rule 4 requires.

18. **`page.Clear()` is in the wrong place.** It is an empty stub in `nh_page::page` (T11), but the sort cache it clears is T12's `pages_cache`, and Go calls it on every `Site.render`. Give it to T12 and have `site_render` (T24) call it.

19. **`.Site.Data` has no oracle.** `loadData`/`handleDataFile` (T23) decide the structure the comments partial reads. Add a dump fixture.

20. **Wrapper method sets need an explicit rule.**
    - `pageForRenderHooks` and `pageForShortcode` expose only the embedded interfaces (PageWithoutContent, TOC, and no-op Markup/Content), not the full `pageState` method set.
    - Shallower struct fields hide promoted methods, as `aliasPage.Permalink` already does correctly.
    - Add both rules to §5.1.

21. **Smaller inaccuracies:**
    - §10.20 assigns `shouldBuild` to T20; it is in `site.go`, owned by T23/T21.
    - §6.3 says "assign pids in Go's order". That is impossible (Go assigns them during parallel collection) and unnecessary (only identity depends on them).
    - `Deps.BuildState.counter` and `ResourceSpec.incr` duplicate the PostProcess id counter. Keep one, shared across sites like Go's `SpecCommon`.
    - The `resourceAdapter` template method table must include `Slice`. It is Go's `commonResource.Slice` (EX), which is how `slice r1…r5` becomes `resource.Resources`; `resources.Concat` rejects `[]interface{}`. Add a T14 fixture case for it.

Files reviewed and checked (no files were edited):
- the layer design (`crates/`)
- `crates/WAVE_B_PLAN.json`
- `crates/nh-*/`
- cargo check logs: `critic/check-*.log` in that session's scratch directory (not kept)