//! The `server` command (REWRITE_PLAN.md T71): the Go implementation's development server.
//!
//! [`Server::start`] loads the configuration, opens the listeners (one per language of a
//! multihost site), points every language's base URL at its listener (Go's `fixURL`),
//! starts watching, builds the site, then serves it:
//!
//! - **Builds** go into memory by default ([`Target::Memory`]; a new sink per build, swapped
//!   in whole when the build succeeds) or into the publish directory ([`Target::Disk`],
//!   `--render-to-disk`), with the LiveReload script in every HTML page.
//! - **HTTP** (`http.rs`, `tree.rs`): the files of the last good build, `index.html` for
//!   directories, Go's `http.FileServer` redirects, content types from the site's media types,
//!   single byte ranges, and for a missing page the `404.html` of the path's language with
//!   status 404; `livereload.js` and the `livereload` WebSocket on the same port, below the
//!   base URL's path.
//! - **Watching** (`watch.rs`, `rebuild.rs`): the mounts of the project and its themes and the
//!   configuration files, through notify with a 1 s debounce (or polling). A change of the
//!   site rebuilds it all, a static-only change copies the changed static files, a
//!   configuration change reloads the configuration and rebuilds; then the browsers get the
//!   LiveReload command.
//!
//! What the server does is reported through a [`Reporter`] (the command line prints it).

#![forbid(unsafe_code)]

mod address;
mod http;
mod livereload;
mod rebuild;
mod tree;
mod watch;

use std::io;
use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, PoisonError, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use axum::Router;
use ssg_base::url::UrlError;
use ssg_build::{BuildError, BuildReport, BuildRequest};
use ssg_config::ConfigError;
use ssg_vfs::VfsError;
use tokio::sync::{broadcast, watch as signal};

use crate::rebuild::Rebuilder;
use crate::tree::Served;
use crate::watch::{Message, WatchSet, Watcher};

/// Where the site is built to and served from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Target {
    /// Memory: each build goes into a new sink, served once the build succeeds.
    #[default]
    Memory,
    /// The publish directory (`--render-to-disk`), written in place by each build.
    Disk,
}

/// How changes are noticed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Watch {
    /// Not at all (`--watch=false`): the site is built once.
    Off,
    /// The operating system's file notifications.
    #[default]
    Native,
    /// Polling at this interval (`--poll`), for file systems without notifications.
    Poll(Duration),
}

/// The port to listen on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Port {
    /// `--port`: this port, or fail (0: a free port the system picks).
    Exact(u16),
    /// The default (1313): a free port when it is taken.
    Preferred(u16),
}

impl Default for Port {
    fn default() -> Self {
        Self::Preferred(1313)
    }
}

/// HTTP caching of the served files.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HttpCache {
    /// No caching headers.
    #[default]
    Default,
    /// `--noHTTPCache`: `Cache-Control: no-store, no-cache, …` and `Pragma: no-cache`.
    Disabled,
}

/// Live reloading (`None` in [`ServeOptions::live_reload`]: `--disableLiveReload`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LiveReloadOptions {
    /// The port the browsers connect to instead of the server's (`--liveReloadPort`, e.g. 443
    /// behind an HTTPS proxy).
    pub port: Option<u16>,
    /// Send the browsers to the page whose content changed (`--navigateToChanged`).
    pub navigate_to_changed: bool,
}

/// What to serve and how.
#[derive(Clone, Debug)]
pub struct ServeOptions {
    /// The project and its build flags. `sink`, `config` and `live_reload` are the server's;
    /// `cli.base_url` is `--baseURL` (the server's address replaces the configured one when
    /// it is not set); `cli.environment` should default to `development`.
    pub build: BuildRequest,
    /// The interface to listen on (`--bind`).
    pub bind: String,
    pub port: Port,
    /// Put the port into the base URLs (`--appendPort`).
    pub append_port: bool,
    pub live_reload: Option<LiveReloadOptions>,
    pub target: Target,
    pub watch: Watch,
    pub http_cache: HttpCache,
}

impl Default for ServeOptions {
    fn default() -> Self {
        Self {
            build: BuildRequest::default(),
            bind: "127.0.0.1".to_owned(),
            port: Port::default(),
            append_port: true,
            live_reload: Some(LiveReloadOptions::default()),
            target: Target::Memory,
            watch: Watch::Native,
            http_cache: HttpCache::Default,
        }
    }
}

/// What a batch of changes touched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    /// Configuration files: reload and rebuild.
    Config,
    /// Static files only: copied, no rebuild.
    Static,
    /// Anything else of the site: rebuild.
    Site,
}

