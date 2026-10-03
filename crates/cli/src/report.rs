//! Printing diagnostics and errors: `ERROR <file>:<line>:<col>: <message>` (then `WARN`,
//! `INFO`), notes indented below; the same format for builds and `templates check`.

use std::io::{self, Write};

use ssg_base::diag::{Diagnostic, Severity};
use ssg_build::BuildError;

/// The label of a severity (Go's log levels).
fn label(s: Severity) -> &'static str {
    match s {
        Severity::Error => "ERROR",
        Severity::Warning => "WARN ",
        Severity::Info => "INFO ",
    }
}

/// Writes one diagnostic: `ERROR [id] <position>: <message>`, each note line indented.
pub(crate) fn write_diagnostic(out: &mut dyn Write, d: &Diagnostic) -> io::Result<()> {
    write!(out, "{}", label(d.severity))?;
    if let Some(id) = &d.id {
        write!(out, " [{id}]")?;
    }
    if let Some(p) = &d.position {
        write!(out, " {p}")?;
    }
    writeln!(out, ": {}", d.message)?;
    for n in &d.notes {
        for line in n.lines() {
            writeln!(out, "      {line}")?;
        }
    }
    Ok(())
}

/// Writes diagnostics to stderr.
pub(crate) fn diagnostics(ds: &[Diagnostic]) {
    let stderr = io::stderr();
    let mut out = stderr.lock();
    for d in ds {
        let _ = write_diagnostic(&mut out, d);
    }
}

/// A failed build (`build`, and `server`'s builds): its diagnostics and a summary line, or the
/// error, whose message carries its position and Tera's snippet.
pub(crate) fn build_failed(e: &BuildError) {
    match e {
        BuildError::Diagnostics(ds) => {
            diagnostics(ds);
            let errors = ds.iter().filter(|d| d.severity == Severity::Error).count();
            eprintln!("ERROR build failed: {errors} error(s)");
        }
        e => eprintln!("ERROR build failed: {e}"),
    }
}

/// A command that could not run: the error and the causes its message does not already
/// contain.
pub(crate) fn fatal(e: &anyhow::Error) {
    let top = e.to_string();
    let mut msg = format!("ERROR {top}");
    for cause in e.chain().skip(1) {
        let c = cause.to_string();
        if !msg.contains(&c) {
            msg.push_str("\n      caused by: ");
            msg.push_str(&c);
        }
    }
    eprintln!("{msg}");
}
