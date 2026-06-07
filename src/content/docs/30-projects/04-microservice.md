---
title: "Project 5: Production Microservice (URL Shortener)"
description: "Build a deployable URL shortener in Rust with axum: layered config, structured JSON logs, health probes, a typed error, and graceful shutdown, the Node"
---

This project is a small but *production-shaped* HTTP microservice: a URL
shortener. You `POST` a long URL and get back a short code; hitting `/{code}`
redirects you to the original. That is the easy part. The point of this project
is everything *around* the two handlers that separates a weekend script from a
service you would actually deploy: layered configuration, structured JSON logs,
liveness/readiness probes, a single typed error, graceful shutdown, and a
storage layer hidden behind a trait so you can swap the in-memory map for Redis
without touching a single handler.

If you have shipped a Node service, you have assembled this same checklist by
hand: `dotenv` + `zod` for config, `pino` for logs, an `/healthz` route, a
`process.on('SIGTERM')` handler, a global Express error middleware, and a Redis
client. Here we build the equivalent with [axum](https://docs.rs/axum) 0.8,
[tokio](https://docs.rs/tokio), and [tracing](https://docs.rs/tracing); and the
compiler enforces most of the wiring for you.

> [!NOTE]
> Built and verified with Rust 1.96.0 (2024 edition), axum 0.8.9, tokio 1.52,
> tracing-subscriber 0.3, thiserror 2.0, and rand 0.9. Every command and every
> line of output in this guide was produced by actually running the code in
> `microservice-code/`.

## What You'll Build

A single binary, `url-shortener`, that exposes four endpoints:

| Method & path     | Purpose                                              |
| ----------------- | ---------------------------------------------------- |
| `POST /shorten`   | Validate a URL, store it under a random code, return the short link. |
| `GET /{code}`     | Look up a code and `307`-redirect to the original URL. |
| `GET /health`     | Liveness probe: is the process up?                  |
| `GET /ready`      | Readiness probe: can it serve traffic *right now*?  |

A typical session looks like this (real output, captured below):

```bash
$ curl -s -X POST http://localhost:8080/shorten \
    -H 'Content-Type: application/json' \
    -d '{"url":"https://www.rust-lang.org/learn"}'
{"code":"bl08zeb","short_url":"http://localhost:8080/bl08zeb","target":"https://www.rust-lang.org/learn"}

$ curl -s -i http://localhost:8080/bl08zeb
HTTP/1.1 307 Temporary Redirect
location: https://www.rust-lang.org/learn
```

Meanwhile, the service emits one structured JSON log object per event, ready for
a log aggregator like Loki, Datadog, or CloudWatch:

```json
{"timestamp":"2026-06-02T07:11:50.279287Z","level":"INFO","fields":{"message":"created short link","code":"bl08zeb","target":"https://www.rust-lang.org/learn"},"target":"url_shortener::routes::links","span":{"url":"https://www.rust-lang.org/learn","name":"shorten"},"spans":[{"url":"https://www.rust-lang.org/learn","name":"shorten"}]}
```

And when the orchestrator sends `SIGTERM`, it drains in-flight requests and exits
cleanly instead of dropping connections.

## Prerequisites

This project ties together most of the second half of the guide. If a concept
here feels unfamiliar, the linked section covers it in depth:

- [Section 11: Async](/11-async/) — `async`/`await`, `tokio`, and why
  Rust futures are *lazy* (the opposite of eager JavaScript promises).
- [Section 16: Web APIs](/16-web-apis/) — axum routing, extractors,
  and `IntoResponse`. This project is the production-hardened sibling of
  [Project 1: REST API](/30-projects/00-rest-api/).
- [Section 08: Error Handling](/08-error-handling/) — `Result`, the
  `?` operator, and `thiserror`.
- [Section 09: Generics & Traits](/09-generics-traits/) — the `Store`
  trait that makes the backend swappable.
- [Section 10: Smart Pointers](/10-smart-pointers/) — `Arc`,
  `RwLock`, and `AtomicU64` for shared state.
- [Section 28: Production](/28-production/) — the patterns this
  project demonstrates:
  [configuration](/28-production/00-configuration/),
  [health checks](/28-production/03-health-checks/),
  [graceful shutdown](/28-production/02-graceful-shutdown/), and
  [distributed tracing](/28-production/05-distributed-tracing/).
- [Section 17: Database](/17-database/) — for swapping the in-memory
  store for [Redis](/17-database/07-redis/) or Postgres.

## Project Structure

The code lives in [`microservice-code/`](https://github.com/zeybek/rs4ts/tree/main/examples/microservice-code). Unlike a toy
single-file example, it is split into focused modules. Each one maps to a
production concern:

```text
microservice-code/
├── Cargo.toml          # Dependencies, pinned to current stable versions
├── src/
│   ├── main.rs         # Binary entry point: load config, init logging, serve
│   ├── lib.rs          # Library crate: re-exports modules for the binary + tests
│   ├── config.rs       # Layered Settings struct (env vars + defaults)
│   ├── telemetry.rs    # tracing-subscriber setup (JSON or pretty logs)
│   ├── error.rs        # The single typed AppError + its IntoResponse impl
│   ├── models.rs       # Request/response DTOs (serde structs)
│   ├── state.rs        # AppState shared across all handlers
│   ├── store.rs        # The Store trait + in-memory implementation
│   ├── shutdown.rs     # Graceful-shutdown signal future (SIGINT/SIGTERM)
│   └── routes/
│       ├── mod.rs      # Router assembly + middleware layer stack
│       ├── health.rs   # /health (liveness) and /ready (readiness) handlers
│       └── links.rs    # /shorten and /{code} handlers
└── tests/
    └── api.rs          # End-to-end HTTP tests against the real router
```

> [!NOTE]
> Splitting into a `lib.rs` *library* crate plus a thin `main.rs` *binary* is a
> deliberate, idiomatic choice. The integration tests in `tests/api.rs` can only
> reach `url_shortener::routes::build_router` because it is exported from the
> library. A binary-only crate has no importable surface. See
> [Section 12: Modules & Packages](/12-modules-packages/).

## Walkthrough

We will build from the outside in: dependencies, then config and logging, then
the storage and error layers, then the handlers, and finally the `main` that
stitches it together with graceful shutdown.

### Step 1: Dependencies (`Cargo.toml`)

Scaffold the project and add the crates with `cargo add` (built into Cargo since
1.62 — no `cargo-edit` needed), which always resolves the newest compatible
version:

```bash
cargo new microservice-code --name url-shortener
cd microservice-code
cargo add axum@0.8
cargo add tokio@1 --features full
cargo add tower@0.5
cargo add tower-http@0.6 --features trace,timeout
cargo add serde@1 --features derive
cargo add serde_json@1
cargo add tracing@0.1
cargo add tracing-subscriber@0.3 --features env-filter,json
cargo add thiserror@2
cargo add rand@0.9
cargo add --dev reqwest@0.12 --no-default-features --features json,rustls-tls
```

The resulting `Cargo.toml`:

```toml
[package]
name = "url-shortener"
version = "0.1.0"
edition = "2024"

[dependencies]
axum = "0.8"
rand = "0.9"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
tokio = { version = "1", features = ["full"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["trace", "timeout"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }

[dev-dependencies]
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
```

> [!NOTE]
> Coming from a `package.json`, the `[dependencies]` / `[dev-dependencies]`
> split mirrors `dependencies` / `devDependencies` exactly. The
> `features = [...]` arrays are the big difference: crates ship with optional
> capabilities turned *off* by default, so you opt in to only what you use
> (`tracing-subscriber`'s `json` formatter, `tower-http`'s `timeout` layer).
> This keeps compile times and binary size down: there is no `tree-shaking`
> step because the unused code was never compiled in.

### Step 2: Layered configuration (`config.rs`)

A production service must be configurable without recompiling. The Node pattern
is `dotenv` to load `.env` into `process.env`, then `zod`/`convict` to parse and
validate. Here we centralise all of that in one strongly-typed `Settings` struct
whose constructor never panics — every field has a default, so the service boots
even with an empty environment.

```rust
//! Layered application configuration.
//!
//! Settings are resolved from environment variables, falling back to sane
//! defaults when a variable is absent or cannot be parsed. This mirrors the
//! `dotenv` + `convict`/`zod`-validated `process.env` pattern common in Node
//! services, but with parsing and validation centralised in one typed struct.

use std::env;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

/// Strongly-typed application settings.
///
/// Construct with [`Settings::from_env`], which never panics: every field has a
/// default, so a service started with an empty environment still boots.
#[derive(Debug, Clone)]
pub struct Settings {
    /// Address the HTTP server binds to (host + port).
    pub bind_addr: SocketAddr,
    /// Public base URL used when building short links (e.g. `http://localhost:8080`).
    pub base_url: String,
    /// Length of the generated short code (number of base62 characters).
    pub code_length: usize,
    /// Per-request timeout applied by the Tower timeout layer.
    pub request_timeout: Duration,
    /// Log output format: `json` for structured logs, anything else for pretty.
    pub log_format: LogFormat,
    /// Logging filter directive (the value normally found in `RUST_LOG`).
    pub log_filter: String,
}

/// Selects the `tracing-subscriber` output formatter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    /// Machine-readable JSON, one object per line — ideal for log aggregators.
    Json,
    /// Human-readable, colourised output for local development.
    Pretty,
}

impl Settings {
    /// Build [`Settings`] from environment variables with defaults applied.
    ///
    /// Recognised variables:
    /// - `HOST` (default `0.0.0.0`)
    /// - `PORT` (default `8080`)
    /// - `BASE_URL` (default derived from host + port)
    /// - `CODE_LENGTH` (default `7`)
    /// - `REQUEST_TIMEOUT_SECS` (default `15`)
    /// - `LOG_FORMAT` (`json` | `pretty`, default `json`)
    /// - `RUST_LOG` (default `info,url_shortener=debug,tower_http=info`)
    pub fn from_env() -> Self {
        let host = env_var("HOST").unwrap_or_else(|| "0.0.0.0".to_string());
        let port = parse_or("PORT", 8080u16);

        let ip: IpAddr = host
            .parse()
            .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
        let bind_addr = SocketAddr::new(ip, port);

        // A 0.0.0.0 bind is not a usable link host, so advertise localhost.
        let advertised_host = if host == "0.0.0.0" { "localhost" } else { &host };
        let base_url =
            env_var("BASE_URL").unwrap_or_else(|| format!("http://{advertised_host}:{port}"));

        let code_length = parse_or("CODE_LENGTH", 7usize).clamp(4, 32);
        let request_timeout = Duration::from_secs(parse_or("REQUEST_TIMEOUT_SECS", 15u64));

        let log_format = match env_var("LOG_FORMAT").as_deref() {
            Some("pretty") => LogFormat::Pretty,
            _ => LogFormat::Json,
        };

        let log_filter = env_var("RUST_LOG")
            .unwrap_or_else(|| "info,url_shortener=debug,tower_http=info".to_string());

        Settings {
            bind_addr,
            base_url,
            code_length,
            request_timeout,
            log_format,
            log_filter,
        }
    }
}

/// Read an environment variable, treating empty strings as absent.
fn env_var(key: &str) -> Option<String> {
    match env::var(key) {
        Ok(v) if !v.trim().is_empty() => Some(v),
        _ => None,
    }
}

/// Parse an environment variable into `T`, falling back to `default` on any error.
fn parse_or<T: std::str::FromStr>(key: &str, default: T) -> T {
    env_var(key)
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}
```

A few things a TypeScript developer should notice:

- **`process.env.PORT` is `string | undefined`; here `bind_addr` is a real
  `SocketAddr`.** The `parse_or` helper turns the stringly-typed environment
  into typed values *once*, at the edge. After `from_env`, no handler ever sees
  a raw string or has to remember that `PORT` might be missing. The type system
  guarantees it is a `u16` inside a `SocketAddr`.
- **`LogFormat` is an `enum`, not a string.** A typo like `LOG_FORMAT=jsno`
  falls through the `match` to the `Json` default rather than silently
  mis-configuring a string comparison later.
- **`.clamp(4, 32)`** bounds the code length so a hostile `CODE_LENGTH=0` or
  `CODE_LENGTH=99999` can't break the service — validation lives next to the
  default. See [Section 28: Configuration](/28-production/00-configuration/) for
  the broader pattern (and how to layer in a `config.toml` file with the
  [`config`](https://docs.rs/config) crate).

### Step 3: Structured logging (`telemetry.rs`)

`tracing` is the Rust equivalent of `pino`/`winston`, but it is built around
*spans* (timed, nested units of work) in addition to flat log events. The
subscriber we install decides how those spans and events are rendered.

```rust
//! Structured logging setup via `tracing` + `tracing-subscriber`.
//!
//! `tracing` is to Rust what `pino`/`winston` are to Node, but it is built
//! around *spans* (timed, nested units of work) as well as flat events. The
//! `#[tracing::instrument]` attributes on the handlers create those spans; the
//! subscriber configured here decides how they are rendered.

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

use crate::config::{LogFormat, Settings};

/// Install the global tracing subscriber based on the configured log format.
///
/// Call this exactly once, as early in `main` as possible, so that even
/// startup messages are captured.
pub fn init(settings: &Settings) {
    let filter = EnvFilter::new(&settings.log_filter);
    let registry = tracing_subscriber::registry().with(filter);

    match settings.log_format {
        LogFormat::Json => registry
            .with(
                tracing_subscriber::fmt::layer()
                    .json()
                    .with_current_span(true)
                    .with_target(true),
            )
            .init(),
        LogFormat::Pretty => registry
            .with(tracing_subscriber::fmt::layer().with_target(true))
            .init(),
    }
}
```

The `EnvFilter` is the `RUST_LOG` mechanism: `info,url_shortener=debug,tower_http=info`
means "everything at INFO by default, but DEBUG for our own crate". This is the
same idea as `DEBUG=myapp:*` with the Node `debug` package, but per-module and
per-level. The two-formatter `match` lets us emit machine-readable JSON in
production (`LOG_FORMAT=json`, the default) and colourised human output locally
(`LOG_FORMAT=pretty`).

### Step 4: The typed error (`error.rs`)

In Express you throw, then a single error-handling middleware (`(err, req, res,
next) => …`) catches everything and decides the status code. axum has no such
global middleware; instead, any type that implements `IntoResponse` can be the
`Err` of a handler's `Result`, and axum converts it to an HTTP response
automatically. We define exactly one error type for the whole service.

```rust
//! A single typed error for the whole service.
//!
//! Every fallible handler returns `Result<T, AppError>`. Because `AppError`
//! implements [`IntoResponse`], axum converts a returned error into a proper
//! HTTP response automatically — there is no equivalent of Express's
//! `next(err)` plumbing or a global error-handling middleware to register.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

/// All the ways a request can fail in this service.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// The submitted URL was empty or not a valid `http(s)` URL.
    #[error("invalid url: {0}")]
    InvalidUrl(String),

    /// No short code matched the requested path.
    #[error("short code not found: {0}")]
    NotFound(String),

    /// The store could not satisfy the request (e.g. lock poisoned).
    #[error("internal store error")]
    Store,
}

