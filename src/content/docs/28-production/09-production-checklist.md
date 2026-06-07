---
title: "Production Readiness Checklist"
description: "Ship a Rust axum service safely: structured logging, typed errors that never leak, timeouts, body limits, and panic isolation, mapped from the Node equivalents."
---

The gap between "it compiles and the tests pass" and "I can page-proof this at 3 a.m." is filled by a handful of unglamorous concerns: structured logging, honest error handling, timeouts on everything that can hang, limits on everything that can grow unbounded, observability you can query, and a security posture that does not leak. This chapter is the checklist a senior TypeScript/JavaScript developer should run through before a Rust service takes traffic — and the idiomatic, current-stable way to satisfy each item.

---

## Quick Overview

Going to production is not one feature; it is a set of cross-cutting properties your service must hold *under load and under failure*. The current stable toolchain is Rust 1.96.0 on the 2024 edition; `cargo new` selects it automatically. The web examples here use [axum](/16-web-apis/) 0.8 with [tower-http](https://docs.rs/tower-http) middleware and the [tracing](https://docs.rs/tracing) ecosystem — the same building blocks the other chapters in this section use.

The six pillars this file covers:

- **Logging:** structured (JSON), level-controlled, with secrets redacted and a correlation ID per request.
- **Errors:** one typed error per surface, the right HTTP status, the cause logged but never leaked.
- **Timeouts:** a hard bound on every inbound request and every outbound call. An unbounded `await` is a latent outage.
- **Limits:** body size, concurrency, and connection caps so one client cannot exhaust the box.
- **Observability:** logs, metrics, traces, and health probes wired up *before* you need them.
- **Security:** least privilege, no secrets in logs or images, dependency auditing, and a minimal runtime.

> **Note:** The sibling files in this section go deep on individual pillars: [Metrics and Monitoring](/28-production/04-metrics/), [Distributed Tracing](/28-production/05-distributed-tracing/), [Health and Readiness Endpoints](/28-production/03-health-checks/), [Graceful Shutdown](/28-production/02-graceful-shutdown/), [Rate Limiting](/28-production/06-rate-limiting/), [Caching Strategies](/28-production/07-caching/), [Application Configuration](/28-production/00-configuration/), and [Environment-Based Configuration](/28-production/01-environment/). This file is the integrating checklist that ties them together.

---

## TypeScript/JavaScript Example

A production-minded Express service on Node v22 bolts these concerns on through middleware. It is the shape most TypeScript developers will recognize:

```typescript
// server.ts — production-hardened Express on Node v22
import express, { NextFunction, Request, Response } from "express";
import pino from "pino";
import pinoHttp from "pino-http";
import { randomUUID } from "node:crypto";

const log = pino({
  level: process.env.LOG_LEVEL ?? "info",
  // Redact secrets so tokens never reach the log sink.
  redact: ["req.headers.authorization", "req.headers.cookie"],
});

const app = express();

// Correlation ID + structured request logging.
app.use(pinoHttp({
  logger: log,
  genReqId: (req) => (req.headers["x-request-id"] as string) ?? randomUUID(),
}));

// Body-size limit: reject oversized payloads before parsing.
app.use(express.json({ limit: "1mb" }));

// Per-request timeout has to be wired by hand — Express has no built-in.
app.use((req: Request, res: Response, next: NextFunction) => {
  res.setTimeout(5000, () => res.status(503).json({ error: "timeout" }));
  next();
});

app.post("/users", (req: Request, res: Response) => {
  const name = String(req.body?.name ?? "").trim();
  if (!name) {
    return res.status(400).json({ error: "name must not be empty" });
  }
  res.json({ id: 1, name });
});

// Central error handler: log the real cause, send a safe message.
app.use((err: unknown, _req: Request, res: Response, _next: NextFunction) => {
  log.error({ err }, "request failed");
  res.status(500).json({ error: "internal error" }); // never leak `err`
});

// Outbound calls must be bounded too — fetch has no default timeout.
async function fetchUpstream(url: string): Promise<Response> {
  return fetch(url, { signal: AbortSignal.timeout(2000) });
}

app.listen(3000, () => log.info("listening on :3000"));
```

**Key points:**

- Logging, redaction, request IDs, body limits, and timeouts are all *opt-in middleware* you must remember to add.
- The error handler must manually avoid leaking `err` to the client — nothing in the type system stops you.
- `fetch` has **no default timeout**; you must pass `AbortSignal.timeout`. A forgotten one is the classic Node outage.

---

## Rust Equivalent

The same hardened service in axum. Each numbered layer corresponds to a checklist item; the typed error makes "log the cause, return a safe message" the path of least resistance.

Set up the project:

```bash
cargo new user-service
cd user-service
cargo add axum
cargo add tokio --features full
cargo add tower
cargo add tower-http --features timeout,trace,request-id,sensitive-headers
cargo add tracing
cargo add tracing-subscriber --features env-filter,json
cargo add serde --features derive
cargo add thiserror
cargo add anyhow
```

```rust
use std::time::Duration;

use axum::{
    extract::{DefaultBodyLimit, Json},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Router,
};
use serde::{Deserialize, Serialize};
use tower::ServiceBuilder;
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    sensitive_headers::SetSensitiveRequestHeadersLayer,
    timeout::TimeoutLayer,
    trace::TraceLayer,
};
use tracing::instrument;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

// One error type for the whole API surface. Each variant maps to a status code.
#[derive(Debug, thiserror::Error)]
enum ApiError {
    #[error("invalid request: {0}")]
    Validation(String),
    #[error("user {0} not found")]
    NotFound(u64),
    #[error("internal error")]
    Internal(#[from] anyhow::Error),
}

// The body we actually send to clients. The internal cause is logged,
// never leaked to the wire.
#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self {
            ApiError::Validation(_) => StatusCode::BAD_REQUEST,
            ApiError::NotFound(_) => StatusCode::NOT_FOUND,
            ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        // Log the full error server-side at the right level.
        if status.is_server_error() {
            tracing::error!(error = %self, "request failed");
        } else {
            tracing::warn!(error = %self, "request rejected");
        }
        // Clients get a safe message; a 5xx never reveals internals.
        let public = if status.is_server_error() {
            "internal error".to_string()
        } else {
            self.to_string()
        };
        (status, Json(ErrorBody { error: public })).into_response()
    }
}

#[derive(Deserialize)]
struct CreateUser {
    name: String,
}

#[derive(Serialize)]
struct User {
    id: u64,
    name: String,
}

#[instrument(skip(payload), fields(user.name = %payload.name))]
async fn create_user(Json(payload): Json<CreateUser>) -> Result<Json<User>, ApiError> {
    if payload.name.trim().is_empty() {
        return Err(ApiError::Validation("name must not be empty".into()));
    }
    tracing::info!("user created");
    Ok(Json(User { id: 1, name: payload.name }))
}

fn app() -> Router {
    // Headers we never want to appear in logs.
    let sensitive = [header::AUTHORIZATION, header::COOKIE];

    Router::new()
        .route("/users", post(create_user))
        // Reject oversized bodies before allocating (1 MiB cap).
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .layer(
            ServiceBuilder::new()
                // 1. Give every request a stable ID for correlating logs.
                .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
                // 2. Redact secrets BEFORE the trace layer reads headers.
                .layer(SetSensitiveRequestHeadersLayer::new(sensitive))
                // 3. Structured per-request spans/events.
                .layer(TraceLayer::new_for_http())
                // 4. Hard request timeout: a slow handler returns 408, never hangs.
                .layer(TimeoutLayer::with_status_code(
                    StatusCode::REQUEST_TIMEOUT,
                    Duration::from_secs(5),
                ))
                // 5. Echo the request ID back to the caller.
                .layer(PropagateRequestIdLayer::x_request_id()),
        )
}

#[tokio::main]
async fn main() {
    // JSON logs, level from RUST_LOG (defaults to info). Machine-parseable in prod.
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer().json())
        .init();

    let app = app();

    // In a real binary you would bind a listener and serve:
    //   let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    //   axum::serve(listener, app).await.unwrap();
    // Here we exercise the pipeline end-to-end without opening a port.
    use tower::ServiceExt;
    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/users")
        .header("content-type", "application/json")
        .header("authorization", "Bearer super-secret-token")
        .body(axum::body::Body::from(r#"{"name":"Ada"}"#))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    println!("status = {status}");
    println!("body   = {}", String::from_utf8_lossy(&bytes));
}
```

Running it with `RUST_LOG=info,tower_http=debug cargo run` produces real structured output (the trace layer emits request/response spans; your handler's `info!` nests inside the request span):

```text
{"timestamp":"2026-06-02T06:51:10.937712Z","level":"DEBUG","fields":{"message":"started processing request"},"target":"tower_http::trace::on_request","span":{"method":"POST","uri":"/users","version":"HTTP/1.1","name":"request"},"spans":[{"method":"POST","uri":"/users","version":"HTTP/1.1","name":"request"}]}
{"timestamp":"2026-06-02T06:51:10.937895Z","level":"INFO","fields":{"message":"user created"},"target":"probe","span":{"user.name":"Ada","name":"create_user"},"spans":[{"method":"POST","uri":"/users","version":"HTTP/1.1","name":"request"},{"user.name":"Ada","name":"create_user"}]}
{"timestamp":"2026-06-02T06:51:10.937974Z","level":"DEBUG","fields":{"message":"finished processing request","latency":"0 ms","status":200},"target":"tower_http::trace::on_response","span":{"method":"POST","uri":"/users","version":"HTTP/1.1","name":"request"},"spans":[{"method":"POST","uri":"/users","version":"HTTP/1.1","name":"request"}]}
status = 200 OK
body   = {"id":1,"name":"Ada"}
```

Note the `authorization` header is set sensitive, so even at `debug` it never appears in the logged request fields.

---

## Detailed Explanation

### Logging: structured, leveled, redacted

The `tracing_subscriber::registry()` builder composes two layers: an `EnvFilter` that reads `RUST_LOG` (falling back to `info`), and a `fmt` layer in `.json()` mode. JSON is the right default in production because your log shipper (Loki, CloudWatch, Datadog) parses fields, not free text. The `#[instrument]` attribute on `create_user` opens a span carrying `user.name`; every `tracing::info!` inside it inherits that context, so a single log line tells you *which user* without manual string interpolation. Contrast with `console.log`: in Node you concatenate context by hand and hope every call site remembers to.

Redaction is structural, not a regex over the final string. `SetSensitiveRequestHeadersLayer` marks `authorization` and `cookie` as sensitive *before* `TraceLayer` reads the headers, so the secret is never rendered. Layer order matters: redaction must come before tracing in the `ServiceBuilder` stack.

### Errors: typed, mapped, never leaked

`ApiError` is a single `thiserror` enum for the whole surface. Its `IntoResponse` impl is the one place that maps a variant to an HTTP status, logs the real cause at the correct level (`error!` for 5xx, `warn!` for client errors), and, critically, returns a *generic* body for server errors. A `4xx` echoes a useful message; a `5xx` says only `"internal error"`. The `#[from] anyhow::Error` arm lets any deep failure bubble up with `?` and land as a 500 without you writing a conversion at every call site. See [Section 08: Error Handling](/08-error-handling/) for the `Result`/`?`/`thiserror`/`anyhow` foundations.

### Timeouts: bound everything

`TimeoutLayer::new(Duration::from_secs(5))` caps *inbound* request processing. Outbound calls need their own bound: wrap them in `tokio::time::timeout` (shown in Best Practices). Rust does not save you here automatically: a future that `.await`s a hung socket waits forever unless something cancels it. This is the same trap as a missing `AbortSignal` in Node, just enforced by the same explicitness.

### Limits: cap what can grow

`DefaultBodyLimit::max(1024 * 1024)` rejects bodies over 1 MiB with `413 Payload Too Large` *before* buffering them: a cheap defense against memory-exhaustion. Production services also cap concurrency (`tower::limit::ConcurrencyLimitLayer` or `tower::load_shed`) and per-client request rate (see [Rate Limiting](/28-production/06-rate-limiting/)). axum's `DefaultBodyLimit` is preferred over a raw tower-http body-limit layer because it integrates with extractors and returns the correct status cleanly.

### Observability and security

The request ID (`SetRequestIdLayer` + `PropagateRequestIdLayer`) is the thread that stitches logs, metrics, and traces together and is echoed back to the caller as `x-request-id` for support tickets. Metrics and distributed traces extend this, covered in [Metrics and Monitoring](/28-production/04-metrics/) and [Distributed Tracing](/28-production/05-distributed-tracing/). Security shows up as redaction, generic 5xx bodies, body limits, and — at the deployment layer — a minimal image and dependency auditing (covered below and in [Section 27: Security](/27-security/)).

---

## Key Differences

| Concern | TypeScript / Node (Express) | Rust (axum + tower) |
| --- | --- | --- |
| Structured logging | `pino`/`winston`, opt-in; context concatenated by hand | `tracing` spans propagate context automatically |
| Log redaction | `redact` path list over the object | Headers marked sensitive *structurally* before rendering |
| Inbound timeout | Manual `res.setTimeout`; no framework default | `TimeoutLayer` as a composable middleware |
| Outbound timeout | `AbortSignal.timeout` per `fetch`; easy to forget | `tokio::time::timeout`; equally explicit, type-checked |
| Body-size limit | `express.json({ limit })` | `DefaultBodyLimit::max` → real `413` |
| Error leakage | Must remember not to send `err` | Typed `IntoResponse` makes the safe path the default |
| Panic isolation | An unhandled throw can crash the process | `catch_panic` turns a panic into a `500`; worker survives |
| Concurrency model | Single-threaded event loop | Multi-threaded runtime, but **opt-in** and explicit |
| Config at startup | Reads `process.env` lazily; fails late | Validate into a typed struct; fail fast (see [Environment-Based Configuration](/28-production/01-environment/)) |

> **Tip:** Rust is **not** "multi-threaded by default." `#[tokio::main]` starts a multi-thread runtime, but you choose that; `#[tokio::main(flavor = "current_thread")]` gives a single-threaded one. Concurrency is fearless and opt-in, not implicit.

---

## Common Pitfalls

### Forgetting `IntoResponse` on your error type

A handler must return something axum knows how to turn into a response. Return a bare error type and the bound fails, with a message that, while long, points you at the fix:

```rust
use axum::{routing::get, Router};

#[derive(Debug)]
struct MyError;

// does not compile (error[E0277]: the trait bound `... : Handler<_, _>` is not satisfied)
async fn handler() -> Result<String, MyError> {
    Err(MyError)
}

fn main() {
    let _app: Router = Router::new().route("/", get(handler));
}
```

The real error from `cargo build`:

```text
error[E0277]: the trait bound `fn() -> impl Future<Output = Result<String, MyError>> {handler}: Handler<_, _>` is not satisfied
   --> src/main.rs:12:53
    |
 12 |     let _app: Router = Router::new().route("/", get(handler));
    |                                                 --- ^^^^^^^ the trait `Handler<_, _>` is not implemented for fn item `fn() -> impl Future<Output = Result<String, MyError>> {handler}`
    |
    = note: Consider using `#[axum::debug_handler]` to improve the error message
```

The fix is to implement `IntoResponse` for `MyError` (as in the main example). The note's suggestion — annotate the handler with `#[axum::debug_handler]` — is the fastest way to get a precise diagnostic when this happens to a real handler.

### `unwrap()` in a request path

`unwrap()` turns a recoverable error into a panic. In a handler that aborts the request (and, without `catch_panic`, can take down the worker). Clippy will not flag it by default, but the `clippy::unwrap_used` restriction lint will; turn it on for production crates:

```rust
#![warn(clippy::unwrap_used)]

fn parse_port(raw: &str) -> u16 {
    raw.parse().unwrap()
}

fn main() {
    println!("{}", parse_port("8080"));
}
```

`cargo clippy` then reports:

```text
warning: used `unwrap()` on a `Result` value
 --> src/main.rs:4:5
  |
4 |     raw.parse().unwrap()
  |     ^^^^^^^^^^^^^^^^^^^^
  |
  = note: if this value is an `Err`, it will panic
  = help: consider using `expect()` to provide a better panic message
  = help: for further information visit https://rust-lang.github.io/rust-clippy/master/index.html#unwrap_used
```

Reserve `unwrap`/`expect` for startup invariants you *want* to crash on (a malformed config is better as a loud panic at boot than a silent default). In request handling, propagate with `?`.

### Leaking the error cause to the client

This one **compiles** — it is a logic and security bug, not a type error. If your `IntoResponse` does `Json(ErrorBody { error: self.to_string() })` for *every* variant, a database error like `connection refused to db-primary.internal:5432` ends up in the client's response body, leaking topology to attackers. The fix in the main example is to gate on `status.is_server_error()` and emit only a generic string for 5xx. Always log the detail; never serialize it to an untrusted caller.

### Unbounded outbound `await`

A `reqwest`/`sqlx` call with no timeout will wait as long as the upstream hangs, tying up a connection and a task. Nothing in the type system forces a bound: wrap every outbound call in `tokio::time::timeout`. See Best Practices.

### Logging to plain text in production

`tracing_subscriber::fmt()` without `.json()` produces pretty, human-readable lines: perfect for `cargo run` locally, useless for a log aggregator. Gate the format on the environment: pretty in dev, `.json()` in prod (driven by config; see [Application Configuration](/28-production/00-configuration/)).

---

## Best Practices

### Bound every outbound call

```rust
use std::time::Duration;
use tokio::time::{sleep, timeout};

// Simulate an outbound dependency call (DB, HTTP, cache) that may hang.
async fn fetch_from_dependency(slow: bool) -> String {
    if slow {
        sleep(Duration::from_secs(10)).await; // a hung upstream
    }
    "ok".to_string()
}

#[tokio::main]
async fn main() {
    // ALWAYS bound an outbound call. An unbounded await is a latent outage.
    match timeout(Duration::from_millis(200), fetch_from_dependency(true)).await {
        Ok(value) => println!("got: {value}"),
        Err(_elapsed) => println!("dependency timed out after 200ms -> degrade gracefully"),
    }

    match timeout(Duration::from_millis(200), fetch_from_dependency(false)).await {
        Ok(value) => println!("got: {value}"),
        Err(_elapsed) => println!("timed out"),
    }
}
```

Real output:

```text
dependency timed out after 200ms -> degrade gracefully
got: ok
```

### Build a release profile that fails loud and ships small

In `Cargo.toml`, a production profile that aborts on panic (no unwinding, smaller binary) and strips symbols:

```toml
[profile.release]
opt-level = 3
lto = "thin"
codegen-units = 1
panic = "abort"   # no unwinding; a panic terminates the process (pair with an orchestrator restart)
strip = "symbols" # smaller binary, no symbol table in the image
```

> **Warning:** `panic = "abort"` means a panic kills the whole process, every task included. That is often *desirable* in a container (the orchestrator restarts a clean instance), but it makes `catch_panic` and unwind-based recovery unavailable. Decide deliberately. If you keep the default `unwind`, add `tower_http::catch_panic::CatchPanicLayer` so a single bad request returns a `500` instead of taking down a worker.

### The rest of the checklist

- **Configuration & environment:** load config into a typed struct and validate it at startup so a bad value fails fast; see [Application Configuration](/28-production/00-configuration/) and [Environment-Based Configuration](/28-production/01-environment/). Follow the [12-factor](https://12factor.net/) separation of config from code.
- **Graceful shutdown:** catch `SIGTERM`, flip readiness to `false`, and drain in-flight requests; see [Graceful Shutdown](/28-production/02-graceful-shutdown/).
- **Health probes:** distinct liveness and readiness endpoints (see [Health and Readiness Endpoints](/28-production/03-health-checks/)).
- **Metrics & tracing:** RED/USE signals and request-scoped traces; see [Metrics and Monitoring](/28-production/04-metrics/) and [Distributed Tracing](/28-production/05-distributed-tracing/).
- **Rate limiting & caching:** protect and accelerate; see [Rate Limiting](/28-production/06-rate-limiting/) and [Caching Strategies](/28-production/07-caching/).
- **Dependency hygiene:** run `cargo audit` (RustSec advisories) and `cargo deny` (licenses, bans, duplicate versions) in CI. Pin a `rust-toolchain.toml`.
- **Minimal runtime image:** build static or distroless. A from-scratch or distroless image has no shell and a tiny attack surface (see [Section 27: Security](/27-security/)).
- **Run as non-root, drop capabilities, read-only filesystem** in the container.

---

## Real-World Example

A panic in one handler should never take down the worker that serves every other request. With the default unwinding profile, `CatchPanicLayer` converts a handler panic into a clean `500`:

```bash
cargo add tower-http --features catch-panic
```

```rust
use axum::{body::Body, http::Request, routing::get, Router};
use tower::ServiceExt; // for `oneshot`
use tower_http::catch_panic::CatchPanicLayer;

async fn boom() -> &'static str {
    panic!("handler bug"); // a latent bug in one endpoint
}

fn app() -> Router {
    Router::new()
        .route("/boom", get(boom))
        // Turn a panic in any handler into a 500 instead of killing the worker.
        .layer(CatchPanicLayer::new())
}

#[tokio::main]
async fn main() {
    let resp = app()
        .oneshot(Request::builder().uri("/boom").body(Body::empty()).unwrap())
        .await
        .unwrap();
    println!("status = {}", resp.status());
}
```

Running it (the default panic hook prints the location and message to stderr first, then `CatchPanicLayer` converts the unwind into a response):

```text
thread 'main' panicked at src/main.rs:6:5:
handler bug
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
status = 500 Internal Server Error
```

The process keeps serving; the bad request gets a `500`; the panic message lands in your logs for triage. In a real deployment you would pair this with [Metrics and Monitoring](/28-production/04-metrics/) to alert on the `5xx` rate and [Distributed Tracing](/28-production/05-distributed-tracing/) to find the offending span. This is the defense-in-depth posture a production checklist exists to enforce: every layer assumes the one below it can fail.

---

## Further Reading

- [tower-http middleware](https://docs.rs/tower-http/latest/tower_http/) — `TraceLayer`, `TimeoutLayer`, request IDs, sensitive headers, and `catch_panic` used throughout this chapter.
- [`tracing` and `tracing-subscriber`](https://docs.rs/tracing-subscriber/latest/tracing_subscriber/): structured logging, JSON output, and `EnvFilter`.
- [axum `DefaultBodyLimit`](https://docs.rs/axum/latest/axum/extract/struct.DefaultBodyLimit.html) and [`IntoResponse`](https://docs.rs/axum/latest/axum/response/trait.IntoResponse.html): limits and error mapping.
- [`tokio::time::timeout`](https://docs.rs/tokio/latest/tokio/time/fn.timeout.html) — bounding outbound calls.
- [`cargo audit`](https://github.com/rustsec/rustsec/tree/main/cargo-audit) and [`cargo deny`](https://embarkstudios.github.io/cargo-deny/): dependency and license checks for CI.
- [The Twelve-Factor App](https://12factor.net/) — the config/logging/process discipline this checklist operationalizes.
- Guide cross-links:
  - [Configuration](/28-production/00-configuration/) and [Environment-Based Config](/28-production/01-environment/) — typed settings and fail-fast startup validation.
  - [Graceful Shutdown](/28-production/02-graceful-shutdown/), [Health Checks](/28-production/03-health-checks/) — clean draining and orchestrator probes.
  - [Metrics](/28-production/04-metrics/), [Distributed Tracing](/28-production/05-distributed-tracing/) — the observability beyond logs.
  - [Rate Limiting](/28-production/06-rate-limiting/), [Caching](/28-production/07-caching/), [Background Jobs](/28-production/08-background-jobs/) — protecting and scaling the service.
  - [Section 08: Error Handling](/08-error-handling/) — the `Result`/`?`/`thiserror`/`anyhow` foundations the error section builds on.
  - [Section 11: Async](/11-async/) — why Rust futures are lazy and need a runtime, which is what makes timeouts necessary.
  - [Section 16: Web APIs](/16-web-apis/) — the axum routing, extractors, and state used here.
  - [Section 27: Security](/27-security/) — secrets, minimal images, and hardening referenced in the security pillar.
  - [Section 02: Basic Types](/02-basics/01-types/) — the explicit numeric and `Result` types underpinning the error model.
  - [Section 29: Migration Guide](/29-migration-guide/) — porting a hardened Node service to this Rust stack.

---

## Exercises

### Exercise 1: Enforce a body-size limit

**Difficulty:** Beginner

**Objective:** Confirm that an oversized request body is rejected before your handler runs.

**Instructions:** Build a router with a single `POST /echo` handler that returns the request body as a string. Add a `DefaultBodyLimit` of 8 bytes (small, for the demo). Send a 100-byte body and assert the response status is `413 Payload Too Large`.

```rust
use axum::{body::Body, extract::DefaultBodyLimit, http::Request, routing::post, Router};
use tower::ServiceExt;

async fn echo(body: String) -> String {
    body
}

fn app() -> Router {
    Router::new()
        .route("/echo", post(echo))
        // TODO: cap the body at 8 bytes
        /* ??? */
}

#[tokio::main]
async fn main() {
    let big = "x".repeat(100);
    let resp = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/echo")
                .body(Body::from(big))
                .unwrap(),
        )
        .await
        .unwrap();
    println!("oversized body -> {}", resp.status());
}
```

<details>
<summary>Solution</summary>

```rust
use axum::{body::Body, extract::DefaultBodyLimit, http::Request, routing::post, Router};
use tower::ServiceExt;

async fn echo(body: String) -> String {
    body
}

fn app() -> Router {
    Router::new()
        .route("/echo", post(echo))
        .layer(DefaultBodyLimit::max(8)) // 8-byte cap for the demo
}

#[tokio::main]
async fn main() {
    let big = "x".repeat(100);
    let resp = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/echo")
                .body(Body::from(big))
                .unwrap(),
        )
        .await
        .unwrap();
    println!("oversized body -> {}", resp.status());
}
```

Add the dependencies with `cargo add axum tower` and `cargo add tokio --features full`. Output:

```text
oversized body -> 413 Payload Too Large
```

</details>

### Exercise 2: A typed error that never leaks

**Difficulty:** Intermediate

**Objective:** Implement `IntoResponse` so that client errors return a useful message but server errors return only a generic one, and the real cause is always logged.

**Instructions:** Define an `AppError` enum with `BadInput(String)` (→ `400`) and `Database(String)` (→ `500`). Implement `IntoResponse` so the `400` body contains the input message, the `500` body contains only `"internal error"`, and both log the real detail with `tracing`.

<details>
<summary>Solution</summary>

```rust
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
enum AppError {
    #[error("bad input: {0}")]
    BadInput(String),
    #[error("database failure: {0}")]
    Database(String),
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match &self {
            AppError::BadInput(_) => StatusCode::BAD_REQUEST,
            AppError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        if status.is_server_error() {
            tracing::error!(error = %self, "request failed");
        } else {
            tracing::warn!(error = %self, "request rejected");
        }
        let public = if status.is_server_error() {
            "internal error".to_string() // never leak the cause
        } else {
            self.to_string()
        };
        (status, Json(ErrorBody { error: public })).into_response()
    }
}

fn main() {
    // Confirm the mapping: a DB error becomes a generic 500 body.
    let resp = AppError::Database("connection refused to db:5432".into()).into_response();
    println!("status = {}", resp.status());
}
```

Dependencies: `cargo add axum serde --features serde/derive`, `cargo add thiserror`, and `cargo add tracing`. The `Database` variant carries `"connection refused to db:5432"`, but the client only ever sees `{"error":"internal error"}`; the real string is logged. Output:

```text
status = 500 Internal Server Error
```

</details>

### Exercise 3: Survive a panicking handler

**Difficulty:** Advanced

**Objective:** Add panic isolation so a bug in one endpoint returns a `500` instead of crashing the worker, and verify the rest of the router still serves.

**Instructions:** Build a router with two routes: `GET /ok` returning `"ok"` and `GET /boom` that `panic!`s. Apply `tower_http::catch_panic::CatchPanicLayer`. Send a request to `/boom`, assert `500`; then send a request to `/ok` on the same router and assert `200` — proving the worker survived.

<details>
<summary>Solution</summary>

```rust
use axum::{body::Body, http::Request, routing::get, Router};
use tower::ServiceExt;
use tower_http::catch_panic::CatchPanicLayer;

async fn ok() -> &'static str {
    "ok"
}

async fn boom() -> &'static str {
    panic!("handler bug");
}

fn app() -> Router {
    Router::new()
        .route("/ok", get(ok))
        .route("/boom", get(boom))
        .layer(CatchPanicLayer::new())
}

#[tokio::main]
async fn main() {
    let boom_resp = app()
        .oneshot(Request::builder().uri("/boom").body(Body::empty()).unwrap())
        .await
        .unwrap();
    println!("/boom -> {}", boom_resp.status());

    // A fresh request on a fresh service instance — the process never died.
    let ok_resp = app()
        .oneshot(Request::builder().uri("/ok").body(Body::empty()).unwrap())
        .await
        .unwrap();
    println!("/ok   -> {}", ok_resp.status());
}
```

Dependencies: `cargo add axum tower`, `cargo add tokio --features full`, and `cargo add tower-http --features catch-panic`. The default panic hook prints to stderr first, then the layer converts the unwind to a `500`, and the `/ok` route still answers:

```text
thread 'main' panicked at src/main.rs:10:5:
handler bug
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
/boom -> 500 Internal Server Error
/ok   -> 200 OK
```

> **Note:** `CatchPanicLayer` relies on unwinding, so it has no effect under `panic = "abort"`. If your release profile aborts on panic, isolation comes from the orchestrator restarting the process instead.

</details>
