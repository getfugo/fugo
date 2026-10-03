//! The layout scan, the Tera load and the context names.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context as _;
use ssg_base::diag::{Diagnostic, Position};
use ssg_config::Config;
use ssg_funcs::scan::{self, Tok};
use ssg_funcs::spec::{CONTEXTS, HOOK_FIELDS, RenderRole};
use ssg_layouts::{
    IssueKind, LayoutEnv, LayoutIssue, LayoutSource, LayoutStore, Origin, Selections,
    StandaloneKind, TemplateError, TemplateRole, Templates,
};
use ssg_vfs::{Component, Module, Vfs};

use super::{CheckFile, display_path};

/// The id of Tera syntax errors (see `super::finish`).
pub(crate) const SYNTAX_ID: &str = "tera-syntax";

/// The layout files of the project and its themes, plus the embedded templates (what
/// `LayoutStore::scan` reads).
fn read_sources(vfs: &Vfs) -> anyhow::Result<Vec<LayoutSource>> {
    let mut sources = Vec::new();
    for f in vfs.walk(Component::Layouts)? {
        let module = vfs
            .mounts()
            .get(usize::from(f.mount_idx))
            .map_or(Module::Project, |m| m.module);
        let origin = match module {
            Module::Project => Origin::User(f.abs.clone()),
            Module::Theme(n) => {
                Origin::Theme(u8::try_from(n + 1).unwrap_or(u8::MAX), f.abs.clone())
            }
        };
        let source = std::fs::read_to_string(&f.abs)
            .with_context(|| format!("reading {}", f.abs.display()))?;
        sources.push(LayoutSource {
            rel: f.rel,
            origin,
            source,
        });
    }
    sources.extend(
        ssg_layouts::embedded::TEMPLATES
            .iter()
            .map(|(rel, src)| LayoutSource {
                rel: (*rel).to_owned(),
                origin: Origin::Embedded,
                source: (*src).to_owned(),
            }),
    );
    Ok(sources)
}

/// The file a layout source is reported under by the store (`LayoutIssue` positions).
fn issue_file(s: &LayoutSource) -> PathBuf {
    match s.origin.file() {
        Some(p) => p.to_owned(),
        None => PathBuf::from(format!("{}{}", s.origin.prefix(), s.rel)),
    }
}

fn issue_diagnostic(root: &Path, i: &LayoutIssue) -> Diagnostic {
    let id = match i.kind {
        IssueKind::LegacyName { .. } => "legacy-name",
        IssueKind::UnknownName { .. } => "unknown-name",
        IssueKind::GoTemplate { .. } => "go-template",
    };
    let mut position = i.position.clone();
    position.file = Arc::from(display_path(root, &position.file));
    Diagnostic::error(i.kind.to_string())
        .with_id(id)
        .at(position)
}

/// The layout store; files the scan refuses (legacy names, unknown names, Go templates) are
/// reported and left out, so the rest can still be checked.
pub(crate) fn scan(
    vfs: &Vfs,
    cfg: &Config,
    root: &Path,
    diags: &mut Vec<Diagnostic>,
) -> anyhow::Result<LayoutStore> {
    let mut sources = read_sources(vfs)?;
    loop {
        match LayoutStore::from_sources(LayoutEnv::from_config(cfg), sources.clone()) {
            Ok(store) => return Ok(store),
            Err(TemplateError::Layouts(issues)) => {
                let bad: BTreeSet<&Path> = issues.iter().map(|i| &*i.position.file).collect();
                let before = sources.len();
                sources.retain(|s| !bad.contains(issue_file(s).as_path()));
                diags.extend(issues.iter().map(|i| issue_diagnostic(root, i)));
                if sources.len() == before {
                    anyhow::bail!("the layouts cannot be scanned");
                }
            }
            Err(e) => return Err(e.into()),
        }
    }
}

/// Where Tera names are reported: the file of every template (and alias) of the store.
fn tera_files(store: &LayoutStore, root: &Path) -> BTreeMap<String, Arc<Path>> {
    let mut m = BTreeMap::new();
    for t in store.templates() {
        let file: Arc<Path> = match t.origin.file() {
            Some(p) => Arc::from(display_path(root, p)),
            None => Arc::from(Path::new(t.name.as_str())),
        };
        m.insert(t.name.to_string(), Arc::clone(&file));
        m.insert(t.render_name().to_string(), file);
    }
    m
}