impl AppError {
    /// Map each variant to its HTTP status code.
    fn status(&self) -> StatusCode {
        match self {
            AppError::InvalidUrl(_) => StatusCode::BAD_REQUEST,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Store => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// A short, stable machine-readable error code for clients.
    fn code(&self) -> &'static str {
        match self {
            AppError::InvalidUrl(_) => "invalid_url",
            AppError::NotFound(_) => "not_found",
            AppError::Store => "internal_error",
        }
    }
}

/// The JSON body returned for any error.
#[derive(Serialize)]
struct ErrorBody {
    error: &'static str,
    message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();

        // Server-side faults are logged at error level; client mistakes at debug.
        if status.is_server_error() {
            tracing::error!(error = %self, code = self.code(), "request failed");
        } else {
            tracing::debug!(error = %self, code = self.code(), "request rejected");
        }

        let body = ErrorBody {
            error: self.code(),
            message: self.to_string(),
        };
        (status, Json(body)).into_response()
    }
}
```

`thiserror`'s `#[error("...")]` derive generates the `Display` implementation,
so `AppError::NotFound("abc".into()).to_string()` is `"short code not found:
abc"`, with no hand-written `match` for messages. The `match` arms in `status()` and
`code()` are *exhaustive*: add a new variant and the compiler forces you to map
its status code, so you can never ship an error that falls back to a generic
500 by accident. (Contrast: in TypeScript a new thrown error type just lands in
the catch-all middleware unnoticed.) Note also that we log *inside*
`into_response`, splitting 5xx faults (ERROR level, page someone) from 4xx
client mistakes (DEBUG, don't).

### Step 5: The storage layer (`store.rs`)

The store is hidden behind a `Store` trait. The default `InMemoryStore` uses an
`Arc<RwLock<HashMap>>`, so the service runs with zero external dependencies.
But because the handlers depend only on the trait method calls (well, on the
concrete `InMemoryStore` held in state, which *implements* the trait), the
implementation is a drop-in swap for a Redis client later.

```rust
//! The persistence layer, behind a trait so the backing store is swappable.
//!
//! The default [`InMemoryStore`] keeps everything in an `Arc<RwLock<HashMap>>`,
//! which makes the service runnable with zero external dependencies. In
//! production you would implement [`Store`] over Redis or Postgres instead —
//! see `../17-database/07_redis.md` and `../17-database/README.md`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use rand::Rng;

use crate::error::AppError;

/// Behaviour every storage backend must provide.
///
/// The methods are synchronous because the in-memory `RwLock` never awaits.
/// A Redis-backed implementation would make these `async` (or wrap a
/// connection pool) — the handlers depend only on this trait, not the concrete
/// type, so swapping backends does not ripple through the codebase.
pub trait Store: Send + Sync + 'static {
    /// Persist `target` under a freshly generated `code` and return the code.
    fn insert(&self, code: String, target: String) -> Result<(), AppError>;

    /// Look up the original URL for a short `code`, incrementing its hit count.
    fn resolve(&self, code: &str) -> Result<Option<String>, AppError>;

    /// Total number of links currently stored.
    ///
    /// This also doubles as the readiness probe's connectivity check: for the
    /// in-memory store it just acquires the lock, but a Redis-backed
    /// implementation would `PING` here, and an `Err` makes `/ready` answer
    /// `503` so the orchestrator stops routing to a broken instance.
    fn len(&self) -> Result<usize, AppError>;

    /// Whether the store holds no links. (Pairs with [`len`](Store::len).)
    fn is_empty(&self) -> Result<bool, AppError> {
        Ok(self.len()? == 0)
    }
}

/// One stored link: where it points and how often it has been followed.
#[derive(Debug, Clone)]
struct Entry {
    target: String,
    hits: u64,
}

/// In-memory [`Store`] backed by an `Arc<RwLock<HashMap>>`.
///
/// `Arc` lets every request handler share one store cheaply; `RwLock` allows
/// many concurrent readers (redirects) and exclusive writers (new links).
#[derive(Clone, Default)]
pub struct InMemoryStore {
    inner: Arc<RwLock<HashMap<String, Entry>>>,
    redirects: Arc<AtomicU64>,
}

impl InMemoryStore {
    /// Create an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Total redirects served since startup (a cheap liveness/traffic signal).
    pub fn redirect_count(&self) -> u64 {
        self.redirects.load(Ordering::Relaxed)
    }

    /// Generate a random base62 short code of `len` characters.
    pub fn generate_code(len: usize) -> String {
        const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
        let mut rng = rand::rng();
        (0..len)
            .map(|_| {
                let idx = rng.random_range(0..ALPHABET.len());
                ALPHABET[idx] as char
            })
            .collect()
    }
}

impl Store for InMemoryStore {
    fn insert(&self, code: String, target: String) -> Result<(), AppError> {
        let mut map = self.inner.write().map_err(|_| AppError::Store)?;
        map.insert(code, Entry { target, hits: 0 });
        Ok(())
    }

    fn resolve(&self, code: &str) -> Result<Option<String>, AppError> {
        let mut map = self.inner.write().map_err(|_| AppError::Store)?;
        match map.get_mut(code) {
            Some(entry) => {
                entry.hits += 1;
                self.redirects.fetch_add(1, Ordering::Relaxed);
                Ok(Some(entry.target.clone()))
            }
            None => Ok(None),
        }
    }

    fn len(&self) -> Result<usize, AppError> {
        let map = self.inner.read().map_err(|_| AppError::Store)?;
        Ok(map.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_then_resolve_roundtrips() {
        let store = InMemoryStore::new();
        store
            .insert("abc123".into(), "https://example.com".into())
            .unwrap();

        assert_eq!(store.len().unwrap(), 1);
        assert_eq!(
            store.resolve("abc123").unwrap().as_deref(),
            Some("https://example.com")
        );
        // resolve incremented the global redirect counter.
        assert_eq!(store.redirect_count(), 1);
    }

    #[test]
    fn missing_code_resolves_to_none() {
        let store = InMemoryStore::new();
        assert_eq!(store.resolve("nope").unwrap(), None);
        assert_eq!(store.len().unwrap(), 0);
    }

    #[test]
    fn generated_codes_have_requested_length() {
        let code = InMemoryStore::generate_code(7);
        assert_eq!(code.len(), 7);
        assert!(code.chars().all(|c| c.is_ascii_alphanumeric()));
    }
}
```

Why this shape?

- **`Arc<RwLock<HashMap>>`** is the canonical "shared mutable state across many
  async tasks" pattern. `Arc` (atomic reference count) lets every cloned
  `AppState` point at the *same* map; `RwLock` allows many concurrent readers
  (redirects) but exclusive writers (new links). In Node you don't think about
  this because there is one thread and one event loop, but tokio runs your
  handlers across a thread pool, so the compiler *requires* you to make shared
  state thread-safe. See [Section 10: Smart Pointers](/10-smart-pointers/).
- **`AtomicU64` for the redirect counter** lets us bump a global counter without
  taking the write lock, using a relaxed atomic add. It is the lock-free
  equivalent of `metrics.increment('redirects')`.
- **`rand::rng()` + `rng.random_range(..)`** is the rand 0.9 API (the old 0.8
  `thread_rng()` / `gen_range` names are gone). A real shortener would also
  guard against the (astronomically unlikely) collision by re-rolling on a
  duplicate; we keep it simple here.
- **The `#[cfg(test)]` module** is a *unit* test compiled only in test builds,
  living right next to the code it tests: Rust's answer to a co-located
  `store.test.ts`.

### Step 6: Shared state (`state.rs`) and DTOs (`models.rs`)

axum clones the application state once per request, so it must be cheap to
clone. `AppState` wraps the (immutable) settings in `Arc` and holds the
already-`Arc`-backed store. This is the typed equivalent of stashing shared
objects on Express's `app.locals`.

```rust
//! Shared application state passed to every handler.
//!
//! axum clones the state per request, so it must be cheap to clone. We wrap the
//! settings in `Arc` and rely on `InMemoryStore` already being `Arc`-backed.
//! This is the typed equivalent of stashing things on `app.locals` in Express.

use std::sync::Arc;
use std::time::Instant;

use crate::config::Settings;
use crate::store::InMemoryStore;

/// Everything a handler might need: configuration, the store, and start time.
#[derive(Clone)]
pub struct AppState {
    /// Immutable, shared configuration.
    pub settings: Arc<Settings>,
    /// The link store (swap this type to change backends).
    pub store: InMemoryStore,
    /// When the process started, used to report uptime on `/health`.
    pub started_at: Instant,
}

impl AppState {
    /// Assemble state from already-loaded settings.
    pub fn new(settings: Settings) -> Self {
        AppState {
            settings: Arc::new(settings),
            store: InMemoryStore::new(),
            started_at: Instant::now(),
        }
    }

    /// Seconds elapsed since the process started.
    pub fn uptime_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }
}
```

The request/response shapes are plain `serde` structs — the contract of the
service, equivalent to the `interface`s or `zod` schemas you'd write for an
Express body and JSON response:

```rust
//! Request and response data-transfer objects.
//!
//! These `serde`-derived structs are this service's contract — the Rust
//! analogue of the `interface`s / `zod` schemas you would define for an
//! Express handler's body and JSON response.

use serde::{Deserialize, Serialize};

/// Body of `POST /shorten`.
#[derive(Debug, Deserialize)]
pub struct ShortenRequest {
    /// The long URL to shorten.
    pub url: String,
}

/// Successful response from `POST /shorten`.
#[derive(Debug, Serialize)]
pub struct ShortenResponse {
    /// The generated short code (e.g. `aZ3kP9q`).
    pub code: String,
    /// The full short link, ready to share.
    pub short_url: String,
    /// Echo of the original URL.
    pub target: String,
}

/// Response body for `GET /health` and `GET /ready`.
#[derive(Debug, Serialize)]
pub struct HealthResponse {
    /// `ok` when the service is healthy.
    pub status: &'static str,
    /// Process uptime in seconds.
    pub uptime_secs: u64,
    /// Number of links currently stored.
    pub links: usize,
    /// Total redirects served since startup.
    pub redirects: u64,
}
```

`#[derive(Deserialize)]` on `ShortenRequest` is what lets axum's `Json(payload):
Json<ShortenRequest>` extractor parse the body *and* reject malformed JSON with
a `422` automatically. The `serde` derive replaces a hand-written
`express.json()` + manual field validation.

### Step 7: The handlers (`routes/links.rs`)

Now the actual feature. Two handlers: create a link, and redirect by code.

```rust
//! Core URL-shortener handlers: create a short link, and redirect by code.

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum::Json;

use crate::error::AppError;
use crate::models::{ShortenRequest, ShortenResponse};
use crate::state::AppState;
use crate::store::{InMemoryStore, Store};

/// `POST /shorten` — validate the URL, generate a code, store it, return the link.
///
/// `#[tracing::instrument]` creates a span for the whole handler. We `skip` the
/// state (it is large and not interesting) but record the submitted URL, so
/// every log line emitted inside this handler is automatically tagged with it —
/// far less boilerplate than threading a `requestId` through Express callbacks.
#[tracing::instrument(skip(state, payload), fields(url = %payload.url))]
pub async fn shorten(
    State(state): State<AppState>,
    Json(payload): Json<ShortenRequest>,
) -> Result<Json<ShortenResponse>, AppError> {
    let target = validate_url(&payload.url)?;

    let code = InMemoryStore::generate_code(state.settings.code_length);
    state.store.insert(code.clone(), target.clone())?;

    let short_url = format!("{}/{}", state.settings.base_url, code);
    tracing::info!(%code, %target, "created short link");

    Ok(Json(ShortenResponse {
        code,
        short_url,
        target,
    }))
}

/// `GET /{code}` — look up the code and issue a `307` redirect to the target.
#[tracing::instrument(skip(state))]
pub async fn redirect(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<Response, AppError> {
    match state.store.resolve(&code)? {
        Some(target) => {
            tracing::info!(%code, %target, "redirecting");
            Ok(Redirect::temporary(&target).into_response())
        }
        None => Err(AppError::NotFound(code)),
    }
}

/// Reject empty input and anything that is not an absolute `http`/`https` URL.
///
/// A real service would use the `url` crate for full RFC parsing; this keeps the
/// example dependency-light while still demonstrating typed validation errors.
fn validate_url(raw: &str) -> Result<String, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidUrl("url must not be empty".into()));
    }
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return Err(AppError::InvalidUrl(
            "url must start with http:// or https://".into(),
        ));
    }
    Ok(trimmed.to_string())
}
```

The `?` operator does the heavy lifting: `validate_url(&payload.url)?` returns
early with an `AppError::InvalidUrl` if validation fails, and `state.store
.insert(...)?` propagates a store error. Because the handler returns
`Result<_, AppError>` and `AppError: IntoResponse`, that early return *becomes
the HTTP response*: no `try/catch`, no `next(err)`.

The `#[tracing::instrument]` attribute is worth dwelling on. It wraps the whole
handler in a span named `shorten` and attaches `url = <the submitted url>` to
it. Every log event emitted *inside* the handler (and inside anything it calls)
automatically inherits that context. Compare to Node, where you'd thread a
`requestId` or a child logger through every function call by hand. We
`skip(state, payload)` so the big state struct and the raw payload struct don't
get dumped into every span; we record just the `url` field we care about.

