//! Saving: the changes of a save checked, then committed to the page's draft (or the branch).

use super::*;

impl Api<'_> {
    /// Commits changes to the draft of `entry` (or the branch, workflow `direct`): the draft the
    /// page was opened from (`draft`), while it exists, else the page's own.
    pub(in super::super) fn save(&self, body: &Map<String, Value>) -> Result<Value, HttpError> {
        let entry = body
            .get("entry")
            .and_then(Value::as_str)
            .map_or("", str::trim);
        if entry.is_empty()
            || entry.encode_utf16().count() > 512
            || entry.chars().any(|c| c < '\u{20}')
        {
            return Err(HttpError::new(
                400,
                "say which page the changes are for (entry)",
            ));
        }
        let changes = body
            .get("changes")
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice);
        if changes.is_empty() {
            return Err(HttpError::new(400, "no changes"));
        }
        if changes.len() > MAX_CHANGES {
            return Err(HttpError::new(
                413,
                format!("at most {MAX_CHANGES} files at a time"),
            ));
        }
        let mut seen = BTreeSet::new();
        let mut checked = Vec::with_capacity(changes.len());
        for raw in changes {
            checked.push(self.check_change(raw, &mut seen)?);
        }
        // A move deletes its source in the same save: files are moved, never copied.
        let deleted: BTreeSet<&str> = checked
            .iter()
            .filter(|c| matches!(c.kind, ChangeKind::Delete))
            .map(|c| c.path.as_str())
            .collect();
        for c in &checked {
            if let ChangeKind::Move(from) = &c.kind
                && !deleted.contains(from.as_str())
            {
                return Err(HttpError::new(
                    400,
                    format!("moving {from} to {} must delete {from}", c.path),
                ));
            }
        }
        let title = body.get("title").and_then(Value::as_str).unwrap_or("");
        let verb = if checked.iter().all(|c| matches!(c.kind, ChangeKind::Delete)) {
            "Delete"
        } else if checked
            .iter()
            .any(|c| matches!(c.kind, ChangeKind::Move(_)))
        {
            "Move"
        } else {
            "Edit"
        };
        let subject = if title.is_empty() {
            format!("{verb} {entry}")
        } else {
            format!("{verb} {entry}: {title}")
        };
        let message = commit_message(
            &subject,
            &[("CMS-Entry", Some(entry)), ("CMS-Title", Some(title))],
        );
        let author = &self.user.author;
        if !self.review() {
            let (commit, _) =
                self.git
                    .commit_files(&self.branch, None, &checked, &message, author)?;
            return Ok(json!({ "commit": commit, "draft": null }));
        }
        let id = match self.open_draft(body.get("draft"))? {
            Some(id) => id,
            None => draft_id(entry),
        };
        let branch = format!("{DRAFTS}{id}");
        let (commit, _) =
            self.git
                .commit_files(&branch, Some(&self.branch), &checked, &message, author)?;
        Ok(json!({ "commit": commit, "draft": id }))
    }

    /// A change of a save, checked: its path, what it does, and the blob it started from.
    fn check_change(&self, raw: &Value, seen: &mut BTreeSet<String>) -> Result<Change, HttpError> {
        let empty = Map::new();
        let c = raw.as_object().unwrap_or(&empty);
        let path = c.get("path").and_then(Value::as_str).and_then(clean_path);
        let Some(path) = path.filter(|p| !seen.contains(*p)) else {
            let shown = c
                .get("path")
                .map_or_else(|| "undefined".to_owned(), Value::to_string);
            return Err(HttpError::new(400, format!("bad path {shown}")));
        };
        let path = path.to_owned();
        seen.insert(path.clone());
        if !may_edit(&self.e.areas, &self.user.edit, &path) {
            return Err(HttpError::new(403, format!("you may not change {path}")));
        }
        let base = c.get("base").map(|b| match b {
            Value::Null => None,
            Value::String(s) => Some(s.clone()),
            other => Some(other.to_string()),
        });
        if c.get("delete") == Some(&Value::Bool(true)) {
            return Ok(Change {
                path,
                base,
                kind: ChangeKind::Delete,
            });
        }
        if let Some(from) = c.get("from") {
            let from = from.as_str().and_then(clean_path).filter(|f| *f != path);
            let Some(from) = from else {
                return Err(HttpError::new(400, format!("bad move to {path}")));
            };
            if !may_edit(&self.e.areas, &self.user.edit, from) {
                return Err(HttpError::new(403, format!("you may not change {from}")));
            }
            return Ok(Change {
                path,
                base,
                kind: ChangeKind::Move(from.to_owned()),
            });
        }
        let Some(content) = c.get("content").and_then(Value::as_str) else {
            return Err(HttpError::new(400, format!("no content for {path}")));
        };
        let b64 = c.get("encoding").and_then(Value::as_str) == Some("base64");
        let size = if b64 {
            base64_size(content)
        } else {
            content.len()
        };
        if u64::try_from(size).unwrap_or(u64::MAX) > self.e.max_upload {
            return Err(HttpError::new(
                413,
                format!("{path} is larger than the upload limit"),
            ));
        }
        let bytes = if b64 {
            let compact: String = content.chars().filter(|c| !c.is_whitespace()).collect();
            let decoded = is_base64(content)
                .then(|| {
                    base64::engine::general_purpose::STANDARD_PAD_INDIFFERENT
                        .decode(compact)
                        .ok()
                })
                .flatten();
            decoded.ok_or_else(|| HttpError::new(400, format!("{path} is not base64")))?
        } else {
            content.as_bytes().to_vec()
        };
        Ok(Change {
            path,
            base,
            kind: ChangeKind::Write(bytes),
        })
    }
}
