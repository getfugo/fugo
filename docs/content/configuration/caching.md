---
title: Caching
description: The file caches fugo keeps between builds — processed images, assets and remote resources — and how to configure or clear them.
weight: 50
---

fugo keeps the results of slow work on disk so that the next build is fast:

| Cache | Holds | Default directory |
|---|---|---|
| `images` | processed images | `:resourceDir/_gen` |
| `assets` | transformed assets (Sass, bundles, …) | `:resourceDir/_gen` |
| `getresource` | files fetched with `get_remote` | `:cacheDir/:project` |
| `misc` | other results | `:cacheDir/:project` |

`:resourceDir` is the project's `resources/`; `:cacheDir` the cache directory (`cacheDir`, or
`--cache-dir`, else `$XDG_CACHE_HOME/fugo_cache` or `~/.cache/fugo_cache`); `:project` the
project directory's name.

{{< code-toggle file=config >}}
[caches]
  [caches.getresource]
    dir = ":cacheDir/:project"
    maxAge = "1h"
  [caches.images]
    dir = ":resourceDir/_gen"
    maxAge = -1
{{< /code-toggle >}}

`maxAge` is how long an entry stays valid: `-1` (the default) forever, `0` never, or a duration
(`"10m"`, `"24h"`) or a number of seconds. `fugo build --ignore-cache` ignores every cache for
one build.

Commit `resources/_gen` (or keep it in your CI cache) so that a fresh checkout does not process
every image again. Delete it to start over.

The [npm packages](/asset-pipelines/npm-packages/) that fugo downloads are kept in
`:cacheDir/packages`. Every project shares them, and they have no `maxAge`. Delete the
directory to download them again.
