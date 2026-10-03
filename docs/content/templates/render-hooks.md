---
title: Render hooks
description: Override how Markdown links, images, headings, code blocks, blockquotes, tables and passthrough elements render, with templates in _markup.
weight: 60
---

A render hook is a template that renders one kind of Markdown element. Put it in
`layouts/_markup/`, or `layouts/<section>/_markup/` for the pages of one section.

| Hook | Renders | Fields |
|---|---|---|
| `render-link.html` | `[text](url "title")` | `destination`, `title`, `text`, `plain_text` |
| `render-image.html` | `![alt](src "title")` | `destination`, `title`, `text`, `plain_text`, `is_block` |
| `render-heading.html` | `## Heading` | `level`, `anchor`, `text`, `plain_text` |
| `render-codeblock.html` | fenced code blocks | `type` (the language), `inner`, `options` |
| `render-codeblock-<lang>.html` | code blocks of one language | as above |
| `render-blockquote.html` | blockquotes and alerts | `type` (`regular`, `alert`), `alert_type`, `alert_title`, `alert_sign`, `text` |
| `render-table.html` | tables | `thead`, `tbody` |
| `render-passthrough.html` | passthrough elements (math) | `type` (`inline`, `block`), `inner` |

Every hook also gets `page`, `page_inner` (the page whose Markdown holds the element; it differs
when a template renders another page's content) and `attributes` (Markdown attributes such as
`{.class #id}`). All but the heading hook get `ordinal`, the element's number on the page; links,
images, code blocks and passthrough elements get `position` for messages. See the
generated table in [Template contexts](/reference/contexts/).

## Examples

External links in a new tab:

```html {title="layouts/_markup/render-link.html"}
{%- set u = destination | parse_url -%}
<a href="{{ destination }}"{% if title %} title="{{ title }}"{% endif %}
  {%- if u.is_absolute %} rel="external noopener" target="_blank"{% endif %}>{{ text }}</a>
{#- -#}
```

Headings with a link to themselves:

```html {title="layouts/_markup/render-heading.html"}
<h{{ level }} id="{{ anchor }}">
  {{- text -}} <a class="anchor" href="#{{ anchor }}" aria-label="Link to this section">#</a>
</h{{ level }}>
{#- -#}
```

Images through image processing:

```html {title="layouts/_markup/render-image.html"}
{%- set img = page_inner.resources | get_resource(name=destination) -%}
{%- if img -%}
  {%- set w = img | resize(width=1200, format="webp") -%}
  <img src="{{ w.rel_permalink }}" width="{{ w.width }}" height="{{ w.height }}" alt="{{ plain_text }}" loading="lazy">
{%- else -%}
  <img src="{{ destination }}" alt="{{ plain_text }}">
{%- endif -%}
{#- -#}
```

End a hook with `{#- -#}` (or `-%}`) so it adds no newline: links and images are inline.

## Embedded hooks

fugo ships hooks that resolve links and images to pages and resources
(`useEmbedded`, see [Links](/content-management/links/)), and one for GoAT
[diagrams](/content-management/diagrams/). Without a hook, elements render as fugo's
[Markdown renderer](/content-management/markdown/) writes them, and code blocks are [highlighted](/content-management/syntax-highlighting/).
