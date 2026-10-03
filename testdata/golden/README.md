# Golden data from the Go build (T01)

What the Go version produced for the target sites, as the **structural oracle** of the Rust
rewrite (`docs/rust-port/REWRITE_PLAN.md` §6.4, §7.2, §7.3). Output bytes do not have to match:
the Rust side is compared file set, URLs, templates, links, text and assets, level by level.

Everything here was written by `tools/dev/oracle.sh` (never by hand, except the image recipes
of `images/manifest.json`) and is frozen: the Go tree and oracle.sh are at 44529028. To
regenerate, run this in a worktree of that commit (`git worktree add <dir> 44529028`) and copy
the results, which it writes to its `rust/testdata/golden/` (or `$FUGO_GOLDEN`), here:

```sh
export FUGO_NODE_MODULES=$PWD/tools/dev/node_modules   # this worktree's own (below)
tools/dev/node.sh              # the pinned node modules (once; network)
tools/dev/oracle.sh install    # the Go binaries (once; Go + module cache)
tools/dev/oracle.sh sites      # manifests + structure dumps of every label below
tools/dev/oracle.sh images     # the golden images
tools/dev/oracle.sh check      # regenerate into a temporary directory and diff (idempotency)
```

At that commit the binaries (the Go version's command and its `-structure` build) and the node
modules live,
gitignored, in `tools/dev/{bin,node_modules}` of the **main checkout**, so every worktree
shares them; `FUGO_TOOLS_BIN` and `FUGO_NODE_MODULES` override the locations (`oracle.sh
bin`, `node.sh path` print them). Its lock file has no esbuild, and its `node.sh` replaces the
modules of another lock, so it gets a directory of its own: the main checkout's modules stay the
ones the Rust tests use.

## Labels and file counts

| Label | Site (`tools/rust-port/i01/sites.py make <label> <dir>`) | Files (L1) | Golden files |
|---|---|---:|---|
| `testsite` | `testdata/upstream/testsite` + `testsite.txtar` | **56** = 55 in `public` + the stats file | manifests, structure |
| `docs-i01` | `testdata/legacy-docs/` (the legacy docs site) with `--docs-patches i01` | **888** = 887 in `public` + the stats file | manifests, structure |
| `docs-reduced` | `testdata/legacy-docs/` with `--docs-patches reduced` | **889** = 888 in `public` + the stats file | manifests, structure |
| `mini` | `testdata/oracle/commands/e2e/mini.txtar` | – | structure |
| `docs-live` | `testdata/legacy-docs/` with `--docs-patches live` (no patches) | **2373** = 2372 published + the stats file | the unminified manifest of the **published site** (below) |

The Go build writes its stats file into the project directory (next to its configuration file),
not into `publishDir`; the manifests list it under a `project:` key, the Go build's file name,
and count it, as the old port's harness did (it copied the file into the output tree). A count
without it is one less. Since T75 fugo writes no stats file, and `tools/dev/structdiff.py`
leaves the `project:` entries out: the gates compare 55, 887, 888 and 2372 files.

`docs-reduced` has one page more than `docs-i01`: `content/en/shortcodes/highlight.md` is removed
only in i01. The docs patch entries of both variants are `tools/rust-port/i01/patches.json`
(below).

## `docs-live`: the published documentation site

Not written by oracle.sh: `docs-live/manifest.unminified.json.gz` is the manifest of the site
https://getfugo.github.io/ as published — the repository getfugo/getfugo.github.io at
`a1928152d8320bfa4db9b363298d038c9841f4a1` ("Update v0.148.2", 2025-10-13T15:05:28Z), which the
Go release workflow wrote with `npm install` and the Go version's build in the docs (version
0.149.0-DEV, no `--minify`, the production environment, network access) — from the same legacy
docs content this repository has. Gate A-D3 (`crates/cli/tests/it/docs.rs`, `compare.sh docs-live`) compares the
Rust build of those docs (`testdata/legacy-docs/`, the Go tree's `docs/`) without patches with it. To regenerate (the published files are frozen
at that commit; the manifest only changes with manifest.py):

