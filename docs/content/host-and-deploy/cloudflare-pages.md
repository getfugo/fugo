---
title: Cloudflare Pages
description: Build and publish a fugo site on Cloudflare Pages.
weight: 40
---

Create a Pages project connected to the repository, with these build settings:

Framework preset
: None

Build command
: `V=1.0.0 && curl -sL https://github.com/getfugo/fugo/releases/download/v$V/fugo_${V}_linux-amd64.tar.gz | tar -xz fugo && ./fugo build --minify --base-url "$CF_PAGES_URL/"`

Build output directory
: `public`

`$CF_PAGES_URL` is the deployment's URL; for the production site with a custom domain, set
`baseURL` in `config.toml` and drop `--base-url`.
