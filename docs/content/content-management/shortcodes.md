---
title: Shortcodes
description: Call templates from content with shortcodes, use fugo's embedded shortcodes for figures, videos, posts and QR codes, and write your own.
weight: 110
---

A shortcode calls a template from content. fugo ships some; your own live in
`layouts/_shortcodes/`.

```md
{{</* youtube id="0RKpf3rK57I" */>}}

{{</* figure src="beach.jpg" alt="A beach" caption="Our first day" */>}}
```

## Calling shortcodes

`{{</* name */>}}`
: The shortcode's output is inserted as HTML.

`{{%/* name */%}}`
: The output is Markdown, rendered with the page.

Arguments are positional (`{{</* name "a" "b" */>}}`) or named (`{{</* name x="a" */>}}`),
not both. A shortcode can enclose content, which it receives as `inner`:

```md
{{</* details summary="Show the configuration" */>}}
Any **Markdown** here.
{{</* /details */>}}
```

To show a shortcode call as text instead of calling it, put `/*` after its opening delimiter
and `*/` before its closing one.

## Embedded shortcodes

### details

A collapsible `<details>` element. Named arguments: `summary` (Markdown; default "Details"),
`open`, `class`, `name`, `title`.

### figure

A `<figure>` with an image. Named arguments: `src` (a page resource, an asset or a URL),
`alt`, `caption` (Markdown), `title`, `attr` (Markdown, the attribution), `attrlink`, `link`,
`target`, `rel`, `class`, `width`, `height`, `loading`.

### highlight

Highlighted code: `{{</* highlight go "linenos=inline, hl_lines=2" */>}}…{{</* /highlight */>}}`.
See [Syntax highlighting](/content-management/syntax-highlighting/).

### instagram

An Instagram post: `{{</* instagram CxOWiQNP2MO */>}}`.

### param

A page or site parameter: `{{</* param "author" */>}}`, a dotted path for nested values.

### qr

A QR code image of `text`, else of the inner content:
`{{</* qr text="https://example.org" */>}}`. Named arguments: `text`, `level` (`low`,
`medium` (default), `quartile`, `high`), `scale` (default 4), `targetDir`, `alt`, `class`,
`id`, `title`, `loading`.

### ref, relref

A page's absolute or relative URL: `{{</* relref "/docs/install" */>}}`. See
[Links](/content-management/links/#ref-and-relref).

### vimeo

A Vimeo video: `{{</* vimeo id="146022717" */>}}`. Named arguments: `id`, `title`, `class`,
`loading`, `allowFullScreen`; or the id as the first positional argument.

### x

A post on X: `{{</* x user="SanDiegoZoo" id="1453110110599868418" */>}}`, fetched with X's
oEmbed API at build time.

### youtube

A YouTube video: `{{</* youtube id="0RKpf3rK57I" */>}}`. Named arguments: `id`, `title`,
`class`, `loading`, `start`, `end`, `autoplay` (forces `mute`), `controls`, `loop`, `mute`,
`allowFullScreen`; or the id as the first positional argument.

The `privacy` configuration (`[privacy.youtube] privacyEnhanced = true`, `[privacy.vimeo]
simple = true`, …) changes what the video and post shortcodes embed. See [Privacy](/configuration/privacy/).

## Your own

`layouts/_shortcodes/note.html` is the shortcode `note`:

```html {title="layouts/_shortcodes/note.html"}
{% set kind = shortcode | arg(name="kind", default="info") %}
<aside class="note note-{{ kind }}">
  {{ inner }}
</aside>
```

```md
{{</* note kind="warning" */>}}Back up your data first.{{</* /note */>}}
```

See [Shortcode templates](/templates/shortcode-templates/) for what a shortcode template can
read.
