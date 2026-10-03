# ssg-highlight

Code highlighting for fugo (T25; REWRITE_PLAN.md §1.1 decision D1, §2.1): a port of
[Chroma](https://github.com/alecthomas/chroma) v2.19.0, the highlighter the Go implementation
uses. Chroma's lexers (its XML definitions converted to Rust data, its Go-written lexers ported)
run on a port of Chroma's regex-lexer engine and of the .NET regex dialect they are written in
(regexp2); the tokens go through Chroma's HTML formatter (line structure, line numbers,
highlighted lines, **Chroma class names** or **inline styles** from **Chroma's own style
files**) inside the Go implementation's wrappers. The output is the Go implementation's, byte
for byte. Sites keep their Chroma style sheets (the docs' `chroma.css`), and the output of the
Go program's `gen chromastyles` command can be reproduced ([`Highlight::css`]).

## API

```rust
pub struct Highlight;                                     // Send + Sync; build once per build
impl Highlight {
    pub fn new(&HighlightConfig) -> Self;                 // [markup.highlight]; first call registers the lexers (~7 ms dev)
    pub fn highlight_with(&self, code, lang, OptionsArg) -> Result<String, HighlightError>; // `highlight` function
    pub fn highlight(&self, code, lang, &Options, attributes: Option<&Map>) -> String;
    pub fn css(&self, style, CssMode) -> Result<String, HighlightError>;  // gen chromastyles
    pub fn can_highlight(&self, lang) -> bool;            // transform.CanHighlight
    pub fn tokens(&self, code, lang) -> Option<Vec<(TokenType, String)>>; // Chroma's coalesced tokens
    pub fn diagnostics(&self) -> Vec<Diagnostic>;         // unknown style names (fallback warnings)
    pub fn defaults(&self) -> &Options; pub fn lexer(&self, lang) -> Option<Lexer>;
    pub fn style_names(&self); pub fn lexer_names(&self); // Chroma's styles; lexers in registration order
}
impl ssg_markup::Highlighter for Highlight { .. }     // fences no code-block hook handles
pub enum OptionsArg<'a> { None, Str(&'a str), Map(&'a Map) }   // "linenos=table,hl_lines=2" or a dict
pub struct Options { style, styling: Styling, line_nos, line_number_layout: LineNumberLayout,
                     anchor_line_nos, line_anchors, line_no_start, hl_lines, hl_ranges,
                     layout: CodeLayout, tab_width, guess_syntax, wrapper_class }
impl Options { from_config, apply_str, apply_map, highlight_ranges }
pub enum Styling { Classes, Inline }  LineNumberLayout { Table, Inline }  CodeLayout { Block, Inline }
pub enum TokenType { .. }  // Chroma's types: name, number, class(), parent(), category()
pub enum CssMode { AllClasses, OmitEmpty }
```

Options follow the Go implementation: the site's `[markup.highlight]`, then a fence's `{…}`
options (keys case-insensitive, `hl_lines` as markup's 0-based ranges shifted by that map's
`linenostart`), or the function's option string / map (weakly typed like mapstructure: `"true"`,
`1`, `"0x10"`). `linenos=table|inline` also picks the layout. A fence without `lineanchors`
(site or fence) numbers its line ids `hl-<ordinal>-<n>`, the ordinal being
`HighlightOptions::ordinal` (the page's code block count), as Go does; the function has no
prefix. Invalid values are errors (`OptionsError`).

## How it works

- **Regex dialect** (`src/regexp2/`): Chroma compiles every rule with dlclark/regexp2, a port of
  .NET's engine, and which match a rule finds decides the tokens. `parser.rs` and
  `charclass.rs` port regexp2's parser and character classes (capture numbering with named
  groups after numbered ones, inline options and `x`-mode comments, literal `{` that is no
  quantifier, `\<` a literal unless it names a group, class subtraction `[a-z-[aeiou]]`, Unicode
  `\w` `\s` `\d` `\p{L}`, .NET's lower-case table under `i`). `vm.rs` runs the tree with an explicit
  backtracking stack (no recursion) and regexp2's rules: alternatives in order, `*`/`+` loops
  ending on an empty iteration (`Branchmark`), counted loops (`Branchcount`), lazy variants,
  atomic look-around that keeps a positive body's captures, look-behind of any length matched
  right to left, back-references, `\G`. A first-character filter skips rules that cannot
  start at the position.
- **Lexer engine** (`src/chroma/`): Chroma's `RegexLexer` — rules compiled as
  `\G(?flags)(?:pattern)`, `include` expanded and `combined` states created at compile time
  (lazily, on a lexer's first use), the state machine (first matching rule wins; an unmatched
  character is an `Error` token; an unmatched newline outside the start state resets the stack;
  `EnsureNL`, `EnsureLF`), the emitters (`token`, `bygroups`, `using`, `usingself`,
  `usingbygroup`) and mutators (`push`, `pop`, `#pop`, `mutators`), `Ignore` tokens dropped,
  `Coalesce`; tokens are produced eagerly, an `EOF` marker standing for an iterator's early
  end (HTTP's `Content-Type` body): `Coalesce` ends a run there and reads on, a consumer of a
  nested iterator (the iterator stack, `Concaterator`) stops, as in Chroma;
  `DelegatingLexer` (template languages in HTML), `TypeRemappingLexer`; the
  registry (`lexers.Get`: name, alias, then `filename.<lang>` and `<lang>` against the file
  name patterns by priority, Go's `filepath.Match`; `MatchMimeType`; `Analyse` for
  `guessSyntax`, Chroma's `fallback` lexer when nothing scores). The file name part of a
  lookup is cached per name, as Go's `chromalexers.Get` caches `lexers.Get`: a language no
  lexer knows (`output`, `console`) tries every pattern once per process, not once per fence.
  The state stack is a persistent list (`stack.rs`): the zero-width-loop guard (deviations)
  and Haxe's pre-processor copy it in O(1).
- **Lexers**: `src/chroma/lexers/*.rs` are Chroma's 255 XML lexers, converted to Rust (Lexer
  and style files; `lexers/mod.rs` lists them in Chroma's registration order, `defs.rs` has the
  types). Chroma's lexers written in Go are in `src/chroma/golexers/exported/*.rs`, exported
  from Chroma's Go code in its XML format and converted (Fixtures); `golexers.rs` ports their
  Go functions (the Go lexer's raw strings lexed as Go text templates, HTTP's header/body
  emitters and its `Content-Type` sub-lexer, reStructuredText's code blocks, Haxe's
  pre-processor mutator), wraps them like Chroma (Go HTML templates, Markdown, PHTML and Svelte
  delegate to HTML; Common Lisp and Emacs Lisp remap words, `golexers/lisp.rs`) and sets the
  Go, DNS, MySQL and Zed analysers. 270 lexers, registered in Chroma's order (its XML lexers by
  file name, then the Go ones in Go's initialisation order, replacing lexers of the same name
  in place).
- **Styles** (`src/style.rs`, `src/styles/*.rs`). All 67 Chroma v2.19.0 style files,
  converted to Rust like the lexers, with Chroma's inheritance (`Background`, `Text`, category,
  sub-category, `noinherit`), its synthesised line-number and line-highlight colours, its CSS
  properties and compression.
