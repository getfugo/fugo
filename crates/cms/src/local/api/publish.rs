//! Publishing a draft onto the branch, and discarding one.

use super::*;

impl Api<'_> {
    /// Copies a draft's files onto the branch in one commit, then deletes the draft. When the
    /// branch changed some of them since the draft was made, it is merged into the draft first.
    pub(in super::super) fn publish(&self, body: &Map<String, Value>) -> Result<Value, HttpError> {
        if !self.review() {
            return Err(HttpError::new(
                400,
                "there are no drafts: workflow is direct",
            ));
        }
        if !self.user.publish {
            return Err(HttpError::new(403, "you may not publish"));
        }
        let id = body.get("id");
        let mut merged = false;
        let mut attempt = 0;
        loop {
            let head = self.draft_head(id)?;
            let draft = format!("{DRAFTS}{}", id.and_then(Value::as_str).unwrap_or_default());
            let main = self.main_head()?;
            let cmp = self.git.compare(&main, &head, false)?;
            if cmp.files.len() >= 300 {
                return Err(HttpError::new(
                    422,
                    "the draft changes too many files to publish here",
                ));
            }
            for p in cmp.files.iter().flat_map(CompareFile::paths) {
                if !may_edit(&self.e.areas, &self.user.edit, p) {
                    return Err(HttpError::new(
                        403,
                        format!("the draft changes {p}, which you may not change"),
                    ));
                }
            }
            let saves = saves_of(&cmp);
            let last = saves.last().map_or("", |c| c.message.as_str());
            let t = parse_trailers(last);
            let conflicts = self.conflicts(&cmp, &main)?;
            if !conflicts.is_empty() {
                if merged {
                    return Err(HttpError::new(
                        409,
                        "the branch keeps changing these files; try again",
                    )
                    .with("conflicts", conflicts));
                }
                // The merge commit carries the draft's trailers: the drafts list reads its last commit's.
                let title = title_of(&t, last);
                let message = commit_message(
                    &format!("Bring in {}", self.branch),
                    &[
                        ("CMS-Entry", t.get("cms-entry").map(String::as_str)),
                        ("CMS-Title", Some(&title)),
                    ],
                );
                if !self
                    .git
                    .merge_into(&draft, &self.branch, &message, &self.user.author)?
                {
                    return Err(HttpError::new(
                        409,
                        "the branch and the draft changed the same lines of these files",
                    )
                    .with("conflicts", conflicts));
                }
                merged = true;
                continue;
            }
            if cmp.files.is_empty() {
                self.git.delete_branch(&draft)?;
                return Ok(json!({ "commit": null, "published": false }));
            }
            let mut entries = Vec::new();
            for f in &cmp.files {
                if let Some(prev) = f.previous.as_ref().filter(|p| **p != f.path) {
                    entries.push(TreeEntry {
                        path: prev.clone(),
                        sha: None,
                    });
                }
                let sha = (f.status != "removed").then(|| f.sha.clone());
                entries.push(TreeEntry {
                    path: f.path.clone(),
                    sha,
                });
            }
            let mut authors: Vec<&Author> = Vec::new();
            for a in saves.iter().filter_map(|c| c.author.as_ref()) {
                if !a.email.is_empty() && !authors.iter().any(|x| x.email == a.email) {
                    authors.push(a);
                }
            }
            let author = authors.first().copied().unwrap_or(&self.user.author);
            let entry = t
                .get("cms-entry")
                .cloned()
                .unwrap_or_else(|| draft[DRAFTS.len()..].to_owned());
            let title = t.get("cms-title").map(String::as_str);
            let subject = match title {
                Some(title) if !title.is_empty() => format!("Publish {entry}: {title}"),
                _ => format!("Publish {entry}"),
            };
            let co: Vec<String> = authors[authors.len().min(1)..]
                .iter()
                .map(|a| format!("{} <{}>", a.name, a.email))
                .collect();
            let mut trailers = vec![("CMS-Entry", Some(entry.as_str())), ("CMS-Title", title)];
            trailers.extend(co.iter().map(|c| ("Co-authored-by", Some(c.as_str()))));
            trailers.push(("CMS-Published-By", Some(&self.user.author.email)));
            let message = commit_message(&subject, &trailers);
            match self
                .git
                .commit_tree(&self.branch, &main, &entries, &message, author)
            {
                Ok(commit) => {
                    // A save that reached the draft after it was compared stays a draft.
                    if self.git.head(&draft)?.as_deref() != Some(head.as_str()) {
                        return Ok(json!({ "commit": commit, "published": true, "kept": true }));
                    }
                    self.git.delete_branch(&draft)?;
                    return Ok(json!({ "commit": commit, "published": true }));
                }
                Err(Fail::Race) if attempt < 2 => attempt += 1,
                Err(Fail::Race) => {
                    return Err(HttpError::new(409, "the branch keeps changing; try again"));
                }
                Err(Fail::Http(e)) => return Err(e),
            }
        }
    }

    /// Deletes a draft (the person may publish, so any draft).
    pub(in super::super) fn discard(&self, body: &Map<String, Value>) -> Result<Value, HttpError> {
        let id = body.get("id");
        self.draft_head(id)?;
        let id = id.and_then(Value::as_str).unwrap_or_default();
        self.git.delete_branch(&format!("{DRAFTS}{id}"))?;
        Ok(json!({ "discarded": id }))
    }
}
