---
title: "Middleware and Layers"
description: "Express app.use becomes Axum's Tower layers and middleware::from_fn: cross-cutting logging, CORS, compression, and auth, plus the inside-out layer ordering in Rust."
---

In Express.js, middleware is the workhorse for cross-cutting concerns: logging, authentication, CORS, compression, error handling. Axum has the same idea, but it borrows its plumbing from **Tower**, a general-purpose library for composing services. This chapter maps Express's `app.use(fn)` mental model onto Axum's `.layer(...)` and `middleware::from_fn(...)`, and shows the production-ready building blocks from the `tower-http` crate.

---

## Quick Overview

Middleware lets you run code **before and after** a request reaches your handler, without duplicating logic in every handler. Express middleware is a flat list of `(req, res, next)` functions; Axum uses **layers** (Tower's reusable, composable middleware) plus a `from_fn` escape hatch for one-off async functions. The big payoff is `tower-http`, a battle-tested collection of ready-made layers (tracing, CORS, compression, timeouts, body limits) that you bolt onto a `Router` with one line each.

---

## TypeScript/JavaScript Example

Here is a typical Express app wired with the usual cross-cutting middleware: a logger, CORS, gzip compression, and a route-specific auth guard.

```typescript
// app.ts — Express middleware stack
import express, { Request, Response, NextFunction } from "express";
import cors from "cors";
import compression from "compression";
import morgan from "morgan";

const app = express();

// Global middleware — runs for EVERY request, top to bottom.
app.use(morgan("tiny")); // request logging
app.use(cors({ origin: "https://app.example.com" })); // CORS headers
app.use(compression()); // gzip responses
app.use(express.json()); // parse JSON bodies

// A custom middleware that times each request.
app.use((req: Request, res: Response, next: NextFunction) => {
  const start = Date.now();
  res.on("finish", () => {
    console.log(`${req.method} ${req.url} -> ${res.statusCode} (${Date.now() - start}ms)`);
  });
  next(); // hand control to the next middleware/handler
});

// Route-specific middleware: an auth guard for /admin only.
function requireApiKey(req: Request, res: Response, next: NextFunction) {
  if (req.headers["x-api-key"] === "secret-token") {
    next();
  } else {
    res.status(401).end(); // short-circuit: never reaches the handler
  }
}

app.get("/", (_req, res) => res.send("Hello, world!"));
app.get("/admin", requireApiKey, (_req, res) => res.send("Welcome, admin"));

app.listen(3000);
```

**Key points:**

- `app.use(fn)` registers global middleware; order matters (top runs first).
- Each middleware receives `next` and must call it (or end the response).
- Calling `res.status(401).end()` **without** `next()` short-circuits the chain.
- Route-specific middleware is passed as an argument before the handler.

---

## Rust Equivalent

Axum expresses the same stack with `tower-http` layers and a couple of `from_fn` functions. Add the dependencies first:

```bash
cargo new my-api
cd my-api
cargo add axum@0.8
cargo add tokio@1 --features full
cargo add tower-http --features "trace,cors,compression-full,timeout,limit"
cargo add tracing tracing-subscriber --features tracing-subscriber/env-filter
```

```rust
// src/main.rs — equivalent Axum middleware stack
use axum::{
    Router,
    routing::get,
    middleware::{self, Next},
    extract::Request,
    response::Response,
    http::StatusCode,
};
use tower_http::{
    trace::TraceLayer,
    cors::CorsLayer,
    compression::CompressionLayer,
};
use std::time::Instant;

async fn root() -> &'static str {
    "Hello, world!"
}

async fn admin() -> &'static str {
    "Welcome, admin"
}

// A custom middleware that times each request (like the morgan-style logger).
// `Next` MUST be the last parameter.
async fn time_requests(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let start = Instant::now();

    let response = next.run(req).await; // hand control to the inner layers + handler

    println!("{} {} -> {} ({:?})", method, uri, response.status(), start.elapsed());
    response
}

// Route-specific auth guard. Returning Err short-circuits — the handler never runs.
async fn require_api_key(req: Request, next: Next) -> Result<Response, StatusCode> {
    let key = req.headers().get("x-api-key").and_then(|v| v.to_str().ok());
    match key {
        Some("secret-token") => Ok(next.run(req).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let app = Router::new()
        .route("/", get(root))
        // Route-specific middleware: only `/admin` is guarded.
        .route("/admin", get(admin).route_layer(middleware::from_fn(require_api_key)))
        // Global middleware, applied to every route on this router.
        .layer(middleware::from_fn(time_requests))
        .layer(CompressionLayer::new())
        .layer(CorsLayer::new().allow_origin(
            "https://app.example.com".parse::<axum::http::HeaderValue>().unwrap(),
        ))
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

**Key points:**

- `.layer(...)` adds middleware to the whole router; `.route_layer(...)` scopes it to specific routes.
- `tower-http` provides production-ready layers; you rarely hand-roll logging, CORS, or compression.
- `middleware::from_fn` wraps a plain async function into a layer.
- `next.run(req).await` is the equivalent of Express's `next()`; returning `Err(...)` short-circuits.

---

## Detailed Explanation

### `next.run(req).await` is Express's `next()`

In Express, you call `next()` to pass control along the chain and (usually) ignore its return value; the response is mutated through the shared `res` object. In Axum, the request flows *in* and a `Response` flows *back out*. `next.run(req)` consumes the request, runs everything "below" this middleware (deeper layers plus the handler), and **returns** the `Response`. Because it is an async function, you must `.await` it.

This return-based model is why an Axum middleware can inspect or modify the response *after* the handler ran:

```rust
use axum::{extract::Request, middleware::Next, response::Response, http::HeaderValue};

// Add a response header after the handler produces the response.
async fn add_version_header(req: Request, next: Next) -> Response {
    let mut response = next.run(req).await;
    response.headers_mut().insert(
        "x-app-version",
        HeaderValue::from_static("1.0.0"),
    );
    response
}
```

In Express you would do this with `res.on("finish", ...)` or by wrapping `res.send`; in Axum it is just "mutate the value before you return it."

### `Next` must come last

`from_fn` lets the function take any number of Axum **extractors** (covered in [Extractors](/16-web-apis/04-extractors/)) before the final two special parameters: the `Request` and the `Next`. The `Next` value represents "the rest of the stack," so it has to be the last argument. Getting this order wrong is a compile error (see [Common Pitfalls](#common-pitfalls)).

### `.layer()` vs `.route_layer()`

- `.layer(L)` wraps **every** route currently on the router (and is also inherited by anything you merge/nest under it). Use it for global concerns: tracing, CORS, compression.
- `.route_layer(L)` wraps only the routes defined so far and, importantly, does **not** run for requests that hit the router's 404 fallback. Use it for per-route guards like authentication.

### Layer ordering: outside-in for requests, inside-out for responses

This is the single most surprising part for Express developers. When you chain calls like:

```rust
use axum::{Router, routing::get, middleware, extract::Request, response::Response, middleware::Next};
use tower_http::trace::TraceLayer;

async fn root() -> &'static str { "ok" }
async fn outer(req: Request, next: Next) -> Response { next.run(req).await }
async fn inner(req: Request, next: Next) -> Response { next.run(req).await }

fn router() -> Router {
    Router::new()
        .route("/", get(root))
        .layer(middleware::from_fn(inner)) // added first  -> closer to the handler
        .layer(middleware::from_fn(outer)) // added last   -> closer to the network
        .layer(TraceLayer::new_for_http())
}
```

The layer added **last** is the **outermost** — it sees the request first and the response last. A request flows `TraceLayer -> outer -> inner -> handler`, and the response unwinds back `handler -> inner -> outer -> TraceLayer`. This is the opposite of Express's `app.use` order, where the *first* registered middleware runs *first*. (If you prefer Express's top-to-bottom reading order, use `ServiceBuilder`; see [Best Practices](#best-practices).)

We can prove the ordering with a real run. With both a logger (outermost) and an auth guard (inner) on the same route, hitting it without a valid key returns `401`, yet the outer logger still logs it, because the response unwinds back out through it:

```rust
// src/main.rs
use axum::{
    Router, routing::get, middleware::{self, Next},
    extract::Request, response::Response, http::StatusCode,
};
use std::time::Instant;

async fn root() -> &'static str { "Hello, world!" }

async fn log_requests(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let uri = req.uri().clone();
    let start = Instant::now();
    let response = next.run(req).await;
    println!("{} {} -> {} ({:?})", method, uri, response.status(), start.elapsed());
    response
}

async fn require_api_key(req: Request, next: Next) -> Result<Response, StatusCode> {
    let key = req.headers().get("x-api-key").and_then(|v| v.to_str().ok());
    match key {
        Some("secret-token") => Ok(next.run(req).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(root))
        .layer(middleware::from_fn(require_api_key)) // inner: added first
        .layer(middleware::from_fn(log_requests));   // outer: added last
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

Hitting it twice (once with no key, once with the right key):

```bash
curl -i http://127.0.0.1:3000/                              # no key
curl -i -H "x-api-key: secret-token" http://127.0.0.1:3000/ # valid key
```

The first request really returns `401`, the second `200`:

```
HTTP/1.1 401 Unauthorized
content-length: 0

HTTP/1.1 200 OK
content-type: text/plain; charset=utf-8
content-length: 13
```

And the server's standard output shows the **outer** logger ran for both, including the rejected request, with its real measured latency:

```
GET / -> 401 Unauthorized (25.083µs)
GET / -> 200 OK (13µs)
```

That is the inside-out unwinding in action: `require_api_key` produced the `401`, and the response traveled back out through `log_requests`.

---

## Key Differences

| Concept                  | Express.js                              | Axum / Tower                                              |
| ------------------------ | --------------------------------------- | --------------------------------------------------------- |
| Register middleware      | `app.use(fn)`                           | `.layer(L)` or `.route_layer(L)`                          |
| Pass control onward      | `next()`                                | `next.run(req).await`                                     |
| Short-circuit            | end `res` without `next()`              | return early (e.g. `Err(StatusCode::...)`)                |
| Modify the response      | mutate `res` / `res.on("finish")`       | mutate the returned `Response` value                      |
| Order of execution       | first registered runs first             | **last** `.layer` is outermost (or use `ServiceBuilder`)  |
| Reusable building blocks | npm packages (`cors`, `compression`)    | `tower-http` layers (`CorsLayer`, `CompressionLayer`)     |
| Per-route scope          | `app.get(path, mw, handler)`            | `.route_layer(...)` on a `MethodRouter`                   |
| Underlying abstraction   | a function signature convention         | the `tower::Service` / `tower::Layer` traits              |

The deepest difference is conceptual: Express middleware is a *convention* (a function shaped a certain way), while a Tower **layer** is a real type implementing the `Layer` and `Service` traits. That makes Axum middleware composable across frameworks: any Tower layer (from `tower`, `tower-http`, or your own crate) works unchanged, because Axum's `Router` *is* a `tower::Service`.

> **Note:** You almost never implement `tower::Service` by hand. `middleware::from_fn` covers the "I just need a quick async wrapper" case, and `tower-http` covers the common production needs. Hand-written `Service` impls are reserved for advanced, highly reusable middleware.

### The most useful `tower-http` layers

```rust
use tower_http::{
    trace::TraceLayer,                 // structured request/response logging
    cors::CorsLayer,                   // CORS headers (see cors.md)
    compression::CompressionLayer,     // gzip/brotli/zstd response compression
    timeout::TimeoutLayer,             // abort slow requests
    limit::RequestBodyLimitLayer,      // reject oversized bodies
};
use axum::http::StatusCode;
use std::time::Duration;

// Examples of constructing each (all compile-verified against tower-http 0.6):
let _trace = TraceLayer::new_for_http();
let _compress = CompressionLayer::new();
let _timeout = TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT, Duration::from_secs(30));
let _body_limit = RequestBodyLimitLayer::new(2 * 1024 * 1024); // 2 MiB
let _cors = CorsLayer::permissive();
```

> **Tip:** `tower-http` features are opt-in. Enable only what you use, e.g. `cargo add tower-http --features "trace,cors,compression-full,timeout,limit"`. The `compression-full` feature enables gzip, brotli, deflate, and zstd; pick `compression-gzip` if you only need gzip.

### `TraceLayer`: real output

`TraceLayer::new_for_http()` emits structured `tracing` spans and events for every request. Wire up a subscriber so you can see them:

```rust
// src/main.rs
use axum::{Router, routing::get};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

async fn root() -> &'static str { "Hello, world!" }

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "my_api=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let app = Router::new()
        .route("/", get(root))
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

Two `curl http://127.0.0.1:3000/` requests produce this real output (run with `RUST_LOG=tower_http=debug`):

```
2026-06-01T11:45:31.785175Z DEBUG request{method=GET uri=/ version=HTTP/1.1}: tower_http::trace::on_request: started processing request
2026-06-01T11:45:31.785454Z DEBUG request{method=GET uri=/ version=HTTP/1.1}: tower_http::trace::on_response: finished processing request latency=0 ms status=200
2026-06-01T11:45:31.802161Z DEBUG request{method=GET uri=/ version=HTTP/1.1}: tower_http::trace::on_request: started processing request
2026-06-01T11:45:31.802222Z DEBUG request{method=GET uri=/ version=HTTP/1.1}: tower_http::trace::on_response: finished processing request latency=0 ms status=200
```

Notice the `request{...}` **span** that wraps each request: every log inside a handler automatically inherits the method and URI fields, so your application logs are correlated to the request that produced them. That structured correlation is something Express's string-based loggers do not give you for free.

---

## Common Pitfalls

### Pitfall 1: Forgetting `.await` on `next.run(req)`

Because `next.run` is async, forgetting the `.await` hands back a `Future` where a `Response` is expected.

```rust
use axum::{extract::Request, middleware::Next, response::Response};

async fn broken(req: Request, next: Next) -> Response {
    next.run(req) // does not compile (error[E0308]): forgot `.await`, returns a Future
}
```

The real compiler error is clear and even suggests the fix:

```
error[E0308]: mismatched types
  --> src/main.rs:14:5
   |
14 |     next.run(req)  // forgot .await — returns a Future, not a Response
   |     ^^^^^^^^^^^^^ expected `Response<Body>`, found future
   |
note: calling an async function returns a future
help: consider `await`ing on the `Future`
   |
14 |     next.run(req).await
   |                  ++++++
```

### Pitfall 2: Putting `Next` before the `Request`

The `Next` argument must be **last**. Swap the order and the function no longer satisfies the trait bound `from_fn` requires, so `.layer(...)` rejects it.

```rust
use axum::{extract::Request, middleware::Next, response::Response};

async fn wrong_order(next: Next, req: Request) -> Response { // does not compile (error[E0277])
    next.run(req).await
}
```

The real error points at the failed `Service` bound (trimmed for length):

```
error[E0277]: the trait bound `FromFn<fn(Next, Request<Body>) -> ... {wrong_order}, ...>: Service<...>` is not satisfied
   --> src/main.rs:19:16
    |
 19 |         .layer(middleware::from_fn(wrong_order));
    |          ----- ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ unsatisfied trait bound
    |
    = help: the trait `tower_service::Service<...Request...>` is not implemented for `FromFn<fn(Next, Request<Body>) -> ...>`
note: required by a bound in `Router::<S>::layer`
```

It is not the friendliest message — when you see "the trait bound `FromFn<...>: Service<...>` is not satisfied," the first thing to check is that your `from_fn` parameters end with `Request` then `Next` (in that order).

### Pitfall 3: Expecting Express's top-to-bottom layer order

Reading a chain of `.layer(a).layer(b).layer(c)` top-to-bottom and assuming `a` runs first is the classic mistake. The **last** layer added is the outermost and runs first. If you want CORS to wrap (and thus run before) your auth middleware, add CORS *after* auth in the chain, or switch to `ServiceBuilder`, where the reading order matches execution order.

### Pitfall 4: `.layer()` not applying to your 404 fallback (or applying when you didn't want it to)

`.layer()` applies to everything currently on the router, including the fallback handler. `.route_layer()` does not run for unmatched routes. Putting an auth guard with `.layer()` instead of `.route_layer()` means unauthenticated requests to nonexistent paths get a `401` instead of a `404`, leaking the difference between "exists but forbidden" and "doesn't exist." For per-route guards, prefer `.route_layer()`.

### Pitfall 5: Middleware order causing wasted work

Compression should sit **outside** (run after) most other layers so it compresses the final response, but **inside** tracing if you want the trace to record the compressed size. Body-limit and timeout layers should be near the outside so they reject bad requests early, before you spend effort parsing JSON or hitting the database. Order is correctness *and* performance.

### Pitfall 6: Stacking `RequestBodyLimitLayer` with `CompressionLayer` inside a `ServiceBuilder`

Each `tower-http` layer wraps the request and/or response body in its own type. When you compose `RequestBodyLimitLayer` and `CompressionLayer` directly inside a single `ServiceBuilder`, the nested body type (`ResponseBody<CompressionBody<Body>>`) does not satisfy a trait bound the inner stack needs, and you get a confusing `Default`-related error:

```
error[E0277]: the trait bound `tower_http::limit::ResponseBody<CompressionBody<Body>>: std::default::Default` is not satisfied
  --> src/main.rs:14:10
   |
14 |         .layer(
   |          ^^^^^ the trait `std::default::Default` is not implemented for `tower_http::limit::ResponseBody<CompressionBody<Body>>`
```

The fix is to apply the body limit as its **own** chained `.layer(...)` on the `Router`, leaving the rest in the `ServiceBuilder`. Axum normalizes the body type between separate `.layer` calls, which keeps the bounds satisfied:

```rust
use axum::{Router, routing::get, http::StatusCode};
use tower::ServiceBuilder;
use tower_http::{
    trace::TraceLayer, timeout::TimeoutLayer,
    limit::RequestBodyLimitLayer, compression::CompressionLayer,
};
use std::time::Duration;

async fn root() -> &'static str { "ok" }

fn build_router() -> Router {
    Router::new()
        .route("/", get(root))
        // Body limit on its own layer — NOT inside the ServiceBuilder below.
        .layer(RequestBodyLimitLayer::new(512 * 1024))
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())
                .layer(TimeoutLayer::with_status_code(
                    StatusCode::REQUEST_TIMEOUT,
                    Duration::from_secs(10),
                ))
                .layer(CompressionLayer::new()),
        )
}

#[tokio::main]
async fn main() {
    let app = build_router();
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

---

## Best Practices

### Use `ServiceBuilder` for readable ordering

When stacking several layers, `tower::ServiceBuilder` lets you list them in the order they execute (outermost first, the natural top-to-bottom reading order), which is easier to reason about than the "last `.layer` wins" rule. Add the helper feature:

```bash
cargo add tower --features util
```

```rust
use axum::{Router, routing::get};
use tower::ServiceBuilder;
use tower_http::{trace::TraceLayer, cors::CorsLayer, compression::CompressionLayer};

async fn root() -> &'static str { "ok" }

fn build_router() -> Router {
    Router::new()
        .route("/", get(root))
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())  // outermost: runs first
                .layer(CorsLayer::permissive())
                .layer(CompressionLayer::new()),     // innermost: runs last
        )
}

#[tokio::main]
async fn main() {
    let app = build_router();
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

> **Tip:** With `ServiceBuilder`, layers execute **top to bottom** for the request, exactly like reading Express's `app.use` calls. This is the recommended way to assemble a multi-layer stack.

### Reach for `tower-http` before hand-rolling

Do not write your own request logger, CORS handler, or compression middleware. `tower-http` versions are correct, tested, and integrate with `tracing`. Save `from_fn` for genuinely app-specific logic (custom auth, request enrichment, feature flags).

### Pass state into middleware with `from_fn_with_state`

When your middleware needs shared state (a database pool, a config value, a key), use `from_fn_with_state`. The state is extracted just like in a handler via `State<T>` (covered in [Shared Application State in Axum](/16-web-apis/06-state-management/)):

```rust
use axum::{
    Router, routing::get, middleware::{self, Next},
    extract::{Request, State}, response::Response, http::StatusCode,
};
use std::sync::Arc;

#[derive(Clone)]
struct AppState {
    api_key: Arc<String>,
}

async fn root() -> &'static str { "ok" }

// State-aware middleware: `State` extractor first, then `Request`, then `Next`.
async fn require_api_key(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let provided = req.headers().get("x-api-key").and_then(|v| v.to_str().ok());
    if provided == Some(state.api_key.as_str()) {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(root))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_api_key))
        .with_state(state)
}

#[tokio::main]
async fn main() {
    let state = AppState { api_key: Arc::new("secret".to_string()) };
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

### Prefer returning a `Result` for short-circuiting

Returning `Result<Response, StatusCode>` (or, better, a custom error type that implements `IntoResponse`) from a `from_fn` middleware is the idiomatic way to bail out early. It is type-checked and composes with the error-handling patterns in [Error Handling in Web Handlers](/16-web-apis/10-error-handling-web/). For real authentication, build an extractor-as-guard or an `AuthUser` extractor instead of inline header checks; see [Authentication Patterns](/16-web-apis/12-authentication/) and [JWT Authentication](/16-web-apis/13-jwt/).

### Set sensible defaults for production

A production router should almost always include: `TraceLayer` (observability), a `TimeoutLayer` (don't let slow clients tie up tasks), a `RequestBodyLimitLayer` (reject oversized uploads early), `CompressionLayer`, and a deliberately configured `CorsLayer` (not the permissive default in production; see [CORS with Axum and tower-http](/16-web-apis/11-cors/)).

---

## Real-World Example

A production-flavored API router that combines `tower-http` layers, a custom request-ID middleware, and a state-aware auth guard scoped to protected routes. This compiles against axum 0.8 and tower-http 0.6.

```rust
// src/main.rs
use axum::{
    Router,
    routing::{get, post},
    middleware::{self, Next},
    extract::{Request, State},
    response::Response,
    http::{StatusCode, HeaderValue, Method, header},
    Json,
};
use tower_http::{
    trace::TraceLayer,
    cors::CorsLayer,
    compression::CompressionLayer,
    timeout::TimeoutLayer,
    limit::RequestBodyLimitLayer,
};
use serde_json::json;
use std::{sync::Arc, time::Duration};
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    api_key: Arc<String>,
}

// Public health check — no auth.
async fn health() -> Json<serde_json::Value> {
    Json(json!({ "status": "ok" }))
}

// Protected handler.
async fn create_widget() -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::CREATED, Json(json!({ "id": 1, "name": "widget" })))
}

