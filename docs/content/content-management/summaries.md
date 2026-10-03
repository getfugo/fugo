---
title: Summaries
description: The short version of a page for lists and feeds — automatic, marked with a divider, or written in front matter.
weight: 80
---

`page.summary` is a page's summary, and `page.truncated` tells whether the content goes on
beyond it. There are three ways to get one, in this order of precedence:

Front matter
: `summary: A short description.` — Markdown, rendered.

The divider
: <code>&lt;!-<wbr>-more-<wbr>-&gt;</code> in the content: everything before it is the summary.

Automatic
: Whole paragraphs, until `summaryLength` words (70 by default) are reached.

```html
{% for p in page.pages %}
  <article>
    <h2><a href="{{ p.rel_permalink }}">{{ p.title }}</a></h2>
    {{ p.summary }}
    {% if p.truncated %}<a href="{{ p.rel_permalink }}">Read more</a>{% endif %}
  </article>
{% endfor %}
```

In a shortcode or render hook, read another page's summary with
`page_summary(page=p)`.