/// What the server did, for the [`Reporter`].
#[derive(Debug)]
#[non_exhaustive]
pub enum Event<'a> {
    /// Before the first build.
    Starting {
        environment: &'a str,
        target: Target,
    },
    /// What is watched: the mounted directories and files of the project and its themes, and
    /// the configuration files and directory.
    Watching {
        sources: &'a [PathBuf],
        config: &'a [PathBuf],
    },
    /// The default port is taken; another one is used.
    PortInUse { port: u16 },
    /// A build succeeded and is served.
    Built {
        report: &'a BuildReport,
        elapsed: Duration,
        first: bool,
    },
    /// A rebuild failed; the last good build is still served.
    BuildFailed { error: &'a BuildError },
    /// A batch of changes, numbered from 1.
    ChangeDetected {
        number: u64,
        kind: ChangeKind,
        paths: &'a [PathBuf],
    },
    /// Static files copied into (or removed from) the served site without a build.
    StaticSynced { files: usize, elapsed: Duration },
    /// The configuration does not load any more; only configuration changes are handled
    /// until it does.
    ConfigFailed { error: &'a ServeError },
    /// A listener is serving the site at `url`.
    Listening { url: &'a str, bind: &'a str },
    /// The LiveReload command sent to the browsers.
    Reload { command: &'a str },
    /// A problem while watching or copying files.
    Error { message: &'a str },
}

/// Receives the server's [`Event`]s (from the thread that caused them).
pub trait Reporter: Send + Sync {
    fn report(&self, event: &Event<'_>);
}

/// Why the server could not start or reload its configuration.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ServeError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Vfs(#[from] VfsError),
    /// The first build failed.
    #[error(transparent)]
    Build(#[from] Box<BuildError>),
    #[error("server startup failed: {address}: {source}")]
    Bind { address: String, source: io::Error },
    #[error("the server's base URL for {url:?}: {source}")]
    BaseUrl { url: String, source: UrlError },
    #[error("--appendPort=false is not supported for a multihost site")]
    MultihostAppendPort,
    #[error(
        "the site became or stopped being multihost, or its multihost languages changed: \
         restart the server"
    )]
    MultihostChanged,
    #[error("watching: {0}")]
    Watch(#[from] notify::Error),
    #[error("starting the HTTP server: {0}")]
    Runtime(io::Error),
}

impl From<BuildError> for ServeError {
    fn from(e: BuildError) -> Self {
        Self::Build(Box::new(e))
    }
}

/// What the HTTP side shares with the watch loop.
pub(crate) struct Shared {
    served: RwLock<Arc<Served>>,
    /// The LiveReload commands.
    pub(crate) reload: broadcast::Sender<Arc<str>>,
    /// Becomes `true` when the server shuts down.
    pub(crate) shutdown: signal::Receiver<bool>,
    pub(crate) live_reload: bool,
    pub(crate) http_cache: HttpCache,
}

impl Shared {
    /// The site being served.
    pub(crate) fn served(&self) -> Arc<Served> {
        Arc::clone(&self.served.read().unwrap_or_else(PoisonError::into_inner))
    }

    /// Serves `served` from now on.
    pub(crate) fn install(&self, served: Served) {
        *self.served.write().unwrap_or_else(PoisonError::into_inner) = Arc::new(served);
    }
}

/// A running server; [`Server::shutdown`] stops it (dropping it only signals the stop).
pub struct Server {
    urls: Vec<String>,
    addrs: Vec<SocketAddr>,
    stop: signal::Sender<bool>,
    loop_tx: Sender<Message>,
    http: Option<JoinHandle<io::Result<()>>>,
    watch: Option<JoinHandle<()>>,
}

impl Server {
    /// Builds the site and starts serving and watching it.
    ///
    /// # Errors
    /// A configuration that does not load, a port that cannot be opened, a watcher that
    /// cannot start, or a first build that fails ([`ServeError::Build`]).
    pub fn start(opts: &ServeOptions, reporter: &Arc<dyn Reporter>) -> Result<Self, ServeError> {
        let cfg = rebuild::load(&opts.build)?;
        if cfg.multihost && !opts.append_port {
            return Err(ServeError::MultihostAppendPort);
        }
        let count = if cfg.multihost { cfg.sites.len() } else { 1 };
        let listeners = address::listen(&opts.bind, opts.port, count, reporter.as_ref())?;
        let ports = listeners
            .iter()
            .map(address::local_port)
            .collect::<Result<Vec<u16>, ServeError>>()?;
        let addrs = listeners
            .iter()
            .map(TcpListener::local_addr)
            .collect::<io::Result<Vec<SocketAddr>>>()
            .map_err(|source| ServeError::Bind {
                address: opts.bind.clone(),
                source,
            })?;

        let (stop, stopped) = signal::channel(false);
        let shared = Arc::new(Shared {
            served: RwLock::new(Arc::new(Served::empty())),
            reload: broadcast::channel(16).0,
            shutdown: stopped.clone(),
            live_reload: opts.live_reload.is_some(),
            http_cache: opts.http_cache,
        });
        let mut rebuilder =
            Rebuilder::new(opts, cfg, ports, Arc::clone(&shared), Arc::clone(reporter))?;
        reporter.report(&Event::Starting {
            environment: &rebuilder.config().environment,
            target: opts.target,
        });

        // Watch before building, so that nothing changed during the first build is missed.
        let (loop_tx, loop_rx) = mpsc::channel();
        let poll = match opts.watch {
            Watch::Off => None,
            Watch::Native => Some(None),
            Watch::Poll(interval) => Some(Some(interval)),
        };
        let watcher = match poll {
            Some(poll) => {
                let mut w = Watcher::new(poll, loop_tx.clone())?;
                for (path, e) in w.update(&WatchSet::default(), rebuilder.watch_set()) {
                    reporter.report(&Event::Error {
                        message: &format!("watching {}: {e}", path.display()),
                    });
                }
                let set = rebuilder.watch_set();
                reporter.report(&Event::Watching {
                    sources: &set.sources,
                    config: &set.config,
                });
                Some(w)
            }
            None => None,
        };
        rebuilder.first_build()?;

        let cfg = rebuilder.config();
        let urls: Vec<String> = if cfg.multihost {
            cfg.sites
                .iter()
                .map(|s| s.base_url.as_str().to_owned())
                .collect()
        } else {
            vec![cfg.default_site().base_url.as_str().to_owned()]
        };
        let routers = (0..listeners.len())
            .map(|i| http::router(Arc::clone(&shared), i))
            .collect();
        let http = spawn_http(listeners, routers, stopped)?;
        for url in &urls {
            reporter.report(&Event::Listening {
                url,
                bind: &opts.bind,
            });
        }
        // From here on, an error drops the server, which stops the HTTP thread.
        let mut server = Self {
            urls,
            addrs,
            stop,
            loop_tx,
            http: Some(http),
            watch: None,
        };
        if let Some(w) = watcher {
            server.watch = Some(
                thread::Builder::new()
                    .name("ssg-watch".to_owned())
                    .spawn(move || rebuilder.run(&loop_rx, w))
                    .map_err(ServeError::Runtime)?,
            );
        }
        Ok(server)
    }

    /// The base URL of each listener (one per language of a multihost site).
    #[must_use]
    pub fn urls(&self) -> &[String] {
        &self.urls
    }

    /// The address of each listener.
    #[must_use]
    pub fn local_addrs(&self) -> &[SocketAddr] {
        &self.addrs
    }

    /// Serves until the HTTP server ends (it only ends on [`Server::shutdown`] or an error).
    ///
    /// # Errors
    /// The HTTP server's.
    pub fn wait(mut self) -> io::Result<()> {
        let result = match self.http.take() {
            Some(h) => h
                .join()
                .unwrap_or_else(|_| Err(io::Error::other("the HTTP server panicked"))),
            None => Ok(()),
        };
        self.shutdown_now();
        result
    }

    /// Stops watching and serving, and waits for both (a build in progress finishes first).
    pub fn shutdown(mut self) {
        self.shutdown_now();
    }

    fn shutdown_now(&mut self) {
        self.signal();
        if let Some(h) = self.watch.take() {
            let _ = h.join();
        }
        if let Some(h) = self.http.take() {
            let _ = h.join();
        }
    }

    fn signal(&self) {
        let _ = self.stop.send(true);
        let _ = self.loop_tx.send(Message::Shutdown);
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.signal();
    }
}

/// Serves the routers on their listeners on one thread (a current-thread runtime) until the
/// stop signal.
fn spawn_http(
    listeners: Vec<TcpListener>,
    routers: Vec<Router>,
    stopped: signal::Receiver<bool>,
) -> Result<JoinHandle<io::Result<()>>, ServeError> {
    for l in &listeners {
        l.set_nonblocking(true).map_err(ServeError::Runtime)?;
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(ServeError::Runtime)?;
    thread::Builder::new()
        .name("ssg-http".to_owned())
        .spawn(move || {
            runtime.block_on(async move {
                let mut servers = tokio::task::JoinSet::new();
                for (listener, router) in listeners.into_iter().zip(routers) {
                    let listener = tokio::net::TcpListener::from_std(listener)?;
                    let mut stopped = stopped.clone();
                    servers.spawn(async move {
                        axum::serve(listener, router)
                            .with_graceful_shutdown(async move {
                                let _ = stopped.wait_for(|stop| *stop).await;
                            })
                            .await
                    });
                }
                let mut result = Ok(());
                while let Some(r) = servers.join_next().await {
                    match r {
                        Ok(Ok(())) => {}
                        Ok(Err(e)) => result = Err(e),
                        Err(e) => result = Err(io::Error::other(e)),
                    }
                }
                result
            })
        })
        .map_err(ServeError::Runtime)
}
