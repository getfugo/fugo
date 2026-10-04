//! The template syntax: the rules of the conversion from Go templates, the embedded templates and
//! the facts of Tera.

/// A Go-template construct that became Tera syntax (an operator, literal, statement or view
/// field).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyntaxRule {
    pub go: &'static str,
    pub tera: &'static str,
}

pub(super) const fn syn(go: &'static str, tera: &'static str) -> SyntaxRule {
    SyntaxRule { go, tera }
}

/// The `op` rows of REWRITE_PLAN.md §4.6.
pub const SYNTAX: &[SyntaxRule] = &[
    syn(
        "`and` `or` `not` `eq` `ne` `lt` `le` `gt` `ge`",
        "`and` `or` `not` `==` `!=` `<` `<=` `>` `>=`; pages compare by `.id`, pagers by `.page_number`, dates by `.unix`",
    ),
    syn("`cond c a b`", "`a if c else b`"),
    syn(
        "`print`, `printf`",
        "`~`, plus the filters `pad_start`, `pad_end`, `round`, `format_number`, `jsonify`; a literal U+00A0 character for `%c` of 160; `'\"' ~ x ~ '\"'` for `%q` in attributes",
    ),
    syn(
        "`dict`, `slice`",
        "map and array literals `{\"k\": v}`, `[a, b]`; computed keys: `[[k, v]] | from_pairs`",
    ),
    syn("`index m k`", "`m[k]`, `m.k`"),
    syn(
        "`in`, `strings.Contains`",
        "`x in l`, `\"x\" in s`; pages `p.id in [q.id for q in l]`",
    ),
    syn("`isset m \"k\"`", "`\"k\" in m`, `is defined`"),
    syn(
        "`first N`, `last N`, `after N`",
        "`l[:N]`, `l[-N:]`, `l[N:]`",
    ),
    syn(
        "`where`",
        "`[p for p in pages if p.params.x == v]`; `in`/`intersect` via ids; `like` via `is matching(pat=)`",
    ),
    syn("`apply l \"float\" \".\"`", "`[x | float for x in l]`"),
    syn(
        "`newScratch`, `.Scratch.*`",
        "`{% set %}`, `{% set_global %}`, `merge`",
    ),
    syn(
        "`add` `sub` `mul` `div` `mod`",
        "`+ - * / %`; `//` for Go's integer `div`",
    ),
    syn("`.GetTerms \"tags\"`", "`page.terms.tags`"),
    syn(
        "`.Data.Singular/Plural/Term/Terms`",
        "`page.taxonomy.singular/plural/terms`, `page.term.term`",
    ),
    syn(
        "`.OutputFormats.Get \"rss\"`, `.AlternativeOutputFormats`, `.MediaType`",
        "`page.output_formats.rss`, `page.alternative_output_formats`, `f.media_type.type`",
    ),
    syn(
        "The site-info object's `Version` / `Environment` / `IsProduction` / `IsDevelopment` / `IsServer` / `Generator`",
        "`build.version` (fugo's version), `build.environment`, `build.is_production`, `build.is_development`, `build.is_server`, `build.generator`",
    ),
    syn(
        "`.Site.ServerPort`",
        "`site.server_port` (the base URL's port, 0 without one)",
    ),
    syn("`.Site.Config.Privacy.*`", "`site.config.privacy.*`"),
    syn(
        "`.Data.Integrity`, `.Width`, `.Height`",
        "`r.data.integrity`, `r.width`, `r.height`",
    ),
    syn(
        "`partial \"x\" .` (shares the context)",
        "`{% include \"_partials/x.html\" %}`",
    ),
    syn(
        "`partial \"x\" (dict …)` with a literal name",
        "a component defined in `_partials/` (`{% component x(page, sep=\"/\", @lang) %}`), called as `{{ <x page={page} /> }}`",
    ),
    syn("`debug.Timer`", "removed"),
    syn(
        "`css.TailwindCSS`",
        "removed: run Tailwind's standalone CLI next to fugo (`tailwindcss -i assets/css/in.css -o assets/css/site.css --watch`) and use its output as an asset",
    ),
    syn(
        "`babel`, `js.Babel`",
        "removed: `js_build` compiles TypeScript and JSX and lowers modern JavaScript for the browser targets",
    ),
    syn(
        "`postCSS`, `css.PostCSS`",
        "removed: `minify` adds vendor prefixes for the site's browserslist and minifies; `purge_css` purges per page",
    ),
];

