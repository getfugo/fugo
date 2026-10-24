//! The configuration the server builds with: loaded as a build loads it, every language's base
//! URL pointed at the server, prepared (the project's npm packages), and its proxies.

use std::sync::Arc;

use ssg_base::url::BaseUrl;
use ssg_build::{BuildError, BuildRequest};
use ssg_config::{Config, LoadOptions};
use ssg_vfs::Vfs;

use super::{Rebuilder, watching};
use crate::ServeError;
use crate::address::server_base_url;

impl Rebuilder {
    /// Loads the configuration again and points it at the server. The listeners stay, so a
    /// site cannot become or stop being multihost, nor change its multihost languages.
    pub(super) fn reload_config(&mut self) -> Result<(), ServeError> {
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
        let proxies = self.proxies.reload(&cfg)?;
        prepare(&self.request, &cfg)?;
        self.vfs = Vfs::new(&cfg)?;
        (self.classifier, self.watch_set) = watching(&cfg, &self.vfs);
        self.proxies = Arc::new(proxies);
        self.cfg = Arc::new(cfg);
        Ok(())
    }
}

/// Every language's base URL becomes the server's (Go's `fixURL`): the language's listener
/// on a multihost site, else the only one.
pub(super) fn point_at_server(
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
/// start and when the configuration, `package.json` or `package-lock.json` changes).
pub(super) fn prepare(r: &BuildRequest, cfg: &Config) -> Result<(), ServeError> {
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
        config_file: r.config_file.clone(),
        cli,
        env: ssg_build::process_env(),
    })?)
}
