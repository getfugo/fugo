---
title: Security
description: Control which environment variables templates may read and which URLs get_remote may fetch.
weight: 90
---

Templates and themes are code. `[security]` limits what they can reach.

{{< code-toggle file=config >}}
[security]
  enableInlineShortcodes = false
  [security.funcs]
    getenv = ["^FUGO_", "^CI$"]
  [security.http]
    methods = ["(?i)GET|POST"]
    urls = [".*"]
{{< /code-toggle >}}

`funcs.getenv`
: Environment variables the `get_env` function may read. The names a project's
  [`.env` file](/configuration/introduction/#the-env-file) defines are always readable.

`http.urls`, `http.methods`, `http.mediaTypes`
: What `get_remote` may fetch.

`enableInlineShortcodes`
: Allow shortcodes defined inside content files.

A blocked action is an error that names the setting to change.

`[security.exec]` (`allow`, `osEnv`) is accepted for compatibility and has no effect: fugo runs
no programs.
