//! Getting a remote resource (`resources.GetRemote`): from the cache when it is fresh, else fetched
//! and cached.

use super::*;

impl ResourceStore {
    /// `resources.GetRemote url options` for a template of `lang`: `Ok(None)` for a 404.
    ///
    /// # Errors
    /// See [`RemoteError`].
    pub fn get_remote(
        &self,
        lang: LangIdx,
        url: &str,
        options: &RemoteOptions,
    ) -> Result<Option<ResourceId>, RemoteError> {
        let parsed = UrlRef::parse(url).map_err(|e| RemoteError::InvalidUrl {
            url: url.to_owned(),
            reason: e.to_string(),
        })?;
        if !matches!(
            parsed.scheme().to_ascii_lowercase().as_str(),
            "http" | "https"
        ) {
            return Err(RemoteError::UnsupportedScheme {
                url: url.to_owned(),
            });
        }
        let rc = &self.cfg.remote;
        if !rc.http_urls.accepts(url) {
            return Err(RemoteError::NotAllowed {
                policy: "urls",
                value: url.to_owned(),
            });
        }
        if !rc.http_methods.accepts(&options.method) {
            return Err(RemoteError::NotAllowed {
                policy: "methods",
                value: options.method.clone(),
            });
        }
        let lang = self.global_lang(lang);
        let slot: Slot = Arc::clone(
            lock(&self.remote.memo)
                .entry((lang, options.identity(url)))
                .or_default(),
        );
        let mut slot = lock(&slot);
        if let Some(r) = &*slot {
            return r.clone();
        }
        let result = self.fetch_remote(lang, url, &parsed, options);
        *slot = Some(result.clone());
        result
    }

    pub(super) fn fetch_remote(
        &self,
        lang: LangIdx,
        url: &str,
        parsed: &UrlRef,
        o: &RemoteOptions,
    ) -> Result<Option<ResourceId>, RemoteError> {
        let (go_user_key, _) = go_keys(url, o.raw.as_ref());
        let res = self.cached_response(url, o, &go_user_key)?;
        if res.code == 404 {
            return Ok(None);
        }
        if !(200..300).contains(&res.code) {
            return Err(RemoteError::Status {
                url: url.to_owned(),
                code: res.code,
                status: res.status.clone(),
                data: res.data(&o.response_headers, !o.is_head()),
            });
        }
        let body = if o.is_head() {
            Vec::new()
        } else {
            res.body.clone()
        };

        let url_path = unescape(&String::from_utf8_lossy(parsed.path()), Component::Path)
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        let filename = res
            .header("Content-Disposition")
            .and_then(disposition_filename)
            .unwrap_or_else(|| paths::base(&url_path).to_owned());
        let content_type = res.header("Content-Type").unwrap_or_default();
        let media_type = self
            .remote_media_type(o.is_head(), content_type, &filename, &body)
            .ok_or_else(|| RemoteError::MediaType {
                url: url.to_owned(),
            })?;
        let stem = paths::trim_ext(&filename);
        let suffix = if media_type.suffixes.is_empty() {
            String::new()
        } else {
            media_type.full_suffix()
        };
        let link = format!("/{stem}_{go_user_key}{suffix}");
        let data = res.data(&o.response_headers, false);
        Ok(Some(self.push(NewResource {
            origin: Origin::Remote {
                url: url.to_owned(),
            },
            media_type,
            name: link.clone(),
            name_normalized: Some(link.clone()),
            title: link.clone(),
            params: Params::default(),
            data,
            lang,
            target: self.global_target(lang, &link),
            link: UrlPath::new(&link),
            body: Body::Bytes(body.into()),
            policy: PublishPolicy::OnReference,
            kind: None,
        })))
    }

