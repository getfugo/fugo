//! `enableGitInfo`: the last commit of every file of the project's Git repository, read with one
//! `git log` per build, for `page.git_info` and the `:git` date.
//!
//! `git` runs as `[security]` allows programs to run: it must match `exec.allow` (it does by
//! default), and gets only the environment variables of `exec.osEnv`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use jiff::Zoned;
use jiff::tz::TimeZone;
use ssg_config::Config;

use crate::ModelError;

/// The last commit of a content file (`page.git_info`); its author date is the `:git` date.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitInfo {
    pub hash: String,
    pub abbreviated_hash: String,
    pub subject: String,
    /// The message after the subject, without surrounding white space.
    pub body: String,
    /// The author's name and email address, as `.mailmap` maps them.
    pub author_name: String,
    pub author_email: String,
    /// The author and commit dates, each in the offset Git recorded.
    pub author_date: Zoned,
    pub commit_date: Zoned,
}

/// The last commit of every file of a repository.
#[derive(Debug)]
pub(crate) struct GitLog {
    /// The repository's top directory, canonical.
    top: PathBuf,
    /// The last commit by path below `top` (`/`-separated, as Git writes it).
    files: HashMap<String, Arc<GitInfo>>,
    /// Whether the repository is a shallow clone: then a file the clone's commits do not change
    /// has the date of its oldest commit.
    pub shallow: bool,
}

/// One commit of `git log -z --name-only`: `\x1e`, the fields separated by `\x1f`, `\x1d`, then
/// the NUL that ends the message (a message has no NUL, so `\x1d\0` ends the fields), a newline
/// and the files, each followed by a NUL.
const FORMAT: &str = "--format=%x1e%H%x1f%h%x1f%s%x1f%aN%x1f%aE%x1f%aI%x1f%cI%x1f%b%x1d";

impl GitLog {
    /// Reads the history of the repository the project is in.
    ///
    /// # Errors
    /// `git` is not allowed by `security.exec.allow` or cannot run, the project is not in a Git
    /// repository, or Git's output cannot be read.
    pub(crate) fn load(cfg: &Config) -> Result<Self, ModelError> {
        if !cfg.security.exec_allow.accepts("git") {
            return Err(ModelError::Git(
                "`git` is not allowed by security.exec.allow (allow it there, or turn \
                 enableGitInfo off)"
                    .to_owned(),
            ));
        }
        let rev = git(
            cfg,
            &["rev-parse", "--show-toplevel", "--is-shallow-repository"],
        )?;
        let mut lines = rev.lines();
        let (Some(top), Some(shallow)) = (lines.next(), lines.next()) else {
            return Err(ModelError::Git(format!(
                "git rev-parse: unexpected output {rev:?}"
            )));
        };
        let top = std::fs::canonicalize(top)
            .map_err(|e| ModelError::Git(format!("the repository's directory {top}: {e}")))?;
        // Every commit's files with their paths from the top directory, whatever the user's
        // configuration says (`log.showRoot`, `diff.relative`, `log.showSignature`,
        // `i18n.logOutputEncoding`, colours).
        let log = git(
            cfg,
            &[
                "log",
                "-z",
                "--name-only",
                "--root",
                "--no-merges",
                "--no-renames",
                "--no-relative",
                "--no-show-signature",
                "--no-color",
                "--encoding=UTF-8",
                FORMAT,
            ],
        )?;
        let files = parse(&log).map_err(|e| ModelError::Git(format!("git log: {e}")))?;
        Ok(Self {
            top,
            files,
            shallow: shallow == "true",
        })
    }

    /// The last commit of the file at `abs`, if it is a file of the repository Git knows.
    pub(crate) fn file(&self, abs: &Path) -> Option<Arc<GitInfo>> {
        let abs = std::fs::canonicalize(abs).ok()?;
        let rel = abs.strip_prefix(&self.top).ok()?;
        let parts: Option<Vec<&str>> = rel.components().map(|c| c.as_os_str().to_str()).collect();
        self.files.get(&parts?.join("/")).cloned()
    }
}

/// Runs `git` in the project directory with the environment `security.exec.osEnv` allows: its
/// standard output.
fn git(cfg: &Config, args: &[&str]) -> Result<String, ModelError> {
    let allowed = &cfg.security.exec_os_env;
    let out = Command::new("git")
        .arg("-C")
        .arg(&cfg.project_dir)
        .args(args)
        .env_clear()
        .envs(std::env::vars_os().filter(|(k, _)| k.to_str().is_some_and(|k| allowed.accepts(k))))
        .output()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => ModelError::Git(
                "`git` is not installed (or not on PATH): install it, or turn enableGitInfo off"
                    .to_owned(),
            ),
            _ => ModelError::Git(format!("running git: {e}")),
        })?;
    if !out.status.success() {
        return Err(ModelError::Git(format!(
            "git {}: {}",
            args[0],
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    String::from_utf8(out.stdout)
        .map_err(|_| ModelError::Git(format!("git {}: the output is not UTF-8", args[0])))
}

/// The last commit of each file of a `git log` in [`FORMAT`] (newest commit first).
fn parse(log: &str) -> Result<HashMap<String, Arc<GitInfo>>, String> {
    let mut files = HashMap::new();
    let mut rest = log;
    while let Some(commit) = rest.strip_prefix('\x1e') {
        let (fields, names) = commit
            .split_once("\x1d\0")
            .ok_or("a commit without the end of its fields")?;
        let info = Arc::new(commit_info(fields)?);
        let mut names = names.strip_prefix('\n').unwrap_or(names);
        while !names.is_empty() && !names.starts_with('\x1e') {
            let (name, next) = names.split_once('\0').unwrap_or((names, ""));
            files
                .entry(name.to_owned())
                .or_insert_with(|| Arc::clone(&info));
            names = next;
        }
        rest = names;
    }
    if rest.is_empty() {
        Ok(files)
    } else {
        Err(format!(
            "unexpected output {:?}",
            rest.chars().take(40).collect::<String>()
        ))
    }
}

/// A commit's fields: hash, abbreviated hash, subject, author name and email, author and
/// commit dates (strict ISO 8601), body.
fn commit_info(fields: &str) -> Result<GitInfo, String> {
    let f: Vec<&str> = fields.splitn(8, '\x1f').collect();
    let [
        hash,
        abbreviated_hash,
        subject,
        author_name,
        author_email,
        author_date,
        commit_date,
        body,
    ] = f[..]
    else {
        return Err(format!("a commit with {} fields instead of 8", f.len()));
    };
    let date = |s: &str| {
        ssg_base::parse_date(s, &TimeZone::UTC).map_err(|_| format!("the date {s:?} of {hash}"))
    };
    Ok(GitInfo {
        hash: hash.to_owned(),
        abbreviated_hash: abbreviated_hash.to_owned(),
        subject: subject.to_owned(),
        body: body.trim().to_owned(),
        author_name: author_name.to_owned(),
        author_email: author_email.to_owned(),
        author_date: date(author_date)?,
        commit_date: date(commit_date)?,
    })
}
