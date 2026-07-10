---
title: "Serving Static Files with Axum"
description: "Serve static files in Axum with tower-http's ServeDir and ServeFile, the equivalent of Express express.static, plus the SPA index.html fallback that returns 200."
---

## Quick Overview

Almost every web service eventually needs to hand a browser some bytes off disk — a built **single-page app** (SPA), an `index.html`, CSS/JS bundles, images, a `robots.txt`. In Express you reach for the built-in `express.static` middleware; in Axum you mount a **`tower-http` file-serving service**: `ServeDir` for a whole directory tree, `ServeFile` for one specific file. This page shows how to wire those services into a `Router`, how to combine them with real API routes, and how to do the one thing every SPA needs: a **fallback to `index.html`** so client-side routes like `/dashboard/settings` load the app instead of returning a 404.

> **Note:** This page targets **axum 0.8** (recorded with 0.8.9) and **tower-http 0.6** (recorded with 0.6.11). The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition; `cargo new` selects that edition automatically. Servers start with `axum::serve(listener, app)` and a `tokio::net::TcpListener`. File-serving lives behind tower-http's opt-in `fs` feature.

---

## TypeScript/JavaScript Example

In Express, static file serving is a single line: `express.static` returns a middleware that, for each request, tries to find a matching file under a root directory and streams it, complete with the right `Content-Type`, `Last-Modified`, `ETag`, conditional-request (304) handling, and an optional `maxAge` cache header. Anything it does not find, it passes to the next middleware.

```typescript
// server.ts — Express 5
// npm install express
// npm install -D @types/express
import express from "express";

const app = express();

// 1) Serve everything under ./public at the URL root.
//    GET /            -> public/index.html (default index)
//    GET /style.css   -> public/style.css
//    GET /js/app.js   -> public/js/app.js
app.use(express.static("public", { maxAge: "1h" }));

// 2) A real API route still works — it is just another middleware.
app.get("/api/health", (_req, res) => {
  res.json({ status: "ok" });
});

app.listen(3000, () => {
  console.log("listening on http://127.0.0.1:3000");
});
```