/// REWRITE_PLAN.md §4.7, applied by hand when converting layouts (and by `ssg-migrate`).
pub const CONVERSION_RULES: &[(&str, &[&str])] = &[
    (
        "Names and paths",
        &[
            "Files use v0.146 names (`_partials/`, `_shortcodes/`, `_markup/`, `home|section|taxonomy|term|single|list|all|<layout>[.<lang>][.<fmt>].<ext>`, `baseof.html`). Include and extends literals are lower-case, new-style names.",
            "`.Title` → `page.title`; `.Site.X` and `site.X` → `site.x`.",
            "Params are lower-cased: `.Site.Params.HomeTitle` → `site.params.hometitle`. Data keys keep their case: `item.Name`.",
            "Reserved front-matter keys stay available in params: `where … \"Params.type\"` → `[p for p in l if p.params.type == \"snacks\"]`.",
        ],
    ),
    (
        "Missing values",
        &[
            "Printed params that may be missing → `page.params.x or \"\"` (printing an undefined value is an error).",
            "Nested optional lookups use `?.`: `page.params.a?.b`, `page.parent?.title or \"\"`.",
            "Go's `default` → `default_if_empty(value=)`. Do not use `or` for bools.",
            "`x == none` is false when `x` is undefined; use `is undefined`, `is none` or truthiness instead.",
        ],
    ),
    (
        "Comparisons",
        &[
            "`eq $p $currentSection` → `p.id == current_section.id`. Pagers compare by `page_number`, dates by `.unix`.",
            "Mixed int/string comparisons get an explicit `int` or `str`.",
        ],
    ),
    (
        "Control flow",
        &[
            "`{{ with X }}…{{ else with Y }}` → `{% if X %}{% set x = X %}…{% elif Y %}…`.",
            "`range $k, $v := m` → `{% for k, v in m %}`; template map literals need `| sort_keys` first.",
            "`range $i, $e := l` → `{% for e in l %}` with `loop.index0` / `loop.first`.",
            "`where` → a list comprehension with `if`; `apply` → a comprehension; `seq N` → `range(start=1, end=N+1)`.",
            "Variable reassignment inside a block → `set_global` (discarded inside includes).",
        ],
    ),
    (
        "Templates and partials",
        &[
            "`define`/`block` in children → `{% extends \"baseof.html\" %}` plus `{% block %}`; delete blocks the parent does not define.",
            "Partials → include, component or `partial()`; component calls pass arguments as `name={expr}`, `name=\"literal\"` or the shorthand `name`.",
            "`try` → `optional=true` on `get_remote` and `to_math` (none on an error), or a `none` check.",
        ],
    ),
    (
        "Formatting",
        &[
            "Go `printf` → `~`, `pad_start`/`pad_end`, `round`/`format_number`, `jsonify`.",
            "Tera 2.4 strings know only the escapes `\\n` `\\t` `\\r` `\\\"` `\\'` `\\/` `\\\\`: write other characters such as U+00A0 literally (`\"\\u{a0}\"` is an error), and double a regex backslash (`pattern=\"\\\\s+\"`).",
            "Go date layouts → strftime: `\"2006-01-02\"` → `\"%Y-%m-%d\"`, `\"Jan 2, 2006\"` → `\"%b %-d, %Y\"`. `.Format` stays English; `time.Format` and `dateFormat` localize names, so add `locale=lang`.",
        ],
    ),
    (
        "Removed Go-template idioms",
        &[
            "`range .Paginator.Pages` → `{% set pager = paginator() %}{% for p in pager.pages %}`; delete a second `.Paginate` that follows `.Paginator`.",
            "`{{ $noop := .WordCount }}` → delete.",
            "Another page's `.Content` inside a shortcode → `page_content(page=p)`.",
            "`.Scratch` / `newScratch` → `set`, `set_global`, `merge`; the page store only for cross-template flags.",
        ],
    ),
    (
        "Components",
        &["A component that calls site-bound functions passes `page=` or declares `@__nh`."],
    ),
    (
        "Escaping",
        &[
            "`html`/`htmlEscape` → `html_escape`. In `<script>`, use `jsonify | safe`. In query strings, use `urlencode`. `safeHTML` and the other `safe*` → `safe`.",
        ],
    ),
    (
        "Assets and i18n",
        &[
            "Assets used with `execute_as_template` are Tera templates: `{{ .api }}` → `{{ data.api }}`.",
            "i18n files stay in Go-template syntax, limited to `{{ . }}` and `{{ .Field }}`.",
        ],
    ),
];

