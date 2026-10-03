//! The watch loop (Go's `handleEvents`): each batch of changes becomes a configuration
//! reload and full rebuild, a full rebuild, or a copy of the changed static files, followed by
//! the LiveReload command.
//!
//! **Rebuilds.** Every change of the site (content, layouts, assets, data, i18n, archetypes,
//! or a lost event) builds the whole site again; a memory build goes into a new sink that
//! replaces the served one only when the build succeeds, so a failing build leaves the last
//! good site in place. A configuration change reloads the configuration first (a
//! configuration that does not load pauses the loop until it loads again).
//!
//! **Static files.** A batch of static files only is copied without a build: the static mounts
//! are listed again and every file below a changed path is written into the served tree (or
//! removed when no static directory has it any more), as Go's `syncsStaticEvents` does; like
//! there, such a file wins over a rendered file of the same path until the next build.
//!
//! **Reload** (Go's fast render mode logic, on the files the build changed, compared with
//! the last good build): nothing changed, no reload; content changed, a full reload (or, with
//! `--navigateToChanged`, a navigation to the changed page); one other file changed, a reload
//! of that path; only stylesheets changed, each is reloaded in place; else a full reload. A
//! configuration change always reloads fully; `--render-to-disk` builds are not compared, so
//! they always reload fully. One static file reloads that path, several reload fully.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use notify::EventKind;
use notify_debouncer_full::DebouncedEvent;
use ssg_base::url::{BaseUrl, UrlRef};
use ssg_build::{BuildError, BuildReport, BuildRequest, LiveReload, SinkKind};
use ssg_config::{Config, LoadOptions};
use ssg_publish::{MemorySink, StaticSyncOptions};
use ssg_vfs::{Component, Vfs, VfsError};

use crate::address::server_base_url;
use crate::tree::{Served, Tree, publish_dir};
use crate::watch::{Changes, Classifier, ConfigPlaces, Message, WatchSet, Watcher};
use crate::{
    ChangeKind, Event, LiveReloadOptions, Reporter, ServeError, ServeOptions, Shared, Target,
    livereload,
};

/// Owns the configuration and the site between builds; runs on the watch thread.
pub(crate) struct Rebuilder {
    request: BuildRequest,
    append_port: bool,
    live_reload: Option<LiveReloadOptions>,
    target: Target,
    config_dir: PathBuf,
    /// The listeners' ports (one per language of a multihost site).
    ports: Vec<u16>,
    cfg: Arc<Config>,
    vfs: Vfs,
    classifier: Classifier,
    watch_set: WatchSet,
    /// The static files of the served tree: publish path → source file.
    static_files: BTreeMap<String, PathBuf>,
    shared: Arc<Shared>,
    reporter: Arc<dyn Reporter>,
    /// The configuration does not load: only configuration changes are handled.
    paused: bool,
    /// Batches of changes handled so far.
    batches: u64,
}

impl Rebuilder {
    /// The site of `cfg` (loaded with [`load`]) served on `ports`.
    pub(crate) fn new(
        opts: &ServeOptions,
        mut cfg: Config,
        ports: Vec<u16>,
        shared: Arc<Shared>,
        reporter: Arc<dyn Reporter>,
    ) -> Result<Self, ServeError> {
        let config_dir = opts.build.source.join(
            opts.build
                .cli
                .config_dir
                .as_deref()
                .unwrap_or(Path::new("config")),
        );
        point_at_server(
            &mut cfg,
            opts.build.cli.base_url.as_deref(),
            &ports,
            opts.append_port,
        )?;
        prepare(&opts.build, &cfg)?;
        let vfs = Vfs::new(&cfg)?;
        let (classifier, watch_set) = watching(&cfg, &vfs, &config_dir);
        Ok(Self {
            request: opts.build.clone(),
            append_port: opts.append_port,
            live_reload: opts.live_reload,
            target: opts.target,
            classifier,
            watch_set,
            config_dir,
            ports,
            cfg: Arc::new(cfg),
            vfs,
            static_files: BTreeMap::new(),
            shared,
            reporter,
            paused: false,
            batches: 0,
        })
    }

    pub(crate) fn config(&self) -> &Config {
        &self.cfg
    }

    pub(crate) fn watch_set(&self) -> &WatchSet {
        &self.watch_set
    }

