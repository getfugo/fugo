//! The template contract (REWRITE_PLAN.md §4.1 item 3, §4.8): every converted template must load
//! into a Tera instance that knows exactly the names of `ssg_funcs::spec::FUNCS`, and every
//! call must use declared kwargs.
//!
//! Tera validates names (filters, functions, tests, components, include targets, blocks) when
//! templates are added, but kwargs only when a call runs. [`scan_calls`] (the tokenizer of
//! `ssg_funcs::scan`, shared with `templates check`) therefore reads the calls of
//! a template and [`kwarg_findings`] checks them against the spec.
//!
//! Template sets: one per site under `sites/<site>/` (its `layouts/**` by relative name,
//! `assets/**` as `assets/<rel>`, its content adapters `content/**/_content.html` as
//! `content/<rel>`) and one more per docs patch variant (`patches/<variant>/**`
//! overlaid on the site by path), each plus the embedded templates of
//! `crates/layouts/embedded/**` (as `_embedded/<rel>`, with an empty stub for every
//! `spec::EMBEDDED_TEMPLATES` name not yet written).

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub use ssg_funcs::scan::{Call, scan_calls};
use ssg_funcs::spec;
use tera::Tera;

use crate::fixture::repo_dir;

/// One template: its Tera name, the file it came from and its source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateSource {
    pub name: String,
    pub path: PathBuf,
    pub source: String,
}

/// The templates that are loaded together, e.g. `docs` or `docs+i01`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TemplateSet {
    pub label: String,
    pub templates: Vec<TemplateSource>,
}

/// A contract violation found without Tera.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub path: PathBuf,
    pub line: usize,
    pub message: String,
}

impl fmt::Display for Finding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.path.display(), self.line, self.message)
    }
}

/// `docs/rust-port/template-api.md`.
#[must_use]
pub fn template_api_path() -> PathBuf {
    repo_dir().join("docs/rust-port/template-api.md")
}

/// The contract instance: `_embedded/` as fallback prefix, a placeholder for every `FUNCS` name,
/// then one `add_raw_templates` call with `templates` plus empty stubs for the embedded templates
/// they do not contain.
///
/// # Errors
/// Tera's load error: syntax, unknown names, inheritance, include cycles.
pub fn contract_instance(templates: &[TemplateSource]) -> Result<Tera, tera::Error> {
    let mut tera = Tera::default();
    tera.set_fallback_prefixes([spec::EMBEDDED_PREFIX])?;
    ssg_funcs::register_placeholders(&mut tera);
    let stubs: Vec<(String, &str)> = spec::EMBEDDED_TEMPLATES
        .iter()
        .map(|name| format!("{}{name}", spec::EMBEDDED_PREFIX))
        .filter(|name| !templates.iter().any(|t| &t.name == name))
        .map(|name| (name, ""))
        .collect();
    tera.add_raw_templates(
        templates
            .iter()
            .map(|t| (t.name.as_str(), t.source.as_str()))
            .chain(stubs.iter().map(|(n, s)| (n.as_str(), *s))),
    )?;
    Ok(tera)
}

/// Every template set of the checkout (see the module docs), sorted by label.
///
/// # Errors
/// I/O errors reading `sites` or `crates/layouts/embedded`.
pub fn template_sets() -> io::Result<Vec<TemplateSet>> {
    let root = repo_dir();
    let embedded = read_tree(&root.join("crates/layouts/embedded"), spec::EMBEDDED_PREFIX)?;
    let mut sets = Vec::new();
    let sites = root.join("sites");
    for site in sorted_dirs(&sites)? {
        let label = file_name(&site);
        let mut base = read_tree(&site.join("layouts"), "")?;
        base.extend(read_tree(&site.join("assets"), "assets/")?);
        base.extend(
            read_tree(&site.join("content"), "content/")?
                .into_iter()
                .filter(|t| t.name.ends_with("/_content.html")),
        );
        for variant in sorted_dirs(&site.join("patches"))? {
            let mut templates = base.clone();
            for patch in read_tree(&variant, "")? {
                let name = patch
                    .name
                    .strip_prefix("layouts/")
                    .map_or_else(|| patch.name.clone(), str::to_owned);
                templates.retain(|t| t.name != name);
                templates.push(TemplateSource { name, ..patch });
            }
            templates.extend(embedded.iter().cloned());
            sets.push(TemplateSet {
                label: format!("{label}+{}", file_name(&variant)),
                templates,
            });
        }
        base.extend(embedded.iter().cloned());
        sets.push(TemplateSet {
            label,
            templates: base,
        });
    }
    if sets.is_empty() {
        sets.push(TemplateSet {
            label: "embedded".to_owned(),
            templates: embedded,
        });
    }
    sets.sort_by(|a, b| a.label.cmp(&b.label));
    Ok(sets)
}

/// The kwargs findings of one template: calls of `FUNCS` names with an undeclared kwarg or
/// without a required one. Names unknown to `FUNCS` are left to Tera's own validation.
#[must_use]
pub fn kwarg_findings(t: &TemplateSource) -> Vec<Finding> {
    ssg_funcs::scan::kwarg_errors(&t.source)
        .into_iter()
        .map(|(call, message)| Finding {
            path: t.path.clone(),
            line: call.line,
            message,
        })
        .collect()
}

fn sorted_dirs(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut dirs = Vec::new();
    match fs::read_dir(dir) {
        Ok(entries) => {
            for e in entries {
                let e = e?;
                if e.file_type()?.is_dir() {
                    dirs.push(e.path());
                }
            }
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    dirs.sort();
    Ok(dirs)
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Every file under `dir` (absent: none) as a template named `prefix` + its slash-separated
/// relative path, sorted by name.
fn read_tree(dir: &Path, prefix: &str) -> io::Result<Vec<TemplateSource>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(d) = stack.pop() {
        for sub in sorted_dirs(&d)? {
            stack.push(sub);
        }
        let entries = match fs::read_dir(&d) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        for e in entries {
            let e = e?;
            if e.file_type()?.is_file() {
                let path = e.path();
                let rel = path
                    .strip_prefix(dir)
                    .expect("walked below dir")
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                out.push(TemplateSource {
                    name: format!("{prefix}{rel}"),
                    source: fs::read_to_string(&path)?,
                    path,
                });
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}
