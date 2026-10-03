//! The conversion lints (REWRITE_PLAN.md §4.7, §4.8), read from the tokens of each template's
//! tags (`ssg_funcs::scan`, the contract test's tokenizer).
//!
//! | id | severity | what |
//! |---|---|---|
//! | `kwarg` | error | a `FUNCS` call with an unknown kwarg or without a required one |
//! | `legacy-literal` | error | an `include`/`extends` literal with a pre-v0.146 path or upper case |
//! | `unknown-partial` | error | `partial(name="<literal>")` naming no partial |
//! | `call-attribute` | error | attribute access after a call or a parenthesised expression |
//! | `nested-close` | error | `}}` inside an expression of a `{{ … }}` tag |
//! | `content-field` | error | a content field of `page` in a shortcode or render hook |
//! | `component-scope` | warning | a site-bound call in a component without `page=` or `@__nh` |
//! | `partial-component` | warning | `partial(name="<literal>")` with arguments (a component) |
//! | `map-order` | warning | a map literal ranged without `sort_keys` |
//! | `none-compare` | warning | `== none` / `!= none` (false for an undefined value) |

use std::collections::BTreeSet;

use ssg_base::diag::Diagnostic;
use ssg_funcs::scan::{self, Tag, TagKind, Tok};
use ssg_funcs::spec::FUNCS;
use ssg_layouts::{LayoutStore, TemplateRole};

use super::CheckFile;

/// Lints whose position is also a Tera syntax error: the Tera snippet becomes their note.
pub(crate) const EXPLAINS_SYNTAX: &[&str] = &["call-attribute", "nested-close"];

/// The content fields of a page value, absent from the Meta generation that shortcodes and
/// render hooks see (`ssg_view::ContentView`).
const CONTENT_FIELDS: &[&str] = &[
    "content",
    "summary",
    "truncated",
    "plain",
    "word_count",
    "fuzzy_word_count",
    "reading_time",
    "table_of_contents",
    "fragments",
    "len",
];

pub(crate) fn run(files: &[CheckFile], store: &LayoutStore, diags: &mut Vec<Diagnostic>) {
    for f in files {
        let tags = scan::tags(&f.source);
        kwargs(f, diags);
        let mut map_vars = BTreeSet::new();
        let mut component: Option<(String, bool)> = None;
        for tag in &tags {
            match tag.keyword() {
                Some("component") => component = component_def(tag),
                Some("endcomponent") => component = None,
                _ => {}
            }
            legacy_literal(f, tag, diags);
            tera_limits(f, tag, diags);
            content_fields(f, tag, diags);
            map_order(f, tag, &mut map_vars, diags);
            partial_calls(f, tag, store, diags);
            if let Some((name, has_nh)) = &component
                && !has_nh
            {
                component_scope(f, tag, name, diags);
            }
        }
    }
}

fn kwargs(f: &CheckFile, diags: &mut Vec<Diagnostic>) {
    for (call, message) in scan::kwarg_errors(&f.source) {
        diags.push(
            Diagnostic::error(message)
                .with_id("kwarg")
                .at(f.at(call.offset)),
        );
    }
}

/// The first token after the tag's keyword.
fn after_keyword<'a, 's>(tag: &'a Tag<'s>) -> &'a [(Tok<'s>, usize)] {
    tag.first().map_or(&[], |i| &tag.toks[i + 1..])
}

/// `include`/`extends` literals: v0.146 names, lower case.
fn legacy_literal(f: &CheckFile, tag: &Tag<'_>, diags: &mut Vec<Diagnostic>) {
    if !matches!(tag.keyword(), Some("include" | "extends")) {
        return;
    }
    let Some((Tok::Str(lit), offset)) = after_keyword(tag).first() else {
        return;
    };
    let lower = lit.to_lowercase();
    let fix = [
        ("_default/", ""),
        ("partials/", "_partials/"),
        ("shortcodes/", "_shortcodes/"),
    ]
    .iter()
    .find_map(|(old, new)| lower.strip_prefix(old).map(|rest| format!("{new}{rest}")))
    .unwrap_or(lower);
    if fix == *lit {
        return;
    }
    diags.push(
        Diagnostic::error(format!(
            "`{lit}` is not a v0.146 template name; use \"{fix}\" (include and extends literals \
             are lower-case, new-style names)"
        ))
        .with_id("legacy-literal")
        .at(f.at(*offset)),
    );
}

