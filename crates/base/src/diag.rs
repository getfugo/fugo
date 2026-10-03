//! Diagnostics: positioned errors, warnings and notes, aggregated across a build.
//!
//! Diagnostics are collected from parallel work into one [`Diagnostics`] sink, de-duplicated by
//! id (or message) and reported sorted, so the report does not depend on scheduling.

use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

/// A position in a source file (1-based line and column; 0 means unknown).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Position {
    pub file: Arc<Path>,
    pub line: u32,
    pub col: u32,
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.file.display())?;
        if self.line > 0 {
            write!(f, ":{}", self.line)?;
            if self.col > 0 {
                write!(f, ":{}", self.col)?;
            }
        }
        Ok(())
    }
}

/// How serious a diagnostic is. Orders from most to least serious.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Info => "info",
        })
    }
}

/// One diagnostic.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Diagnostic {
    pub severity: Severity,
    /// A stable id (`warnf`/`erroridf` ids, deprecation keys); `ignoreLogs` matches it.
    pub id: Option<String>,
    pub message: String,
    pub position: Option<Position>,
    pub notes: Vec<String>,
}

impl Diagnostic {
    /// A diagnostic without id, position or notes.
    #[must_use]
    pub fn new(severity: Severity, message: impl Into<String>) -> Self {
        Self {
            severity,
            id: None,
            message: message.into(),
            position: None,
            notes: Vec::new(),
        }
    }

    /// An error.
    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        Self::new(Severity::Error, message)
    }

    /// A warning.
    #[must_use]
    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(Severity::Warning, message)
    }

    /// An informational message.
    #[must_use]
    pub fn info(message: impl Into<String>) -> Self {
        Self::new(Severity::Info, message)
    }

    /// With a stable id.
    #[must_use]
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// With a position.
    #[must_use]
    pub fn at(mut self, position: Position) -> Self {
        self.position = Some(position);
        self
    }

    /// With a note appended.
    #[must_use]
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// The de-duplication key: the id when there is one, else the message.
    fn dedupe_key(&self) -> (Severity, &str) {
        (self.severity, self.id.as_deref().unwrap_or(&self.message))
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.severity)?;
        if let Some(id) = &self.id {
            write!(f, " [{id}]")?;
        }
        if let Some(p) = &self.position {
            write!(f, " {p}")?;
        }
        write!(f, ": {}", self.message)?;
        for n in &self.notes {
            write!(f, "\n  note: {n}")?;
        }
        Ok(())
    }
}

/// A thread-safe collector of diagnostics.
#[derive(Debug, Default)]
pub struct Diagnostics {
    items: Mutex<Vec<Diagnostic>>,
    /// Ids (lower case) whose diagnostics are dropped (`ignoreLogs`).
    ignored: BTreeSet<String>,
}

impl Diagnostics {
    /// A collector that drops diagnostics whose id is in `ignore_logs` (compared ignoring ASCII
    /// case, as Go does).
    #[must_use]
    pub fn new<I, S>(ignore_logs: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self {
            items: Mutex::new(Vec::new()),
            ignored: ignore_logs
                .into_iter()
                .map(|s| s.as_ref().to_ascii_lowercase())
                .collect(),
        }
    }

    /// Records `d` unless its id is ignored.
    pub fn push(&self, d: Diagnostic) {
        if d.id
            .as_ref()
            .is_some_and(|id| self.ignored.contains(&id.to_ascii_lowercase()))
        {
            return;
        }
        self.items
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(d);
    }

    /// The number of diagnostics recorded so far (a mark for [`since`](Self::since)).
    #[must_use]
    pub fn len(&self) -> usize {
        self.items
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// Whether nothing was recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The diagnostics recorded after the first `mark` ones, in recording order.
    #[must_use]
    pub fn since(&self, mark: usize) -> Vec<Diagnostic> {
        self.items
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(mark..)
            .map(<[Diagnostic]>::to_vec)
            .unwrap_or_default()
    }

    /// Whether any error was recorded.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.items
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    /// The recorded diagnostics: sorted (severity, position, message, …) and de-duplicated by
    /// (severity, id or message), keeping the first in that order.
    #[must_use]
    pub fn report(&self) -> Vec<Diagnostic> {
        let mut items = self
            .items
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        items.sort_by(|a, b| {
            (a.severity, &a.position, &a.message, &a.id, &a.notes).cmp(&(
                b.severity,
                &b.position,
                &b.message,
                &b.id,
                &b.notes,
            ))
        });
        let mut seen = BTreeSet::new();
        items.retain(|d| {
            let (s, k) = d.dedupe_key();
            seen.insert((s, k.to_owned()))
        });
        items
    }
}