// Custom middleware: attach a request ID header to every response.
async fn request_id(req: Request, next: Next) -> Response {
    let id = Uuid::new_v4().to_string();
    let mut response = next.run(req).await;
    if let Ok(value) = HeaderValue::from_str(&id) {
        response.headers_mut().insert("x-request-id", value);
    }
    response
}

// State-aware auth guard for the protected routes.
async fn require_api_key(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let provided = req.headers().get("x-api-key").and_then(|v| v.to_str().ok());
    if provided == Some(state.api_key.as_str()) {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

fn build_router(state: AppState) -> Router {
    // CORS locked to a known frontend origin.
    let cors = CorsLayer::new()
        .allow_origin("https://app.example.com".parse::<HeaderValue>().unwrap())
        .allow_methods([Method::GET, Method::POST])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION])
        .allow_credentials(true);

    // Protected routes carry their own auth guard via `route_layer`.
    let protected = Router::new()
        .route("/widgets", post(create_widget))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_api_key));

    Router::new()
        .route("/health", get(health))
        .merge(protected)
        // Global stack (last added = outermost).
        .layer(middleware::from_fn(request_id))
        .layer(CompressionLayer::new())
        .layer(cors)
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(15),
        ))
        .layer(RequestBodyLimitLayer::new(1024 * 1024)) // 1 MiB max body
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let state = AppState { api_key: Arc::new("secret-token".to_string()) };
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("listening on http://0.0.0.0:3000");
    axum::serve(listener, app).await.unwrap();
}
```

This example needs two extra crates beyond the earlier list:

```bash
cargo add serde_json
cargo add uuid --features v4
```

`/health` is open; `POST /widgets` requires `x-api-key: secret-token`. Every response (success or `401`) carries an `x-request-id` header because `request_id` is global and the response unwinds back out through it. Oversized bodies are rejected by `RequestBodyLimitLayer` before they reach a handler, and slow requests are cut off by the timeout — defenses you get for free from `tower-http`.

---

## Further Reading

### Official Documentation

- [axum `middleware` module](https://docs.rs/axum/0.8/axum/middleware/index.html) — `from_fn`, `from_fn_with_state`, and the layering rules
- [`tower-http` crate docs](https://docs.rs/tower-http/0.6/tower_http/) — every ready-made layer with examples
- [`tower::ServiceBuilder`](https://docs.rs/tower/latest/tower/struct.ServiceBuilder.html) — ordered layer composition
- [Tower guide: "Inventing the `Service` trait"](https://tokio.rs/blog/2021-05-14-inventing-the-service-trait) — the abstraction under Axum middleware

### Related Topics

- [Axum Basics](/16-web-apis/01-axum-basics/) — Router and handler fundamentals
- [Extractors](/16-web-apis/04-extractors/) — how middleware parameters before `Next` are resolved
- [State Management](/16-web-apis/06-state-management/) — sharing a DB pool/config with `from_fn_with_state`
- [Request and Response](/16-web-apis/07-request-response/) — `IntoResponse`, status codes, headers
- [Error Handling](/16-web-apis/10-error-handling-web/) — short-circuiting with a custom error type
- [CORS](/16-web-apis/11-cors/) — configuring `CorsLayer` for production
- [Authentication](/16-web-apis/12-authentication/) and [JWT](/16-web-apis/13-jwt/) — turning auth middleware into proper guards
- [Async/Await](/11-async/) — why `next.run(req).await` needs the `.await`
- [Functions](/03-functions/) and [Ownership](/05-ownership/) — the borrowing rules behind `req.headers()`
- Next section: [Databases](/17-database/) — the DB pool you will inject through state and middleware

---

## Exercises

### Exercise 1: A response-timing header

**Difficulty:** Easy

**Objective:** Write a `from_fn` middleware that measures how long a request took and adds the duration (in milliseconds) as an `x-response-time-ms` response header.

**Instructions:**

1. Capture an `Instant` before calling `next.run(req).await`.
2. After the response comes back, compute the elapsed time in milliseconds.
3. Insert it as the `x-response-time-ms` header on the response, then return the response.
4. Attach it to a one-route router with `.layer(...)`.

```rust
use axum::{
    Router, routing::get, middleware::{self, Next},
    extract::Request, response::Response, http::HeaderValue,
};
use std::time::Instant;

