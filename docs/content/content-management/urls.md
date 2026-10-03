---
title: URLs
description: How page URLs are formed, how to change them with slugs, URLs and permalink patterns, and how to redirect old URLs with aliases.
weight: 90
---

## Defaults

A page's URL is its path in `content/`, lower-cased, with spaces as `-`:
`content/Posts/My Post.md` is `/posts/my-post/`. A leaf bundle's directory is the page:
`content/posts/my-post/index.md` is also `/posts/my-post/`.

## Front matter

`slug`
: Replaces the last segment: `slug: hello` gives `/posts/hello/`.

`url`
: Replaces the whole path: `url: /about-us/`.

## Permalinks

Patterns per section and kind:

{{< code-toggle file=config >}}
[permalinks]
  [permalinks.page]
    posts = "/:year/:month/:slug/"
  [permalinks.section]
    posts = "/articles/"
{{< /code-toggle >}}

| Token | Value |
|---|---|
| `:year`, `:month`, `:monthname`, `:day`, `:weekday`, `:weekdayname`, `:yearday` | parts of the date |
| `:section`, `:sections` | the top-level section, every section |
| `:title`, `:slug` | the title, the slug (else the title) — made URL-safe |
| `:slugorfilename`, `:filename`, `:contentbasename` | the slug or file name, the file name, the page's name |
| `:slugorcontentbasename` | the slug, else the page's name |
| `:2006-01-02` and other Go date layouts | the date formatted with that layout |

## Aliases

Redirect old URLs to a page:

{{< code-toggle file=content/posts/hello fm=true >}}
title = "Hello"
aliases = ["/2019/01/hello/", "/old-blog/hello.html"]
{{< /code-toggle >}}

fugo writes a small HTML page at each alias that redirects to the page (`alias.html`, which a
site can override).

## Site-wide settings

`uglyURLs = true`
: `/posts/my-post.html` instead of `/posts/my-post/`.

`disablePathToLower = true`
: Keep the case of paths.

`removePathAccents = true`
: `/café/` becomes `/cafe/`.

`relativeURLs = true`
: Make links relative to the page.

`canonifyURLs = true`
: Make root-relative links absolute with `baseURL`.

In templates, build URLs with `rel_url` (`"/css/main.css" | rel_url`, honouring a base URL
with a path) and `abs_url`.
