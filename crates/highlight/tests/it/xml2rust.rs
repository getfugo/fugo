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

mod lexer;
mod xml;
use lexer::*;
use xml::*;

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
