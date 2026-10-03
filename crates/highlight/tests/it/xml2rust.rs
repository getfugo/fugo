//! The converter of Chroma's XML lexers and styles to the crate's Rust files (crate README,
//! "Lexer and style files"): `FUGO_HL_XML2RUST=<xml dir>:<rust dir>` writes
//! `<rust dir>/<name>.rs` for every `<xml dir>/<name>.xml` (all `<lexer>`s or all `<style>`s)
//! and `<rust dir>/mod.rs`, which lists them in Chroma's order (relative directories are
//! relative to the repository root; `rustfmt` formats `mod.rs`).
//!
//! The XML is read as Chroma reads it (Go's `encoding/xml`): attributes may repeat
//! (`<push state="a" state="b"/>`), references are resolved, attribute values keep their white
//! space. Comments, and the text Chroma ignores inside a rule, become Rust comments.

use std::fmt::Write as _;
use std::path::Path;

use quick_xml::events::{BytesStart, Event};
use ssg_highlight::TokenType;
use ssg_testkit::fixture::repo_dir;

#[test]
fn xml_to_rust() {
    let Ok(arg) = std::env::var("FUGO_HL_XML2RUST") else {
        return;
    };
    let (from, to) = arg.split_once(':').expect("<xml dir>:<rust dir>");
    let (from, to) = (repo_dir().join(from), repo_dir().join(to));
    let mut files: Vec<_> = std::fs::read_dir(&from)
        .unwrap_or_else(|e| panic!("{}: {e}", from.display()))
        .map(|e| e.expect("directory entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "xml"))
        .collect();
    // Chroma registers its embedded lexers in file name order (Go's `fs.Glob`, bytewise).
    files.sort();
    let mut kind = None;
    let mut modules = Vec::new();
    for path in &files {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("file name");
        let xml =
            std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let (k, rust) = convert(&xml, stem).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(
            kind.is_none_or(|kind| kind == k),
            "{}: lexers and styles in one directory",
            path.display()
        );
        kind = Some(k);
        let module = ident(stem).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(!modules.contains(&module), "two files make module {module}");
        let out = to.join(format!("{module}.rs"));
        std::fs::write(&out, rust).unwrap_or_else(|e| panic!("{}: {e}", out.display()));
        modules.push(module);
    }
    let Some(kind) = kind else {
        panic!("no XML file in {}", from.display());
    };
    let out = to.join("mod.rs");
    std::fs::write(&out, list(kind, &modules)).unwrap_or_else(|e| panic!("{}: {e}", out.display()));
    rustfmt(&out);
    println!("{} files converted", files.len());
}

/// Formats `path` (its `mod` lines, in rustfmt's order).
fn rustfmt(path: &Path) {
    let rustfmt = std::env::var("RUSTFMT").unwrap_or_else(|_| "rustfmt".to_owned());
    let status = std::process::Command::new(&rustfmt)
        .args(["--edition", "2024"])
        .arg(path)
        .status()
        .unwrap_or_else(|e| panic!("{rustfmt}: {e}"));
    assert!(status.success(), "{rustfmt} {}: {status}", path.display());
}

/// What a file defines.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Lexer,
    Style,
}

/// The module name of Chroma's file `stem` (`c#` → `csharp`, `c++` → `cpp`, `-` → `_`).
fn ident(stem: &str) -> Result<String, String> {
    let id = stem
        .replace("++", "pp")
        .replace('#', "sharp")
        .replace('-', "_");
    let ok = id.starts_with(|c: char| c.is_ascii_lowercase())
        && id
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
    if ok {
        Ok(id)
    } else {
        Err(format!("no module name for {stem:?}"))
    }
}

/// `mod.rs`: the modules and the list of their lexers or styles.
fn list(kind: Kind, modules: &[String]) -> String {
    let (what, ty, item, list) = match kind {
        Kind::Lexer => ("lexers", "crate::chroma::defs::LexerDef", "LEXER", "LEXERS"),
        Kind::Style => ("styles", "crate::style::StyleDef", "STYLE", "STYLES"),
    };
    let mut out = format!(
        "//! Chroma's {what}, converted from its XML (crate README, \"Lexer and style files\"), and\n\
         //! [`{list}`], in Chroma's order (its file names, bytewise). Written by\n\
         //! `tests/it/xml2rust.rs` with the files.\n\nuse {ty};\n\n"
    );
    for m in modules {
        let _ = writeln!(out, "mod {m};");
    }
    let _ = writeln!(
        out,
        "\n/// Every one of this directory, in Chroma's order.\n#[rustfmt::skip]\npub(crate) static {list}: &[&{}] = &[",
        ty.rsplit("::").next().expect("type")
    );
    for m in modules {
        let _ = writeln!(out, "    &{m}::{item},");
    }
    out.push_str("];\n");
    out
}

/// The Rust of a Chroma lexer or style file.
fn convert(xml: &str, stem: &str) -> Result<(Kind, String), String> {
    let doc = parse(xml)?;
    let [root] = doc.children.as_slice() else {
        return Err("not one root element".into());
    };
    check_text(root)?;
    let mut out = String::new();
    let kind = match root.name.as_str() {
        "lexer" => {
            lexer(root, stem, &mut out)?;
            Kind::Lexer
        }
        "style" => {
            style(root, stem, &mut out)?;
            Kind::Style
        }
        other => return Err(format!("unknown root element <{other}>")),
    };
    if !doc.tail.is_empty() {
        out.push('\n');
        comments(&mut out, 0, &doc.tail);
    }
    // Every comment is kept.
    let mut all = Vec::new();
    doc.all_comments(&mut all);
    for c in &all {
        let Some(line) = c.lines().map(str::trim).find(|l| !l.is_empty()) else {
            continue;
        };
        if !out.contains(line) {
            return Err(format!("comment lost: {line:?}"));
        }
    }
    Ok((kind, out))
}

/// Text is a value only in the elements holding one; in a rule Chroma ignores it (it becomes a
/// comment); anywhere else it would be lost.
fn check_text(el: &Element) -> Result<(), String> {
    const VALUES: &[&str] = &[
        "name",
        "alias",
        "filename",
        "alias_filename",
        "mime_type",
        "case_insensitive",
        "dot_all",
        "not_multiline",
        "ensure_nl",
        "priority",
        "sublexer_name_group",
        "code_group",
    ];
    if !el.text.trim().is_empty() && el.name != "rule" && !VALUES.contains(&el.name.as_str()) {
        return Err(format!("text in <{}>: {:?}", el.name, el.text.trim()));
    }
    el.children.iter().try_for_each(check_text)
}

// ── reading ──

/// An element of the document.
#[derive(Debug, Default)]
struct Element {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<Element>,
    text: String,
    /// The comments since the previous sibling (or the parent's start).
    comments: Vec<String>,
    /// A comment after the element, on the line it ends on.
    trailing: Option<String>,
    /// The comments after the last child.
    tail: Vec<String>,
}

impl Element {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    fn attrs_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a str> + 'a {
        self.attrs
            .iter()
            .filter(move |(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == name)
    }

    /// Its comments and those of its descendants, in document order (the text Chroma
    /// ignores in a rule included).
    fn all_comments(&self, out: &mut Vec<String>) {
        out.extend(self.comments.iter().cloned());
        if self.name == "rule" && !self.text.trim().is_empty() {
            out.push(self.text.trim().to_owned());
        }
        for c in &self.children {
            c.all_comments(out);
        }
        out.extend(self.tail.iter().cloned());
        out.extend(self.trailing.iter().cloned());
    }
}

/// Resolves `&name;` and `&#n;`.
fn resolve_ref(name: &str) -> Option<char> {
    match name {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let n = name.strip_prefix('#')?;
            let v = match n.strip_prefix('x') {
                Some(h) => u32::from_str_radix(h, 16).ok()?,
                None => n.parse().ok()?,
            };
            char::from_u32(v)
        }
    }
}

