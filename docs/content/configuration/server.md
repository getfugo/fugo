---
title: Development server
description: The settings of fugo server — proxies that send the requests of a path, such as your site's API, to another server while you develop.
weight: 105
---

`fugo server` reads one part of `[server]`: its proxies. The Go implementation's
`[[server.headers]]` and `[[server.redirects]]` are not read.

## Proxies

A site whose pages call an API on their own origin (`fetch("/api/items")`) is published next
to that API. While you develop, the API runs as a program of its own, on another port. A proxy
sends the requests of a path to it, so that the pages `fugo server` serves reach the API as
they will once published: on their own origin, with their cookies, without CORS.

{{< code-toggle file=config >}}
[[server.proxies]]
  from = "/api/"
  to = "http://127.0.0.1:3000"
{{< /code-toggle >}}

`from`
: A path of the site. The requests of that path and of every path below it go to the server:
  `/api/` covers `/api`, `/api/` and `/api/items`, not `/apiary/`. It may not be `/`.

`to`
: The server, an `http://` URL without a path. Requests keep their path and query:
  `/api/items?page=2` goes to `http://127.0.0.1:3000/api/items?page=2`.

A request goes to the proxy of the longest `from` that covers it. The server's own paths come
first (`livereload.js`, the LiveReload WebSocket and the [CMS editor](/configuration/cms/)'s
API), and every other path is the site's.

Requests go with their method, headers and body, and with the `Host` the browser sent;
`X-Forwarded-For`, `X-Forwarded-Host` and `X-Forwarded-Proto` say where they came from.
Answers stream back as the server writes them, so server-sent events work, and WebSockets go
through. A request the server does not answer, because it is not running for example, gets
`502 Bad Gateway`, and `fugo server` prints the error.

`--proxy PATH=URL` adds a proxy for one run, and replaces the configuration's proxy of the same
path. It can be given more than once:

```sh
fugo server --proxy /api/=http://127.0.0.1:3000
```

## An API next to the site

Any web framework can serve the API, and the built site with it once published. With
[axum](https://github.com/tokio-rs/axum) and
[tower-http](https://github.com/tower-rs/tower-http):

```rust
use axum::{Json, Router, routing::get};
use tower_http::services::{ServeDir, ServeFile};

#[tokio::main]
async fn main() {
    // The site that `fugo` built, with its 404 page.
    let site = ServeDir::new("public").not_found_service(ServeFile::new("public/404.html"));
    let app = Router::new()
        .route("/api/items", get(|| async { Json(["one", "two"]) }))
        .fallback_service(site);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

While you develop, run it and `fugo server --proxy /api/=http://127.0.0.1:3000`, and open the
site at <http://localhost:1313/>: pages reload as you edit them, and their calls to `/api/` go
to the program. To publish, build the site with `fugo` and run the program, which serves both.