    /// Loads the configuration again and points it at the server. The listeners stay, so a
    /// site cannot become or stop being multihost, nor change its multihost languages.
    fn reload_config(&mut self) -> Result<(), ServeError> {
        let mut cfg = load(&self.request)?;
        if cfg.multihost != self.cfg.multihost
            || (cfg.multihost && cfg.sites.len() != self.cfg.sites.len())
        {
            return Err(ServeError::MultihostChanged);
        }
        point_at_server(
            &mut cfg,
            self.request.cli.base_url.as_deref(),
            &self.ports,
            self.append_port,
        )?;
        prepare(&self.request, &cfg)?;
        self.vfs = Vfs::new(&cfg)?;
        (self.classifier, self.watch_set) = watching(&cfg, &self.vfs, &self.config_dir);
        self.cfg = Arc::new(cfg);
        Ok(())
    }

    fn build(&self) -> Result<BuildReport, BuildError> {
        ssg_build::build(BuildRequest {
            config: Some(Arc::clone(&self.cfg)),
            sink: match self.target {
                Target::Memory => SinkKind::Memory,
                Target::Disk => SinkKind::Disk,
            },
            live_reload: self.live_reload.map(|l| LiveReload { port: l.port }),
            server: true,
            ..self.request.clone()
        })
    }

    /// Serves a build's files; returns the memory tree they replace.
    fn install(&mut self, report: &BuildReport) -> Option<Arc<MemorySink>> {
        let tree = match (&report.memory, self.target) {
            (Some(m), Target::Memory) => Tree::Memory(Arc::clone(m)),
            _ => Tree::Disk(publish_dir(&self.cfg)),
        };
        let old = match &self.shared.served().tree {
            Tree::Memory(m) => Some(Arc::clone(m)),
            Tree::Disk(_) => None,
        };
        self.shared.install(Served::new(tree, &self.cfg));
        match static_files(&self.vfs, &self.cfg) {
            Ok(files) => self.static_files = files,
            Err(e) => self.error(&format!("listing the static files: {e}")),
        }
        old
    }

    /// The first build: its failure ends the server.
    pub(crate) fn first_build(&mut self) -> Result<(), ServeError> {
        let started = Instant::now();
        let report = self.build()?;
        self.install(&report);
        self.reporter.report(&Event::Built {
            report: &report,
            elapsed: started.elapsed(),
            first: true,
        });
        Ok(())
    }

    /// Handles batches until the server shuts down.
    pub(crate) fn run(mut self, rx: &Receiver<Message>, mut watcher: Watcher) {
        while let Ok(Message::Events(first)) = rx.recv() {
            // Whatever arrived meanwhile (during the last build) is part of this batch.
            let mut results = vec![first];
            loop {
                match rx.try_recv() {
                    Ok(Message::Events(r)) => results.push(r),
                    Ok(Message::Shutdown) => return,
                    Err(_) => break,
                }
            }
            let mut events = Vec::new();
            for r in results {
                match r {
                    Ok(batch) => events.extend(batch),
                    Err(errors) => {
                        for e in errors {
                            self.error(&format!("watching: {e}"));
                        }
                    }
                }
            }
            let changes = self.classifier.classify(&events);
            // Before the reload goes out, so that the next edit in a new directory is seen.
            self.refresh_watches(&events, &mut watcher);
            if !changes.is_empty() {
                self.handle(&changes, &mut watcher);
            }
        }
    }

    /// Watches the mounted directories as they are now: one created since (a first `static/`
    /// or `assets/`) is watched from now on, and one removed and created again is watched
    /// anew.
    fn refresh_watches(&mut self, events: &[DebouncedEvent], watcher: &mut Watcher) {
        let vfs = match Vfs::new(&self.cfg) {
            Ok(v) => v,
            Err(e) => {
                self.error(&format!("mounts: {e}"));
                return;
            }
        };
        let (classifier, set) = watching(&self.cfg, &vfs, &self.config_dir);
        let mut old = self.watch_set.clone();
        for e in events {
            if matches!(e.kind, EventKind::Create(_)) {
                for p in &e.paths {
                    old.roots.remove(p);
                }
            }
        }
        for (path, e) in watcher.update(&old, &set) {
            self.error(&format!("watching {}: {e}", path.display()));
        }
        self.classifier = classifier;
        self.vfs = vfs;
        self.watch_set = set;
    }

    fn handle(&mut self, changes: &Changes, watcher: &mut Watcher) {
        if self.paused && changes.config.is_empty() {
            return;
        }
        self.batches += 1;
        let kind = if !changes.config.is_empty() {
            ChangeKind::Config
        } else if changes.static_only() {
            ChangeKind::Static
        } else {
            ChangeKind::Site
        };
        self.reporter.report(&Event::ChangeDetected {
            number: self.batches,
            kind,
            paths: &changes.paths(),
        });
        if kind == ChangeKind::Static {
            self.sync_static(&changes.statics);
        } else {
            self.rebuild(changes, watcher);
        }
    }

