---
title: Benchmarks
description: How long fugo takes to build sites, and how much memory it uses, commit by commit, next to the Go implementation it replaced.
weight: 20
---

Every change to fugo's main branch is measured on a GitHub Actions Linux runner, with the binary
that CI builds for `x86_64-unknown-linux-gnu`. Each site is built once to fill the caches, then
five times. A chart shows the median of the five, per commit; hover over a point (or focus it) for
the commit and the value. Runners are shared machines, so a single point can be off by a few
percent: look at the trend.

- **docs site**: this documentation site, built from `docs/` (Sass, a bundled script, image
  processing, about 330 pages).
- **1,000 and 10,000 generated pages**: generated sites of Markdown pages with headings, a table
  of contents, a list, a table, a highlighted code block, tags and categories, listed on
  paginated section, taxonomy and term pages, with RSS and a sitemap.

**Build time** is the time a build takes, start to exit. **Peak memory** is the maximum resident
set size of the process. Lower is better for both. You can run the same measurements on your
machine with `cargo dev bench <binary> <results.json>` in a clone of the repository.

## fugo and the Go implementation

Until version 0.148, fugo was written in Go. The generated sites are also built, in the same run
on the same machine, by that Go version (its last commit, built from the repository's history),
from the same content with the same layouts written as Go templates: the dashed blue line. The
pages the two write are the same, but for escaping details. The docs site has no Go-template
version, so its charts show fugo alone.

{{< benchmarks suite="fugo" >}}

## The Go implementation's micro-benchmarks

These are the micro-benchmarks of the Go code, from 2020 to 2025: the time one call of an internal
function took, in nanoseconds. They do not measure today's fugo, and no new results are added;
they are kept as the project's history.

{{< benchmarks suite="Benchmark" search=true >}}
