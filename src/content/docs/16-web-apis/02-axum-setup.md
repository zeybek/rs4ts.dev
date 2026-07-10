---
title: "Setting Up an Axum Project"
description: "Scaffold an Axum project from scratch: the axum plus tokio dependencies, Cargo features, module layout, and a hello-server you can run and curl, vs Express setup."
---

Before you can write a single route, you need a project that compiles. This page is the "create-react-app moment" for Rust web APIs: the exact dependencies, features, project layout, and a hello-server you can run and `curl`.

---

## Quick Overview

[Axum](https://docs.rs/axum) is a web framework built on top of the [Tokio](https://tokio.rs) async runtime and the [Tower](https://docs.rs/tower) middleware ecosystem. Unlike Express, where `npm install express` gives you a server you can start immediately, an Axum app has **two** mandatory pieces: the framework (`axum`) and an async runtime (`tokio`). This page shows you how to wire them together, which Cargo features to turn on, how to lay out a real project, and how to get a server responding on `localhost:3000`.

The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the **2024 edition**, which `cargo new` selects automatically. We target **axum 0.8** and **tokio 1.x** throughout.

> **Note:** This page covers project *setup*. The mechanics of routing, handlers, and `axum::serve` are covered in [Axum Fundamentals](/16-web-apis/01-axum-basics/); deeper routing in [Routing in Axum](/16-web-apis/03-routing/). For *why* you might choose Axum over Actix Web or Rocket, see [Choosing a Rust Web Framework](/16-web-apis/00-framework-comparison/).

---

## TypeScript/JavaScript Example

In Node, scaffolding an Express server is a two-command affair, and the runtime (Node/V8) is already installed:

```bash
# Create the project
mkdir my-api && cd my-api
npm init -y

# One dependency gets you a working HTTP server
npm install express
npm install --save-dev typescript @types/express @types/node tsx
```

A minimal `package.json` and a typical layout:

```json
{
  "name": "my-api",
  "version": "1.0.0",
  "type": "module",
  "scripts": {
    "dev": "tsx watch src/index.ts",
    "start": "node dist/index.js",
    "build": "tsc"
  },
  "dependencies": {
    "express": "^5.1.0"
  },
  "devDependencies": {
    "typescript": "^5.6.0",
    "@types/express": "^5.0.0",
    "@types/node": "^22.0.0",
    "tsx": "^4.19.0"
  }
}
```

```text
my-api/
├── package.json
├── tsconfig.json
└── src/
    ├── index.ts        # entry point: creates the app, listens
    ├── routes.ts       # route definitions
    └── handlers.ts     # request handlers
```

```typescript
// src/index.ts
import express from "express";

const app = express();

app.get("/", (_req, res) => {
  res.send("Hello, world!");
});

const port = 3000;
app.listen(port, () => {
  console.log(`listening on http://localhost:${port}`);
});
```

```bash
npm run dev
# listening on http://localhost:3000
```

Two things to notice for the comparison ahead: the **runtime is implicit** (Node ships an event loop), and **one dependency** (`express`) is enough. Rust differs on both counts.

---

## Rust Equivalent

First, scaffold the project and add dependencies:

```bash
cargo new my-api
cd my-api
cargo add axum
cargo add tokio --features full
```

> **Note:** `cargo add` has been built into Cargo since 1.62. There is no separate `cargo-edit` tool to install. It edits `Cargo.toml` for you and resolves the newest compatible version.

The resulting `Cargo.toml` looks like this (versions are what resolved at the time of writing; yours may be newer):

```toml
[package]
name = "my-api"
version = "0.1.0"
edition = "2024"

[dependencies]
axum = "0.8.9"
tokio = { version = "1.52.3", features = ["full"] }
```

Now the entry point:

```rust
// src/main.rs
use axum::{routing::get, Router};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    // Build the application with a single route.
    let app = Router::new().route("/", get(hello));

    // Bind a TCP listener to a local address.
    let listener = TcpListener::bind("127.0.0.1:3000").await.unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());

    // Run the server. This future never resolves until the process is stopped.
    axum::serve(listener, app).await.unwrap();
}

// A handler is just an async function returning something that implements `IntoResponse`.
async fn hello() -> &'static str {
    "Hello, world!"
}
```

Run it:

```bash
cargo run
```

Real output:

```text
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.16s
     Running `target/debug/my-api`
