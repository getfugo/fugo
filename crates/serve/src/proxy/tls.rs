//! The TLS of the proxies to `https://` servers: rustls with the ring provider, the server's
//! certificate checked against Mozilla's roots (webpki-roots, as GetRemote's ureq checks them)
//! and those of [`ServeOptions::proxy_roots`](crate::ServeOptions::proxy_roots), with ALPN
//! asking for HTTP/1.1, the only version the forwarding speaks.

use std::sync::Arc;

use rustls::pki_types::CertificateDer;
use rustls::{ClientConfig, RootCertStore};

/// The client configuration of a server's proxies to `https://` servers, trusting `roots` as
/// well as Mozilla's.
pub(crate) fn client_config(roots: &[CertificateDer<'static>]) -> Arc<ClientConfig> {
    let mut trusted = RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    trusted.add_parsable_certificates(roots.iter().cloned());
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("ring has the cipher suites of the default versions")
        .with_root_certificates(trusted)
        .with_no_client_auth();
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Arc::new(config)
}
