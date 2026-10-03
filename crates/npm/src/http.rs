//! The installer's HTTP client: `ureq` (rustls with ring), on tokio's blocking pool.

use std::time::Duration;

use deno_error::JsErrorBox;
use deno_npm_cache::{
    DownloadError, NpmCacheHttpClient, NpmCacheHttpClientBytesResponse, NpmCacheHttpClientResponse,
};
use deno_npmrc::RegistryConfig;
use url::Url;

/// Abbreviated package documents when the registry has them (npm's own client asks the same).
const ACCEPT: &str = "application/vnd.npm.install-v1+json; q=1.0, application/json; q=0.8, */*";
/// The largest response read (a tarball or a package document).
const LIMIT: u64 = 1 << 30;
/// Attempts per download: network errors and 5xx responses are retried.
const ATTEMPTS: u32 = 3;

#[derive(Debug)]
pub(crate) struct Http {
    agent: ureq::Agent,
}

impl Http {
    pub(crate) fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(30)))
            .timeout_global(Some(Duration::from_secs(600)))
            .user_agent(format!("{}/{}", ssg_base::APP_NAME, ssg_base::VERSION))
            .build()
            .new_agent();
        Self { agent }
    }
}

fn failed(status_code: Option<u16>, message: String) -> DownloadError {
    DownloadError {
        status_code,
        error: JsErrorBox::generic(message),
    }
}

/// One GET; `Err` with whether it may be retried.
fn get(
    agent: &ureq::Agent,
    url: &Url,
    auth: Option<&str>,
    etag: Option<&str>,
) -> Result<NpmCacheHttpClientResponse, (bool, DownloadError)> {
    let mut req = agent.get(url.as_str()).header("Accept", ACCEPT);
    if let Some(auth) = auth {
        req = req.header("Authorization", auth);
    }
    if let Some(etag) = etag {
        req = req.header("If-None-Match", etag);
    }
    let mut resp = req
        .call()
        .map_err(|e| (true, failed(None, format!("{url}: {e}"))))?;
    match resp.status().as_u16() {
        404 => return Ok(NpmCacheHttpClientResponse::NotFound),
        304 => return Ok(NpmCacheHttpClientResponse::NotModified),
        200..=299 => {}
        s => return Err((s >= 500, failed(Some(s), format!("{url}: HTTP {s}")))),
    }
    let etag = resp
        .headers()
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let bytes = resp
        .body_mut()
        .with_config()
        .limit(LIMIT)
        .read_to_vec()
        .map_err(|e| (true, failed(None, format!("{url}: {e}"))))?;
    Ok(NpmCacheHttpClientResponse::Bytes(
        NpmCacheHttpClientBytesResponse { bytes, etag },
    ))
}

#[async_trait::async_trait(?Send)]
impl NpmCacheHttpClient for Http {
    async fn download_with_retries_on_any_tokio_runtime(
        &self,
        url: Url,
        maybe_auth: Option<String>,
        maybe_etag: Option<String>,
        _registry: Option<&RegistryConfig>,
    ) -> Result<NpmCacheHttpClientResponse, DownloadError> {
        let agent = self.agent.clone();
        tokio::task::spawn_blocking(move || {
            let mut attempt = 1;
            loop {
                match get(&agent, &url, maybe_auth.as_deref(), maybe_etag.as_deref()) {
                    Ok(r) => return Ok(r),
                    Err((true, _)) if attempt < ATTEMPTS => {
                        std::thread::sleep(Duration::from_millis(250 << attempt));
                        attempt += 1;
                    }
                    Err((_, e)) => return Err(e),
                }
            }
        })
        .await
        .map_err(|e| failed(None, e.to_string()))?
    }
}