> [!NOTE]
> `Redirect::temporary` produces a `307 Temporary Redirect` (the method and body
> are preserved). Use `Redirect::permanent` (`308`) only if the mapping will
> never change — browsers and proxies cache `308`s aggressively, which would
> make a later edit to the link invisible.

### Step 8: Health and readiness (`routes/health.rs`)

Orchestrators distinguish two probes. **Liveness** (`/health`) answers "is the
process alive?" If it fails, restart the container. **Readiness** (`/ready`)
answers "can it serve traffic *right now*?" — if it fails (e.g. the database is
down), pull the instance out of the load balancer without killing it. Conflating
the two is a classic production bug: a readiness failure that triggers a restart
loop.

```rust
//! Liveness and readiness probes.
//!
//! Kubernetes (and most orchestrators) distinguish two checks:
//! - **liveness** (`/health`): is the process up? Restart it if not.
//! - **readiness** (`/ready`): can it serve traffic *right now*? Pull it from
//!   the load balancer if not.
//!
//! See `../28-production/03_health-checks.md` for the production rationale.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use crate::error::AppError;
use crate::models::HealthResponse;
use crate::state::AppState;
use crate::store::Store;

/// `GET /health` — liveness. Always returns `200 OK` if the process can route.
pub async fn health(State(state): State<AppState>) -> Result<Json<HealthResponse>, AppError> {
    let body = HealthResponse {
        status: "ok",
        uptime_secs: state.uptime_secs(),
        links: state.store.len()?,
        redirects: state.store.redirect_count(),
    };
    Ok(Json(body))
}

/// `GET /ready` — readiness. Probes the store; returns `503` if it is broken.
///
/// For the in-memory store this only fails if the lock is poisoned, but the
/// same shape covers a real backend: ping Redis / run `SELECT 1` here and map a
/// failure to `503 Service Unavailable` so the orchestrator stops routing to
/// this instance until it recovers.
pub async fn ready(
    State(state): State<AppState>,
) -> Result<Json<HealthResponse>, (StatusCode, Json<serde_json::Value>)> {
    // `is_empty` exercises the same connectivity path as a real backend ping.
    match state.store.is_empty().and_then(|_| state.store.len()) {
        Ok(links) => Ok(Json(HealthResponse {
            status: "ready",
            uptime_secs: state.uptime_secs(),
            links,
            redirects: state.store.redirect_count(),
        })),
        Err(_) => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "status": "unavailable",
                "error": "store_unreachable"
            })),
        )),
    }
}
```

