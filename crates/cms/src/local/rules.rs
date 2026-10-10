//! The rules the Worker shares with the editor (`web/assets/common.ts`) and the draft ids of
//! `web/assets/worker/api.ts`, as the local API needs them: clean paths, the areas a path is
//! in, commit messages and their trailers.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};
use ssg_base::glob::{self, Case, GlobOpts, Separator};

use crate::paths::{Area, DENY};

/// Whether `path` matches the whole of `pattern` (a glob of `ssg_base::glob`, case-sensitive,
/// `/`-separated, as the editor's `matchGlob`); a glob that does not compile matches nothing.
fn matches(pattern: &str, path: &str) -> bool {
    glob::compile(
        pattern,
        GlobOpts {
            case: Case::Sensitive,
            separator: Separator::Slash,
        },
    )
    .is_ok_and(|g| g.is_match(path))
}

/// A project-relative path of plain segments, or `None`: no leading `/`, `\`, control
/// characters, empty, `.` or `..` segments, and no hidden files or directories.
pub(super) fn clean_path(path: &str) -> Option<&str> {
    let units = path.encode_utf16().count();
    if units == 0 || units > 1024 {
        return None;
    }
    if path.starts_with('/')
        || path.contains('\\')
        || path.chars().any(|c| c < '\u{20}' || c == '\u{7f}')
    {
        return None;
    }
    path.split('/')
        .all(|seg| !seg.is_empty() && !seg.starts_with('.'))
        .then_some(path)
}

/// The lower-case extension of the last segment, without the dot (`""` when it has none).
fn extension_of(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name.rfind('.') {
        Some(dot) if dot > 0 => name[dot + 1..].to_lowercase(),
        _ => String::new(),
    }
}

/// The area that `path` is in (a clean path, not denied, with an extension the area allows).
/// The deny globs match the path as the build keys it (lower case, spaces as `-`).
pub(super) fn area_of<'a>(areas: &'a [Area], path: &str) -> Option<&'a Area> {
    let clean = clean_path(path)?;
    let key = clean.to_lowercase().replace(' ', "-");
    if DENY.iter().any(|g| matches(g, &key)) {
        return None;
    }
    let ext = extension_of(clean);
    areas
        .iter()
        .find(|a| a.ext.contains(&ext) && matches(&a.glob, clean))
}

/// Whether globs `edit` (the person's) let them write `path`.
pub(super) fn may_edit(areas: &[Area], edit: &[String], path: &str) -> bool {
    area_of(areas, path).is_some() && edit.iter().any(|g| matches(g, path))
}

/// The trailers of a commit message (`CMS-Entry: x` → `cms-entry` → `x`), from its last
/// paragraph; the first of a name wins.
pub(super) fn parse_trailers(message: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let text = message.trim();
    // Paragraphs end at a line of spaces and tabs only.
    let mut last = text;
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        at += line.len();
        if line.trim_matches([' ', '\t', '\n']).is_empty() && line.ends_with('\n') {
            last = &text[at..];
        }
    }
    for line in last.split('\n') {
        let line = line.trim();
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let mut chars = name.chars();
        let named = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '-');
        if named {
            out.entry(name.to_ascii_lowercase())
                .or_insert_with(|| value.trim_start_matches([' ', '\t']).to_owned());
        }
    }
    out
}

/// A commit message: the subject, then the trailers (empty values left out).
pub(super) fn commit_message(subject: &str, trailers: &[(&str, Option<&str>)]) -> String {
    let one = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    let lines: Vec<String> = trailers
        .iter()
        .filter_map(|(k, v)| {
            v.map(one)
                .filter(|v| !v.is_empty())
                .map(|v| format!("{k}: {v}"))
        })
        .collect();
    format!("{}\n\n{}\n", one(subject), lines.join("\n"))
}

/// The draft id of a page: its key as a slug, and 8 hex digits of its SHA-256 (keys that slug
/// alike stay apart), as the Worker's `draftId`.
pub(super) fn draft_id(entry: &str) -> String {
    let digest = Sha256::digest(entry.as_bytes());
    let hash: String = digest[..4].iter().map(|b| format!("{b:02x}")).collect();
    let key = if entry == "_index" {
        ""
    } else {
        entry.strip_suffix("/_index").unwrap_or(entry)
    };
    let mut slug = String::new();
    for c in key.to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            slug.push(c);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    let slug = slug[..slug.len().min(60)].trim_end_matches('-');
    format!("{}-{hash}", if slug.is_empty() { "home" } else { slug })
}