listening on http://127.0.0.1:3000
```

In another terminal, `curl` it. This is the real response, headers included:

```bash
curl -i http://127.0.0.1:3000/
```

```text
HTTP/1.1 200 OK
content-type: text/plain; charset=utf-8
content-length: 13
date: Mon, 01 Jun 2026 11:44:03 GMT

Hello, world!
```

> **Tip:** `cargo run` compiles *and* runs in one step, so it is the closest analogue to `npm run dev`. For an auto-reloading dev loop like `tsx watch`, install `cargo-watch` (`cargo install cargo-watch`) and run `cargo watch -x run`.

---

## Detailed Explanation

Let's walk the differences a TypeScript developer will trip over.

### Why two dependencies, not one?

In Node, the event loop is part of the runtime; it is always there. In Rust, **the standard library does not ship an async runtime.** `async`/`await` is built into the language, but the thing that actually *drives* those futures to completion (the executor, the I/O reactor, the timer wheel) is a library you choose. Tokio is the de-facto standard.

So the split is:

- **`axum`** — the web framework: routers, extractors, responses.
- **`tokio`** — the async runtime: it polls futures, manages the thread pool, and provides the async `TcpListener`.

This is the opposite of JavaScript, where Promises are **eager** (they start running the moment they are created) and the runtime is baked in. Rust futures are **lazy**: nothing happens until a runtime polls them. `axum::serve(...)` returns a future, and it is the `.await`, driven by the Tokio runtime, that actually runs the server. See [Section 11: Async](/11-async/) for the full mental model.

### `#[tokio::main]`

```rust
#[tokio::main]
async fn main() {
    // ...
}
```

`main` cannot itself be `async`: the operating system calls a plain, synchronous entry point. The `#[tokio::main]` attribute macro rewrites your `async fn main` into a regular `fn main` that starts the Tokio runtime and blocks on your async body. Conceptually it expands to roughly:

```rust
fn main() {
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async {
            // ... your code ...
        });
}
```

This is *not* a decorator in the JavaScript/Python sense: macros operate on the syntax tree at compile time and generate new code. (See [Section 14: Macros](/14-macros/).)

### `tokio::net::TcpListener` and `axum::serve`

```rust
let listener = TcpListener::bind("127.0.0.1:3000").await.unwrap();
axum::serve(listener, app).await.unwrap();
```

In axum 0.8 you bind a Tokio `TcpListener` yourself and hand it to `axum::serve(listener, app)`. This is deliberately explicit: *you* own the socket, which makes it trivial to bind to `0.0.0.0` in production, pick a port from the environment, or pass a pre-bound socket from systemd.

