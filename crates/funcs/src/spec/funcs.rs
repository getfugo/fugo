//! The functions, filters and tests of the template API (`FUNCS`).

use super::*;

use ArgType as A;
use Group as G;
use PhaseAvail as P;

pub(super) const fn filter(
    group: Group,
    name: &'static str,
    go: &'static str,
    doc: &'static str,
) -> FuncSpec {
    FuncSpec::new(NameKind::Filter, group, name, go, doc)
}

pub(super) const fn func(
    group: Group,
    name: &'static str,
    go: &'static str,
    doc: &'static str,
) -> FuncSpec {
    FuncSpec::new(NameKind::Function, group, name, go, doc)
}

pub(super) const fn test(name: &'static str, go: &'static str, doc: &'static str) -> FuncSpec {
    FuncSpec::new(NameKind::Test, Group::Tests, name, go, doc)
}

pub(super) const fn req(name: &'static str, ty: ArgType) -> Kwarg {
    Kwarg {
        name,
        ty,
        required: true,
    }
}

pub(super) const fn opt(name: &'static str, ty: ArgType) -> Kwarg {
    Kwarg {
        name,
        ty,
        required: false,
    }
}

pub(super) const PAGE_OPT: Kwarg = opt("page", A::Page);

pub(super) const PAGE_REQ: Kwarg = req("page", A::Page);

pub(super) const IMAGE_ARGS: &[Kwarg] = &[
    opt("width", A::Int),
    opt("height", A::Int),
    opt("format", A::String),
    opt("quality", A::Int),
    opt("filter", A::String),
    opt("anchor", A::String),
    opt("spec", A::String),
];

pub(super) const PIPE_OPTIONS: &[Kwarg] = &[opt("options", A::Map)];

pub(super) const REF_ARGS: &[Kwarg] = &[
    req("path", A::String),
    opt("lang", A::String),
    opt("output_format", A::String),
    PAGE_OPT,
];

pub(super) const MENU_ARGS: &[Kwarg] = &[req("menu", A::String), req("entry", A::Map), PAGE_OPT];

