//! Content adapters (phase A4b; the Go implementation's `content_map.go` `addPagesFromGoTmplFi`
//! and `pagesfromdata/pagesfromgotmpl.go`): every `content/**/_content.html` runs before
//! the model is built, and the pages and resources it adds join the content files.
//!
//! The adapters render with a session of the model of the content files alone (Go's site is
//! "not fully initialized" while they run): its templates, partials and site functions
//! (`get_remote`, `partial`, `i18n`, …) work, `site` has no page lists. Each adapter runs for
//! its own language (file name, mount, else the default language), then, when it called
//! `enable_all_languages()`, once for every other language in language order; its runs share
//! its store. Adapters run one after the other in discovery order (directory, then language),
//! so the added pages and their ids do not depend on the thread count, and of two adapters
//! that add one path the later one wins (`ssg_site::assemble`). The model is then
//! assembled again with the added pages and resources, and what the adapters logged is
//! reported with the build.

use std::sync::Arc;

use ssg_base::diag::Diagnostic;
use ssg_config::Config;
use ssg_render::{Project, RenderOptions, Session};
use ssg_site::{Added, Captured, LoadModelOptions, Model, ModelError};

use crate::{BuildError, RenderPool};

/// Runs the content adapters of `captured` and builds the model with what they added; also
/// returns the diagnostics of their runs.
pub(crate) fn assemble_with_adapters(
    cfg: &Arc<Config>,
    project: &Project,
    captured: Captured,
    opts: &LoadModelOptions,
    render: &RenderOptions,
    pool: &RenderPool,
) -> Result<(Model, Vec<Diagnostic>), BuildError> {
    let files =
        pool.run(|| ssg_site::assemble(Arc::clone(cfg), captured.clone(), Added::default(), opts))?;
    let session = pool.run(|| Session::new(project.clone(), Arc::new(files), render))?;
    let mark = session.diagnostics().len();
    let runs = Arc::clone(&session.handles().adapters);
    let mut added = Added::default();
    for (i, a) in captured.adapters().iter().enumerate() {
        let path = &a.file.abs;
        let source = std::fs::read_to_string(path).map_err(|source| ModelError::Read {
            path: path.clone(),
            source,
        })?;
        if let Some((line, marker)) = ssg_layouts::go_marker(&source) {
            return Err(ModelError::Adapter {
                path: path.clone(),
                message: format!(
                    "line {line}: Go template syntax `{marker}`: content adapters are Tera \
                     templates; convert the file (`add_page(page={{…}})`, \
                     https://github.com/getfugo/fugo/blob/main/docs/rust-port/template-api.md \
                     gives the Go template functions with their Tera names)"
                ),
            }
            .into());
        }
        let name = path.display().to_string();
        let dir = a.info.key.clone();
        let first = runs.begin(i, a.lang, dir.clone());
        session.render_adapter(&name, &source, a.lang, first)?;
        let run = runs.take(first);
        let all_languages = run.all_languages;
        added.pages.extend(run.pages);
        added.resources.extend(run.resources);
        if all_languages {
            for lang in cfg.sites.ids().filter(|&l| l != a.lang) {
                let id = runs.begin(i, lang, dir.clone());
                session.render_adapter(&name, &source, lang, id)?;
                let run = runs.take(id);
                added.pages.extend(run.pages);
                added.resources.extend(run.resources);
            }
        }
    }
    let diagnostics = session.diagnostics().since(mark);
    drop(session);
    let model = pool.run(|| ssg_site::assemble(Arc::clone(cfg), captured, added, opts))?;
    Ok((model, diagnostics))
}