`/health` returns the typed `AppError` path; `/ready` returns a tuple
`(StatusCode, Json<Value>)` directly to show the alternative — a handler can
return *any* `IntoResponse`, including an ad-hoc `503` with a `serde_json::json!`
body. See [Section 28: Health Checks](/28-production/03-health-checks/) for
deeper probe design (e.g. separating "starting up" from "ready").

### Step 9: Wiring the router (`routes/mod.rs`)

The router maps paths to handlers and stacks the Tower middleware layers.

```rust
//! Router assembly: wire handlers to paths and stack the middleware layers.

mod health;
mod links;

use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::Router;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;

use crate::state::AppState;

/// Build the complete application router from shared state.
///
/// The layer order matters: layers added later wrap the handlers more tightly,
/// so `TraceLayer` (added first here) sits outermost and sees every request,
/// while `TimeoutLayer` runs closer to the handler. This is the typed,
/// compile-checked version of `app.use(...)` middleware stacking in Express.
pub fn build_router(state: AppState) -> Router {
    let timeout = state.settings.request_timeout;

    Router::new()
        .route("/health", get(health::health))
        .route("/ready", get(health::ready))
        .route("/shorten", post(links::shorten))
        // axum 0.8 path params use `{name}` syntax (not the old `:name`).
        .route("/{code}", get(links::redirect))
        // A request exceeding the budget gets `408 Request Timeout`.
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            timeout,
        ))
        // Emit a structured span + access log for every request/response.
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
```

