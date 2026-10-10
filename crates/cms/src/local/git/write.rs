//! Writing to the repository: blobs, then trees made in an index of their own (never the work
//! tree's), commits, and branches moved only from the commit they were at (`git update-ref
//! <new> <old>`). The branch checked out moves with its files instead (`git merge
//! --ff-only`), which refuses to overwrite changes of the work tree.

use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use super::{Author, Git, failed, output};
use crate::local::HttpError;

/// Why a write failed.
pub(in super::super) enum Fail {
    /// The branch moved meanwhile (the write is retried).
    Race,
    Http(HttpError),
}

impl From<HttpError> for Fail {
    fn from(e: HttpError) -> Self {
        Self::Http(e)
    }
}

/// A checked change of a save (`base`: the blob id the editor started from; `Some(None)`: a
/// new file; `None`: not checked).
pub(in super::super) struct Change {
    pub(in super::super) path: String,
    pub(in super::super) base: Option<Option<String>>,
    pub(in super::super) kind: ChangeKind,
}

pub(in super::super) enum ChangeKind {
    Delete,
    Write(Vec<u8>),
    /// The file is the one at this path, which the same save deletes.
    Move(String),
}

/// A tree entry: a blob, or `None` to delete the path.
#[derive(Clone)]
pub(in super::super) struct TreeEntry {
    pub(in super::super) path: String,
    pub(in super::super) sha: Option<String>,
}

/// An index file of the git directory, removed when dropped.
struct TempIndex(PathBuf);

impl TempIndex {
    fn new(git_dir: &std::path::Path) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        Self(git_dir.join(format!("fugo-cms-{}-{n}.index", std::process::id())))
    }
}

impl Drop for TempIndex {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

impl Git {
    /// Runs git with `env` too.
    fn run_env(
        &self,
        args: &[&str],
        stdin: Option<&[u8]>,
        env: &[(&str, &OsStr)],
    ) -> Result<Vec<u8>, HttpError> {
        let mut cmd = self.command();
        cmd.args(args);
        for (k, v) in env {
            cmd.env(k, v);
        }
        let out = output(cmd, stdin)?;
        if out.status.success() {
            Ok(out.stdout)
        } else {
            Err(failed(args, &out))
        }
    }

    /// Stores `bytes` as a blob (as they are: no filters, like a git host's API).
    fn hash(&self, bytes: &[u8]) -> Result<String, HttpError> {
        let out = self.run(
            &["hash-object", "-w", "--no-filters", "--stdin"],
            Some(bytes),
        )?;
        Ok(String::from_utf8_lossy(&out).trim().to_owned())
    }

