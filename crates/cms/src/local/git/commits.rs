//! Commits and branches, read in one `git cat-file --batch`.

use super::*;

#[derive(Clone, Debug)]
pub(in super::super) struct Commit {
    pub(in super::super) parents: Vec<String>,
    pub(in super::super) author: Option<Author>,
    /// The committer's time, in seconds since the epoch.
    pub(in super::super) committed: i64,
    pub(in super::super) message: String,
}

impl Commit {
    /// The committer's time as RFC 3339 (`2026-10-10T08:00:00Z`), as GitHub gives it.
    pub(in super::super) fn date(&self) -> String {
        jiff::Timestamp::from_second(self.committed)
            .map(|t| t.to_string())
            .unwrap_or_default()
    }

    /// A merge commit (one that brought the branch into a draft).
    pub(in super::super) fn is_merge(&self) -> bool {
        self.parents.len() > 1
    }
}

/// A branch and its last commit.
pub(in super::super) struct Branch {
    pub(in super::super) name: String,
    pub(in super::super) head: Commit,
}

impl Git {
    /// The branches under `prefix` with their last commit, newest first (at most 100, as on
    /// the git host).
    pub(in super::super) fn branches_with_heads(
        &self,
        prefix: &str,
    ) -> Result<Vec<Branch>, HttpError> {
        let pattern = format!("refs/heads/{prefix}");
        let out = self.text(&[
            "for-each-ref",
            "--format=%(objectname) %(refname)",
            &pattern,
        ])?;
        let refs: Vec<(String, String)> = out
            .lines()
            .filter_map(|l| {
                let (sha, name) = l.split_once(' ')?;
                Some((sha.to_owned(), name.strip_prefix(&pattern)?.to_owned()))
            })
            .collect();
        let shas: Vec<String> = refs.iter().map(|(s, _)| s.clone()).collect();
        let mut branches: Vec<Branch> = refs
            .into_iter()
            .zip(self.commits(&shas)?)
            .map(|((_, name), head)| Branch { name, head })
            .collect();
        branches.sort_by_key(|b| std::cmp::Reverse(b.head.committed));
        branches.truncate(100);
        Ok(branches)
    }

    /// Commits by id, in that order.
    pub(super) fn commits(&self, ids: &[String]) -> Result<Vec<Commit>, HttpError> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let input = format!("{}\n", ids.join("\n"));
        let out = self.run(&["cat-file", "--batch"], Some(input.as_bytes()))?;
        let mut rest = out.as_slice();
        let mut commits = Vec::with_capacity(ids.len());
        while !rest.is_empty() {
            let end = rest.iter().position(|&b| b == b'\n').unwrap_or(rest.len());
            let header = String::from_utf8_lossy(&rest[..end]).into_owned();
            let size = match header.split(' ').collect::<Vec<_>>()[..] {
                [_, "commit", size] => size.parse::<usize>().ok(),
                _ => None,
            };
            let Some(size) = size.filter(|s| end + 1 + s <= rest.len()) else {
                return Err(HttpError::new(500, format!("git cat-file: {header}")));
            };
            commits.push(parse_commit(&rest[end + 1..end + 1 + size]));
            rest = rest.get(end + 2 + size..).unwrap_or_default();
        }
        Ok(commits)
    }
}

/// A raw commit object.
fn parse_commit(raw: &[u8]) -> Commit {
    let text = String::from_utf8_lossy(raw);
    let (headers, message) = text.split_once("\n\n").unwrap_or((&text, ""));
    let mut commit = Commit {
        parents: Vec::new(),
        author: None,
        committed: 0,
        message: message.to_owned(),
    };
    for line in headers.lines() {
        if let Some(p) = line.strip_prefix("parent ") {
            commit.parents.push(p.to_owned());
        } else if let Some(a) = line.strip_prefix("author ") {
            commit.author = ident(a).map(|(a, _)| a);
        } else if let Some(c) = line.strip_prefix("committer ") {
            commit.committed = ident(c).map_or(0, |(_, t)| t);
        }
    }
    commit
}
