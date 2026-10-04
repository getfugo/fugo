//! `[cms]`: the settings, decoded from the merged configuration and checked.
//!
//! Keys are case-insensitive like every other setting (`maxUpload`, `maxupload`); role and field
//! names are folded to lower case by the configuration loader.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ssg_base::Value;
use ssg_base::glob::{self, Case, GlobOpts, Separator};

use crate::CmsError;

/// The largest upload Cloudflare serves as a static asset (25 MiB), in MiB.
const MAX_UPLOAD_CEILING: u64 = 25;
/// The default upload limit, in MiB.
const MAX_UPLOAD_DEFAULT: u64 = 10;

/// The decoded `[cms]` table.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
struct Raw {
    path: Option<String>,
    title: Option<String>,
    workflow: Workflow,
    media: Option<String>,
    max_upload: Option<u64>,
    html: bool,
    git: RawGit,
    login: RawLogin,
    roles: BTreeMap<String, RawRole>,
    fields: BTreeMap<String, Field>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
struct RawGit {
    host: Option<String>,
    repo: Option<String>,
    branch: Option<String>,
    dir: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
struct RawLogin {
    provider: Option<String>,
    team: Option<String>,
    aud: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
struct RawRole {
    edit: Vec<String>,
    publish: bool,
}

/// Where a save goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Workflow {
    /// Saves go to a draft branch per page; a role with `publish` merges it into the branch.
    #[default]
    Review,
    /// Saves are commits to the branch.
    Direct,
}

/// The git host the API commits to. Only GitHub for now; the Worker reaches each host through
/// one small interface (`GitHost` in `worker.js`), so another host is a new implementation of it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Host {
    #[default]
    Github,
}

impl Host {
    const NAMES: &[&str] = &["github"];

    fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "github" => Some(Self::Github),
            _ => None,
        }
    }
}

/// How editors sign in. Only Cloudflare Access for now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Provider {
    #[default]
    CloudflareAccess,
}

impl Provider {
    const NAMES: &[&str] = &["cloudflare-access"];

    fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "cloudflare-access" => Some(Self::CloudflareAccess),
            _ => None,
        }
    }
}

/// An editor hint for a front matter key (`[cms.fields.<key>]`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
pub struct Field {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub widget: Option<Widget>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub help: Option<String>,
}

/// How the editor shows a front matter value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Widget {
    Text,
    Textarea,
    Number,
    Boolean,
    Date,
    Select,
    List,
    Image,
    Hidden,
}

/// The git repository the API commits to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Git {
    pub host: Host,
    /// `owner/name`.
    pub repo: String,
    pub branch: String,
    /// The project's directory in the repository (`""` at its root, else `docs/`-style with a
    /// trailing slash); `None` until [`crate::paths::repo_dir`] finds it.
    pub dir: Option<String>,
}

/// The sign-in settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Login {
    pub provider: Provider,
    /// The team domain's URL (`https://<team>.cloudflareaccess.com`), the tokens' issuer.
    pub team: String,
    /// The Access applications' audience tags; a token must carry one of them.
    pub aud: Vec<String>,
}

/// What a role may do.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Role {
    /// Globs of project-relative paths the role may create, change and delete.
    pub edit: Vec<String>,
    /// Whether the role may publish drafts (merge them into the branch).
    pub publish: bool,
}

/// The checked `[cms]` settings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CmsConfig {
    /// The editor's directory under the publish directory, without slashes (`admin`).
    pub path: String,
    /// The editor's title (default: the site's).
    pub title: Option<String>,
    pub workflow: Workflow,
    /// The directory for uploads that do not belong to a page bundle (project-relative,
    /// without a trailing slash).
    pub media: Option<String>,
    /// The largest upload, in bytes.
    pub max_upload: u64,
    /// Roles may write `.html` content files (raw HTML pages, which run on the editor's origin).
    pub html: bool,
    pub git: Git,
    pub login: Login,
    pub roles: BTreeMap<String, Role>,
    pub fields: BTreeMap<String, Field>,
}