async fn root() -> &'static str { "ok" }

async fn timing_header(req: Request, next: Next) -> Response {
    let start = Instant::now();
    let mut response = next.run(req).await;
    // TODO: compute elapsed ms and insert the `x-response-time-ms` header
    response
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(root))
        .layer(middleware::from_fn(timing_header));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

<details>
<summary>Solution</summary>

```rust
use axum::{
    Router, routing::get, middleware::{self, Next},
    extract::Request, response::Response, http::HeaderValue,
};
use std::time::Instant;

async fn root() -> &'static str { "ok" }

async fn timing_header(req: Request, next: Next) -> Response {
    let start = Instant::now();
    let mut response = next.run(req).await;

    let ms = start.elapsed().as_millis();
    if let Ok(value) = HeaderValue::from_str(&ms.to_string()) {
        response.headers_mut().insert("x-response-time-ms", value);
    }
    response
}

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(root))
        .layer(middleware::from_fn(timing_header));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

A request now returns a header like `x-response-time-ms: 0`. The header is added *after* `next.run` returns; that "after the handler" hook is exactly what the inside-out unwinding enables.

</details>

### Exercise 2: A role-based guard with state

**Difficulty:** Medium

**Objective:** Write a `from_fn_with_state` middleware that only allows requests whose `x-role` header matches a required role stored in application state; otherwise return `403 Forbidden`.

**Instructions:**

1. Define an `AppState { required_role: Arc<String> }` that derives `Clone`.
2. Write `require_role(State(state), req, next)` returning `Result<Response, StatusCode>`.
3. Read the `x-role` header; if it equals the required role, proceed, else return `StatusCode::FORBIDDEN`.
4. Wire it with `route_layer(middleware::from_fn_with_state(...))` and `with_state`.

<details>
<summary>Solution</summary>

```rust
use axum::{
    Router, routing::get, middleware::{self, Next},
    extract::{Request, State}, response::Response, http::StatusCode,
};
use std::sync::Arc;

