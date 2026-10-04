//! Writing a lexer: its config, states, rules, mutators and emitters.

use super::*;

pub(super) fn lexer(el: &Element, stem: &str, out: &mut String) -> Result<(), String> {
    let _ = writeln!(
        out,
        "{}\nuse crate::chroma::defs::prelude::*;\n",
        header(&format!("Chroma's `{stem}.xml` lexer"))
    );
    comments(out, 0, &el.comments);
    let _ = writeln!(
        out,
        "#[rustfmt::skip]\npub(crate) static LEXER: LexerDef = LexerDef {{\n    file: {},",
        lit(stem)
    );
    let mut states = false;
    for child in &el.children {
        match child.name.as_str() {
            "config" => config(child, out)?,
            "rules" => {
                rules(child, out)?;
                states = true;
            }
            other => return Err(format!("unknown element <{other}> in <lexer>")),
        }
    }
    if !states {
        out.push_str("    states: &[],\n");
    }
    out.push_str("};\n");
    if !el.tail.is_empty() {
        out.push('\n');
        comments(out, 0, &el.tail);
    }
    Ok(())
}

/// A `ConfigDef` field: its name, its value (`Err`: a list's items), the comments of the
/// elements that make it and a trailing comment.
pub(super) type Field<'a> = (
    &'static str,
    Result<String, Vec<String>>,
    Vec<String>,
    Option<&'a String>,
);

/// `<config>` (Chroma's `Config`, read as `fastUnmarshalConfig` reads it) as a `ConfigDef`.
pub(super) fn config(el: &Element, out: &mut String) -> Result<(), String> {
    for c in &el.children {
        if !matches!(
            c.name.as_str(),
            "name"
                | "alias"
                | "filename"
                | "alias_filename"
                | "mime_type"
                | "case_insensitive"
                | "dot_all"
                | "not_multiline"
                | "ensure_nl"
                | "priority"
                | "analyse"
        ) {
            return Err(format!("unknown element <{}> in <config>", c.name));
        }
    }
    comments(out, 4, &el.comments);
    let _ = writeln!(
        out,
        "    config: ConfigDef {{{}",
        trailing(el.trailing.as_ref())
    );
    let named = |n: &str| -> Vec<&Element> { el.children.iter().filter(|c| c.name == n).collect() };
    // In Chroma's field order.
    let mut fields: Vec<Field<'_>> = Vec::new();
    // A repeated single-valued element: the last one counts.
    if let Some(name) = named("name").last() {
        fields.push((
            "name",
            Ok(lit(&name.text)),
            name.comments.clone(),
            name.trailing.as_ref(),
        ));
    }
    for (field, element) in [
        ("aliases", "alias"),
        ("filenames", "filename"),
        ("alias_filenames", "alias_filename"),
        ("mime_types", "mime_type"),
    ] {
        let elements = named(element);
        if elements.is_empty() {
            continue;
        }
        let items: Vec<String> = elements.iter().map(|e| lit(&e.text)).collect();
        let notes = elements.iter().flat_map(|e| e.comments.clone()).collect();
        let trail = elements.iter().rev().find_map(|e| e.trailing.as_ref());
        fields.push((field, Err(items), notes, trail));
    }
    for flag in ["case_insensitive", "dot_all", "not_multiline", "ensure_nl"] {
        if let Some(e) = named(flag).last()
            && parse_bool(&e.text)
        {
            fields.push((
                flag,
                Ok("true".into()),
                e.comments.clone(),
                e.trailing.as_ref(),
            ));
        }
    }
    if let Some(e) = named("priority").last() {
        let p: f32 = e.text.trim().parse().unwrap_or(0.0);
        if p != 0.0 {
            fields.push((
                "priority",
                Ok(format!("{p:?}")),
                e.comments.clone(),
                e.trailing.as_ref(),
            ));
        }
    }
    let n = fields.len();
    for (field, value, notes, trail) in &fields {
        comments(out, 8, notes);
        let line = match value {
            Ok(value) => format!("        {field}: {value},"),
            Err(items) => long_slice(&format!("{field}: "), items, 8, ","),
        };
        let _ = writeln!(out, "{line}{}", trailing(*trail));
    }
    let mut analysed = false;
    if let Some(a) = named("analyse").last() {
        analysed = true;
        comments(out, 8, &a.comments);
        let first = a.attr("first").is_some_and(parse_bool);
        let _ = writeln!(
            out,
            "        analyse: Some(AnalyseDef {{{}\n            first: {first},\n            regexes: &[",
            trailing(a.trailing.as_ref())
        );
        for r in &a.children {
            if r.name != "regex" {
                return Err(format!("unknown element <{}> in <analyse>", r.name));
            }
            let mut notes = Vec::new();
            r.all_comments(&mut notes);
            comments(out, 16, &notes);
            let score: f32 = r
                .attr("score")
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0.0);
            let _ = writeln!(
                out,
                "                ({}, {score:?}),",
                raw(r.attr("pattern").unwrap_or_default())
            );
        }
        comments(out, 16, &a.tail);
        out.push_str("            ],\n        }),\n");
    }
    comments(out, 8, &el.tail);
    // `..EMPTY` fills what is not given (every field given: nothing to fill).
    if n + usize::from(analysed) < 11 {
        out.push_str("        ..ConfigDef::EMPTY\n");
    }
    out.push_str("    },\n");
    Ok(())
}

