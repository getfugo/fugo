# ssg-vfs

Mounts → one union file view per component, walkers, ignore rules and the path parser
(REWRITE_PLAN.md §2.4, phases A2 and A3 of §3.1).

| API | What |
|---|---|
| `Vfs::new(&Config)` | the effective mounts: `[[module.mounts]]` (missing sources skipped), default mounts for unconfigured components (content per language from `languages.X.contentDir`, static per `staticDir*`, with the language only on multihost sites), the root JS config files → `assets/_jsconfig/`, then the mounts of each theme of `Config::themes` (see Themes); duplicates (source, target, lang) dropped per module |
| `Vfs::mounts`, `mounts_of` | the mounts in precedence order (project, then the themes in `Config::themes` order: the `theme` list and `[[module.imports]]`, each theme's own themes after it) |
| `Vfs::walk(c)` | the union view of a component, sorted by path (bytes) then mount precedence: first mount wins (content: per mount language; data and i18n keep every file; static: the last mount of the first module, see Rules) |
| `Vfs::open(c, rel)` | the winning file at `rel`, same rules as `walk` |
| `PathParser::from_config`, `parse(c, rel)` | `PathInfo` (key, path, name, section, ext, language, output format, `BundleKind`, layout parts, original spelling) or `Parsed::DisabledLanguage` |
| `Vfs::discover_content(&PathParser)` | phase A3: content files with path info and resolved language; leaf bundles demote their other files to resources; duplicate (key, language) pairs settled (`Discovery::duplicates`); content adapters apart (`Discovery::adapters`) |

## Rules

- **Themes.** `ssg-config` finds the themes (nested themes, `[[module.imports]]`,
  `_vendor`, replacements) and says what each mounts (`ThemeMounts`); here a theme's mounts
  are: its configured mounts (the importer's `[[module.imports.mounts]]`, else its own
  `[[module.mounts]]`; sources are relative to the theme's directory, an absolute one too, as
  in Go; missing ones skipped; `lang` resolved like the project's), or each component
  directory it has (Go's name order); then its root JS config files →
  `assets/_jsconfig/` unless a mount targets that directory; nothing with `noMounts`.
  `Module::Theme(n)` is the n-th of `Config::themes`.
- **Ignore rules.** Content, data, i18n: names starting with `.` or `#` or ending with `~`
  (files and directories) and `ignoreFiles` regexps (absolute file name). Layouts: files
  starting with `.` or ending with `~`. Assets, static, archetypes: none (the static copy keeps
  dotfiles). Symbolic links below a mount root are skipped, except in static (below).
- **Static** follows the Go implementation's static copy (`copyStaticTo` of its command package,
  the root-mapping and overlay file systems of its file system package): of the mounts of one
  module holding a path the *last* wins (`staticDir = ["static", "static-b"]`, or two
  `[[module.mounts]]` into `static`), and the project still wins over the themes (per language
  on multihost sites). Symbolic links below a static mount root are followed; a dangling link is
  skipped, and so is a link to a directory that is already on the walked path (loop protection).
- **File names on macOS** are NFC-normalised as they are walked, as Go does on darwin
  (`normalizeFilename`, `componentFs.applyMeta` of its file system package): HFS+ stores names
  decomposed (NFD) and APFS keeps the form they were created in, so `rel` (and with it paths,
  URLs and keys) and the names the ignore rules (the `ignoreFiles` regexps: the NFC form of the
  absolute name) and filters see are NFC; `abs` keeps the OS's name. Elsewhere names are used as
  they are. `entry_name` is the rule, for what compares names read from the file system with
  `rel` (the static copy's `cleanDestinationDir`).
- **`includeFiles`/`excludeFiles`** are Go globs (`base::glob`, case-folded) matched against
  the path below the mount source with a leading slash. A file matching an inclusion is kept,
  else one matching an exclusion dropped, else kept only without inclusions. A directory is
  walked when it matches an inclusion or is a directory leading to one (`/`, `/guide` for
  `guide/**.md`), so `guide/**.md` does not reach `guide/deep/x.md` (Go's rule).
- **Leaf bundles.** A directory is a leaf bundle when its first file is a leaf index, ranking
  module, bundle files first, suffix descending (`md` before `html`), key, mount, a language in
  the file name first, path. Everything below it becomes a resource except the index files of
  other languages in the bundle directory itself.
- **Content adapters** (`_content.html`, fugo's Tera adapters, and the Go implementation's
  `_content.gotmpl`, which `ssg-site` refuses with a hint) are `BundleKind::ContentAdapter`
  keyed by their directory, but no pages: discovery lists them in `Discovery::adapters` (one per
  directory and language, the others reported as duplicates), so an adapter and the section's
  `_index.md` coexist (Go keeps adapters in a tree of their own). `_content.html` is an adapter
  only in the content component (Go would read it as an HTML page; no oracle case has one).
- **Duplicates** (same key, language and page/resource tree): the kept file is the first by
  bundle index before single page (`foo/_index.md` before `foo.md`), `_index` before `index`,
  suffix descending, mount, file-name language, path. Go gets the same winners from its walk
  and insertion order (verified against the capture oracle, including its duplicate warnings).
  The plan put this in `site` (B1); it lives here because it is a rule about files, and `site`
  reports `Discovery::duplicates` as warnings.

## Acceptance evidence

`cargo test -p ssg-vfs`:

- `pathparser`: `oracle/common/paths/pathparser.json.gz`, 14,513 cases × 3 parsers, 324,666
  checks, 0 differences: `Base` (key), `BaseNameNoIdentifier`, `Path`, `Dir`, `Section`, `Ext`,
  `Type`, `Lang`, `OutputFormat`, `Layout`, `Kind`, `Is*`, `Disabled`, the normalisation, the
  unnormalised `Path`/`Base`/`BaseNameNoIdentifier`/`Section`, and the bundled-resource form
  (`ModifyPathBundleTypeResource`).
- `mounts`: `oracle/allconfig/load/mounts.json.gz`, all 13 cases equal (the invalid target is an
  error); `theme_mounts_match_go`: the 30 themes of `oracle/allconfig/load/{themes,merge}`
  (order, directory, `_vendor` version, importer, mounts incl. JS config files) equal Go's
  modules, and the cases Go fails fail.
- `capture`: `oracle/sitebuild/capture/{testsite,docs}` — (file, key, language, kind) for every
  file in the Go implementation's page and resource trees, plus name, section, extension and
  original base of every page: testsite 2, docs 1,011 files equal. Also contentdir, edge-tree,
  homeleaf, nokinds, shortcodes, synthetic.
- `walk`: nested themes and import options (`theme_mounts_and_nested_themes`), a missing theme
  (a configuration error; a theme directory removed after loading: `VfsError::ThemeNotFound`),
  mount precedence (project over themes, per-language content, data/i18n keep all,
  static later-mount-wins within a module and symlink following with loops),
  mounts below a component and single-file mounts, ignore rules, filters, disabled and unknown
  mount languages, symlinks, leaf bundles, duplicates, NFC names on macOS
  (`file_names_are_nfc_on_macos`; the normalisation itself is unit-tested on every platform).
- `walk::discover_sites` (ignored; `FUGO_VFS_SITES=<dir>:…`): whole `sites.py` sites.

## Accepted deviations

| What | Why |
|---|---|
| `original` names keep the normalised structure (192 oracle cases) | Go parses the unnormalised path again with case-sensitive lookups, so `Index.EN.md` or `UPPER.MD` get another structure there (`EN` no language, `MD` no content suffix). Here every lookup uses the normalised identifier, so `original.base` of `Index.md` is `/`, like its key. |
| Keys with an empty segment or a trailing slash (162 checks) | Go's `Base()` of `a//`, `/tags//_index.md` or a page file named `.md` keeps the slashes; `ContentKey` has neither. Walks never produce such paths (no empty segments; content names starting with `.` are ignored). |
| Go `TypeShortcode` outside layouts is `BundleKind::Resource` (144 cases) | A non-content file below `/_shortcodes/` in another component; Go treats it exactly like `TypeFile`. |
| Not modelled: `Container`, `ContainerDir`, `Identifiers`, `NameNoExt`, `NameNoLang`, `PathNoLang`, `PathBeforeLangAndOutputFormatAndExt`, `BaseReTyped`, `IdentifierBase`, `TrimLeadingSlash`, `ForType`, `PathRel`, `BaseRel` | Go conveniences; callers derive what they need from `key`, `path` and `dir()`. |
| A missing mount source of the Go build's stats file is skipped like any other | Go keeps it and creates the empty file, which it writes later; this port writes no stats file. |
| On macOS `abs` keeps the name the OS returned; only `rel` is NFC | Go normalises its absolute file names too. Reading by the OS's name also works on file systems that do not normalise names, and keeps the server's watcher events (which carry the OS's names) matching `abs`. |
| `walk` returns a `Vec` in byte order, not Go's `ReadDir` order | Order only affected Go's insertion ids; the trees are keyed. |
| Discovery is sequential | The plan's `par_iter` over mounts is not needed: the docs site (1,000 files) walks in milliseconds. |
| Pages of disabled kinds and front matter `path`/`lang` moves | Not file-system rules: `page`/`site` apply them (the capture test leaves those files out). |
