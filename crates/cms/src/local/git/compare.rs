//! Comparisons: what a draft changed since its merge base with the branch, file by file.

use super::*;

/// A file a comparison found changed.
pub(in super::super) struct CompareFile {
    pub(in super::super) path: String,
    /// GitHub's names: `added`, `removed`, `modified`, `renamed`, `copied`, `changed`.
    pub(in super::super) status: &'static str,
    /// The blob at the head.
    pub(in super::super) sha: String,
    pub(in super::super) previous: Option<String>,
    pub(in super::super) patch: Option<String>,
}

impl CompareFile {
    /// Its path, and its previous path when it moved.
    pub(in super::super) fn paths(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.path.as_str()).chain(self.previous.as_deref())
    }
}

/// What a head changed since its merge base with a base.
pub(in super::super) struct Comparison {
    pub(in super::super) merge_base: String,
    pub(in super::super) files: Vec<CompareFile>,
    /// Oldest first.
    pub(in super::super) commits: Vec<Commit>,
}

impl Git {
    /// What `head` changed since its merge base with `base`, with each file's patch when
    /// `patches`.
    pub(in super::super) fn compare(
        &self,
        base: &str,
        head: &str,
        patches: bool,
    ) -> Result<Comparison, HttpError> {
        let merge_base = self.text(&["merge-base", base, head])?;
        let range = format!("{merge_base}..{head}");
        let shas: Vec<String> = self
            .text(&["rev-list", "--reverse", "--topo-order", &range])?
            .lines()
            .map(str::to_owned)
            .collect();
        let commits = self.commits(&shas)?;
        let raw = self.run(
            &[
                "diff",
                "--raw",
                "-z",
                "-M",
                "--no-abbrev",
                "--no-ext-diff",
                &merge_base,
                head,
            ],
            None,
        )?;
        let mut files = Vec::new();
        for change in diff_raw(&raw) {
            let patch = if patches {
                let mut args = vec![
                    "diff",
                    "-M",
                    "--no-color",
                    "--no-ext-diff",
                    "--no-textconv",
                    &merge_base,
                    head,
                    "--",
                    &change.src,
                ];
                if let Some(dst) = &change.dst {
                    args.push(dst);
                }
                hunks(&String::from_utf8_lossy(&self.run(&args, None)?))
            } else {
                None
            };
            let (path, previous) = match &change.dst {
                Some(dst) => (self.project_path(dst), Some(self.project_path(&change.src))),
                None => (self.project_path(&change.src), None),
            };
            files.push(CompareFile {
                path,
                status: change.status,
                sha: change.sha,
                previous,
                patch,
            });
        }
        Ok(Comparison {
            merge_base,
            files,
            commits,
        })
    }
}

/// A change of `git diff --raw -z`.
struct RawChange {
    status: &'static str,
    /// The blob at the head.
    sha: String,
    /// The path (a move's or copy's source).
    src: String,
    /// A move's or copy's destination.
    dst: Option<String>,
}

fn diff_raw(out: &[u8]) -> Vec<RawChange> {
    let fields: Vec<String> = out
        .split(|&b| b == 0)
        .map(|f| String::from_utf8_lossy(f).into_owned())
        .collect();
    let mut changes = Vec::new();
    let mut i = 0;
    while i + 1 < fields.len() {
        let meta: Vec<&str> = fields[i].trim_start_matches(':').split(' ').collect();
        let (Some(sha), Some(letter)) = (meta.get(3), meta.get(4).and_then(|s| s.chars().next()))
        else {
            break;
        };
        let two = matches!(letter, 'R' | 'C');
        let status = match letter {
            'A' => "added",
            'D' => "removed",
            'R' => "renamed",
            'C' => "copied",
            'T' => "changed",
            _ => "modified",
        };
        changes.push(RawChange {
            status,
            sha: (*sha).to_owned(),
            src: fields[i + 1].clone(),
            dst: two.then(|| fields.get(i + 2).cloned().unwrap_or_default()),
        });
        i += if two { 3 } else { 2 };
    }
    changes
}

/// The hunks of a file's diff (from its first `@@`), as GitHub's `patch`; none for a binary
/// file or a move without changes.
fn hunks(diff: &str) -> Option<String> {
    let start = if diff.starts_with("@@") {
        0
    } else {
        diff.find("\n@@")? + 1
    };
    Some(diff[start..].trim_end_matches('\n').to_owned())
}
