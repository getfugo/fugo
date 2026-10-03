---
title: Modules and mounts
description: Mount directories into fugo's file system, share content, layouts and assets between projects, and import themes as modules.
weight: 60
---

fugo builds from a union of directories: the project's `content`, `layouts`, `assets`, `data`,
`i18n`, `archetypes` and `static`, then each theme's. **Mounts** decide which directory
provides which part.

## Mounts

{{< code-toggle file=config >}}
[[module.mounts]]
  source = "content"
  target = "content"
[[module.mounts]]
  source = "node_modules/bootstrap/scss"
  target = "assets/scss/bootstrap"
[[module.mounts]]
  source = "../brand"
  target = "assets/brand"
[[module.mounts]]
  source = "docs-th"
  target = "content"
  lang = "th"
{{< /code-toggle >}}

`source`
: A directory or file, relative to the project (or absolute).

`target`
: Where it appears: `content`, `layouts`, `assets`, `data`, `i18n`, `archetypes` or `static`,
  and a path below it.

`lang`
: The language of mounted content.

`includeFiles`, `excludeFiles`
: Globs of the files to take or leave.

`disableWatch`
: Do not watch the source in `fugo server`.

Once you configure one mount for a component, the default for that component is gone: mount
`content` and `static` again if you still need them (as above).

This documentation mounts the repository's `brand/` directory as `assets/brand` for its logo
and icons.

## Imports

A project can import other projects as modules, with their own mounts:

{{< code-toggle file=config >}}
[[module.imports]]
  path = "my-shortcodes"
[[module.imports]]
  path = "shared-content"
  [[module.imports.mounts]]
    source = "content/legal"
    target = "content/legal"
{{< /code-toggle >}}

An import's `path` is a directory in `themesDir` (`themes/`), in `_vendor/` when a
`_vendor/modules.txt` lists it, or absolute. `disable` turns it off; `ignoreConfig` skips its
configuration; `ignoreImports` skips its imports; `noMounts` mounts nothing from it.
`replacements = "old/path -> ../local/path"` swaps import paths, for local development.

fugo does not download modules: there is no `go.mod` resolution. Put modules in `themes/`
or `_vendor/` (a vendor directory written by a Go-template generator works), or point to them
by path.