/// `<rules>` as `states`, in the file's order (a state defined twice keeps its last
/// definition).
pub(super) fn rules(el: &Element, out: &mut String) -> Result<(), String> {
    let mut states: Vec<&Element> = Vec::new();
    for s in &el.children {
        if s.name != "state" {
            return Err(format!("unknown element <{}> in <rules>", s.name));
        }
        if let Some(r) = s.children.iter().find(|r| r.name != "rule") {
            return Err(format!("unknown element <{}> in <state>", r.name));
        }
        let name = s.attr("name").unwrap_or_default();
        states.retain(|p| p.attr("name").unwrap_or_default() != name);
        states.push(s);
    }
    comments(out, 4, &el.comments);
    let _ = writeln!(out, "    states: &[{}", trailing(el.trailing.as_ref()));
    for s in states {
        comments(out, 8, &s.comments);
        let name = lit(s.attr("name").unwrap_or_default());
        if s.children.is_empty() && s.tail.is_empty() {
            let _ = writeln!(
                out,
                "        ({name}, &[]),{}",
                trailing(s.trailing.as_ref())
            );
            continue;
        }
        let _ = writeln!(out, "        ({name}, &[");
        for r in &s.children {
            rule(r, out)?;
        }
        comments(out, 12, &s.tail);
        let _ = writeln!(out, "        ]),{}", trailing(s.trailing.as_ref()));
    }
    comments(out, 8, &el.tail);
    out.push_str("    ],\n");
    Ok(())
}

/// A `<rule>` as `rule(pattern)` and its emitter's and mutator's builders, or `include(state)`.
pub(super) fn rule(el: &Element, out: &mut String) -> Result<(), String> {
    let mut notes = el.comments.clone();
    if !el.text.trim().is_empty() {
        notes.push(el.text.trim().to_owned());
    }
    for c in &el.children {
        c.all_comments(&mut notes);
    }
    notes.extend(el.tail.iter().cloned());
    comments(out, 12, &notes);
    let pattern = el.attr("pattern").unwrap_or_default();
    let (mut emit, mut mutate) = (None, None);
    for c in &el.children {
        if let Some(m) = mutator(c)? {
            if mutate.replace(m).is_some() {
                return Err(format!("a second mutator <{}>", c.name));
            }
        } else if let Some(e) = emitter(c)? {
            if emit.replace(e).is_some() {
                return Err(format!("a second emitter <{}>", c.name));
            }
        } else {
            return Err(format!("unknown emitter <{}>", c.name));
        }
    }
    let expr = match (&emit, &mutate) {
        (
            None,
            Some(Op {
                method: "include",
                args,
                ..
            }),
        ) if pattern.is_empty() => {
            format!("include({args})")
        }
        _ => {
            let mut s = format!("rule({})", raw(pattern));
            for op in emit.iter().chain(&mutate) {
                let _ = write!(s, ".{}({})", op.method, op.args);
            }
            s
        }
    };
    let _ = writeln!(out, "            {expr},{}", trailing(el.trailing.as_ref()));
    Ok(())
}

