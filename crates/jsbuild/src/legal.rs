//! esbuild's `legalComments` modes `eof` (its default for bundles), `external` and `linked`: the
//! legal comments of a script (`/*!`, `//!`, `@license`, `@preserve`) move to its end, or to a
//! file of their own (`<script>.LEGAL.txt`), once each. A bundle of many modules of one package
//! otherwise repeats the package's licence header once per module.
//!
//! The comments are found by parsing the script (never by matching text, which strings and
//! regular expressions could fool). Only comments on lines of their own move; one inside a line
//! of code stays. A source map stays valid: the lines removed hold no code, so their (empty)
//! groups are dropped from the mappings, and the comments are appended after the last mapped
//! line. A comment on a line that does map somewhere stays where it is.

use oxc::allocator::Allocator;
use oxc::parser::Parser;
use oxc::span::SourceType;

/// `code` with its stand-alone legal comments moved to the end, and `map` (a source map's JSON)
/// in step (see [`extract`]).
pub(crate) fn move_to_end(code: String, map: Option<String>) -> (String, Option<String>) {
    let (mut out, map, comments) = extract(code, map);
    if comments.is_empty() {
        return (out, map);
    }
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    for c in &comments {
        out.push_str(c);
        out.push('\n');
    }
    (out, map)
}

/// The text of a `.LEGAL.txt` file of `comments`: each comment, then a blank line.
pub(crate) fn file_text(comments: &[String]) -> String {
    comments.join("\n\n") + "\n"
}

/// `code` without its stand-alone legal comments, `map` (a source map's JSON) with the mapping
/// groups of the removed lines dropped, and the comments (once each, in order). Code that does
/// not parse, or has no legal comment, comes back as it was, with no comments.
pub(crate) fn extract(code: String, map: Option<String>) -> (String, Option<String>, Vec<String>) {
    let allocator = Allocator::default();
    let parsed = Parser::new(&allocator, &code, SourceType::unambiguous()).parse();
    if parsed.diagnostics.has_errors() {
        return (code, map, Vec::new());
    }
    let mut mappings: Option<Vec<String>> = map.as_deref().and_then(|m| {
        let v: serde_json::Value = serde_json::from_str(m).ok()?;
        Some(
            v["mappings"]
                .as_str()?
                .split(';')
                .map(str::to_owned)
                .collect(),
        )
    });
    if map.is_some() && mappings.is_none() {
        return (code, map, Vec::new());
    }

    // The lines each comment occupies (start of its first line, end of its last line with the
    // line break), when nothing else is on them.
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(code.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    let line_of = |offset: usize| line_starts.partition_point(|&s| s <= offset) - 1;
    let mut cuts: Vec<(usize, usize)> = Vec::new();
    let mut removed_lines: Vec<usize> = Vec::new();
    let mut comments: Vec<String> = Vec::new();
    for c in parsed.program.comments.iter().filter(|c| c.is_legal()) {
        let (start, end) = (c.span.start as usize, c.span.end as usize);
        let first = line_of(start);
        let last = line_of(end.saturating_sub(1));
        let line_start = line_starts[first];
        let line_end = line_starts.get(last + 1).copied().unwrap_or(code.len());
        let alone =
            code[line_start..start].trim().is_empty() && code[end..line_end].trim().is_empty();
        let unmapped = mappings.as_ref().is_none_or(|groups| {
            (first..=last).all(|l| groups.get(l).is_none_or(String::is_empty))
        });
        if !alone || !unmapped || cuts.last().is_some_and(|&(_, e)| e > line_start) {
            continue;
        }
        cuts.push((line_start, line_end));
        removed_lines.extend(first..=last);
        let text = code[start..end].to_owned();
        if !comments.contains(&text) {
            comments.push(text);
        }
    }
    if cuts.is_empty() {
        return (code, map, Vec::new());
    }

    let mut out = String::with_capacity(code.len());
    let mut at = 0;
    for (s, e) in &cuts {
        out.push_str(&code[at..*s]);
        at = *e;
    }
    out.push_str(&code[at..]);

    let map = match (map, mappings.as_mut()) {
        (Some(map), Some(groups)) => {
            let kept: Vec<&str> = groups
                .iter()
                .enumerate()
                .filter(|(i, _)| !removed_lines.contains(i))
                .map(|(_, g)| g.as_str())
                .collect();
            let mut v: serde_json::Value = serde_json::from_str(&map).unwrap_or_default();
            v["mappings"] = serde_json::Value::String(kept.join(";"));
            Some(v.to_string())
        }
        (map, _) => map,
    };
    (out, map, comments)
}

#[cfg(test)]
mod tests {
    use super::{extract, file_text, move_to_end};

    #[test]
    fn moves_stand_alone_legal_comments_to_the_end_once() {
        let code = "/*! A v1 | MIT */\nconst a=1;\n/*!\n * A v1 | MIT\n */\nconst b=2;\n/*! A v1 | MIT */\nexport{a,b};\n";
        let (out, _) = move_to_end(code.to_owned(), None);
        assert_eq!(
            out,
            "const a=1;\nconst b=2;\nexport{a,b};\n/*! A v1 | MIT */\n/*!\n * A v1 | MIT\n */\n"
        );
    }

    #[test]
    fn leaves_comments_inside_code_strings_and_other_comments() {
        let code = "const s=\"/*! not a comment */\";\nf(/*! inline */1);\n/* plain */\n// @license MIT\nexport{s};\n";
        let (out, _) = move_to_end(code.to_owned(), None);
        assert_eq!(
            out,
            "const s=\"/*! not a comment */\";\nf(/*! inline */1);\n/* plain */\nexport{s};\n// @license MIT\n"
        );
    }

    #[test]
    fn keeps_the_source_map_in_step() {
        let code = "/*! L */\nconst a=1;\n/*! L */\nconst b=2;\n";
        let map = r#"{"version":3,"sources":["a.js"],"mappings":";AAAA;;AACA","names":[]}"#;
        let (out, map) = move_to_end(code.to_owned(), Some(map.to_owned()));
        assert_eq!(out, "const a=1;\nconst b=2;\n/*! L */\n");
        let v: serde_json::Value = serde_json::from_str(&map.expect("map")).expect("json");
        assert_eq!(v["mappings"], "AAAA;AACA");
    }

    #[test]
    fn a_comment_on_a_mapped_line_stays() {
        let code = "/*! L */\nconst a=1;\n";
        let map = r#"{"version":3,"sources":["a.js"],"mappings":"AAAA;AACA","names":[]}"#;
        let (out, map) = move_to_end(code.to_owned(), Some(map.to_owned()));
        assert_eq!(out, code);
        assert!(map.expect("map").contains("AAAA;AACA"));
    }

    #[test]
    fn extracts_for_a_file_of_their_own() {
        let code = "/*! L1 */\nconst a=1;\n/*! L1 */\n//! L2\nexport{a};";
        let (out, _, comments) = extract(code.to_owned(), None);
        assert_eq!(out, "const a=1;\nexport{a};");
        assert_eq!(comments, ["/*! L1 */", "//! L2"]);
        assert_eq!(file_text(&comments), "/*! L1 */\n\n//! L2\n");
        let (same, _, none) = extract("const a=1;\n".to_owned(), None);
        assert_eq!((same.as_str(), none.len()), ("const a=1;\n", 0));
    }

    #[test]
    fn code_that_does_not_parse_is_left_alone() {
        let code = "/*! L */\nconst = ;\n";
        assert_eq!(move_to_end(code.to_owned(), None).0, code);
    }
}
