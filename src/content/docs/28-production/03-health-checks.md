---
title: "Health and Readiness Endpoints"
description: "Expose liveness and readiness endpoints in Rust with axum, checking dependencies concurrently with tokio::join! and timeouts, vs Express probes in Node.js."
---

A health check is the contract between your service and whatever is operating it: Kubernetes, a load balancer, an autoscaler, or a paging system. Get it wrong and a perfectly healthy service gets restarted in a loop, or a broken one keeps receiving traffic. This chapter shows how to expose **liveness** and **readiness** endpoints in Rust with [axum](/16-web-apis/), and how to check downstream dependencies safely.

---

## Quick Overview

Production orchestrators poll two distinct kinds of probe, and conflating them is the single most common health-check bug:

- **Liveness** answers "is this process broken beyond recovery?" If it fails, the orchestrator **restarts the container**. It must be cheap and must **never** depend on the database or other services.
- **Readiness** answers "should this instance receive traffic right now?" If it fails, the orchestrator **stops routing requests** to this instance but leaves it running. This is where you check dependencies (database, cache, downstream APIs).

In Node you usually bolt these onto an Express router. In Rust the shape is the same, but the type system makes the response codes explicit, native `async` lets you check dependencies with hard timeouts, and `tokio::join!` lets you probe several dependencies concurrently.

---

## TypeScript/JavaScript Example

A typical Express service exposes both probes. Note how readiness pings the database while liveness deliberately does not:

```typescript
// health.ts — Express on Node v22
import express, { Request, Response } from "express";
import { Pool } from "pg";
import { createClient } from "redis";

const pool = new Pool({ connectionString: process.env.DATABASE_URL });
const redis = createClient({ url: process.env.REDIS_URL });
await redis.connect();

// `ready` flips to true once startup work (migrations, warm pools) is done.
let ready = false;

export const health = express.Router();

// Liveness: cheap, no dependencies. If this fails, restart me.
health.get("/health/live", (_req: Request, res: Response) => {
  res.status(200).json({ status: "ok" });
});

// Readiness: check dependencies. If this fails, stop sending me traffic.
health.get("/health/ready", async (_req: Request, res: Response) => {
  if (!ready) {
    return res.status(503).json({ status: "starting", checks: [] });
  }

  const checks: { name: string; healthy: boolean; detail?: string }[] = [];

  try {
    await Promise.race([
      pool.query("SELECT 1"),
      timeout(2000), // a hung DB must not hang the probe
    ]);
    checks.push({ name: "database", healthy: true });
  } catch (err) {
    checks.push({ name: "database", healthy: false, detail: String(err) });
  }

  try {
    await Promise.race([redis.ping(), timeout(2000)]);
    checks.push({ name: "cache", healthy: true });
  } catch (err) {
    checks.push({ name: "cache", healthy: false, detail: String(err) });
  }

  const allOk = checks.every((c) => c.healthy);
  res
    .status(allOk ? 200 : 503)
    .json({ status: allOk ? "ready" : "degraded", checks });
});

function timeout(ms: number): Promise<never> {
  return new Promise((_, reject) =>
    setTimeout(() => reject(new Error("timed out")), ms),
  );
}
```

Key points:

- Liveness is a constant `200`; readiness returns `503` when a dependency is down so the load balancer drains the instance.
- `Promise.race` against a timeout protects the probe from a hung dependency. Without it, a stuck `pool.query` would hang the endpoint and the orchestrator would eventually kill a healthy process.
- A `ready` flag gates traffic until startup finishes.

---

## Rust Equivalent

The same service in axum. The dependency clients (`Db`, `Cache`) stand in for `sqlx::PgPool` and a Redis client; the structure is identical to what you would write with the real crates from [Section 17: Database](/17-database/).

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::get,
};
use serde::Serialize;
use serde_json::json;

// Stand-ins for real clients (`sqlx::PgPool`, a Redis client, ...).
#[derive(Clone)]
struct Db;
impl Db {
    async fn ping(&self) -> Result<(), String> {
        tokio::time::sleep(Duration::from_millis(3)).await;
        Ok(()) // imagine: sqlx::query("SELECT 1").execute(pool).await
    }
}