/// The Tera 2.4 syntax limits found in T35 (REWRITE_PLAN.md §4.7).
fn tera_limits(f: &CheckFile, tag: &Tag<'_>, diags: &mut Vec<Diagnostic>) {
    for w in tag.toks.windows(2) {
        if let [(Tok::Punct(")"), _), (Tok::Punct(p @ ("." | "?.")), offset)] = w {
            diags.push(
                Diagnostic::error(format!(
                    "Tera cannot use `{p}` after a call or a parenthesised expression; bind the \
                     value first (`{{% set x = f(…) %}}{{{{ x.y }}}}`) or use `| get(key=…)` / \
                     `| get_path(path=[…])`"
                ))
                .with_id("call-attribute")
                .at(f.at(*offset)),
            );
        }
    }
    for offset in &tag.inner_closes {
        diags.push(
            Diagnostic::error(
                "`}}` inside an expression ends the `{{ … }}` tag in Tera; write nested map \
                 literals with a space: `{\"a\": {\"b\": 1} }`",
            )
            .with_id("nested-close")
            .at(f.at(*offset)),
        );
    }
    for (i, (t, offset)) in tag.toks.iter().enumerate() {
        let Tok::Punct(op @ ("==" | "!=")) = t else {
            continue;
        };
        let none = |j: Option<usize>| {
            j.and_then(|j| tag.toks.get(j))
                .is_some_and(|(t, _)| *t == Tok::Ident("none"))
        };
        if none(i.checked_sub(1)) || none(Some(i + 1)) {
            diags.push(
                Diagnostic::warning(format!(
                    "`{op} none` is {} when the value is undefined; use `is undefined`, \
                     `is none` or truthiness",
                    if *op == "==" { "false" } else { "true" }
                ))
                .with_id("none-compare")
                .at(f.at(*offset)),
            );
        }
    }
}

/// Content fields of the page in shortcodes and render hooks (their page is the Meta
/// generation's).
fn content_fields(f: &CheckFile, tag: &Tag<'_>, diags: &mut Vec<Diagnostic>) {
    if !matches!(
        f.role,
        TemplateRole::Shortcode { .. } | TemplateRole::Hook { .. }
    ) {
        return;
    }
    for (i, w) in tag.toks.windows(3).enumerate() {
        let [
            (Tok::Ident(p @ ("page" | "page_inner")), _),
            (Tok::Punct("." | "?."), _),
            (Tok::Ident(field), offset),
        ] = w
        else {
            continue;
        };
        let after_dot = i > 0 && matches!(tag.toks[i - 1].0, Tok::Punct("." | "?."));
        if after_dot || !CONTENT_FIELDS.contains(field) {
            continue;
        }
        diags.push(
            Diagnostic::error(format!(
                "`{p}.{field}` is a content field: shortcodes and render hooks see the page \
                 without content (use `page_content(page=…)` for another page's content)"
            ))
            .with_id("content-field")
            .at(f.at(*offset)),
        );
    }
}

/// `{% for k, v in <map literal> %}` (or a variable set to one) without `sort_keys`.
fn map_order(
    f: &CheckFile,
    tag: &Tag<'_>,
    map_vars: &mut BTreeSet<String>,
    diags: &mut Vec<Diagnostic>,
) {
    let rest = after_keyword(tag);
    match tag.keyword() {
        Some("set" | "set_global") => {
            if let [(Tok::Ident(name), _), (Tok::Punct("="), _), value, ..] = rest {
                if value.0 == Tok::Punct("{") {
                    map_vars.insert((*name).to_owned());
                } else {
                    map_vars.remove(*name);
                }
            }
        }
        Some("for") => {
            let [
                (Tok::Ident(_), _),
                (Tok::Punct(","), _),
                (Tok::Ident(_), _),
                (Tok::Ident("in"), _),
                (source, offset),
                tail @ ..,
            ] = rest
            else {
                return;
            };
            let literal = match source {
                Tok::Punct("{") => true,
                Tok::Ident(v) => {
                    map_vars.contains(*v)
                        && !matches!(tail.first(), Some((Tok::Punct("." | "?." | "["), _)))
                }
                _ => false,
            };
            if literal && !tag.toks.iter().any(|(t, _)| *t == Tok::Ident("sort_keys")) {
                diags.push(
                    Diagnostic::warning(
                        "a template map literal is ranged in insertion order; add `| sort_keys` \
                         for the sorted order of Go templates",
                    )
                    .with_id("map-order")
                    .at(f.at(*offset)),
                );
            }
        }
        _ => {}
    }
}