    /// Commits `changes` to `branch`, made from branch `from` when it does not exist yet
    /// (`true`: made). A change whose `base` is no longer the file's fails with 409, as does a
    /// move whose source is gone; a moved file keeps its blob.
    pub(in super::super) fn commit_files(
        &self,
        branch: &str,
        from: Option<&str>,
        changes: &[Change],
        message: &str,
        author: &Author,
    ) -> Result<(String, bool), HttpError> {
        let mut entries = Vec::new();
        let mut moves = Vec::new();
        for c in changes {
            match &c.kind {
                ChangeKind::Delete => entries.push(TreeEntry {
                    path: c.path.clone(),
                    sha: None,
                }),
                ChangeKind::Write(bytes) => entries.push(TreeEntry {
                    path: c.path.clone(),
                    sha: Some(self.hash(bytes)?),
                }),
                ChangeKind::Move(source) => moves.push((c.path.as_str(), source.as_str())),
            }
        }
        let mut attempt = 0;
        loop {
            let (parent, creating) = match self.head(branch)? {
                Some(head) => (head, false),
                None => {
                    let from = from.ok_or_else(|| {
                        HttpError::new(500, format!("branch {branch} does not exist"))
                    })?;
                    let head = self.head(from)?.ok_or_else(|| {
                        HttpError::new(500, format!("branch {from} does not exist"))
                    })?;
                    (head, true)
                }
            };
            let checks: Vec<(&str, &Option<String>)> = changes
                .iter()
                .filter_map(|c| c.base.as_ref().map(|b| (c.path.as_str(), b)))
                .collect();
            let paths: Vec<&str> = checks.iter().map(|(p, _)| *p).collect();
            let current = self.blob_ids(&parent, &paths)?;
            let stale: Vec<&str> = checks
                .iter()
                .filter(|(p, base)| current.get(*p) != Some(*base))
                .map(|(p, _)| *p)
                .collect();
            if !stale.is_empty() {
                return Err(HttpError::new(
                    409,
                    "someone changed these files since you opened them; reload them first",
                )
                .with("stale", stale));
            }
            let sources: Vec<&str> = moves.iter().map(|(_, f)| *f).collect();
            let found = self.blob_ids(&parent, &sources)?;
            let gone: Vec<&str> = sources
                .iter()
                .copied()
                .filter(|f| found.get(*f).is_none_or(Option::is_none))
                .collect();
            if !gone.is_empty() {
                return Err(HttpError::new(
                    409,
                    "these files are gone since you opened the page; reload it first",
                )
                .with("stale", gone));
            }
            let mut all = entries.clone();
            all.extend(moves.iter().map(|(path, from)| TreeEntry {
                path: (*path).to_owned(),
                sha: found.get(*from).cloned().flatten(),
            }));
            match self.commit_onto(branch, &parent, creating, &all, message, author) {
                Ok(commit) => return Ok((commit, creating)),
                Err(Fail::Race) if attempt < 2 => attempt += 1,
                Err(Fail::Race) => {
                    return Err(HttpError::new(
                        409,
                        format!("branch {branch} keeps changing; try again"),
                    ));
                }
                Err(Fail::Http(e)) => return Err(e),
            }
        }
    }

    /// Commits tree `entries` on top of `parent` and moves `branch` to it (fails with
    /// [`Fail::Race`] when the branch moved).
    pub(in super::super) fn commit_tree(
        &self,
        branch: &str,
        parent: &str,
        entries: &[TreeEntry],
        message: &str,
        author: &Author,
    ) -> Result<String, Fail> {
        self.commit_onto(branch, parent, false, entries, message, author)
    }

    fn commit_onto(
        &self,
        branch: &str,
        parent: &str,
        creating: bool,
        entries: &[TreeEntry],
        message: &str,
        author: &Author,
    ) -> Result<String, Fail> {
        let tree = self.tree_with(parent, entries)?;
        let commit = self.commit_object(&tree, &[parent], message, author)?;
        self.advance(branch, (!creating).then_some(parent), &commit)?;
        Ok(commit)
    }

    /// The tree of commit `parent` with `entries`, made in an index of its own.
    fn tree_with(&self, parent: &str, entries: &[TreeEntry]) -> Result<String, HttpError> {
        let index = TempIndex::new(&self.git_dir);
        let env = [("GIT_INDEX_FILE", index.0.as_os_str())];
        self.run_env(&["read-tree", parent], None, &env)?;
        let mut adds = Vec::new();
        let mut removes = Vec::new();
        for e in entries {
            let repo = self.to_repo(&e.path);
            match &e.sha {
                Some(sha) => adds.extend(format!("100644 {sha}\t{repo}\0").into_bytes()),
                None => removes.extend(format!("{repo}\0").into_bytes()),
            }
        }
        if !adds.is_empty() {
            self.run_env(&["update-index", "-z", "--index-info"], Some(&adds), &env)?;
        }
        if !removes.is_empty() {
            self.run_env(
                &["update-index", "-z", "--force-remove", "--stdin"],
                Some(&removes),
                &env,
            )?;
        }
        let tree = self.run_env(&["write-tree"], None, &env)?;
        Ok(String::from_utf8_lossy(&tree).trim().to_owned())
    }