    fn rebuild(&mut self, changes: &Changes, watcher: &mut Watcher) {
        let started = Instant::now();
        let config_changed = !changes.config.is_empty();
        if config_changed {
            let old_set = self.watch_set.clone();
            if let Err(error) = self.reload_config() {
                self.paused = true;
                self.reporter.report(&Event::ConfigFailed { error: &error });
                return;
            }
            self.paused = false;
            for (path, e) in watcher.update(&old_set, &self.watch_set) {
                self.error(&format!("watching {}: {e}", path.display()));
            }
            self.reporter.report(&Event::Watching {
                sources: &self.watch_set.sources,
                config: &self.watch_set.config,
            });
        }
        let report = match self.build() {
            Ok(r) => r,
            Err(error) => {
                self.reporter.report(&Event::BuildFailed { error: &error });
                return;
            }
        };
        let old = self.install(&report);
        self.reporter.report(&Event::Built {
            report: &report,
            elapsed: started.elapsed(),
            first: false,
        });
        if self.live_reload.is_none() {
            return;
        }
        if config_changed || changes.rescan {
            self.send(&livereload::force_refresh());
            return;
        }
        let changed = match (old, &report.memory) {
            (Some(old), Some(new)) => Some(changed_files(&old, new)),
            _ => None,
        };
        self.reload_after_build(changes, changed.as_deref(), &report);
    }

    fn reload_after_build(
        &self,
        changes: &Changes,
        changed: Option<&[String]>,
        report: &BuildReport,
    ) {
        if changed.is_some_and(<[String]>::is_empty) {
            return;
        }
        if !changes.content.is_empty() {
            let navigate = self.live_reload.is_some_and(|l| l.navigate_to_changed);
            match self.changed_page(changes, report).filter(|_| navigate) {
                Some((path, port)) => self.send(&livereload::navigate(&path, port)),
                None => self.send(&livereload::force_refresh()),
            }
            return;
        }
        let Some(changed) = changed else {
            self.send(&livereload::force_refresh());
            return;
        };
        let (css, other): (Vec<&String>, Vec<&String>) =
            changed.iter().partition(|p| p.ends_with(".css"));
        if other.len() == 1 {
            self.send(&livereload::reload(&self.url_path(other[0])));
        } else if css.is_empty() || other.len() > 1 {
            self.send(&livereload::force_refresh());
        }
        if !css.is_empty() {
            if !other.is_empty() {
                // Let the reloaded pages connect again first (Go waits as long).
                std::thread::sleep(Duration::from_millis(200));
            }
            for c in css {
                self.send(&livereload::reload(&self.url_path(c)));
            }
        }
    }

    /// The page of the content file a batch wrote or created (an index file first), with the
    /// port of its language's server: Go's `pickOneWriteOrCreatePath`.
    fn changed_page(
        &self,
        changes: &Changes,
        report: &BuildReport,
    ) -> Option<(String, Option<u16>)> {
        let cfg = &self.cfg;
        let is_content = |p: &Path| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| cfg.content_types.is_content_suffix(&cfg.media_types, e))
        };
        let is_index = |p: &Path| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("index.") || n.starts_with("_index."))
        };
        let mut file = None;
        for (p, written) in &changes.content {
            if *written && is_content(p) {
                file = Some(p);
                if is_index(p) {
                    break;
                }
            }
        }
        let file = file?;
        let model = report.model.as_ref()?;
        let page = model
            .pages
            .iter()
            .find(|p| p.source.as_ref().is_some_and(|s| s.file.abs == *file))?;
        let links = page.urls.first()?.links.as_ref()?;
        let url = UrlRef::parse(links.permalink.as_str()).ok()?;
        let port = cfg.sites.get(page.lang).and_then(|s| s.base_url.port());
        Some((url.escaped_path().into_owned(), port))
    }

    /// The URL path of a file of the served tree, on its listener.
    fn url_path(&self, file: &str) -> String {
        let served = self.shared.served();
        served
            .hosts
            .iter()
            .find_map(|h| {
                file.strip_prefix(h.root.as_str())
                    .map(|rest| format!("{}{rest}", h.base_path))
            })
            .unwrap_or_else(|| format!("/{file}"))
    }

    /// Copies the static files below `paths` into the served tree.
    fn sync_static(&mut self, paths: &[PathBuf]) {
        let started = Instant::now();
        let files = match static_files(&self.vfs, &self.cfg) {
            Ok(f) => f,
            Err(e) => {
                self.error(&format!("listing the static files: {e}"));
                return;
            }
        };
        let touched = |abs: &Path| paths.iter().any(|p| abs.starts_with(p));
        let targets: BTreeSet<String> = files
            .iter()
            .chain(&self.static_files)
            .filter(|(_, abs)| touched(abs))
            .map(|(t, _)| t.clone())
            .collect();
        let tree = self.shared.served().tree.clone();
        // Only files whose bytes change count (an editor saving the same bytes, a write
        // reported twice).
        let mut changed: Vec<&String> = Vec::new();
        for t in &targets {
            let current = tree.read_now(t);
            let result = match files.get(t) {
                Some(abs) => std::fs::read(abs).and_then(|bytes| {
                    if current.as_deref() == Some(bytes.as_slice()) {
                        return Ok(());
                    }
                    changed.push(t);
                    tree.write(t, &bytes)
                }),
                None if current.is_some() => {
                    changed.push(t);
                    tree.remove(t)
                }
                None => Ok(()),
            };
            if let Err(e) = result {
                self.error(&format!("copying the static file {t}: {e}"));
            }
        }
        self.static_files = files;
        self.reporter.report(&Event::StaticSynced {
            files: changed.len(),
            elapsed: started.elapsed(),
        });
        if self.live_reload.is_none() {
            return;
        }
        match changed.as_slice() {
            [] => {}
            [one] => self.send(&livereload::reload(&self.url_path(one))),
            _ => self.send(&livereload::force_refresh()),
        }
    }

    fn send(&self, command: &str) {
        self.reporter.report(&Event::Reload { command });
        let _ = self.shared.reload.send(Arc::from(command));
    }

    fn error(&self, message: &str) {
        self.reporter.report(&Event::Error { message });
    }
}

