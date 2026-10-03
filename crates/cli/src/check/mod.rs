//! `templates check` (REWRITE_PLAN.md §4.8): everything that can be known about a project's
//! layouts without rendering, reported at once.
//!
//! 1. The layout scan: legacy names, unknown names, Go-template files ([`layouts::scan`]; the
//!    files are left out and the check goes on).
//! 2. Tera: every template's syntax on its own, then all of them loaded together against
//!    `spec::FUNCS` (unknown filters, functions, tests, components, include and extends targets,
//!    blocks), with Tera's snippets ([`layouts::load`]).
//! 3. Top-level names outside the documented context of the template's role
//!    ([`layouts::context_names`]).
//! 4. The lints of [`lint`].
//! 5. Lookup coverage ([`coverage`]): the layout and base of every (page, format), every
//!    shortcode the content calls, the render hooks of content pages; misses are diagnostics.
//!
//! Output: the diagnostics (sorted by file and line, [`crate::report`]'s format) on stdout, the
//! coverage listing, then a summary line. Exit 1 when there are errors (or warnings with
//! `--deny-warnings`).

mod coverage;
mod layouts;
mod lint;

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ssg_base::diag::{Diagnostic, Position, Severity};
use ssg_layouts::{LayoutStore, TemplateRole};
use ssg_vfs::Vfs;

use crate::Exit;
use crate::args::{CheckArgs, Coverage};
use crate::report::write_diagnostic;

/// One template being checked.
pub(crate) struct CheckFile {
    /// The Tera name.
    pub name: String,
    pub role: TemplateRole,
    pub source: Arc<str>,
    /// The file as reported: relative to the project directory when it is inside it.
    pub file: Arc<Path>,
}

impl CheckFile {
    /// The position of byte `offset` of the source.
    pub fn at(&self, offset: usize) -> Position {
        let (line, col) = ssg_funcs::scan::line_col(&self.source, offset);
        Position {
            file: Arc::clone(&self.file),
            line: u32::try_from(line).unwrap_or(u32::MAX),
            col: u32::try_from(col).unwrap_or(u32::MAX),
        }
    }

    /// The file without a line.
    pub fn whole(&self) -> Position {
        Position {
            file: Arc::clone(&self.file),
            line: 0,
            col: 0,
        }
    }
}

/// How a file is reported: relative to `root` when inside it.
pub(crate) fn display_path(root: &Path, p: &Path) -> PathBuf {
    p.strip_prefix(root)
        .map_or_else(|_| p.to_owned(), Path::to_owned)
}

/// The files the check reads: user and theme templates (the embedded ones are our).
fn check_files(store: &LayoutStore, root: &Path) -> Vec<CheckFile> {
    store
        .templates()
        .filter_map(|t| {
            let file = display_path(root, t.origin.file()?);
            Some(CheckFile {
                name: t.name.to_string(),
                role: t.role.clone(),
                source: Arc::from(t.source()),
                file: Arc::from(file),
            })
        })
        .collect()
}

pub(crate) fn run(a: &CheckArgs) -> anyhow::Result<Exit> {
    let opts = a.project.load_options()?;
    let root = opts.source.clone();
    let cfg = Arc::new(ssg_config::load(&opts)?);
    let vfs = Vfs::new(&cfg)?;

    let mut diags = Vec::new();
    let store = Arc::new(layouts::scan(&vfs, &cfg, &root, &mut diags)?);
    let files = check_files(&store, &root);
    lint::run(&files, &store, &mut diags);
    let loaded = layouts::load(&store, &root, &mut diags);
    if let Some(t) = &loaded {
        layouts::context_names(t, &files, &mut diags);
    }
    let cov = match a.coverage {
        Coverage::None => None,
        mode => coverage::run(&cfg, &vfs, &store, a.project.clock, mode, &mut diags),
    };
    let diags = finish(diags);

    let stdout = io::stdout();
    let mut out = stdout.lock();
    writeln!(
        out,
        "templates check {}: {} templates ({} checked, {} embedded)",
        root.display(),
        store.templates().len(),
        files.len(),
        store.templates().len() - files.len()
    )?;
    for d in &diags {
        write_diagnostic(&mut out, d)?;
    }
    if let Some(c) = &cov {
        c.write(&mut out, a.coverage)?;
    }
    let errors = diags
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    let warnings = diags
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .count();
    writeln!(out, "{errors} error(s), {warnings} warning(s)")?;
    Ok(if errors > 0 || (a.deny_warnings && warnings > 0) {
        Exit::Failure
    } else {
        Exit::Success
    })
}

/// Sorts by position, then severity; a Tera syntax error at the position of a lint that
/// explains it becomes that lint's note; exact duplicates are dropped.
fn finish(mut diags: Vec<Diagnostic>) -> Vec<Diagnostic> {
    let explained: Vec<(Position, usize)> = diags
        .iter()
        .enumerate()
        .filter(|(_, d)| lint::EXPLAINS_SYNTAX.contains(&d.id.as_deref().unwrap_or("")))
        .filter_map(|(i, d)| Some((d.position.clone()?, i)))
        .collect();
    let mut drop = Vec::new();
    for (i, d) in diags.iter().enumerate() {
        if d.id.as_deref() != Some(layouts::SYNTAX_ID) {
            continue;
        }
        if let Some((_, j)) = explained
            .iter()
            .find(|(p, _)| Some(p) == d.position.as_ref())
        {
            drop.push((i, *j));
        }
    }
    for (i, j) in drop.iter().rev() {
        let notes = diags[*i].notes.clone();
        diags[*j].notes.extend(notes);
    }
    let dropped: Vec<usize> = drop.iter().map(|(i, _)| *i).collect();
    let mut kept: Vec<Diagnostic> = diags
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !dropped.contains(i))
        .map(|(_, d)| d)
        .collect();
    kept.sort_by(|a, b| {
        (&a.position, a.severity, &a.message, &a.id).cmp(&(
            &b.position,
            b.severity,
            &b.message,
            &b.id,
        ))
    });
    kept.dedup();
    kept
}