    fn commit_object(
        &self,
        tree: &str,
        parents: &[&str],
        message: &str,
        author: &Author,
    ) -> Result<String, HttpError> {
        let mut args = vec!["commit-tree", tree];
        for p in parents {
            args.extend(["-p", *p]);
        }
        args.extend(["-F", "-"]);
        let env = [
            ("GIT_AUTHOR_NAME", OsStr::new(&author.name)),
            ("GIT_AUTHOR_EMAIL", OsStr::new(&author.email)),
        ];
        let out = self.run_env(&args, Some(message.as_bytes()), &env)?;
        Ok(String::from_utf8_lossy(&out).trim().to_owned())
    }

    /// Moves `branch` from `old` (`None`: makes it) to `new`. The branch checked out
    /// fast-forwards with its files, and the server rebuilds the site from them.
    fn advance(&self, branch: &str, old: Option<&str>, new: &str) -> Result<(), Fail> {
        if self.current_branch()?.as_deref() == Some(branch) {
            if self.head(branch)?.as_deref() != old {
                return Err(Fail::Race);
            }
            let out = self.output(&["merge", "--ff-only", "--quiet", new], None)?;
            if out.status.success() {
                return Ok(());
            }
            let said = String::from_utf8_lossy(&out.stderr);
            if said.contains("Not possible to fast-forward") {
                return Err(Fail::Race);
            }
            let files: Vec<&str> = said.lines().filter_map(|l| l.strip_prefix('\t')).collect();
            let message = if files.is_empty() {
                format!("git merge: {}", said.trim())
            } else {
                format!(
                    "your checkout has changes of its own to {}: commit or stash them, then try again",
                    files.join(", ")
                )
            };
            return Err(Fail::Http(HttpError::new(409, message)));
        }
        let name = format!("refs/heads/{branch}");
        let args = [
            "update-ref",
            "-m",
            "fugo cms",
            &name,
            new,
            old.unwrap_or(""),
        ];
        let out = self.output(&args, None)?;
        if out.status.success() {
            return Ok(());
        }
        let said = String::from_utf8_lossy(&out.stderr);
        if ["but expected", "already exists", "unable to resolve"]
            .iter()
            .any(|s| said.contains(s))
        {
            Err(Fail::Race)
        } else {
            Err(Fail::Http(failed(&args, &out)))
        }
    }

    /// Merges branch `from` into `branch` with git's merge (it follows moved files and merges
    /// changes to different lines of a file): `false` when they conflict.
    pub(in super::super) fn merge_into(
        &self,
        branch: &str,
        from: &str,
        message: &str,
        author: &Author,
    ) -> Result<bool, HttpError> {
        for _ in 0..3 {
            let head = self
                .head(branch)?
                .ok_or_else(|| HttpError::new(404, "no such draft (published or discarded?)"))?;
            let theirs = self
                .head(from)?
                .ok_or_else(|| HttpError::new(500, format!("branch {from} does not exist")))?;
            if self.text(&["merge-base", &head, &theirs])? == theirs {
                return Ok(true);
            }
            let args = [
                "merge-tree",
                "--write-tree",
                "--no-messages",
                &head,
                &theirs,
            ];
            let out = self.output(&args, None)?;
            let tree = match out.status.code() {
                Some(0) => String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .to_owned(),
                Some(1) => return Ok(false),
                _ if String::from_utf8_lossy(&out.stderr).contains("write-tree") => {
                    return Err(HttpError::new(
                        500,
                        "publishing a draft whose files the branch changed needs git 2.38 or newer",
                    ));
                }
                _ => return Err(failed(&args, &out)),
            };
            let commit = self.commit_object(&tree, &[&head, &theirs], message, author)?;
            match self.advance(branch, Some(&head), &commit) {
                Ok(()) => return Ok(true),
                Err(Fail::Race) => {}
                Err(Fail::Http(e)) => return Err(e),
            }
        }
        Err(HttpError::new(
            409,
            format!("branch {branch} keeps changing; try again"),
        ))
    }

    /// Deletes `branch` (git refuses while a work tree has it checked out).
    pub(in super::super) fn delete_branch(&self, branch: &str) -> Result<(), HttpError> {
        if self.head(branch)?.is_none() {
            return Ok(());
        }
        self.run(&["branch", "-D", branch], None).map(drop)
    }
}
