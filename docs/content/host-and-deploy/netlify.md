---
title: Netlify
description: Build and publish a fugo site on Netlify.
weight: 30
---

Add `netlify.toml` to the project, then create a site from the repository in Netlify:

```toml {title="netlify.toml"}
[build]
  publish = "public"
  command = """
    V=1.0.0 && \
    curl -sL https://github.com/getfugo/fugo/releases/download/v$V/fugo_${V}_linux-amd64.tar.gz | tar -xz fugo && \
    ./fugo build --minify --base-url "$URL/"
  """

[context.deploy-preview]
  command = """
    V=1.0.0 && \
    curl -sL https://github.com/getfugo/fugo/releases/download/v$V/fugo_${V}_linux-amd64.tar.gz | tar -xz fugo && \
    ./fugo build --build-drafts --build-future --base-url "$DEPLOY_PRIME_URL/"
  """
```

`$URL` is the site's address and `$DEPLOY_PRIME_URL` the preview's. Netlify serves
`404.html` for missing pages.