#[derive(Clone)]
struct AppState {
    required_role: Arc<String>,
}

async fn admin_panel() -> &'static str { "admin panel" }

async fn require_role(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let role = req.headers().get("x-role").and_then(|v| v.to_str().ok());
    if role == Some(state.required_role.as_str()) {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/admin", get(admin_panel))
        .route_layer(middleware::from_fn_with_state(state.clone(), require_role))
        .with_state(state)
}

#[tokio::main]
async fn main() {
    let state = AppState { required_role: Arc::new("admin".to_string()) };
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

`curl -H "x-role: admin" http://127.0.0.1:3000/admin` succeeds; any other role gets `403`. Because the guard is on a `route_layer`, requests to unknown paths still fall through to a normal `404` rather than being rejected as forbidden.

</details>

### Exercise 3: Assemble a production stack with `ServiceBuilder`

**Difficulty:** Hard

**Objective:** Build a router whose layers execute in this order for incoming requests: tracing (outermost), then a 10-second timeout, then a custom `request_id` middleware, then compression (innermost), using `ServiceBuilder` so the code reads top-to-bottom in execution order.

**Instructions:**

1. Add `tower` with the `util` feature and `tower-http` with `trace,timeout,compression-full`.
2. Write a `request_id` `from_fn` middleware that adds an `x-request-id` response header.
3. Construct a `ServiceBuilder` listing the four layers in execution order.
4. Apply it to a router with a single `GET /` route via one `.layer(...)` call, return `Router`, and verify it compiles.

<details>
<summary>Solution</summary>

```rust
use axum::{
    Router, routing::get, middleware::{self, Next},
    extract::Request, response::Response, http::{StatusCode, HeaderValue},
};
use tower::ServiceBuilder;
use tower_http::{trace::TraceLayer, timeout::TimeoutLayer, compression::CompressionLayer};
use std::time::Duration;

async fn root() -> &'static str { "ok" }

async fn request_id(req: Request, next: Next) -> Response {
    let mut response = next.run(req).await;
    response.headers_mut().insert("x-request-id", HeaderValue::from_static("demo"));
    response
}

fn build_router() -> Router {
    Router::new()
        .route("/", get(root))
        .layer(
            ServiceBuilder::new()
                .layer(TraceLayer::new_for_http())          // outermost: runs first
                .layer(TimeoutLayer::with_status_code(
                    StatusCode::REQUEST_TIMEOUT,
                    Duration::from_secs(10),
                ))
                .layer(middleware::from_fn(request_id))
                .layer(CompressionLayer::new()),            // innermost: runs last
        )
}

#[tokio::main]
async fn main() {
    let app = build_router();
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

Required dependencies:

```bash
cargo add tower --features util
cargo add tower-http --features "trace,timeout,compression-full"
```

With `ServiceBuilder`, the request travels through the layers in the exact order written: tracing wraps everything, the timeout cuts off slow requests, `request_id` tags the response, and compression runs closest to the handler so it compresses the final response.

> **Tip:** If you also need a `RequestBodyLimitLayer`, add it as a separate chained `.layer(...)` rather than inside this `ServiceBuilder`; see the body-limit composition pitfall above for why.

</details>