#[derive(Clone)]
struct Cache;
impl Cache {
    async fn ping(&self) -> Result<(), String> {
        tokio::time::sleep(Duration::from_millis(2)).await;
        Ok(()) // imagine a Redis PING
    }
}

#[derive(Clone)]
struct AppState {
    db: Db,
    cache: Cache,
    // Flipped to `true` once startup work finishes (see Detailed Explanation).
    ready: Arc<AtomicBool>,
}

#[derive(Serialize)]
struct CheckResult {
    name: &'static str,
    healthy: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

fn to_result(name: &'static str, r: Result<(), String>) -> CheckResult {
    match r {
        Ok(()) => CheckResult { name, healthy: true, detail: None },
        Err(e) => CheckResult { name, healthy: false, detail: Some(e) },
    }
}

// Liveness: cheap, no dependencies. If this fails the orchestrator restarts us.
async fn liveness() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({ "status": "ok" })))
}

// Readiness: checks every dependency CONCURRENTLY, each bounded by a deadline.
async fn readiness(State(state): State<AppState>) -> impl IntoResponse {
    if !state.ready.load(Ordering::Relaxed) {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "status": "starting", "checks": [] })),
        );
    }

    let deadline = Duration::from_secs(2);
    let db_fut = tokio::time::timeout(deadline, state.db.ping());
    let cache_fut = tokio::time::timeout(deadline, state.cache.ping());

    // Run both at once: total latency is max(db, cache), not the sum.
    let (db_res, cache_res) = tokio::join!(db_fut, cache_fut);

    // A timeout (the outer Err) and a failed ping (the inner Err) both mean
    // "unhealthy"; collapse them into one Result<(), String>.
    let flatten = |r: Result<Result<(), String>, tokio::time::error::Elapsed>| match r {
        Ok(inner) => inner,
        Err(_) => Err("timed out".to_string()),
    };

    let checks = vec![
        to_result("database", flatten(db_res)),
        to_result("cache", flatten(cache_res)),
    ];
    let all_ok = checks.iter().all(|c| c.healthy);

    let code = if all_ok { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    let body = json!({
        "status": if all_ok { "ready" } else { "degraded" },
        "checks": checks,
    });
    (code, Json(body))
}

fn app(state: AppState) -> Router {
    Router::new()
        .route("/health/live", get(liveness))
        .route("/health/ready", get(readiness))
        .with_state(state)
}

