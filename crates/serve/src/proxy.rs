//! The proxies of `--proxy` and `[[server.proxies]]` (fugo's, not the Go server's): the
//! requests of a path and below it go to another HTTP server, such as the site's API running
//! on this computer, so that the pages reach it on their own origin, as they will once the
//! site and the API are published side by side. `forward.rs` forwards them.
//!
//! ```toml
//! [[server.proxies]]
//! from = "/api/"                # a path of the site: /api and everything below it
//! to = "http://127.0.0.1:3000"  # an http:// server; the requests keep their path
//! ```
//!
//! A request goes to the proxy of the longest path that covers it, after the server's own
//! paths (`livereload.js`, `livereload`, the CMS editor's API) and before the site's files.
//! `--proxy` replaces the configuration's proxy of the same path.

use std::str::FromStr;

use axum::http::Uri;
use serde::Deserialize;
use ssg_config::Config;

use crate::ServeError;

mod forward;

pub(crate) use forward::forward;

/// One proxy: the requests of [`Proxy::path`] and below it go to [`Proxy::url`]. `FromStr`
/// reads the value of `--proxy`, `PATH=URL` (`/api/=http://127.0.0.1:3000`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Proxy {
    /// The path with a trailing slash (`/api/`).
    path: String,
    to: Origin,
}

/// The server a proxy forwards to.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Origin {
    /// `http://127.0.0.1:3000`.
    url: String,
    /// The `Host` of a request that has none (`127.0.0.1:3000`).
    host: String,
    /// What to connect to (`127.0.0.1:3000`, `localhost:80`).
    addr: String,
}

impl Proxy {
    /// The path (`/api/`): it and the paths below it are forwarded.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The server they go to (`http://127.0.0.1:3000`).
    #[must_use]
    pub fn url(&self) -> &str {
        &self.to.url
    }

    /// Whether the request path `path` (decoded) is the proxy's path or below it.
    fn covers(&self, path: &str) -> bool {
        path.starts_with(&self.path) || Some(path) == self.path.strip_suffix('/')
    }
}

impl FromStr for Proxy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let Some((from, to)) = s.split_once('=') else {
            return Err(format!(
                "{s:?} is not PATH=URL (such as /api/=http://127.0.0.1:3000)"
            ));
        };
        Ok(Self {
            path: path(from)?,
            to: origin(to)?,
        })
    }
}

/// The proxies in force: `--proxy`'s, and the configuration's of the paths the flags do not
/// name.
#[derive(Debug, Default)]
pub(crate) struct Proxies {
    /// `--proxy`'s, one per path.
    flags: Vec<Proxy>,
    /// The flags' and the configuration's, by path.
    all: Vec<Proxy>,
}

impl Proxies {
    /// `flags` (a later one replacing an earlier one of the same path) and the configuration's
    /// `[[server.proxies]]`.
    ///
    /// # Errors
    /// A `[[server.proxies]]` entry that is wrong ([`ServeError::Proxy`]).
    pub(crate) fn new(flags: &[Proxy], cfg: &Config) -> Result<Self, ServeError> {
        let mut own: Vec<Proxy> = Vec::new();
        for p in flags.iter().rev() {
            if !own.iter().any(|o| o.path == p.path) {
                own.push(p.clone());
            }
        }
        let mut all = own.clone();
        for p in configured(cfg)? {
            if !all.iter().any(|o| o.path == p.path) {
                all.push(p);
            }
        }
        all.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(Self { flags: own, all })
    }

    /// The flags' proxies with those of `cfg`, a configuration loaded again.
    ///
    /// # Errors
    /// As [`Proxies::new`].
    pub(crate) fn reload(&self, cfg: &Config) -> Result<Self, ServeError> {
        Self::new(&self.flags, cfg)
    }

    /// The proxy of the request path `path` (decoded): of the longest path that covers it.
    pub(crate) fn find(&self, path: &str) -> Option<&Proxy> {
        self.all
            .iter()
            .filter(|p| p.covers(path))
            .max_by_key(|p| p.path.len())
    }

    /// The proxies, by path.
    pub(crate) fn iter(&self) -> std::slice::Iter<'_, Proxy> {
        self.all.iter()
    }
}

/// One `[[server.proxies]]` entry.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    from: String,
    to: String,
}

/// The configuration's `[[server.proxies]]`, checked (the rest of `[server]` is not read).
fn configured(cfg: &Config) -> Result<Vec<Proxy>, ServeError> {
    let Some(v) = cfg.raw.get_path("server.proxies") else {
        return Ok(Vec::new());
    };
    let raw: Vec<Raw> = ssg_config::de::from_value(v).map_err(|e| ServeError::Proxy {
        key: format!("server.proxies{}", e.dotted_path()),
        message: e.message,
    })?;
    let mut proxies: Vec<Proxy> = Vec::with_capacity(raw.len());
    for (i, r) in raw.iter().enumerate() {
        let error = |field: &str, message: String| ServeError::Proxy {
            key: format!("server.proxies[{i}].{field}"),
            message,
        };
        let p = Proxy {
            path: path(&r.from).map_err(|m| error("from", m))?,
            to: origin(&r.to).map_err(|m| error("to", m))?,
        };
        if let Some(j) = proxies.iter().position(|o| o.path == p.path) {
            return Err(error(
                "from",
                format!("{:?} is the path of server.proxies[{j}] too", r.from),
            ));
        }
        proxies.push(p);
    }
    Ok(proxies)
}