/// An emitter or mutator: its builder (`.method(args)`) and its enum value (`E::…`, `M::…`).
pub(super) struct Op {
    pub(super) method: &'static str,
    pub(super) args: String,
    pub(super) value: String,
}

pub(super) fn op(method: &'static str, args: String, value: String) -> Op {
    Op {
        method,
        args,
        value,
    }
}

pub(super) fn states(e: &Element) -> String {
    slice(&e.attrs_named("state").map(lit).collect::<Vec<_>>())
}

pub(super) fn mutator(e: &Element) -> Result<Option<Op>, String> {
    Ok(Some(match e.name.as_str() {
        "include" => {
            let s = lit(e.attr("state").unwrap_or_default());
            op("include", s.clone(), format!("M::Include({s})"))
        }
        "combined" => op("combined", states(e), format!("M::Combined({})", states(e))),
        // No state: push the current state again.
        "push" => op("push", states(e), format!("M::Push({})", states(e))),
        "pop" => {
            let depth: usize = e
                .attr("depth")
                .and_then(|d| d.trim().parse().ok())
                .unwrap_or(0);
            op("pop", depth.to_string(), format!("M::Pop({depth})"))
        }
        "mutators" => {
            let mut items = Vec::new();
            for c in &e.children {
                let m = mutator(c)?.ok_or_else(|| format!("unknown mutator <{}>", c.name))?;
                items.push(m.value);
            }
            let list = slice(&items);
            op("mutators", list.clone(), format!("M::Multi({list})"))
        }
        "mutatorfunc" => {
            let name = lit(e.attr("name").unwrap_or_default());
            op("mutator_func", name.clone(), format!("M::Func({name})"))
        }
        _ => return Ok(None),
    }))
}

pub(super) fn emitter(e: &Element) -> Result<Option<Op>, String> {
    Ok(Some(match e.name.as_str() {
        "token" => {
            let t = token(e.attr("type").unwrap_or_default())?;
            op("token", t.clone(), format!("E::Token({t})"))
        }
        "bygroups" => {
            if e.children.iter().all(|c| c.name == "token") {
                let types = e
                    .children
                    .iter()
                    .map(|c| token(c.attr("type").unwrap_or_default()))
                    .collect::<Result<Vec<_>, _>>()?;
                let list = slice(&types);
                op("groups", list.clone(), format!("E::Groups({list})"))
            } else {
                let mut items = Vec::new();
                for c in &e.children {
                    if c.name == "nil" {
                        // A group that emits nothing (Go's `nil` emitter).
                        items.push("E::Nil".to_owned());
                        continue;
                    }
                    let em = emitter(c)?.ok_or_else(|| format!("unknown emitter <{}>", c.name))?;
                    items.push(em.value);
                }
                let list = slice(&items);
                op("bygroups", list.clone(), format!("E::ByGroups({list})"))
            }
        }
        "using" => {
            let l = lit(e.attr("lexer").unwrap_or_default());
            op("using", l.clone(), format!("E::Using({l})"))
        }
        "usingself" => {
            let s = lit(e.attr("state").unwrap_or_default());
            op("using_self", s.clone(), format!("E::UsingSelf({s})"))
        }
        "usingbygroup" => {
            let int = |n: &str| -> usize {
                e.child(n)
                    .and_then(|c| c.text.trim().parse().ok())
                    .unwrap_or(0)
            };
            let mut items = Vec::new();
            if let Some(list) = e.child("emitters") {
                for c in &list.children {
                    let em = emitter(c)?.ok_or_else(|| format!("unknown emitter <{}>", c.name))?;
                    items.push(em.value);
                }
            }
            let (name, code, list) = (int("sublexer_name_group"), int("code_group"), slice(&items));
            op(
                "using_by_group",
                format!("{name}, {code}, {list}"),
                format!(
                    "E::UsingByGroup {{ name_group: {name}, code_group: {code}, emitters: {list} }}"
                ),
            )
        }
        "emitfunc" => {
            let name = lit(e.attr("name").unwrap_or_default());
            op("emit_func", name.clone(), format!("E::Func({name})"))
        }
        _ => return Ok(None),
    }))
}