/// A draft id: this editor's ([`draft_id`]), or Decap CMS's (`<collection>/<slug>`). Its
/// segments start with a letter, digit, `_` or `-`, and hold those and `.`.
pub(super) fn is_draft_id(id: &str) -> bool {
    let word = |c: char| c.is_alphanumeric() || is_mark(c) || c == '_' || c == '-';
    id.encode_utf16().count() <= 200
        && id.split('/').all(|seg| {
            let mut chars = seg.chars();
            chars.next().is_some_and(word) && chars.all(|c| word(c) || c == '.')
        })
}

/// A combining mark (`\p{M}`, which the Worker's id pattern allows): the blocks of combining
/// diacritical marks, enough for the ids of pages in any script the editor sees.
fn is_mark(c: char) -> bool {
    matches!(c,
        '\u{0300}'..='\u{036f}'
        | '\u{0483}'..='\u{0489}'
        | '\u{0591}'..='\u{05bd}'
        | '\u{0610}'..='\u{061a}'
        | '\u{064b}'..='\u{065f}'
        | '\u{0900}'..='\u{0903}'
        | '\u{093a}'..='\u{094f}'
        | '\u{0e31}'
        | '\u{0e34}'..='\u{0e3a}'
        | '\u{0e47}'..='\u{0e4e}'
        | '\u{1ab0}'..='\u{1aff}'
        | '\u{1dc0}'..='\u{1dff}'
        | '\u{20d0}'..='\u{20ff}'
        | '\u{fe20}'..='\u{fe2f}')
}

/// The decoded size of base64 text, in bytes.
pub(super) fn base64_size(b64: &str) -> usize {
    let s: String = b64.chars().filter(|c| !c.is_whitespace()).collect();
    let pad = if s.ends_with("==") {
        2
    } else {
        usize::from(s.ends_with('='))
    };
    (s.len() * 3 / 4).saturating_sub(pad)
}

/// Whether `text` is base64 as the Worker accepts it: the alphabet and whitespace, then at most
/// two `=` (and whitespace).
pub(super) fn is_base64(text: &str) -> bool {
    let body = text.trim_end_matches(char::is_whitespace);
    let body = body
        .strip_suffix("==")
        .or_else(|| body.strip_suffix('='))
        .unwrap_or(body);
    body.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c.is_whitespace())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_plain_segments() {
        for ok in ["content/a.md", "content/สวัสดี/index.th.md", "a b/c"] {
            assert_eq!(clean_path(ok), Some(ok), "{ok}");
        }
        for bad in [
            "",
            "/content/a.md",
            "content//a.md",
            "content/./a.md",
            "content/../a.md",
            "content/.hidden.md",
            ".git/config",
            "a\\b",
            "a\u{1}b",
            "a/",
        ] {
            assert_eq!(clean_path(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn trailers_come_from_the_last_paragraph() {
        let t = parse_trailers(
            "Edit x\n\nBody: no\n \t\nCMS-Entry: posts/a\ncms-entry: later\nCMS-Title:  A  \n",
        );
        assert_eq!(t.get("cms-entry").map(String::as_str), Some("posts/a"));
        assert_eq!(t.get("cms-title").map(String::as_str), Some("A"));
        assert!(!t.contains_key("body"));
        assert!(parse_trailers("").is_empty());
    }

    #[test]
    fn messages_have_a_subject_and_trailers() {
        assert_eq!(
            commit_message(
                "Edit  posts/a:\n A",
                &[
                    ("CMS-Entry", Some("posts/a")),
                    ("CMS-Title", Some(" ")),
                    ("X", None)
                ]
            ),
            "Edit posts/a: A\n\nCMS-Entry: posts/a\n"
        );
    }

    #[test]
    fn base64_is_checked_and_sized() {
        assert!(is_base64("aGk=\n"));
        assert!(is_base64("aG k\n"));
        assert!(!is_base64("aGk=x"));
        assert!(!is_base64("a-b"));
        assert_eq!(base64_size("aGk="), 2);
        assert_eq!(base64_size("aGVsbG8="), 5);
    }

    #[test]
    fn draft_ids_are_the_workers() {
        let cases: Vec<(String, String)> =
            serde_json::from_str(include_str!("../../tests/draft-id-cases.json")).expect("cases");
        assert!(cases.len() >= 10);
        for (entry, id) in cases {
            assert_eq!(draft_id(&entry), id, "{entry}");
            assert!(is_draft_id(&id), "{id}");
        }
    }

    #[test]
    fn draft_ids_have_plain_segments() {
        for ok in ["posts-1a2b3c4d", "posts/my-post", "ไทย-1a2b3c4d", "a.b"] {
            assert!(is_draft_id(ok), "{ok}");
        }
        for bad in ["", ".a", "a/.b", "a//b", "a b", "a:b", "a~1", "a\u{0}"] {
            assert!(!is_draft_id(bad), "{bad:?}");
        }
    }
}
