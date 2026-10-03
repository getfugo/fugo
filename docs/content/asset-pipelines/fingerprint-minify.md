---
title: Fingerprinting and minification
description: Minify HTML, CSS, JavaScript, JSON, SVG and XML, and give files content-hashed names with Subresource Integrity.
weight: 30
---

## Minify

`minify` shrinks a resource by its media type — HTML, CSS, JavaScript, JSON, SVG and XML — with
fugo's built-in minifiers ([lightningcss](https://lightningcss.dev/), [oxc](https://oxc.rs/),
minify-html):

```html
{% set css = get_asset(path="css/site.css") | minify %}
```

The result is published as `site.min.css`. Other media types are an error.

Minified CSS is prepared for the browsers of the project's
[browserslist](https://github.com/browserslist/browserslist#queries) configuration (a
`.browserslistrc` or `browserslist` file, or the `browserslist` key of `package.json`): vendor
prefixes those browsers need are added, newer syntax they lack is lowered, and prefixes none of
them needs are removed, as autoprefixer does. Without a
browserslist configuration, prefixes stay as written.

`fugo build --minify` minifies every rendered page as well, per the `[minify]` configuration;
`[minify] minifyOutput = true` does the same without the flag.

## Fingerprint

`fingerprint` renames a resource with a hash of its content, so it can be cached forever, and
records the [Subresource Integrity](https://developer.mozilla.org/docs/Web/Security/Subresource_Integrity)
hash:

```html
{% set js = get_asset(path="js/app.js") | js_build | minify | fingerprint %}
<script src="{{ js.rel_permalink }}" integrity="{{ js.data.integrity }}" crossorigin="anonymous"></script>
```

`app.js` becomes `app.min.<sha256>.js`. `algo` picks the hash: `sha256` (default), `sha384`,
`sha512` or `md5`.

Fingerprint in production only, to keep stable names while you work:

```html
{% set css = get_asset(path="scss/main.scss") | to_css %}
{% if build.is_production %}{% set css = css | fingerprint %}{% endif %}
```

Minified bytes differ from the Go implementation's (other minifiers), so fingerprints of
minified files differ too.
