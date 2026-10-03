---
title: About fugo
description: What fugo is, where it comes from, and the material it derives from.
weight: 130
---

fugo is a static site generator written in Rust. It builds sites organized the way Go-template
static site generators organize them — content, front matter, sections, taxonomies, themes —
with [Tera](https://keats.github.io/tera/) templates, in a single binary.

## History

fugo began as a fork of another static site generator, written in Go. Versions up to 0.148 were
that Go code with the fork's changes, released under the project's former name. From version
1.0.0, fugo is a rewrite in Rust: the site model, Markdown rendering, Chroma's highlighter,
image processing and the development server were rewritten and tested against the Go
implementation's output, page by page, until its documentation site and other sites built the
same.

## Attribution

fugo is developed independently: it is not affiliated with, sponsored or endorsed by the
project it was forked from. It uses material from that project under its Apache 2.0 licence:
embedded templates rewritten in Tera, behaviour transcribed from its Go source, and its test
data and documentation as test fixtures. The repository's `NOTICE` carries the attribution of
the derived material, and `PROVENANCE.md` lists each such file.

If you need Go templates, module downloads or other features fugo does not have, keep your
Go-template generator. If you want that content model with Tera templates in a Rust binary,
fugo is for you; see [Migrating from Go templates](/migrating/).