```sh
git clone https://github.com/getfugo/getfugo.github.io <pub>
git -C <pub> checkout a1928152d8320bfa4db9b363298d038c9841f4a1 && rm -rf <pub>/.git
python3 tools/rust-port/i01/sites.py make docs-live <proj>/docs-live    # base URL, static/
cp testdata/legacy-docs/<stats file> <proj>/docs-live/build_stats.json   # the Go build's stats (below)
python3 tools/dev/manifest.py extract <pub> --project <proj>/docs-live --levels L1,L2,L3,L4 \
  --site docs-live --pass unminified --full-text -o testdata/golden/docs-live/manifest.unminified.json.gz
```

It differs from the other labels: one pass (the site is published unminified), extracted with
L4 (structdiff takes L4 from the unminified pass when neither side has a minified one) and with
the full text (`--full-text`, so reports show the word hunks against the reference); no
structure dump (the published site has none). The project file is the stats file at the root of
`testdata/legacy-docs/` (the Go build's file name, `GO_STATS_KEY`), which the Go docs build wrote
and committed with that content.
The candidate builds at the clock of the published build, `2025-10-13T15:00:00Z` (13 deprecated
pages have an `expiryDate` between then and today), with the GetRemote responses of that day
(`sites.py cache docs-live`, `tools/rust-port/testdata/getremote-cache/docs-live/README.md`).

## How the Go builds run

Per label and pass, `sites.py` writes the site afresh outside the repository, and the Go binary
builds it from the site directory with `--clock 2026-09-27T12:00:00Z [--minify] -d <out>` in a
clean environment: `HOME` and the Go program's cache-directory environment variable in the work
directory (the cache holds the site's golden GetRemote entries, `sites.py cache <label>`),
`TZ=UTC`, its worker-multiplier variable at 1 (one last writer for colliding targets), every proxy variable pointing at a refusing port
(`127.0.0.1:9`: outbound HTTP disabled, GetRemote is served from the cache or fails), and the
node modules as a `node_modules` symlink in the site plus `node_modules/.bin` on `PATH`.

| Pass | Flags | Golden file | Levels |
|---|---|---|---|
| minified | `--minify` | `<label>/manifest.minified.json[.gz]` | L1, L4 |
| unminified | – | `<label>/manifest.unminified.json[.gz]` | L1, L2, L3 |
| structure | – (the overlaid binary) | `<label>/structure.json[.gz]` | – |

A file over 256 KiB is stored gzipped (deterministically: no name, mtime 0); readers accept both
names (`ssg_testkit::fixture::read_json` does). JSON everywhere has sorted keys and one
entry per line.

## `structure.json` (schema `ssg-structure/1`)

Written by `tools/go-oracle/structure` at 44529028 (the Go version's command line built with
recording hooks, `go build -overlay`): what the Go build did, per (language, page, output
format). Read by
`crates/layouts/tests/it/structure.rs` (T30: `config`, `records[].template/baseof`),
`crates/site/tests/it/golden.rs` (T23b: targets, permalinks, resources),
`crates/nav/tests/it/structure.rs` (T24: `aliases`) and the parity tests.

```json
{
"aliases": [ {"alias": "/old-about", "format": "html", "from": "/old-about/index.html",
              "kind": "front matter", "lang": "en", "path": "/about",
              "permalink": "https://example.org/about/"} ],
"config": { "baseURLs": {"en": "https://example.org/"}, "contentSuffixes": […],
            "defaultContentLanguage": "en", "defaultOutputFormat": "html",
            "disabledLanguages": [], "languageIndex": {"en": 0, "nn": 1}, "mediaTypes": […],
            "outputFormats": […], "renderHookImage": "auto", "renderHookLink": "auto",
            "taxonomies": {"category": "categories", "tag": "tags"} },
"layouts": [ {"category": "layout", "desc": {"kind": "taxonomy", "mediaType": "text/html",
              "outputFormat": "html"}, "file": "taxonomy/list.html", "key": "", "legacy": true,
              "name": "taxonomy.html"} ],
"pagerAliases": [ {"alias": "/posts/page/1/index.html", "format": "html",
                   "from": "/posts/page/1/index.html", "kind": "pager", "lang": "en",
                   "path": "/posts", "permalink": "https://example.org/posts/"} ],
"pages": [ {"kind": "section", "lang": "en", "outputs": ["html", "rss"], "path": "/posts"} ],
"records": [ {"baseof": "baseof.html", "baseofFile": "_default/baseof.html", "format": "html",
              "kind": "page", "lang": "en", "layout": "", "pagers": 3, "path": "/posts/p1",
              "permalink": "https://example.org/posts/p1/", "relPermalink": "/posts/p1/",
              "target": "/posts/p1/index.html", "template": "single.html",
              "templateFile": "_default/single.html"} ],
"resources": [ {"lang": "en", "name": "sub/nested.txt", "path": "/biscuit/koalas-march-chocolate",
                "relPermalink": "/biscuit/koalas-march-chocolate/sub/nested.txt",
                "target": "/biscuit/koalas-march-chocolate/sub/nested.txt"} ],
"schema": "ssg-structure/1"
}
```

- **`records`**: one per (page, output format) Go's page renderer resolved a template for,
  i.e. every render of pager 1 (standalone pages such as 404, robots.txt and the sitemaps only
  where Go renders them: robots once, 404 and sitemap once per language). Sorted by
  (`lang`, `path`, `kind`, `format`).
  - `lang`: the language key; `path`: the page's `.Path` (`/` for the home page, `/404`,
    `/_robots.txt`, `/_sitemap.xml`, `/_sitemapindex.xml`); `kind`: `.Kind`; `format`: the
    output format name (the 404 page renders in format `404`, robots.txt in `robots`);
    `layout`: the front matter `layout` (`""`: none).
  - `lookupPath`: only when it differs from `path`: the path Go's template lookup walks, the
    page path with its first segment replaced by the front matter `type`
    (`PathInfo().BaseReTyped(type)`, e.g. `/seo` for a page `/about` with `type: seo`).
  - `template` / `baseof`: the layout and base template Go chose, as **normalised v0.146
    names** relative to `layouts/` (`""`: none; a (page, format) with no template is recorded
    with `template: ""` and `written: false`). Normalisation: the store's own conversion of
    legacy paths (`_default/` dropped, `partials/` → `_partials/`, `shortcodes/` →
    `_shortcodes/`, `<id>-baseof` → `baseof.<id>`, lower case), plus the mappings the store
    keeps only in its descriptors: the legacy home layout `index` is `home` (`index.html` →
    `home.html`, `_default/index.json` → `home.json`), and a template inserted through the
    store's legacy taxonomy/term/section mapping is `<tree key>/<kind><identifiers>`
    (`taxonomy/list.html` → `taxonomy.html`, `term/term.html` → `term.html`,
    `taxonomy/tag.terms.html` → `tags/taxonomy.html`). The Go implementation's embedded templates
    have their plain names (`rss.xml`, `sitemap.xml`, `robots.txt`, …).
  - `templateFile` / `baseofFile`: only when it differs from the name: the file the template
    came from, relative to `layouts/` (original case; `_embedded/<name>` for an embedded
    template). E.g. the term pages of a site with a legacy `term/term.html`: `template: "term.html"`,
    `templateFile: "term/term.html"`; mini's term pages use `_default/taxonomy.html` through
    its legacy term mapping, which replaces the store entry of `_default/term.html`.
  - `target`: the file below `publishDir`, leading slash (`TargetFilename`, language directory
    included); `relPermalink` / `permalink`: the links of this format
    (`.OutputFormats.Get <format>`, e.g. `/posts/index.xml` for a section's RSS; the page's
    own `.RelPermalink` is the one of its first format), escaped as templates print them.
  - `written`: only when `false`: the template rendered nothing, so Go wrote no file.
  - `pagers`: only for pages that paginated in this format: the number of pagers
    (`TotalPages`); pagers 2…N are written below `<page>/<pagination.path>/<n>/`.
- **`aliases`**: every front matter alias file (`kind: "front matter"`) and the main-language
  redirect (`kind: "redirect"`, `lang` and `path` empty), sorted by `from`. `alias` is the
  alias as Go publishes it (after relative/ugly/multihost resolution), `from` the file below
  `publishDir`, `lang`/`path` the page it redirects to, `permalink` the redirect target.
  (T24's reader reads `from`, `lang`, `path`, `format`, `kind`; the site README's `target` is
  `from` here.)
- **`pagerAliases`**: the `page/1` redirect files of paginated pages (`kind: "pager"`), same
  fields. They are kept out of `aliases` because they depend on what templates paginate (T24
  checks them with pagination).
- **`pages`**: every page of every language's page tree, rendered or not (headless bundles,
  `build.render: never`), with the names of its output formats (`.OutputFormats`). Sorted by
  (`lang`, `path`, `kind`).
