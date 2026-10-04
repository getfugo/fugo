//! Build orchestration (REWRITE_PLAN.md §2.6, §3): configuration → file system → model →
//! layouts → render session → static copy → render waves → deferred wave → resources.
//!
//! [`build`] runs the phases of §3.1 in order:
//!
//! | Phase | What | Where |
//! |---|---|---|
//! | A1–B6 | configuration, mounts, discovery, parsing, the site model | `config`, `vfs`, `site` |
//! | A4b | content adapters (`_content.html`), with a session of the content files' model; the model again with what they added | `adapters` |
//! | B7 | layout scan and selection; `Session::new` loads Tera once (site functions, i18n) | `layouts`, `render` |
//! | C1, D | content of every page in every hook variant, then the frozen views | `render` (render pool) |
//! | E1 | static files into the sink (rendered outputs win conflicts) | `publish` |
//! | E2 | wave 1: one sub-wave per language, in language order | `waves` (render pool) |
//! | E3 | wave 2: pagers 2..N, `page/1/` aliases, the language redirect | `waves` |
//! | E5 | `defer(...)` templates once per key, post-process fields → `patch_held` | `deferred` (render pool) |
//! | E6 | resources named by URL tokens (and eager bundle files), processed images | `publish_resources` |
//! | E6b | the CMS editor of `[cms]`: its page, content index and API Worker | `ssg_cms` |
//! | E7 | sorted, de-duplicated diagnostics; errors fail the build | `build` |
//!
//! **Structure dump.** With [`STRUCTURE_ENV`] (`FUGO_STRUCTURE_OUT=<file>`) set, the waves
//! record every job and a successful build writes the Go structure oracle's dump of what it
//! did (`structure`); without it nothing is recorded.
//!
//! **Determinism.** Every parallel phase runs on one render pool (`stack_size(16 MiB)`, the
//! thread count of [`BuildRequest::threads`], else `RAYON_NUM_THREADS`, else the CPUs) and
//! collects into indexed or sorted collections; target collisions are resolved from the jobs'
//! targets before rendering, by [`JobOrder`] (a page beats an alias; among equals the later job
//! wins; losers render for what they record but are not published), so the output tree does
//! not depend on the thread count.

#![forbid(unsafe_code)]

mod adapters;
mod deferred;
mod structure;
mod waves;

pub use structure::ENV as STRUCTURE_ENV;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ssg_base::diag::Diagnostic;
use ssg_base::paths::OutputPath;
use ssg_base::url::{BaseUrl, UrlRef};
use ssg_base::{Clock, Idx, LangIdx, Sink};
use ssg_config::{CliOverrides, Config, ConfigError, LoadOptions};
use ssg_layouts::{LayoutStore, TemplateError};
use ssg_publish::{
    DiskSink, MemorySink, PublishError, PublishSettings, Publisher, StaticSyncOptions, sync_static,
    sync_static_dir,
};
use ssg_render::{JobOrder, Project, RenderError, RenderOptions, Session};
use ssg_resources::ResourceError;
use ssg_site::{Added, LoadModelOptions, Model, ModelError};
use ssg_vfs::{Vfs, VfsError};

/// Where the outputs go.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SinkKind {
    /// The publish directory (`--destination`, else `publishDir`).
    #[default]
    Disk,
    /// Memory ([`BuildReport::memory`]): `serve` and tests.
    Memory,
}

