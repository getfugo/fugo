//! Errors of the layout scan and of loading the templates into Tera.

use std::fmt;
use std::path::PathBuf;

use ssg_base::diag::Position;
use ssg_vfs::VfsError;

/// Why the layouts cannot be used.
#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    /// Layout files with names or contents the engine does not accept (every one found, sorted
    /// by position).
    #[error("{}", Issues(.0))]
    Layouts(Vec<LayoutIssue>),
    #[error(transparent)]
    Vfs(#[from] VfsError),
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// Tera's load error: syntax, unknown names, inheritance, include cycles.
    #[error("loading the templates: {0}")]
    Tera(#[source] tera::Error),
}

struct Issues<'a>(&'a [LayoutIssue]);

impl fmt::Display for Issues<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} layout file(s) cannot be used:", self.0.len())?;
        for i in self.0 {
            write!(f, "\n  {i}")?;
        }
        Ok(())
    }
}

/// One unusable layout file.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LayoutIssue {
    /// The file (the Tera name for embedded templates), with the line of a content issue.
    pub position: Position,
    pub kind: IssueKind,
}

impl fmt::Display for LayoutIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.position, self.kind)
    }
}

/// What is wrong with a layout file.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum IssueKind {
    /// A pre-v0.146 name; `rename` is the v0.146 name (or pattern) to use instead.
    LegacyName { rename: String },
    /// A name the engine does not know what to do with.
    UnknownName { reason: String },
    /// Go-template syntax in a template.
    GoTemplate { marker: String },
}

impl fmt::Display for IssueKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LegacyName { rename } => write!(
                f,
                "legacy layout name; rename it to {rename} (v0.146 layout names are required; \
                 `{} templates check` lists every legacy name)",
                ssg_base::APP_NAME
            ),
            Self::UnknownName { reason } => f.write_str(reason),
            Self::GoTemplate { marker } => write!(
                f,
                "Go template syntax `{marker}`: layouts must be Tera templates; run \
                 `{} templates check` and convert the file \
                 (https://github.com/getfugo/fugo/blob/main/docs/rust-port/template-api.md \
                 gives the Go template functions with their Tera names)",
                ssg_base::APP_NAME
            ),
        }
    }
}
