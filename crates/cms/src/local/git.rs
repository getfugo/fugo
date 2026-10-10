//! The local repository through the `git` command: what the Worker asks of its git host
//! (`web/assets/worker/github.ts`), done to local branches. Paths are project-relative; in the
//! repository they are under the project's directory (`dir`), and a repository path outside it
//! is `/path` (never writable).

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use base64::Engine as _;
use serde::Serialize;

use super::HttpError;

mod commits;
mod compare;
mod write;
pub(super) use commits::Commit;
pub(super) use compare::{CompareFile, Comparison};
pub(super) use write::{Change, ChangeKind, Fail, TreeEntry};

/// The variables that would point git at another repository, index or work tree than the
/// project's (set for a server started from a git hook, for example).
const REPO_ENV: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_PREFIX",
    "GIT_COMMON_DIR",
    "GIT_NAMESPACE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
];

/// The repository of the project.
pub(super) struct Git {
    /// The work tree's top directory.
    top: PathBuf,
    /// The work tree's git directory (where the temporary indexes go).
    git_dir: PathBuf,
    /// The project's directory in the repository (`""`, or `docs/`).
    dir: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(super) struct Author {
    pub(super) name: String,
    pub(super) email: String,
}

/// A file at a commit, its content as base64.
pub(super) struct FileAt {
    pub(super) sha: String,
    pub(super) size: usize,
    pub(super) content: String,
}

impl Git {
    /// The repository the project at `project` is in.
    pub(super) fn open(project: &Path) -> Result<Self, HttpError> {
        let mut cmd = command(project);
        cmd.args([
            "rev-parse",
            "--show-toplevel",
            "--absolute-git-dir",
            "--show-prefix",
        ]);
        let out = output(cmd, None)?;
        if !out.status.success() {
            return Err(HttpError::new(
                500,
                format!(
                    "the project is not in a git repository, which the local editor commits to ({})",
                    String::from_utf8_lossy(&out.stderr).trim()
                ),
            ));
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let mut lines = text.lines();
        let (Some(top), Some(git_dir)) = (lines.next(), lines.next()) else {
            return Err(HttpError::new(500, "git rev-parse: unexpected output"));
        };
        Ok(Self {
            top: PathBuf::from(top),
            git_dir: PathBuf::from(git_dir),
            dir: lines.next().unwrap_or("").to_owned(),
        })
    }

    fn command(&self) -> Command {
        command(&self.top)
    }

    /// Runs git with `args` (and `stdin`); its output whatever its exit status.
    fn output(&self, args: &[&str], stdin: Option<&[u8]>) -> Result<Output, HttpError> {
        let mut cmd = self.command();
        cmd.args(args);
        output(cmd, stdin)
    }

    /// Runs git; its standard output, or an error with what it said.
    fn run(&self, args: &[&str], stdin: Option<&[u8]>) -> Result<Vec<u8>, HttpError> {
        let out = self.output(args, stdin)?;
        if out.status.success() {
            Ok(out.stdout)
        } else {
            Err(failed(args, &out))
        }
    }

    /// [`Self::run`] as text, trimmed.
    fn text(&self, args: &[&str]) -> Result<String, HttpError> {
        Ok(String::from_utf8_lossy(&self.run(args, None)?)
            .trim()
            .to_owned())
    }

    pub(super) fn to_repo(&self, path: &str) -> String {
        match path.strip_prefix('/') {
            Some(outside) => outside.to_owned(),
            None => format!("{}{path}", self.dir),
        }
    }

    fn project_path(&self, path: &str) -> String {
        match path.strip_prefix(self.dir.as_str()) {
            Some(inside) => inside.to_owned(),
            None => format!("/{path}"),
        }
    }

    /// Who commits: git's author identity (the repository's `user.name` and `user.email`).
    pub(super) fn identity(&self) -> Result<Author, HttpError> {
        let out = self.output(&["var", "GIT_AUTHOR_IDENT"], None)?;
        let text = String::from_utf8_lossy(&out.stdout);
        match ident(text.trim()) {
            Some((author, _)) if out.status.success() && !author.email.is_empty() => Ok(author),
            _ => Err(HttpError::new(
                500,
                "tell git who you are first: git config --global user.email you@example.com, and user.name",
            )),
        }
    }

    /// The branch checked out (`None`: a detached head).
    pub(super) fn current_branch(&self) -> Result<Option<String>, HttpError> {
        let args = ["symbolic-ref", "-q", "HEAD"];
        let out = self.output(&args, None)?;
        match out.status.code() {
            Some(0) => Ok(String::from_utf8_lossy(&out.stdout)
                .trim()
                .strip_prefix("refs/heads/")
                .map(str::to_owned)),
            Some(1) => Ok(None),
            _ => Err(failed(&args, &out)),
        }
    }

    /// The commit `branch` points at.
    pub(super) fn head(&self, branch: &str) -> Result<Option<String>, HttpError> {
        let rev = format!("refs/heads/{branch}^{{commit}}");
        let args = ["rev-parse", "-q", "--verify", &rev];
        let out = self.output(&args, None)?;
        match out.status.code() {
            Some(0) => Ok(Some(String::from_utf8_lossy(&out.stdout).trim().to_owned())),
            Some(1 | 128) => Ok(None),
            _ => Err(failed(&args, &out)),
        }
    }

    /// A file at `rev`, or `None`.
    pub(super) fn read(&self, rev: &str, path: &str) -> Result<Option<FileAt>, HttpError> {
        let repo = self.to_repo(path);
        let listing = self.run(&["ls-tree", "-z", "--full-tree", rev, "--", &repo], None)?;
        let Some((kind, sha)) = ls_tree(&listing)
            .into_iter()
            .find(|(p, _, _)| *p == repo)
            .map(|(_, kind, sha)| (kind, sha))
        else {
            return Ok(None);
        };
        if kind != "blob" {
            return Err(HttpError::new(400, format!("{path} is not a file")));
        }
        let bytes = self.run(&["cat-file", "blob", &sha], None)?;
        Ok(Some(FileAt {
            sha,
            size: bytes.len(),
            content: base64::engine::general_purpose::STANDARD.encode(&bytes),
        }))
    }

    /// The object ids of `paths` at commit `sha` (`None`: no such file).
    pub(super) fn blob_ids(
        &self,
        sha: &str,
        paths: &[&str],
    ) -> Result<BTreeMap<String, Option<String>>, HttpError> {
        let mut out: BTreeMap<String, Option<String>> =
            paths.iter().map(|p| ((*p).to_owned(), None)).collect();
        if paths.is_empty() {
            return Ok(out);
        }
        let repo: Vec<String> = paths.iter().map(|p| self.to_repo(p)).collect();
        let mut args = vec!["ls-tree", "-r", "-z", "--full-tree", sha, "--"];
        args.extend(repo.iter().map(String::as_str));
        let found: BTreeMap<String, String> = ls_tree(&self.run(&args, None)?)
            .into_iter()
            .map(|(p, _, sha)| (p, sha))
            .collect();
        for (path, repo) in paths.iter().zip(&repo) {
            out.insert((*path).to_owned(), found.get(repo).cloned());
        }
        Ok(out)
    }
}

/// git, at `dir`, with literal paths and no repository from the environment.
fn command(dir: &Path) -> Command {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(dir)
        .args(["--literal-pathspecs", "-c", "core.quotePath=false"]);
    for name in REPO_ENV {
        cmd.env_remove(name);
    }
    cmd.env("GIT_TERMINAL_PROMPT", "0").env("LC_ALL", "C");
    cmd
}

/// Runs `cmd`, feeding it `stdin`.
fn output(mut cmd: Command, stdin: Option<&[u8]>) -> Result<Output, HttpError> {
    cmd.stdin(if stdin.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    })
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            HttpError::new(
                500,
                "the local editor runs git: install it, or put it on PATH",
            )
        } else {
            HttpError::new(500, format!("running git: {e}"))
        }
    })?;
    // A thread writes the input, so that a large output cannot block it.
    let writer = match (stdin, child.stdin.take()) {
        (Some(input), Some(mut pipe)) => {
            let input = input.to_vec();
            Some(std::thread::spawn(move || pipe.write_all(&input)))
        }
        _ => None,
    };
    let out = child
        .wait_with_output()
        .map_err(|e| HttpError::new(500, format!("running git: {e}")))?;
    if let Some(w) = writer {
        let _ = w.join();
    }
    Ok(out)
}

