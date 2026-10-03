---
title: Quick start
description: Create a small site with a home page and a blog post, preview it with the development server, and build it for publishing.
weight: 10
---

This guide takes about ten minutes. You need the `fugo` binary on your `PATH`; see
[Installation](/installation/) if you do not have it yet.

```sh
fugo version
```

## Create the project

A fugo project is a directory with a configuration file, content and templates:

```sh
mkdir -p my-site/content/posts my-site/layouts
cd my-site
```

Create the configuration file `config.toml`:

{{< code-toggle file=config >}}
baseURL = "https://example.org/"
title = "My site"
languageCode = "en-us"
{{< /code-toggle >}}

## Add content

Content is Markdown with front matter. The home page's content is `content/_index.md`:

```md {title="content/_index.md"}
---
title: Home
---
Welcome to my site.
```

A blog post is a page of the `posts` section, `content/posts/hello.md`:

```md {title="content/posts/hello.md"}
---
title: Hello, fugo
date: 2026-10-02
---
This is my **first** post.
```

## Add templates

fugo does not ship a theme: a site brings its layouts, or a [theme](/configuration/themes/)
that has them. Templates are [Tera](/templates/introduction/) files, named after what they render.
Start with a base template that every page extends, `layouts/baseof.html`:

```html {title="layouts/baseof.html"}
<!doctype html>
<html lang="{{ site.language_code or "en" }}">
  <head>
    <meta charset="utf-8">
    <title>{% block title %}{{ page.title }} | {{ site.title }}{% endblock title %}</title>
  </head>
  <body>
    <header><a href="{{ site.home.rel_permalink }}">{{ site.title }}</a></header>
    <main>{% block main %}{% endblock main %}</main>
  </body>
</html>
```

The home page lists the posts, `layouts/home.html`:

```html {title="layouts/home.html"}
{% extends "baseof.html" %}
{% block title %}{{ site.title }}{% endblock title %}
{% block main %}
  {{ page.content }}
  <h2>Posts</h2>
  <ul>
    {% for p in site.regular_pages %}
      <li>
        <a href="{{ p.rel_permalink }}">{{ p.title }}</a>
        <time>{{ p.date | date(format="%B %-d, %Y") }}</time>
      </li>
    {% endfor %}
  </ul>
{% endblock main %}
```

A post uses `layouts/single.html`:

```html {title="layouts/single.html"}
{% extends "baseof.html" %}
{% block main %}
  <article>
    <h1>{{ page.title }}</h1>
    {{ page.content }}
  </article>
{% endblock main %}
```

And a section such as `/posts/` uses `layouts/list.html`:

```html {title="layouts/list.html"}
{% extends "baseof.html" %}
{% block main %}
  <h1>{{ page.title }}</h1>
  <ul>
    {% for p in page.pages %}
      <li><a href="{{ p.rel_permalink }}">{{ p.title }}</a></li>
    {% endfor %}
  </ul>
{% endblock main %}
```

Check the templates before you build. `fugo templates check` reports syntax errors, unknown
functions and missing templates with their positions:

```sh
fugo templates check
```

## Preview with the server

```sh
fugo server
```

Open <http://localhost:1313/>. The server builds the site into memory, watches the project, and
reloads the browser when you save a file. Edit `content/posts/hello.md` and watch the page
change. Press <kbd>Ctrl</kbd>+<kbd>C</kbd> to stop it.

Drafts and pages dated in the future are left out unless you ask for them:
`fugo server -D -F`.

## Build for publishing

```sh
fugo
```

`fugo` without a command builds the site into `public/`, the directory you upload to your host.
Add `--minify` to minify HTML, CSS and JavaScript. See [Host and deploy](/host-and-deploy/) for
publishing to GitHub Pages and other hosts.

## Next steps

- Learn the [directory structure](/getting-started/directory-structure/) of a project.
- Organize content with [sections and page bundles](/content-management/organization/).
- Learn [Tera templates](/templates/introduction/) and the [template lookup order](/templates/lookup-order/).
- Look up any function in the [reference](/reference/functions/).
