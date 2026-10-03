//! Where the server listens and the base URLs it builds the site with (Go's
//! `createServerPorts` and `fixURL`).

use std::io;
use std::net::TcpListener;

use ssg_base::url::{UrlError, UrlRef};

use crate::{Event, Port, Reporter, ServeError};

/// The base URL a language is built with when served on `port`: the `--baseURL` flag, else
/// the configured base URL on `localhost` (over `http`); with `append_port`, the port
/// replaces the URL's own. A base URL without a host (`/`) gets `//localhost:<port>/`, as in
/// Go.
pub(crate) fn server_base_url(
    configured: &str,
    flag: Option<&str>,
    port: u16,
    append_port: bool,
) -> Result<String, UrlError> {
    let use_localhost = flag.is_none();
    let mut base = flag.unwrap_or(configured).to_owned();
    if !base.ends_with('/') {
        base.push('/');
    }
    let mut url = UrlRef::parse(&base)?;
    if !url.has_host() && base != "/" {
        url = UrlRef::parse(&format!("//{base}"))?;
    }
    if use_localhost {
        if url.scheme() == "https" {
            url.set_scheme("http");
        }
        url.set_host("localhost");
    }
    if append_port {
        let name = url.hostname().to_vec();
        let mut host = if name.contains(&b':') {
            [b"[", name.as_slice(), b"]"].concat()
        } else {
            name
        };
        host.extend_from_slice(format!(":{port}").as_bytes());
        url.set_host(host);
    }
    Ok(url.to_string())
}

/// Opens `count` listeners on `bind` (one per language of a multihost site): the requested
/// port, then each next one; a busy port is replaced by a free one unless it was asked for
/// explicitly (`--port`) and is the first.
pub(crate) fn listen(
    bind: &str,
    port: Port,
    count: usize,
    reporter: &dyn Reporter,
) -> Result<Vec<TcpListener>, ServeError> {
    let (mut next, explicit) = match port {
        Port::Exact(p) => (p, true),
        Port::Preferred(p) => (p, false),
    };
    let mut listeners = Vec::with_capacity(count);
    for i in 0..count {
        let listener = match TcpListener::bind((bind, next)) {
            Ok(l) => l,
            Err(source) if i == 0 && explicit => {
                return Err(ServeError::Bind {
                    address: format!("{bind}:{next}"),
                    source,
                });
            }
            Err(_) => {
                reporter.report(&Event::PortInUse { port: next });
                TcpListener::bind((bind, 0)).map_err(|source| ServeError::Bind {
                    address: format!("{bind}:0"),
                    source,
                })?
            }
        };
        let port = local_port(&listener)?;
        // Port 0 asks the system for each listener; otherwise the next one is tried.
        if next != 0 {
            next = port.saturating_add(1);
        }
        listeners.push(listener);
    }
    Ok(listeners)
}

pub(crate) fn local_port(l: &TcpListener) -> Result<u16, ServeError> {
    l.local_addr()
        .map(|a| a.port())
        .map_err(|source: io::Error| ServeError::Bind {
            address: "listener".to_owned(),
            source,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_urls() {
        let fix =
            |configured, flag, append| server_base_url(configured, flag, 1313, append).unwrap();
        assert_eq!(
            fix("https://example.org/", None, true),
            "http://localhost:1313/"
        );
        assert_eq!(
            fix("https://example.org/docs", None, true),
            "http://localhost:1313/docs/"
        );
        assert_eq!(
            fix("http://example.org:8080/", None, true),
            "http://localhost:1313/"
        );
        assert_eq!(fix("/", None, true), "//localhost:1313/");
        assert_eq!(fix("example.org/sub/", None, true), "//localhost:1313/sub/");
        assert_eq!(
            fix("https://example.org/", None, false),
            "http://localhost/"
        );
        assert_eq!(
            fix(
                "https://example.org/",
                Some("https://dev.example:8443/x/"),
                true
            ),
            "https://dev.example:1313/x/"
        );
        assert_eq!(
            fix(
                "https://example.org/",
                Some("https://dev.example:8443/x/"),
                false
            ),
            "https://dev.example:8443/x/"
        );
        assert_eq!(fix("", Some("http://[::1]:9/"), true), "http://[::1]:1313/");
    }
}
