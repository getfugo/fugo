---
title: Bundling and generated files
description: Concatenate assets, create resources from strings, and render assets as Tera templates.
weight: 50
---

## Concatenate

```html
{% set parts = [get_asset(path="js/a.js"), get_asset(path="js/b.js")] %}
{% set bundle = concat_assets(target="js/bundle.js", items=parts) | minify | fingerprint %}
```

Items must have the same media type. For JavaScript with imports, use
[`js_build`](/asset-pipelines/js-build/) instead.

## From a string

```html
{% set json = {"name": site.title, "start_url": "/"} | jsonify %}
{% set manifest = asset_from_string(target="manifest.webmanifest", content=json) %}
<link rel="manifest" href="{{ manifest.rel_permalink }}">
```

## Assets as templates

`execute_as_template` renders an asset as a Tera template, with `data`:

```js {title="assets/js/config.js"}
export const api = "{{ data.api }}";
export const version = "{{ data.version }}";
```

```html
{% set cfg = get_asset(path="js/config.js")
  | execute_as_template(target="js/config.js", data={"api": site.params.api, "version": build.version}) %}
```

The template sees `data`, `site` and `build`, not `page`.