Two production-relevant details:

- **axum 0.8 path syntax is `{code}`**, not the `:code` you'd write in Express
  or in older axum. Getting this wrong is a common gotcha after upgrading.
- **`TraceLayer::new_for_http()`** is what produces the per-request access logs
  (method, URI, status, latency) you'll see in the pretty-format output. It is
  the `morgan`/`pino-http` of the Rust world, and it ties request spans into the
  same `tracing` system as our handler logs.
- **`TimeoutLayer`** caps how long any single request may run, returning `408`
  if it blows the budget: a backstop against a slow dependency hanging your
  worker, like wrapping every route in a `Promise.race([handler, timeout])`.

> [!NOTE]
> `tower` and `tower-http` are a shared middleware ecosystem: any `Layer` works
> with axum, tonic (gRPC), or a bare hyper server. CORS, compression, request-body
> limits, and rate limiting are all just more layers you `.layer(...)` on. See
> [Section 28: Rate Limiting](/28-production/06-rate-limiting/).

### Step 10: Graceful shutdown (`shutdown.rs`)

When a deploy rolls or you scale down, the orchestrator sends `SIGTERM` and
expects the process to stop accepting *new* connections, finish the *in-flight*
ones, and exit. Dropping live requests mid-flight means 502s for users. In Node
you'd write `process.on('SIGTERM', () => server.close())`. Here we return a
future that resolves on the first signal, and hand it to axum.