#[tokio::main]
async fn main() {
    let state = AppState {
        db: Db,
        cache: Cache,
        ready: Arc::new(AtomicBool::new(true)),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8772").await.unwrap();
    axum::serve(listener, app(state)).await.unwrap();
}
```

The dependencies for this example:

```toml
[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

> **Note:** The current stable toolchain is Rust 1.96.0 on the 2024 edition. `cargo new` selects it automatically, and `cargo add axum tokio serde serde_json` resolves the versions above.

Hitting both endpoints against the running server returns compact JSON (use `-w '\n[HTTP %{http_code}]\n'` to also print the status code):

```text
$ curl -s http://127.0.0.1:8772/health/live
{"status":"ok"}

$ curl -s -w '\n[HTTP %{http_code}]\n' http://127.0.0.1:8772/health/ready
{"checks":[{"healthy":true,"name":"database"},{"healthy":true,"name":"cache"}],"status":"ready"}
[HTTP 200]
```

Pipe to `jq` if you want it pretty-printed: `curl -s http://127.0.0.1:8772/health/ready | jq`.

When a dependency is down, readiness reports the specific failure and returns `503` so the load balancer drains the instance. For example, if the cache ping returned `Err("connection refused")` the response body would be (again, compact):

```text
$ curl -s -w '\n[HTTP %{http_code}]\n' http://127.0.0.1:8772/health/ready
{"checks":[{"healthy":true,"name":"database"},{"detail":"connection refused","healthy":false,"name":"cache"}],"status":"degraded"}
[HTTP 503]
```

---

## Detailed Explanation

### Why two endpoints, not one

A single `/health` endpoint cannot serve both purposes, and using one for both causes outages:

| Probe         | Question it answers                       | On failure                  | May touch dependencies? |
| ------------- | ----------------------------------------- | --------------------------- | ----------------------- |
| **Liveness**  | Is the process wedged / deadlocked?       | **Restart the container**   | No, never               |
| **Readiness** | Should this instance get traffic *now*?   | **Remove from the LB pool** | Yes, that is the point  |

The trap: if your liveness probe pings the database, then a brief database outage makes liveness fail, the orchestrator restarts every instance, and now you have a database outage *and* a thundering herd of cold-starting processes hammering the database as it recovers. Liveness must depend only on the process itself.

### `impl IntoResponse` and tuple responses

Both handlers return `impl IntoResponse`. axum implements `IntoResponse` for many shapes, including:

- `StatusCode` → an empty body with that status.
- `Json<T>` → a `200` with a JSON body.
- `(StatusCode, Json<T>)` → that status *with* a JSON body.

The readiness handler returns `(StatusCode, Json<Value>)` from **both** branches. That uniformity matters: every `return` path and the tail expression must produce the *same* type, because `impl IntoResponse` resolves to one concrete type. (Mixing a bare `StatusCode` with a tuple is a compile error; see Common Pitfalls.)

### Concurrent checks with `tokio::join!`

The Node version `await`s the database, then `await`s Redis, so the probe's latency is the **sum**. `tokio::join!` polls both futures on the same task concurrently, so latency is the **max**:

```rust
let (db_res, cache_res) = tokio::join!(db_fut, cache_fut);
```

Unlike `Promise.all`, `tokio::join!` does not short-circuit on the first failure. It waits for every future and gives you all results, which is exactly what a health report wants: you want to know *every* unhealthy dependency, not just the first one.

### Bounding every check with a timeout

`tokio::time::timeout(deadline, fut)` wraps a future and returns `Err(Elapsed)` if it does not finish in time. This is the Rust equivalent of `Promise.race([work, timeout(2000)])`, but it actually *cancels* the inner future when the deadline fires (Rust futures are lazy and droppable), rather than leaving an orphaned operation running. A health probe with no timeout is a latent outage: a single hung connection turns into a hung endpoint, and the orchestrator eventually kills a process that was otherwise fine.

### The startup gate

`ready: Arc<AtomicBool>` mirrors the `let ready = false` flag in the Node example. Until startup work (warming pools, running migrations, priming caches) completes, readiness returns `503 "starting"` so traffic is held back. An `AtomicBool` is the right tool here: it is shared across handler tasks (`Arc`), needs no lock for a single boolean, and `Ordering::Relaxed` is sufficient because the value is independent of any other memory. A realistic startup sequence flips it from a spawned task:

```rust
let ready_flag = state.ready.clone();
tokio::spawn(async move {
    run_migrations().await;          // imagine real startup work
    warm_connection_pools().await;
    ready_flag.store(true, Ordering::Relaxed);
});
```

This same flag is what your [graceful shutdown](/28-production/02-graceful-shutdown/) handler flips back to `false` when a `SIGTERM` arrives, so the load balancer drains the instance *before* you stop accepting connections.

---

## Key Differences

| Concern              | TypeScript / Express (Node v22)                  | Rust / axum                                              |
| -------------------- | ------------------------------------------------ | ------------------------------------------------------- |
| Status code          | `res.status(503)`, runtime string/number          | `StatusCode::SERVICE_UNAVAILABLE`, a checked constant    |
| Response shape       | Any object; mismatches surface at runtime         | Every branch must return the same `IntoResponse` type    |
| Timeout              | `Promise.race`; loser keeps running               | `tokio::time::timeout`; the inner future is **cancelled** |
| Concurrent checks    | Sequential `await`s = sum of latencies (the closest concurrent analogue, `Promise.all`, short-circuits on first reject) | `tokio::join!` runs both concurrently and waits for all — a full report |
| Startup flag         | `let ready` captured in a closure                 | `Arc<AtomicBool>` shared across tasks                    |
| Missing `await`      | Probe silently "passes" on a pending Promise      | **Won't compile**; `Future` has no `.is_ok()`           |

The throughline: in Node a sloppy health check is a *silent* liability. A forgotten `await` makes the probe pass unconditionally, and a wrong status code is just a typo. In Rust the compiler rejects the forgotten `await` and forces every response branch into a consistent, typed shape. The runtime cost of a check is also far lower (no event-loop scheduling overhead, no GC pause skewing your probe latency).

---

## Common Pitfalls

### Pitfall 1: Forgetting `.await` on a dependency check

In JavaScript, calling an `async` function without `await` yields a pending `Promise`, which is truthy, so a health check like `if (db.ping()) ...` "passes" forever. Rust catches this at compile time. This program:

```rust
// does not compile (error[E0599])
use std::time::Duration;

async fn db_ping() -> Result<(), String> {
    tokio::time::sleep(Duration::from_millis(1)).await;
    Ok(())
}

#[tokio::main]
async fn main() {
    // Forgot `.await` — `db_ping()` is a Future, not a Result.
    let healthy = db_ping().is_ok();
    println!("{healthy}");
}
```

produces the real error:

```text
error[E0599]: no method named `is_ok` found for opaque type `impl Future<Output = Result<(), String>>` in the current scope
  --> src/main.rs:11:29
   |
11 |     let healthy = db_ping().is_ok();
   |                             ^^^^^ method not found in `impl Future<Output = Result<(), String>>`
   |
help: consider `await`ing on the `Future` and calling the method on its `Output`
   |
11 |     let healthy = db_ping().await.is_ok();
   |                             ++++++
```

The fix is exactly what the compiler suggests: `db_ping().await.is_ok()`.

### Pitfall 2: Inconsistent response types across branches

Returning a bare `StatusCode` from one branch and a `(StatusCode, Json<_>)` tuple from another does not compile, because `impl IntoResponse` must resolve to a single concrete type:

```rust
// does not compile (error[E0308])
use axum::{Json, http::StatusCode, response::IntoResponse};
use serde_json::json;

async fn readiness(ready: bool) -> impl IntoResponse {
    if !ready {
        // This branch returns a bare StatusCode...
        return StatusCode::SERVICE_UNAVAILABLE;
    }
    // ...but this one returns a (StatusCode, Json) tuple — different types.
    (StatusCode::OK, Json(json!({ "status": "ready" })))
}

fn main() {
    let _ = readiness(true);
}
```

The real message points right at the mismatch:

```text
error[E0308]: mismatched types
  --> src/main.rs:10:5
   |
 4 | async fn readiness(ready: bool) -> impl IntoResponse {
   |                                    ----------------- expected `StatusCode` because of return type
...
10 |     (StatusCode::OK, Json(json!({ "status": "ready" })))
   |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ expected `StatusCode`, found `(StatusCode, Json<Value>)`
   |
   = note: expected struct `StatusCode`
               found tuple `(StatusCode, Json<Value>)`
```

The fix: make the early return a tuple too — `return (StatusCode::SERVICE_UNAVAILABLE, Json(...));`. axum's `Response` type erases the body, but `impl IntoResponse` does not, so consistency across branches is required. (Returning `Response` explicitly via `.into_response()` on each branch is the escape hatch when branches truly differ.)

### Pitfall 3: Liveness that touches a dependency

The most damaging pitfall compiles fine and passes review. It is a design mistake. If `/health/live` runs `SELECT 1`, a transient database blip makes liveness fail, and the orchestrator restarts every pod simultaneously, turning a recoverable dependency outage into a full outage with a cold-start stampede. Keep liveness dependency-free; put dependency checks only in readiness.

### Pitfall 4: No timeout on a check

`state.db.ping().await` without a `timeout` wrapper means a single hung connection hangs the probe. The orchestrator's probe timeout then fires, the liveness check (if you wired it wrong) fails, and the process is killed. Always wrap dependency calls in `tokio::time::timeout` with a deadline shorter than the orchestrator's probe timeout.

---

## Best Practices

- **Separate the routes.** Expose `/health/live` and `/health/ready` (or `/healthz` and `/readyz` if you follow Kubernetes convention). Never reuse one path for both.
- **Keep liveness trivial.** Return `200` unconditionally, or at most check an in-process invariant (e.g., a critical background task has not panicked). No I/O.
- **Probe dependencies concurrently and with timeouts.** Use `tokio::join!` plus `tokio::time::timeout` so one slow dependency cannot dominate or hang the probe.
- **Report per-dependency detail.** A `503` body listing *which* dependency failed turns a page into a diagnosis. Skip the `detail` field on healthy checks (`skip_serializing_if`).
- **Use a cheap query.** `SELECT 1` for SQL, `PING` for Redis. Do not run an expensive query in a probe that the load balancer hits every few seconds.
- **Cache readiness for a short TTL** when probe frequency is high, so a burst of probes does not become a burst of database round-trips (see Exercise 3).
- **Wire readiness into shutdown.** Flip the `ready` flag to `false` the moment you receive `SIGTERM`, then sleep briefly before closing the listener, so the load balancer notices and drains you. See [graceful shutdown](/28-production/02-graceful-shutdown/).
- **Do not authenticate the liveness probe.** The orchestrator that calls it usually cannot present credentials; keep these endpoints unauthenticated but bound to the internal interface, or behind the orchestrator's network policy.

---

## Real-World Example

A production-shaped service: it starts up asynchronously (so readiness returns `503` until ready), then serves liveness and readiness. This is the complete, runnable program behind the output shown earlier. Copy it into `src/main.rs` of a project with the dependencies listed above.

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::get,
};
use serde::Serialize;
use serde_json::json;

#[derive(Clone)]
struct Db;
impl Db {
    async fn ping(&self) -> Result<(), String> {
        tokio::time::sleep(Duration::from_millis(3)).await;
        Ok(())
    }
}

#[derive(Clone)]
struct Cache;
impl Cache {
    async fn ping(&self) -> Result<(), String> {
        tokio::time::sleep(Duration::from_millis(2)).await;
        Ok(())
    }
}

#[derive(Clone)]
struct AppState {
    db: Db,
    cache: Cache,
    ready: Arc<AtomicBool>,
}

#[derive(Serialize)]
struct CheckResult {
    name: &'static str,
    healthy: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

fn to_result(name: &'static str, r: Result<(), String>) -> CheckResult {
    match r {
        Ok(()) => CheckResult { name, healthy: true, detail: None },
        Err(e) => CheckResult { name, healthy: false, detail: Some(e) },
    }
}

async fn liveness() -> impl IntoResponse {
    (StatusCode::OK, Json(json!({ "status": "ok" })))
}

async fn readiness(State(state): State<AppState>) -> impl IntoResponse {
    if !state.ready.load(Ordering::Relaxed) {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "status": "starting", "checks": [] })),
        );
    }

    let deadline = Duration::from_secs(2);
    let (db_res, cache_res) = tokio::join!(
        tokio::time::timeout(deadline, state.db.ping()),
        tokio::time::timeout(deadline, state.cache.ping()),
    );

    let flatten = |r: Result<Result<(), String>, tokio::time::error::Elapsed>| match r {
        Ok(inner) => inner,
        Err(_) => Err("timed out".to_string()),
    };

    let checks = vec![
        to_result("database", flatten(db_res)),
        to_result("cache", flatten(cache_res)),
    ];
    let all_ok = checks.iter().all(|c| c.healthy);

    let code = if all_ok { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE };
    (
        code,
        Json(json!({
            "status": if all_ok { "ready" } else { "degraded" },
            "checks": checks,
        })),
    )
}

