# ssg-resources

The `ResourceStore` of a build (REWRITE_PLAN.md §2.1, §2.4, §3.4; tasks T40 and T42): bundle,
asset, remote and named-target resources, front matter `resources` metadata and the
`Resources` lookups, publishing by URL token, and the pipes (`src/pipes/`, T42): `fingerprint`,
`minify`, `to_css` (grass), `js_build` (rolldown), `post_process` placeholders and
`execute_as_template`. Every pipe runs in process; nothing runs an external program.

## API

| Item | What it is |
|---|---|
| `ResourceStore::new(StoreConfig)` | `StoreConfig::from_config(&Config, Option<Arc<Vfs>>, Option<Arc<ImageQueue>>)`: per-language base URL and target prefix (`/<lang>` on multihost sites), media types, the assets view, the image queue, `RemoteConfig`. |
| `Resource` | Immutable: `id`, `kind` (`Image` = processable raster, `Page`, `Text`, `Other`), `origin`, `media_type` (empty when the extension is unknown), `name`, `name_normalized`, `title`, `params`, `data`, `lang`, `target: OutputPath`, `link: UrlPath` (unescaped, without base path), `rel_permalink` (escaped, with the base path), `permalink`, `body` (`File`, `Bytes`, `PendingImage`, `Pending`), `policy` (`Eager`, `OnReference`, `Never`). `resource_type()`, `media_type_string()`, `integrity()`. |
| `resource(id)`, `resources()`, `content(id)` | Lookup; `.Content` (reads the file, or asks the image queue). |
| `get_asset(lang, path)`, `find_assets(lang, glob)`, `find_asset` | `resources.Get` / `Match` / `GetMatch` over the assets view. Assets are one resource per path (per language on multihost sites). |
| `register_bundle(&BundleResource)` | A page bundle file (`lang`, `file`, `name` relative to the bundle, link `dir`, `policy`); one resource per target. |
| `register_adapter_resource(&AdapterResource)` | A page resource a content adapter added (`add_resource`): its `body` (text bytes, or the file or bytes of a resource it passed), media type, `name`/`dir` as for a bundle file or the passed resource's own `place` (target and link), display name, title, params; a new resource per call. |
| `from_string`, `from_template_output`, `concat`, `copy` (target, …, `&CallSite`) | Resources that name a target path. `CallSite { lang, position }`. `template_outputs()` (T36): the ids `from_template_output` made, whose text the build scans for URL tokens. |
| `qr_code(text, &QrOptions, &CallSite)`, `QrOptions`, `qr_target` | `images.QR` (T72a): the PNG of `ssg_images::qr_png` (the Go implementation's bytes) as a named target at Go's path, `<targetDir>/qr_<hex>.png` with the hex of `hashing.HashStringHex(text, {Level, Scale, TargetDir})` (`gohash`, checked against Go's `TestQR`); `QrOptions` defaults to medium, 4, no directory. |
| `transform(id, Transform)` | A pipe (see [Pipes](#pipes-t42)); memoized per `(id, transform)`. `fingerprint` (md5, sha256, sha384, sha512; `HashAlgo::from_str`, `""` = sha256): `.<hex>` before the extension, `Data.Integrity` = SRI. |
| `realize(id)` | Computes a pending transform result (its record is replaced; the id stays). |
| `post_process(id) -> PostProcessId`, `post_processes()` (T36: every id so far), `PostProcessId::placeholder(PpField)`, `resolve_post_process(text)`, `pipes::has_placeholder` | `resources.PostProcess`: `__nh_pp_<n>_<field>__` placeholders (content, rel_permalink, permalink, integrity, media_type), filled in E5. |
| `execute_as_template(id, target, &dyn TemplateExecutor, &CallSite)` | `resources.ExecuteAsTemplate`; the template engine and the `data` context are the render layer's (`TemplateExecutor`). The result is a named target (`from_template_output`). |
| `meta::ResourceMeta::parse(&Value)`, `apply_meta(id, &meta)` | Front matter `resources` metadata; a new resource (same target and content) only when name, title or params change. |
| `meta::{get, get_match, matches, by_type}` over `impl Named` | `.Resources.Get/GetMatch/Match/ByType` (bundle pages implement `Named` too). |
| `image_input(id)`, `register_image(from, &Enqueued)`, `image_size(&Resource)` | The seam to `ssg-images`: what `ImageQueue::enqueue` reads (the file, the operation, or the bytes of a remote image or QR code, which the queue then holds: `ImageQueue::add_memory`), the resource of a queued operation (sibling target `<stem>_hu_<hash>.<ext>` named after its own source, `Body::PendingImage`), and `.Width`/`.Height` (planned size of a processed image, else the source's header, cached per file). The store never decodes pixels. |
| `mark_published(id)` | The `publish` filter. |
| `publish(tokens, &dyn Sink) -> PublishStats` | See below. `resolve_token(token)` for diagnostics. |
| `get_remote(lang, url, &RemoteOptions)` | `resources.GetRemote`; `Ok(None)` for a 404. `RemoteOptions::from_map(Option<&Map>)`; `RemoteError`; `cache_key`, `go_keys`. |

## Pipes (T42)

`StoreConfig::transforms` is the build's `TransformEnv` (`TransformEnv::from_config`: project and
publish directories, the build environment, `[minify]`, the `js_build` builder made on first
use). Options are
typed and decoded from the template's map with `from_json` (keys case-insensitive, as Go's).

| `Transform` | Options | Implementation | Result |
|---|---|---|---|
| `Fingerprint(HashAlgo)` | | sha/md5 digest | `.<hex>` before the extension, `Data.Integrity` |
| `Minify` | | `ssg-minify` by media type (HTML, CSS, JS, JSON, SVG, XML; others are an error, as in Go) | `.min` before the extension |
| `ToCss(ToCssOptions)` | `targetPath`, `outputStyle`, `includePaths`, `vars` (`SassVar`), `precision`, `enableSourceMap` | grass; imports through the assets view (entry at its asset path in a virtual root), then `includePaths` relative to the project; `@import "build:vars"`; URLs with an explicit `.scss`/`.sass` extension, which grass looks up only next to the importer, resolved as dart-sass does (the load paths too, import-only files first) by rewriting them to the found file's absolute path (`pipes::sass_imports`) | `targetPath` or `.css`, `text/css` |
| `JsBuild(JsBuildSpec)` | `ssg_jsbuild::JsBuildOptions` | `ssg_jsbuild::JsBuilder` (rolldown in process), assets resolved through a shared assets view (`SharedAssets`) | `targetPath` or `.js`, `text/javascript`; external/linked map as `<target>.map` |

- **Laziness.** `transform` registers the result at once with its final target, link and
  media type and `Body::Pending`; the work runs on `realize`, `content` or publishing. A
  `fingerprint` of a pending resource is pending too (provisional record: the source's link,
  `PublishPolicy::Never`) — so `to_css | minify | fingerprint | post_process` (a site's head)
  runs in E5. Failures are not memoized.
- **For the template layer (T35).** A view of a pending result can be built without computing
  it: its links, name and media type are final (`.Content` computes). Only a pending
  `fingerprint` has provisional links and no integrity: either realize it at the call (errors
  surface there, but a chain into `post_process` then runs before E5), or give those fields
  post-process placeholders (`post_process(id)`), which keeps Go's laziness at the cost of
  holding the output until E5. The `fingerprint` filter realizes it at the call unless
  `waits_for_e5(id)`: the chain processes an image (images are processed in E6); only those
  keep the placeholders, so a fingerprinted CSS or JS file does not hold every page.
- **Errors** are `ResourceError::Pipe { resource, transform, source: PipeError }`; Sass errors
  carry the real file (or `build:vars`), line and column.

## Identity

- Assets memoize on the cleaned path (and the language on multihost sites), bundles on their
  target, transforms on `(source, transform)`, metadata views on `(source, name, title,
  params)`, processed images on `(source, operation)`, remote resources on the typed request.
- Named targets (`from_string`, `from_template_output`, `concat`, `copy`) memoize on the target
  path. A second claim with the same input returns the first resource. With other input: in
  the same language it is `ResourceError::TargetConflict { target, first, second }` naming
  both call sites; in another language the earlier language's resource is returned (language
  sub-waves render in order). On multihost sites every language has its own target, so nothing
  is shared. QR codes are named targets whose name hashes every input, so equal calls share one
  resource.
- `gohash`: the Go implementation's `hashing.HashString` (its fork of hashstructure, v0.5.0,
  with xxHash64) for the names that must equal Go's: getresource cache entries and QR codes.

## Publishing

`publish` writes what was not written yet, one writer per target, in target order. A transform
that keeps its source's path (`js_build` of a `.js` asset without `targetPath`, `to_css` onto a
`.css` path) has its source's URL, so a token names both: the result is written, not the
source (Go publishes what the template linked; a template that links the source and only reads
the result's `.content` gets the result here). Otherwise the lowest id wins. Candidates are
`Eager` resources, resources marked with `mark_published`, and resources named by a URL token
(never `Never`). Tokens are canonicalised before lookup: HTML character references
(`&amp;`, `&#39;`, `&#x27;`) and JSON escapes (`\/`, `&`) decoded, query and fragment
dropped, percent-escapes decoded (either case), scheme dropped and host lower-cased for
absolute and protocol-relative URLs. Each resource is indexed under its relative permalink and
its permalink in the same form. Processed images go to `ImageQueue::process` (rayon) in one
batch.

## GetRemote and the getresource cache

- The request is typed (`RemoteOptions`: method, headers, body, `key`, `responseHeaders`);
  option maps that differ only in key order or key case are the same request.
- Cache entries (`[caches.getresource]`, `maxAge`) are raw HTTP responses, named by
  `cache_key` (xxh3-128 of the `key` option, else of the request).
- **Importer**: on a miss, the entry the Go build would have written for the call (named by
  `go_keys`, Go's hashstructure/xxHash64 of `[url, options]` or of `key`) is looked up in
  the cache directory itself and in `RemoteConfig::import_dirs`, and copied under this crate's
  name. A cache directory filled by the Go build (via `sites.py cache`, which sets the Go
  program's cache-directory environment variable) is thereby replayed offline.
- Only then, if `RemoteConfig::network`, the URL is fetched with `ureq` (no retries); the
  response is cached unless it is a redirect or `maxAge` is 0.
- Security: `security.http.urls` and `.methods` are checked; a `Content-Type` matching
  `.mediaTypes` (or any, for HEAD) is trusted, otherwise the media type is sniffed from the
  content and narrowed by the extensions of the content type and the file name
  (`Content-Disposition` `filename`/`filename*` first).
- The resource is named `/<file stem>_<Go user key><suffix>` like Go's, so its URLs are the
  Go build's.

## Accepted deviations

Listed with their reasons in `expected_diffs.toml` (the tests read it):

- **Sass (grass, dart-sass semantics) instead of LibSass**: plain CSS `@import` hoisted to the
  top; numbers with up to 10 decimals (`precision` accepted, no effect); CSS escapes in strings
  written as characters; `calc()` of constants simplified; `nested` and `compact` written
  expanded; no source maps (`enableSourceMap` accepted, no effect); a boolean in `vars` is
  written unquoted (Go fails on it); `transpiler = "dartsass"` also compiles with grass.
- **Minified bytes** are ssg-minify's (lightningcss, oxc, minify-html), not tdewolff's, so a
  fingerprint after `minify` names other bytes.
- **`to_css` is always available** (built in); Go's "feature not available" cases do not apply.
- PostCSS, Babel and Tailwind CSS (`css.PostCSS`, `js.Babel`, `css.TailwindCSS`) are not
  pipes: this port runs no external programs (`minify` adds vendor prefixes, `js_build` lowers
  for the browser targets, `purge_css` purges per page).
- Front matter params are not written into a shared base resource (Go's aliasing makes a
  translation see the other language's params).
- `resources.Copy` of a different source to an already copied target in the same language is
  a `TargetConflict`; Go returns the first copy because its cache key ignores the source.
- Remote: equivalent option maps are one request (`echo-post-upper`); a HEAD response's
  parameterised `Content-Type` resolves to the configured type and suffix (`head`); a trusted
  vendor type drops its parameters (`vnd-accepted`).
- Go's resource `Key` is not modelled; image sizes come from the image queue.
- `:counter` in metadata names and titles is always `1`, as in Go, which applies metadata to
  one resource at a time.
- Error texts are this crate's own.

## Tests (`tests/it`)

| Test | Oracle | Cases |
|---|---|---|
| `collection` | `tests/it/data/collection.json` (Get/GetMatch/Match/ByType of the old `nh-resource` oracle, 5 sets) | 445 |
| `resources::{synth,docs}` | `oracle/resources/resources` (assets, bundles, metadata, publishing) | 43 + 89 records, 34 + 89 published files |
| `fingerprint` | `oracle/resources/transform` (fingerprint chains, SRI, copy) | 144 equal to Go, 28 sha1 errors, 52 copy conflicts |
| `remote` | `oracle/resource-transformers/getremote` (key vectors, 48 calls from the Go build's cache, 51 YouTube entries) | 48 + 48 + 51 |
| `identity`, `publish` | — | named targets, concat, metadata, image seam, multihost, token forms |
| `identity::qr_codes`, `gohash` unit test | Go's `TestQR` (8 option maps: file names; bytes via `ssg-images`) | names, bytes, sizes, identity, publishing, multihost |
| `pipes::tocss` | `oracle/resource-transformers/tocss` (LibSass on t16site) + the reconstruction's SCSS | 31: 16 equal (normalised), 15 accepted; reconstruction compiled, slash division checked; explicit-extension `@import`/`@use` found in `includePaths` (relative imports of the found file, import-only files, the indented syntax) |
| `pipes::jsbuild` | `oracle/resource-transformers/jsbuild` (t16site, docs) | 62 + 6: media types, data, links (fingerprints normalised), `Data.Integrity`'s algorithm, errors at Go's positions, published files; what the scripts do is compared in `ssg-jsbuild` |
| `pipes::minify` | `oracle/resources/transform` minify chains | 112 chains: 48 Go's bytes, 16 fixed points, 48 errors for types without minifier; 56 PostCSS or Babel chains not applicable |
| `pipes::postprocess` | `oracle/resources/transform` post-processed resources | 65 fields equal to Go's |
| `pipes::template` | — | `execute_as_template` with Tera |