/// The event classifier and the watch set of a configuration.
fn watching(cfg: &Config, vfs: &Vfs, config_dir: &Path) -> (Classifier, WatchSet) {
    let places = ConfigPlaces::new(cfg, config_dir);
    let set = WatchSet::new(vfs, &places);
    (Classifier::new(vfs, places), set)
}

/// Every language's base URL becomes the server's (Go's `fixURL`): the language's listener
/// on a multihost site, else the only one.
fn point_at_server(
    cfg: &mut Config,
    flag: Option<&str>,
    ports: &[u16],
    append_port: bool,
) -> Result<(), ServeError> {
    let multihost = cfg.multihost;
    for (i, site) in cfg.sites.iter_mut().enumerate() {
        let port = if multihost {
            ports.get(i)
        } else {
            ports.first()
        }
        .copied()
        .ok_or(ServeError::MultihostChanged)?;
        let url =
            server_base_url(site.base_url.as_str(), flag, port, append_port).map_err(|source| {
                ServeError::BaseUrl {
                    url: site.base_url.as_str().to_owned(),
                    source,
                }
            })?;
        site.base_url = BaseUrl::parse(&url).map_err(|source| ServeError::BaseUrl {
            url: url.clone(),
            source,
        })?;
    }
    Ok(())
}

/// The request's [`BuildRequest::prepare`], on a configuration the server loaded (at the
/// start and when the configuration or `package.json` changes).
fn prepare(r: &BuildRequest, cfg: &Config) -> Result<(), ServeError> {
    match &r.prepare {
        Some(p) => p.prepare(cfg).map_err(|e| BuildError::Prepare(e).into()),
        None => Ok(()),
    }
}

/// The configuration of a request, as `ssg_build::build` would load it.
pub(crate) fn load(r: &BuildRequest) -> Result<Config, ServeError> {
    let mut cli = r.cli.clone();
    if let Some(d) = &r.destination {
        cli.destination = Some(d.clone());
    }
    Ok(ssg_config::load(&LoadOptions {
        source: r.source.clone(),
        config_files: r.config_files.clone(),
        cli,
        env: ssg_build::process_env(),
    })?)
}

/// The static files of a site: publish path (no leading slash) → source file.
fn static_files(vfs: &Vfs, cfg: &Config) -> Result<BTreeMap<String, PathBuf>, VfsError> {
    let o = StaticSyncOptions::from_config(cfg);
    Ok(vfs
        .walk(Component::Static)?
        .into_iter()
        .map(|f| (o.target(&f), f.abs))
        .collect())
}

/// The files of `new` that `old` does not have or has with other bytes (source maps left
/// out, as in Go's change detector), sorted.
fn changed_files(old: &MemorySink, new: &MemorySink) -> Vec<String> {
    let mut changed: Vec<String> = new
        .files
        .iter()
        .filter(|e| {
            old.files
                .get(e.key())
                .is_none_or(|o| o.value().as_ref() != e.value().as_ref())
        })
        .map(|e| e.key().relative().to_owned())
        .filter(|p| !p.ends_with(".map"))
        .collect();
    changed.sort();
    changed
}
