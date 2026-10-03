---
title: Site settings
description: The root settings of a fugo site — base URL, title, parameters, directories, URLs, page kinds, titles, summaries and the table of contents.
weight: 20
---

## Identity

`baseURL`
: The absolute URL of the published site, with a trailing slash: `https://example.org/` or
  `https://example.org/blog/`. `fugo server` replaces it with the server's address.

`title`, `copyright`
: The site's title and copyright notice (`site.title`, `site.copyright`).

`languageCode`
: The site's language tag, e.g. `en-US` (`site.language_code`).

`defaultContentLanguage`
: The key of the default language (`en`); see [Multilingual sites](/content-management/multilingual/).

`timeZone`
: The zone of dates written without one: `Asia/Bangkok`. Default UTC.

`params`
: Your own settings, for templates (`site.params`). Keys are lower-cased.

## Directories

| Setting | Default |
|---|---|
| `contentDir` | `content` |
| `layoutDir` | `layouts` |
| `assetDir` | `assets` |
| `dataDir` | `data` |
| `i18nDir` | `i18n` |
| `archetypeDir` | `archetypes` |
| `staticDir` | `static` (also `staticDir0` … `staticDir10`) |
| `publishDir` | `public` |
| `resourceDir` | `resources` |
| `themesDir` | `themes` |
| `cacheDir` | the user cache directory (`~/.cache/fugo_cache`), see [Caching](/configuration/caching/) |

`ignoreFiles` lists regular expressions of content files to skip.

## Pages

`disableKinds`
: Kinds not to build: `home`, `page`, `section`, `taxonomy`, `term`, `rss`, `sitemap`,
  `sitemapindex`, `robotstxt`, `404`.

`mainSections`
: The sections of the main content (`site.main_sections`).

`summaryLength`
: Words in automatic [summaries](/content-management/summaries/) (70).

`hasCJKLanguage`
: Count words and summaries for Chinese, Japanese and Korean text.

`titleCaseStyle`
: How generated titles are capitalised: `ap` (default), `chicago`, `go`, `firstupper`, `none`.

`pluralizeListTitles`, `capitalizeListTitles`
: Taxonomy titles (`Tags` from `tag`).

`enableEmoji`
: Replace `:emoji:` codes in content.

`enableRobotsTXT`
: Write [robots.txt](/templates/sitemap/#robotstxt).

`buildDrafts`, `buildFuture`, `buildExpired`
: Build drafts, future and expired pages (also `-D`, `-F`, `-E`).

`timeout`
: How long a page may take to render before the build fails (`60s`).

## URLs

`uglyURLs`, `relativeURLs`, `canonifyURLs`, `disablePathToLower`, `removePathAccents`,
`disableAliases`: see [URLs](/content-management/urls/#site-wide-settings).

`refLinksErrorLevel`, `refLinksNotFoundURL`
: What a [`ref`](/content-management/links/#ref-and-relref) to a missing page does: `error`
  (default) or `warning`, and the URL it writes.

## Table of contents

{{< code-toggle file=config >}}
[markup.tableOfContents]
  startLevel = 2
  endLevel = 3
  ordered = false
{{< /code-toggle >}}

`page.table_of_contents` lists the headings from `startLevel` to `endLevel`, as a nested `<ul>`
(or `<ol>` with `ordered`).

## Logs

`ignoreLogs` lists warning ids not to print: `ignoreLogs = ["shortcode-x-getremote"]`. A
warning's id is printed in brackets after `WARN`; warnings with the same id are printed once.
