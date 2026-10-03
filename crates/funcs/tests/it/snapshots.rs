//! One or more rendered examples for every pure `spec::FUNCS` entry, snapshotted.

use std::collections::BTreeMap;

use serde_json::json;
use ssg_funcs::{NOT_COMPILED, pure_specs};
use ssg_testkit::tera_value;
use tera::Context;

use crate::support::Harness;

/// (label, template). Labels start with the entry's name.
const EXAMPLES: &[(&str, &str)] = &[
    // logic
    ("log_error", "[{{ log_error(message='boom') }}]"),
    ("log_warn", "[{{ log_warn(message='careful', id='w-1') }}]"),
    ("max", "{{ max(values=[1, 7.5, 3]) }}"),
    ("min", "{{ min(values=[4, -2, 3]) }}"),
    // collections
    (
        "default_if_empty/empty-string",
        "{{ '' | default_if_empty(value='fallback') }}",
    ),
    (
        "default_if_empty/zero",
        "{{ 0 | default_if_empty(value=9) }}",
    ),
    (
        "default_if_empty/false-is-set",
        "{{ false | default_if_empty(value=true) }}",
    ),
    (
        "default_if_empty/undefined",
        "{{ nothing | default_if_empty(value='d') }}",
    ),
    ("get_path", "{{ data | get_path(path=['a', 'b', 1]) }}"),
    (
        "get_path/missing",
        "{{ data | get_path(path=['a', 'x']) is none }}",
    ),
    (
        "sort_keys",
        "{% for k, v in {'b': 1, 'a': 2} | sort_keys %}{{ k }}={{ v }} {% endfor %}",
    ),
    (
        "from_pairs",
        "{% set k = 'b' ~ 1 %}{{ [[k, 1], ['a', {'x': 2}], [3, true], [k, 4]] | from_pairs | jsonify }}",
    ),
    ("append", "{{ [1, 2] | append(value=3) | jsonify }}"),
    ("concat", "{{ [1, 2] | concat(with=[3, 4]) | jsonify }}"),
    (
        "merge",
        "{{ {'b': 1, 'n': {'x': 1} } | merge(with={'a': 2, 'n': {'y': 2} }) | jsonify }}",
    ),
    (
        "delimit",
        "{{ ['a', 'b', 'c'] | delimit(sep=', ', last=' and ') }}",
    ),
    (
        "complement",
        "{{ [1, 2, 3, 4] | complement(without=[2, 4]) | jsonify }}",
    ),
    ("union", "{{ [1, 2] | union(with=[2, 3]) | jsonify }}"),
    (
        "intersect",
        "{{ [1, 2, 3] | intersect(with=[3, 2, 9]) | jsonify }}",
    ),
    (
        "symdiff",
        "{{ [1, 2, 3] | symdiff(with=[3, 4]) | jsonify }}",
    ),
    (
        "sets/pages-by-id",
        "{% for p in pages | intersect(with=[pages[1]]) %}{{ p.title }},{% endfor %}",
    ),
    (
        "sort_by/title",
        "{% for p in pages | sort_by(attribute='title') %}{{ p.title }},{% endfor %}",
    ),
    (
        "sort_by/date-desc",
        "{% for p in pages | sort_by(attribute='date', reverse=true) %}{{ p.title }},{% endfor %}",
    ),
    (
        "sort_by/param",
        "{% for p in pages | sort_by(attribute='params.weight') %}{{ p.title }},{% endfor %}",
    ),
    (
        "sort_by/values",
        "{{ ['b', 'B', 'a', 'ä'] | sort_by(attribute='') | join(sep=',') }}",
    ),
    (
        "querify",
        "{{ querify(params={'q': 'a b', 'c': 'x&y', 'a': 1}) }}",
    ),
    // strings
    ("title_case", "{{ 'the lord of the rings' | title_case }}"),
    (
        "title_case/chicago",
        "{{ 'the lord of the rings' | title_case(style='chicago') }}",
    ),
    ("trim_chars", "{{ '--a-b--' | trim_chars(chars='-') }}"),
    (
        "trim_start_chars",
        "{{ 'xxa' | trim_start_chars(chars='x') }}",
    ),
    ("trim_end_chars", "{{ 'axx' | trim_end_chars(chars='x') }}"),
    (
        "strip_prefix",
        "{{ 'prefix-name' | strip_prefix(prefix='prefix-') }}",
    ),
    (
        "strip_suffix",
        "{{ 'name.md' | strip_suffix(suffix='.md') }}",
    ),
    (
        "regex_replace",
        "{{ 'a1b22' | regex_replace(pattern='([0-9]+)', rep='<$1>') }}",
    ),
    (
        "regex_find",
        "{{ 'a1b22c333' | regex_find(pattern='[0-9]+', limit=2) | jsonify }}",
    ),
    ("substr", "{{ 'ข้าวผัดกุ้ง' | substr(start=1, length=3) }}"),
    ("substr/negative", "{{ 'abcdef' | substr(start=-3) }}"),
    (
        "truncate_html",
        "{{ '<p>Some <em>long</em> text here</p>' | safe | truncate_html(length=12) }}",
    ),
    (
        "truncate_html/plain",
        "{{ 'Some long text' | truncate_html(length=8, ellipsis='...') }}",
    ),
    ("pad_start", "[{{ 'ab' | pad_start(width=5) }}]"),
    ("pad_end", "[{{ 'ab' | pad_end(width=5) }}]"),
    (
        "pluralize_word",
        "{{ 'category' | pluralize_word }} {{ 'person' | pluralize_word }}",
    ),
    ("singularize_word", "{{ 'categories' | singularize_word }}"),
    (
        "humanize",
        "{{ 'my-first_post' | humanize }} / {{ 3 | humanize }}",
    ),
    (
        "ordinalize",
        "{{ 22 | ordinalize }} {{ '13' | ordinalize }}",
    ),
    ("urlize", "{{ 'Hello World: Ünïcode' | urlize }}"),
    (
        "anchorize",
        "{{ 'Hello, World!' | anchorize }} {{ 'Crème Brûlée' | anchorize(style='github-ascii') }}",
    ),
    (
        "plainify",
        "{{ '<p>Hello <b>world</b></p><p>again</p>' | plainify }}",
    ),
    (
        "emojify",
        "{{ 'I :heart: Rust :no_such_emoji:' | emojify }}",
    ),
    (
        "format_number",
        "{{ 1234567.891 | format_number(precision=2) }}",
    ),
    (
        "format_number/th",
        "{% set lang = 'th' %}{{ 1234.5 | format_number(precision=1) }}",
    ),
    ("filesize_format", "{{ 123456789 | filesize_format }}"),
    // encoding
    ("html_escape", "{{ \"<a href='x'>&</a>\" | html_escape }}"),
    (
        "xml_escape",
        "{{ `<p class=\"x\">'&'</p>` | safe | xml_escape }}",
    ),
    (
        "html_unescape",
        "{{ '&lt;b&gt; &amp;amp; &#39; &eacute;' | html_unescape }}",
    ),
    (
        "jsonify",
        "{{ {'z': [1, 2.0, 1000000000000000000000.0, 0.0000001], 'a': '<>&', 'n': none} | jsonify }}",
    ),
    (
        "jsonify/indent",
        "{{ {'b': {'c': []}, 'a': 1 } | jsonify(indent='  ') }}",
    ),
    (
        "remarshal/yaml",
        "{{ '{\"b\": [1, 2], \"a\": {\"c\": \"x\"}}' | remarshal(format='yaml') }}",
    ),
    (
        "remarshal/toml",
        "{{ 'b: [1, 2]\na:\n  c: x\n' | remarshal(format='toml') }}",
    ),
    (
        "remarshal/toml_quoting_dates",
        "{{ `d = 2023-01-01\nz = 2023-01-01T07:32:00Z\nf = 2.0\nl = [1.0, 'x']\n[\"text/html\"]\ns = 'a.b'\nq = \"it's\"\n` | remarshal(format='toml') | safe }}",
    ),
    (
        "remarshal/yaml_quoting",
        "{{ `a = \":dir\"\nb = \"{{ x }}\"\nc = \"true\"\nd = \"\"\ne = \"\"\"l1\nl2\"\"\"\nk10 = 1\nk9 = 2\nz = 2023-01-01T07:32:00Z\nl = 2023-01-01\n` | remarshal(format='yaml') | safe }}",
    ),
    (
        "remarshal/json",
        "{{ 'a = 1\nb = \"x\"' | remarshal(format='json') }}",
    ),
    (
        "remarshal/json_dates_html",
        "{{ 'd = 2023-01-01\nt = 2023-01-01T07:32:00.5\nz = 2023-01-01T07:32:00+07:00\nh = \"<i>&</i>\"' | remarshal(format='json') | safe }}",
    ),
    ("urlencode", "{{ 'a b/c?d' | urlencode }}"),
    ("urlencode_strict", "{{ 'a b/c?d' | urlencode_strict }}"),
    ("urldecode", "{{ 'a%20b+c' | urldecode }}"),
    ("b64_encode", "{{ 'example' | b64_encode }}"),
    ("b64_decode", "{{ 'ZnVnbw==' | b64_decode }}"),
    ("md5", "{{ 'example' | md5 }}"),
    ("sha1", "{{ 'example' | sha1 }}"),
    ("sha256", "{{ 'example' | sha256 }}"),
    ("fnv32a", "{{ 'example' | fnv32a }}"),
    ("xxhash", "{{ 'example' | xxhash }}"),
    // urls
    (
        "parse_url",
        "{{ 'https://example.org/a%20b/?q=1#frag' | parse_url | jsonify }}",
    ),
    (
        "join_url",
        "{{ join_url(parts=['https://example.org/docs/', 'a', 'b/']) }}",
    ),
    ("path_ext", "{{ 'a/b/c.tar.gz' | path_ext }}"),
    ("path_base", "{{ 'a/b/c.md' | path_base }}"),
    ("path_base_name", "{{ 'a/b/c.md' | path_base_name }}"),
    ("path_dir", "{{ 'a/b/c.md' | path_dir }}"),
    ("path_clean", "{{ 'a//b/../c/.' | path_clean }}"),
    (
        "path_join",
        "{{ path_join(parts=['a', '', 'b/', '../c']) }}",
    ),
    // dates
    ("now", "{% set n = now() %}{{ n.rfc3339 }} {{ n.unix }}"),
    (
        "to_date",
        "{{ '2024-02-29T08:00:00+07:00' | to_date | jsonify }}",
    ),
    (
        "date",
        "{{ '2024-02-29T08:00:00+07:00' | date(format='%B %-d, %Y %H:%M') }}",
    ),
    (
        "date/style-th",
        "{% set lang = 'th' %}{{ now() | date(style='long') }}",
    ),
    (
        "date/locale",
        "{{ 1700000000 | date(format='%A %d %b %Y', locale='th') }}",
    ),
    ("date/none", "[{{ none | date(format='%Y') }}]"),
    // resources
    (
        "get_resource",
        "{{ resources | get_resource(name='IMG/A.JPG') | get(key='name') }}",
    ),
    (
        "find_resource",
        "{{ resources | find_resource(pattern='img/*.png') | get(key='name') }}",
    ),
    (
        "find_resources",
        "{% for r in resources | find_resources(pattern='**.{jpg,png}') %}{{ r.name }},{% endfor %}",
    ),
    (
        "by_type",
        "{% for r in resources | by_type(type='page') %}{{ r.name }},{% endfor %}",
    ),
    // templates
    (
        "arg/index",
        "{{ positional | arg(index=1) }} [{{ positional | arg(index=5) }}] {{ positional | arg(index=5, default='d') }}",
    ),
    (
        "arg/name",
        "{{ named | arg(name='src') }} {{ named | arg(index=0) is none }}",
    ),
    // system
    ("get_env", "[{{ get_env(name='FUGO_FUNCS_TEST_UNSET') }}]"),
    (
        "read_file",
        "{{ read_file(path='tests/fixtures/hello.txt') }}",
    ),
    (
        "file_exists",
        "{{ file_exists(path='Cargo.toml') }} {{ file_exists(path='nope.txt') }}",
    ),
    ("dump", "{{ {'b': [1], 'a': 'x'} | dump }}"),
    // tests
    (
        "matching",
        "{{ 'abc123' is matching(pat='^[a-z]+[0-9]+$') }}",
    ),
    (
        "version_at_least",
        "{{ '1.0.0-DEV' is version_at_least(version='0.148.2') }} {{ '1.0.0-DEV' is version_at_least(version='1.0.0') }} {{ '1.0.0' is version_at_least(version='1.0.0-DEV') }}",
    ),
];