/// A Tera report (`error: <message>\n --> <name>:<line>:<col>\n<snippet>`) as a diagnostic.
fn report_diagnostic(
    report: &str,
    id: &str,
    files: &BTreeMap<String, Arc<Path>>,
) -> (Diagnostic, Option<String>) {
    let mut lines = report.lines();
    let first = lines.next().unwrap_or_default();
    let message = first.strip_prefix("error: ").unwrap_or(first);
    let mut d = Diagnostic::error(message).with_id(id);
    let rest: Vec<&str> = lines.collect();
    let locus = rest
        .iter()
        .find_map(|l| l.trim_start().strip_prefix("--> "))
        .and_then(|l| {
            let mut parts = l.rsplitn(3, ':');
            let col = parts.next()?.parse::<u32>().ok()?;
            let line = parts.next()?.parse::<u32>().ok()?;
            Some((parts.next()?.to_owned(), line, col))
        });
    let mut name = None;
    if let Some((n, line, col)) = locus {
        let file = files
            .get(&n)
            .cloned()
            .unwrap_or_else(|| Arc::from(Path::new(&n)));
        d = d.at(Position { file, line, col });
        name = Some(n);
    }
    let snippet: Vec<&str> = rest
        .into_iter()
        .filter(|l| !l.trim_start().starts_with("--> "))
        .collect();
    if !snippet.is_empty() {
        d = d.note(snippet.join("\n"));
    }
    (d, name)
}

/// The Tera load error as diagnostics, with the templates they point at.
fn tera_errors(
    e: &tera::Error,
    files: &BTreeMap<String, Arc<Path>>,
) -> Vec<(Diagnostic, Option<String>)> {
    match e.kind() {
        // Unknown names and blocks: every report, joined by blank lines.
        tera::ErrorKind::Msg(m) if m.starts_with("error: ") => m
            .split("\n\nerror: ")
            .map(|r| {
                let r = if r.starts_with("error: ") {
                    r.to_owned()
                } else {
                    format!("error: {r}")
                };
                report_diagnostic(&r, "tera-name", files)
            })
            .collect(),
        tera::ErrorKind::SyntaxError(_) => {
            vec![report_diagnostic(&e.to_string(), SYNTAX_ID, files)]
        }
        tera::ErrorKind::MissingParent { current, .. }
        | tera::ErrorKind::CircularExtend { tpl: current, .. }
        | tera::ErrorKind::CircularInclude { tpl: current, .. } => {
            let mut d = Diagnostic::error(e.to_string()).with_id("tera-load");
            if let Some(file) = files.get(current) {
                d = d.at(Position {
                    file: Arc::clone(file),
                    line: 0,
                    col: 0,
                });
            }
            vec![(d, Some(current.clone()))]
        }
        _ => vec![(Diagnostic::error(e.to_string()).with_id("tera-load"), None)],
    }
}