fn app(state: AppState) -> Router {
    Router::new()
        .route("/health/live", get(liveness))
        .route("/health/ready", get(readiness))
        .with_state(state)
}

#[tokio::main]
async fn main() {
    let state = AppState {
        db: Db,
        cache: Cache,
        ready: Arc::new(AtomicBool::new(false)),
    };

    // Simulate async startup work: readiness stays 503 until this finishes.
    let ready_flag = state.ready.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await; // migrations, warm pools
        ready_flag.store(true, Ordering::Relaxed);
        println!("startup complete: now ready");
    });

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8772").await.unwrap();
    println!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app(state)).await.unwrap();
}
```

This is the program whose verified output appears in the Rust Equivalent section: `/health/live` returns `200 {"status":"ok"}`, and `/health/ready` returns `200` with a `"ready"` status once the 50 ms startup task has flipped the flag (and `503 "starting"` before that). In a real service, replace `Db`/`Cache` with your `sqlx::PgPool` and Redis client and `.ping()` with `SELECT 1` / `PING`.

---

## Further Reading

- [axum `IntoResponse` docs](https://docs.rs/axum/latest/axum/response/trait.IntoResponse.html) — every response shape the handlers rely on.
- [`tokio::time::timeout`](https://docs.rs/tokio/latest/tokio/time/fn.timeout.html) and [`tokio::join!`](https://docs.rs/tokio/latest/tokio/macro.join.html) — bounding and parallelizing checks.
- [Kubernetes: Configure Liveness, Readiness and Startup Probes](https://kubernetes.io/docs/tasks/configure-pod-container/configure-liveness-readiness-startup-probes/) — how an orchestrator actually consumes these endpoints.
- Guide cross-links:
  - [Graceful Shutdown](/28-production/02-graceful-shutdown/) — flip readiness to `false` on `SIGTERM` and drain in-flight requests.
  - [Configuration](/28-production/00-configuration/) and [Environment-Based Config](/28-production/01-environment/) — where probe paths, timeouts, and dependency URLs come from.
  - [Metrics and Monitoring](/28-production/04-metrics/) and [Distributed Tracing](/28-production/05-distributed-tracing/) — observability beyond a binary up/down signal.
  - [Production Readiness Checklist](/28-production/09-production-checklist/) — health checks in the context of timeouts, limits, and logging.
  - [Section 16: Web APIs](/16-web-apis/) — axum routing, extractors, and state, used throughout this chapter.
  - [Section 17: Database](/17-database/) — the real `sqlx`/`PgPool` clients these examples stand in for.
  - [Section 11: Async](/11-async/) — why Rust futures are lazy and how `tokio::join!`/`timeout` work.
  - [Section 08: Error Handling](/08-error-handling/) — modeling check outcomes as `Result`.
  - [Section 29: Migration Guide](/29-migration-guide/) — porting an existing Node health endpoint to axum.

---

## Exercises

### Exercise 1: Add a startup probe

**Difficulty:** Beginner

**Objective:** Distinguish "still starting up" from "running but a dependency is down" so an orchestrator with a long startup grace period treats them differently.

**Instructions:** Add a third endpoint, `/health/startup`, that returns `200` only once `state.ready` is `true`, and `503` otherwise. (Kubernetes uses a startup probe to give slow-booting apps extra time before the liveness probe takes over.) Reuse the `AppState` from the chapter.

<details>
<summary>Solution</summary>

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::get,
};
use serde_json::json;

#[derive(Clone)]
struct AppState {
    ready: Arc<AtomicBool>,
}

async fn startup(State(state): State<AppState>) -> impl IntoResponse {
    if state.ready.load(Ordering::Relaxed) {
        (StatusCode::OK, Json(json!({ "status": "started" })))
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "status": "starting" })),
        )
    }
}

fn app(state: AppState) -> Router {
    Router::new()
        .route("/health/startup", get(startup))
        .with_state(state)
}

#[tokio::main]
async fn main() {
    let state = AppState { ready: Arc::new(AtomicBool::new(false)) };

    let ready_flag = state.ready.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        ready_flag.store(true, Ordering::Relaxed);
    });

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    axum::serve(listener, app(state)).await.unwrap();
}
```

