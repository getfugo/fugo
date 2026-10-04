//! The template API as JSON (`template_api_json`) and as Markdown (`template_api_markdown`).

use super::*;

pub(super) fn md_cell(s: &str) -> String {
    s.replace('|', "\\|")
}

pub(super) fn phase_str(p: PhaseAvail) -> &'static str {
    match p {
        PhaseAvail::Both => "both",
        PhaseAvail::Content => "content",
        PhaseAvail::Layout => "layout",
        PhaseAvail::Adapter => "adapter",
    }
}

/// `docs/data/template_api.json`, the template reference of the documentation site, generated
/// from the tables of this module (schema `ssg-template-api/1`): the groups, every name with its
/// signature, keyword arguments and phase, the render contexts, the hook fields, and the
/// Go-template constructs with their Tera replacements.
#[must_use]
pub fn template_api_json() -> String {
    let mut w = String::new();
    write_json(&mut w).expect("writing to a String cannot fail");
    w
}

/// A JSON string literal.
pub(super) fn js(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub(super) fn js_list<'a>(items: impl IntoIterator<Item = &'a str>) -> String {
    let items: Vec<String> = items.into_iter().map(js).collect();
    format!("[{}]", items.join(", "))
}

pub(super) const fn group_id(g: Group) -> &'static str {
    match g {
        Group::Logic => "logic",
        Group::Collections => "collections",
        Group::Pages => "pages",
        Group::Strings => "strings",
        Group::Encoding => "encoding",
        Group::Urls => "urls",
        Group::Dates => "dates",
        Group::Locale => "language",
        Group::Resources => "resources",
        Group::Images => "images",
        Group::Templates => "templates",
        Group::System => "system",
        Group::Tests => "tests",
    }
}

pub(super) fn write_json(w: &mut String) -> std::fmt::Result {
    writeln!(w, "{{\n\"schema\": \"ssg-template-api/1\",")?;
    writeln!(
        w,
        "\"about\": \"GENERATED from ssg_funcs::spec (crates/funcs/src/spec.rs); do not edit. Regenerate: INSTA_UPDATE=always cargo test -p ssg-testkit contract\","
    )?;
    let groups: Vec<String> = Group::ALL
        .iter()
        .map(|g| {
            format!(
                "{{\"id\": {}, \"title\": {}}}",
                js(group_id(*g)),
                js(g.title())
            )
        })
        .collect();
    writeln!(w, "\"groups\": [\n{}\n],", groups.join(",\n"))?;
    let funcs: Vec<String> = FUNCS
        .iter()
        .map(|f| {
            let kind = match f.kind {
                NameKind::Filter => "filter",
                NameKind::Function => "function",
                NameKind::Test => "test",
            };
            let source = match f.source {
                Source::Builtin => "builtin",
                Source::Contrib => "contrib",
                Source::Native => "native",
            };
            let kwargs: Vec<String> = f
                .kwargs
                .iter()
                .map(|k| {
                    format!(
                        "{{\"name\": {}, \"type\": {}, \"required\": {}}}",
                        js(k.name),
                        js(k.ty.as_str()),
                        k.required
                    )
                })
                .collect();
            let go = f.go.split(", ").filter(|g| !g.is_empty());
            format!(
                "{{\"name\": {}, \"kind\": {}, \"group\": {}, \"signature\": {}, \"kwargs\": [{}], \"rest_kwargs\": {}, \"phase\": {}, \"safe\": {}, \"site_bound\": {}, \"source\": {}, \"go\": {}, \"doc\": {}}}",
                js(f.name),
                js(kind),
                js(group_id(f.group)),
                js(&f.signature()),
                kwargs.join(", "),
                f.rest_kwargs,
                js(phase_str(f.phase)),
                f.safe,
                f.site_bound,
                js(source),
                js_list(go),
                js(f.doc)
            )
        })
        .collect();
    writeln!(w, "\"funcs\": [\n{}\n],", funcs.join(",\n"))?;
    let contexts: Vec<String> = CONTEXTS
        .iter()
        .map(|c| {
            format!(
                "{{\"title\": {}, \"names\": {}, \"note\": {}}}",
                js(c.title),
                js_list(c.names.iter().map(|n| n.name)),
                js(c.note)
            )
        })
        .collect();
    writeln!(w, "\"contexts\": [\n{}\n],", contexts.join(",\n"))?;
    let mut seen: Vec<&str> = Vec::new();
    let mut names = Vec::new();
    for n in CONTEXTS.iter().flat_map(|c| c.names) {
        if !seen.contains(&n.name) {
            seen.push(n.name);
            names.push(format!(
                "{{\"name\": {}, \"doc\": {}}}",
                js(n.name),
                js(n.doc)
            ));
        }
    }
    writeln!(w, "\"context_names\": [\n{}\n],", names.join(",\n"))?;
    let hooks: Vec<String> = HOOK_FIELDS
        .iter()
        .map(|(hook, fields)| {
            format!(
                "{{\"hook\": {}, \"fields\": {}}}",
                js(hook),
                js_list(fields.iter().copied())
            )
        })
        .collect();
    writeln!(w, "\"hook_fields\": [\n{}\n],", hooks.join(",\n"))?;
    let syntax: Vec<String> = SYNTAX
        .iter()
        .map(|r| format!("{{\"go\": {}, \"tera\": {}}}", js(r.go), js(r.tera)))
        .collect();
    writeln!(w, "\"syntax\": [\n{}\n],", syntax.join(",\n"))?;
    let rules: Vec<String> = CONVERSION_RULES
        .iter()
        .map(|(title, rules)| {
            format!(
                "{{\"title\": {}, \"rules\": {}}}",
                js(title),
                js_list(rules.iter().copied())
            )
        })
        .collect();
    writeln!(w, "\"conversion_rules\": [\n{}\n],", rules.join(",\n"))?;
    writeln!(
        w,
        "\"embedded_templates\": {},",
        js_list(EMBEDDED_TEMPLATES.iter().copied())
    )?;
    writeln!(
        w,
        "\"tera_facts\": {}\n}}",
        js_list(TERA_FACTS.iter().copied())
    )
}

