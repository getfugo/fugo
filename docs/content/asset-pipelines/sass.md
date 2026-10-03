---
title: Sass
description: Compile Sass and SCSS to CSS in process with to_css — imports, include paths, variables from templates and output styles.
weight: 10
---

`to_css` compiles Sass and SCSS with [grass](https://github.com/connorskees/grass), a Rust
implementation of Dart Sass. Nothing to install.

```html
{% set opts = {"targetPath": "css/style.css", "outputStyle": "compressed"} %}
{% set css = get_asset(path="sass/main.scss") | to_css(options=opts) %}
<link rel="stylesheet" href="{{ css.rel_permalink }}">
```

## Options

`targetPath`
: The published path. Default: the asset's path with `.css`.

`outputStyle`
: `expanded` (default) or `compressed`.

`includePaths`
: Directories, relative to the project, searched for imports after `assets/`.

`vars`
: Variables for the `build:vars` module: `{"primary": "#d4337c"}`.

Imports resolve in `assets/` first (themes' and modules' included), then `includePaths`:
`@use "components/button"` finds `assets/sass/components/_button.scss` next to the importer.

## Variables from templates

```html
{% set css = get_asset(path="sass/main.scss")
  | to_css(options={"vars": {"brand": site.params.brand_color}}) %}
```

```scss
@use "build:vars" as v;
a { color: v.$brand; }
```

## Compatibility

Go-template generators compile with LibSass or Dart Sass; fugo's grass follows Dart Sass. Source maps are not
written (`enableSourceMap` is accepted), and `precision` has no effect. `transpiler` is
accepted and ignored.