- **`resources`**: every bundle resource in a page's `.Resources` (bundled pages excepted), per
  language, including resources a translation shares with the default language: `name` is
  `NameNormalized` (the path below the bundle), `relPermalink` as templates print it, `target`
  the file below `publishDir` (`targets` when there is more than one, multihost), `publish:
  false` when the page's `build.publishResources` is false. Sorted by (`lang`, `path`, `name`).
- **`layouts`**: every template the Go store read from the site's `layouts/` (themes included;
  embedded and inline templates are not listed), sorted by `file`: its normalised `name`, the
  store `category` (`layout`, `baseof`, `partial`, `shortcode`, `markup`), the tree `key`, the
  non-empty descriptor fields (`kind`, `layout`, `lang`, `outputFormat`, `mediaType`,
  `variant1`, `variant2`, `plainText`) and `legacy: true` for an entry the legacy mapping
  added. A legacy file appears once per store entry (e.g. `_default/simple.html` is also the
  section template `simple/section.html` of a section named `simple`).
- **`config`**: what the template store depends on, as `oracle/tplimpl/store/*.json.gz` dumps
  it, plus `baseURLs` (per language).

## `manifest.<pass>.json` (schema `ssg-manifest/1`)

Written by `tools/dev/manifest.py extract` over the output directory (the same extractor
runs over the Rust output):