```rust
//! Graceful shutdown: stop accepting new connections on a signal, then let
//! in-flight requests finish before the process exits.
//!
//! In Node you would listen for `process.on('SIGTERM', ...)` and call
//! `server.close()`. Here we return a future that resolves on the first signal;
//! `axum::serve(...).with_graceful_shutdown(future)` does the rest.
//! See `../28-production/02_graceful-shutdown.md`.

/// Resolve when the process receives `Ctrl-C` (SIGINT) or, on Unix, SIGTERM.
///
/// SIGTERM is what container orchestrators (Docker, Kubernetes) send when they
/// want a pod to stop, so handling it is what makes rolling deploys graceful.
pub async fn signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl-C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    // Whichever signal arrives first wins.
    tokio::select! {
        () = ctrl_c => tracing::info!("received SIGINT (Ctrl-C), shutting down"),
        () = terminate => tracing::info!("received SIGTERM, shutting down"),
    }
}
```

`tokio::select!` races two futures and proceeds with whichever completes first —
the idiomatic way to wait for "any of these events". The `#[cfg(unix)]` /
`#[cfg(not(unix))]` pair is conditional compilation: SIGTERM only exists on Unix,
so on Windows the `terminate` branch becomes a future that never resolves
(`std::future::pending`). This is compile-time platform branching: the unused
branch isn't `#ifdef`-skipped at runtime, it is never compiled at all.

### Step 11: The entry point (`main.rs`)

Finally, `main` stitches everything together in order: config, then logging,
then state, then router, then serve-with-shutdown.

```rust
//! Production-ready URL-shortener microservice (binary entry point).
//!
//! This is a thin wrapper: all logic lives in the library crate (`lib.rs`).
//! `main` wires the pieces together and demonstrates the Section 28 production
//! patterns end to end:
//! - layered configuration (`config`)
//! - structured JSON logging (`telemetry`)
//! - a single typed error (`error`)
//! - `/health` + `/ready` probes (`routes`)
//! - graceful shutdown on SIGINT/SIGTERM (`shutdown`)
//! - an in-memory store behind a trait (`store`) so Redis can drop in later.

use tokio::net::TcpListener;

use url_shortener::config::Settings;
use url_shortener::state::AppState;
use url_shortener::{routes, shutdown, telemetry};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Load configuration from the environment (with defaults).
    let settings = Settings::from_env();

    // 2. Bring up structured logging before anything else can emit events.
    telemetry::init(&settings);

    tracing::info!(
        bind = %settings.bind_addr,
        base_url = %settings.base_url,
        code_length = settings.code_length,
        timeout_secs = settings.request_timeout.as_secs(),
        "starting url-shortener"
    );

    // 3. Build shared state and the router.
    let bind_addr = settings.bind_addr;
    let state = AppState::new(settings);
    let app = routes::build_router(state);

    // 4. Bind the TCP listener and serve with graceful shutdown.
    let listener = TcpListener::bind(bind_addr).await?;
    tracing::info!(addr = %listener.local_addr()?, "listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown::signal())
        .await?;

    tracing::info!("shutdown complete");
    Ok(())
}
```