impl CmsConfig {
    /// The `[cms]` table of a configuration tree, checked; `None` without one.
    ///
    /// # Errors
    /// A setting of the wrong type, an unknown key, or a value that is not allowed.
    pub fn from_tree(cms: Option<&Value>) -> Result<Option<Self>, CmsError> {
        let Some(v) = cms else {
            return Ok(None);
        };
        let raw: Raw = ssg_config::de::from_value(v).map_err(|e| {
            let path = e.dotted_path();
            CmsError::config(
                if path.is_empty() {
                    "cms".to_owned()
                } else {
                    format!("cms.{path}")
                },
                e.message,
            )
        })?;
        raw.check().map(Some)
    }
}

impl Raw {
    fn check(self) -> Result<CmsConfig, CmsError> {
        let path = match self.path.as_deref().map(|p| p.trim_matches('/')) {
            None => "admin".to_owned(),
            Some(p) if is_clean_dir(p) && !p.is_empty() => p.to_owned(),
            Some(p) => {
                return Err(CmsError::config(
                    "cms.path",
                    format!("{p:?} is not a directory path (letters, digits, '-', '_', '/')"),
                ));
            }
        };
        let media = match self.media.as_deref().map(|p| p.trim_matches('/')) {
            None | Some("") => None,
            Some(p) if is_clean_path(p) && !p.contains(GLOB_CHARS) => Some(p.to_owned()),
            Some(p) => {
                return Err(CmsError::config(
                    "cms.media",
                    format!("{p:?} is not a relative path inside the project"),
                ));
            }
        };
        let max_upload = match self.max_upload {
            None => MAX_UPLOAD_DEFAULT,
            Some(mb @ 1..=MAX_UPLOAD_CEILING) => mb,
            Some(mb) => {
                return Err(CmsError::config(
                    "cms.maxUpload",
                    format!(
                        "{mb} MiB: give 1 to {MAX_UPLOAD_CEILING} (Cloudflare serves static files of up to 25 MiB)"
                    ),
                ));
            }
        } * 1024
            * 1024;
        Ok(CmsConfig {
            path,
            title: self.title.filter(|t| !t.trim().is_empty()),
            workflow: self.workflow,
            media,
            max_upload,
            html: self.html,
            git: self.git.check()?,
            login: self.login.check()?,
            roles: check_roles(self.roles)?,
            fields: self.fields,
        })
    }
}

impl RawGit {
    fn check(self) -> Result<Git, CmsError> {
        let host = match self.host.as_deref() {
            None => Host::default(),
            Some(h) => Host::parse(h).ok_or_else(|| {
                CmsError::config(
                    "cms.git.host",
                    format!(
                        "{h:?} is not supported (supported: {})",
                        Host::NAMES.join(", ")
                    ),
                )
            })?,
        };
        let repo = self
            .repo
            .ok_or_else(|| CmsError::config("cms.git.repo", "missing: give \"owner/name\""))?;
        let valid_part = |s: &str| {
            !s.is_empty()
                && s != "."
                && s != ".."
                && s.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        };
        match repo.split_once('/') {
            Some((owner, name)) if valid_part(owner) && valid_part(name) => {}
            _ => {
                return Err(CmsError::config(
                    "cms.git.repo",
                    format!("{repo:?} is not \"owner/name\""),
                ));
            }
        }
        let branch = self.branch.unwrap_or_else(|| "main".to_owned());
        if !is_branch_name(&branch) {
            return Err(CmsError::config(
                "cms.git.branch",
                format!("{branch:?} is not a branch name"),
            ));
        }
        let dir = match self.dir.as_deref().map(|d| d.trim_matches('/')) {
            None => None,
            Some("") => Some(String::new()),
            Some(d) if is_clean_path(d) => Some(format!("{d}/")),
            Some(d) => {
                return Err(CmsError::config(
                    "cms.git.dir",
                    format!("{d:?} is not a relative path inside the repository"),
                ));
            }
        };
        Ok(Git {
            host,
            repo,
            branch,
            dir,
        })
    }
}

