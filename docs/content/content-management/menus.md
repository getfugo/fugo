---
title: Menus
description: Define menus in the configuration or in front matter, nest entries, and render them with the current page highlighted.
weight: 70
---

A menu is a named list of entries: `main`, `footer`, `docs`. Entries point to pages or URLs and
may have children.

## In the configuration

{{< code-toggle file=config >}}
[menus]
  [[menus.main]]
    name = "Docs"
    pageRef = "/docs/"
    weight = 10
  [[menus.main]]
    name = "Install"
    pageRef = "/docs/install/"
    parent = "Docs"
    weight = 10
  [[menus.main]]
    name = "GitHub"
    url = "https://github.com/getfugo/fugo"
    weight = 30
{{< /code-toggle >}}

`name`
: The text of the entry.

`pageRef`
: A page's path; the entry links to it and knows it.

`url`
: A URL, when the entry is not a page.

`weight`
: The order: lower first.

`parent`
: The `identifier` (or name) of the parent entry.

`identifier`
: A unique key, needed when two entries share a name.

`pre`, `post`
: HTML before and after the name, e.g. an icon.

`title`
: The `title` attribute of the link.

`params`
: Parameters of the entry.

## In front matter

{{< code-toggle file=content/about fm=true >}}
title = "About"
[menus.main]
  weight = 20
{{< /code-toggle >}}

`menus: main` adds the page with its link title; a map sets `name`, `weight`, `parent`,
`identifier`; a list of names adds it to several menus.

## Render a menu

```html
<nav>
  <ul>
    {% for e in site.menus.main or [] %}
      {% set current = is_menu_current(menu="main", entry=e) or has_menu_current(menu="main", entry=e) %}
      <li>
        <a href="{{ e.url }}"{% if current %} aria-current="page"{% endif %}>{{ e.pre }}{{ e.name }}{{ e.post }}</a>
        {% if e.has_children %}
          <ul>
            {% for c in e.children %}
              <li><a href="{{ c.url }}">{{ c.name }}</a></li>
            {% endfor %}
          </ul>
        {% endif %}
      </li>
    {% endfor %}
  </ul>
</nav>
```

`is_menu_current` is true when the entry points to the page being rendered; `has_menu_current`
when one of its children does. See the [menu entry](/reference/objects/menu-entry/) fields.
