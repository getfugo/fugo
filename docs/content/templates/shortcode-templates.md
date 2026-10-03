---
title: Shortcode templates
description: Write your own shortcodes — read positional and named arguments, render the inner content, nest shortcodes and report errors.
weight: 50
---

A file in `layouts/_shortcodes/` is a shortcode of the same name: `_shortcodes/note.html` is
`{{</* note */>}}`. Shortcodes in `layouts/<section>/_shortcodes/` are available only to the
pages of that section.

## Arguments

```html {title="layouts/_shortcodes/button.html"}
{%- set href = shortcode | arg(name="href") -%}
{%- set label = shortcode | arg(name="label", default="Read more") -%}
{%- if not href -%}
  {{- log_error(message="button: missing href at " ~ shortcode.position) -}}
{%- endif -%}
<a class="button" href="{{ href | rel_url }}">{{ label }}</a>
```

```md
{{</* button href="/docs/" label="Get started" */>}}
```

`shortcode | arg(name=…)` reads a named argument, `arg(index=0)` a positional one; `default` is
the value when it is missing. `shortcode.params` holds the named arguments,
`shortcode.args` the positional ones, `shortcode.is_named_params` which kind the call used.

## Inner content

A shortcode used with a closing tag gets its content as `inner`:

```html {title="layouts/_shortcodes/note.html"}
<aside class="note">
  {{ inner | render_string(display="block") }}
</aside>
```

`render_string` renders the inner Markdown with the page's settings and hooks. Called as
`{{%/* note */%}}`, the shortcode's whole output is rendered as Markdown with the page instead;
`inner_deindent` is the inner content without its common indentation (for code).

## Other names

`page`
: The page the shortcode is used on.

`shortcode.name`, `shortcode.ordinal`
: The shortcode's name and its position among the page's shortcode calls (from 0), for unique
  ids.

`shortcode.parent`
: The enclosing shortcode, when one is nested in another: a `tab` can read its `tabs`.

`shortcode.position`
: `file:line:column` of the call, for error messages.

## Output formats

`_shortcodes/note.html` serves HTML pages; add `note.rss.xml` or `note.json.json` for other
output formats.