Both arms return `(StatusCode, Json<Value>)`, so `impl IntoResponse` resolves to one type — the same discipline as the readiness handler.

</details>

### Exercise 2: A trait-based check registry

**Difficulty:** Intermediate

**Objective:** Replace hand-written per-dependency code with a reusable `HealthCheck` abstraction so adding a dependency is one line.

**Instructions:** Define a `HealthCheck` trait with a `name(&self) -> &'static str` and an async `check(&self) -> Result<(), String>`. Implement it for a `DbCheck` type. Write a generic `probe` function that wraps any `HealthCheck` in a timeout and returns `(bool, Option<String>)`. (Native `async fn` in traits works directly for *generic* dispatch; you only need boxing or a helper crate for a heterogeneous `Vec<Box<dyn HealthCheck>>`.)

<details>
<summary>Solution</summary>

```rust
use std::time::Duration;

// Native `async fn` in traits is stable. It works directly for STATIC dispatch
// (generics). For a heterogeneous `Vec<Box<dyn HealthCheck>>` you would box the
// returned futures yourself or pull in the `trait-variant` crate.
trait HealthCheck {
    fn name(&self) -> &'static str;
    async fn check(&self) -> Result<(), String>;
}

struct DbCheck {
    healthy: bool,
}

impl HealthCheck for DbCheck {
    fn name(&self) -> &'static str {
        "database"
    }
    async fn check(&self) -> Result<(), String> {
        tokio::time::sleep(Duration::from_millis(5)).await; // imagine SELECT 1
        if self.healthy {
            Ok(())
        } else {
            Err("connection refused".into())
        }
    }
}

async fn probe<C: HealthCheck>(c: &C, deadline: Duration) -> (bool, Option<String>) {
    match tokio::time::timeout(deadline, c.check()).await {
        Ok(Ok(())) => (true, None),
        Ok(Err(e)) => (false, Some(e)),
        Err(_) => (false, Some("timed out".into())),
    }
}

#[tokio::main]
async fn main() {
    let db = DbCheck { healthy: true };
    let (ok, detail) = probe(&db, Duration::from_secs(2)).await;
    println!("{} healthy={ok} detail={detail:?}", db.name());
}
```

