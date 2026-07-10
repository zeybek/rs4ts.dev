---
title: "Deploying Axum Applications"
description: "Deploy Axum apps as one native binary. Swap node dist/index.js for cargo build --release, a slim multi-stage Docker image, 0.0.0.0 binding, and graceful shutdown."
---

## Quick Overview

Deploying a Rust web service is, in most respects, *easier* than deploying a Node app: `cargo build --release` produces a single, self-contained, statically-ish linked native binary. There is no `node_modules` to ship, no separate runtime to install on the server, and no transpile step at deploy time. This page shows how a TypeScript/JavaScript developer goes from `npm run build && node dist/index.js` to a Rust release build, a slim multi-stage Docker image, the handful of operational habits Rust requires (binding `0.0.0.0`, reading config from the environment, graceful shutdown), and where Rust deployment genuinely differs from Node deployment.

> **Note:** This page targets the **axum 0.8** API line (recorded with 0.8.9). The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the **2024 edition**; `cargo new` selects that edition automatically. Servers are started with `axum::serve(listener, app)` over a `tokio::net::TcpListener`, never the removed `Server::bind().serve()` builder from older axum.

---

## TypeScript/JavaScript Example

A typical production Express service ships a transpiled `dist/`, reads config from `process.env`, binds `0.0.0.0` so it is reachable inside a container, and exits cleanly on `SIGTERM`. Here is the kind of `index.ts` and `Dockerfile` that pair you would deploy:

```typescript
// src/index.ts — Express 5, production-shaped
import express from "express";

const app = express();
app.use(express.json());

app.get("/healthz", (_req, res) => {
  res.json({ status: "ok" });
});

// Read config from the environment, with sane local defaults.
const port = Number(process.env.PORT ?? 8080);
// Bind 0.0.0.0 (all interfaces) so the socket is reachable from outside a container.
const host = process.env.HOST ?? "0.0.0.0";

const server = app.listen(port, host, () => {
  console.log(`listening on http://${host}:${port}`);
});

// Orchestrators (Kubernetes, `docker stop`) send SIGTERM to ask for shutdown.
process.on("SIGTERM", () => {
  server.close(() => process.exit(0));
});
```

```dockerfile
# Dockerfile — a typical Node multi-stage build
FROM node:22-slim AS builder
WORKDIR /app
COPY package*.json ./
RUN npm ci
COPY . .
RUN npm run build           # tsc -> dist/

FROM node:22-slim AS runtime
WORKDIR /app
ENV NODE_ENV=production
COPY package*.json ./
RUN npm ci --omit=dev       # prod deps only, but node_modules still ships
COPY --from=builder /app/dist ./dist
EXPOSE 8080
CMD ["node", "dist/index.js"]
```

The runtime image still contains Node itself plus a production `node_modules` tree, commonly **150–400 MB**. The deploy artifact is "interpreter + your JavaScript + its dependency tree."

---

## Rust Equivalent

The deploy artifact is one file: the compiled binary. First, the production-shaped server: config from the environment, `0.0.0.0` binding, structured logs, a per-request timeout, and graceful shutdown:

```bash
cargo add axum
cargo add tokio --features full
cargo add serde --features derive
cargo add tower-http --features "trace timeout"
cargo add tracing
cargo add tracing-subscriber --features env-filter
```

```rust
// src/main.rs
use std::{net::SocketAddr, time::Duration};