/// Every filter, function and test (REWRITE_PLAN.md §4.6), including all Tera 2.4.0 built-ins.
/// Order: by [`Group`], then as a reader would look for them.
pub const FUNCS: &[FuncSpec] = &[
    // ── logic, math and errors ──
    func(G::Logic, "throw", "", "Aborts the render at once with `message`.").args(&[req("message", A::String)]).builtin(),
    func(G::Logic, "log_error", "errorf, erroridf", "Records an error with the template position; the build fails when it ends. Prints nothing.")
        .args(&[req("message", A::String)]),
    func(G::Logic, "log_warn", "warnf, warnidf", "Records a warning; `id` lets `ignoreLogs` suppress it. Prints nothing.")
        .args(&[req("message", A::String), opt("id", A::String)]),
    func(G::Logic, "max", "math.Max", "The largest of `values` (numbers).").args(&[req("values", A::Array)]),
    func(G::Logic, "min", "math.Min", "The smallest of `values` (numbers).").args(&[req("values", A::Array)]),
    filter(G::Logic, "abs", "math.Abs", "Absolute value.").builtin(),
    filter(G::Logic, "round", "math.Round, math.Ceil, math.Floor", "`method` is `common` (default), `ceil` or `floor`; `precision` digits after the point.")
        .args(&[opt("method", A::String), opt("precision", A::Int)]).builtin(),
    filter(G::Logic, "int", "int", "Converts to an integer; strings are parsed in `base` (default 10).").args(&[opt("base", A::Int)]).builtin(),
    filter(G::Logic, "float", "float", "Converts to a float.").builtin(),
    filter(G::Logic, "str", "string", "Converts to a string.").builtin(),
    // ── collections and maps ──
    filter(G::Collections, "length", "len", "Length of a string (characters), array or map.").builtin(),
    filter(G::Collections, "default", "", "`value` when the input is undefined (only then; `boolean=true` also replaces falsy values). Not Go's `default`.")
        .args(&[req("value", A::Any), opt("boolean", A::Bool)]).builtin(),
    filter(G::Collections, "default_if_empty", "default", "Go's `default`: `value` when the input is undefined, none, 0, \"\", or an empty array or map. `false` counts as set.")
        .args(&[req("value", A::Any)]),
    filter(G::Collections, "get", "index", "The map entry `key`, else `default`, else an error.").args(&[req("key", A::String), opt("default", A::Any)]).builtin(),
    filter(G::Collections, "get_path", "index m \"a\" \"b\"", "Walks `path` (keys and integer indices); none when a step is missing.").args(&[req("path", A::Array)]),
    filter(G::Collections, "first", "index l 0", "The first element, or none.").builtin(),
    filter(G::Collections, "last", "", "The last element, or none.").builtin(),
    filter(G::Collections, "nth", "index l n", "The element at index `n`, or none.").args(&[req("n", A::Int)]).builtin(),
    filter(G::Collections, "keys", "", "The keys of a map.").builtin(),
    filter(G::Collections, "values", "", "The values of a map.").builtin(),
    filter(G::Collections, "pairs", "", "`[key, value]` pairs of a map.").builtin(),
    filter(G::Collections, "sort_keys", "(range over a map)", "The map with its keys sorted; Go ranges maps in key order, Tera literals keep insertion order."),
    filter(G::Collections, "from_pairs", "dict $k $v (computed keys)", "A map from `[key, value]` pairs (the inverse of `pairs`), for keys a map literal cannot compute: `[[k, v]] | from_pairs`. Keys are stringified, a later pair wins, keys sorted."),
    filter(G::Collections, "append", "append", "The array with `value` appended.").args(&[req("value", A::Any)]),
    filter(G::Collections, "concat", "append l1 l2", "The array followed by the elements of `with`.").args(&[req("with", A::Array)]),
    filter(G::Collections, "merge", "merge", "Deep merge of two maps; `with` wins; keys sorted.").args(&[req("with", A::Map)]),
    filter(G::Collections, "join", "delimit", "Joins with `sep`.").args(&[opt("sep", A::String)]).builtin(),
    filter(G::Collections, "delimit", "delimit l sep last", "Joins with `sep`, and `last` before the final element.").args(&[req("sep", A::String), opt("last", A::String)]),
    filter(G::Collections, "reverse", ".Reverse", "Reversed array or string.").builtin(),
    filter(G::Collections, "unique", "uniq", "Removes duplicates, keeping the first.").builtin(),
    filter(G::Collections, "sort", "", "Tera's sort (by value, or by `attribute` path); not locale-aware. Use `sort_by` for Go's `sort`.")
        .args(&[opt("attribute", A::String)]).builtin(),
    filter(G::Collections, "group_by", "", "Tera's grouping by `attribute` path into a map.").args(&[req("attribute", A::String)]).builtin(),
    filter(G::Collections, "sort_by", "sort", "Sorts by `attribute` (a path such as `params.weight`; `\"\"` or `value`: the elements): collation of the render's `lang`, dates as instants, stable.")
        .args(&[req("attribute", A::String), opt("reverse", A::Bool)]),
    filter(G::Collections, "complement", "complement", "Elements not in `without` (pages compared by id).").args(&[req("without", A::Array)]),
    filter(G::Collections, "union", "union", "Elements of either array, first occurrence kept (pages by id).").args(&[req("with", A::Array)]),
    filter(G::Collections, "intersect", "intersect", "Elements present in both (pages by id).").args(&[req("with", A::Array)]),
    filter(G::Collections, "symdiff", "symdiff", "Elements present in exactly one (pages by id).").args(&[req("with", A::Array)]),
    func(G::Collections, "range", "seq", "Integers from `start` (default 0) to `end` (exclusive) by `step_by`; Go's `seq N` is `range(start=1, end=N+1)`.")
        .args(&[opt("start", A::Int), req("end", A::Int), opt("step_by", A::Int)]).builtin(),
    func(G::Collections, "querify", "querify", "A URL query string from `params`, keys sorted.").args(&[req("params", A::Map)]),
    // ── pages, taxonomies, menus, pagination ──
    func(G::Pages, "get_page", "site.GetPage, .GetPage", "The full value of the page at `path` (relative paths resolve against `page`), or none.")
        .args(&[req("path", A::String), opt("lang", A::String), PAGE_OPT]).site(),
    filter(G::Pages, "deref", "", "The full value (with relations) of a listed summary page.").site(),
    func(G::Pages, "get_terms", ".GetTerms", "The term links of `page` for `taxonomy`; `[]` when it is not a taxonomy.")
        .args(&[req("taxonomy", A::String), PAGE_OPT]).site(),
    func(G::Pages, "related", ".Related", "Pages related to `page` among `pages`, per the `related` config.")
        .args(&[req("pages", A::Array), PAGE_OPT, opt("indices", A::Array), opt("limit", A::Int)]).site(),
    func(G::Pages, "param", ".Param", "The page param `key` (a dotted path), else the site param.").args(&[req("key", A::String), PAGE_OPT]).site(),
    func(G::Pages, "paginator", ".Paginator", "The pager of the current (page, format) over its default list. Recorded: the first call wins.")
        .site().only(P::Layout),
    func(G::Pages, "paginate", ".Paginate", "The pager over `pages`. A re-call with another list or size is an error naming both positions.")
        .args(&[req("pages", A::Array), opt("size", A::Int)]).site().only(P::Layout),
    func(G::Pages, "store_set", ".Store.Set", "Sets `key` in the page store (content-phase writes are buffered per transaction); in a content adapter without `page=`, in the adapter's store, which its runs for every language share. Prints nothing.")
        .args(&[req("key", A::String), req("value", A::Any), PAGE_OPT]).site(),
    func(G::Pages, "store_get", ".Store.Get", "Reads `key` from the page store (in a content adapter without `page=`: the adapter's store), or none.").args(&[req("key", A::String), PAGE_OPT]).site(),
    func(G::Pages, "add_page", ".AddPage (content adapter)", "Adds the page `page` describes to the adapter's directory: `path` (relative, required but for `kind: home`), `kind` (default `page`), `title`, `content` (`{mediaType, value}`, default Markdown), `dates` (`{date, lastmod, publishDate, expiryDate}`), `params`, `build`, `cascade`, `outputs` and the other front matter fields (not `lang`, `content.markup`). A path added again replaces the earlier page. Prints nothing.")
        .args(&[req("page", A::Map)]).site().only(P::Adapter),
    func(G::Pages, "add_resource", ".AddResource (content adapter)", "Adds the page resource `resource` describes: `path` (relative to the adapter's directory, required), `content` (`{mediaType, value}`: a string, or a resource, which keeps its own URL), `name`, `title`, `params`. A path added again replaces the earlier resource. Prints nothing.")
        .args(&[req("resource", A::Map)]).site().only(P::Adapter),
    func(G::Pages, "enable_all_languages", ".EnableAllLanguages (content adapter)", "Runs the adapter for every language, not only its own (the runs share the adapter's store). Prints nothing.")
        .site().only(P::Adapter),
    func(G::Pages, "page_content", ".Content (another page, content phase)", "The rendered content of `page`; memoised and cycle-checked.")
        .args(&[PAGE_REQ]).site().safe(),
    func(G::Pages, "page_summary", ".Summary (content phase)", "The summary of `page`.").args(&[PAGE_REQ]).site().safe(),
    func(G::Pages, "page_plain", ".Plain (content phase)", "The content of `page` as plain text.").args(&[PAGE_REQ]).site(),
    func(G::Pages, "page_word_count", ".WordCount (content phase)", "The word count of `page`.").args(&[PAGE_REQ]).site(),
    func(G::Pages, "page_fragments", ".Fragments (content phase)", "`{headings, identifiers}` of `page`.").args(&[PAGE_REQ]).site(),
    func(G::Pages, "page_toc", ".TableOfContents (content phase)", "The table of contents of `page`.").args(&[PAGE_REQ]).site().safe(),
    func(G::Pages, "render_shortcodes", ".RenderShortcodes", "The source of `page` with its shortcodes expanded (placeholders renumbered).")
        .args(&[PAGE_REQ]).site().safe(),
    func(G::Pages, "is_menu_current", ".IsMenuCurrent", "Whether `entry` of `menu` points at `page`.").args(MENU_ARGS).site(),
    func(G::Pages, "has_menu_current", ".HasMenuCurrent", "Whether a child of `entry` of `menu` points at `page`.").args(MENU_ARGS).site(),
    filter(G::Pages, "by_title", ".ByTitle", "Pages by title (collation of the language).").site(),
    filter(G::Pages, "by_link_title", ".ByLinkTitle", "Pages by link title.").site(),
    filter(G::Pages, "by_date", ".ByDate", "Pages by date, oldest first.").site(),
    filter(G::Pages, "by_publish_date", ".ByPublishDate", "Pages by publish date.").site(),
    filter(G::Pages, "by_lastmod", ".ByLastmod", "Pages by last modification.").site(),
    filter(G::Pages, "by_weight", ".ByWeight", "Pages by Go's default page order (weight, date, link title, path).").site(),
    filter(G::Pages, "group_by_date", ".GroupByDate", "`[{key, pages}]` grouped by the date formatted with `format` (strftime), newest first (`order=\"asc\"`: oldest first); no date is Go's zero date (`0001`).")
        .args(&[req("format", A::String), opt("attribute", A::String), opt("order", A::String)]).site(),
    filter(G::Pages, "group_by_param", ".GroupByParam", "`[{key, pages}]` grouped by the page param `param`.").args(&[req("param", A::String)]).site(),
    filter(G::Pages, "by_count", ".ByCount", "Taxonomy terms by page count, then lower-cased name in Go's string order.").site(),
    filter(G::Pages, "alphabetical", ".Alphabetical", "Taxonomy terms by name (collation).").site(),
    // ── strings ──
    filter(G::Strings, "lower", "lower", "Lower case.").builtin(),
    filter(G::Strings, "upper", "upper", "Upper case.").builtin(),
    filter(G::Strings, "capitalize", "", "First character upper, the rest lower.").builtin(),
    filter(G::Strings, "title", "", "Tera's naive title case. Go's `title` is `title_case`.").builtin(),
    filter(G::Strings, "title_case", "title, strings.Title", "Title case in `style` (`ap`, `chicago`, `go`, `firstupper`, `none`; default: `titleCaseStyle`).")
        .args(&[opt("style", A::String)]),
    filter(G::Strings, "wordcount", "", "Tera's word count (whitespace split).").builtin(),
    filter(G::Strings, "trim", "strings.TrimSpace", "Trims whitespace, or the string `pat` repeatedly.").args(&[opt("pat", A::String)]).builtin(),
    filter(G::Strings, "trim_start", "", "Trims leading whitespace, or `pat`.").args(&[opt("pat", A::String)]).builtin(),
    filter(G::Strings, "trim_end", "", "Trims trailing whitespace, or `pat`.").args(&[opt("pat", A::String)]).builtin(),
    filter(G::Strings, "trim_chars", "trim, strings.Trim", "Trims any of the characters in `chars` from both ends.").args(&[req("chars", A::String)]),
    filter(G::Strings, "trim_start_chars", "strings.TrimLeft", "Trims any of `chars` from the start.").args(&[req("chars", A::String)]),
    filter(G::Strings, "trim_end_chars", "strings.TrimRight", "Trims any of `chars` from the end.").args(&[req("chars", A::String)]),
    filter(G::Strings, "strip_prefix", "strings.TrimPrefix", "Removes `prefix` once.").args(&[req("prefix", A::String)]),
    filter(G::Strings, "strip_suffix", "strings.TrimSuffix", "Removes `suffix` once.").args(&[req("suffix", A::String)]),
    filter(G::Strings, "replace", "replace, strings.Replace", "Replaces every `from` with `to`.").args(&[req("from", A::String), req("to", A::String)]).builtin(),
    filter(G::Strings, "split", "split", "Splits on `pat`.").args(&[req("pat", A::String)]).builtin(),
    filter(G::Strings, "regex_replace", "replaceRE", "Replaces matches of `pattern` with `rep` (`$1` groups).").args(&[req("pattern", A::String), req("rep", A::String)]).contrib(),
    filter(G::Strings, "regex_find", "findRE", "Matches of `pattern`, at most `limit`.").args(&[req("pattern", A::String), opt("limit", A::Int)]),
    filter(G::Strings, "substr", "substr", "`length` characters from `start` (negative counts from the end).").args(&[req("start", A::Int), opt("length", A::Int)]),
    filter(G::Strings, "truncate", "", "Tera's plain-text truncate to `length` characters plus `end`.").args(&[req("length", A::Int), opt("end", A::String)]).builtin(),
    filter(G::Strings, "truncate_html", "truncate", "Go's HTML-aware `truncate`: closes open tags, `ellipsis` default `…`. Keeps the input's safety.")
        .args(&[req("length", A::Int), opt("ellipsis", A::String)]),
    filter(G::Strings, "pad_start", "printf \"%5s\"", "Pads on the left with spaces to `width` characters.").args(&[req("width", A::Int)]),
    filter(G::Strings, "pad_end", "printf \"%-35s\"", "Pads on the right with spaces to `width` characters.").args(&[req("width", A::Int)]),
    filter(G::Strings, "indent", "", "Tera's indent.").args(&[opt("width", A::Int), opt("indentation", A::String), opt("first", A::Bool), opt("blank", A::Bool)]).builtin(),
    filter(G::Strings, "newlines_to_br", "", "Replaces line breaks with `<br>`.").builtin(),
    filter(G::Strings, "pluralize", "", "Tera's suffix pluralizer for a count (`singular`, `plural`). Go's `inflect.Pluralize` is `pluralize_word`.")
        .args(&[opt("singular", A::String), opt("plural", A::String)]).builtin(),
    filter(G::Strings, "pluralize_word", "inflect.Pluralize, pluralize", "English plural of a word."),
    filter(G::Strings, "singularize_word", "inflect.Singularize, singularize", "English singular of a word."),
    filter(G::Strings, "humanize", "humanize", "Go's `humanize` (`my-first-post` → `My first post`; numbers → ordinals)."),
    filter(G::Strings, "ordinalize", "humanize (numbers)", "`1` → `1st`."),
    filter(G::Strings, "urlize", "urlize", "The URL-safe path form of a string, as Go's `urlize` makes it."),
    filter(G::Strings, "anchorize", "anchorize", "An anchor id as Go generates it; `style` `github` (default), `github-ascii` or `blackfriday`.")
        .args(&[opt("style", A::String)]),
    filter(G::Strings, "plainify", "plainify", "Strips HTML tags."),
    filter(G::Strings, "emojify", "emojify", "Replaces `:shortcode:` emoji.").safe(),
    filter(G::Strings, "markdownify", "markdownify", "Renders Markdown with the current page's hooks; a single paragraph is unwrapped.").site().safe(),
    filter(G::Strings, "render_string", ".RenderString", "Renders Markdown with `page`'s hooks; `display=\"block\"` keeps the paragraph.")
        .args(&[opt("display", A::String), PAGE_OPT]).site().safe(),
    filter(G::Strings, "highlight", "highlight, transform.Highlight", "Syntax highlighting of the input as `lang` (Chroma classes, or inline styles per `noClasses`; needs the site's highlight configuration).")
        .args(&[req("lang", A::String), opt("options", A::Any)]).site().safe(),
    filter(G::Strings, "to_math", "transform.ToMath (+ try)", "LaTeX to MathML and/or HTML with KaTeX 0.16.22 and mhchem, as Go renders it (SHOULD; feature `math`). `options`: KaTeX's `output` (`mathml` default, `html`, `htmlAndMathml`), `displayMode`, `leqno`, `fleqn`, `errorColor`, `macros`, `minRuleThickness`, `throwOnError` (default true), `strict` (`error` default, `ignore`, `warn`: warnings). An error (a formula KaTeX rejects, invalid options) fails the render; with `optional=true` it is a warning (id `to_math`) and the result none.").args(&[opt("options", A::Map), opt("optional", A::Bool)]).safe(),
    func(G::Strings, "diagrams_goat", "diagrams.Goat", "`{inner (safe SVG), width, height, wrapped}` for the ASCII diagram `text` (SHOULD; feature `goat`).")
        .args(&[req("text", A::String)]),
    filter(G::Strings, "format_number", "lang.FormatNumber, printf \"%.1f\"", "The number with `precision` decimals in the format of the render's `lang`.")
        .args(&[opt("precision", A::Int)]),
    filter(G::Strings, "filesize_format", "", "Human file size (`binary` units by default).").args(&[opt("binary", A::Bool)]).contrib(),
    // ── encoding, escaping, hashing ──
    filter(G::Encoding, "safe", "safeHTML, safeHTMLAttr, safeURL, safeJS, safeCSS", "Marks the value safe.").builtin(),
    filter(G::Encoding, "escape", "", "Tera's escape (leaves safe input alone). Go's `html` is `html_escape`.").builtin(),
    filter(G::Encoding, "escape_html", "", "Tera's HTML escape of a string.").builtin(),
    filter(G::Encoding, "escape_xml", "", "Tera's XML escape (`&quot;`, `&apos;`; leaves safe input alone). Go's `transform.XMLEscape` is `xml_escape`.").builtin(),
    filter(G::Encoding, "xml_escape", "transform.XMLEscape", "Drops the characters XML forbids, then escapes `& < > \" '`, tab, newline and CR (`&#34; &#39; &#x9; &#xA; &#xD;`, Go's `xml.EscapeText`) even when the input is safe; the result is safe.").safe(),
    filter(G::Encoding, "html_escape", "html, htmlEscape, transform.HTMLEscape", "Escapes `& < > \" '` even when the input is safe; the result is safe.").safe(),
    filter(G::Encoding, "html_unescape", "htmlUnescape, transform.HTMLUnescape", "Decodes HTML entities."),
    filter(G::Encoding, "jsonify", "jsonify", "JSON with sorted keys, `<>&` escaped as `\\u003c…`; `indent` pretty-prints.").args(&[opt("indent", A::String)]).safe(),
    filter(G::Encoding, "unmarshal", "transform.Unmarshal", "Parses a string or resource as JSON, TOML, YAML, CSV or XML (`format` overrides detection); keys sorted.")
        .args(&[opt("format", A::String)]).site(),
    filter(G::Encoding, "remarshal", "transform.Remarshal", "Re-encodes data as `format` (`toml`, `yaml`, `json`).").args(&[req("format", A::String)]),
    filter(G::Encoding, "urlencode", "urlquery", "Percent-encodes for a URL path (keeps `/`).").contrib(),
    filter(G::Encoding, "urlencode_strict", "urlquery", "Percent-encodes every non-alphanumeric character.").contrib(),
    filter(G::Encoding, "urldecode", "urls.PathUnescape", "Decodes percent-encoding."),
    filter(G::Encoding, "b64_encode", "base64Encode", "Base64 (`url_safe`, `padded`).").args(&[opt("url_safe", A::Bool), opt("padded", A::Bool)]).contrib(),
    filter(G::Encoding, "b64_decode", "base64Decode", "Decodes base64.").args(&[opt("url_safe", A::Bool)]).contrib(),
    filter(G::Encoding, "md5", "md5, crypto.MD5", "Hex MD5."),
    filter(G::Encoding, "sha1", "sha1", "Hex SHA-1."),
    filter(G::Encoding, "sha256", "sha256", "Hex SHA-256."),
    filter(G::Encoding, "fnv32a", "hash.FNV32a", "FNV-1a 32-bit hash as an integer."),
    filter(G::Encoding, "xxhash", "hash.XxHash", "Hex xxHash64."),
    // ── URLs and paths ──
    filter(G::Urls, "abs_url", "absURL", "Absolute URL against `baseURL` (base path kept).").site(),
    filter(G::Urls, "rel_url", "relURL", "Root-relative URL with the base path.").site(),
    filter(G::Urls, "abs_lang_url", "absLangURL", "`abs_url` with the language prefix of `page`'s language.").args(&[PAGE_OPT]).site(),
    filter(G::Urls, "rel_lang_url", "relLangURL", "`rel_url` with the language prefix of `page`'s language.").args(&[PAGE_OPT]).site(),
    func(G::Urls, "ref", "ref", "The permalink of the page at `path`; unresolved per `refLinksErrorLevel`.").args(REF_ARGS).site(),
    func(G::Urls, "rel_ref", "relref", "The relative permalink of the page at `path`.").args(REF_ARGS).site(),
    filter(G::Urls, "parse_url", "urls.Parse", "`{scheme, host, path, fragment, query, is_absolute, string}`."),
    func(G::Urls, "join_url", "urls.JoinPath", "Joins URL `parts` with single slashes.").args(&[req("parts", A::Array)]),
    filter(G::Urls, "path_ext", "path.Ext", "Extension with the dot."),
    filter(G::Urls, "path_base", "path.Base", "Last element."),
    filter(G::Urls, "path_base_name", "path.BaseName", "Last element without extension."),
    filter(G::Urls, "path_dir", "path.Dir", "All but the last element."),
    filter(G::Urls, "path_clean", "path.Clean", "Lexically cleaned path."),
    func(G::Urls, "path_join", "path.Join", "Joins `parts` and cleans the result.").args(&[req("parts", A::Array)]),
    // ── dates ──
    func(G::Dates, "now", "now", "The build time (honours `--clock`) as a date value."),
    filter(G::Dates, "date", ".Format, time.Format, dateFormat", "Formats a date with strftime `format` or `style` (`short`, `medium`, `long`, `full`). A style is localized in `locale` (default: the render's `lang`; Thai uses the Gregorian calendar); a `format`'s month and weekday names are English (Go's `.Format`) unless `locale` is given (`time.Format`, `dateFormat`: `locale=lang`). Accepts a date value, a date string or Unix seconds; none prints nothing.")
        .args(&[opt("format", A::String), opt("style", A::String), opt("locale", A::String)]),
    filter(G::Dates, "to_date", "time.AsTime, time", "Parses a string or number into a date value (`{rfc3339, unix}`)."),
    // ── language ──
    func(G::Locale, "i18n", "i18n, T", "The translation of `key` in `page`'s language; `count` picks the plural form, `data` fills `{{ .Field }}`.")
        .args(&[req("key", A::String), opt("count", A::Number), opt("data", A::Any), PAGE_OPT]).site(),
    // ── resources ──
    func(G::Resources, "get_asset", "resources.Get", "The asset at `path` under `assets/`, or none.").args(&[req("path", A::String)]).site(),
    func(G::Resources, "find_asset", "resources.GetMatch", "The first asset matching the glob `pattern`, or none.").args(&[req("pattern", A::String)]).site(),
    func(G::Resources, "find_assets", "resources.Match", "All assets matching `pattern`.").args(&[req("pattern", A::String)]).site(),
    func(G::Resources, "get_remote", "resources.GetRemote, try", "A remote resource (`options`: headers, method, body, key). Errors propagate unless `optional=true` (then none and a warning).")
        .args(&[req("url", A::String), opt("options", A::Map), opt("optional", A::Bool)]).site(),
    func(G::Resources, "concat_assets", "resources.Concat", "Concatenates `items` into a resource at `target`.").args(&[req("target", A::String), req("items", A::Array)]).site(),
    func(G::Resources, "asset_from_string", "resources.FromString", "A resource at `target` with `content`.").args(&[req("target", A::String), req("content", A::String)]).site(),
    filter(G::Resources, "get_resource", ".Resources.Get", "The resource named `name` in a list (case-insensitive), or none.").args(&[req("name", A::String)]),
    filter(G::Resources, "find_resource", ".Resources.GetMatch", "The first resource matching the glob `pattern`, or none.").args(&[req("pattern", A::String)]),
    filter(G::Resources, "find_resources", ".Resources.Match", "All resources matching `pattern`.").args(&[req("pattern", A::String)]),
    filter(G::Resources, "by_type", ".Resources.ByType", "Resources whose type is `type` (`image`, `page`, …).").args(&[req("type", A::String)]),
    filter(G::Resources, "fingerprint", "fingerprint", "The resource renamed with its hash (`algo`: sha256 default, sha384, sha512, md5); sets `data.integrity`.")
        .args(&[opt("algo", A::String)]).site(),
    filter(G::Resources, "minify", "minify", "The minified resource.").site(),
    filter(G::Resources, "resource_content", ".Content (resource)", "The text of a resource; for a bundled content page, its rendered HTML (marked safe).").site(),
    filter(G::Resources, "publish", ".Publish", "Publishes the resource and returns it.").site(),
    filter(G::Resources, "to_css", "toCSS, css.Sass", "Sass/SCSS to CSS.").args(PIPE_OPTIONS).site(),
    filter(G::Resources, "js_build", "js.Build", "Bundles with rolldown.").args(PIPE_OPTIONS).site(),
    filter(G::Resources, "execute_as_template", "resources.ExecuteAsTemplate", "Renders the asset as a Tera template with `data`, published at `target`.")
        .args(&[req("target", A::String), opt("data", A::Any)]).site(),
    filter(G::Resources, "post_process", "resources.PostProcess", "Defers the resource's fields until all pages are rendered.").site(),
    filter(G::Resources, "purge_css", "PurgeCSS (PostCSS)", "A placeholder that each page's published output replaces with the rules of the CSS (a resource or string) that page uses: the tags, classes and ids of its elements, the words of its `<script>` elements. `safelist` names (or `/regex/`) count as used; `greedy` keeps any selector whose text contains the string or matches the `/regex/`; `blocklist` names drop their selectors; `content` resources or strings (scripts that add classes) count their words as used on every page; `variables=true` drops custom properties nothing kept references; `important=false` drops `!important`. Printed compactly for the project's browserslist targets. E.g. `<style>{{ css | purge_css(content=[js]) }}</style>`.")
        .args(&[opt("safelist", A::Array), opt("greedy", A::Array), opt("blocklist", A::Array), opt("content", A::Array), opt("variables", A::Bool), opt("important", A::Bool)]).site().safe(),
    // ── images ──
    filter(G::Images, "resize", ".Resize", "Resizes to `width` and/or `height` (or a `spec` string in Go's syntax, e.g. `\"600x400 webp q75\"`).").args(IMAGE_ARGS).site(),
    filter(G::Images, "fill", ".Fill", "Crops and resizes to fill `width`×`height` at `anchor`.").args(IMAGE_ARGS).site(),
    filter(G::Images, "fit", ".Fit", "Downscales to fit `width`×`height`.").args(IMAGE_ARGS).site(),
    filter(G::Images, "crop", ".Crop", "Crops to `width`×`height` at `anchor`.").args(IMAGE_ARGS).site(),
    filter(G::Images, "process", ".Process", "Any of the above per `spec` (or the typed kwargs).").args(IMAGE_ARGS).site(),
    filter(G::Images, "image_filter", "images.Filter, .Filter, images.Text, images.Dither", "Applies `filters`, a list of `{\"op\": …}` maps, one per Go `images.*` filter: `brightness`, `color_balance`, `colorize`, `contrast`, `gamma`, `gaussian_blur`, `grayscale`, `hue`, `invert`, `saturation`, `sepia`, `sigmoid`, `unsharp_mask`, `pixelate`, `opacity`, `padding`, `overlay` and `mask` (`image`: a resource), `auto_orient`, `text` (`text`, `color`, `size`, `x`, `y`, `alignx`, `aligny`, `linespacing`, `font`: a font resource), `dither` (`colors`, `method`, `serpentine`, `strength`), `process` (`spec`). E.g. `img | image_filter(filters=[{\"op\": \"text\", \"text\": page.title, \"size\": 40}, {\"op\": \"dither\"}])`.").args(&[req("filters", A::Array)]).site(),
    filter(G::Images, "exif", ".Exif", "EXIF data of an image, or none.").site(),
    filter(G::Images, "image_colors", ".Colors", "Not implemented yet: calling it is an error (Go's `.Colors` gives the dominant colours as hex strings).").site(),
    func(G::Images, "qr_code", "images.QR", "A PNG image resource of the QR code of `text`, with Go's bytes and name (`<target_dir>/qr_<hash>.png`): `level` low, medium (default), quartile or high; `scale` pixels per module (at least 2, default 4). E.g. `qr_code(text=page.permalink, target_dir=\"images/qr\")`.")
        .args(&[req("text", A::String), opt("level", A::String), opt("scale", A::Int), opt("target_dir", A::String)]).site(),
    // ── templates ──
    func(G::Templates, "super", "", "The parent block's content (inside `{% block %}` only).").builtin(),
    func(G::Templates, "partial", "partial (dynamic name or returned value)", "Renders `_partials/<name>` with the kwargs as top-level names; returns its `return_value` or the rendered string.")
        .args(&[req("name", A::String)]).rest().site().safe(),
    func(G::Templates, "partial_cached", "partialCached", "`partial` memoised on (`name`, `key`).").args(&[req("name", A::String), req("key", A::Any)]).rest().site().safe(),
    func(G::Templates, "return_value", "return", "Sets the value the enclosing `partial()` returns. Prints nothing.").args(&[req("value", A::Any)]).site(),
    func(G::Templates, "template_exists", "templates.Exists", "Whether a template called `name` exists.").args(&[req("name", A::String)]).site(),
    func(G::Templates, "defer", "templates.Defer", "A placeholder; `template` is rendered once per `key` with `data` after all pages.")
        .args(&[req("template", A::String), req("key", A::String), opt("data", A::Any)]).site().safe().only(P::Layout),
    filter(G::Templates, "arg", ".Get (shortcodes)", "A shortcode argument by position `index` or by `name`, else `default`: `shortcode | arg(index=0, default=\"\")`.")
        .args(&[opt("index", A::Int), opt("name", A::String), opt("default", A::Any)]).only(P::Content),
    // ── environment, files, debugging ──
    func(G::System, "get_env", "os.Getenv", "An environment variable (\"\" when unset): one the project's `.env` file defines (the process environment wins), or one `security.funcs.getenv` allows; any other name is an error.").args(&[req("name", A::String)]),
    func(G::System, "read_file", "os.ReadFile", "A file of the project (`security` rules apply).").args(&[req("path", A::String)]),
    func(G::System, "file_exists", "os.FileExists", "Whether a project file exists.").args(&[req("path", A::String)]),
    filter(G::System, "dump", "debug.Dump", "Pretty-printed JSON of any value."),
    // ── tests ──
    test("defined", "isset", "The value is defined.").builtin(),
    test("undefined", "", "The value is undefined.").builtin(),
    test("none", ".IsZero (dates)", "The value is none (zero dates serialise as none).").builtin(),
    test("string", "", "A string.").builtin(),
    test("number", "", "A number.").builtin(),
    test("integer", "", "An integer.").builtin(),
    test("float", "", "A float.").builtin(),
    test("bool", "", "A bool.").builtin(),
    test("map", "reflect.IsMap", "A map.").builtin(),
    test("array", "reflect.IsSlice", "An array.").builtin(),
    test("iterable", "", "An array, map or string.").builtin(),
    test("odd", "", "An odd number.").builtin(),
    test("even", "", "An even number.").builtin(),
    test("divisible_by", "", "Divisible by `divisor`.").args(&[req("divisor", A::Int)]).builtin(),
    test("starting_with", "strings.HasPrefix", "Starts with `pat`.").args(&[req("pat", A::String)]).builtin(),
    test("ending_with", "strings.HasSuffix", "Ends with `pat`.").args(&[req("pat", A::String)]).builtin(),
    test("containing", "in, strings.Contains", "Contains `pat` (substring, element or key).").args(&[req("pat", A::Any)]).builtin(),
    test("matching", "findRE (as a condition), where … \"like\"", "Matches the regex `pat`.").args(&[req("pat", A::String)]).contrib(),
    test("version_at_least", "", "A semver at least `version`; a `-DEV` build ranks below its release. Replaces Go-template comparisons of the site-info object's `Version`.").args(&[req("version", A::String)]),
];