Running it prints `database healthy=true detail=None`. Flip `healthy` to `false` and you get `database healthy=false detail=Some("connection refused")`.

</details>

### Exercise 3: Cache the readiness result

**Difficulty:** Advanced

**Objective:** Stop a high-frequency probe (or a noisy load balancer) from turning into a flood of database round-trips, while keeping readiness reasonably fresh.

**Instructions:** Build a `CachedReadiness` type that stores the last result with an `Instant` timestamp behind a `tokio::sync::Mutex`. Its `is_ready` method takes a closure producing the *fresh* check; if the cached value is younger than a TTL, return it without running the closure, otherwise run the closure and update the cache. Prove that three rapid calls within the TTL only probe the dependency once.

<details>
<summary>Solution</summary>

```rust
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

#[derive(Clone)]
struct CachedReadiness {
    ttl: Duration,
    inner: Arc<Mutex<Option<(Instant, bool)>>>,
}

impl CachedReadiness {
    fn new(ttl: Duration) -> Self {
        Self { ttl, inner: Arc::new(Mutex::new(None)) }
    }

    /// Returns a cached value if fresh; otherwise runs `check` and caches it.
    async fn is_ready<F, Fut>(&self, check: F) -> bool
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        let mut guard = self.inner.lock().await;
        if let Some((at, value)) = *guard {
            if at.elapsed() < self.ttl {
                return value;
            }
        }
        let fresh = check().await;
        *guard = Some((Instant::now(), fresh));
        fresh
    }
}

#[tokio::main]
async fn main() {
    let cache = CachedReadiness::new(Duration::from_secs(5));
    let mut calls = 0u32;

    for _ in 0..3 {
        let ready = cache
            .is_ready(|| {
                calls += 1; // counts real dependency probes
                async { true }
            })
            .await;
        println!("ready={ready}");
    }
    println!("dependency was actually probed {calls} time(s)");
}
```

Output:

```text
ready=true
ready=true
ready=true
dependency was actually probed 1 time(s)
```

Holding the `Mutex` across the `check().await` also collapses a *concurrent* burst into a single probe (later callers wait for the in-flight one and then see the fresh cached value). If you would rather not serialize callers during the refresh, swap in an `RwLock` or a single-flight primitive — but for a check that takes a few milliseconds, the simple mutex is usually the right trade-off.

</details>
