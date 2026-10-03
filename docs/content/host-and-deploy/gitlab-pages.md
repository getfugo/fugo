---
title: GitLab Pages
description: Build and publish a fugo site on GitLab Pages with GitLab CI.
weight: 20
---

```yaml {title=".gitlab-ci.yml"}
pages:
  image: debian:stable-slim
  variables:
    V: "1.0.0"
  before_script:
    - apt-get update && apt-get install -y --no-install-recommends ca-certificates curl
    - curl -sL "https://github.com/getfugo/fugo/releases/download/v$V/fugo_${V}_linux-amd64.tar.gz" | tar -xz fugo
  script:
    - ./fugo build --minify --base-url "$CI_PAGES_URL/"
  artifacts:
    paths:
      - public
  cache:
    paths:
      - resources/_gen
  rules:
    - if: $CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH
```

GitLab publishes the `public` artifact of the `pages` job.