    /// The response from the cache, from an imported entry of the Go build, or from the network.
    pub(super) fn cached_response(
        &self,
        url: &str,
        o: &RemoteOptions,
        go_user_key: &str,
    ) -> Result<Response, RemoteError> {
        let rc = &self.cfg.remote;
        let ours = rc.cache_dir.as_ref().map(|d| d.join(cache_key(url, o)));
        if let Some(p) = &ours
            && is_fresh(p, rc.max_age)
        {
            let bytes = fs::read(p).map_err(|e| cache_error(p, &e))?;
            return Response::parse(&bytes).ok_or_else(|| RemoteError::Cache {
                path: p.clone(),
                reason: "not an HTTP response".into(),
            });
        }
        let import_from = rc.cache_dir.iter().chain(&rc.import_dirs);
        for dir in import_from {
            let go_entry = dir.join(go_user_key);
            if !is_fresh(&go_entry, rc.max_age) {
                continue;
            }
            let bytes = fs::read(&go_entry).map_err(|e| cache_error(&go_entry, &e))?;
            if let Some(res) = Response::parse(&bytes) {
                if let Some(p) = &ours {
                    write_entry(p, &bytes)?;
                }
                return Ok(res);
            }
        }
        if !rc.network {
            return Err(RemoteError::Offline {
                url: url.to_owned(),
            });
        }
        let res = fetch(url, o, rc.timeout)?;
        let redirect = matches!(res.code, 301 | 302 | 303 | 307 | 308);
        let keep = !matches!(rc.max_age, MaxAge::For(age) if age.is_zero());
        if let Some(p) = &ours
            && !redirect
            && keep
        {
            write_entry(p, &res.to_bytes())?;
        }
        Ok(res)
    }

    /// The media type of a response: a trusted `Content-Type` (HEAD requests, or types
    /// `security.http.mediaTypes` allows), else what the content looks like, narrowed by the
    /// extensions of the `Content-Type` and of the file name.
    pub(super) fn remote_media_type(
        &self,
        is_head: bool,
        content_type: &str,
        filename: &str,
        body: &[u8],
    ) -> Option<MediaType> {
        let types = &self.cfg.media_types;
        let essence = MediaType::parse(content_type).ok();
        if (is_head || self.cfg.remote.http_media_types.accepts(content_type))
            && let Some(e) = &essence
        {
            return Some(
                types
                    .by_type(&e.type_string())
                    .map_or_else(|| e.clone(), |id| types.get(id).clone()),
            );
        }
        let mut hints: Vec<String> = if content_type.starts_with("text/plain") {
            vec!["txt".to_owned()]
        } else {
            essence
                .as_ref()
                .and_then(|e| mime_guess::get_mime_extensions_str(&e.type_string()))
                .map(|exts| {
                    let mut v: Vec<String> = exts.iter().map(|&x| x.to_owned()).collect();
                    v.sort();
                    v
                })
                .unwrap_or_default()
        };
        if hints.first().is_none_or(|h| h == "txt") {
            let ext = paths::ext_no_delimiter(filename);
            if !ext.is_empty() {
                hints = vec![ext.to_ascii_lowercase()];
            }
        }
        from_content(types, &hints, body)
    }
}

pub(super) fn write_entry(path: &Path, bytes: &[u8]) -> Result<(), RemoteError> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| cache_error(dir, &e))?;
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).map_err(|e| cache_error(&tmp, &e))?;
    fs::rename(&tmp, path).map_err(|e| cache_error(path, &e))
}

/// Fetches `url` (no retries; a status outside 2xx is a response, not an error).
pub(super) fn fetch(
    url: &str,
    o: &RemoteOptions,
    timeout: Duration,
) -> Result<Response, RemoteError> {
    let net = |e: &dyn std::fmt::Display| RemoteError::Network {
        url: url.to_owned(),
        reason: e.to_string(),
    };
    let config = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(timeout))
        .user_agent(ssg_base::APP_NAME)
        .build();
    let agent = ureq::Agent::new_with_config(config);
    let mut b = ureq::http::Request::builder()
        .method(o.method.as_str())
        .uri(url);
    for (k, vs) in &o.headers {
        for v in vs {
            b = b.header(k.as_str(), v.as_str());
        }
    }
    let res = if o.body.is_empty() {
        agent.run(b.body(()).map_err(|e| net(&e))?)
    } else {
        agent.run(b.body(o.body.clone()).map_err(|e| net(&e))?)
    }
    .map_err(|e| net(&e))?;
    let status = res.status();
    let code = status.as_u16();
    let status_text = match status.canonical_reason() {
        Some(r) => format!("{code} {r}"),
        None => code.to_string(),
    };
    let headers = res
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                canonical_header(k.as_str()),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();
    let body = res
        .into_body()
        .with_config()
        .limit(1 << 30)
        .read_to_vec()
        .map_err(|e| net(&e))?;
    Ok(Response {
        status: status_text,
        code,
        headers,
        body,
    })
}