/// `docs/rust-port/template-api.md`, generated from the tables of this module.
#[must_use]
pub fn template_api_markdown() -> String {
    let mut out = String::new();
    write_markdown(&mut out).expect("writing to a String cannot fail");
    out
}

pub(super) fn write_markdown(w: &mut String) -> std::fmt::Result {
    writeln!(w, "# Template API\n")?;
    writeln!(
        w,
        "<!-- GENERATED from ssg_funcs::spec (crates/funcs/src/spec.rs); do not edit.\n     \
         Regenerate: INSTA_UPDATE=always cargo test -p ssg-testkit contract -->\n"
    )?;
    writeln!(
        w,
        "Templates are Tera 2.4.0 ([REWRITE_PLAN.md](REWRITE_PLAN.md) §4). Kind codes: `bi` Tera \
         built-in, `tc` tera-contrib, `F` native filter, `fn` native function, `T` native \
         test; `(s)` site-bound (needs the site model or the render scope). Phase: `both`, or \
         the only phase the name works in. `=?` marks an optional kwarg, `…` any further kwargs.\n"
    )?;

    writeln!(
        w,
        "## Render contexts\n\n| Render | Top-level names |\n|---|---|"
    )?;
    for c in CONTEXTS {
        let mut names: Vec<String> = c.names.iter().map(|n| format!("`{}`", n.name)).collect();
        if !c.note.is_empty() {
            names.push(c.note.to_owned());
        }
        writeln!(w, "| {} | {} |", c.title, md_cell(&names.join(", ")))?;
    }
    writeln!(w, "\n| Name | Meaning |\n|---|---|")?;
    let mut seen: Vec<&str> = Vec::new();
    for n in CONTEXTS.iter().flat_map(|c| c.names) {
        if !seen.contains(&n.name) {
            seen.push(n.name);
            writeln!(w, "| `{}` | {} |", n.name, md_cell(n.doc))?;
        }
    }
    writeln!(
        w,
        "\nFlattened render-hook fields:\n\n| Hook | Fields |\n|---|---|"
    )?;
    for (hook, fields) in HOOK_FIELDS {
        let list: Vec<String> = fields.iter().map(|f| format!("`{f}`")).collect();
        writeln!(w, "| {hook} | {} |", list.join(" "))?;
    }

    writeln!(
        w,
        "\n## Syntax that replaces Go-template functions\n\n| Go | Tera |\n|---|---|"
    )?;
    for s in SYNTAX {
        writeln!(w, "| {} | {} |", md_cell(s.go), md_cell(s.tera))?;
    }

    for group in Group::ALL {
        writeln!(
            w,
            "\n## {}\n\n| Call | Kind | Phase | Safe | Go | Description |\n|---|---|---|---|---|---|",
            group.title()
        )?;
        for f in FUNCS.iter().filter(|f| f.group == group) {
            let go = if f.go.is_empty() {
                String::new()
            } else {
                format!("`{}`", f.go.replace(", ", "`, `"))
            };
            let types: Vec<String> = f
                .kwargs
                .iter()
                .map(|k| format!("{}: {}", k.name, k.ty.as_str()))
                .collect();
            let doc = if types.is_empty() {
                f.doc.to_owned()
            } else {
                format!("{} ({})", f.doc, types.join(", "))
            };
            writeln!(
                w,
                "| `{}` | {} | {} | {} | {} | {} |",
                md_cell(&f.signature()),
                f.code(),
                phase_str(f.phase),
                if f.safe { "yes" } else { "" },
                md_cell(&go),
                md_cell(&doc)
            )?;
        }
    }

    writeln!(w, "\n## Conversion rules\n")?;
    for (section, rules) in CONVERSION_RULES {
        writeln!(w, "**{section}**\n")?;
        for r in *rules {
            writeln!(w, "- {r}")?;
        }
        writeln!(w)?;
    }

    writeln!(
        w,
        "## Embedded templates\n\nLoaded under the fallback prefix `{EMBEDDED_PREFIX}`; a user or \
         theme template of the same name wins.\n"
    )?;
    for t in EMBEDDED_TEMPLATES {
        writeln!(w, "- `{t}`")?;
    }

    writeln!(
        w,
        "\n## Tera facts\n\nVerified against the tera 2.4.0 source and by the `tera_facts` tests of \
         ssg-testkit.\n"
    )?;
    for f in TERA_FACTS {
        writeln!(w, "- {f}")?;
    }
    Ok(())
}
