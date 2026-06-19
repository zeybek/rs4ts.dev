---
title: "CORS with Axum and tower-http"
description: "Swap Express's cors for tower-http's CorsLayer in Axum: lock down origins, methods, and credentials, and see why the browser enforces CORS, not you."
---

## Quick Overview

**CORS** (Cross-Origin Resource Sharing) is the browser security mechanism that decides whether JavaScript running on `https://app.example.com` is allowed to call your API on `https://api.example.com`. In Express you reach for the `cors` npm package; in Axum you add the `CorsLayer` from `tower-http`. This page shows how to go from the wide-open development default to a deliberately locked-down production configuration, and explains the one rule that trips up every developer the first time: **CORS is enforced by the browser, not by your server**. Your server's only job is to send the right headers.

> **Note:** This page uses **axum 0.8** and **tower-http 0.6**. The current stable toolchain is Rust 1.96.0 on the latest stable edition (2024); `cargo new` selects it automatically. Servers start with `axum::serve(listener, app)` and a `tokio::net::TcpListener`.

---

## TypeScript/JavaScript Example

In Express, CORS is a middleware you mount with `app.use`. The popular [`cors`](https://www.npmjs.com/package/cors) package handles both the preflight `OPTIONS` request and the actual request.

```typescript
// server.ts — Express 5 with the `cors` package
// npm install express cors
// npm install -D @types/express @types/cors
import express from "express";
import cors, { CorsOptions } from "cors";

const app = express();
app.use(express.json());

// 1) Wide open — fine for a quick local prototype, dangerous in production.
//    app.use(cors());

// 2) Locked down — what you actually ship.
const corsOptions: CorsOptions = {
  origin: ["https://app.example.com", "https://admin.example.com"],
  methods: ["GET", "POST", "PUT", "DELETE"],
  allowedHeaders: ["Content-Type", "Authorization"],
  credentials: true, // allow cookies / Authorization to be sent cross-origin
  maxAge: 86_400, // cache the preflight result for 24h
};
app.use(cors(corsOptions));

app.get("/tasks", (_req, res) => {
  res.json([{ id: 1, title: "Write the docs" }]);
});

app.listen(3000, () => {
  console.log("listening on http://127.0.0.1:3000");
});
```

Things a TypeScript developer relies on here: `cors()` with no arguments reflects any origin; passing an `origin` array restricts it; `credentials: true` is required before browsers will send cookies or `Authorization` cross-origin; and the package quietly answers preflight `OPTIONS` requests for you.

---

## Rust Equivalent

The same two configurations in Axum. First add the dependency with the `cors` feature:

```bash
cargo add tower-http --features cors
cargo add tower
```

`tower-http`'s features are opt-in, so you enable only `cors` (compression, tracing, etc. are separate features). Here is the locked-down version that mirrors the Express `corsOptions` above:

```rust
use axum::{
    http::{header, HeaderValue, Method},
    routing::get,
    Router,
};
use std::time::Duration;
use tower_http::cors::CorsLayer;

async fn list_tasks() -> &'static str {
    r#"[{"id":1,"title":"Write the docs"}]"#
}

fn app() -> Router {
    let cors = CorsLayer::new()
        // The browser may read responses for requests from this origin.
        .allow_origin("https://app.example.com".parse::<HeaderValue>().unwrap())
        // Which HTTP methods cross-origin requests may use.
        .allow_methods([Method::GET, Method::POST])
        // Which request headers the client is allowed to send.
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION])
        // Allow cookies / Authorization to be sent cross-origin.
        .allow_credentials(true)
        // Cache the preflight result in the browser for an hour.
        .max_age(Duration::from_secs(3600));

    Router::new()
        .route("/tasks", get(list_tasks))
        .layer(cors)
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    println!("listening on http://127.0.0.1:3000");
    axum::serve(listener, app()).await.unwrap();
}
```

And the wide-open development default — the one-liner equivalent of `app.use(cors())`:

```rust
use axum::{routing::get, Router};
use tower_http::cors::CorsLayer;

async fn list_tasks() -> &'static str {
    "[]"
}

fn dev_app() -> Router {
    Router::new()
        .route("/tasks", get(list_tasks))
        // Allows ANY origin, method, and header. Development only.
        .layer(CorsLayer::permissive())
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, dev_app()).await.unwrap();
}
```

> **Tip:** Build the `CorsLayer` in its own `fn` (e.g. `fn cors_layer() -> CorsLayer`) and attach it with `.layer(cors_layer())`. It keeps the router declaration readable and makes the CORS policy unit-testable in isolation.

---

## Detailed Explanation

### What CORS actually does

CORS is **not** authentication or a firewall. It does not stop anyone from calling your API: `curl`, Postman, a mobile app, or another server can hit your endpoint regardless of CORS. CORS only governs whether a **browser** will let *its own JavaScript* read the response of a cross-origin request. The enforcement happens entirely in the browser; your server merely advertises a policy via `Access-Control-*` response headers, and the browser decides whether to hand the response to your `fetch().then(...)` or to throw a `TypeError` and log a red CORS error in the console.

This is the single most important mental shift. A passing `curl` does **not** prove your CORS config is correct, and a failing browser request does **not** mean your server "rejected" anything. The response very likely arrived fine; the browser just refused to expose it.

### The preflight request

For "non-simple" requests (anything with a JSON `Content-Type`, an `Authorization` header, or a method like `PUT`/`DELETE`), the browser first sends an `OPTIONS` **preflight** request asking "am I allowed to do this?". `CorsLayer` answers that preflight automatically; you never write an `OPTIONS` handler. Here is the real preflight exchange against the locked-down server above (captured with `curl -X OPTIONS`):

```text
$ curl -i -X OPTIONS http://127.0.0.1:3000/tasks \
    -H "Origin: https://app.example.com" \
    -H "Access-Control-Request-Method: POST" \
    -H "Access-Control-Request-Headers: content-type"

HTTP/1.1 200 OK
access-control-allow-credentials: true
vary: origin, access-control-request-method, access-control-request-headers
access-control-allow-methods: GET,POST
access-control-allow-headers: content-type,authorization
access-control-max-age: 3600
access-control-allow-origin: https://app.example.com
allow: GET,HEAD
content-length: 0
```

(Plus a standard `date:` header, omitted here.) Every method (`allow_methods`), header (`allow_headers`), credential (`allow_credentials`), and `max-age` you configured shows up here. The `Vary` header tells caches the response depends on the request's origin and preflight headers.

### The actual request

If the preflight passes, the browser sends the real request. The server attaches a smaller set of headers:

```text
$ curl -i http://127.0.0.1:3000/tasks -H "Origin: https://app.example.com"

HTTP/1.1 200 OK
vary: origin, access-control-request-method, access-control-request-headers
access-control-allow-credentials: true
access-control-allow-origin: https://app.example.com
```

### The behavior that surprises everyone

Watch what happens when a request arrives from an origin you did **not** allow, against that same fixed-origin server:

```text
$ curl -i http://127.0.0.1:3000/tasks -H "Origin: https://evil.example.com"

HTTP/1.1 200 OK
vary: origin, access-control-request-method, access-control-request-headers
access-control-allow-credentials: true
access-control-allow-origin: https://app.example.com
```

The request **still returns `200 OK` with the body**, and the `access-control-allow-origin` header still says `https://app.example.com`. The server did not block the request. A real browser at `https://evil.example.com` would compare its own origin (`evil`) against the returned `allow-origin` (`app`), see they differ, and refuse to give the response to the page's JavaScript. That comparison is what "blocks" the request, and it lives in the browser, not your server. (See [Common Pitfalls](#common-pitfalls) for why testing CORS with `curl` is misleading.)

### Line-by-line

- `CorsLayer::new()` starts an empty, *deny-by-default* policy. With no further calls, no `Access-Control-Allow-*` headers are sent and browsers block all cross-origin reads.
- `.allow_origin("https://app.example.com".parse::<HeaderValue>().unwrap())` accepts a single `HeaderValue`. The string must include the scheme (`https://`) and must **not** have a trailing slash. `HeaderValue` parsing of an invalid origin returns an `Err`, which `.unwrap()` would turn into a panic.
- `.allow_methods([Method::GET, Method::POST])` takes anything convertible into a list of methods. Use the typed `Method::GET` constants rather than stringly-typed values.
- `.allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION])` lists the request headers clients may send. If a client sends a header that is not on this list, the preflight fails.
- `.allow_credentials(true)` sets `Access-Control-Allow-Credentials: true`, the prerequisite for cookies and `Authorization` to flow cross-origin. This option has an important constraint covered below.
- `.max_age(Duration::from_secs(3600))` tells the browser it may cache this preflight result and skip the `OPTIONS` round-trip for an hour.

---

## Key Differences

| Concept | Express (`cors` package) | Axum (`tower-http` `CorsLayer`) |
| --- | --- | --- |
| Install | `npm install cors` | `cargo add tower-http --features cors` |
| Wide open | `app.use(cors())` | `.layer(CorsLayer::permissive())` |
| Allow one origin | `cors({ origin: "https://x" })` | `.allow_origin("https://x".parse::<HeaderValue>()?)` |
| Allow a list | `cors({ origin: ["a", "b"] })` | `.allow_origin([a, b])` (typed `HeaderValue`s) |
| Dynamic check | `origin: (o, cb) => cb(null, ok)` | `.allow_origin(AllowOrigin::predicate(\|o, parts\| ...))` |
| Credentials | `credentials: true` | `.allow_credentials(true)` |
| Preflight cache | `maxAge: 86400` | `.max_age(Duration::from_secs(86_400))` |
| Bad config | silently misbehaves | typed; some invalid combos **panic** at request time |
| Where applied | `app.use(...)` (insertion order) | `.layer(...)` (outer-to-inner; last added runs first) |

The conceptual differences worth internalizing:

- **Typed, not stringly-typed.** Methods are `Method::GET`, header names are `header::CONTENT_TYPE`, origins are parsed into `HeaderValue`. A typo in `"Athorization"` is a value you must construct deliberately, not a silent string.
- **Wildcard origins are a distinct type.** Express overloads `origin: "*"` / `origin: true` / an array / a function on one option. tower-http splits these: `Any` (the wildcard `*`), a concrete `HeaderValue` (or a list of them, echoed back when matched), and `AllowOrigin::predicate(...)` for dynamic logic. The type tells you which behavior you get.
- **Some invalid policies are rejected, loudly.** Combining `allow_credentials(true)` with a wildcard origin is forbidden by the CORS spec. The `cors` npm package will happily send the contradictory headers and leave you to debug a browser error; tower-http **panics** when such a request arrives (see Pitfalls). Loud-and-early beats silent-and-wrong.
- **Layers, not insertion order.** `CorsLayer` is a Tower layer. It typically belongs near the outside of the stack so preflight responses are produced before auth or other middleware can reject them. See [Middleware and Layers](/16-web-apis/05-middleware/) for how `.layer()` ordering works (the last layer added is the outermost and runs first).

---

## Common Pitfalls

### Pitfall 1: "I tested it with `curl` and it worked"

`curl`, Postman, and server-to-server HTTP clients do not enforce CORS; they ignore the `Access-Control-*` headers entirely. As shown above, a disallowed origin still gets `200 OK` and a body from `curl`. CORS only matters to a browser running page JavaScript. **Always verify CORS in a real browser** (the DevTools Network tab and Console), or by carefully inspecting the response headers and reasoning about what a browser *would* do with them. A green `curl` is not a passing CORS test.

### Pitfall 2: combining `allow_credentials(true)` with a wildcard origin panics

The CORS spec forbids `Access-Control-Allow-Origin: *` together with `Access-Control-Allow-Credentials: true`: a credentialed response must name a specific origin. tower-http enforces this at runtime:

```rust
use tower_http::cors::{Any, CorsLayer};

// does not work: panics when the first request hits this layer.
let _cors = CorsLayer::new()
    .allow_origin(Any)          // wildcard *
    .allow_credentials(true);   // contradicts the wildcard
```

The real panic, captured by sending one request to a server built this way:

```text
thread 'main' panicked at tower-http-0.6.11/src/cors/mod.rs:797:9:
Invalid CORS configuration: Cannot combine `Access-Control-Allow-Credentials: true` with `Access-Control-Allow-Origin: *`
```

> **Warning:** Because this panics at *request* time, not at startup, a server with this misconfiguration boots fine and then crashes the first time a browser sends a request. Note that `CorsLayer::permissive()` does **not** set `allow_credentials`, so it does not hit this, but it also means cookies will not flow. To support credentials, name your origins explicitly (a fixed `HeaderValue`, a list, or a predicate).

### Pitfall 3: a trailing slash or missing scheme in the origin

The `Origin` header a browser sends never includes a path or trailing slash; it is exactly `https://app.example.com`. If you configure `"https://app.example.com/"` (trailing slash) or `"app.example.com"` (no scheme), it will never match the browser's `Origin`, and every request silently fails CORS in the browser while looking fine on the server. Match the origin byte-for-byte: scheme + host + optional port, no trailing slash.

### Pitfall 4: expecting a list of origins to send `*`

`.allow_origin([origin_a, origin_b])` does **not** emit `Access-Control-Allow-Origin: *`. tower-http inspects the request's `Origin`, and if it is in your list, echoes that single origin back; otherwise it sends no allow-origin header. This is correct (a response may only name one origin), but if you were expecting the literal header to contain both, you will not see it.

### Pitfall 5: putting `CorsLayer` inside an auth-guarded sub-router

If your authentication middleware runs *before* CORS and rejects the unauthenticated preflight `OPTIONS` request with a `401`, the browser never sees the CORS headers and reports a CORS failure (not an auth failure). Preflight requests carry no credentials by design. Apply `CorsLayer` so it wraps (runs before) auth — typically by layering it on the outer router. See [Authentication Patterns](/16-web-apis/12-authentication/) and [Middleware and Layers](/16-web-apis/05-middleware/).

---

## Best Practices

- **Never ship `CorsLayer::permissive()` to production.** It is the equivalent of `cors()` with no options: any website on the internet can drive your API from its users' browsers. Use it for local development only, ideally gated behind a debug build or an environment flag.
- **Allowlist explicit origins.** Name the exact front-end origins you trust. Do not reflect the request's `Origin` back unconditionally (the dynamic equivalent of `*` while still appearing "specific") — that defeats the purpose.
- **Only enable `allow_credentials(true)` if you actually use cookies or send `Authorization` cross-origin,** and remember it is incompatible with wildcard origins.
- **Restrict methods and headers to what you use.** `.allow_methods([Method::GET, Method::POST])` and an explicit `allow_headers` list are tighter than `Any`.
- **Set a `max_age`** to cut preflight traffic; browsers cap it (Chromium caps at 7200s), so a value like 24 hours is requested-but-clamped, which is fine.
- **Read allowed origins from configuration**, not hard-coded literals, so staging and production can differ without a recompile (shown in the Real-World Example).
- **Use `expose_headers` when the browser must read a custom response header** (e.g. a pagination header like `X-Total-Count`). By default browsers only expose a short safelist of response headers to JavaScript:

  ```rust
  use axum::http::{header, Method};
  use tower_http::cors::CorsLayer;

  let _cors = CorsLayer::new()
      .allow_origin("https://app.example.com".parse::<axum::http::HeaderValue>().unwrap())
      .allow_methods([Method::GET])
      // Let the browser's JS read this custom response header.
      .expose_headers([header::HeaderName::from_static("x-total-count")]);
  ```

---

## Real-World Example

A production-flavored setup: allowed origins come from the `CORS_ALLOWED_ORIGINS` environment variable (comma-separated), credentials are enabled, and the CORS policy is built in its own function so it can be tested and reused. The `CorsLayer` is applied as the outer layer of the router.

```rust
use axum::{
    extract::State,
    http::{header, HeaderValue, Method, StatusCode},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tower_http::cors::CorsLayer;

#[derive(Clone, Serialize)]
struct Task {
    id: u64,
    title: String,
}

#[derive(Deserialize)]
struct NewTask {
    title: String,
}

#[derive(Clone, Default)]
struct AppState {
    tasks: Arc<Mutex<Vec<Task>>>,
}

async fn list_tasks(State(state): State<AppState>) -> Json<Vec<Task>> {
    Json(state.tasks.lock().unwrap().clone())
}

async fn create_task(
    State(state): State<AppState>,
    Json(body): Json<NewTask>,
) -> (StatusCode, Json<Task>) {
    let mut tasks = state.tasks.lock().unwrap();
    let id = tasks.len() as u64 + 1;
    let task = Task { id, title: body.title };
    tasks.push(task.clone());
    (StatusCode::CREATED, Json(task))
}

/// Build the CORS policy from configuration.
/// `CORS_ALLOWED_ORIGINS=https://app.example.com,https://admin.example.com`
fn cors_from_env() -> CorsLayer {
    let raw = std::env::var("CORS_ALLOWED_ORIGINS")
        .unwrap_or_else(|_| "https://app.example.com".to_string());

    // Parse each comma-separated origin into a typed HeaderValue.
    let origins: Vec<HeaderValue> = raw
        .split(',')
        .filter_map(|s| s.trim().parse::<HeaderValue>().ok())
        .collect();

    CorsLayer::new()
        .allow_origin(origins) // echoes back whichever listed origin matches
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION])
        .allow_credentials(true)
        .max_age(Duration::from_secs(86_400))
}