- **HTML** (`src/html.rs`): Chroma's formatter (lines split after `\n`, `line`/`cl` spans,
  `ln` inline or the `lntable` layout, `hl` lines, `lnlinks` anchors, `style` attributes with
  sub-category/category fallback) inside the Go implementation's wrappers (`<div
  class="{wrapperClass} {class}" attrs>`, `<pre tabindex="0">`, `<code class="language-x"
  data-lang="x">`, `hl_inline`'s `<code class="code-inline language-x">`). Fences get a final
  newline (Go's code block renderer); the function does not. Code without a lexer is escaped
  as is (Go's plain `<pre><code>`).

## Acceptance

`cargo test -p ssg-highlight --test it -- --nocapture` prints the tables.

| criterion | result |
|---|---|
| all docs fences highlight | **2,284/2,284** items render: 1,996 fences (goat fences go to the goat hook), 285 `code-toggle` bodies (in the format they are written in), 2 `highlight` and 1 `hl` shortcodes; 2,283 have a Chroma lexer, 1 (`texts`, a typo) stays plain exactly as in Go |
| **byte-identical to the Go implementation** (docs config: `noClasses=false`, `solarized-dark`, `lineNumbersInTable=false`, wrapper `highlight not-prose`; hashes of Go's `transform.Highlight` output) | **2,284/2,284** (asserted) |
| token classes (non-whitespace characters Chroma classifies, 181,725) | **100.0%** the same class, every lexer (asserted) |
| Chroma's own lexer test suite (`lexers/testdata`, 298 inputs, `*.expected` tokens) | **296/298** token streams identical; the 2 Raku inputs differ (not ported) |
| `guessSyntax` (Chroma's `lexers.Analyse` over those inputs and its analysis inputs, 305) | **305/305** pick the same lexer |
| lexer lookup (`src/data/chroma-lexers.tsv`: `lexers.Get` of every name, alias and file pattern) | every entry |
| every lexer compiles | 270/270 (and the 269 lexer files) |
| solarized-dark CSS | byte-identical to the Go program's `gen chromastyles --style=solarized-dark`, with and without `--omitEmpty` |
| inline styles (`noClasses`), `hl_inline`, `lineNumbersInTable`, `linenos`, `hl_lines`, `linenostart`, `anchorlinenos`, `lineanchors`, `tabWidth`, `wrapperClass`, styles | option matrix (5 inputs × 19 option sets: known/plain/unknown/no language) **95/95 byte-identical** to the Go implementation |
| docs site (live-check against getfugo.github.io) | all 3,543 `<pre>` blocks and 3,514 wrappers/inline code byte-identical, including the style gallery (67 styles × 6 languages, inline styles) |
| Chroma style names → styles, with a fallback warning | every Chroma style is bundled; an unknown name falls back to Chroma's `swapoff` (as in Go) and is reported once by `diagnostics()` |

Speed: `Highlight::new` ≈ 3 ms (dev; it registers the 270 lexers from their static data; a
lexer builds and compiles its rules on first use); the 2,284 docs items ≈ 0.6 s in a dev
build (syntect took 1.9 s); the docs site builds in ≈ 1.35 s release (≈ 1.9 s with
syntect). Tokenising is linear in the input, deep stacks included (release, tokens only;
Chroma v2.19.0 in parentheses): Sass 100 KB ≈ 40 ms (≈ 200 ms), 1 MB ≈ 340 ms (1.8 s); Haxe
`(`×12,000 `)`×12,000 16 ms (65 ms); Metal 1 MB 330 ms (1.4 s); Racket 40 KB of nested lines
22 ms (100 ms). An unknown fence language costs one file name lookup (≈ 0.3 ms) per name and
process: a 50-page site with 2,000 `output` fences builds in ≈ 20–60 ms. What is slow in
Chroma itself stays slow, though not slower: regex backtracking over a 40 KB identifier
(Makefile 24 s, Chroma 67 s), Svelte's lexer, quadratic in Chroma too (30 KB 0.34 s, Chroma
1.3 s).

Size: the release binary is ≈ 2.1 MB smaller than with the XML (65.0 MB against 67.1 MB,
stripped, macOS arm64), ≈ 0.6 MB larger than with syntect (it was ≈ 2.7 MB larger). The Rust
data holds each pattern and name once; the XML was 2.0 MB of text (lexers 1.9 MB, styles
0.13 MB), and the 255 lexers were in the binary twice (their table was a `const` used in two
places).

## Lexer and style files

Chroma keeps its lexers and styles as XML (`lexers/embedded/*.xml`, `styles/*.xml`); the crate
keeps them as Rust, one file each, converted from that XML by `tests/it/xml2rust.rs`:
`src/chroma/lexers/<name>.rs`, `src/chroma/golexers/exported/<name>.rs` (Chroma's Go lexers)
and `src/styles/<name>.rs`, each directory's `mod.rs` listing them in Chroma's order (its file
names, bytewise: the lexers' registration order). They are `static` data, compiled like the rest
of the crate: nothing is parsed at run time, a misspelt token type does not compile, and neither
does a rule with two emitters or two mutators (the builders below assert it). The conversion is
exact: every lexer's configuration and rules, and every style, equal what the XML gave (checked
against the crate's former XML reader, by the acceptance tests and by a differential run of
5,119 inputs). The XML's comments are Rust comments. A file whose name is no Rust identifier
gets one (`c#.xml` → `csharp.rs`, `c++.xml` → `cpp.rs`, `-` → `_`); `file` keeps Chroma's
name.

A lexer (the builders and types are in `src/chroma/defs.rs`):

```rust
#[rustfmt::skip]
pub(crate) static LEXER: LexerDef = LexerDef {
    file: "go_template",                   // Chroma's file name
    config: ConfigDef {                    // Chroma's `Config`
        name: "Go Template",
        aliases: &["go-template"],
        filenames: &["*.gotmpl", "*.go.tmpl"],
        // also alias_filenames, mime_types, case_insensitive, dot_all, not_multiline,
        // ensure_nl, priority, analyse (`Some(AnalyseDef { first, regexes: &[(r"…", 1.0)] })`)
        ..ConfigDef::EMPTY
    },
    states: &[
        ("template", &[
            rule(r"[-]?}}").token(T::CommentPreproc).pop(1),
            rule(r#""(\\\\|\\"|[^"])*""#).token(T::LiteralString),
            include("expression"),
        ]),
        // …
    ],
};
```

- A rule: `rule(r"…")`, the .NET regex as written (`rule("")`: the empty pattern), then at
  most one emitter and one mutator; `include("state")` is a rule standing for that state's
  rules. Patterns are raw strings (`r"…"`, or `r#"…"#` when they hold `"`); the one holding a
  control character (U+0085) is an escaped string.
- Emitters: `.token(T::Type)`; `.groups(&[T::A, T::B])` (one token type per group);
  `.bygroups(&[E::Token(T::A), E::UsingSelf("state"), E::Nil])` (one emitter per group,
  `E::Nil`: the group emits nothing); `.using("Lexer")`; `.using_self("state")`;
  `.using_by_group(name_group, code_group, &[E::Token(T::A), …])`; `.emit_func("name")` (a Go
  function, ported in `golexers.rs`).
- Mutators: `.push(&["state"])` (several states; `"#pop"` pops; `&[]` pushes the current state
  again); `.pop(1)`; `.include("state")`; `.combined(&["a", "b"])`;
  `.mutators(&[M::Push(&["a"]), M::Pop(1)])`; `.mutator_func("name")`.

A style (`src/styles/<name>.rs`): `static STYLE: StyleDef`, its `name` and its `entries` from
token types to Chroma's entry syntax (`(T::Keyword, "bold #0000ff")`,
`(T::Background, "bg:#ffffff")`).

The files are the source: edit them like any code. To move to another Chroma version, convert
its files (directories relative to the repository root; the converter rewrites the files and
`mod.rs`; a file Chroma removed is deleted by hand), then update the Go lexers (Fixtures), the
fixtures and the acceptance numbers:

```sh
C=$(go env GOMODCACHE)/github.com/alecthomas/chroma/v2@v2.19.0
FUGO_HL_XML2RUST=$C/lexers/embedded:crates/highlight/src/chroma/lexers \
  cargo test -p ssg-highlight --test it xml_to_rust
FUGO_HL_XML2RUST=$C/styles:crates/highlight/src/styles \
  cargo test -p ssg-highlight --test it xml_to_rust
```

## Accepted deviations

- **Raku** is registered with its configuration and plain-text rules: Chroma's Raku lexer is Go
  code that rewrites the lexer's compiled rules while it runs (and uses named-group emitters);
  it is not ported. Raku code renders in Chroma's structure as one text token.
- **Chroma panics** (a push to an unknown state, `using` an unknown lexer, popping more states
  than the stack holds, `usingbygroup` with the wrong number of emitters) become `Error`
  tokens or an empty stack instead; none of Chroma's lexers reaches them on its test suite.
- **Chroma loops forever** where zero-width matches bring the lexer back to a configuration
  (state stack, Haxe's pre-processor stack) it already had at the same position, e.g.
  JSONata's catch-all `[a-zA-Z0-9_]*` before `é`, Jungle's `(?=\S)` push and default pop
  before `"` (the Go build hangs). The port undoes those matches and treats the position as
  matched by no rule (an `Error` character, or the newline reset); the same past 1,024
  configurations at one position (a stack growing without bound). The guard is not free but
  costs O(1) per zero-width match (it keeps the configurations, which share the persistent
  stacks' nodes, in a hash set); copying the stacks instead made deep inputs quadratic (Sass
  100 KB 2.2 s, Haxe `(`×12,000 `)`×12,000 28 s).
- **Regex limits**: `\p{…}` knows Unicode general categories (Go's `unicode.Categories`), not
  scripts or properties, and balancing groups (`(?<a-b>…)`) are refused; no Chroma lexer uses
  either. Categories come from Unicode 17 (`unicode-properties`), Go's from Unicode 15. A match
  gives up after 50 M steps where regexp2 gives up after 250 ms (Chroma then treats the rule as
  not matching).
- **Haxe's pre-processor stack** copies the state stack where Go keeps a slice that can alias it.
- **Fence attributes** are written in key order (markup's attribute `Map` is sorted; Go keeps
  source order), and the `class` value is escaped (Go writes it raw).

## Fixtures (`tests/data/`)

| file | content |
|---|---|
| `chroma-tokens.json.gz` | per docs item (`key` = FNV-1a of `lang\0code`): Chroma's lexer and class runs (`[class, chars]`) |
| `legacy-docs-html.json.gz` | per docs item (`key` = FNV-1a of `lang\0code\0options-JSON`): FNV-1a of the Go implementation's HTML with the docs config |
| `go-html.json` | the option matrix with the Go implementation's HTML (`golden.rs`) |
| `solarized-dark{,.omit-empty}.css` | `WriteCSS` of Chroma's HTML formatter (`--omitEmpty` = `WithClasses`) |
| `chroma-testdata.json.gz` | Chroma's lexer test suite: each `lexers/testdata` input, its lexer, the FNV-1a of Chroma's `*.expected` tokens (`type\0text\0…`) and the lexer Chroma's `Analyse` picks (`lexers.rs`) |
| `oracle/chroma.go.txt`, `oracle/chroma_testdata.py`, `oracle/export.go.txt` | the Chroma oracle (tokens, `Analyse`, registration order), the script that writes `chroma-testdata.json.gz`, and the exporter of the Go lexers' rules (Chroma XML, converted to `src/chroma/golexers/exported/`) |

The first four (and `src/data/chroma-lexers.tsv`) come from the Go implementation's
`markup/highlight` with Chroma v2.19.0, by the oracle at commit 44529028: in a worktree of it
(`git worktree add <dir> 44529028`; Go ≥ 1.24 with the module cache of its `go.mod`, offline),
from its root (the Cargo workspace is `rust/` there), then copy
`rust/crates/highlight/tests/data/*` (oracle/ aside) and
`rust/crates/highlight/src/data/chroma-lexers.tsv` to the same paths below `crates/highlight/`:

```sh
W=$(mktemp -d); mkdir $W/oracle; cp rust/crates/highlight/tests/data/oracle/main.go.txt $W/oracle/main.go
M=$(go list -m)   # the Go module's path
cp go.sum $W/oracle/ && cat > $W/oracle/go.mod <<EOF
module t25oracle
go 1.24
require $M v0.0.0
replace $M => $PWD
EOF
(cd $W/oracle && GOFLAGS=-mod=mod GOPROXY=off go build -o oracle .)
(cd rust && FUGO_HL_DUMP=$W/corpus.json cargo test -p ssg-highlight --test it dump_corpus)
python3 rust/crates/highlight/tests/data/oracle/regen.py $W/oracle/oracle $W
```

The Chroma oracle and the exporter need only Chroma (Go ≥ 1.22, the module cache):

```sh
C=$(go env GOMODCACHE)/github.com/alecthomas/chroma/v2@v2.19.0; D=crates/highlight/tests/data
O=$(mktemp -d); cp $D/oracle/chroma.go.txt $O/main.go; mkdir $O/export; cp $D/oracle/export.go.txt $O/export/main.go
printf 'module gochroma\ngo 1.22\nrequire (\n\tgithub.com/alecthomas/chroma/v2 v2.19.0\n\tgithub.com/dlclark/regexp2 v1.11.5\n)\n' > $O/go.mod
(cd $O && GOFLAGS=-mod=mod GOPROXY=off go build -o chroma . && GOFLAGS=-mod=mod GOPROXY=off go build -o export ./export)
python3 $D/oracle/chroma_testdata.py $C $O/cases.jsonl
$O/chroma analyse < $O/cases.jsonl > $O/analyse.txt
python3 $D/oracle/chroma_testdata.py $C $O/cases.jsonl $O/analyse.txt $D/chroma-testdata.json.gz
$O/export $O/golexers   # then the rename export.go.txt describes, and convert:
FUGO_HL_XML2RUST=$O/golexers:crates/highlight/src/chroma/golexers/exported \
  cargo test -p ssg-highlight --test it xml_to_rust
```

Study aids: `FUGO_HL_PAIRS=1` (most frequent class differences per lexer, `docs_corpus`),
`FUGO_HL_OURS=<file>` (our HTML per docs item), `FUGO_HL_TOKENS=<in>:<out>` (our tokens
per JSON line `{lang, code}`, in the format of the oracle's `tokens`, `write_tokens`),
`FUGO_HL_LEXERS=1` (the lexers in registration order, `print_lexers`), all with
`cargo test -p ssg-highlight --test it <test> -- --nocapture`.