/// The error of a git command that failed.
fn failed(args: &[&str], out: &Output) -> HttpError {
    let said = String::from_utf8_lossy(&out.stderr);
    HttpError::new(
        500,
        format!("git {}: {}", args.first().unwrap_or(&""), said.trim()),
    )
}

/// An identity (`Name <email> 1700000000 +0100`) and its time.
fn ident(s: &str) -> Option<(Author, i64)> {
    let lt = s.find('<')?;
    let gt = lt + s[lt..].find('>')?;
    let time = s[gt + 1..].split_whitespace().next()?.parse().unwrap_or(0);
    Some((
        Author {
            name: s[..lt].trim().to_owned(),
            email: s[lt + 1..gt].trim().to_owned(),
        },
        time,
    ))
}

/// The entries of `git ls-tree -z`: path, type, object id.
fn ls_tree(out: &[u8]) -> Vec<(String, String, String)> {
    out.split(|&b| b == 0)
        .filter_map(|rec| {
            let rec = String::from_utf8_lossy(rec);
            let (meta, path) = rec.split_once('\t')?;
            let mut meta = meta.split(' ');
            let (_mode, kind, sha) = (meta.next()?, meta.next()?, meta.next()?);
            Some((path.to_owned(), kind.to_owned(), sha.to_owned()))
        })
        .collect()
}
