---
title: JavaScript bundling
description: Bundle JavaScript and TypeScript in process with js_build — imports from assets and node_modules, JSX, parameters from templates, source maps.
weight: 20
---

`js_build` bundles a script and everything it imports into one file, with
[rolldown](https://rolldown.rs/) running inside fugo. TypeScript and JSX work without
configuration.

```html
{% set js = get_asset(path="js/main.ts") | js_build(options={"minify": build.is_production}) %}
<script src="{{ js.rel_permalink }}" defer></script>
```

Imports resolve in `assets/` first (so `import "./menu"` and `import "js/menu"` find
`assets/js/menu.ts`), then in the project's `node_modules`. A `tsconfig.json` (or
`jsconfig.json`) in the project directory sets path aliases and JSX settings.

## Options

`targetPath`
: The published path. Default: the asset's path with `.js`.

`minify`
: Minify whitespace, names and syntax.

`format`
: `iife` (default), `esm` or `cjs`.

`target`
: The JavaScript version to write: `esnext` (default), `es2015` … `es2024`, `es5`.

`platform`
: `browser` (default), `node` or `neutral`.

`sourceMap`
: `inline`, `external` or `linked`; `sourcesContent: false` leaves the sources out.

`externals`
: Imports left as they are, for the runtime.

`defines`
: Replacements: `{"process.env.NODE_ENV": "\"production\""}`.

`shims`
: Imports replaced with assets: `{"react": "js/shims/react.js"}`.

`inject`
: Assets imported into every module.

`params`
: Data for `import * as params from "@params"`.

`jsx`, `jsxFactory`, `jsxFragment`, `jsxImportSource`
: JSX handling: `transform` (default), `preserve` or `automatic`.

`loaders`
: Loaders by extension: `{".svg": "text"}`.

`drop`
: Drop `console` or `debugger` statements.

## Parameters from templates

```html
{% set js = get_asset(path="js/search.js")
  | js_build(options={"params": {"index": "/search-index.json"}}) %}
```

```js
import * as params from "@params";
fetch(params.index);
```

## npm packages

List them in `package.json`; fugo installs them into `node_modules` when it builds (see
[npm packages](/asset-pipelines/npm-packages/)), and `js_build` finds them there.
