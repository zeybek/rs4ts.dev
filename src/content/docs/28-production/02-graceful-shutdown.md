---
title: "Graceful Shutdown"
description: "Catch SIGTERM with Tokio and drain in-flight requests via axum's with_graceful_shutdown for zero-downtime deploys, the Rust answer to Node's server.close()."
---

When an orchestrator like Kubernetes redeploys your service, it sends a signal and gives you a few seconds to clean up before it kills the process. Handling that window correctly is the difference between zero-downtime deploys and a stream of `502`s for every in-flight request. This page shows how to catch shutdown signals with Tokio and drain in-flight requests with `axum`'s `with_graceful_shutdown`.

---

## Quick Overview

**Graceful shutdown** means: stop accepting new work, let work already in progress finish, then exit. In a Node.js service you reach for `process.on("SIGTERM", ...)` and `server.close(callback)`. In Rust with Tokio and `axum`, you build a future that resolves when a signal arrives and hand it to `axum::serve(...).with_graceful_shutdown(...)`. The server then stops accepting connections and waits for outstanding requests to complete before the `await` returns.

This matters to a TypeScript/JavaScript developer because the mental model is nearly identical to `server.close()`, but the mechanics are different: instead of a callback you pass a **future**, and instead of an event-loop hook you compose async building blocks (`tokio::select!`, `CancellationToken`) that the compiler checks for you.

---

## TypeScript/JavaScript Example

A typical Node.js HTTP service that drains on `SIGTERM`. This is realistic production code: it tracks the server lifecycle, flips a "draining" flag, and enforces a hard deadline so a stuck request can never block the deploy forever.

```typescript
// server.mjs — Node v22, graceful shutdown of an http.Server
import http from "node:http";

let shuttingDown = false;

const server = http.createServer((req, res) => {
  if (req.url === "/slow") {
    // An in-flight request that takes a while to finish.
    setTimeout(() => {
      res.writeHead(200, { "Content-Type": "text/plain" });
      res.end("done\n");
    }, 1500);
    return;
  }
  // Once draining, fail readiness so the load balancer stops sending traffic.
  res.writeHead(shuttingDown ? 503 : 200);
  res.end(shuttingDown ? "draining\n" : "ok\n");
});

function shutdown(signal: string) {
  console.log(`${signal} received, draining`);
  shuttingDown = true;

  // Stop accepting new connections; the callback fires once existing ones close.
  server.close(() => {
    console.log("all connections drained, exiting");
    process.exit(0);
  });

  // Safety net: never wait forever. `.unref()` lets the process exit early
  // if draining finishes first.
  setTimeout(() => {
    console.error("drain timed out, forcing exit");
    process.exit(1);
  }, 10_000).unref();
}

process.on("SIGTERM", () => shutdown("SIGTERM"));
process.on("SIGINT", () => shutdown("SIGINT"));

server.listen(3100, () => console.log("listening on :3100"));
```

Running this, sending `SIGTERM` while a `/slow` request is in flight, prints (real Node v22 output):

```text
listening on :3100
SIGTERM received, draining
all connections drained, exiting
```

The in-flight `/slow` request still returns `done` with exit code `0`; a brand-new connection opened after `SIGTERM` is refused. That is exactly the behavior we want to reproduce in Rust.

**Key points:**

- `process.on("SIGTERM", ...)` registers a signal handler on the event loop.
- `server.close(cb)` stops accepting connections and calls back once existing ones finish.
- A `setTimeout(...).unref()` enforces a hard drain deadline.
- A `shuttingDown` flag lets readiness probes fail so traffic stops arriving.

---

## Rust Equivalent

The same idea in `axum` 0.8 on Tokio. The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition, and `cargo new` selects that edition automatically. Add the dependencies:

```bash
cargo add axum
cargo add tokio --features full
```

```rust
use std::time::Duration;

use axum::{extract::State, routing::get, Router};
use tokio::signal;

#[derive(Clone)]
struct AppState {
    started_at: std::time::Instant,
}

async fn root() -> &'static str {
    "Hello from a graceful server\n"
}

async fn slow(State(state): State<AppState>) -> String {
    // Simulate an in-flight request that takes a while to finish.
    tokio::time::sleep(Duration::from_secs(3)).await;
    format!("done after {:?}\n", state.started_at.elapsed())
}

#[tokio::main]
async fn main() {
    let state = AppState {
        started_at: std::time::Instant::now(),
    };

    let app = Router::new()
        .route("/", get(root))
        .route("/slow", get(slow))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .expect("failed to bind");

    println!("listening on {}", listener.local_addr().unwrap());

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");

    println!("server has shut down cleanly");
}

/// A future that resolves when the process should begin shutting down.
async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
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

    println!("signal received, starting graceful shutdown");
}
```

