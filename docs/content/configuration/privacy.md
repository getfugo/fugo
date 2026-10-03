---
title: Privacy and services
description: Configure the embedded shortcodes and partials that talk to Google Analytics, YouTube, Vimeo, Instagram and X, for privacy.
weight: 80
---

The embedded [shortcodes](/content-management/shortcodes/) and partials read `[services]` and
`[privacy]`.

## Services

{{< code-toggle file=config >}}
[services]
  [services.googleAnalytics]
    id = "G-XXXXXXXXXX"
  [services.rss]
    limit = 20
  [services.x]
    disableInlineCSS = false
  [services.instagram]
    disableInlineCSS = false
{{< /code-toggle >}}

`{% include "_partials/google_analytics.html" %}` adds Google Analytics 4 with the `id`.
`services.rss.limit` caps [RSS feeds](/templates/rss/).

## Privacy

{{< code-toggle file=config >}}
[privacy]
  [privacy.googleAnalytics]
    disable = false
    respectDoNotTrack = true
  [privacy.youtube]
    disable = false
    privacyEnhanced = true
  [privacy.vimeo]
    disable = false
    enableDNT = true
    simple = false
  [privacy.instagram]
    disable = false
    simple = true
  [privacy.x]
    disable = false
    enableDNT = true
    simple = true
{{< /code-toggle >}}

`disable`
: The shortcode or partial renders nothing.

`privacyEnhanced`
: YouTube videos from `youtube-nocookie.com`.

`enableDNT`
: Ask Vimeo and X not to track.

`simple`
: Render a static version without the service's script: a thumbnail linking to the Vimeo
  video, an X post or Instagram post without its script.

`respectDoNotTrack`
: Skip Google Analytics for visitors with Do Not Track on.

Templates read these as `site.config.privacy` and `site.config.services`, in snake case.
