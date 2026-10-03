[discussions]: https://github.com/getfugo/fugo/discussions
[issue tracker]: https://github.com/getfugo/fugo/issues
[releases]: https://github.com/getfugo/fugo/releases
[rust]: https://www.rust-lang.org/
[static site generator]: https://en.wikipedia.org/wiki/Static_site_generator

<p align="center">
  <img src="brand/logo.png" alt="fugo: an orange cat with goggles in a gear, above the word fugo" width="220">
</p>

<h3 align="center">The Rust-powered static site generator</h3>

<p align="center">
  <a href="https://getfugo.github.io">Website</a> ·
  <a href="https://github.com/getfugo/fugo/discussions">Discussions</a> ·
  <a href="https://github.com/getfugo/fugo/issues">Issues</a> ·
  <a href="#installation">Installation</a> ·
  <a href="CONTRIBUTING.md">Contributing</a> ·
  <a href="docs/content/migrating/">Migrating</a>
</p>

<p align="center">
  <a href="https://github.com/getfugo/fugo/actions/workflows/ci.yml"><img src="https://github.com/getfugo/fugo/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
</p>

## Overview

fugo is a [static site generator] written in [Rust]. It began as a fork of another static site generator, written in Go, and was rewritten in Rust ([Origin and attribution](#origin-and-attribution)): it keeps that generator's project layout, content model and configuration keys, and its tests compare its output with the Go implementation's, but its layouts are Tera templates. With its templating system and fast asset pipelines, fugo renders a complete site in seconds, often less.

With its multilingual support and taxonomy system, fugo suits:

- Corporate, government, nonprofit, education, news, event, and project sites
- Documentation sites
- Image portfolios
- Landing pages
- Business, professional, and personal blogs
- Resumes and CVs

Use fugo's built-in web server (`fugo server`) during development to instantly see changes to content, structure, behavior, and presentation. Then deploy the site to your host, or push changes to your Git provider for automated builds and deployment.

Fugo's fast asset pipelines include:

- Image processing &ndash; Convert, resize, crop, rotate, adjust colors, apply filters, overlay text and images, and extract EXIF data
- JavaScript bundling &ndash; Transpile TypeScript and JSX to JavaScript, bundle, tree shake, minify, create source maps, and perform SRI hashing.
- Sass processing &ndash; Transpile Sass to CSS, bundle, tree shake, minify (with vendor prefixes for your browserslist), purge unused rules per page, create source maps, and perform SRI hashing
- npm packages &ndash; Install the packages of `package.json` for bundling and Sass imports, without Node.js or npm

Fugo reads its configuration from `config.toml` (or `config.yaml`, `config.yml`, `config.json`), with the Go build's configuration keys and project layout, and uses its own names throughout: the `build` template object, `config.toml` and `.env` files. Its templates are Tera 2 with the Go build's layout names (those of v0.146 and later) instead of Go templates; [Upgrading from the Go build](#upgrading-from-the-go-build) says what else changed with v1.0.0. The known differences from the Go implementation are listed in §7 of [docs/rust-port/HANDOFF.md](docs/rust-port/HANDOFF.md).

fugo's documentation is the site in [`docs/`](docs/), built with fugo: getting started, content management, templates, asset pipelines, configuration, a generated reference of every template function, object and command, and a guide for [migrating from Go templates](docs/content/migrating/). See [Documentation](#documentation).

Minified CSS (`--minify`, `resources.Minify`) is prepared for the browsers of the project's [browserslist](https://github.com/browserslist/browserslist#queries) configuration (`.browserslistrc`, a `browserslist` file or the `browserslist` key of `package.json`; the section named like the environment applies, else the default queries): vendor prefixes those browsers need are added, newer syntax they lack is lowered and prefixes none of them needs are removed, as autoprefixer does, so PostCSS is not needed for that. A style rule declaring a property twice (a value and its fallback) is kept as written. Without a browserslist configuration, prefixes stay as written.

`purge_css` cuts a stylesheet down, for each page, to the rules that page uses (its elements' tags, classes and ids, the words of its scripts), with PurgeCSS's `safelist`, `greedy`, `blocklist`, `content` and `variables` options: `<style>{{ get_asset(path="main.scss") | to_css | minify | purge_css(content=[get_asset(path="js/main.js")]) }}</style>`. It needs no stats file and no PostCSS, and pages are written as they are rendered. See [template-api.md](docs/rust-port/template-api.md).

## Installation

Download the archive for your platform from the [releases] page. Releases are tagged `v<version>`; v1.0.0 and later are the Rust implementation named fugo, v0.148.2 and earlier the former Go implementation, released under the project's former name. The archives are named `fugo_<version>_<os>-<arch>.tar.gz` (`.zip` for Windows; up to v0.148.2 the archives carry the former name), and hold the `fugo` binary, `README.md`, `LICENSE`, `NOTICE`, `THIRD_PARTY_NOTICES.txt`, `PROVENANCE.md` and `THIRD_PARTY/`:

- `linux-amd64`, `linux-arm64` (glibc 2.35 or later)
- `darwin-amd64`, `darwin-arm64`
- `windows-amd64`

Check a download with `sha256sum -c fugo_<version>_checksums.txt --ignore-missing` (on macOS: `shasum -a 256 -c fugo_<version>_checksums.txt --ignore-missing`), then put `fugo` on your `PATH`:

```text
fugo version
fugo -s <site>                # build into the publish directory
fugo server -s <site>         # development server with live reload
```

The commands and flags (the Go build's, in kebab case, with its camelCase spellings as aliases) are listed in [crates/cli/README.md](crates/cli/README.md).

## Upgrading from the Go build

v1.0.0 replaces the Go build, under a new name and its own version numbers. What a site or a script may have to change:

- **The name is fugo**, and the source code avoids the program's name: the binary is `fugo` (`fugo version` prints `fugo v<version> …`), the release archives are `fugo_<version>_…`, and the repository is [getfugo/fugo](https://github.com/getfugo/fugo). The names a site uses carry no program name: the configuration file is `config.toml` (or `config.yaml`, `config.yml`, `config.json`; no file named after a program is read), the template object is `build` (`build.environment`, `build.is_server`, `build.generator`; it replaces the Go build's objects named after the program), and Sass imports the template's variables as `build:vars`. Settings are not read from environment variables: they come from the configuration files and the command line. The environment comes from `--environment`; secrets for templates go in the project's `.env` files (`get_env`), and `security.funcs.getenv` allows `^FUGO_` by default.
- **No PostCSS, Babel or Tailwind CSS pipeline, and no stats file** (`css.PostCSS`, `js.Babel`, `css.TailwindCSS`, `[build.buildStats]`): fugo runs no external programs ([External tools](#external-tools)). `[build] buildStats` and `writeStats` print a warning and are ignored; `[security.exec]` is accepted and has no effect.

- **Layouts are Tera 2 templates, not Go templates**, with the layout names of v0.146 and later (`home.html`, `single.html`, `_partials/`, `_shortcodes/`, `_markup/`). A layout with Go template syntax or a legacy name is an error that says what to change. [docs/rust-port/template-api.md](docs/rust-port/template-api.md) lists every function, filter and test with its Go-template name and how Go-template idioms translate; `fugo templates check -s <site>` checks a site's templates against it. Output is escaped by output format (HTML and XML), not by context as in Go's `html/template`: in `<script>` use `jsonify | safe`, in query strings `urlencode`.
- **Commands and flags:** `build` (also with no command), `server`, `templates check`, `config` and `version`; the Go build's `env`, `new`, `mod`, `deploy`, `list`, `gen`, `convert`, `import`, `release`, `server trust` and `config mounts`, and the `completion` and `help` commands are not available (`--help` prints the help). Of the Go build's flags, those [crates/cli/README.md](crates/cli/README.md) lists are accepted (`--noChmod` and `--noTimes` included); the logging and housekeeping flags (`--gc`, `--logLevel`, `--noBuildLock`, `--printI18nWarnings`, `--printPathWarnings`, `--printUnusedTemplates`, `--templateMetrics`, `--templateMetricsHints`) and the server's `--disableFastRender` and `--disableBrowserError` are not, so drop them from command lines such as `--gc --minify`. Boolean flags take an explicit value as before (`--minify=false`, `--buildDrafts=true`), and `=false` overrides the configuration as before (`-D=false` against `buildDrafts = true`). `--quiet` (also `-q` in fugo) only hides the build summary: warnings and errors are still printed, where the Go build discarded them too. Another, such as `--enableGitInfo`, `--contentDir`, `--disableKinds`, `--panicOnWarning` or the build's `-w`/`--watch`, is an error. As before, flags may come before or after the command (`fugo -s <site> server`), and every command takes the persistent flags (`-s`, `-d`, `-e`, `--config`, `--configDir`, `--themesDir`, `--clock`, `--quiet`, `-M`). A build prints one summary line (`pages … | files … | … | static files …`, then `Total in N ms`) instead of the Go build's per-language statistics table. `config` prints fugo's resolved configuration model (snake_case fields, one entry per site under `sites`, the merged user keys under `raw`), not the Go build's lower-cased keys (`baseurl`, `publishdir`, …), so a script that reads its output must change; it prints JSON by default (the Go build: TOML) and takes `--format json` or `toml`, not `yaml`, `--lang` or `--printZero`. A usage error exits with 2 (the Go build exited with 1 on every error), and error messages start with `error:` (usage) or `ERROR` (build) instead of `Error:`.
- **`server` renders into memory by default.** The Go build wrote the site to the publish directory and served it from there (`-M`/`--renderToMemory` rendered into memory). For the Go behaviour add `--render-to-disk`, a flag of fugo, not of the Go build; `-d`/`--destination` without it is an error.
- **Modules** are not downloaded (no `go.mod` resolution): themes and modules come from the themes directory, `_vendor` or an absolute path.
- **`js.Build` bundles in process with [rolldown](https://rolldown.rs)** (the Go build linked esbuild 0.25.6): nothing to install, and the options are the same. Scripts behave as before, but their bytes differ, so fingerprinted names, `Data.Integrity` and source maps change. Other visible differences: the IIFE wrapper is `(function() { … })();`; legal comments stay where they are instead of moving to the end; error texts are rolldown's, except unresolved imports (`Could not resolve "x"`) and the `es5` target's errors, which keep esbuild's wording and positions. As with esbuild, TC39 decorators are lowered, `target: es5` checks and lowers the bundle, and CSS imported from scripts is dropped (`local-css` modules give their class names). Sass is compiled in process with dart-sass semantics, whatever `transpiler` says.
- The Docker images of the Go build, published under the former name, are no longer updated; they stay at the last Go build.
- The website [getfugo.github.io](https://getfugo.github.io) is no longer redeployed on release tags, and until it is redeployed from `docs/` it documents the Go build. fugo's documentation is now `docs/`, a site with its own theme built by fugo (`tools/docs/build.sh`). The Go build's documentation site is kept as the test fixture `testdata/legacy-docs/`: `tools/legacy-docs/build.sh` builds it with fugo (Tera layouts in `sites/docs`), and gate A-D3 checks every page of that build against the published one.

## Build from source

Prerequisites to build fugo from source:

- Rust 1.96 or later (`rust-version` in `Cargo.toml`; CI builds with 1.96.0)
- A C compiler (libwebp and ring are compiled with the `cc` crate)

Build fugo:

```text
cargo build --release --locked -p ssg-cli
```

The binary is `target/release/fugo`. [DEVELOPMENT.md](DEVELOPMENT.md) describes the workspace, its tests and the CI and release workflow.

## External tools

None: every asset pipeline runs in process, and fugo runs no programs. It installs the npm packages of a project's `package.json` itself, for `js_build` and Sass imports. There is no PostCSS, Babel or Tailwind CSS pipeline: `minify` adds the vendor prefixes for the project's browserslist, `purge_css` purges per page, `js_build` compiles TypeScript and JSX and lowers modern JavaScript for the browser targets, and Tailwind's own CLI runs next to fugo ([docs/content/asset-pipelines/tailwind-css.md](docs/content/asset-pipelines/tailwind-css.md)).

## Star History

[![Star History Chart](https://api.star-history.com/svg?repos=getfugo/fugo&type=Timeline)](https://star-history.com/#getfugo/fugo&Timeline)

## Documentation

- [docs/](docs/): fugo's documentation site. Build it with `tools/docs/build.sh` (or preview it with `fugo server -s docs`); `docs/content/` is readable as Markdown too.
- [crates/cli/README.md](crates/cli/README.md): the commands and flags.
- [docs/rust-port/template-api.md](docs/rust-port/template-api.md): every template function, filter and test, with its Go-template name and how Go-template idioms translate.
- [Upgrading from the Go build](#upgrading-from-the-go-build), above, and the known differences from the Go implementation in §7 of [docs/rust-port/HANDOFF.md](docs/rust-port/HANDOFF.md).
- The website [getfugo.github.io](https://getfugo.github.io), which documents the Go build (v0.148.2 and earlier) until it is redeployed from `docs/`.

## Support

Please **do not use the issue tracker** for questions or troubleshooting: ask in fugo's [discussions]. Use the [issue tracker] for defects of fugo and for feature requests. Report only fugo issues here.

## Contributing

You can contribute to fugo by answering questions in the [discussions], reporting and fixing bugs, improving the documentation and proposing features. Before you work on a feature, open an issue with the feature request template so that it can be discussed first. The [Contribution Guide](CONTRIBUTING.md) covers the code guidelines, the commit messages and the checks a pull request must pass.

The code is the Cargo workspace at the repository root: [DEVELOPMENT.md](DEVELOPMENT.md) has the layout, the commands and the CI and release workflow, and [docs/rust-port/HANDOFF.md](docs/rust-port/HANDOFF.md) the crate map, the parity gates, the deviations from the Go implementation and the open items.

## Origin and attribution

fugo began as a fork of another static site generator and was rewritten in Rust: v0.148.2 and earlier were that generator's Go code with the fork's changes, released under the project's former name, and v1.0.0 and later are the Rust rewrite. Parts of fugo derive from the original project, which is licensed under the Apache License 2.0: templates rewritten from its embedded templates, its LiveReload plugin, behaviour transcribed from its Go sources, and its test data and documentation used as test fixtures. Each of these is listed in [PROVENANCE.md](PROVENANCE.md), and [NOTICE](NOTICE) carries the attribution of the derived material.

fugo is developed independently: it is not affiliated with, sponsored by or endorsed by the original project, its maintainers or its sponsors.

## License and dependencies

fugo is licensed under the [Apache License 2.0](LICENSE); [NOTICE](NOTICE) holds the attribution notices of the work it derives from. It stands on the shoulders of great open source libraries. The Rust crates it uses are declared in [Cargo.toml](Cargo.toml) (`[workspace.dependencies]`, locked by `Cargo.lock`); material taken from other projects is listed in [PROVENANCE.md](PROVENANCE.md), with licences cargo cannot see in [THIRD_PARTY](THIRD_PARTY/README.md). The `THIRD_PARTY_NOTICES.txt` of each release archive holds the licences of the crates linked into that binary.