Building and running this server, then sending `SIGTERM` while a `curl http://127.0.0.1:3000/slow` is in flight, produces this **real** output:

```text
listening on 127.0.0.1:3000
signal received, starting graceful shutdown
server has shut down cleanly
```

Importantly, the in-flight `/slow` request still completes (the client receives `done after 3.04008875s` with a success exit code) while a new connection attempted after the signal is refused (the listener is already closed). The `await` on `axum::serve(...)` returns only after the last in-flight request has been served.

**Key points:**

- `axum::serve(listener, app)` is the current API. The old `axum::Server::bind(...).serve(...)` builder was removed; do not use it.
- `.with_graceful_shutdown(future)` makes the server stop accepting connections as soon as `future` resolves, then drain.
- `shutdown_signal()` is just an `async fn`: a future you compose from signal sources with `tokio::select!`.
- `#[cfg(unix)]` guards the `SIGTERM` handler so the code still compiles on Windows.

---

## Detailed Explanation

### `axum::serve` and the shutdown future

```rust
// (excerpt)
axum::serve(listener, app)
    .with_graceful_shutdown(shutdown_signal())
    .await
    .expect("server error");
```

`axum::serve(listener, app)` returns a `Serve` value: a future that, when awaited, runs the accept loop forever. Calling `.with_graceful_shutdown(future)` wraps it so the accept loop *also* watches `future`. The moment `future` resolves:

1. The server stops accepting **new** connections (the TCP listener is dropped).
2. Connections with a request currently in flight are allowed to finish.
3. Once they all complete, the outer `.await` returns and control falls through to `println!("server has shut down cleanly")`.

This is the direct analog of `server.close(callback)` in Node, but instead of a callback you supply a **future** describing *when* to start closing, and the cleanup code is whatever you write after `.await`.

> **Note:** The future you pass decides *when* shutdown begins. It does not have to be a signal. It could resolve on a message from a channel, a `CancellationToken`, or an admin HTTP endpoint. Signals are just the most common trigger.

### Catching signals with Tokio

```rust
// (excerpt)
let ctrl_c = async {
    signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
};

#[cfg(unix)]
let terminate = async {
    signal::unix::signal(signal::unix::SignalKind::terminate())
        .expect("failed to install SIGTERM handler")
        .recv()
        .await;
};
```

`tokio::signal::ctrl_c()` returns a future that resolves once on the next `Ctrl+C` (`SIGINT`). That covers interactive use and `docker stop --signal SIGINT`.

