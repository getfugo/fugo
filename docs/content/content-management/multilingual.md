---
title: Multilingual sites
description: Publish a site in several languages, link translations, translate strings with i18n files, and serve languages from one host or several.
weight: 150
---

## Languages

{{< code-toggle file=config >}}
defaultContentLanguage = "en"
[languages]
  [languages.en]
    languageName = "English"
    weight = 1
  [languages.th]
    languageName = "ไทย"
    languageCode = "th-TH"
    title = "ตัวอย่าง"
    weight = 2
    [languages.th.params]
      subtitle = "เว็บไซต์ตัวอย่าง"
{{< /code-toggle >}}

The default language is published at the root (`/about/`), the others under their key
(`/th/about/`). `defaultContentLanguageInSubdir = true` puts the default language under
`/en/` too. `disableLanguages = ["th"]` leaves a language out of the build.

A language's table may set the site's settings for that language: `title`, `params`, `menus`,
`baseURL`, `contentDir`, `languageDirection` (`rtl`) and others.

## Translate content

By file name
: `about.md` and `about.th.md` are the English and Thai versions of one page.

By directory
: Give each language its own `contentDir` (`content/en`, `content/th`); files with the same
  path are translations.

By key
: Pages with the same `translationKey` in front matter are translations, whatever their paths.

A page in only one language exists only in that language. Link the translations:

```html
{% if page.is_translated %}
  <ul class="languages">
    {% for t in page.translations %}
      <li><a href="{{ t.rel_permalink }}" hreflang="{{ t.lang }}">{{ t.language.name }}</a></li>
    {% endfor %}
  </ul>
{% endif %}
```

## Translate strings

Put the theme's strings in `i18n/<lang>.toml` (or YAML, JSON):

```toml {title="i18n/th.toml"}
[read_more]
other = "อ่านต่อ"

[minutes]
one = "{{ .Count }} นาที"
other = "{{ .Count }} นาที"
```

```html
{{ i18n(key="read_more") }}
{{ i18n(key="minutes", count=page.reading_time) }}
```

A missing translation falls back to the default language's text, else is empty;
`enableMissingTranslationPlaceholders = true` shows `[i18n] key` instead, so missing keys stand
out.

## URLs and dates

`rel_lang_url` and `abs_lang_url` add the language prefix to a path. Dates and numbers format
in the page's language: `page.date | date(style="long")` is "2 ตุลาคม 2026" on a Thai page,
and `1234.5 | format_number(precision=1)` writes the language's separators.

## Several hosts

Give each language its own `baseURL` to serve it from its own host (`example.org`,
`example.th`): fugo writes each language to its own directory under `public/`.