Running this and probing it (with Node v22's built-in `fetch`) shows the behavior a TypeScript developer relies on:

```text
/            -> 200 text/html; charset=UTF-8 cc=public, max-age=3600
/api/health  -> 200 application/json; charset=utf-8 cc=null
/missing.html -> 404 text/html; charset=utf-8 cc=null
```

So `express.static` (a) serves `index.html` for a directory, (b) applies the `maxAge` cache header, and (c) returns a 404 for files it cannot find. For a single-page app you typically add one more line so that unknown paths return `index.html`:

```typescript
// SPA fallback: any GET that did not match a file or API route gets index.html,
// so the client-side router (React Router, etc.) can take over.
import path from "node:path";

app.get("/{*splat}", (_req, res) => {
  res.sendFile(path.resolve("public/index.html"));
});
```

Three jobs, then: serve a directory, serve one file, and fall back to the app shell. Axum has a dedicated, well-tested service for each.

---

## Rust Equivalent

The file-serving services live in `tower-http`, gated behind the `fs` feature. Add the dependencies in a fresh project (`cargo new static-demo`):

```bash
cargo add axum
cargo add tokio --features full
cargo add tower-http --features fs
```

```toml
# Cargo.toml (resolved versions)
[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
tower-http = { version = "0.6", features = ["fs"] }
```

> **Note:** tower-http's features are opt-in. `ServeDir`/`ServeFile` need `fs`; the same `fs` feature also enables `.precompressed_gzip()` and friends shown later. You do **not** need a separate `compression-gzip` feature just to serve pre-built `.gz` files.

Here is the direct equivalent of the Express example: `ServeDir` for the directory, `ServeFile` for a single named file, and an ordinary handler for the API:

```rust
use axum::{routing::get, Json, Router};
use serde_json::{json, Value};
use tower_http::services::{ServeDir, ServeFile};

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

fn app() -> Router {
    // Serve a single file at a specific route.
    let favicon = ServeFile::new("public/favicon.ico");

    // Serve a whole directory tree.
    let static_files = ServeDir::new("public");

    Router::new()
        // Real API routes take priority because they are matched first.
        .route("/api/health", get(health))
        // `route_service` mounts a tower Service (not a handler) at one path.
        .route_service("/favicon.ico", favicon)
        // Anything not matched above is looked up on disk under ./public.
        .fallback_service(static_files)
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3007").await.unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app()).await.unwrap();
}
```

This also needs `serde_json` (`cargo add serde_json`). Running it and probing with `curl -i` produces this **real output**:

```text
$ curl -s -i http://127.0.0.1:3007/
HTTP/1.1 200 OK
content-type: text/html
accept-ranges: bytes
last-modified: Mon, 01 Jun 2026 12:13:21 GMT
content-length: 65

<!doctype html><title>Home</title><h1>Hello from index.html</h1>

$ curl -s -i http://127.0.0.1:3007/assets/app.css
HTTP/1.1 200 OK
content-type: text/css
accept-ranges: bytes
last-modified: Mon, 01 Jun 2026 12:13:22 GMT
content-length: 33

body { font-family: system-ui; }

$ curl -s -i http://127.0.0.1:3007/api/health
HTTP/1.1 200 OK
content-type: application/json
content-length: 15

{"status":"ok"}

$ curl -s -i http://127.0.0.1:3007/does-not-exist.html
HTTP/1.1 404 Not Found
content-length: 0
```

Notice what `ServeDir` did for free, exactly like `express.static`: it guessed `text/html` and `text/css` from the file extension, set `Last-Modified` and `Accept-Ranges`, served `index.html` for the `/` request, and returned a 404 for the missing file. You wrote zero of that logic.

---

## Detailed Explanation

### `ServeDir` — a directory of files

`ServeDir::new("public")` builds a tower **`Service`** (not an Axum handler) that, for each request, maps the request path onto a path under `public/` and streams the file if it exists. It handles, out of the box:

- **MIME type** inference from the file extension (`text/css`, `text/javascript`, `image/png`, …).
- **`index.html` for directories**: a request for `/` (or `/docs/`) serves `public/index.html` (or `public/docs/index.html`). This is on by default; disable it with `.append_index_html_on_directories(false)`.
- **Conditional requests**: it honors `If-Modified-Since` and `If-None-Match` and replies `304 Not Modified` (more on this below).
- **Range requests**: `Accept-Ranges: bytes` plus partial `206` responses for video scrubbing and resumable downloads.
- **Path traversal protection**: a request for `/../../etc/passwd` is rejected; you cannot escape the served root.

Because `ServeDir` is a `Service` and not a `fn`, you mount it with `fallback_service`, `route_service`, or `nest_service`, never with `get(...)`, which expects a handler (see [Common Pitfalls](#common-pitfalls)).

### `ServeFile` — one specific file

`ServeFile::new("public/favicon.ico")` is the single-file cousin: every request routed to it serves that one file, ignoring the request path. It is what you use to pin a known file to a known route (`/favicon.ico`, `/robots.txt`) and, importantly, it is the building block of the SPA fallback below.

### Routing precedence: API first, files last

The order in the `Router` matters conceptually, not lexically: Axum matches **declared routes** (`/api/health`, `/favicon.ico`) before consulting the **fallback service**. So real endpoints always win, and only requests that match no route reach `ServeDir`. The `curl /api/health` output above proves it: the JSON handler answered, not a file lookup.

> **Tip:** `fallback_service` takes a `Service`; the plain `fallback` takes a handler. Use `fallback_service(ServeDir::new(...))` for files and `fallback(handler)` for a custom 404 handler. They are mutually exclusive on a given router — pick the one that matches what you are mounting.

### The SPA fallback: serving `index.html` for unknown routes

A single-page app ships a built bundle (often under `dist/`) and does its own routing in the browser. When a user reloads on `/dashboard/settings`, the server must return the app shell (`index.html`) so the JS router can render that view. Returning a 404 would break deep links and refreshes. There are **two ways** to express this in tower-http, and they differ in one important detail: the **status code**.

```rust
use axum::Router;
use tower_http::services::{ServeDir, ServeFile};

fn app() -> Router {
    // Serve real files from ./dist; for any missing path, fall back to the SPA
    // shell. `.fallback` here is ServeDir's OWN fallback (a tower-http method),
    // and it preserves a 200 status — what an SPA wants.
    let spa = ServeDir::new("dist")
        .fallback(ServeFile::new("dist/index.html"));

    Router::new().fallback_service(spa)
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3009").await.unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app()).await.unwrap();
}
```

A deep link returns the shell with a **200 OK**, verified:

```text
$ curl -s -i http://127.0.0.1:3009/settings/profile
HTTP/1.1 200 OK
content-type: text/html
accept-ranges: bytes
last-modified: Mon, 01 Jun 2026 12:17:41 GMT
```

`ServeDir` also has a `.not_found_service(...)` method. It looks almost identical but behaves differently: it serves the file with the **404 status** that triggered the fallback. That is correct for a *custom 404 page*, but wrong for an SPA shell, where you want a 200. Keep the two straight:

| Method | Use it for | Resulting status |
| --- | --- | --- |
| `ServeDir::fallback(ServeFile::new("index.html"))` | SPA app shell on deep links | **200 OK** |
| `ServeDir::not_found_service(ServeFile::new("404.html"))` | A real custom error page | **404 Not Found** |

This is a genuinely easy mistake to make; the names suggest they are interchangeable, but the status code is exactly the thing your CDN, SEO crawler, and uptime monitor care about.

---

## Key Differences

| Concern | Express (`express.static`) | Axum (`tower-http`) |
| --- | --- | --- |
| What it is | A middleware function in the chain | A tower **`Service`** mounted on the router |
| Mounting | `app.use(express.static(dir))` | `.fallback_service(ServeDir::new(dir))` |
| Single file | `res.sendFile(path)` in a handler | `ServeFile::new(path)` via `route_service` |
| MIME type | Inferred (via `mime` package) | Inferred (via `mime_guess`) |
| Conditional GET / 304 | Built in | Built in |
| Range requests | Built in | Built in |
| SPA fallback | An extra catch-all `app.get("/{*splat}", …)` | `ServeDir::fallback(ServeFile::new("index.html"))` |
| Cache-Control | `{ maxAge }` option | A separate `SetResponseHeaderLayer` (tower) |
| Pre-compressed assets | Needs `serve-static` config / a plugin | `.precompressed_gzip()` etc., built in |
| Missing root dir | Errors per-request | Silently 404s per-request (no startup check) |

The mental shift for a TypeScript developer: in Express, serving files is *middleware in a pipeline*. In Axum, it is a *self-contained service that you compose into the router tree*: the same composable `Service` abstraction that powers handlers, middleware layers, and nested routers (see [Middleware and Layers](/16-web-apis/05-middleware/) for the tower model and [Routing in Axum](/16-web-apis/03-routing/) for `nest`/`fallback`). One consequence: a cache header is not an option on the file service; it is a **layer** you wrap around it, because layers are the universal way to transform any response in tower.

### Caching: add a `Cache-Control` layer

`ServeDir` already sets `Last-Modified` and answers conditional requests, but it deliberately does **not** set `Cache-Control`. That is a policy decision you make per route. Add it with `SetResponseHeaderLayer` from tower-http's `set-header` feature (`cargo add tower-http --features fs,set-header`):

```rust
use axum::{http::header, Router};
use http::HeaderValue;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;

fn app() -> Router {
    // Build artifacts are content-hashed (app.4f2a1c.js), so they never change
    // under a given name -> cache for a year, immutable.
    let cache_forever = SetResponseHeaderLayer::overriding(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );

    Router::new()
        .nest_service("/assets", ServeDir::new("dist/assets"))
        .layer(cache_forever)
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3013").await.unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app()).await.unwrap();
}
```

This needs `http` (`cargo add http`). The header appears, and conditional requests still produce a real `304` — verified:

```text
$ curl -s -i http://127.0.0.1:3013/assets/main.js
HTTP/1.1 200 OK
content-type: text/javascript
last-modified: Mon, 01 Jun 2026 12:17:41 GMT
cache-control: public, max-age=31536000, immutable

$ curl -s -i -H "If-Modified-Since: Mon, 01 Jun 2026 12:17:41 GMT" \
       http://127.0.0.1:3013/assets/main.js
HTTP/1.1 304 Not Modified
```

That 304 was produced by `ServeDir` itself, comparing the request's `If-Modified-Since` against the file's modification time, exactly what `express.static` does, with no code from you.

### Serving pre-compressed assets

If your build step already emits `main.js.gz` next to `main.js`, `ServeDir` can serve the smaller pre-compressed file to clients that advertise `Accept-Encoding: gzip`, avoiding on-the-fly compression entirely. This is part of the `fs` feature:

```rust
use axum::Router;
use tower_http::services::ServeDir;

fn app() -> Router {
    // For a request to /assets/main.js with `Accept-Encoding: gzip`, serve
    // main.js.gz if it exists; otherwise serve main.js uncompressed.
    let assets = ServeDir::new("dist/assets").precompressed_gzip();
    Router::new().nest_service("/assets", assets)
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3014").await.unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app()).await.unwrap();
}
```

Verified: the gzip request gets `content-encoding: gzip` and the smaller body, a request without `Accept-Encoding` gets the plain file:

```text
$ curl -s -D - -o /dev/null -H "Accept-Encoding: gzip" \
       http://127.0.0.1:3014/assets/main.js
HTTP/1.1 200 OK
content-type: text/javascript
accept-ranges: bytes
content-encoding: gzip
content-length: 55

$ curl -s -D - -o /dev/null http://127.0.0.1:3014/assets/main.js
HTTP/1.1 200 OK
content-type: text/javascript
```

There are matching `.precompressed_br()` (Brotli) and `.precompressed_zstd()` methods. For on-the-fly compression of *dynamic* responses (your JSON APIs), use the separate `CompressionLayer` instead. That is covered in [Middleware and Layers](/16-web-apis/05-middleware/).

---

## Common Pitfalls

### 1. Mounting a `ServeDir` with `get(...)`

`get(...)`/`post(...)` expect a **handler** (an async fn). `ServeDir` is a tower **`Service`**, so passing it to `get` does not compile:

```rust
use axum::{routing::get, Router};
use tower_http::services::ServeDir;

fn app() -> Router {
    // does not compile (error[E0277]: ServeDir: Handler<_, _> is not satisfied)
    Router::new().route("/assets", get(ServeDir::new("dist")))
}
```

The real compiler error names the missing trait:

```text
error[E0277]: the trait bound `ServeDir: Handler<_, _>` is not satisfied
  --> src/main.rs:7:40
   |
 7 |     Router::new().route("/assets", get(ServeDir::new("dist")))
   |                                    --- ^^^^^^^^^^^^^^^^^^^^^ the trait `Handler<_, _>` is not implemented for `ServeDir`
   |                                    |
   |                                    required by a bound introduced by this call
```

The fix is to mount the service with the `_service` family of methods: `route_service("/assets/{*path}", ServeDir::new("dist"))`, `nest_service("/assets", ServeDir::new("dist"))`, or `fallback_service(ServeDir::new("dist"))`.

### 2. SPA fallback returns 404 instead of 200

Using `.not_found_service(ServeFile::new("dist/index.html"))` *does* serve the shell, but with a **404** status (it is meant for error pages). Browsers usually still render it, but CDNs may refuse to cache it, SEO crawlers treat the route as missing, and uptime checks flag the page as down. For an SPA shell use `ServeDir::fallback(...)`, which preserves the **200**. See the table in [The SPA fallback](#the-spa-fallback-serving-indexhtml-for-unknown-routes).

### 3. Relative paths resolve against the process working directory, not the binary

`ServeDir::new("dist")` is relative to the **current working directory of the running process**, not the location of the compiled binary. Run the same binary from a different directory (or from a container with a different `WORKDIR`) and it will quietly fail to find your files. Worse, a missing root directory does **not** panic at startup; it just returns 404 for every request:

```text
$ curl -s -i http://127.0.0.1:3015/anything   # ServeDir points at a non-existent dir
HTTP/1.1 404 Not Found
content-length: 0
```

In production, prefer an **absolute path** derived from configuration (e.g. a `STATIC_DIR` env var), or embed the assets into the binary (see Best Practices). See [Deploying Axum Applications](/16-web-apis/19-deployment/) for the Docker `WORKDIR` angle.

### 4. The SPA fallback swallows your API 404s

If `/api/*` shares the same catch-all `index.html` fallback, a request to a *misspelled* API endpoint like `/api/todoss` returns the HTML app shell with a 200 — and your frontend's `fetch` then tries to `JSON.parse("<!doctype html>…")` and throws a confusing error. Keep API routes under their own nested router with a JSON 404 fallback, and only let the SPA fallback catch non-API paths. The Real-World example below does exactly this.

### 5. Expecting `Cache-Control` for free

`ServeDir` sets `Last-Modified` and does 304s, but never sets `Cache-Control`. If you expected `express.static`'s `maxAge` behavior, add a `SetResponseHeaderLayer` as shown above; there is no implicit caching policy.

---

## Best Practices

- **Separate cache policies by route.** Long-cache content-hashed bundles (`/assets/*` → `max-age=31536000, immutable`), but serve `index.html` with `no-cache` (or a short max-age) so users pick up new deployments. Apply different `SetResponseHeaderLayer`s to different sub-routers.
- **Put the API on its own nested router with a JSON 404.** Mount it with `nest("/api", api_router())` so a mistyped endpoint returns JSON, not the SPA shell (pitfall #4).
- **Use `ServeDir::fallback(ServeFile::new("index.html"))` for the SPA shell** so deep links return 200.
- **Resolve the static directory from config / an absolute path** in production, never a bare relative string that depends on the launch directory.
- **Add `TraceLayer` while developing** so you can see which requests hit a file vs. the fallback (see [Middleware and Layers](/16-web-apis/05-middleware/)).
- **Consider embedding assets into the binary for single-file deploys.** Crates like `rust-embed` (with an axum integration) bake `dist/` into the executable at compile time, eliminating the working-directory problem and producing one self-contained binary — handy for `scratch`-based Docker images. `ServeDir` from disk is simpler when you deploy the assets alongside the binary or behind a reverse proxy / CDN.
- **Let a reverse proxy or CDN serve static files in production when you can.** Nginx/Cloudflare in front of your Axum app can serve `/assets/*` directly; Axum's `ServeDir` is then a convenient fallback and the source of truth in development. See [Deploying Axum Applications](/16-web-apis/19-deployment/).

---

## Real-World Example

A production-shaped app that combines all the pieces: a JSON API under `/api` (with its own JSON 404), aggressively-cached fingerprinted assets under `/assets`, an SPA shell fallback that returns 200 for deep links, and request tracing. Dependencies: `cargo add axum tokio --features full; cargo add tower-http --features fs,set-header,trace; cargo add http serde_json tracing tracing-subscriber`.

```rust
use axum::{
    http::{header, StatusCode},
    routing::get,
    Json, Router,
};
use http::HeaderValue;
use serde_json::{json, Value};
use tower_http::{
    services::{ServeDir, ServeFile},
    set_header::SetResponseHeaderLayer,
    trace::TraceLayer,
};

// --- API handlers ---------------------------------------------------------

async fn list_todos() -> Json<Value> {
    Json(json!([{ "id": 1, "title": "Wire up the SPA" }]))
}

// Any /api/* path that no route matched is a genuine 404 -> JSON, not HTML.
async fn api_fallback() -> (StatusCode, Json<Value>) {
    (StatusCode::NOT_FOUND, Json(json!({ "error": "no such endpoint" })))
}

// --- Router assembly -------------------------------------------------------

fn api_router() -> Router {
    Router::new()
        .route("/todos", get(list_todos))
        .fallback(api_fallback)
}

fn static_router() -> Router {
    // Content-hashed build artifacts never change under a given name.
    let cache_forever = SetResponseHeaderLayer::overriding(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    let assets = ServeDir::new("dist/assets");

    // Everything else falls back to the SPA shell with a 200 status, so deep
    // links such as /dashboard/settings load the app rather than a 404.
    let spa = ServeDir::new("dist").fallback(ServeFile::new("dist/index.html"));

    Router::new()
        .nest_service("/assets", assets)
        .layer(cache_forever)
        .fallback_service(spa)
}

fn build_app() -> Router {
    Router::new()
        .nest("/api", api_router())
        .merge(static_router())
        .layer(TraceLayer::new_for_http())
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_target(false).compact().init();
    let app = build_app();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3016").await.unwrap();
    tracing::info!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
```

Exercising every path proves each policy fires correctly. **Real output**:

```text
$ curl -s -i http://127.0.0.1:3016/api/todos
HTTP/1.1 200 OK
content-type: application/json
[{"id":1,"title":"Wire up the SPA"}]

$ curl -s -i http://127.0.0.1:3016/dashboard/settings   # SPA deep link
HTTP/1.1 200 OK
content-type: text/html

$ curl -s -D - -o /dev/null http://127.0.0.1:3016/assets/main.js
HTTP/1.1 200 OK
content-type: text/javascript
cache-control: public, max-age=31536000, immutable

$ curl -s -i http://127.0.0.1:3016/api/nope            # misspelled API route
HTTP/1.1 404 Not Found
content-type: application/json
{"error":"no such endpoint"}
```

Four routes, four distinct behaviors, all composed from small services and layers: the API answers JSON, the SPA deep link returns the shell at 200, the asset carries a year-long cache header, and the mistyped API path returns JSON 404 instead of leaking the HTML shell.

> **Note:** `merge` combines two routers at the same level; `nest("/api", …)` mounts a router under a path prefix. Because declared routes are matched before fallbacks, `/api/*` is handled by `api_router` and never reaches the SPA fallback. See [Routing in Axum](/16-web-apis/03-routing/).

---

## Further Reading

- [tower-http `ServeDir` docs](https://docs.rs/tower-http/latest/tower_http/services/struct.ServeDir.html) and [`ServeFile` docs](https://docs.rs/tower-http/latest/tower_http/services/struct.ServeFile.html): every option (`append_index_html_on_directories`, `precompressed_*`, `call_fallback_on_method_not_allowed`).
- [tower-http `SetResponseHeaderLayer` docs](https://docs.rs/tower-http/latest/tower_http/set_header/struct.SetResponseHeaderLayer.html) — for cache and security headers.
- [Axum `Router` docs](https://docs.rs/axum/latest/axum/struct.Router.html): `route_service`, `nest_service`, `fallback_service`, `merge`.
- [Routing in Axum](/16-web-apis/03-routing/) — method routing, path params (`{id}` in 0.8), nesting, and fallbacks.
- [Middleware and Layers](/16-web-apis/05-middleware/): the tower `Service`/`Layer` model, `TraceLayer`, and on-the-fly `CompressionLayer`.
- [Request and Response Handling](/16-web-apis/07-request-response/) — `IntoResponse`, status codes, and setting headers from handlers.
- [File Uploads](/16-web-apis/17-file-uploads/) — the inverse direction: receiving multipart uploads and streaming them to disk.
- [Deploying Axum Applications](/16-web-apis/19-deployment/) — Docker `WORKDIR`, binding `0.0.0.0`, and putting a reverse proxy / CDN in front of static assets.
- [CORS with Axum and tower-http](/16-web-apis/11-cors/): when a separate frontend origin loads these assets cross-origin.
- Foundations: [Ownership](/05-ownership/) (why paths are owned `PathBuf`s), [Async](/11-async/) (the async runtime that streams files), and [Generics & Traits](/09-generics-traits/) (the trait machinery behind `Service`).
- Once your API needs data behind those endpoints, see [Databases](/17-database/).

---

## Exercises

### Exercise 1: A documentation site with a custom 404 page

**Difficulty:** Beginner

**Objective:** Serve a directory of static HTML and return a styled custom 404 page (with a 404 status) for missing files.

**Instructions:** Create a server that serves everything under `./site` at the URL root. When a requested file does not exist, return `./site/404.html` with a **404 Not Found** status (not a 200). Use the method whose name signals "this is an error page", not the SPA-shell method.

<details>
<summary>Solution</summary>

```rust
use axum::Router;
use tower_http::services::{ServeDir, ServeFile};

fn app() -> Router {
    // `.not_found_service` serves the file WITH the 404 status that triggered
    // the miss — correct for a real error page.
    let files = ServeDir::new("site")
        .not_found_service(ServeFile::new("site/404.html"));
    Router::new().fallback_service(files)
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3017").await.unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app()).await.unwrap();
}
```

A request for a missing path returns the custom page with a real 404, verified:

```text
$ curl -s -i http://127.0.0.1:3017/whatever
HTTP/1.1 404 Not Found
content-type: text/html
accept-ranges: bytes
content-length: 69

<!doctype html><title>Not Found</title><h1>404 — nothing here</h1>
```

Contrast with `ServeDir::fallback(ServeFile::new(...))`, which would have returned **200** — that variant is for SPA shells, not error pages.

</details>

### Exercise 2: Cache assets aggressively, but never cache the HTML shell

**Difficulty:** Intermediate

**Objective:** Apply two different `Cache-Control` policies to two sub-routers.

**Instructions:** Serve content-hashed bundles under `/assets` with `public, max-age=31536000, immutable`, and serve the SPA shell (and all deep links) with `no-cache` so users always pick up new deployments. Mount each sub-router with its own `SetResponseHeaderLayer`, then combine them. Requires `cargo add tower-http --features fs,set-header` and `cargo add http`.

<details>
<summary>Solution</summary>

```rust
use axum::{http::header, Router};
use http::HeaderValue;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;

fn assets_router() -> Router {
    let long_cache = SetResponseHeaderLayer::overriding(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );
    Router::new()
        .nest_service("/assets", ServeDir::new("dist/assets"))
        .layer(long_cache)
}

fn shell_router() -> Router {
    let no_cache = SetResponseHeaderLayer::overriding(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache"),
    );
    // SPA fallback returns index.html at 200 for any unmatched path.
    let spa = ServeDir::new("dist").fallback(ServeFile::new("dist/index.html"));
    Router::new().fallback_service(spa).layer(no_cache)
}

fn app() -> Router {
    assets_router().merge(shell_router())
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3018").await.unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app()).await.unwrap();
}
```

`/assets/*` carries the year-long immutable header; the shell carries `no-cache`. Because each layer wraps only its own sub-router, the two policies never collide. The `/assets` routes are declared first, so they win over the SPA fallback.

</details>

### Exercise 3: A unified app — API JSON 404 plus SPA shell fallback

**Difficulty:** Advanced

**Objective:** Build the routing tree that prevents the SPA fallback from swallowing API 404s (pitfall #4).

**Instructions:** Create one app where (1) `GET /api/ping` returns `{"pong":true}`, (2) any other `/api/*` path returns a **JSON** 404, and (3) every non-API path returns the SPA shell `dist/index.html` with a 200. Verify that `/api/oops` returns JSON `404` while `/some/spa/route` returns HTML `200`. Requires `cargo add axum tokio --features full; cargo add tower-http --features fs; cargo add serde_json`.

<details>
<summary>Solution</summary>

```rust
use axum::{http::StatusCode, routing::get, Json, Router};
use serde_json::{json, Value};
use tower_http::services::{ServeDir, ServeFile};

async fn ping() -> Json<Value> {
    Json(json!({ "pong": true }))
}

async fn api_fallback() -> (StatusCode, Json<Value>) {
    (StatusCode::NOT_FOUND, Json(json!({ "error": "no such endpoint" })))
}

fn app() -> Router {
    // /api/* lives in its own nested router with a JSON 404, so a mistyped
    // endpoint never falls through to the HTML shell.
    let api = Router::new()
        .route("/ping", get(ping))
        .fallback(api_fallback);

    // The SPA shell catches everything that is not an /api route.
    let spa = ServeDir::new("dist").fallback(ServeFile::new("dist/index.html"));

    Router::new()
        .nest("/api", api)
        .fallback_service(spa)
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3008").await.unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app()).await.unwrap();
}
```

Verified behavior: the API path returns JSON 404, the SPA route returns the HTML shell at **200**:

```text
$ curl -s -i http://127.0.0.1:3008/api/ping
HTTP/1.1 200 OK
content-type: application/json
{"pong":true}

$ curl -s -i http://127.0.0.1:3008/api/missing
HTTP/1.1 404 Not Found
content-type: application/json
{"error":"no such endpoint"}

$ curl -s -i http://127.0.0.1:3008/settings/profile
HTTP/1.1 200 OK
content-type: text/html
<!doctype html><title>SPA</title><div id="root">SPA shell</div>...
```

> **Note:** The `nest("/api", …)` boundary is what keeps API 404s as JSON — without it, a mistyped endpoint would fall through to the SPA shell. The shell uses `ServeDir::fallback(...)`, so deep links return **200**; switch it to `.not_found_service(...)` only if you specifically want a 404 status on the shell.

</details>