> **Warning:** Older tutorials show `axum::Server::bind(&addr).serve(app.into_make_service())`. That builder was **removed** in axum 0.7 and does not exist in 0.8. `axum::Server` no longer exists at all. Always use `axum::serve(TcpListener, app)`. See [Common Pitfalls](#common-pitfalls).

The two `.unwrap()` calls turn a `Result` into a panic on error (e.g. the port is already taken). That is fine for a setup example; production code should handle these. See [Error Handling in Web Handlers](/16-web-apis/10-error-handling-web/).

### `&'static str` as a response

```rust
async fn hello() -> &'static str {
    "Hello, world!"
}
```

A handler returns any type implementing axum's `IntoResponse` trait. `&'static str` implements it, producing a `200 OK` with `content-type: text/plain; charset=utf-8`, which is exactly what the `curl` headers above show. Returning JSON, status codes, or custom types is covered in [Request and Response Handling](/16-web-apis/07-request-response/) and [JSON REST APIs](/16-web-apis/08-json-apis/).

---

## Key Differences

| Concern | Express (Node) | Axum (Rust) |
| --- | --- | --- |
| Runtime | Built into Node | Separate crate (`tokio`), you pick it |
| Dependencies to start | `express` only | `axum` **and** `tokio` |
| Entry point | `app.listen(port, cb)` | `axum::serve(TcpListener, app).await` |
| Async model | Eager Promises, implicit loop | Lazy futures, explicit runtime via `#[tokio::main]` |
| Socket ownership | Hidden by `app.listen` | Explicit `TcpListener::bind(...)` |
| Reload-on-save | `tsx watch` / `nodemon` (built into workflow) | `cargo watch -x run` (extra tool) |
| Type checking | Optional layer (TypeScript) | Always on (the compiler) |
| Production artifact | `node dist/index.js` + `node_modules/` | A single self-contained binary |

The headline difference is the **explicit runtime**. Once you internalize "futures are lazy and a runtime polls them," the rest of Axum's design (extractors, layers, `IntoResponse`) feels natural rather than magical.

---

## Choosing Tokio features

`tokio = { version = "1", features = ["full"] }` is the simplest choice and the right default while you are learning. `"full"` turns on every feature (the multi-threaded runtime, networking, timers, the `macros` for `#[tokio::main]`, and more).

For production you may want to trim it down. The pieces an Axum server actually needs are:

```toml
[dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "net", "macros"] }
```

- `rt-multi-thread` — the work-stealing multi-threaded scheduler (the default flavor of `#[tokio::main]`).
- `net` — async TCP, used by `TcpListener`.
- `macros` — provides `#[tokio::main]` and `#[tokio::test]`.

> **Tip:** While learning, just use `features = ["full"]`. Optimizing the feature set is a compile-time and binary-size concern you can revisit later; it does not change runtime behavior.

### What `cargo add axum` enables

`cargo add axum` prints the feature flags it turned on. Here is the real output (trimmed to the feature list); a `+` means enabled by default, a `-` means available but off:

```text
      Adding axum v0.8.9 to dependencies
             Features:
             + form
             + http1
             + json
             + matched-path
             + original-uri
             + query
             + tokio
             + tower-log
             + tracing
             - http2
             - macros
             - multipart
             - ws
```

The defaults already include JSON, query strings, and form parsing. Three optional features matter for later pages in this section:

- `multipart` — required for file uploads (see [File Uploads](/16-web-apis/17-file-uploads/)). Enable with `cargo add axum --features multipart`.
- `ws` — required for WebSockets (see [WebSockets with Axum](/16-web-apis/15-websockets/)). Enable with `cargo add axum --features ws`.
- `http2` — HTTP/2 support, off by default.

---

## Recommended project layout

A one-file `main.rs` is fine for a toy. Real APIs split routing, handlers, and state into modules. Here is a layout that mirrors the Express `src/index.ts` + `src/routes.ts` + `src/handlers.ts` split and scales cleanly:

```text
my-api/
├── Cargo.toml
└── src/
    ├── main.rs        # entry point: logging, build router, serve
    ├── routes.rs      # assemble the Router
    └── handlers.rs    # the async fn handlers
```

> **Note:** In Rust, modules are declared, not auto-discovered. `mod routes;` in `main.rs` tells the compiler to load `src/routes.rs`. There is no equivalent of Node's "any file under `src/` is importable." See [Section 12: Modules & Packages](/12-modules-packages/).

**`src/main.rs`** — wires everything together and starts the server:

```rust
mod handlers;
mod routes;

use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    // Initialize structured logging. RUST_LOG controls the level (e.g. RUST_LOG=info).
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let app = routes::app();

    let listener = TcpListener::bind("127.0.0.1:3000").await.unwrap();
    tracing::info!("listening on http://{}", listener.local_addr().unwrap());

    axum::serve(listener, app).await.unwrap();
}
```

**`src/routes.rs`** — the single source of truth for the API surface:

```rust
use axum::{routing::get, Router};

use crate::handlers;

/// Assemble the full application router. Keeping this in one place makes the
/// API surface easy to read and unit-test.
pub fn app() -> Router {
    Router::new()
        .route("/", get(handlers::root))
        .route("/health", get(handlers::health))
}
```

**`src/handlers.rs`** — the request handlers:

```rust
use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct Health {
    status: &'static str,
    version: &'static str,
}

pub async fn root() -> &'static str {
    "Hello, world!"
}

pub async fn health() -> Json<Health> {
    Json(Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}
```

This layout needs four crates. Add them with:

```bash
cargo add axum
cargo add tokio --features full
cargo add serde --features derive
cargo add serde_json
cargo add tracing
cargo add tracing-subscriber --features env-filter
```

`tracing` + `tracing-subscriber` give you structured logging (the Rust analogue of `pino` or `winston`); `serde` powers the JSON response (see [Section 15: Serialization](/15-serialization/)). Running it:

```bash
RUST_LOG=info cargo run
```

Real log output (the tracing-subscriber `fmt` formatter; it adds terminal colors when attached to a TTY):

```text
2026-06-01T11:49:32.033250Z  INFO hello_server: listening on http://127.0.0.1:3000
```

And the `/health` endpoint returns real JSON:

```bash
curl -i http://127.0.0.1:3000/health
```

```text
HTTP/1.1 200 OK
content-type: application/json
content-length: 33
date: Mon, 01 Jun 2026 11:45:03 GMT

{"status":"ok","version":"0.1.0"}
```

> **Tip:** `env!("CARGO_PKG_VERSION")` reads the `version` from `Cargo.toml` at compile time, so `/health` always reports the right build version with zero maintenance.

---

## Common Pitfalls

### Pitfall 1: Forgetting `#[tokio::main]`

A plain `async fn main` won't even compile, and a plain `fn main` can't use `.await`. If you write `fn main` and call `.await` inside, you get a real error:

```rust
use axum::{routing::get, Router};
use tokio::net::TcpListener;

// does not compile (error[E0728]): missing #[tokio::main], so main is not async
fn main() {
    let app = Router::new().route("/", get(|| async { "hi" }));
    let listener = TcpListener::bind("127.0.0.1:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

The compiler says exactly what's wrong:

```text
error[E0728]: `await` is only allowed inside `async` functions and blocks
 --> src/main.rs:7:56
  |
5 | fn main() {
  | --------- this is not `async`
6 |     let app = Router::new().route("/", get(|| async { "hi" }));
7 |     let listener = TcpListener::bind("127.0.0.1:3000").await.unwrap();
  |                                                        ^^^^^ only allowed inside `async` functions and blocks
```

**Fix:** add `#[tokio::main]` above `async fn main`.

### Pitfall 2: Adding `tokio` without the runtime features

A very common surprise: `cargo add tokio` (with no `--features`) adds Tokio with *no* features enabled, so `#[tokio::main]` has no runtime to start:

```toml
[dependencies]
tokio = "1.52.3"   # no features — the macro can't find a runtime
```

```text
error: The default runtime flavor is `multi_thread`, but the `rt-multi-thread` feature is disabled.
 --> src/main.rs:4:1
  |
4 | #[tokio::main]
  | ^^^^^^^^^^^^^^
```

**Fix:** add the features: `cargo add tokio --features full` (simplest) or at minimum `["rt-multi-thread", "net", "macros"]`.

### Pitfall 3: Using the removed `axum::Server` API

The single most common copy-paste failure from old tutorials:

```rust
use axum::{routing::get, Router};

#[tokio::main]
async fn main() {
    let app = Router::new().route("/", get(|| async { "hi" }));
    // does not compile (error[E0433]): axum::Server was removed in 0.7
    axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}
```

```text
error[E0433]: failed to resolve: could not find `Server` in `axum`
 --> src/main.rs:7:11
  |
7 |     axum::Server::bind(&"0.0.0.0:3000".parse().unwrap())
  |           ^^^^^^ could not find `Server` in `axum`
```

**Fix:** bind a Tokio listener and call `axum::serve`:

```rust
let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
axum::serve(listener, app).await.unwrap();
```

### Pitfall 4: Expecting modules to be auto-discovered

Creating `src/handlers.rs` does **not** make it part of the crate. You must declare `mod handlers;` in `main.rs` (or another module in the tree). A forgotten `mod` produces an "unresolved import" / "failed to resolve" error, not silent omission. This trips up developers used to Node's filesystem-based importing.

### Pitfall 5: Reaching for `:id`-style routes

If you start adding parameterized routes, note that axum 0.8 uses `{id}`, **not** the Express-style `:id`. Writing `:id` is a 0.7-era habit that behaves differently in 0.8. This is covered in [Routing in Axum](/16-web-apis/03-routing/).

---

## Best Practices

- **Use `features = ["full"]` for tokio while learning;** trim to `["rt-multi-thread", "net", "macros"]` once the app is real and you care about compile time.
- **Keep `main` thin.** Logging setup, router assembly, and serving: that's it. Push routes into a `routes` module and handlers into a `handlers` module from day one; it costs nothing and scales.
- **Expose the router from a function** (`pub fn app() -> Router`). This makes the whole app testable without binding a port; see the test below.
- **Add `tracing` early.** Structured logs from the first commit beat retrofitting `println!` later. Request-level logging via `TraceLayer` lives in [Middleware and Layers](/16-web-apis/05-middleware/).
- **Bind `127.0.0.1` in development, `0.0.0.0` in production**, and read the address/port from the environment for deployment. See [Deploying Axum Applications](/16-web-apis/19-deployment/).
- **Wire in graceful shutdown** before you ship, so in-flight requests finish on deploy (shown below).
- **Commit `Cargo.lock`** for binaries (an API is a binary). It pins exact dependency versions for reproducible builds: the equivalent of committing `package-lock.json`.

### Testing the router without a socket

Because `app()` returns a `Router`, you can drive it directly in tests using Tower's `oneshot`, with no network involved:

```bash
cargo add --dev tower
cargo add --dev http-body-util
```

```rust
// tests/health.rs
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt; // brings `oneshot` into scope

#[tokio::test]
async fn health_returns_ok_json() {
    use axum::{routing::get, Router};
    let app = Router::new().route(
        "/health",
        get(|| async { axum::Json(serde_json::json!({ "status": "ok" })) }),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], br#"{"status":"ok"}"#);
}
```

Real test output:

```text
running 1 test
test health_returns_ok_json ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

This is far faster and more reliable than booting a real server and `curl`-ing it: the analogue of `supertest` against an Express `app`, but built into the type system. More on testing in [Section 13: Testing](/13-testing/).

---

## Real-World Example

A production-flavored entry point: structured logging, configuration from the environment, binding `0.0.0.0`, and graceful shutdown so a deploy doesn't drop in-flight requests.

```rust
// src/main.rs
use std::net::SocketAddr;

use axum::{routing::get, Json, Router};
use serde::Serialize;
use tokio::net::TcpListener;
use tokio::signal;

#[derive(Serialize)]
struct Health {
    status: &'static str,
    version: &'static str,
}

async fn root() -> &'static str {
    "Hello, world!"
}

async fn health() -> Json<Health> {
    Json(Health {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

fn app() -> Router {
    Router::new()
        .route("/", get(root))
        .route("/health", get(health))
}

#[tokio::main]
async fn main() {
    // Structured logging; RUST_LOG (e.g. "info") controls verbosity.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    // Read the bind address from the environment, defaulting to all interfaces.
    let addr: SocketAddr = std::env::var("BIND_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:3000".to_string())
        .parse()
        .expect("BIND_ADDR must be a valid socket address");

    let listener = TcpListener::bind(addr).await.unwrap();
    tracing::info!("listening on http://{}", listener.local_addr().unwrap());

    axum::serve(listener, app())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

/// Resolves when the user presses Ctrl-C, letting in-flight requests finish.
async fn shutdown_signal() {
    signal::ctrl_c()
        .await
        .expect("failed to install Ctrl-C handler");
    tracing::info!("shutdown signal received, draining connections");
}
```

Dependencies for this example:

```bash
cargo add axum
cargo add tokio --features full
cargo add serde --features derive
cargo add tracing
cargo add tracing-subscriber --features env-filter
```

Three production details worth calling out:

- **`with_graceful_shutdown`** stops accepting new connections on Ctrl-C (or `SIGTERM` from an orchestrator if you extend `shutdown_signal`) and waits for active requests to complete before the process exits.
- **`BIND_ADDR` from the environment:** twelve-factor config. Default to `0.0.0.0:3000` so it works in a container without extra setup, but allow override.
- **No `panic`-on-startup behavior is hidden.** `bind` and `serve` failures still surface; for handler-level errors, use a typed error response as in [Error Handling in Web Handlers](/16-web-apis/10-error-handling-web/).

> **Note:** Choosing the runtime flavor is a one-liner. The default is multi-threaded; for a low-concurrency sidecar you can switch to a single-threaded runtime with `#[tokio::main(flavor = "current_thread")]`. Both compile against the same Axum code.

---

## Further Reading

### Official Documentation

- [Axum documentation (docs.rs)](https://docs.rs/axum): the API reference; check `axum::serve` and `Router`.
- [Axum GitHub examples](https://github.com/tokio-rs/axum/tree/main/examples): `hello-world`, `graceful-shutdown`, and more, kept current with the latest release.
- [Tokio documentation (docs.rs)](https://docs.rs/tokio): runtime, `TcpListener`, and the `#[tokio::main]` macro.
- [The Tokio Tutorial](https://tokio.rs/tokio/tutorial): the canonical introduction to the async runtime.
- [tracing-subscriber docs](https://docs.rs/tracing-subscriber): configuring logging output and filters.

### Related Sections

- [Section 16 README](/16-web-apis/): the full Web APIs section index.
- [Choosing a Rust Web Framework](/16-web-apis/00-framework-comparison/) — Axum vs Actix Web vs Rocket vs Express/Nest.
- [Axum Fundamentals](/16-web-apis/01-axum-basics/) — `Router`, handlers, and `axum::serve` fundamentals.
- [Routing in Axum](/16-web-apis/03-routing/) — path params (`{id}`), query params, nested routers.
- [Shared Application State in Axum](/16-web-apis/06-state-management/) — sharing a DB pool or config with `State<T>`.
- [Deploying Axum Applications](/16-web-apis/19-deployment/) — release builds, Docker multi-stage, reverse proxies.
- [Section 11: Async](/11-async/) — futures, `async`/`await`, and why Rust futures are lazy.
- [Section 12: Modules & Packages](/12-modules-packages/) — `mod`, crates, and project structure.
- [Section 15: Serialization](/15-serialization/) — serde and JSON, used by the `/health` handler.
- [Section 17: Database](/17-database/): adding a real data store behind your API.

---

## Exercises

### Exercise 1: Scaffold and run

**Difficulty:** Beginner

**Objective:** Build the hello-server from scratch and confirm it responds.

**Instructions:**

1. Run `cargo new greeter` and `cd` into it.
2. Add `axum` and `tokio` (with the `full` feature).
3. Write a server that responds with `"Hello from Rust!"` on `GET /`.
4. Run it with `cargo run` and verify with `curl http://127.0.0.1:3000/`.

<details>
<summary>Solution</summary>

```bash
cargo new greeter
cd greeter
cargo add axum
cargo add tokio --features full
```

```rust
// src/main.rs
use axum::{routing::get, Router};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    let app = Router::new().route("/", get(hello));

    let listener = TcpListener::bind("127.0.0.1:3000").await.unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());

    axum::serve(listener, app).await.unwrap();
}

async fn hello() -> &'static str {
    "Hello from Rust!"
}
```

```bash
cargo run
# in another terminal:
curl http://127.0.0.1:3000/
# Hello from Rust!
```

</details>

### Exercise 2: Split into modules and add `/health`

**Difficulty:** Intermediate

**Objective:** Refactor the single-file server into the recommended `main.rs` / `routes.rs` / `handlers.rs` layout, and add a `GET /health` endpoint that returns JSON.

**Instructions:**

1. Move the handlers into `src/handlers.rs` and the router into `src/routes.rs`.
2. Declare both modules in `main.rs`.
3. Add a `health` handler returning JSON like `{"status":"ok"}` (add `serde`/`serde_json` and use the `Json` type).
4. Verify both routes with `curl`.

<details>
<summary>Solution</summary>

```bash
cargo add axum
cargo add tokio --features full
cargo add serde --features derive
cargo add serde_json
```

```rust
// src/main.rs
mod handlers;
mod routes;

use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    let app = routes::app();
    let listener = TcpListener::bind("127.0.0.1:3000").await.unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
```

```rust
// src/routes.rs
use axum::{routing::get, Router};

use crate::handlers;

pub fn app() -> Router {
    Router::new()
        .route("/", get(handlers::root))
        .route("/health", get(handlers::health))
}
```

```rust
// src/handlers.rs
use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct Health {
    status: &'static str,
}

pub async fn root() -> &'static str {
    "Hello, world!"
}

pub async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}
```

```bash
cargo run
curl http://127.0.0.1:3000/          # Hello, world!
curl http://127.0.0.1:3000/health    # {"status":"ok"}
```

</details>

### Exercise 3: Graceful shutdown and env-configured binding

**Difficulty:** Advanced

**Objective:** Make the server production-ready: bind from an environment variable and shut down cleanly on Ctrl-C.

**Instructions:**

1. Read the bind address from a `BIND_ADDR` environment variable, defaulting to `0.0.0.0:3000`.
2. Add graceful shutdown using `axum::serve(...).with_graceful_shutdown(...)` and `tokio::signal::ctrl_c`.
3. Run with `BIND_ADDR=127.0.0.1:8080 cargo run`, confirm it binds there, then press Ctrl-C and confirm it logs a shutdown message before exiting.

<details>
<summary>Solution</summary>

```bash
cargo add axum
cargo add tokio --features full
```

```rust
// src/main.rs
use std::net::SocketAddr;

use axum::{routing::get, Router};
use tokio::net::TcpListener;
use tokio::signal;

#[tokio::main]
async fn main() {
    let app = Router::new().route("/", get(|| async { "Hello, world!" }));

    let addr: SocketAddr = std::env::var("BIND_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:3000".to_string())
        .parse()
        .expect("BIND_ADDR must be a valid socket address");

    let listener = TcpListener::bind(addr).await.unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

async fn shutdown_signal() {
    signal::ctrl_c()
        .await
        .expect("failed to install Ctrl-C handler");
    println!("shutting down gracefully");
}
```

```bash
BIND_ADDR=127.0.0.1:8080 cargo run
# listening on http://127.0.0.1:8080
# ... press Ctrl-C ...
# shutting down gracefully
```

</details>