fn context() -> Context {
    let mut ctx = Context::new();
    ctx.insert("lang", "en");
    ctx.insert_value("data", tera_value(&json!({"a": {"b": ["x", "y"]}})));
    let date = |rfc3339: &str, unix: i64| json!({"rfc3339": rfc3339, "unix": unix});
    ctx.insert_value(
        "pages",
        tera_value(&json!([
            {"id": 1, "kind": "page", "title": "Zebra", "date": date("2024-01-02T00:00:00+00:00", 1_704_153_600), "params": {"weight": 3}},
            {"id": 2, "kind": "page", "title": "apple", "date": date("2024-03-01T00:00:00+07:00", 1_709_226_000), "params": {"weight": 1}},
            {"id": 3, "kind": "page", "title": "Äpfel", "date": date("2023-12-31T00:00:00+00:00", 1_703_980_800), "params": {"weight": 2}},
        ])),
    );
    ctx.insert_value(
        "resources",
        tera_value(&json!([
            {"__rid": 1, "name": "img/a.jpg", "resource_type": "image"},
            {"__rid": 2, "name": "img/b.png", "resource_type": "image"},
            {"__rid": 3, "name": "notes/index.md", "resource_type": "page"},
        ])),
    );
    ctx.insert_value(
        "positional",
        tera_value(
            &json!({"name": "sc", "args": ["a", "b"], "params": {}, "is_named_params": false}),
        ),
    );
    ctx.insert_value(
        "named",
        tera_value(
            &json!({"name": "sc", "args": [], "params": {"src": "x.png"}, "is_named_params": true}),
        ),
    );
    ctx
}

#[test]
fn every_pure_entry_has_an_example() {
    let covered: Vec<&str> = EXAMPLES
        .iter()
        .map(|(label, _)| label.split('/').next().unwrap_or(label))
        .collect();
    let missing: Vec<&str> = pure_specs()
        .map(|f| f.name)
        // `to_math` and `diagrams_goat` have their own tests (features `math`, `goat`)
        .filter(|n| {
            !covered.contains(n)
                && !NOT_COMPILED.contains(n)
                && !["to_math", "diagrams_goat"].contains(n)
        })
        .collect();
    assert!(missing.is_empty(), "no example for {missing:?}");
}

#[test]
fn examples() {
    let h = Harness::new();
    let ctx = context();
    let out: BTreeMap<&str, String> = EXAMPLES
        .iter()
        .map(|(label, src)| {
            let rendered = h
                .render(src, &ctx)
                .unwrap_or_else(|e| panic!("{label}: {e}"));
            (*label, rendered)
        })
        .collect();
    ssg_testkit::snapshot::settings().bind(|| {
        insta::assert_yaml_snapshot!("examples", out);
    });
}
