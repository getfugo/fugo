# ssg-config

The configuration of a project (docs/rust-port/REWRITE_PLAN.md §2.4, §3.1 A1): one
`Value`-tree pipeline, then typed serde structs whose `Default` holds Go's defaults.

```
LoadOptions { source, config_files, cli: CliOverrides, env }
  └─ load() ─► Config { sites: IdVec<LangIdx, SiteConfig>, output_formats, media_types, … }
```

1. **Bootstrap**: environment = `--environment` → `production` (no variable chooses it);
   config directory = `--configDir` or `config`.
2. **Sources** (`source`): the project file, the first that exists of `config.toml`,
   `config.yaml`, `config.yml`, `config.json` (`config_file_names()`; the Go program's
   configuration file name is not read). When several exist, the first is read and a
   warning (`config-file-ignored`, at the file read) names the others. Or the `--config`
   list (unchanged: first file wins, `custom` finds `custom.toml`…). Then `config/_default/**`
   and `config/<env>/**` in path order. A file's name places it: `config.*` at
   the root (a file with the Go program's configuration file name is an ordinary file, under
   its base name), `params.en.toml` under `languages.en.params`, `menus.en.toml` (or
   `menu.en.toml`) under `languages.en.menus`, `name.toml` under `name`.
3. **Normalise** (`tree::normalize_keys`: lower-case keys except inside arrays, `menu` →
   `menus`, drop `internal`) and **migrate legacy keys** (`tree::LEGACY_KEYS`, applied at the
   root and in each language table; a current key wins; each migration is a deprecation
   `Diagnostic`).
4. **Merge once**: file < `_default` < `<env>` < CLI (`CliOverrides::to_tree`); settings never
   come from the environment (`env`). `disableKinds`/`disableLanguages` strings are split on commas and white space.
   `_merge = "none"` in a table makes it replace instead of merge (a later file's `_merge`
   wins over an earlier one's).
   **Then the themes** (`theme`, below): found, their configuration read, and merged below
   the project's with Go's `_merge` strategies (`merge`, below). Legacy keys are migrated
   once more over the merged tree.
5. **Languages**: the enabled languages (not `disabled`, not in `disableLanguages`) sorted by
   (weight, key), the default language first. Default = `defaultContentLanguage`, else `en`
   if configured, else the first language. Each language's tree is `languages.X` merged over
   the root: tables merge deeply, `menus`, `taxonomies` and `permalinks` replace.
6. **Typed decode** (`de`): a serde `Deserializer` over `Value` that matches struct fields and
   enum variants ignoring ASCII case and converts scalars weakly (`"true"`, `"12"`, a single
   value for a list). Errors carry the key path; `load` maps it back to the file, line and
   column the value was written on (TOML via `toml::de::DeTable` spans; YAML/JSON by key
   search) and reports the key as written (`languages.th.pagination.pagerSize`); a value a
   theme brought in is located in the theme's file.

## Themes

**Finding them** (`theme::collect`, Go's `modules/collect.go`):

- A configuration imports its `[[module.imports]]` (`path`, `ignoreConfig`, `ignoreImports`,
  `noMounts`, `disable`, `mounts`), then the names of `theme` (a string or a list). The
  project's imports come first, and each theme's imports follow it depth first:
  `theme = ["a", "b"]` with `a` importing `c` gives `a`, `c`, `b` — the order of precedence
  (`Config::themes`, the vfs module order). An import seen before (paths compared ignoring
  case and a `/vN` major version suffix) keeps its first place; `disable` drops it;
  `ignoreConfig` reads neither the theme's configuration nor its imports; `ignoreImports`
  reads its configuration only.
- Where: `_vendor/<path>` when a `_vendor/modules.txt` of the importer (or of an earlier one)
  lists the path (the first listing wins, the closest with `module.vendorClosest`; paths
  matching the glob `ignoreVendorPaths` are not looked up there), else `<themesDir>/<path>`,
  or the absolute path. The project's `module.replacements` (`"old -> new, …"`, a string or a
  list) renames `[[module.imports]]` paths (not `theme` names, as in Go). A theme's own
  imports must stay below `themesDir` unless replaced. An import that is not found is an
  error (`ConfigError::ThemeNotFound`): remote modules are not downloaded.
- Configuration: the first of `config.*` in the theme's directory
  (with the same warning), then its `config/_default/**` and `config/<environment>/**`,
  assembled like the project's. `theme.toml` is theme-site metadata, not configuration.
- Mounts (`Theme::mounts`, used by `ssg-vfs`): the importer's `[[module.imports.mounts]]`,
  else the theme's own `[[module.mounts]]` (sources relative to the theme's directory), else
  `ThemeMounts::Components` (each component directory it has, plus its JS config files);
  `noMounts` gives `ThemeMounts::None`. Mount targets are validated here, located in the file
  that configured them.