/// Loads the templates as the build does, against the `FUNCS` placeholders. Syntax is checked
/// per template first (Tera stops at the first syntax error). Templates with errors are then
/// loaded empty, so the names of all the others are checked and the loaded instance serves the
/// context check; only the first load's errors are reported (later ones follow from the emptied
/// templates). `None` when no load succeeds.
pub(crate) fn load(
    store: &LayoutStore,
    root: &Path,
    diags: &mut Vec<Diagnostic>,
) -> Option<Templates> {
    let files = tera_files(store, root);
    let mut broken = BTreeSet::new();
    for t in store.templates() {
        let mut tera = tera::Tera::default();
        if let Err(e) = tera.add_raw_template(t.name.as_str(), t.source())
            && let tera::ErrorKind::SyntaxError(_) = e.kind()
        {
            diags.push(report_diagnostic(&e.to_string(), SYNTAX_ID, &files).0);
            broken.insert(t.name.to_string());
        }
    }
    for attempt in 0..8 {
        let sources = store.templates().map(|t| LayoutSource {
            rel: t.path().trim_start_matches('/').to_owned(),
            origin: t.origin.clone(),
            source: if broken.contains(t.name.as_str()) {
                String::new()
            } else {
                t.source().to_owned()
            },
        });
        let reloaded = match LayoutStore::from_sources(store.env().clone(), sources) {
            Ok(s) => Arc::new(s),
            Err(e) => {
                diags.push(Diagnostic::error(format!("reloading the layouts: {e}")));
                return None;
            }
        };
        let e = match ssg_layouts::load(reloaded, &Selections::new(), &|t| {
            ssg_funcs::register_placeholders(t);
        }) {
            Ok(t) => return Some(t),
            Err(TemplateError::Tera(e)) => e,
            Err(e) => {
                diags.push(Diagnostic::error(e.to_string()).with_id("tera-load"));
                return None;
            }
        };
        let mut progress = false;
        for (d, name) in tera_errors(&e, &files) {
            if attempt == 0 {
                diags.push(d);
            }
            if let Some(n) = name {
                progress |= broken.insert(n);
            }
        }
        if !progress {
            return None;
        }
    }
    None
}

/// The documented top-level names of a template's role (`None`: not checked — partials see
/// their kwargs, components only their arguments).
fn documented(role: &TemplateRole) -> Option<(RenderRole, Vec<&'static str>)> {
    let r = match role {
        TemplateRole::Layout { .. } | TemplateRole::Base { .. } => RenderRole::LayoutJob,
        TemplateRole::Standalone(StandaloneKind::Alias) => RenderRole::Alias,
        TemplateRole::Standalone(StandaloneKind::SitemapIndex) => RenderRole::SitemapIndex,
        TemplateRole::Standalone(_) => RenderRole::Standalone,
        TemplateRole::Shortcode { .. } => RenderRole::Shortcode,
        TemplateRole::Hook { .. } => RenderRole::RenderHook,
        TemplateRole::Partial { .. } => return None,
    };
    let ctx = CONTEXTS.iter().find(|c| c.role == r)?;
    let mut names: Vec<&'static str> = ctx.names.iter().map(|n| n.name).collect();
    if let TemplateRole::Hook { kind, .. } = role {
        for (kinds, fields) in HOOK_FIELDS {
            if kinds.split(", ").any(|k| k == kind.as_str()) {
                names.extend(fields.iter().copied());
            }
        }
    }
    Some((r, names))
}

/// The offset of the first use of top-level name `name` in `source`.
fn first_use(source: &str, name: &str) -> Option<usize> {
    scan::tags(source).into_iter().find_map(|tag| {
        tag.toks.iter().enumerate().find_map(|(i, (t, off))| {
            let after_dot = i > 0 && matches!(tag.toks[i - 1].0, Tok::Punct("." | "?."));
            (*t == Tok::Ident(name) && !after_dot).then_some(*off)
        })
    })
}

/// Top-level names (Tera's `get_template_variables`: the template, its parents and includes)
/// outside the documented context of the template's role.
pub(crate) fn context_names(t: &Templates, files: &[CheckFile], diags: &mut Vec<Diagnostic>) {
    for f in files {
        let Some((role, known)) = documented(&f.role) else {
            continue;
        };
        let Some(info) = t.store().template(&f.name) else {
            continue;
        };
        let Ok(vars) = t.tera().get_template_variables(info.render_name().as_str()) else {
            continue;
        };
        let mut unknown: Vec<&str> = vars.into_iter().filter(|v| !known.contains(v)).collect();
        unknown.sort_unstable();
        for name in unknown {
            let position = first_use(&f.source, name).map_or_else(|| f.whole(), |o| f.at(o));
            diags.push(
                Diagnostic::warning(format!(
                    "`{name}` is not in the context of a {} template: it is undefined when \
                     rendered (known: {})",
                    CONTEXTS
                        .iter()
                        .find(|c| c.role == role)
                        .map_or("", |c| c.title),
                    known.join(", ")
                ))
                .with_id("context-name")
                .at(position),
            );
        }
    }
}