/// Resolves the references in an attribute value; white space stays as written (Go's
/// `encoding/xml` does not normalise it: a pattern may hold newlines).
fn unescape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        match after
            .find(';')
            .and_then(|end| resolve_ref(&after[..end]).map(|c| (c, end)))
        {
            Some((c, end)) => {
                out.push(c);
                rest = &after[end + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn start(e: &BytesStart<'_>) -> Result<Element, String> {
    let mut attrs = Vec::new();
    for a in e.attributes().with_checks(false) {
        let a = a.map_err(|e| e.to_string())?;
        attrs.push((a.key.local_name().as_ref().to_owned(), unescape(&a.value)));
    }
    Ok(Element {
        name: e.local_name().as_ref().to_owned(),
        attrs,
        ..Element::default()
    })
}

/// The document: a pseudo-element whose children are the root element.
fn parse(xml: &str) -> Result<Element, String> {
    let mut reader = quick_xml::Reader::from_str(xml);
    let mut stack = vec![Element::default()];
    // A child of the innermost open element ended and no newline followed yet: a comment now
    // is that child's trailing comment.
    let mut same_line = false;
    loop {
        let event = reader.read_event().map_err(|e| e.to_string())?;
        let top = stack.last_mut().ok_or("unbalanced end tag")?;
        match event {
            Event::Start(e) => {
                let mut el = start(&e)?;
                el.comments = std::mem::take(&mut top.tail);
                stack.push(el);
                same_line = false;
            }
            Event::Empty(e) => {
                let mut el = start(&e)?;
                el.comments = std::mem::take(&mut top.tail);
                top.children.push(el);
                same_line = true;
            }
            Event::End(_) => {
                let el = stack.pop().ok_or("unbalanced end tag")?;
                stack
                    .last_mut()
                    .ok_or("unbalanced end tag")?
                    .children
                    .push(el);
                same_line = true;
            }
            Event::Text(t) => {
                let text = t.xml10_content();
                if text.contains('\n') {
                    same_line = false;
                }
                top.text.push_str(&text);
            }
            Event::CData(t) => top.text.push_str(&t),
            Event::GeneralRef(r) => {
                let name = r.into_inner();
                match resolve_ref(&name) {
                    Some(c) => top.text.push(c),
                    None => {
                        top.text.push('&');
                        top.text.push_str(&name);
                        top.text.push(';');
                    }
                }
            }
            Event::Comment(c) => {
                let text = c.xml10_content().into_owned();
                match top.children.last_mut() {
                    Some(prev) if same_line && prev.trailing.is_none() && top.tail.is_empty() => {
                        prev.trailing = Some(text);
                    }
                    _ => top.tail.push(text),
                }
                same_line = false;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let [doc] = <[Element; 1]>::try_from(stack).map_err(|_| "unclosed element")?;
    Ok(doc)
}

/// Go's `strconv.ParseBool`.
fn parse_bool(s: &str) -> bool {
    matches!(s.trim(), "1" | "t" | "T" | "true" | "TRUE" | "True")
}

// ── writing ──

/// A Rust string literal of a pattern: raw (`r"…"`, `r#"…"#`, …), as written, when it can be.
fn raw(s: &str) -> String {
    if s.is_empty() {
        return "\"\"".into();
    }
    // A raw string holds no carriage return or other control character.
    if !s.chars().all(|c| c == '\t' || c == '\n' || !c.is_control()) {
        return format!("{s:?}");
    }
    let mut hashes = String::new();
    while s.contains(&format!("\"{hashes}")) {
        hashes.push('#');
    }
    format!("r{hashes}\"{s}\"{hashes}")
}

/// A Rust string literal.
fn lit(s: &str) -> String {
    format!("{s:?}")
}

/// `T::Variant` of Chroma's token type `name`.
fn token(name: &str) -> Result<String, String> {
    let t = TokenType::from_name(name).ok_or_else(|| format!("unknown token type {name:?}"))?;
    Ok(format!("T::{t:?}"))
}

/// `&[a, b]`.
fn slice(items: &[String]) -> String {
    format!("&[{}]", items.join(", "))
}

/// A slice after `prefix` (at `indent`): on one line when it fits in 100 columns, else one item
/// per line.
fn long_slice(prefix: &str, items: &[String], indent: usize, suffix: &str) -> String {
    let one = format!("{}{prefix}{}{suffix}", " ".repeat(indent), slice(items));
    if one.len() <= 100 || items.len() < 2 {
        return one;
    }
    let pad = " ".repeat(indent);
    let mut out = format!("{pad}{prefix}&[\n");
    for i in items {
        let _ = writeln!(out, "{pad}    {i},");
    }
    let _ = write!(out, "{pad}]{suffix}");
    out
}

/// Writes comments as `//` lines at `indent` (a multi-line comment dedented).
fn comments(out: &mut String, indent: usize, list: &[String]) {
    let pad = " ".repeat(indent);
    for c in list {
        let lines: Vec<&str> = c.lines().map(str::trim_end).collect();
        let start = lines.iter().position(|l| !l.trim().is_empty());
        let end = lines.iter().rposition(|l| !l.trim().is_empty());
        let (Some(start), Some(end)) = (start, end) else {
            continue;
        };
        let lines = &lines[start..=end];
        // The first line follows `<!--`; the others share an indent.
        let common = lines[1..]
            .iter()
            .filter(|l| !l.trim().is_empty())
            .map(|l| l.len() - l.trim_start().len())
            .min()
            .unwrap_or(0);
        for (i, l) in lines.iter().enumerate() {
            let l = if i == 0 {
                l.trim()
            } else {
                l.get(common..).unwrap_or_else(|| l.trim())
            };
            if l.is_empty() {
                let _ = writeln!(out, "{pad}//");
            } else {
                let _ = writeln!(out, "{pad}// {l}");
            }
        }
    }
}

/// The module doc of a converted file (on one line when it fits).
fn header(what: &str) -> String {
    let one = format!("//! {what}, converted to Rust (crate README, \"Lexer and style files\").\n");
    if one.len() <= 101 {
        one
    } else {
        format!("//! {what}, converted to Rust\n//! (crate README, \"Lexer and style files\").\n")
    }
}

/// ` // comment` for a trailing comment.
fn trailing(c: Option<&String>) -> String {
    c.map(|c| format!(" // {}", c.trim())).unwrap_or_default()
}

fn lexer(el: &Element, stem: &str, out: &mut String) -> Result<(), String> {
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
type Field<'a> = (
    &'static str,
    Result<String, Vec<String>>,
    Vec<String>,
    Option<&'a String>,
);

/// `<config>` (Chroma's `Config`, read as `fastUnmarshalConfig` reads it) as a `ConfigDef`.
fn config(el: &Element, out: &mut String) -> Result<(), String> {
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
fn rules(el: &Element, out: &mut String) -> Result<(), String> {
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
fn rule(el: &Element, out: &mut String) -> Result<(), String> {
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
struct Op {
    method: &'static str,
    args: String,
    value: String,
}

fn op(method: &'static str, args: String, value: String) -> Op {
    Op {
        method,
        args,
        value,
    }
}

fn states(e: &Element) -> String {
    slice(&e.attrs_named("state").map(lit).collect::<Vec<_>>())
}

fn mutator(e: &Element) -> Result<Option<Op>, String> {
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

fn emitter(e: &Element) -> Result<Option<Op>, String> {
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

/// `<style name="…"><entry type="…" style="…"/>…</style>` as a `StyleDef`, its entries in the
/// file's order (an entry given twice keeps its last value).
fn style(el: &Element, stem: &str, out: &mut String) -> Result<(), String> {
    let _ = writeln!(
        out,
        "{}\nuse crate::style::StyleDef;\nuse crate::token::TokenType as T;\n",
        header(&format!("Chroma's `{stem}.xml` style"))
    );
    comments(out, 0, &el.comments);
    let name = el.attr("name").ok_or("style without a name")?;
    let _ = writeln!(
        out,
        "#[rustfmt::skip]\npub(crate) static STYLE: StyleDef = StyleDef {{\n    name: {},\n    entries: &[",
        lit(name)
    );
    let mut entries: Vec<(&str, &str, &Element)> = Vec::new();
    for e in &el.children {
        if e.name != "entry" {
            return Err(format!("unknown element <{}> in <style>", e.name));
        }
        let ty = e.attr("type").ok_or("entry without a type")?;
        entries.retain(|(t, _, _)| *t != ty);
        entries.push((ty, e.attr("style").unwrap_or_default(), e));
    }
    for (ty, value, e) in entries {
        comments(out, 8, &e.comments);
        let _ = writeln!(
            out,
            "        ({}, {}),{}",
            token(ty)?,
            lit(value),
            trailing(e.trailing.as_ref())
        );
    }
    comments(out, 8, &el.tail);
    out.push_str("    ],\n};\n");
    Ok(())
}
