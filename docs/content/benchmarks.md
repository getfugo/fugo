---
title: Benchmarks
description: How fast fugo builds sites and runs its functions, commit by commit, next to the Go implementation it replaced.
aliases: [/about/benchmarks/]
---

Every change to fugo's main branch is measured on a GitHub Actions Linux runner, with the binary
that CI builds for `x86_64-unknown-linux-gnu`, and so is the Go implementation that fugo replaced
(fugo was written in Go until version 0.148; its last commit is built from the repository's
history). Both run in the same job on the same machine, so their results compare directly. Hover
over a point (or focus it) for the commit and the value. Runners are shared machines, so a single
point can be off by a few percent: look at the trend. Lower is better everywhere.

## Site builds

Each site is built once to fill the caches, then five times; a chart shows the median of the five.
**Build time** is the time a build takes, start to exit; **peak memory** is the maximum resident
set size of the process.

- **docs site**: this documentation site, built from `docs/` (Sass, a bundled script, image
  processing, about 330 pages). It has no Go-template version, so its charts show fugo alone.
- **1,000 and 10,000 generated pages**: generated sites of Markdown pages with headings, a table
  of contents, a list, a table, a highlighted code block, tags and categories, listed on
  paginated section, taxonomy and term pages, with RSS and a sitemap. The Go implementation
  builds the same content with the same layouts written as Go templates (the dashed blue line);
  the pages the two write are the same, but for escaping details.

{{< benchmarks suite="fugo" >}}

## Micro-benchmarks

The Go implementation's benchmarks of its internal functions: the time one call takes, recorded
from 2020 to 2025 (the solid blue line). Where fugo has a function that does the same job, it
runs the same benchmark, with the same input, on every change: the orange line, after the marker.
The Go benchmark runs again in the same job (the dashed blue line), so the two compare on the same
machine. Benchmarks with a fugo version come first; the others are the Go history alone. You can
run fugo's on your machine with `cargo run --release -p ssg-microbench -- <results.json>` in a
clone of the repository.

{{< benchmarks suite="Benchmark" with="micro" >}}