fn app() -> Router {
    Router::new()
        .route("/tasks", get(list_tasks).post(create_task))
        // Outermost layer: preflights are answered before anything else.
        .layer(cors_from_env())
        .with_state(AppState::default())
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    println!("listening on http://127.0.0.1:3000");
    axum::serve(listener, app()).await.unwrap();
}
```

This compiles and runs against axum 0.8.9 / tower-http 0.6.11. Setting `CORS_ALLOWED_ORIGINS=https://app.example.com,https://admin.example.com` before launch makes both front ends usable; a single value (or the default) restricts it to one. Because the origins are typed `HeaderValue`s and credentials are enabled, this never hits the wildcard-plus-credentials panic from Pitfall 2.

---

## Further Reading

- [tower-http `cors` module docs](https://docs.rs/tower-http/latest/tower_http/cors/index.html): the authoritative `CorsLayer` reference.
- [`CorsLayer` API](https://docs.rs/tower-http/latest/tower_http/cors/struct.CorsLayer.html) and [`AllowOrigin`](https://docs.rs/tower-http/latest/tower_http/cors/struct.AllowOrigin.html) for dynamic/predicate origins.
- [MDN: Cross-Origin Resource Sharing (CORS)](https://developer.mozilla.org/en-US/docs/Web/HTTP/CORS): how browsers enforce the policy.
- [Middleware and Layers](/16-web-apis/05-middleware/) — Tower layers, layer ordering, and where `CorsLayer` fits in the stack.
- [Authentication Patterns](/16-web-apis/12-authentication/) — why CORS must wrap auth so preflight `OPTIONS` are not rejected.
- [Request and Response Handling](/16-web-apis/07-request-response/): setting headers and status codes on responses.
- [Axum Fundamentals](/16-web-apis/01-axum-basics/) and [Routing in Axum](/16-web-apis/03-routing/) — the `Router`/handler foundations these examples build on.
- [Deploying Axum Applications](/16-web-apis/19-deployment/) — wiring `CORS_ALLOWED_ORIGINS` and other config through environment variables in production.
- Foundations: [Section 00 — Introduction](/00-introduction/), [Section 01 — Getting Started](/01-getting-started/), [Section 02 — Basics](/02-basics/). Next up after web APIs: [Section 17 — Database](/17-database/).

---

## Exercises

### Exercise 1: From permissive to locked-down

**Difficulty:** Beginner

**Objective:** Replace a development `CorsLayer::permissive()` with an explicit single-origin policy.

**Instructions:** Start from a router that serves `GET /ping` returning `"pong"` behind `CorsLayer::permissive()`. Rewrite the layer so it allows only the origin `https://app.example.com`, only the `GET` method, and only the `Content-Type` request header. Do not enable credentials.

<details>
<summary>Solution</summary>

```rust
use axum::{
    http::{header, HeaderValue, Method},
    routing::get,
    Router,
};
use tower_http::cors::CorsLayer;

async fn ping() -> &'static str {
    "pong"
}

fn app() -> Router {
    let cors = CorsLayer::new()
        .allow_origin("https://app.example.com".parse::<HeaderValue>().unwrap())
        .allow_methods([Method::GET])
        .allow_headers([header::CONTENT_TYPE]);

    Router::new().route("/ping", get(ping)).layer(cors)
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app()).await.unwrap();
}
```

A preflight from `https://app.example.com` returns `access-control-allow-origin: https://app.example.com`, `access-control-allow-methods: GET`, and `access-control-allow-headers: content-type`. No `access-control-allow-credentials` header is sent because credentials were not enabled.

</details>

### Exercise 2: A dynamic origin allowlist that supports credentials

**Difficulty:** Intermediate

**Objective:** Allow several origins *and* credentials without hitting the wildcard-plus-credentials restriction, by checking the origin per request.

**Instructions:** Use `AllowOrigin::predicate` so that requests from `https://app.example.com` and `https://admin.example.com` are accepted (the matching origin reflected back), any other origin gets no allow-origin header, and `allow_credentials(true)` is enabled. Serve `GET /tasks`.

<details>
<summary>Solution</summary>

```rust
use axum::{
    http::{header, request::Parts, HeaderValue, Method},
    routing::get,
    Router,
};
use tower_http::cors::{AllowOrigin, CorsLayer};

async fn list_tasks() -> &'static str {
    "[]"
}

fn cors_layer() -> CorsLayer {
    let allowed: &[&str] = &["https://app.example.com", "https://admin.example.com"];

    CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(
            move |origin: &HeaderValue, _parts: &Parts| {
                origin
                    .to_str()
                    .map(|o| allowed.contains(&o))
                    .unwrap_or(false)
            },
        ))
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION])
        .allow_credentials(true)
}

fn app() -> Router {
    Router::new()
        .route("/tasks", get(list_tasks))
        .layer(cors_layer())
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app()).await.unwrap();
}
```

Verified behavior: a request with `Origin: https://app.example.com` returns `access-control-allow-origin: https://app.example.com`; `Origin: https://admin.example.com` reflects that origin; and `Origin: https://evil.test` returns `200 OK` with **no** `access-control-allow-origin` header — which is exactly what makes a real browser block the cross-origin read. Because each response names a single concrete origin (never `*`), enabling credentials is valid here.

</details>

### Exercise 3: Configurable CORS with a fallback and an exposed header

**Difficulty:** Advanced

**Objective:** Read allowed origins from an environment variable, fall back to a safe default, and expose a custom pagination header to the browser.

**Instructions:** Write `fn cors_from_env() -> CorsLayer` that reads `CORS_ALLOWED_ORIGINS` (comma-separated). If the variable is unset, default to `https://app.example.com`. Allow `GET` and `POST`, allow the `Content-Type` and `Authorization` request headers, enable credentials, set a 24-hour `max_age`, and expose the response header `X-Total-Count` so client JavaScript can read it. Attach it to a router serving `GET /tasks`.

<details>
<summary>Solution</summary>

```rust
use axum::{
    http::{header, HeaderValue, Method},
    routing::get,
    Router,
};
use std::time::Duration;
use tower_http::cors::CorsLayer;

async fn list_tasks() -> &'static str {
    "[]"
}

fn cors_from_env() -> CorsLayer {
    let raw = std::env::var("CORS_ALLOWED_ORIGINS")
        .unwrap_or_else(|_| "https://app.example.com".to_string());

    let origins: Vec<HeaderValue> = raw
        .split(',')
        .filter_map(|s| s.trim().parse::<HeaderValue>().ok())
        .collect();

    CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION])
        .allow_credentials(true)
        .max_age(Duration::from_secs(86_400))
        // Let client JS read this custom response header.
        .expose_headers([header::HeaderName::from_static("x-total-count")])
}

fn app() -> Router {
    Router::new()
        .route("/tasks", get(list_tasks))
        .layer(cors_from_env())
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app()).await.unwrap();
}
```

`filter_map(... .ok())` quietly drops any malformed origin entry rather than panicking, so one bad value in the environment variable cannot take the server down at startup. With `CORS_ALLOWED_ORIGINS` unset, the policy allows the single default origin; set it to a comma-separated list to allow several. The `expose_headers` call adds `access-control-expose-headers: x-total-count` to responses, which is required before a browser will let `fetch` read that header off the response.

</details>
