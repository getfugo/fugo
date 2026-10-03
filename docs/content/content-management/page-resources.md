---
title: Page resources
description: The images and files of a page bundle — list them, find them by name or pattern, give them titles and parameters, and process them.
weight: 30
---

The files of a [page bundle](/content-management/organization/#page-bundles) other than its
content are the page's resources, `page.resources`:

```text
content/posts/my-trip/
├── index.md
├── cover.jpg
├── gallery/
│   ├── 01.jpg
│   └── 02.jpg
└── route.gpx
```

## Find resources

```text
{# By name: the path in the bundle #}
{% set cover = page.resources | get_resource(name="cover.jpg") %}

{# By pattern: the first match, or every match #}
{% set first = page.resources | find_resource(pattern="gallery/*.jpg") %}
{% set photos = page.resources | find_resources(pattern="gallery/*") %}

{# By type: image, text, application, … #}
{% set images = page.resources | by_type(type="image") %}
```

Patterns are globs (`*`, `**`, `?`, `[abc]`, `{a,b}`), matched without regard to case.

## Use them

A resource has a `rel_permalink`, `permalink`, `name`, `title`, `media_type` and, for images,
`width` and `height`. Asking for its link publishes it:

```html
{% for img in page.resources | by_type(type="image") %}
  {% set thumb = img | fill(width=300, height=200) %}
  <a href="{{ img.rel_permalink }}">
    <img src="{{ thumb.rel_permalink }}" width="{{ thumb.width }}" height="{{ thumb.height }}" alt="{{ img.title }}">
  </a>
{% endfor %}
```

See [Image processing](/content-management/image-processing/) for what images can do, and
[`resource_content`](/reference/functions/resources/resource_content/) to read a text file.

## Metadata

The page's front matter can name resources, give them titles and parameters:

{{< code-toggle file=content/posts/my-trip/index fm=true >}}
title = "My trip"
[[resources]]
  src = "gallery/*.jpg"
  title = "Photo #:counter"
  [resources.params]
    credit = "Ada"
[[resources]]
  src = "cover.jpg"
  name = "header"
{{< /code-toggle >}}

`src` is a glob; the first entry that matches a resource sets each of its fields. `:counter`
counts the matches of the entry. Read them as `img.title` and `img.params.credit`; a renamed
resource is found by its new name: `get_resource(name="header")`.

## Publishing

A resource is published when a template asks for its link, and a processed image when its
result is used. Set `build.publishResources: false` to publish none, or render the page
without its files with the other [build options](/content-management/build-options/).
