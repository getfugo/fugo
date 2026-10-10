//! The requests, as the Worker's `Api` answers them (`web/assets/worker/api.ts`), with the
//! local repository for the git host: same checks, same commit messages and trailers, same
//! answers, and no pull requests.

use std::collections::{BTreeMap, BTreeSet};

use base64::Engine as _;
use serde_json::{Map, Value, json};

use super::git::{Author, Change, ChangeKind, CompareFile, Comparison, Fail, Git, TreeEntry};
use super::rules::{
    area_of, base64_size, clean_path, commit_message, draft_id, is_base64, is_draft_id, may_edit,
    parse_trailers,
};
use super::{Editor, HttpError};
use crate::config::Workflow;
use crate::paths::DENY;

mod publish;
mod save;

/// The branches of drafts: `cms/<id>`.
const DRAFTS: &str = "cms/";
const MAX_CHANGES: usize = 30;
const MAX_PATCH: usize = 20000;

/// The person: the repository's git identity, with every role.
struct User {
    author: Author,
    roles: Vec<String>,
    edit: Vec<String>,
    publish: bool,
}

/// The API of one request.
pub(super) struct Api<'a> {
    e: &'a Editor,
    git: Git,
    user: User,
    /// The branch checked out: drafts are made from it and published onto it.
    branch: String,
}

impl<'a> Api<'a> {
    pub(super) fn open(e: &'a Editor) -> Result<Self, HttpError> {
        let git = Git::open(&e.project_dir)?;
        let mut author = git.identity()?;
        author.email = author.email.to_lowercase();
        if author.name.is_empty() {
            author.name = author
                .email
                .split('@')
                .next()
                .unwrap_or_default()
                .to_owned();
        }
        let branch = git.current_branch()?.ok_or_else(|| {
            HttpError::new(
                409,
                "check out a branch: the local editor makes drafts from the branch checked out, and publishes onto it",
            )
        })?;
        if branch.starts_with(DRAFTS) {
            return Err(HttpError::new(
                409,
                format!(
                    "{branch} is a draft of the editor: check out the branch drafts are published onto"
                ),
            ));
        }
        let user = User {
            author,
            roles: e.roles.clone(),
            edit: vec!["**".to_owned()],
            publish: true,
        };
        Ok(Self {
            e,
            git,
            user,
            branch,
        })
    }

    fn review(&self) -> bool {
        self.e.workflow == Workflow::Review
    }

    pub(super) fn me(&self) -> Value {
        json!({
            "email": self.user.author.email,
            "roles": self.user.roles,
            "edit": self.user.edit,
            "publish": self.user.publish && self.review(),
            "workflow": self.e.workflow,
            "login": "local",
            "repo": self.e.repo,
            "branch": self.branch,
            "areas": self.e.areas,
            "deny": DENY,
            "maxUpload": self.e.max_upload,
        })
    }

    /// A file of the branch or of a draft, as base64 (`content`) with its blob id (`sha`).
    pub(super) fn file(&self, path: Option<&str>, draft: Option<&str>) -> Result<Value, HttpError> {
        let Some(path) = path.filter(|p| area_of(&self.e.areas, p).is_some()) else {
            return Err(HttpError::new(
                403,
                format!("the editor cannot open {}", path.unwrap_or("null")),
            ));
        };
        let rev = match draft.filter(|d| !d.is_empty()) {
            Some(id) => self.draft_head(Some(&Value::from(id)))?,
            None => self.main_head()?,
        };
        let file = self
            .git
            .read(&rev, path)?
            .ok_or_else(|| HttpError::new(404, format!("{path} does not exist")))?;
        Ok(json!({ "path": path, "sha": file.sha, "size": file.size, "content": file.content }))
    }

    /// The open drafts, newest first.
    pub(super) fn drafts(&self) -> Result<Value, HttpError> {
        let drafts: Vec<Value> = self
            .git
            .branches_with_heads(DRAFTS)?
            .into_iter()
            .filter(|b| is_draft_id(&b.name))
            .map(|b| {
                let t = parse_trailers(&b.head.message);
                json!({
                    "id": b.name,
                    "entry": t.get("cms-entry").cloned().unwrap_or_default(),
                    "title": title_of(&t, &b.head.message),
                    "author": b.head.author,
                    "updated": b.head.date(),
                    "pr": null,
                })
            })
            .collect();
        Ok(json!({ "drafts": drafts }))
    }