For Unix, `SIGTERM` is the signal Kubernetes and most process managers send first, so you must handle it explicitly. `signal::unix::signal(SignalKind::terminate())` returns a `Signal`, which is a **stream** of signal deliveries, not a one-shot future: you call `.recv().await` to wait for the next one. (Forgetting `.recv()` is a real compile error; see [Common Pitfalls](#common-pitfalls).)

### `tokio::select!` — wait for whichever happens first

```rust
// (excerpt)
tokio::select! {
    _ = ctrl_c => {},
    _ = terminate => {},
}
```

`tokio::select!` polls several futures concurrently and completes as soon as **any one** of them resolves, dropping the rest. This is the async equivalent of registering listeners for *both* `SIGINT` and `SIGTERM` and reacting to whichever fires first. It is conceptually close to `Promise.race([...])`, but `select!` works on the spot inside an async block, can bind the winning value with patterns, and cancels the losing futures cleanly. See [select and join](/11-async/07-select-join/) for a deeper comparison.

### Why the `#[cfg(not(unix))]` arm exists

```rust
// (excerpt)
#[cfg(not(unix))]
let terminate = std::future::pending::<()>();
```

`SignalKind::terminate()` does not exist on Windows, so the `SIGTERM` block is compiled in only on Unix. On other platforms we substitute `std::future::pending::<()>()` — a future that *never* resolves — so the `select!` still type-checks and simply relies on `ctrl_c`. Without this arm, the code would fail to compile on Windows. This is the conditional-compilation analog of a runtime `if (process.platform !== "win32")` guard, but resolved at compile time with zero runtime cost.

---

## Key Differences

| Concern | Node.js | Rust (`axum` + Tokio) |
| --- | --- | --- |
| Register signal handler | `process.on("SIGTERM", cb)` | `tokio::signal::ctrl_c()` / `signal::unix::signal(...)` futures |
| Stop accepting connections | `server.close(cb)` | `.with_graceful_shutdown(future)` resolves |
| "When to start" trigger | a callback fired by an event | a **future** you compose and pass in |
| Wait for in-flight work | callback fires after sockets close | the `serve(...).await` returns after draining |
| React to first of N events | `Promise.race([...])` | `tokio::select! { ... }` |
| Hard drain deadline | `setTimeout(...).unref()` | `tokio::time::timeout(dur, fut)` |
| Cancel background tasks | manual flags / `AbortController` | `CancellationToken` + `TaskTracker` |
| Cross-platform signals | `process.platform` checks | `#[cfg(unix)]` conditional compilation |

The deepest conceptual shift: in Node you hook the event loop and write imperative cleanup in a callback. In Rust you describe shutdown **declaratively as a future**, and the runtime drives it. Because futures are values, you can clone the trigger, hand copies to background tasks, and compose timeouts around them, all checked by the compiler.

> **Warning:** Rust futures are **lazy**. A future does nothing until it is `.await`ed or spawned onto a runtime — the opposite of an eager JavaScript `Promise`, which starts executing the moment you create it. `signal::ctrl_c()` does not begin listening until its future is polled inside `select!`.

---

## Common Pitfalls

### Awaiting a `Signal` directly

A `tokio::signal::unix::Signal` is a stream of deliveries, not a future. Awaiting it directly does not compile:

```rust
use tokio::signal::unix::{signal, SignalKind};

#[tokio::main]
async fn main() {
    let mut sigterm = signal(SignalKind::terminate()).unwrap();
    // does not compile (error[E0277]: `Signal` is not a future)
    sigterm.await;
}
```

The real compiler error:

```text
error[E0277]: `Signal` is not a future
 --> src/main.rs:7:13
  |
7 |     sigterm.await;
  |             ^^^^^ `Signal` is not a future
  |
  = help: the trait `Future` is not implemented for `Signal`
  = note: Signal must be a future or must implement `IntoFuture` to be awaited
help: remove the `.await`
  |
7 -     sigterm.await;
7 +     sigterm;
  |
```

The fix is `sigterm.recv().await`, which waits for the next delivery and yields `Option<()>`.

### Forgetting `with_graceful_shutdown` entirely

```rust
// Compiles and runs, but NOT graceful.
// axum::serve(listener, app).await.unwrap();
```

If you omit `.with_graceful_shutdown(...)`, the server has no idea a signal arrived. The default runtime behavior on `Ctrl+C` is to abort the process immediately, severing every in-flight request mid-response. The code compiles fine: the bug is silent and only shows up as truncated responses during a deploy. Always attach a shutdown future to any service that must deploy without dropping requests.

### Doing the slow cleanup *before* the server drains

A tempting mistake is to run all your cleanup (flush metrics, close the DB pool) **inside** the shutdown future, before the server has drained:

```rust
// Anti-pattern: close the DB pool while requests are still in flight.
// .with_graceful_shutdown(async move {
//     wait_for_signal().await;
//     db_pool.close().await; // requests still running will now fail!
// })
```

The shutdown future resolves the instant the signal arrives; draining happens *after* it returns. So tearing down dependencies inside that future yanks them out from under requests that are still completing. Put dependency teardown **after** `axum::serve(...).await`, once draining is done. (See the [Real-World Example](#real-world-example).)

### Blocking the async runtime during shutdown

Calling a blocking function (`std::thread::sleep`, synchronous file I/O, a blocking DB driver) inside the shutdown path stalls a Tokio worker thread and can wedge the drain. Use the async equivalents (`tokio::time::sleep`, `tokio::fs`) or `tokio::task::spawn_blocking` for unavoidably blocking work. See [async vs sync](/11-async/13-async-vs-sync/).

### Not bounding the drain

If one request hangs forever (a slow upstream, a deadlock), an unbounded drain blocks your deploy indefinitely, and Kubernetes will eventually `SIGKILL` you anyway, ungracefully. Always wrap the drain in a deadline with `tokio::time::timeout`, mirroring the Node `setTimeout(...).unref()` safety net.

---

## Best Practices

- **Always handle both `SIGINT` and `SIGTERM`.** `SIGTERM` is what orchestrators send; `SIGINT` is `Ctrl+C` in development. Handle both with `tokio::select!`.
- **Guard `SIGTERM` with `#[cfg(unix)]`** and supply a `std::future::pending()` fallback so the binary still builds on Windows.
- **Flip readiness to "not ready" first.** When shutdown begins, make your `/readyz` probe return `503` so the load balancer stops sending new traffic *before* you stop accepting connections. This closes the small window where new requests arrive at a server that is about to die. (Readiness probes are covered in [health checks](/28-production/03-health-checks/).)
- **Bound the drain with `tokio::time::timeout`.** Pick a budget shorter than your orchestrator's `terminationGracePeriodSeconds` so you exit cleanly before being force-killed.
- **Cancel background tasks with a `CancellationToken`** and wait for them with a `TaskTracker`. Clone the token into every spawned task so a single `.cancel()` reaches all of them.
- **Tear down dependencies after the drain**, not inside the shutdown future: close DB pools, flush metrics, and finish background jobs only once in-flight HTTP requests have completed.
- **Emit structured logs around each phase** (`signal received`, `draining`, `complete`) so you can confirm graceful shutdown in production. Use [tracing](/11-async/) and the [distributed tracing](/28-production/05-distributed-tracing/) page.

---

## Real-World Example

A production-flavored service that ties everything together: it flips readiness off on shutdown, gives the load balancer a moment to react, drains in-flight HTTP requests, then cancels a background worker and waits for it with a bounded deadline. Dependencies:

```bash
cargo add axum
cargo add tokio --features full
cargo add tokio-util --features rt
cargo add tracing
cargo add tracing-subscriber
```

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::{routing::get, Router};
use tokio::signal;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

#[derive(Clone)]
struct AppState {
    /// Flips to `false` the moment shutdown begins so the load balancer
    /// stops routing new traffic here while we drain.
    ready: Arc<AtomicBool>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let ready = Arc::new(AtomicBool::new(true));
    let state = AppState {
        ready: ready.clone(),
    };

    let shutdown = CancellationToken::new();
    let tracker = TaskTracker::new();

    // Background worker (e.g. a queue consumer) that drains on cancel.
    {
        let shutdown = shutdown.clone();
        tracker.spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(1));
            loop {
                tokio::select! {
                    _ = tick.tick() => tracing::info!("worker heartbeat"),
                    _ = shutdown.cancelled() => break,
                }
            }
            tracing::info!("worker drained");
        });
    }

    let app = Router::new()
        .route("/", get(|| async { "hello\n" }))
        .route("/healthz", get(live))
        .route("/readyz", get(ready_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    tracing::info!(addr = %listener.local_addr().unwrap(), "listening");

    let shutdown_for_server = shutdown.clone();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            wait_for_signal().await;
            tracing::info!("shutdown signal received");
            // 1. Mark unready so readiness probes fail and traffic stops.
            ready.store(false, Ordering::SeqCst);
            // 2. Give the orchestrator a moment to notice before we stop
            //    accepting connections (avoids a brief 502 window).
            tokio::time::sleep(Duration::from_secs(1)).await;
            // 3. Signal background tasks to wind down.
            shutdown_for_server.cancel();
        })
        .await
        .unwrap();

    // The HTTP server has fully drained. Now drain background tasks,
    // but never wait forever: cap the drain at a deadline.
    tracker.close();
    match tokio::time::timeout(Duration::from_secs(15), tracker.wait()).await {
        Ok(()) => tracing::info!("graceful shutdown complete"),
        Err(_) => tracing::warn!("drain timed out; exiting anyway"),
    }
}

async fn live() -> StatusCode {
    StatusCode::OK
}

async fn ready_handler(State(state): State<AppState>) -> StatusCode {
    if state.ready.load(Ordering::SeqCst) {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

async fn wait_for_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("ctrl_c handler");
    };
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}
```

Running this, hitting `/readyz`, then sending `SIGTERM` and probing `/readyz` again during the drain shows the readiness flip in action:

```text
readyz BEFORE shutdown: 200
readyz DURING drain:    503
```

And the **real** structured log over the full lifecycle (ANSI colors stripped):

```text
2026-06-02T06:42:36.170473Z  INFO probe2: listening addr=0.0.0.0:8080
2026-06-02T06:42:36.171560Z  INFO probe2: worker heartbeat
2026-06-02T06:42:37.172777Z  INFO probe2: worker heartbeat
2026-06-02T06:42:37.683791Z  INFO probe2: shutdown signal received
2026-06-02T06:42:38.172939Z  INFO probe2: worker heartbeat
2026-06-02T06:42:38.685163Z  INFO probe2: worker drained
2026-06-02T06:42:38.685255Z  INFO probe2: graceful shutdown complete
```

Notice the ordering: the signal arrives, readiness flips to `503`, the worker keeps heartbeating during the one-second grace window, then is cancelled and drains, and only then does shutdown complete. This is the full zero-downtime sequence.

> **Tip:** `CancellationToken::cancel()` is idempotent and the token is cheap to `clone()`, so you can hand a clone to every background task and a single `.cancel()` reaches all of them. `TaskTracker::wait()` returns once every tracked task has finished, after you call `tracker.close()` to stop accepting new ones. See [spawning tasks](/11-async/09-spawning-tasks/) and [background jobs](/28-production/08-background-jobs/).

---

## Further Reading

- [Tokio `signal` module docs](https://docs.rs/tokio/latest/tokio/signal/index.html): `ctrl_c` and Unix signals
- [`axum::serve` and `with_graceful_shutdown`](https://docs.rs/axum/latest/axum/serve/struct.Serve.html#method.with_graceful_shutdown)
- [`tokio_util::sync::CancellationToken`](https://docs.rs/tokio-util/latest/tokio_util/sync/struct.CancellationToken.html) and [`TaskTracker`](https://docs.rs/tokio-util/latest/tokio_util/task/struct.TaskTracker.html)
- [`tokio::select!` macro](https://docs.rs/tokio/latest/tokio/macro.select.html)
- Related guide sections:
  - [11 - Async: select and join](/11-async/07-select-join/) — `select!` vs `Promise.race`
  - [11 - Async: spawning tasks](/11-async/09-spawning-tasks/): background tasks and cancellation
  - [16 - Web APIs: axum basics](/16-web-apis/01-axum-basics/) — building the router this page shuts down
  - [28 - Production: health checks](/28-production/03-health-checks/): the readiness probe we flip on shutdown
  - [28 - Production: background jobs](/28-production/08-background-jobs/) — draining job runners on shutdown
  - [28 - Production: production checklist](/28-production/09-production-checklist/): where graceful shutdown fits in
  - [29 - Migration Guide](/29-migration-guide/) — moving a Node service to Rust

---

## Exercises

### Exercise 1: Add an `SIGTERM`-aware health endpoint

**Difficulty:** Beginner

**Objective:** Reproduce the readiness flip so a load balancer stops sending traffic the moment shutdown starts.

**Instructions:** Starting from the first Rust example, add an `AppState` carrying an `Arc<AtomicBool>` named `ready`, initialized to `true`. Add a `/readyz` route that returns `200 OK` when `ready` is `true` and `503 Service Unavailable` otherwise. In the shutdown future, set `ready` to `false` before the drain begins. Verify with `curl` that `/readyz` returns `200` before the signal and `503` after.

<details>
<summary>Solution</summary>

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::{routing::get, Router};
use tokio::signal;

#[derive(Clone)]
struct AppState {
    ready: Arc<AtomicBool>,
}

#[tokio::main]
async fn main() {
    let ready = Arc::new(AtomicBool::new(true));
    let state = AppState {
        ready: ready.clone(),
    };

    let app = Router::new()
        .route("/", get(|| async { "ok\n" }))
        .route("/readyz", get(readyz))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    println!("listening on {}", listener.local_addr().unwrap());

    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            wait_for_signal().await;
            // Fail readiness so traffic stops arriving, then drain.
            ready.store(false, Ordering::SeqCst);
            println!("readiness disabled, draining");
        })
        .await
        .unwrap();

    println!("shut down cleanly");
}

async fn readyz(State(state): State<AppState>) -> StatusCode {
    if state.ready.load(Ordering::SeqCst) {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    }
}

async fn wait_for_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("ctrl_c handler");
    };
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}
```

> Note: the `Ordering::SeqCst` here is a memory-ordering parameter for the atomic, not connected to HTTP status codes.

</details>

### Exercise 2: Bound the shutdown with a deadline

**Difficulty:** Intermediate

**Objective:** Ensure a hung request can never block the deploy forever.

**Instructions:** Spawn the `axum` server as a Tokio task (`tokio::spawn`) so you can `await` it separately. After the shutdown signal triggers, wrap the server's join handle in `tokio::time::timeout(Duration::from_secs(10), ...)`. If the drain completes in time, log "drained cleanly"; if the timeout fires, log "drain timed out; forcing exit". This mirrors the Node `setTimeout(...).unref()` safety net.

<details>
<summary>Solution</summary>

```rust
use std::time::Duration;

use axum::{routing::get, Router};
use tokio::signal;
use tokio_util::sync::CancellationToken;

#[tokio::main]
async fn main() {
    let shutdown = CancellationToken::new();

    let app = Router::new().route("/", get(|| async { "ok\n" }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    println!("listening on {}", listener.local_addr().unwrap());

    // Run the server on its own task so we can time the drain.
    let server_shutdown = shutdown.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                server_shutdown.cancelled().await;
            })
            .await
            .unwrap();
    });

    // Wait for the OS signal, then ask the server to drain.
    wait_for_signal().await;
    println!("signal received, draining");
    shutdown.cancel();

    // Never wait forever for the drain to finish.
    match tokio::time::timeout(Duration::from_secs(10), server).await {
        Ok(Ok(())) => println!("drained cleanly"),
        Ok(Err(join_err)) => println!("server task panicked: {join_err}"),
        Err(_) => println!("drain timed out; forcing exit"),
    }
}

async fn wait_for_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("ctrl_c handler");
    };
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}
```

This requires `cargo add tokio-util`. The server runs as a spawned task; after the signal we cancel the token (which resolves the server's shutdown future) and then race the join handle against a 10-second deadline.

</details>

### Exercise 3: Drain a background worker on shutdown

**Difficulty:** Advanced

**Objective:** Cancel a long-running background task cooperatively and wait for it to finish before exiting.

**Instructions:** Spawn a background worker with `TaskTracker::spawn` that loops on a `tokio::time::interval`, doing a unit of work each tick. Inside the loop, use `tokio::select!` to watch a shared `CancellationToken`; when it is cancelled, log a message and break. After the HTTP server drains, call `tracker.close()` and `tracker.wait()` (wrapped in a `tokio::time::timeout`) so the process waits for the worker to finish its current unit of work. Add `tokio-util` with the `rt` feature.

<details>
<summary>Solution</summary>

```bash
cargo add tokio --features full
cargo add tokio-util --features rt
```

```rust
use std::time::Duration;

use tokio::signal;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

#[tokio::main]
async fn main() {
    let shutdown = CancellationToken::new();
    let tracker = TaskTracker::new();

    // A background worker that drains cooperatively on cancel.
    {
        let shutdown = shutdown.clone();
        tracker.spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_millis(200));
            loop {
                tokio::select! {
                    _ = tick.tick() => println!("worker: doing a unit of work"),
                    _ = shutdown.cancelled() => {
                        println!("worker: cancellation observed, finishing up");
                        break;
                    }
                }
            }
            println!("worker: stopped");
        });
    }

    // Wait for the OS signal, then tell the worker to wind down.
    wait_for_signal().await;
    println!("signal received");
    shutdown.cancel();

    // Drain background tasks with a deadline.
    tracker.close();
    match tokio::time::timeout(Duration::from_secs(10), tracker.wait()).await {
        Ok(()) => println!("all background tasks drained cleanly"),
        Err(_) => println!("drain deadline exceeded; forcing exit"),
    }
    println!("bye");
}

async fn wait_for_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("ctrl_c handler");
    };
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("SIGTERM handler")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}
```

Sending `SIGTERM` after a couple of seconds produces this real output:

```text
worker: doing a unit of work
worker: doing a unit of work
worker: doing a unit of work
signal received
worker: cancellation observed, finishing up
worker: stopped
all background tasks drained cleanly
bye
```

The worker observes the cancellation, finishes cleanly, and the process waits for it before printing `bye`. In a real service you would combine this with the `axum` server from the [Real-World Example](#real-world-example) so HTTP and background work both drain together.

</details>
