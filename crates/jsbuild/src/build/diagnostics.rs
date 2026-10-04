//! Rolldown's errors as esbuild reports them: sorted, with their position and the line they are on.

use super::*;

/// A failed build's errors: the plugin's own errors carry positions rolldown drops, so they
/// replace rolldown's reports of them.
pub(super) fn errors(ctx: &BuildContext, diags: &[BuildDiagnostic]) -> Vec<Diagnostic> {
    let recorded = ctx.take_errors();
    // rolldown reports a hook error as a plugin error, or for `load` as a failed load naming
    // the plugin.
    let reports_ours = |d: &BuildDiagnostic| {
        d.plugin().as_deref() == Some(plugin::PLUGIN_NAME)
            || d.to_string()
                .contains(&format!("`{}`", plugin::PLUGIN_NAME))
    };
    let mut out: Vec<Diagnostic> = diags
        .iter()
        .filter(|d| recorded.is_empty() || !reports_ours(d))
        .map(|d| diagnostic(ctx, d))
        .collect();
    out.splice(0..0, recorded);
    let mut out = sorted(out);
    if out.is_empty() {
        out.push(Diagnostic {
            text: "the build failed".to_owned(),
            plugin: None,
            position: None,
        });
    }
    out
}

/// Errors in esbuild's order: by file and position (rolldown reports them as modules finish);
/// errors without a position keep their order, after the others.
pub(super) fn sorted(mut diags: Vec<Diagnostic>) -> Vec<Diagnostic> {
    diags.sort_by(|a, b| match (&a.position, &b.position) {
        (Some(a), Some(b)) => (&a.file, a.line, a.column).cmp(&(&b.file, b.line, b.column)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    diags
}

/// A rolldown diagnostic located in a real file, with esbuild's byte columns.
pub(super) fn diagnostic(ctx: &BuildContext, d: &BuildDiagnostic) -> Diagnostic {
    let mut text = d.to_string();
    if d.kind().to_string() == "UNRESOLVED_IMPORT"
        && let Some(spec) = text.split('\'').nth(1)
    {
        // esbuild's wording, which sites and docs know.
        text = format!("Could not resolve {}", serde_json::Value::from(spec));
    }
    let position = d
        .to_diagnostic()
        .get_primary_location()
        .and_then(|(file, line, column, _)| {
            let path = d
                .id()
                .filter(|id| Path::new(id).is_absolute())
                .map_or_else(|| ctx.working_dir.join(&file), PathBuf::from);
            let line = u32::try_from(line).ok()?;
            let is_entry = path.to_string_lossy() == ctx.entry_id;
            let source = if is_entry {
                ctx.entry_code.clone()
            } else {
                std::fs::read_to_string(&path).ok()?
            };
            let mut column = byte_column(&source, line, column);
            if is_entry && line == 1 {
                column = column.saturating_sub(ctx.entry_prefix);
            }
            path.is_file().then_some(Position {
                file: path,
                line,
                column,
            })
        });
    Diagnostic {
        text,
        plugin: d.plugin(),
        position,
    }
}

/// The byte column of a UTF-16 column on a 1-based line.
pub(super) fn byte_column(source: &str, line: u32, utf16_column: usize) -> u32 {
    let text = source
        .split('\n')
        .nth(usize::try_from(line.saturating_sub(1)).unwrap_or(usize::MAX))
        .unwrap_or_default();
    let mut units = 0;
    let mut bytes = 0;
    for ch in text.chars() {
        if units >= utf16_column {
            break;
        }
        units += ch.len_utf16();
        bytes += ch.len_utf8();
    }
    u32::try_from(bytes).unwrap_or(u32::MAX)
}