/// The literal of kwarg `name` of the call whose `(` is token `open`.
fn literal_kwarg<'s>(toks: &[(Tok<'s>, usize)], open: usize, name: &str) -> Option<&'s str> {
    let close = scan::matching_close(toks, open);
    toks[open..=close].windows(3).find_map(|w| match w {
        [(Tok::Ident(k), _), (Tok::Punct("="), _), (Tok::Str(v), _)] if *k == name => Some(*v),
        _ => None,
    })
}

/// `partial(name="<literal>")`: the partial exists; with arguments it should be a component.
fn partial_calls(f: &CheckFile, tag: &Tag<'_>, store: &LayoutStore, diags: &mut Vec<Diagnostic>) {
    for call in scan::calls_in_tag(&f.source, tag) {
        if !matches!(call.name.as_str(), "partial" | "partial_cached") {
            continue;
        }
        let Some(open) = tag
            .toks
            .iter()
            .position(|(_, o)| *o == call.offset)
            .map(|i| i + 1)
        else {
            continue;
        };
        let Some(name) = literal_kwarg(&tag.toks, open, "name") else {
            continue;
        };
        let Some(target) = store.partial(name).and_then(|n| store.get(&n)) else {
            diags.push(
                Diagnostic::error(format!("no partial `{name}` (`_partials/{name}`)"))
                    .with_id("unknown-partial")
                    .at(f.at(call.offset)),
            );
            continue;
        };
        let args: Vec<&str> = call
            .kwargs
            .iter()
            .map(String::as_str)
            .filter(|k| !matches!(*k, "name" | "key"))
            .collect();
        if args.is_empty() || target.source().contains("return_value") {
            continue;
        }
        let hint = if args == ["page"] {
            format!("`{{% include \"_partials/{name}\" %}}` shares the caller's context")
        } else {
            format!(
                "define it as a component in `_partials/` and call it as `{{{{ <{} … /> }}}}`",
                name.trim_end_matches(".html").replace(['/', '-'], "_")
            )
        };
        diags.push(
            Diagnostic::warning(format!(
                "`{}(name=\"{name}\")` with arguments ({}) and a literal name: {hint}; `partial()` \
                 is for dynamic names and returned values",
                call.name,
                args.join(", ")
            ))
            .with_id("partial-component")
            .at(f.at(call.offset)),
        );
    }
}

/// A component definition: its name and whether it declares `@__nh`.
fn component_def(tag: &Tag<'_>) -> Option<(String, bool)> {
    let Some((Tok::Ident(name), _)) = after_keyword(tag).first() else {
        return None;
    };
    let has_nh = tag
        .toks
        .windows(2)
        .any(|w| matches!(w, [(Tok::Punct("@"), _), (Tok::Ident("__nh"), _)]));
    Some(((*name).to_owned(), has_nh))
}

/// Site-bound calls inside a component that declares no `@__nh` need `page=`.
fn component_scope(f: &CheckFile, tag: &Tag<'_>, component: &str, diags: &mut Vec<Diagnostic>) {
    if tag.kind == TagKind::Stmt && tag.keyword() == Some("component") {
        return;
    }
    for call in scan::calls_in_tag(&f.source, tag) {
        let Some(spec) = FUNCS
            .iter()
            .find(|s| s.name == call.name && s.kind == call.kind && s.site_bound)
        else {
            continue;
        };
        if call.kwargs.iter().any(|k| k == "page") {
            continue;
        }
        let fix = if spec.kwarg("page").is_some() {
            "pass `page=` or declare `@__nh` in its arguments"
        } else {
            "declare `@__nh` in its arguments"
        };
        diags.push(
            Diagnostic::warning(format!(
                "`{}` is site-bound and component `{component}` cannot see the render scope: {fix}",
                call.name
            ))
            .with_id("component-scope")
            .at(f.at(call.offset)),
        );
    }
}
