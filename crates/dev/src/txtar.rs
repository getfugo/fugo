//! txtar archives (Go's x/tools/txtar format): `-- name --` lines start files; text before the
//! first one is a comment.

use indexmap::IndexMap;

/// The files of a txtar archive, in order. Every non-empty file ends with a newline (the last
/// file of an archive without a final newline gets one).
#[must_use]
pub fn parse(text: &str) -> IndexMap<String, String> {
    let mut files = IndexMap::new();
    let mut name: Option<String> = None;
    let mut buf: Vec<&str> = Vec::new();
    for line in text.split('\n') {
        if line.starts_with("-- ") && line.ends_with(" --") && line.len() > 6 {
            if let Some(n) = name.take() {
                files.insert(n, buf.join("\n"));
            }
            name = Some(crate::py::strip(&line[3..line.len() - 3]).to_owned());
            buf.clear();
        } else if name.is_some() {
            buf.push(line);
        }
    }
    if let Some(n) = name {
        files.insert(n, buf.join("\n"));
    }
    files
        .into_iter()
        .map(|(k, v)| {
            let v = if v.is_empty() || v.ends_with('\n') {
                v
            } else {
                v + "\n"
            };
            (k, v)
        })
        .collect()
}
