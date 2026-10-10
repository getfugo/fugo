//! The API of the CMS editor of `[cms]` (`<path>api/`): the server answers it itself, from the
//! local git repository (`ssg_cms::local`), where a published site has its Worker. Git runs
//! on the blocking pool.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::request::Parts;
use axum::http::{HeaderName, HeaderValue, StatusCode, header};
use axum::response::Response;
use ssg_cms::Editor;
use ssg_cms::local::{Answer, Call, HEADERS};

/// The answer to request `req` (with `body`) of API `name` (`file`, `save`).
pub(crate) async fn answer(editor: Arc<Editor>, req: &Parts, body: Body, name: &str) -> Response {
    let answer = match axum::body::to_bytes(body, editor.body_limit()).await {
        Ok(bytes) => {
            let header = |name: HeaderName| {
                req.headers
                    .get(name)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_owned)
            };
            let call = Call {
                method: req.method.as_str().to_owned(),
                name: name.to_owned(),
                query: req.uri.query().unwrap_or_default().to_owned(),
                host: header(header::HOST),
                loopback: req
                    .extensions
                    .get::<ConnectInfo<SocketAddr>>()
                    .is_some_and(|c| c.0.ip().to_canonical().is_loopback()),
                origin: header(header::ORIGIN),
                content_type: header(header::CONTENT_TYPE),
                body: bytes.to_vec(),
            };
            tokio::task::spawn_blocking(move || editor.answer(&call))
                .await
                .unwrap_or_else(|e| Answer {
                    status: 500,
                    body: serde_json::json!({ "error": format!("internal error: {e}") })
                        .to_string(),
                })
        }
        Err(_) => Answer::too_large(),
    };
    let mut response = Response::new(Body::from(answer.body));
    *response.status_mut() =
        StatusCode::from_u16(answer.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    for (name, value) in HEADERS {
        response.headers_mut().insert(
            HeaderName::from_static(name),
            HeaderValue::from_static(value),
        );
    }
    response
}