/// What to build.
#[derive(Clone, Debug, Default)]
pub struct BuildRequest {
    /// The project directory.
    pub source: PathBuf,
    /// The publish directory (`--destination`); relative to `source`.
    pub destination: Option<PathBuf>,
    /// `--config` files, relative to `source`, the first with the highest precedence (empty:
    /// the first `config.*`).
    pub config_files: Vec<PathBuf>,
    pub cli: CliOverrides,
    /// The build's "now" (`--clock`; `None`: the system clock).
    pub clock: Option<jiff::Timestamp>,
    pub sink: SinkKind,
    /// `--cleanDestinationDir` (`None`: the configuration's `cleanDestinationDir`).
    pub clean_destination: Option<bool>,
    /// The render pool's thread count (`None`: `RAYON_NUM_THREADS`, else the CPUs). The
    /// output does not depend on it.
    pub threads: Option<usize>,
    /// A configuration the caller loaded, used instead of loading one from `source`,
    /// `config_files`, `cli`, `destination` and the process environment (the `server` command
    /// loads it once per configuration change and points the base URLs at itself).
    pub config: Option<Arc<Config>>,
    /// The `server` command: put the LiveReload script into the HTML pages (not into `build`'s
    /// output).
    pub live_reload: Option<LiveReload>,
    /// The `server` command: `build.is_server` is true.
    pub server: bool,
    /// Work before the build reads the project's files, when this build loads the
    /// configuration (`config` is `None`; the `server` command prepares when it loads it).
    pub prepare: Option<Arc<dyn Prepare>>,
}

/// Work before a build reads the project's files: the command line installs the project's
/// npm packages here, so that mounts of `node_modules` find them.
pub trait Prepare: Send + Sync + std::fmt::Debug {
    /// Prepares the project of `cfg`.
    ///
    /// # Errors
    /// What failed, as the build's error.
    fn prepare(&self, cfg: &Config) -> Result<(), String>;
}

/// The LiveReload script of the `server` command (T71): every HTML page except alias
/// redirects loads `livereload.js` from its language's base URL, which the script also
/// connects to (Go's `livereloadinject`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LiveReload {
    /// The port the script connects to instead of the base URL's (`--liveReloadPort`, e.g.
    /// 443 behind an HTTPS proxy).
    pub port: Option<u16>,
}

impl LiveReload {
    /// The URL the script of a language with base URL `base` uses.
    #[must_use]
    pub fn url(self, base: &BaseUrl) -> UrlRef {
        match self.port {
            Some(port) => base.with_port(port).url().clone(),
            None => base.url().clone(),
        }
    }
}