```json
{
"baseURLs": ["https://example.org/"],
"count": 56,
"files": {
"404.html": {"L2": {"links": ["/", "/img/404.png"], "rel": [], "title": []},
             "L3": {"ids": [], "text": {"len": 4, "sha256": "…", "words": 1}}, "type": "html"},
"css/site.css": {"L4": {"nonEmpty": true, "referenced": true}, "sha256": "…", "size": 18,
                 "static": true, "type": "css"},
"posts/page/1/index.html": {"L2": {"alias": "/posts/"}, "type": "alias"},
"project:<stats file>": {"L3": {"classes": […], "ids": […], "tags": […]}, "type": "stats"}
},
"levels": ["L1", "L2", "L3"],
"pass": "unminified",
"schema": "ssg-manifest/1",
"site": "testsite"
}
```

- `files` is keyed by the path below `publishDir` (`/`-separated); `project:<name>` keys are
  files the Go build writes into the project directory (its stats file, `GO_STATS_KEY`). `count` is their number.
- `type`: `html`, `alias` (an HTML redirect page: `<meta http-equiv=refresh>`), `xml`, `json`
  (`.json`, `.webmanifest`), `lines` (`_redirects`, `_headers`, `robots.txt`), `css`, `js`,
  `image`, `stats`, `other`.