**Merging them** (`merge::merge_themes`, Go's `Merge` with `_merge` strategies from
`common/maps/params.go` and `config/defaultConfigProvider.go`). The project's values always
win; a theme only adds keys the project lacks, as the strategy of the project's table allows.
Themes merge one after the other in precedence order, so an earlier theme wins over a later
one.

| table of the project | strategy when it has no `_merge` |
|---|---|
| `params`, at any level (also `languages.X.params`) | `deep` |
| `menus`, `languages.X.menus` | `shallow` |
| `outputFormats`, `mediaTypes` | `shallow` |
| `languages`, `outputs`, `taxonomies`, `permalinks`, `markup`, `sitemap`, `build`, … (any other key of the root) | `none`, or the root's `_merge` when written there |
| a table below those | its parent's |

- `deep` adds the theme's keys at every level; `shallow` adds keys to its own table only, not
  to the tables inside it (a theme adds media types, but no key to the project's
  `mediaTypes."text/x"`); `none` adds nothing to its table, but a table below it with a
  strategy of its own still merges (so `languages.en.params` merges although `languages` is
  `none`). `_merge` accepts `none`, `shallow`, `deep` (any case); another value means `deep`.
- At the root: a table the project does not have is taken from the theme when its default
  strategy is not `none` (so a theme's `[params]`, `[menus]`, `[outputFormats]`,
  `[mediaTypes]` come in; its `[taxonomies]` or `[outputs]` do not); values that are not
  tables (`title`, `baseURL`, `disableKinds`, …) come in only with `_merge = "deep"` at the
  root; `_merge = "none"` at the root ignores every theme's configuration.
- Languages: every project language gets a (`deep`) `params` table for the theme's
  `languages.X.params`; the theme's `languages.X.menus` add menus only to a project language
  that has menus. Without `[languages]` the project has one implicit language
  (`defaultContentLanguage`, else `en`): the languages a theme defines are added, but the
  implicit language takes nothing from the theme's table for it (unless the root is `deep`).
  With `[languages]`, the project's list is kept.
- `_merge` written in a theme travels with the tables it brings in and governs later themes'
  merges into them; on a table the project has, the project's strategy decides.