use axum::{
    extract::State,
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Serialize;
use tokio::signal;
use tower_http::{timeout::TimeoutLayer, trace::TraceLayer};

/// Runtime configuration, loaded once from the environment at startup.
#[derive(Clone, Debug)]
struct Config {
    /// Address to bind, e.g. "0.0.0.0:8080".
    bind_addr: SocketAddr,
    database_url: String,
}

impl Config {
    fn from_env() -> Result<Self, String> {
        // PORT is the de-facto standard many platforms (Render, Railway,
        // Fly.io, Cloud Run) inject; default to 8080 for local runs.
        let port: u16 = std::env::var("PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse()
            .map_err(|_| "PORT must be a number".to_string())?;

        // Bind 0.0.0.0 in containers so the socket is reachable from outside
        // the container, not just from inside it.
        let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let bind_addr: SocketAddr = format!("{host}:{port}")
            .parse()
            .map_err(|_| "HOST/PORT did not form a valid socket address".to_string())?;

        // Required secrets fail loudly at startup, not on the first request.
        let database_url =
            std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL is required".to_string())?;

        Ok(Config { bind_addr, database_url })
    }
}

#[derive(Clone)]
struct AppState {
    config: Config,
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
}

async fn health() -> Json<Health> {
    Json(Health { status: "ok" })
}

async fn root(State(state): State<AppState>) -> String {
    format!("connected to {}", state.config.database_url)
}

fn app(state: AppState) -> Router {
    Router::new()
        .route("/", get(root))
        .route("/healthz", get(health))
        // Per-request timeout so a slow handler cannot pin a connection forever.
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(15),
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

/// Resolve when the process receives Ctrl-C or (on Unix) SIGTERM — the signal
/// orchestrators like Kubernetes and `docker stop` send to ask for shutdown.
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to install Ctrl-C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received, draining connections");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Structured logs to stdout; the platform collects them. RUST_LOG controls
    // verbosity, e.g. RUST_LOG=info,tower_http=debug.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=debug".into()),
        )
        .init();

    let config = Config::from_env().map_err(|e| {
        tracing::error!("configuration error: {e}");
        e
    })?;

    let state = AppState { config: config.clone() };
    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    tracing::info!("listening on http://{}", listener.local_addr()?);

    axum::serve(listener, app(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}
```

Build it for production and run it with real environment variables:

```bash
cargo build --release
PORT=8080 DATABASE_URL="postgres://localhost/app" \
  RUST_LOG=info,tower_http=debug \
  ./target/release/myapi
```

Real startup log and responses (captured from running the binary above and `curl`ing it):

```text
2026-06-01T12:28:24.340167Z  INFO myapi: listening on http://0.0.0.0:8080
2026-06-01T12:28:24.979435Z DEBUG request{method=GET uri=/healthz version=HTTP/1.1}: tower_http::trace::on_request: started processing request
2026-06-01T12:28:24.979550Z DEBUG request{method=GET uri=/healthz version=HTTP/1.1}: tower_http::trace::on_response: finished processing request latency=0 ms status=200
```

```console
$ curl -s http://127.0.0.1:8080/healthz
{"status":"ok"}
$ curl -s -i http://127.0.0.1:8080/healthz | head -4
HTTP/1.1 200 OK
content-type: application/json
content-length: 15
date: Mon, 01 Jun 2026 12:28:25 GMT
```

And when a required secret is missing, the process fails *at startup* (exit code 1) instead of crashing on the first request:

```console
$ PORT=8080 ./target/release/myapi
2026-06-01T12:28:36.290088Z ERROR myapi: configuration error: DATABASE_URL is required
Error: "DATABASE_URL is required"
$ echo $?
1
```

---

## Detailed Explanation

**`cargo build --release` is the deploy build.** Without `--release`, `cargo build` produces an unoptimized **debug** binary that can be an order of magnitude slower; it is for local iteration only. The release binary lands in `target/release/<crate-name>`. This is the single line that replaces Node's `tsc` transpile *and* the `node` runtime: the output is native machine code, not JavaScript that an interpreter still has to parse and JIT at runtime. There is no warm-up: a release binary is at full speed from the first request.

**Config comes from the environment.** `Config::from_env()` mirrors `process.env` access in Node, but with one deliberate difference: a missing required variable (`DATABASE_URL`) returns an `Err` that propagates out of `main` via `?`, so the process exits non-zero **before it ever binds a port**. In Node it is common for a missing `process.env.X` to be `undefined` and only blow up later, deep inside a request handler. Failing fast at startup means a bad deploy is caught immediately by your platform's health check, not by your first user.

**`bind_addr` defaults to `0.0.0.0`.** This is the single most common deployment mistake for newcomers. `127.0.0.1` (loopback) only accepts connections from *inside the same network namespace*, inside the container itself. A container that binds `127.0.0.1` will pass its own internal health check and then reject every connection from the host or the orchestrator. Binding `0.0.0.0` listens on *all* interfaces, which is what containers and PaaS platforms require. (`SocketAddr` is `std`'s parsed `IP:port` type; parsing `"0.0.0.0:8080"` into it validates the address at startup.)

**`PORT` is read from the environment.** Most managed platforms — Render, Railway, Fly.io, Google Cloud Run, Heroku — inject the port your service must listen on via `$PORT` and route external traffic to it. Hardcoding `3000` will fail on those platforms. The default of `8080` is for local runs.

**`TraceLayer` writes structured request logs to stdout.** Production logging belongs on stdout/stderr; the platform (Docker, journald, your log aggregator) is responsible for collecting it. `tracing_subscriber`'s `EnvFilter` reads the `RUST_LOG` variable, the Rust analogue of `DEBUG=express:*`. `RUST_LOG=info,tower_http=debug` shows info-level app logs plus debug-level HTTP traces. See [Middleware and Layers](/16-web-apis/05-middleware/) for the layer mechanics.

**`with_graceful_shutdown` drains in-flight requests.** When the process receives `SIGTERM` (what `docker stop` and Kubernetes send first, before `SIGKILL`), `axum::serve` stops accepting *new* connections but lets in-flight requests finish. This is the direct equivalent of Node's `server.close()` in a `SIGTERM` handler. Without it, the binary would be killed mid-request on every deploy. The `#[cfg(unix)]` block adds SIGTERM on top of Ctrl-C (`SIGINT`); on non-Unix the `terminate` future is `pending()` (never resolves), so only Ctrl-C triggers shutdown there.

**A per-request `TimeoutLayer`** ensures one stuck handler cannot tie up a connection indefinitely. In axum 0.8 / tower-http 0.6 the constructor is `TimeoutLayer::with_status_code(status, duration)`; the older bare `TimeoutLayer::new(duration)` is deprecated.

---

## Key Differences

| Concern | Node / Express | Rust / Axum |
| --- | --- | --- |
| Deploy artifact | Interpreter + your JS + `node_modules` (often 150–400 MB) | One native binary (~1–5 MB), optionally a slim base image |
| Build step | `tsc` transpile at build; V8 JITs at runtime | `cargo build --release` produces optimized machine code; no runtime warm-up |
| Runtime on server | Node must be installed/present | None: the binary is self-contained (with a libc, or fully static with musl) |
| Startup time | Process start + module load | Process start only (no module graph to load) |
| Memory baseline | Tens to hundreds of MB | Typically single-digit to low-tens of MB |
| Missing config | Often `undefined`, fails later in a handler | `?` out of `main`, process exits non-zero at startup |
| Graceful shutdown | `server.close()` in a `SIGTERM` handler | `.with_graceful_shutdown(future)` on `axum::serve` |
| Concurrency model | Single-threaded event loop; scale with cluster/PM2 | Tokio multi-threaded runtime uses all cores in one process |

> **Note:** Because one Axum process already uses all CPU cores via the Tokio work-stealing runtime, you usually do **not** run a process-per-core supervisor like PM2 `cluster` or Node's `cluster` module. One container = one binary = all cores. This is covered conceptually in [the async section](/11-async/).

The deepest difference is the **dependency story**. In Node, dependencies are resolved and present at runtime inside `node_modules`. In Rust, every crate your code uses is *compiled into the binary at build time*; there is nothing to install on the server. The cost is paid once, during `cargo build`, which is why Docker layer caching of dependencies (below) matters so much for CI speed.

---

## Common Pitfalls

### Pitfall 1: Binding `127.0.0.1` inside a container

```rust
// Wrong for containers: only reachable from inside the container itself.
let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;
```

The server starts fine and even passes a self-issued health check, but the orchestrator and the host cannot reach it: every external request is refused. Bind `0.0.0.0` (all interfaces) in any containerized or PaaS deployment:

```rust
// Reachable from outside the container.
let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
```

### Pitfall 2: Shipping (or worse, deploying) the debug binary

Running plain `cargo build` and copying `target/debug/myapi` into your image ships an unoptimized binary. Debug builds skip optimizations and embed extra debug info; they can be many times slower and substantially larger. Always build with `--release` for deployment, and point your `Dockerfile`'s `COPY --from=builder` at `target/release/...`, not `target/debug/...`.

### Pitfall 3: Hardcoding the port

```rust
// Breaks on Render/Railway/Fly.io/Cloud Run, which inject $PORT.
let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
```

Read `PORT` from the environment with a local default, as in the main example. A hardcoded port means the platform routes traffic to a port nothing is listening on.

### Pitfall 4: Forgetting graceful shutdown, then losing requests on every deploy

Without `.with_graceful_shutdown(...)`, the process is terminated immediately on `SIGTERM` and any in-flight request is dropped, visible to users as connection resets during every rolling deploy. Wire up the shutdown future once and the problem disappears.

### Pitfall 5: A `glibc` mismatch between build and runtime images

If you build on a newer Debian/Ubuntu and copy the binary into an older or different base (or a `musl`-based Alpine image without recompiling for musl), the binary may fail to start with a dynamic-linker error such as `version 'GLIBC_2.x' not found` or `no such file or directory` (for the missing loader). Two reliable fixes: build *and* run on the same `glibc` (e.g. `rust:1.96-slim` builder → `gcr.io/distroless/cc-debian12` runtime, both Debian 12), or build a fully static binary against musl (`rustup target add x86_64-unknown-linux-musl` then `cargo build --release --target x86_64-unknown-linux-musl`) so there is no dynamic-linking requirement at all.

---

## Best Practices

### Shrink the release binary with a profile

A default `cargo build --release` of the server above produced a **2.5 MB** binary. Adding a size-tuned `[profile.release]` to `Cargo.toml` brought it down to **968 KB** (measured on the same code, this machine):

```toml
# Cargo.toml
[profile.release]
opt-level = "z"     # optimize for size ("s" is a slightly faster middle ground)
lto = true          # link-time optimization across crate boundaries
codegen-units = 1   # one codegen unit: better optimization, slower compile
strip = true        # strip symbols from the binary
panic = "abort"     # abort on panic; drops unwinding tables (std::panic::catch_unwind can no longer recover)
```

> **Tip:** `opt-level = "z"`/`"s"` optimize for *size*; the default release `opt-level = 3` optimizes for *speed*. For a network service, raw binary size rarely matters as much as throughput, so many teams keep `opt-level = 3` and only add `lto = true`, `codegen-units = 1`, and `strip = true`. Measure before choosing — `panic = "abort"` in particular changes runtime behavior (a panic aborts the process instead of unwinding), which is usually fine and even desirable for a stateless web service, but confirm it suits yours.

### Multi-stage Docker build with dependency caching

The whole point of a multi-stage build is to compile in a fat image with the full Rust toolchain, then copy only the resulting binary into a tiny runtime image. The dependency-caching trick — build a dummy `main.rs` from just the manifests first — means `cargo` only recompiles your dependency graph when `Cargo.toml`/`Cargo.lock` change, not on every source edit:

```dockerfile
# ---- Stage 1: build ----
# Pin the toolchain so CI builds are reproducible.
FROM rust:1.96-slim AS builder
WORKDIR /app

# Cache dependencies: copy only the manifests first, build a dummy main,
# then copy the real sources. The dependency layer only rebuilds when Cargo.* changes.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs \
    && cargo build --release \
    && rm -rf src

COPY src ./src
# `touch` so Cargo sees the real main.rs as newer than the dummy build.
RUN touch src/main.rs && cargo build --release

# ---- Stage 2: runtime ----
# Distroless "cc" image: a glibc + libstdc++ runtime, no shell, no package
# manager, runs as a non-root user — a tiny attack surface.
FROM gcr.io/distroless/cc-debian12 AS runtime
WORKDIR /app
COPY --from=builder /app/target/release/myapi /usr/local/bin/myapi
ENV PORT=8080
EXPOSE 8080
USER nonroot:nonroot
CMD ["myapi"]
```

Add a `.dockerignore` so the local `target/` directory (which can be gigabytes) is never sent to the Docker daemon:

```text
# .dockerignore
target
.git
Dockerfile
.dockerignore
```

Build, run, and verify (real output from building the `myapi` project above with this exact Dockerfile):

```console
$ docker build -t myapi:latest .
...
 => [builder 6/6] RUN touch src/main.rs && cargo build --release
 #13 2.878    Compiling myapi v0.1.0 (/app)
 #13 2.878     Finished `release` profile [optimized] target(s) in 2.04s
 => exporting to image ... done

$ docker images myapi:latest --format '{{.Repository}}:{{.Tag}}  {{.Size}}'
myapi:latest  36.2MB

# The server requires DATABASE_URL, so pass it in; the Dockerfile already sets PORT=8080.
$ docker run -d -e DATABASE_URL=postgres://localhost/app -p 18080:8080 myapi:latest
$ curl -s http://127.0.0.1:18080/healthz
{"status":"ok"}
$ docker logs <container>
2026-06-01T12:30:11.482913Z  INFO myapi: listening on http://0.0.0.0:8080
```

The final image is **36.2 MB**, most of which is the distroless base; the binary itself is around 1–3 MB. Compare that to a typical 150–400 MB Node runtime image. Notice the second `cargo build` finished in **2.04s** because the dependency layer was cached.

> **Note:** The `-e DATABASE_URL=...` flag is required because `Config::from_env()` treats `DATABASE_URL` as a mandatory secret and exits non-zero at startup if it is missing: exactly the fail-fast behavior shown earlier. Without it the container would crash on launch and `curl` would get a connection refused, not `{"status":"ok"}`.

> **Tip:** For even faster CI, replace the manual dummy-`main.rs` trick with [`cargo-chef`](https://github.com/LukeMathWalker/cargo-chef), which computes a recipe of your dependencies and caches them as a dedicated Docker layer. For statically-linked images on `scratch` or Alpine, build against `x86_64-unknown-linux-musl` and copy into `FROM scratch`; the binary then needs no base OS at all.

### Run as non-root and add a health check

The distroless `USER nonroot:nonroot` line above runs the process unprivileged. Expose a cheap `/healthz` route (no database call) for liveness and a separate readiness route if you need to gate traffic on dependencies being up. Most platforms poll an HTTP health endpoint; your `Dockerfile` can also declare one:

```dockerfile
# Optional: container-level health check (note distroless has no shell,
# so use an exec-form check that does not rely on /bin/sh).
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s \
  CMD ["/usr/local/bin/myapi", "--health-check"]
```

> **Note:** Distroless images have no shell, so the common `CMD curl ...` health check (which needs `/bin/sh` and `curl`) will not work there. Either add a tiny `--health-check` subcommand to your binary, switch the runtime base to `debian:bookworm-slim` (which has a shell), or let the orchestrator do the HTTP probe instead of Docker.

### Reverse proxy and TLS termination

In production you usually put a reverse proxy (Nginx, Caddy, Traefik, or your cloud load balancer) *in front of* Axum. The proxy terminates TLS and forwards plain HTTP to your app on `0.0.0.0:8080`. A minimal Nginx server block:

```nginx
# /etc/nginx/conf.d/myapi.conf
server {
    listen 443 ssl;
    server_name api.example.com;

    ssl_certificate     /etc/letsencrypt/live/api.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/api.example.com/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

This is the same pattern you would use in front of Express, and the reasoning is identical: a battle-tested proxy handles TLS, HTTP/2, compression, and rate limiting at the edge while your app speaks plain HTTP behind it.

> **Tip:** When you sit behind a proxy, the client IP arrives in `X-Forwarded-For`, not on the TCP socket. To read the real client IP in a handler, parse that header (via tower-http's `SetSensitiveHeaders`/your own extractor) rather than using `ConnectInfo<SocketAddr>`, which would give you the proxy's address. Only trust forwarded headers from a proxy you control.

Axum *can* terminate TLS itself (e.g. with [`axum-server`](https://docs.rs/axum-server) + `rustls`) when there is no proxy — common on Fly.io or a bare VM — but a fronting proxy or platform load balancer is the more common production shape.

### Keep secrets out of the image

Never `COPY` a `.env` file or bake secrets into a layer: image layers are cacheable and inspectable. Inject secrets at runtime via environment variables (`docker run -e`, Kubernetes `Secret`, your platform's secret store). For local development, the [`dotenvy`](https://docs.rs/dotenvy) crate can load a git-ignored `.env`, but treat that strictly as a dev convenience.

---

## Real-World Example

A deployment-ready binary that ties the pieces together: environment-driven config that fails fast, a database-pool placeholder in shared state, `0.0.0.0`/`$PORT` binding, request tracing, a per-request timeout, a body-size limit, and graceful shutdown. This compiles and runs as shown above.

```bash
cargo add axum
cargo add tokio --features full
cargo add serde --features derive
cargo add tower-http --features "trace timeout limit"
cargo add tracing
cargo add tracing-subscriber --features env-filter
```

```rust
// src/main.rs
use std::{net::SocketAddr, time::Duration};

use axum::{
    extract::State,
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Serialize;
use tokio::signal;
use tower_http::{
    limit::RequestBodyLimitLayer, timeout::TimeoutLayer, trace::TraceLayer,
};

#[derive(Clone, Debug)]
struct Config {
    bind_addr: SocketAddr,
    database_url: String,
    max_body_bytes: usize,
}

impl Config {
    fn from_env() -> Result<Self, String> {
        let port: u16 = std::env::var("PORT")
            .unwrap_or_else(|_| "8080".to_string())
            .parse()
            .map_err(|_| "PORT must be a number".to_string())?;
        let host = std::env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
        let bind_addr: SocketAddr = format!("{host}:{port}")
            .parse()
            .map_err(|_| "HOST/PORT did not form a valid socket address".to_string())?;

        let database_url =
            std::env::var("DATABASE_URL").map_err(|_| "DATABASE_URL is required".to_string())?;

        let max_body_bytes: usize = std::env::var("MAX_BODY_BYTES")
            .unwrap_or_else(|_| "1048576".to_string()) // 1 MiB default
            .parse()
            .map_err(|_| "MAX_BODY_BYTES must be a number".to_string())?;

        Ok(Config { bind_addr, database_url, max_body_bytes })
    }
}

#[derive(Clone)]
struct AppState {
    config: Config,
    // In a real app this would hold a `sqlx::PgPool` or similar; see
    // ../17-database/README.md. We keep a string here so the example is
    // self-contained and compiles without a database crate.
    db: String,
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
}

// Liveness: cheap, no dependencies. Used by orchestrator liveness probes.
async fn healthz() -> Json<Health> {
    Json(Health { status: "ok" })
}

// Readiness: confirm dependencies are reachable before accepting traffic.
async fn readyz(State(state): State<AppState>) -> Result<Json<Health>, StatusCode> {
    if state.db.is_empty() {
        // 503 tells the load balancer "not ready, do not route to me yet".
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    Ok(Json(Health { status: "ready" }))
}

fn app(state: AppState) -> Router {
    let max_body = state.config.max_body_bytes;
    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .layer(RequestBodyLimitLayer::new(max_body))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(15),
        ))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to install Ctrl-C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received, draining connections");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,tower_http=debug".into()),
        )
        .init();

    let config = Config::from_env().map_err(|e| {
        tracing::error!("configuration error: {e}");
        e
    })?;

    // Pretend to open a connection pool from config.database_url here.
    let state = AppState { db: config.database_url.clone(), config: config.clone() };

    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    tracing::info!("listening on http://{}", listener.local_addr()?);

    axum::serve(listener, app(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}
```

This separates **liveness** (`/healthz`: am I running?) from **readiness** (`/readyz`: are my dependencies up and should I receive traffic?), which is exactly the distinction Kubernetes liveness vs. readiness probes expect. `RequestBodyLimitLayer` (from tower-http's `limit` feature) rejects oversized request bodies before they reach a handler — a cheap, important hardening step for any public API. Swap the `db: String` placeholder for a real `sqlx::PgPool` as described in [the database section](/17-database/), and pair it with the connection-pool startup pattern from [Shared Application State in Axum](/16-web-apis/06-state-management/).

---

## Further Reading

- [Axum deployment examples](https://github.com/tokio-rs/axum/tree/main/examples) — official `graceful-shutdown` and TLS examples.
- [`axum::serve` docs](https://docs.rs/axum/latest/axum/fn.serve.html) and [`Serve::with_graceful_shutdown`](https://docs.rs/axum/latest/axum/serve/struct.Serve.html).
- [The Cargo Book — profiles](https://doc.rust-lang.org/cargo/reference/profiles.html) — `[profile.release]` knobs (`lto`, `codegen-units`, `strip`, `opt-level`, `panic`).
- [Distroless images](https://github.com/GoogleContainerTools/distroless) and [`cargo-chef`](https://github.com/LukeMathWalker/cargo-chef) for cached Docker dependency layers.
- [tower-http docs](https://docs.rs/tower-http) — `TimeoutLayer`, `RequestBodyLimitLayer`, `TraceLayer`.
- Sibling pages: [Setting Up an Axum Project](/16-web-apis/02-axum-setup/) (project setup), [Axum Fundamentals](/16-web-apis/01-axum-basics/) (`axum::serve` fundamentals), [Middleware and Layers](/16-web-apis/05-middleware/) (tower layers and tracing), [Shared Application State in Axum](/16-web-apis/06-state-management/) (injecting a DB pool/config), [CORS with Axum and tower-http](/16-web-apis/11-cors/) (locking down origins in production), [Choosing a Rust Web Framework](/16-web-apis/00-framework-comparison/).
- Related sections: [the async runtime](/11-async/), [databases and connection pools](/17-database/), and the prerequisites in [Getting Started](/01-getting-started/) and [Basics](/02-basics/).

---

## Exercises

### Exercise 1: Read the port from the environment

**Difficulty:** Beginner

**Objective:** Make a server deploy-ready by binding `0.0.0.0` and reading `PORT` from the environment with a sensible default.

**Instructions:** Start from a hello-world Axum app. Replace any hardcoded `127.0.0.1:3000` bind address with one that reads the `PORT` environment variable (default `8080`) and binds `0.0.0.0`. Print the bound address on startup. Verify it works by running it twice: once with `PORT` unset, once with `PORT=9000`.

<details>
<summary>Solution</summary>

```rust
// src/main.rs
// cargo add axum
// cargo add tokio --features full
use axum::{routing::get, Router};

async fn root() -> &'static str {
    "hello"
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/", get(root));

    // Default to 8080; many platforms inject the real port via $PORT.
    let port = std::env::var("PORT").unwrap_or_else(|_| "8080".to_string());
    // Bind 0.0.0.0 so the socket is reachable from outside a container.
    let addr = format!("0.0.0.0:{port}");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());

    axum::serve(listener, app).await.unwrap();
}
```

Running it (real output from this code):

```console
$ cargo run
listening on http://0.0.0.0:8080
$ PORT=9000 cargo run
listening on http://0.0.0.0:9000
```

Reading `PORT` from the environment with a default is the smallest change that makes a Rust web server portable across local runs and managed platforms.

</details>

### Exercise 2: Add graceful shutdown

**Difficulty:** Intermediate

**Objective:** Drain in-flight requests on `SIGINT` (Ctrl-C) and `SIGTERM` instead of dropping them.

**Instructions:** Take the server from Exercise 1 and add a `shutdown_signal()` async function that resolves on either Ctrl-C or (on Unix) SIGTERM, then pass it to `axum::serve(...).with_graceful_shutdown(...)`. Print a message when the signal arrives. Verify by starting the server and pressing Ctrl-C: it should log the shutdown message and exit cleanly.

<details>
<summary>Solution</summary>

```rust
// src/main.rs
// cargo add axum
// cargo add tokio --features full
use axum::{routing::get, Router};
use tokio::signal;

async fn root() -> &'static str {
    "hello"
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to install Ctrl-C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    println!("shutdown signal received, draining connections");
}

#[tokio::main]
async fn main() {
    let app = Router::new().route("/", get(root));
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    println!("listening on http://{}", listener.local_addr().unwrap());

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}
```

`tokio::select!` races the two signal futures; whichever fires first wins, and the function returns, which tells `axum::serve` to stop accepting new connections and finish in-flight ones. On non-Unix targets the `terminate` branch is `std::future::pending()` — a future that never completes — so only Ctrl-C triggers shutdown.

</details>

### Exercise 3: Multi-stage Dockerfile with a size-tuned profile

**Difficulty:** Advanced

**Objective:** Produce a small, secure container image for an Axum binary, building in a Rust toolchain image and shipping only the binary in a distroless runtime.

**Instructions:** Write a `[profile.release]` in `Cargo.toml` that strips symbols and enables LTO, a `.dockerignore` that excludes `target` and `.git`, and a multi-stage `Dockerfile` that (1) builds with `rust:1.96-slim`, caching dependencies via the dummy-`main.rs` trick, and (2) copies only the release binary into `gcr.io/distroless/cc-debian12`, running as `nonroot`, listening on `$PORT`/`0.0.0.0`. Build the image and `curl` a health endpoint to confirm.

<details>
<summary>Solution</summary>

`Cargo.toml` profile:

```toml
[profile.release]
lto = true
codegen-units = 1
strip = true
```

`.dockerignore`:

```text
target
.git
Dockerfile
.dockerignore
```

`Dockerfile`:

```dockerfile
# ---- Stage 1: build ----
FROM rust:1.96-slim AS builder
WORKDIR /app

# Dependency cache layer: build a dummy main from the manifests only.
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs \
    && cargo build --release \
    && rm -rf src

# Now the real sources; only this layer rebuilds on a code change.
COPY src ./src
RUN touch src/main.rs && cargo build --release

# ---- Stage 2: runtime ----
FROM gcr.io/distroless/cc-debian12 AS runtime
WORKDIR /app
COPY --from=builder /app/target/release/myapi /usr/local/bin/myapi
ENV PORT=8080
EXPOSE 8080
USER nonroot:nonroot
CMD ["myapi"]
```

Build and verify (real output from building and running this against the `myapi` server):

```console
$ docker build -t myapi:latest .
 => exporting to image ... done
$ docker images myapi:latest --format '{{.Size}}'
36.2MB
# Pass the required DATABASE_URL; the Dockerfile already sets PORT=8080.
$ docker run -d -e DATABASE_URL=postgres://localhost/app -p 18080:8080 myapi:latest
$ curl -s http://127.0.0.1:18080/healthz
{"status":"ok"}
```

The dependency layer is cached, so editing only `src/` rebuilds in seconds rather than recompiling every crate. The distroless runtime has no shell or package manager and runs unprivileged, giving a small image with a minimal attack surface — and the deployed artifact is just your binary, not a runtime plus a dependency tree.

</details>