/// The embedded templates this port provides (T32), by v0.146 name. They are loaded under
/// [`EMBEDDED_PREFIX`], a Tera fallback prefix, so user and theme templates of the same name win.
pub const EMBEDDED_TEMPLATES: &[&str] = &[
    "_markup/render-codeblock-goat.html",
    "_markup/render-image.html",
    "_markup/render-link.html",
    "_markup/render-table.html",
    "_partials/_funcs/get-page-images.html",
    "_partials/google_analytics.html",
    "_partials/opengraph.html",
    "_partials/pagination.html",
    "_partials/schema.html",
    "_partials/twitter_cards.html",
    "_shortcodes/details.html",
    "_shortcodes/figure.html",
    "_shortcodes/highlight.html",
    "_shortcodes/instagram.html",
    "_shortcodes/param.html",
    "_shortcodes/qr.html",
    "_shortcodes/ref.html",
    "_shortcodes/relref.html",
    "_shortcodes/vimeo.html",
    "_shortcodes/x.html",
    "_shortcodes/youtube.html",
    "alias.html",
    "robots.txt",
    "rss.xml",
    "sitemap.xml",
    "sitemapindex.xml",
];

/// The Tera fallback prefix of the embedded templates.
pub const EMBEDDED_PREFIX: &str = "_embedded/";

/// Tera 2.4.0 behaviours the template model relies on, verified by T02 against the source and by
/// the `tera_facts` tests of ssg-testkit.
pub const TERA_FACTS: &[&str] = &[
    "`__nh` is an ordinary identifier (a leading `_` is allowed) and `@__nh` a valid implicit component argument, resolved through the callers' scopes; a component that does not declare it cannot see it.",
    "Component call arguments are `name={expr}`, `name=\"literal\"` or the shorthand `name`; `name=expr` is a syntax error.",
    "`==` and `!=` never fail on an undefined final path segment: the value is undefined, and undefined equals only undefined (`x == none` is false). A missing non-final segment is an error, even inside `if`.",
    "Printing an undefined value is an error; `x or \"\"` and `default(value=)` are the fallbacks.",
    "`?.` (and `?[`) yield undefined when the receiver is undefined or none; the result must still not be printed bare.",
    "Built-in kwargs: `split(pat=)`, `nth(n=)`, `replace(from=, to=)`, `trim(pat=)`, `join(sep=)`, `round(method=, precision=)`, `truncate(length=, end=)`, `default(value=, boolean=)`, `get(key=, default=)`, `range(start=, end=, step_by=)`.",
    "Unknown filters, tests, functions, components and include targets are errors when templates are added; kwargs are checked only when called, so the contract test checks them statically.",
    "tera-contrib 0.3 names: `b64_encode`/`b64_decode`, `filesize_format`, `regex_replace(pattern=, rep=)`, `matching(pat=)`, `urlencode`, `urlencode_strict`, `date(format=, locale=, timezone=)`.",
];
