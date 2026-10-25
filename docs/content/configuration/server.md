---
title: Development server
description: The settings of fugo server — proxies that send the requests of a path, such as your site's API, to another server while you develop, on this computer or published.
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
: The server, an `http://` or `https://` URL without a path. Requests keep their path and
  query: `/api/items?page=2` goes to `http://127.0.0.1:3000/api/items?page=2`.

A request goes to the proxy of the longest `from` that covers it. The server's own paths come
first (`livereload.js`, the LiveReload WebSocket and the [CMS editor](/configuration/cms/)'s
API), and every other path is the site's.

Requests go with their method, headers and body, and with the `Host` the browser sent (an
`https://` server's own name, [below](#a-published-api)); `X-Forwarded-For`,
`X-Forwarded-Host` and `X-Forwarded-Proto` say where they came from.
Answers stream back as the server writes them, so server-sent events work, and WebSockets go
through. A request the server does not answer, because it is not running for example, gets
`502 Bad Gateway`, and `fugo server` prints the error.

`--proxy PATH=URL` adds a proxy for one run, and replaces the configuration's proxy of the same
path. It can be given more than once:

```sh
fugo server --proxy /api/=http://127.0.0.1:3000
```

## A published API

A proxy can also send the pages' calls to the API as it is published, so that you work on the
site with its real data:

{{< code-toggle file=config >}}
[[server.proxies]]
  from = "/api/"
  to = "https://api.example.org"
{{< /code-toggle >}}

`fugo server` reaches an `https://` server in TLS and checks its certificate against the
authorities browsers trust (Mozilla's list): a server whose certificate comes from another
authority, such as a self-signed one, gets `502 Bad Gateway`. The requests carry the server's
own name as their `Host`, which TLS and a server of several sites need, and the browser's host
in `X-Forwarded-Host`.

The answers come back as the server wrote them: a cookie it sets for its own domain
(`Domain=api.example.org`) does not reach the pages, and a redirect to its own address leaves
the site.

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
