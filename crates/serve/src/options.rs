//! What the server is asked to do ([`ServeOptions`]): where it builds to, how it notices
//! changes, where it listens, live reloading, caching and the proxies of `--proxy`, with the
//! certificates their `https://` servers may have.

use std::time::Duration;

use ssg_build::BuildRequest;

use crate::{CertificateDer, Proxy};

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
    /// `--proxy`: the requests below a path go to another server. A later one replaces an
    /// earlier one of the same path, and both replace the configuration's
    /// (`[[server.proxies]]`).
    pub proxies: Vec<Proxy>,
    /// The authorities an `https://` proxy's server may have its certificate from besides
    /// Mozilla's (webpki-roots), such as a local one (mkcert's) or a test's.
    pub proxy_roots: Vec<CertificateDer<'static>>,
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
            proxies: Vec::new(),
            proxy_roots: Vec::new(),
        }
    }
}