`#[tokio::main]` is the macro that turns `async fn main` into a synchronous
`main` that boots the tokio runtime, the thing that actually *polls* our lazy
futures. Remember from [Section 11](/11-async/): unlike a JavaScript
`Promise`, a Rust future does nothing until a runtime drives it. `axum::serve`
is the axum 0.8 entry point (it replaced the old `axum::Server` builder), and
`.with_graceful_shutdown(shutdown::signal())` is the one line that wires in our
signal future.

The supporting `lib.rs` is just a list of the public modules:

```rust
//! Library crate for the URL-shortener microservice.
//!
//! The binary (`main.rs`) is a thin wrapper around this library: it loads
//! [`config::Settings`], initialises [`telemetry`], builds the router with
//! [`routes::build_router`], and serves it. Exposing the internals as a library
//! is what lets the integration tests in `tests/` boot the real app in-process.

pub mod config;
pub mod error;
pub mod models;
pub mod routes;
pub mod shutdown;
pub mod state;
pub mod store;
pub mod telemetry;
```

## Running It

### Build and run

```bash
cargo run
```

Real output of a clean build (first run compiles dependencies; subsequent runs
are instant):

```text
   Compiling url-shortener v0.1.0 (.../examples/microservice-code)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 9.93s
```

On startup, with the default JSON log format, the service prints two lines:

```json
{"timestamp":"2026-06-02T07:11:50.204052Z","level":"INFO","fields":{"message":"starting url-shortener","bind":"0.0.0.0:8080","base_url":"http://localhost:8080","code_length":7,"timeout_secs":15},"target":"url_shortener"}
{"timestamp":"2026-06-02T07:11:50.204658Z","level":"INFO","fields":{"message":"listening","addr":"0.0.0.0:8080"},"target":"url_shortener"}
```

### Exercise the API with curl

Create a short link (real response):

```bash
$ curl -s -X POST http://localhost:8080/shorten \
    -H 'Content-Type: application/json' \
    -d '{"url":"https://www.rust-lang.org/learn"}'
{"code":"bl08zeb","short_url":"http://localhost:8080/bl08zeb","target":"https://www.rust-lang.org/learn"}
```

Follow the short code — note the `307` and the `location` header (we use `-i` and
do *not* follow the redirect, to show the raw response):

```bash
$ curl -s -i http://localhost:8080/leAJT0b
HTTP/1.1 307 Temporary Redirect
location: https://doc.rust-lang.org/book/
content-length: 0
date: Tue, 02 Jun 2026 07:11:50 GMT
```

Request an unknown code, and the typed `NotFound` becomes a clean `404` with a JSON
body:

```bash
$ curl -s -i http://localhost:8080/nope
HTTP/1.1 404 Not Found
...
{"error":"not_found","message":"short code not found: nope"}
```

Submit an invalid URL, and `InvalidUrl` becomes a `400`:

```bash
$ curl -s -i -X POST http://localhost:8080/shorten \
    -H 'Content-Type: application/json' \
    -d '{"url":"ftp://x"}'
HTTP/1.1 400 Bad Request
...
{"error":"invalid_url","message":"invalid url: url must start with http:// or https://"}
```

Check the probes (real responses after creating two links and following one):

```bash
$ curl -s http://localhost:8080/health
{"status":"ok","uptime_secs":0,"links":2,"redirects":1}

$ curl -s http://localhost:8080/ready
{"status":"ready","uptime_secs":0,"links":2,"redirects":1}
```

### The structured logs

Driving the requests above produces this real JSON log stream (one object per
line — exactly what a log shipper expects). Note the `span` object attached to
the handler events, carrying the `url` and `code` context we set with
`#[tracing::instrument]`, and the DEBUG-level `request rejected` lines from our
`AppError::into_response`:

```json
{"timestamp":"2026-06-02T07:11:50.279287Z","level":"INFO","fields":{"message":"created short link","code":"bl08zeb","target":"https://www.rust-lang.org/learn"},"target":"url_shortener::routes::links","span":{"url":"https://www.rust-lang.org/learn","name":"shorten"},"spans":[{"url":"https://www.rust-lang.org/learn","name":"shorten"}]}
{"timestamp":"2026-06-02T07:11:50.289243Z","level":"INFO","fields":{"message":"created short link","code":"leAJT0b","target":"https://doc.rust-lang.org/book/"},"target":"url_shortener::routes::links","span":{"url":"https://doc.rust-lang.org/book/","name":"shorten"},"spans":[{"url":"https://doc.rust-lang.org/book/","name":"shorten"}]}
{"timestamp":"2026-06-02T07:11:50.298507Z","level":"INFO","fields":{"message":"redirecting","code":"leAJT0b","target":"https://doc.rust-lang.org/book/"},"target":"url_shortener::routes::links","span":{"code":"\"leAJT0b\"","name":"redirect"},"spans":[{"code":"\"leAJT0b\"","name":"redirect"}]}
{"timestamp":"2026-06-02T07:11:50.311046Z","level":"DEBUG","fields":{"message":"request rejected","error":"short code not found: nope","code":"not_found"},"target":"url_shortener::error"}
{"timestamp":"2026-06-02T07:11:50.330346Z","level":"DEBUG","fields":{"message":"request rejected","error":"invalid url: url must start with http:// or https://","code":"invalid_url"},"target":"url_shortener::error"}
```

Prefer human-readable logs locally? Set `LOG_FORMAT=pretty` and bump
`tower_http` to `debug` to see the per-request access logs from `TraceLayer`:

```bash
LOG_FORMAT=pretty RUST_LOG="info,url_shortener=debug,tower_http=debug" cargo run
```

Real pretty output (ANSI colour codes stripped for the page):