- **L1**: the key, and `norm` when the normalised path differs: `_hu_<hex>` → `_hu_H`,
  fingerprints `.<16–64 hex>.` → `.H.`. L1 compares the multisets of normalised paths.
- **L2** (unminified pass): `html`: `title` (the texts of `<title>`), `rel` (`[rel, href,
  hreflang, type]` of every `<link rel=canonical|alternate>`), `links` (the sorted set of
  internal `href`/`src`/`srcset` URLs); `alias`: `alias` (the redirect target); `xml`: `items`
  (`"link <url>"`, `"loc <url>"`, `"guid <url>"` element texts and `"<element> href <url>"`
  attributes, in document order); `json`: `keys` (the key paths, array indices as `[]`) and
  `urls` (`"<key path> <url>"` for every string leaf that starts with `/` or holds `://`);
  `lines`: `lines` (the sorted set of non-empty lines, whitespace collapsed). Every URL is
  percent-decoded and NFC-normalised; an internal URL (below a base URL, root-relative, or
  relative to the page) becomes a site path with the L1 normalisation applied, query and
  fragment kept (`/functions/images/mask/#usage`); external URLs are kept whole.
- **L3** (unminified pass): `html`: `text` (the visible text: tags, comments, `script` and
  `style` removed, entities decoded, typographic quotes/dashes/ellipsis/nbsp mapped to ASCII,
  whitespace collapsed; its `sha256`, `len` in characters and `words`; `manifest.py
  --full-text` adds the text as `t`) and `ids` (the `id`s of `h1`–`h6` in document order);
  `stats`: the `tags`, `classes` and `ids` sets of the stats file.
- **L4** (minified pass): `size` and `sha256` of every file, `static: true` for files that
  exist below the site's `static/`; `image`: `image` = `[width, height, format]` from the file
  header (`jpeg`, `png`, `webp`, `gif`, `bmp`, `ico`); `css`/`js`: `nonEmpty` and
  `referenced` (some HTML page links it).

`manifest.py summary <file>…` prints the counts of manifests and structure dumps.

## `images/`

`images/manifest.json` holds the 20 recipes of the PSNR gate (T41,
`crates/images/tests/it/psnr.rs`), the image operations the target sites run: an array of
`{"golden": "<file in images/>", "source": "<path from the repository root>", "steps":
[{"spec": "<processing spec>"} | {"filters": [<ssg_images::ImageFilter JSON>]}]}` (each step
applies to the previous result; filter `image` paths are relative to the repository root; no
per-recipe `imaging`, all use the default `[imaging]`). Paths recorded below `rust/` (where the
workspace was until it moved to the root) are read without that prefix (`repo_file` of
`ssg_testkit::fixture` and of `sites.py`). `oracle.sh images` (44529028) writes the
recipes as a Go site (`sites.py make images`: `.Process` for a spec, `images.Filter` with the
`images.*` functions for filters), builds it with the Go binary and copies each result here
under its `golden` name.

## Docs patch variants: `tools/rust-port/i01/patches.json`

Not in this directory: `sites.py patches` writes it next to `sites.py` from its `DOCS_REMOVE`,
`DOCS_REPLACE` and `DOCS_WRITE` lists, and `sites.py patches --check` (which `oracle.sh sites`
ran before the docs labels) asserts that it is current and that the Tera patch files of
`sites/docs/patches/<variant>/` correspond 1:1 to its layout entries. Schema
`ssg-docs-patches/1`: `variants` (`["i01", "reduced", "live"]`; `live` has no patches but the
removal of the committed stats file) and `patches`, in application order,
each `{"op": "remove" | "replace" | "write", "file": "<path in the site>", "old"/"new"
(replace), "content" (write), "variants": [...], "why": "...", "tera": "<path below
sites/docs/patches/<variant>/>" | null}`. A patch of a file below `layouts/` has a Tera
counterpart at the same path in every variant it belongs to; a `null` `tera` means the patch
changes the site input both builds share (content, config, JS assets).
