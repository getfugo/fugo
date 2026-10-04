//! txtar archives (Go's x/tools/txtar format): `-- name --` lines start files; text before the
//! first one is a comment.

use indexmap::IndexMap;

/// The name of a file marker line (`-- name --`).
fn marker(line: &str) -> Option<&str> {
    let name = line.strip_prefix("-- ")?.strip_suffix(" --")?.trim();
    (!name.is_empty()).then_some(name)
}

/// The files of a txtar archive, in order. Every non-empty file ends with a newline (the last
/// file of an archive without a final newline gets one).
#[must_use]
pub fn parse(text: &str) -> IndexMap<String, String> {
    let mut files = IndexMap::new();
    let mut current: Option<(String, String)> = None;
    for line in text.split_inclusive('\n') {
        if let Some(name) = marker(line.trim_end_matches('\n')) {
            files.extend(current.take());
            current = Some((name.to_owned(), String::new()));
        } else if let Some((_, body)) = &mut current {
            body.push_str(line);
        }
    }
    files.extend(current);
    for body in files.values_mut() {
        if !body.is_empty() && !body.ends_with('\n') {
            body.push('\n');
        }
    }
    files
}