    /// A draft: its files (with their diff), its saves, and its files the branch changed since
    /// (`conflicts`, which publishing merges).
    pub(super) fn draft(&self, id: Option<&str>) -> Result<Value, HttpError> {
        let id = id.map(Value::from);
        let head = self.draft_head(id.as_ref())?;
        let main = self.main_head()?;
        let cmp = self.git.compare(&main, &head, true)?;
        let saves = saves_of(&cmp);
        let last = saves.last().map_or("", |c| c.message.as_str());
        let t = parse_trailers(last);
        let files: Vec<Value> = cmp
            .files
            .iter()
            .map(|f| {
                let mut o = Map::new();
                o.insert("path".into(), f.path.clone().into());
                o.insert("status".into(), f.status.into());
                if let Some(p) = &f.previous {
                    o.insert("previous".into(), p.clone().into());
                }
                if let Some(patch) = &f.patch {
                    let cut = patch.char_indices().nth(MAX_PATCH).map(|(i, _)| i);
                    let text = cut.map_or_else(|| patch.clone(), |i| format!("{}\n…", &patch[..i]));
                    o.insert("patch".into(), text.into());
                }
                Value::Object(o)
            })
            .collect();
        let commits: Vec<Value> = saves
            .iter()
            .map(|c| json!({ "author": c.author, "subject": c.message.lines().next().unwrap_or("") }))
            .collect();
        Ok(json!({
            "id": id,
            "entry": t.get("cms-entry").cloned().unwrap_or_default(),
            "title": title_of(&t, last),
            "pr": null,
            "files": files,
            "commits": commits,
            "conflicts": self.conflicts(&cmp, &main)?,
        }))
    }

    /// `id` when it names a draft that exists (a page opened from a draft that was published or
    /// discarded since saves to a draft of its own).
    fn open_draft(&self, id: Option<&Value>) -> Result<Option<String>, HttpError> {
        let Some(id) = id.filter(|v| !v.is_null()) else {
            return Ok(None);
        };
        let Some(id) = id.as_str().filter(|i| is_draft_id(i)) else {
            return Err(HttpError::new(400, "bad draft id"));
        };
        Ok(self
            .git
            .head(&format!("{DRAFTS}{id}"))?
            .map(|_| id.to_owned()))
    }

    fn draft_head(&self, id: Option<&Value>) -> Result<String, HttpError> {
        let Some(id) = id.and_then(Value::as_str).filter(|i| is_draft_id(i)) else {
            return Err(HttpError::new(400, "bad draft id"));
        };
        self.git
            .head(&format!("{DRAFTS}{id}"))?
            .ok_or_else(|| HttpError::new(404, "no such draft (published or discarded?)"))
    }

    fn main_head(&self) -> Result<String, HttpError> {
        self.git.head(&self.branch)?.ok_or_else(|| {
            HttpError::new(500, format!("branch {} has no commits yet", self.branch))
        })
    }

    /// The draft's paths the branch changed since the draft was made from it (or last merged
    /// it): their blob at the merge base and on the branch differ.
    fn conflicts(&self, cmp: &Comparison, main: &str) -> Result<Vec<String>, HttpError> {
        if cmp.merge_base == main {
            return Ok(Vec::new());
        }
        let mut paths: Vec<&str> = Vec::new();
        for p in cmp.files.iter().flat_map(CompareFile::paths) {
            if !paths.contains(&p) {
                paths.push(p);
            }
        }
        let then = self.git.blob_ids(&cmp.merge_base, &paths)?;
        let now = self.git.blob_ids(main, &paths)?;
        Ok(paths
            .into_iter()
            .filter(|p| then.get(*p) != now.get(*p))
            .map(str::to_owned)
            .collect())
    }
}

/// A draft's saves: its commits but the merges that brought the branch in.
fn saves_of(cmp: &Comparison) -> Vec<&super::git::Commit> {
    cmp.commits.iter().filter(|c| !c.is_merge()).collect()
}

/// A draft's title: the page's, from its last commit; for a draft of Decap CMS, the commit's
/// subject.
fn title_of(trailers: &BTreeMap<String, String>, message: &str) -> String {
    if trailers.contains_key("cms-entry") {
        return trailers.get("cms-title").cloned().unwrap_or_default();
    }
    trailers
        .get("cms-title")
        .cloned()
        .unwrap_or_else(|| message.lines().next().unwrap_or("").to_owned())
}