impl RawLogin {
    fn check(self) -> Result<Login, CmsError> {
        let provider = match self.provider.as_deref() {
            None => Provider::default(),
            Some(p) => Provider::parse(p).ok_or_else(|| {
                CmsError::config(
                    "cms.login.provider",
                    format!(
                        "{p:?} is not supported (supported: {})",
                        Provider::NAMES.join(", ")
                    ),
                )
            })?,
        };
        let team = self.team.ok_or_else(|| {
            CmsError::config(
                "cms.login.team",
                "missing: give the Cloudflare Access team name (<team>.cloudflareaccess.com)",
            )
        })?;
        let host = team
            .trim()
            .trim_start_matches("https://")
            .trim_end_matches('/');
        let host = if host.contains('.') {
            host.to_ascii_lowercase()
        } else {
            format!("{}.cloudflareaccess.com", host.to_ascii_lowercase())
        };
        if host.is_empty()
            || !host
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.'))
        {
            return Err(CmsError::config(
                "cms.login.team",
                format!("{team:?} is not a team name or team domain"),
            ));
        }
        let aud: Vec<String> = self
            .aud
            .into_iter()
            .map(|a| a.trim().to_owned())
            .filter(|a| !a.is_empty())
            .collect();
        if aud.is_empty() {
            return Err(CmsError::config(
                "cms.login.aud",
                "missing: give the Access application's audience (AUD) tag",
            ));
        }
        Ok(Login {
            provider,
            team: format!("https://{host}"),
            aud,
        })
    }
}

fn check_roles(raw: BTreeMap<String, RawRole>) -> Result<BTreeMap<String, Role>, CmsError> {
    if raw.is_empty() {
        return Err(CmsError::config(
            "cms.roles",
            "define at least one role, e.g. [cms.roles.owner] edit = [\"**\"] publish = true",
        ));
    }
    let mut roles = BTreeMap::new();
    for (name, r) in raw {
        let key = format!("cms.roles.{name}");
        if !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
        {
            return Err(CmsError::config(
                key,
                "a role name has letters, digits, '-' and '_' only",
            ));
        }
        if r.edit.is_empty() && !r.publish {
            return Err(CmsError::config(
                key,
                "the role may do nothing: give `edit` globs or `publish = true`",
            ));
        }
        for g in &r.edit {
            glob::compile(
                g,
                GlobOpts {
                    case: Case::Sensitive,
                    separator: Separator::Slash,
                },
            )
            .map_err(|e| CmsError::config(format!("{key}.edit"), e.to_string()))?;
            if g.starts_with('/') || g.split('/').any(|s| s == ".." || s == ".") {
                return Err(CmsError::config(
                    format!("{key}.edit"),
                    format!("{g:?}: write paths relative to the project directory"),
                ));
            }
        }
        roles.insert(
            name,
            Role {
                edit: r.edit,
                publish: r.publish,
            },
        );
    }
    Ok(roles)
}

/// The characters of the glob syntax, not allowed in the media directory's name.
const GLOB_CHARS: &[char] = &['*', '?', '[', ']', '{', '}', ',', '\\'];

/// A relative path of plain segments (no `.`, `..`, empty or hidden segments, no `\`).
pub(crate) fn is_clean_path(p: &str) -> bool {
    !p.is_empty()
        && !p.contains('\\')
        && !p.contains('\0')
        && p.split('/')
            .all(|s| !s.is_empty() && s != ".." && !s.starts_with('.'))
}

/// A directory path of URL-safe segments (`admin`, `cms/edit`).
fn is_clean_dir(p: &str) -> bool {
    p.split('/').all(|s| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
    })
}

/// A branch name git and the hosts accept (a conservative subset of `git check-ref-format`).
fn is_branch_name(b: &str) -> bool {
    !b.is_empty()
        && !b.starts_with('/')
        && !b.ends_with('/')
        && !b.ends_with(".lock")
        && !b.contains("..")
        && !b.contains("//")
        && !b.contains("@{")
        && b.split('/').all(|s| !s.starts_with('.'))
        && b.bytes().all(|c| {
            c.is_ascii_graphic() && !matches!(c, b'~' | b'^' | b':' | b'?' | b'*' | b'[' | b'\\')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branch_names() {
        for ok in ["main", "release/1.x", "cms-v2"] {
            assert!(is_branch_name(ok), "{ok}");
        }
        for bad in [
            "", "a..b", "a b", "x.lock", "/a", "a/", ".a", "a/.b", "a~1", "a:b",
        ] {
            assert!(!is_branch_name(bad), "{bad}");
        }
    }

    #[test]
    fn clean_paths() {
        assert!(is_clean_path("static/images/uploads"));
        for bad in ["", "../x", "a/../b", "a//b", ".git", "a/.env", "a\\b"] {
            assert!(!is_clean_path(bad), "{bad}");
        }
    }
}