/// Why a build failed.
#[derive(thiserror::Error, Debug)]
pub enum BuildError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Vfs(#[from] VfsError),
    #[error(transparent)]
    Model(#[from] ModelError),
    #[error(transparent)]
    Template(#[from] TemplateError),
    #[error(transparent)]
    Render(#[from] RenderError),
    #[error(transparent)]
    Publish(#[from] PublishError),
    #[error(transparent)]
    Resource(#[from] ResourceError),
    #[error(transparent)]
    Cms(#[from] ssg_cms::CmsError),
    /// The render pool could not be started.
    #[error("render pool: {0}")]
    Pool(String),
    /// [`BuildRequest::prepare`] failed.
    #[error("{0}")]
    Prepare(String),
    #[error("{} errors", .0.len())]
    Diagnostics(Vec<Diagnostic>),
}

/// Two jobs for the same file: the winner is written, the loser is not rendered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Collision {
    pub path: OutputPath,
    pub winner: JobOrder,
    pub loser: JobOrder,
}

/// What a build did.
#[derive(Debug, Default)]
pub struct BuildReport {
    /// Pages of the model (all kinds, auto nodes included).
    pub pages: usize,
    /// Rendered files published (static files and resources not counted).
    pub outputs: usize,
    /// Alias files among them (front matter, pager, language redirect).
    pub aliases: usize,
    /// Resource files published (processed images not counted).
    pub resources: usize,
    /// Processed images published.
    pub images: usize,
    /// Static files copied.
    pub static_files: usize,
    /// The URL path of the CMS editor (`/admin/`), when the configuration has `[cms]`.
    pub cms: Option<String>,
    /// Target collisions, by path then loser order.
    pub collisions: Vec<Collision>,
    /// Sorted, de-duplicated warnings.
    pub diagnostics: Vec<Diagnostic>,
    pub timings: Vec<(&'static str, Duration)>,
    /// The files of a [`SinkKind::Memory`] build (static files included).
    pub memory: Option<Arc<MemorySink>>,
    /// The site model that was rendered (pages, their content files and links), for callers
    /// that map files to pages (`server --navigateToChanged`).
    pub model: Option<Arc<Model>>,
}

/// The process environment the configuration reads ([`ssg_config::env::is_read`]): `HOME`,
/// `XDG_CACHE_HOME`, `TMPDIR` and `USER` for the default cache directory.
#[must_use]
pub fn process_env() -> Vec<(String, String)> {
    std::env::vars()
        .filter(|(k, _)| ssg_config::env::is_read(k))
        .collect()
}

/// The thread pool every parallel phase runs on. Parallel work is only started from outside
/// it: a render never starts parallel work (REWRITE_PLAN.md §3.5).
pub(crate) struct RenderPool(rayon::ThreadPool);

impl RenderPool {
    fn new(threads: Option<usize>) -> Result<Self, BuildError> {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads.unwrap_or(0))
            .stack_size(16 << 20)
            .thread_name(|i| format!("ssg-render-{i}"))
            .build()
            .map(Self)
            .map_err(|e| BuildError::Pool(e.to_string()))
    }

    /// Runs `f` (which starts the parallel work) on the pool.
    pub(crate) fn run<R: Send>(&self, f: impl FnOnce() -> R + Send) -> R {
        debug_assert!(
            self.0.current_thread_index().is_none(),
            "parallel work started from inside a render"
        );
        self.0.install(f)
    }
}

/// Phase timings.
struct Laps(Instant);

impl Laps {
    fn lap(&mut self, report: &mut BuildReport, name: &'static str) {
        report.timings.push((name, self.0.elapsed()));
        self.0 = Instant::now();
    }
}

/// Phase E6: the resources the outputs reference (URL tokens of every output, of the
/// `execute_as_template` results and of the patched held outputs), the eager bundle files and
/// the `publish` filter's, with the processed images they name.
fn publish_resources(
    session: &Session,
    publisher: &Publisher,
    sink: &dyn Sink,
    pool: &RenderPool,
    report: &mut BuildReport,
) -> Result<(), BuildError> {
    let store = &session.handles().store;
    for id in store.template_outputs() {
        let bytes = store.content(id)?;
        publisher.add_tokens_from(&String::from_utf8_lossy(&bytes));
    }
    let tokens = publisher.url_tokens();
    let stats = pool.run(|| store.publish(tokens.iter(), sink))?;
    report.images = stats.images;
    report.resources = stats.files - stats.images;
    Ok(())
}

/// Builds a site (see the crate docs for the phases).
///
/// # Errors
/// The first failing phase; [`BuildError::Diagnostics`] when a phase reported errors.
pub fn build(r: BuildRequest) -> Result<BuildReport, BuildError> {
    let mut report = BuildReport::default();
    let mut laps = Laps(Instant::now());

    // A1–B6.
    let cfg = match r.config {
        Some(cfg) => cfg,
        None => {
            let mut cli = r.cli;
            if let Some(d) = &r.destination {
                cli.destination = Some(d.clone());
            }
            let cfg = ssg_config::load(&LoadOptions {
                source: r.source.clone(),
                config_files: r.config_files,
                cli,
                env: process_env(),
            })?;
            if let Some(p) = &r.prepare {
                p.prepare(&cfg).map_err(BuildError::Prepare)?;
            }
            Arc::new(cfg)
        }
    };
    let vfs = Arc::new(Vfs::new(&cfg)?);
    let clock = r.clock.map_or_else(Clock::system, Clock);
    let pool = RenderPool::new(r.threads)?;
    let render_options = RenderOptions {
        clock,
        server: r.server,
    };
    let model_options = LoadModelOptions::from_config(&cfg, clock);
    let captured = pool.run(|| ssg_site::capture_content(&cfg, &vfs))?;
    // A4b: content adapters need the layouts (partials) before the model.
    let mut layouts = None;
    let mut adapter_diags = Vec::new();
    let model = if captured.adapters().is_empty() {
        pool.run(|| {
            ssg_site::assemble(Arc::clone(&cfg), captured, Added::default(), &model_options)
        })?
    } else {
        let project = Project {
            vfs: Arc::clone(&vfs),
            layouts: Arc::clone(layouts.insert(Arc::new(LayoutStore::scan(&vfs, &cfg)?))),
        };
        let (model, diags) = adapters::assemble_with_adapters(
            &cfg,
            &project,
            captured,
            &model_options,
            &render_options,
            &pool,
        )?;
        adapter_diags = diags;
        model
    };
    let model_diags = model.diagnostics.clone();
    laps.lap(&mut report, "model");

    // B7, C0.
    let layouts = match layouts {
        Some(l) => l,
        None => Arc::new(LayoutStore::scan(&vfs, &cfg)?),
    };
    let session = pool.run(|| {
        Session::new(
            Project {
                vfs: Arc::clone(&vfs),
                layouts,
            },
            Arc::new(model),
            &render_options,
        )
    })?;
    // The configuration's notices (deprecated keys, ignored configuration files) first.
    for d in cfg
        .diagnostics
        .iter()
        .cloned()
        .chain(model_diags)
        .chain(adapter_diags)
    {
        session.diagnostics().push(d);
    }
    report.pages = session.model().pages.len();
    laps.lap(&mut report, "templates");

    // C1, D.
    pool.run(|| session.render_content())?;
    session.freeze_views()?;
    laps.lap(&mut report, "content");

    // E1.
    let memory = (r.sink == SinkKind::Memory).then(|| Arc::new(MemorySink::new()));
    let mut sync = StaticSyncOptions::from_config(&cfg);
    if let Some(clean) = r.clean_destination {
        sync.clean_destination = clean;
    }
    let sink: Arc<dyn Sink> = match &memory {
        Some(m) => {
            report.static_files = pool.run(|| sync_static(&vfs, m.as_ref(), &sync))?;
            Arc::clone(m) as Arc<dyn Sink>
        }
        None => {
            let root = if cfg.dirs.publish.is_absolute() {
                cfg.dirs.publish.clone()
            } else {
                cfg.project_dir.join(&cfg.dirs.publish)
            };
            report.static_files = pool.run(|| sync_static_dir(&vfs, &root, &sync))?;
            Arc::new(DiskSink::new(root))
        }
    };
    let mut settings = PublishSettings::from_config(&cfg)?;
    if let Some(lr) = r.live_reload {
        for (links, site) in settings.sites.iter_mut().zip(&cfg.sites) {
            links.livereload = Some(lr.url(&site.base_url));
        }
    }
    let publisher = Publisher::new(
        settings,
        Arc::clone(&sink),
        Arc::clone(session.diagnostics()),
    )
    .with_css_purges(Arc::clone(&session.handles().css_purges));
    laps.lap(&mut report, "static");

    // E2 (language sub-waves in order), E3.
    let mut structure = structure::Recorder::from_env();
    let mut written = waves::Written::default();
    for lang in 0..cfg.sites.len() {
        let jobs = session.wave1(LangIdx::from_index(lang));
        waves::run(
            &session,
            &publisher,
            &pool,
            &jobs,
            &mut written,
            &mut report,
            structure.as_mut(),
        )?;
    }
    laps.lap(&mut report, "wave 1");
    let jobs = session.wave2();
    waves::run(
        &session,
        &publisher,
        &pool,
        &jobs,
        &mut written,
        &mut report,
        structure.as_mut(),
    )?;
    report.collisions = written.into_collisions();
    laps.lap(&mut report, "wave 2");

    // E5 (E4 wrote the Go build's stats file; there is none).
    deferred::run(&session, &publisher, &pool)?;
    laps.lap(&mut report, "deferred");

    // E6.
    publish_resources(&session, &publisher, sink.as_ref(), &pool, &mut report)?;
    laps.lap(&mut report, "resources");

    // E6b: after the site, so the editor replaces any output at its path.
    if let Some(cms) = ssg_cms::publish(&cfg, sink.as_ref())? {
        for w in cms.warnings {
            session.diagnostics().push(w);
        }
        report.cms = Some(cms.path);
        laps.lap(&mut report, "cms");
    }

    // E7.
    let diagnostics = session.diagnostics().report();
    if session.diagnostics().has_errors() {
        return Err(BuildError::Diagnostics(diagnostics));
    }
    if let Some(s) = structure {
        s.write(&session)?;
    }
    report.diagnostics = diagnostics;
    report.memory = memory;
    report.model = Some(Arc::clone(session.model()));
    Ok(report)
}