- A theme's `theme`, `module`, `themesDir`, `environment`, `configDir`, `cacheDir` and
  `workingDir` describe the theme or were needed to find it: they are never merged (Go merges
  them under a `deep` root but has already used the project's).

## Public API

| item | what |
|---|---|
| `load(&LoadOptions) -> Result<Config, ConfigError>` | the pipeline |
| `LoadOptions` | `source`, `config_files`, `cli`, `env` (the process environment: `FUGO*` plus `HOME`, `XDG_CACHE_HOME`, `TMPDIR`, `USER`) |
| `CliOverrides` | typed CLI layer (`base_url`, `environment`, `destination`, `minify`, `build_drafts/future/expired`, `cache_dir`, `themes_dir`, `theme`, `ignore_cache`, `config_dir`, `no_times`, `no_chmod`); `to_tree()` |
| `Config` | `project_dir`, `environment`, `config_files` (the themes' then the project's, lowest precedence first), `sites`, `disabled_languages`, `multihost`, `default_language_in_subdir`, `output_formats: Arc<OutputFormats>`, `media_types: Arc<MediaTypes>`, `content_types`, `default_output_format`, `dirs`, `cache_dir`, `mounts`, `themes: Vec<Theme>`, `build`, `caches`, `security`, `privacy`, `imaging`, `minify`, `content: ContentFilter`, `timeout`, `ignore_files`, `ignore_logs`, `enable_git_info`, `raw: Params`, `diagnostics` (deprecations, ignored configuration files; the build reports them); `default_site()`, `site(key)` |
| `Theme` / `ThemeMounts` (`theme`) | a theme in precedence order: `path`, `dir`, `owner`, `config_files`, `mounts` (`Components`, `Configured(Vec<MountConfig>)`, `None`), `vendored`; `theme::path_key` |
| `merge` | `MergeStrategy` (`None`, `Shallow`, `Deep`; `parse`, `written`, `default_for`), `merge_themes(&mut Map, themes)` |
| `config_file_names()`, `CONFIG_BASE_NAMES`, `CONFIG_EXTENSIONS` | the configuration file lookup order |
| `SiteConfig` (`site`) | per language: `language: Language` (key, name, code, `Direction`, weight, `time_zone`, `url_prefix`, own title), `base_url: BaseUrl`, `title`, `copyright`, `params: Params`, `taxonomies: IdVec<TaxonomyIdx, TaxonomyDef>`, `outputs: KindOutputs`, `permalinks: Permalinks`, `pagination`, `markup: MarkupConfig`, `front_matter: Vec<(DateField, Vec<DateSource>)>`, `related`, `sitemap`, `services`, `menus: Vec<MenuEntryConfig>`, `cascade: Vec<CascadeConfig>`, `urls: UrlPolicy` (`LinkStyle`, `LinkOutput`, `UglyUrls`, `PathCase`, `Accents`), `disable_kinds: KindSet`, `aliases: AliasPolicy`, `robots_txt: RobotsPolicy`, `emoji: EmojiPolicy`, `titles: TitleConfig`, `summary_length`, `content_dir`, `static_dirs`, `ref_links`, `main_sections`, `has_cjk_language`; `site_urls()` → `base::url::SiteUrls` |
| `MediaTypes` / `MediaType` (`media`) | `IdVec<MediaTypeId, _>` sorted by type string; `decode`, `get`, `by_type`, `by_suffix`; `BUILTIN`, `ContentTypes` |
| `OutputFormats` / `OutputFormat` (`output`) | `IdVec<FormatId, _>` in render order (non-zero weights ascending, then name); `Escaping`, `UglyPolicy`, `LinkPolicy`, `Listing`, `Placement`; `builtin`, `decode`, `by_name` |
| `global` | `Dirs`, `BuildConfig`, `CachesConfig`/`FileCache`/`MaxAge` (`:cacheDir`, `:project`, `:resourceDir` resolved), `SecurityPolicy`/`Whitelist`, `PrivacyConfig`, `ImagingConfig`, `MinifyConfig`, `MountConfig`, `ContentFilter` |
| `markup` | `MarkupConfig`: goldmark extensions/parser/renderer/render hooks (`UseEmbedded`), `HighlightConfig`, `TocConfig`, `AsciidocConfig` |
| `de`, `tree`, `env`, `duration` | the pipeline's building blocks (`de::from_value` is reusable for any case-insensitive typed decode) |

## Acceptance (T13)

`cargo test -p ssg-config -- --nocapture` prints the tallies.

| bullet | evidence |
|---|---|
| `nh-allconfig/load` values for docs and testsite | `load::acceptance_sites` (strict, no accepted differences): `repo/docs`, `repo/docs-development`, `repo/testsite` 294/294 checks. Each case compares the language order, multihost, timeout, compiled cache directories, per-language facts (baseURL, prefix, name, weight, title, time zone, direction), kind outputs, disabled kinds, and 57 keys of Go's per-language `config` command dump (zero values ignored; `[minify.tdewolff]` defaults belong to the minify crate) |
| … and t24 sites | `sites::t24_sites`: the 17 `sitebuild/assemble` sites load; languages equal Go's for all; the formats of the enabled kinds equal Go's `.Site` render formats for 25 of 26 site languages (`asm-taxo` adds `json` in page front matter, which the site config cannot know; the check is "a subsequence of Go's") |
| other `load` groups (coverage) | `load::other_groups`: basic 2738/2747, configdir 1015/1015, env 2051/2054, languages 2287/2290, merge 1777/1780, mounts 1045/1045, sections 3128/3139, themes 1135/1136; every difference is listed in `expected_diffs.toml` with its reason (a configured media type's `delimiter`, which Go's dump shows as written, is compared as decoded) |
| themes (C1) | the `merge/*` and `themes/*` groups above compare every merged value of Go's per-language dumps (params, language params, menus, output formats, media types, taxonomies, languages, …); `themes::*` ports the Go site builder's `config_test.go` (`TestLoadConfigFromThemes` default/shallow/no params/sitemap by root strategy, `TestLoadConfigFromThemeDir`, `TestLoadConfigThemeLanguage`, `TestLoadConfigModules`, `TestConfigOutputFormatDefinedInTheme`), `config/allconfig` (`TestMergeDeep`, `TestMergeDeepBuildStats*`) and `modules` (`TestDecodeConfig*`, `TestPathKey`) with their expected values, plus the lookup order, the rules above and error positions in theme files; `ssg-vfs` `mounts::theme_mounts_match_go` compares the 30 themes of the oracle cases (order, directory, vendoring, owner, mounts) |
| media tables equal | `media::builtin_media_tables`: the 41 built-in types (field by field, sorted: main, sub, mime suffix, suffixes, delimiter, first/full suffix) and the 6 default content types equal Go's; `media::output_formats`: the 15 built-in output formats in render order, every field |
| `[mediaTypes]`/`[outputFormats]` decoding | DecodeTypes 1165/1410 equal, DecodeConfig 1203/1530 equal; named cases that differ are listed in `tests/it/media.rs` with reasons; generated cases are tallied by category (see deviation 8) |
| legacy-key table on a synthetic real-site-style config | `api::legacy_keys`: a real-site-style config written with every legacy key (root and language level) decodes to the current settings; the deprecation notices are an insta snapshot |
| no settings from the environment; `CliOverrides` | `api::environment_variables_are_not_settings` (variables named like settings change nothing); `api::cli_overrides_and_precedence` (file < dir < env dir < CLI; only `--environment` chooses the environment directory); `env_file::the_environment_s_file_wins` |
| `[caches]` with `:cacheDir`/`:project`; privacy | `api::caches_resolve_placeholders`, `api::privacy`, plus the oracle's `cachesCompiled` and `privacy` in every load case |
| error spans | `api::error_positions`: TOML, YAML and JSON syntax errors; typed errors in a config-dir file, in a language table, in an array element; language errors — each with file, line and column |
| insta snapshots | `tests/it/snapshots/`: typed summaries of docs, testsite and the 16 t24 sites; deprecation notices; an error display |

## Accepted deviations

1. **Themes** follow Go's rules (above) with these differences:
   - Remote modules are not downloaded (no `go.mod` resolution, `module.workspace`, version
     range checks): a theme comes from `themesDir`, `_vendor` or an absolute path, and one
     that is not found is always an error (Go's `mod` commands can ignore it:
     `themes/missing-ignored`).
   - A theme's `theme`, `module`, `themesDir` and bootstrap settings are never merged (Go
     merges them under a `deep` root, after it has used the project's).
   - `_merge` values are matched ignoring case (Go: `"None"` is an unknown value, i.e.
     `deep`), consistent with the project's own `_merge = "none"`.
   - A project value that is not a table where a theme has a table is kept (Go panics).
   - Go copies the root `languageCode` into the only configured language before merging
     themes, which then hides a theme's `languages.X.languageCode` under a `deep` merge; that
     step is not reproduced.
2. **Project-wide settings come from the default language.** Go keeps imaging, media types and
   output formats per language; fugo has one of each per project (the default language's).
   Per-language `contentDir` and `staticDir` are kept (`SiteConfig::content_dir`, `static_dirs`).
3. **Untyped sections**: `deployment` (Go's `deploy` is not ported), `segments`, `httpCache`,
   `server`, the `module` workspace and version range and `[minify.tdewolff]` stay in
   `Config::raw`; their validation errors are not reported here (`module.imports`, `module.mounts` and
   `module.replacements` are read and checked by the theme step). Resample filter names are
   the images crate's (T41).
4. **Errors instead of silently ignored values**: a `[caches]` entry that is not a table,
   `FUGO_SITEMAP=weekly` (a table expected), an output format without a media type, a missing
   `--config` file, no configuration at all (Go leaves that check to the CLI), invalid media
   type keys (`/`, `text/`).
5. **No settings from the environment**: Go merged its own environment variables over the
   configuration; this port reads files and flags only (`env::is_read`: the environment
   variable and the cache directory's), so the oracle's `env` group was removed and cases that
   set a variable are not applicable. A number of seconds written as a string is a valid `maxAge`.
6. **`disableLanguages` inside a language table** is ignored (a root-only setting; Go applies
   the default language's merged value).
7. **The cache directory is never created while loading**: `$XDG_CACHE_HOME/fugo_cache` (or
   `$HOME/.cache/fugo_cache`; Go: its own name) is used when its first existing ancestor is a
   directory, else `$TMPDIR/fugo_cache_$USER`. A relative `cacheDir` (or `:project/…`) is an error, as in Go.
8. **No mapstructure weak decoding of struct fields**: a media type entry can set only
   `suffixes` and `delimiter` (not `mainType`, `subType`, `type`, `firstSuffix`,
   `suffixesCSV`), an output format's name is its key, non-string suffixes and fractional
   weights are rejected, and keys are normalised before decoding (so `RSS` and `rss` collide).
   These account for all differences of the generated `media` cases.
9. `--clock` is not configuration (the CLI parses it into `base::Clock`).
10. **fugo's names instead of Go's**: no configuration file with the Go program's name (the
   oracle cases' are replayed as `config.*`, `ssg_testkit::fixture::local_path`); no stats file:
   `build.writeStats` and `build.buildStats` are "no longer supported" warnings (`tree.rs`), and
   the comparisons leave Go's `buildStats` out. **No environment variable** chooses the
   environment or a setting (oracle cases that do are not applicable); the default
   `security.funcs.getenv` is `^FUGO_`, `^CI$`.
11. **The project's `.env` files** (`env_file`, not in Go): `.env`, then
   `.env.<environment>` over it; `NAME=value` lines read beside the configuration, never into it (`Config::env_file`, skipped when serialised, names only in
   `Debug`); `get_env` reads its names without the `getenv` allowlist, the process environment
   winning; `FUGO*` names in it are ignored with a warning (`env-file-prefix`).

## Notes for later tasks

- **Decoders for other crates**: `Permalinks::decode(&Value)` (rejects kinds without
  permalinks and non-string patterns, as Go does), `decode_front_matter(&Map)`,
  `decode_cascade(&Value)`, `DateSource::parse`; `markup::TocConfig::end_level` is
  `Option<u8>` (`endLevel = -1` is `None`).
- **base serde** (F1): `IdVec` and `Params` serialize and `Map` deserializes in base; `de_map`
  remains for sections where `null` means an empty table.
- **vfs (T20)**: default mounts for unconfigured components, `_jsconfig` auto-mounts and
  language content directories (`SiteConfig::content_dir`) are the file system layer's; the
  mount targets are validated here.
- **markup (T22)**: `UseEmbedded::Auto` is resolved by the renderer (Go: fallback for
  multilingual single-host sites); the default is already `Fallback` in that case.