/// `from` checked, with a trailing slash: `/api` and `/api/` are `/api/`.
fn path(from: &str) -> Result<String, String> {
    let Some(rest) = from.strip_prefix('/') else {
        return Err(format!("{from:?} is not a path (such as /api/)"));
    };
    if from.contains(['?', '#']) {
        return Err(format!("{from:?} is a path without a query or fragment"));
    }
    let rest = rest.strip_suffix('/').unwrap_or(rest);
    if rest.is_empty() {
        return Err(format!(
            "{from:?} would send the whole site to the other server: name its paths (such as /api/)"
        ));
    }
    if rest.split('/').any(|s| matches!(s, "" | "." | "..")) {
        return Err(format!(
            "{from:?} has an empty, . or .. segment (such as /api/)"
        ));
    }
    Ok(format!("/{rest}/"))
}

/// `to` checked: an `http://` URL with a host and nothing after it, since requests keep their
/// path.
fn origin(to: &str) -> Result<Origin, String> {
    let example = "such as http://127.0.0.1:3000";
    let uri: Uri = to
        .parse()
        .map_err(|_| format!("{to:?} is not a URL ({example})"))?;
    match uri.scheme_str() {
        Some("http") => {}
        Some("https") => {
            return Err(format!(
                "{to:?}: https is not supported; the server is an http:// one ({example})"
            ));
        }
        _ => return Err(format!("{to:?} is not an http:// URL ({example})")),
    }
    let authority = match uri.authority() {
        Some(a) if !a.host().is_empty() && !a.as_str().contains('@') => a,
        _ => return Err(format!("{to:?} has no host, or has user info ({example})")),
    };
    if uri.path() != "/" || uri.query().is_some() || to.contains('#') {
        return Err(format!(
            "{to:?} has a path: name the server only ({example}); requests keep their path"
        ));
    }
    Ok(Origin {
        url: format!("http://{authority}"),
        host: authority.as_str().to_owned(),
        addr: format!(
            "{}:{}",
            authority.host(),
            authority.port_u16().unwrap_or(80)
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proxy(s: &str) -> Proxy {
        s.parse().unwrap_or_else(|e| panic!("{s}: {e}"))
    }

    #[test]
    fn flags() {
        let p = proxy("/api/=http://127.0.0.1:3000");
        assert_eq!((p.path(), p.url()), ("/api/", "http://127.0.0.1:3000"));
        assert_eq!(
            (p.to.host.as_str(), p.to.addr.as_str()),
            ("127.0.0.1:3000", "127.0.0.1:3000")
        );
        let p = proxy("/a/b=http://localhost/");
        assert_eq!((p.path(), p.url()), ("/a/b/", "http://localhost"));
        assert_eq!(
            (p.to.host.as_str(), p.to.addr.as_str()),
            ("localhost", "localhost:80")
        );
        let p = proxy("/ws/=http://[::1]:8080");
        assert_eq!(p.to.addr, "[::1]:8080");
        assert!(p.to.addr.parse::<std::net::SocketAddr>().is_ok());

        for (bad, says) in [
            ("/api/", "is not PATH=URL"),
            ("api/=http://h:1", "is not a path"),
            ("/=http://h:1", "the whole site"),
            ("/api/?x=http://h:1", "without a query"),
            ("/a//b/=http://h:1", "empty, . or .."),
            ("/a/../b/=http://h:1", "empty, . or .."),
            ("/api/=https://h:1", "https is not supported"),
            ("/api/=ftp://h:1", "not an http:// URL"),
            ("/api/=127.0.0.1:3000", "not an http:// URL"),
            ("/api/=http://u@h:1", "user info"),
            ("/api/=http://h:1/api", "has a path"),
            ("/api/=http://h:1/?q", "has a path"),
            ("/api/=http://h:1/#f", "has a path"),
            ("/api/=http://h:1 x", "not a URL"),
        ] {
            let e = bad.parse::<Proxy>().expect_err(bad);
            assert!(e.contains(says), "{bad}: {e}");
        }
    }

    #[test]
    fn paths_are_covered_by_segments() {
        let api = proxy("/api=http://h:1");
        for path in ["/api", "/api/", "/api/x", "/api/x/y.json"] {
            assert!(api.covers(path), "{path}");
        }
        for path in ["/apix", "/ap", "/", "/x/api/"] {
            assert!(!api.covers(path), "{path}");
        }
        let proxies = Proxies {
            flags: Vec::new(),
            all: vec![proxy("/api/=http://a:1"), proxy("/api/v2/=http://b:1")],
        };
        let to = |path| proxies.find(path).map(Proxy::url);
        assert_eq!(to("/api/v1/x"), Some("http://a:1"));
        assert_eq!(to("/api/v2"), Some("http://b:1"));
        assert_eq!(to("/api/v2/x"), Some("http://b:1"));
        assert_eq!(to("/about/"), None);
    }
}