```text
2026-06-02T07:09:48.720633Z  INFO url_shortener: starting url-shortener bind=0.0.0.0:8081 base_url=http://localhost:8081 code_length=7 timeout_secs=15
2026-06-02T07:09:48.721298Z  INFO url_shortener: listening addr=0.0.0.0:8081
2026-06-02T07:09:48.824909Z DEBUG request{method=GET uri=/health version=HTTP/1.1}: tower_http::trace::on_request: started processing request
2026-06-02T07:09:48.825031Z DEBUG request{method=GET uri=/health version=HTTP/1.1}: tower_http::trace::on_response: finished processing request latency=0 ms status=200
2026-06-02T07:09:48.834426Z  INFO request{method=POST uri=/shorten version=HTTP/1.1}:shorten{url=https://crates.io}: url_shortener::routes::links: created short link code=rk2Kkav target=https://crates.io
```

Notice how the pretty output makes the *span nesting* visible:
`request{...}:shorten{url=...}:` shows that the `created short link` event
happened inside the `shorten` span, which happened inside the HTTP `request`
span. That nesting is exactly the context that, in Node, you'd have to assemble
manually with a child logger.

### Graceful shutdown

Press `Ctrl-C`, or send `SIGTERM` (`kill -TERM <pid>`). The service logs the
signal, drains in-flight requests, and exits with status 0:

```json
{"timestamp":"2026-06-02T07:11:50.360718Z","level":"INFO","fields":{"message":"received SIGTERM, shutting down"},"target":"url_shortener::shutdown"}
{"timestamp":"2026-06-02T07:11:50.360841Z","level":"INFO","fields":{"message":"shutdown complete"},"target":"url_shortener"}
```

### Configuration via environment

Because config is layered, you can retune without recompiling:

```bash
PORT=9000 CODE_LENGTH=4 REQUEST_TIMEOUT_SECS=30 cargo run
```

### Tests

The suite has three unit tests (in `store.rs`) and four end-to-end HTTP tests (in
`tests/api.rs`) that boot the real router on an ephemeral port and drive it with
`reqwest`, the Rust equivalent of a `supertest` suite. Real output:

```text
running 3 tests
test store::tests::insert_then_resolve_roundtrips ... ok
test store::tests::generated_codes_have_requested_length ... ok
test store::tests::missing_code_resolves_to_none ... ok
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out

running 4 tests
test health_endpoint_reports_ok ... ok
test rejects_invalid_url ... ok
test unknown_code_is_404 ... ok
test shorten_then_redirect ... ok
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Run them with `cargo test`. See [Section 13: Testing](/13-testing/)
for the full testing story, including the `#[tokio::test]` attribute used for
async tests.

## Key Concepts

This project cements the production-Rust ideas that distinguish a deployable
service from a demo:

- **The newtype-config pattern.** Parse the stringly-typed environment into one
  typed `Settings` struct *at the edge*, so the rest of the code works with real
  `SocketAddr`s, `Duration`s, and `enum`s. ([Section 28: Configuration](/28-production/00-configuration/))
- **A single typed error implementing `IntoResponse`.** The exhaustive `match`
  on error variants means the compiler forces you to assign a status code to
  every failure mode: there is no silent catch-all 500.
  ([Section 08: Error Handling](/08-error-handling/))
- **Trait-based dependency inversion.** The `Store` trait lets the in-memory map
  stand in for Redis with zero changes to the handlers.
  ([Section 09: Generics & Traits](/09-generics-traits/))
- **Shared state with `Arc`, `RwLock`, and `AtomicU64`.** tokio runs handlers
  across threads, so the compiler *requires* thread-safe sharing: a class of
  data races that simply cannot compile.
  ([Section 10: Smart Pointers](/10-smart-pointers/))
- **Structured tracing with spans.** `#[tracing::instrument]` propagates request
  context automatically, replacing manual `requestId` threading.
  ([Section 28: Distributed Tracing](/28-production/05-distributed-tracing/))
- **Graceful shutdown via `tokio::select!`.** Race the shutdown signal against
  the server so a `SIGTERM` drains cleanly instead of dropping connections.
  ([Section 28: Graceful Shutdown](/28-production/02-graceful-shutdown/))
- **Library-plus-binary crate layout.** A thin `main.rs` over a testable
  `lib.rs` is what lets integration tests boot the real app.
  ([Section 12: Modules & Packages](/12-modules-packages/))

## Extending It

Concrete next steps, roughly in order of value:

1. **Swap the in-memory store for Redis.** Add the
   [`redis`](https://docs.rs/redis) crate, make the `Store` trait methods
   `async`, and implement them over a connection pool
   (`SET code target` / `GET code`, with `INCR` for the hit counter). The
   handlers change only by adding `.await`. Follow
   [Section 17: Redis](/17-database/07-redis/), and for a relational backend see
   [Section 17: Database](/17-database/) and
   [connection pooling](/17-database/08-connection-pooling/).
2. **Add rate limiting.** Stack a `tower` rate-limit layer (or
   [`tower_governor`](https://docs.rs/tower_governor)) in `build_router` so a
   single client can't flood `POST /shorten`.
   ([Section 28: Rate Limiting](/28-production/06-rate-limiting/))
3. **Emit Prometheus metrics.** Add a `/metrics` endpoint with the
   [`metrics`](https://docs.rs/metrics) + `metrics-exporter-prometheus` crates to
   expose request counts and latencies.
   ([Section 28: Metrics](/28-production/04-metrics/))
4. **Guarantee unique codes and add an API key.** Re-roll `generate_code` on a
   collision (check the store before inserting), and require an `Authorization`
   header on `POST /shorten` via an axum extractor or middleware layer.
   ([Section 27: Security](/27-security/))

## Further Reading

- [Project 1: REST API](/30-projects/00-rest-api/) — the simpler Express-to-axum starting point.
- [Section 11: Async](/11-async/) — futures, `tokio`, and `async`/`await`.
- [Section 16: Web APIs](/16-web-apis/) — axum routing and extractors.
- [Section 28: Production](/28-production/) — the full production checklist.
- [Section 17: Database](/17-database/) — swapping in a real store.
- [axum documentation](https://docs.rs/axum/0.8) and
  [examples](https://github.com/tokio-rs/axum/tree/main/examples).
- [The `tracing` documentation](https://docs.rs/tracing) and
  [`tracing-subscriber`](https://docs.rs/tracing-subscriber).
- [The Tokio tutorial](https://tokio.rs/tokio/tutorial), especially the
  graceful-shutdown chapter.
```
