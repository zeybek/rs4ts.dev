---
title: "Incremental Migration: The Strangler-Fig Approach"
description: "Rewriting a Node.js system in Rust at once usually fails. The strangler-fig pattern moves one hot route at a time behind a proxy, with instant rollback."
---

Rewriting a working Node.js system in Rust as one big "stop the world" project is the most reliable way to fail. Incremental migration lets you move one route, one service, or one hot path at a time while production keeps serving traffic. This page covers the **strangler-fig pattern**, going **service-by-service**, and the highest-payoff starting point: porting **hot paths first**.

---

## Quick Overview

**Incremental migration** means the old Node.js system and the new Rust system run side by side, with a router or proxy in front deciding which one handles each request. Over time you "strangle" the legacy system by moving more traffic to Rust, one slice at a time, with the ability to roll back at any point.

For a TypeScript/JavaScript developer this should feel familiar: it is the same playbook you use when you split a monolith into services, or move a route from an old Express app to a new one behind an API gateway. The new ingredient is *language boundaries* instead of just *module boundaries*, and the discipline of choosing slices small enough that each one ships independently.

> **Note:** This page is about *strategy and sequencing*. The mechanical "how do I port this Express endpoint to Axum" walkthrough lives in [Porting a Node.js Service to Rust](/29-migration-guide/01-node-to-rust/); keeping the JSON/headers identical is covered in [Maintaining API Compatibility During Migration](/29-migration-guide/02-api-compatibility/); sharing a database during the transition is in [Data Migration Strategies](/29-migration-guide/03-data-migration/).

---

## TypeScript/JavaScript Example

Here is a typical Express monolith. Most routes are cheap I/O, but `/price/:symbol` does a CPU-bound calculation on every request and shows up at the top of every flame graph. That is the slice worth moving first.

```typescript
// legacy/server.ts — the existing Node.js monolith (Express 5, Node 22)
import express, { Request, Response } from "express";

const app = express();
app.use(express.json());

// Cheap I/O routes — fine on Node, no reason to touch them yet.
app.get("/health", (_req, res) => res.json({ status: "ok" }));
app.get("/users/:id", async (req, res) => {
  const user = await db.users.findById(req.params.id);
  res.json(user);
});

// The HOT PATH: CPU-bound, runs on every request, dominates p99 latency.
app.get("/price/:symbol", (req: Request, res: Response) => {
  const symbol = req.params.symbol;
  const base = { AAPL: 190.0, MSFT: 410.0 }[symbol];
  if (base === undefined) {
    return res.status(404).json({ error: "unknown symbol" });
  }
  // Single-threaded JS event loop: this blocks every other request while it runs.
  let adjustment = 0;
  for (let n = 1; n <= 1000; n++) adjustment += Math.sqrt(n);
  adjustment /= 1_000_000;
  res.json({ symbol, price: base + adjustment, computedBy: "node" });
});

app.listen(3000, () => console.log("legacy node service on :3000"));
```

The problem is structural: Node runs JavaScript on a single event-loop thread, so a CPU-bound loop like the pricing calculation **blocks the entire process** while it runs. Under load, every other request — even `/health` — waits behind it. You could spawn worker threads, but that is most of the complexity of a separate service with none of the benefits.

---

## Rust Equivalent

The strangler-fig move is: stand up a small Axum service that owns **only** the hot path, and put a proxy in front that routes `/price/*` to Rust and everything else to the untouched Node app.

```rust
// pricing-service/src/main.rs — the new Rust slice (one route only)
// Cargo.toml dependencies:
//   axum = "0.8"
//   tokio = { version = "1", features = ["full"] }
//   serde = { version = "1", features = ["derive"] }
use axum::{extract::Path, http::StatusCode, routing::get, Json, Router};
use serde::Serialize;

#[derive(Serialize)]
struct Price {
    symbol: String,
    price: f64,
    #[serde(rename = "computedBy")]
    computed_by: &'static str,
}

async fn get_price(Path(symbol): Path<String>) -> Result<Json<Price>, StatusCode> {
    let price = compute_price(&symbol).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(Price { symbol, price, computed_by: "rust" }))
}

fn compute_price(symbol: &str) -> Option<f64> {
    let base = match symbol {
        "AAPL" => 190.0,
        "MSFT" => 410.0,
        _ => return None,
    };
    // The same CPU-bound work — but it runs on a thread pool, not a single event loop,
    // so it never blocks unrelated requests.
    let adjustment: f64 = (1..=1000).map(|n| (n as f64).sqrt()).sum::<f64>() / 1_000_000.0;
    Some(base + adjustment)
}

#[tokio::main]
async fn main() {
    let app: Router = Router::new().route("/price/{symbol}", get(get_price));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3001").await.unwrap();
    println!("rust pricing service on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
```

Running the calculation directly confirms behavior matches the Node version:

```text
rust pricing service on 127.0.0.1:3001
AAPL computed by rust = 190.0211
```

The two services now coexist. A single proxy route — described below — sends `/price/*` to port 3001 (Rust) and leaves `/health`, `/users/*`, and everything else on port 3000 (Node). You have strangled exactly one branch of the legacy tree, and you can promote, throttle, or roll it back independently of the rest of the app.

---

## Detailed Explanation

### The strangler fig

The name comes from a vine that grows around a host tree, gradually taking over until the original tree is gone but the shape it occupied remains. Martin Fowler popularized it for software: instead of replacing a system in one cutover, you wrap it, intercept calls at the edge, and reroute them to new code one at a time. The legacy system shrinks until it is empty, and at no point is there a risky "big bang" release.

The three pieces you always need:

1. **An interception point**: a reverse proxy, API gateway, or thin routing layer that sees every request and decides old-vs-new. In a TypeScript shop this is often Nginx, an existing API gateway, or even a small Express app that proxies.
2. **The new implementation**: the Rust service owning a narrow, well-defined slice.
3. **A way back**: feature flags or proxy config that lets you flip a slice back to Node in seconds if metrics regress.

### A minimal interception point in Node

You do not need new infrastructure to start. The existing Express app can proxy the carved-out routes to Rust, which means the migration is invisible to clients:

```typescript
// legacy/server.ts — add a proxy for the carved-out slice
import { createProxyMiddleware } from "http-proxy-middleware"; // npm i http-proxy-middleware

const RUST_PRICING = process.env.PRICING_BACKEND === "rust";

if (RUST_PRICING) {
  // Strangle /price/* — forward it to the Rust service on :3001.
  app.use(
    "/price",
    createProxyMiddleware({ target: "http://127.0.0.1:3001", changeOrigin: true }),
  );
} else {
  // Fall back to the original in-process handler (kept for instant rollback).
  app.get("/price/:symbol", legacyPriceHandler);
}
```

The `PRICING_BACKEND` env var (or a real feature flag) *is* your rollback switch. Flip it and traffic returns to the battle-tested Node path with no redeploy of the Rust service required.

### Why the Rust version doesn't block

The Node handler runs on the event-loop thread, so its `for` loop monopolizes the CPU and stalls concurrent requests. The Axum handler is an `async fn` scheduled by the Tokio runtime across a multi-threaded worker pool. The CPU work in `compute_price` runs on one worker while other workers keep serving requests.

> **Tip:** Truly heavy, long-running CPU work (hundreds of milliseconds) can still hog a Tokio worker. For that, offload to a blocking pool with `tokio::task::spawn_blocking` or a [`rayon`](/21-performance/) pool. The 1,000-iteration loop here is far below that threshold, so the plain `async fn` is correct and simplest.

### `computedBy` — proving which backend served the request

Both responses include a `computedBy` field (`"node"` vs `"rust"`). This is a small but invaluable migration trick: it lets dashboards and integration tests see, per request, which implementation answered, so you can confirm a slice is actually live and catch accidental fallbacks. The `#[serde(rename = "computedBy")]` attribute keeps the Rust struct idiomatic (`snake_case` field) while emitting the exact `camelCase` key the old API used. Matching JSON shapes precisely is the whole subject of [Maintaining API Compatibility During Migration](/29-migration-guide/02-api-compatibility/).

---

## Key Differences

| Concern | TypeScript/Node monolith | Incremental Rust migration |
| --- | --- | --- |
| **Unit of change** | A module or route in one process | A *service* (or route) in a separate process behind a proxy |
| **Cutover** | Often a single redeploy | Per-slice; traffic shifts gradually |
| **Rollback** | Revert the commit, redeploy | Flip a proxy rule / feature flag; seconds, no redeploy |
| **CPU-bound work** | Blocks the single event loop | Runs across a thread pool; does not block other requests |
| **Boundary type** | Function/module call (same runtime) | Network or FFI call (cross-runtime) |
| **Risk profile** | All-or-nothing on each release | Bounded to one slice at a time |

The conceptual shift for a TS/JS developer: in a Node refactor your boundaries are *module imports*, and a broken refactor takes the whole process with it. In an incremental Rust migration your boundaries are *processes behind a proxy*, so a broken slice is contained — the proxy can route around it, and the rest of the app never noticed it existed.

> **Note:** Unlike a pure TypeScript refactor where the new and old code share types at compile time, the Node and Rust sides share *nothing* at compile time. The contract between them is the HTTP/JSON wire format, which neither compiler checks. That contract must be pinned by tests (contract tests, golden JSON fixtures); see [Maintaining API Compatibility During Migration](/29-migration-guide/02-api-compatibility/).

---

## Common Pitfalls

### Pitfall 1: Migrating the easy code first instead of the valuable code

The instinct is to port `/health` or some trivial CRUD route to "learn Rust safely." That teaches the team syntax but delivers no measurable win, and stakeholders see a rewrite with nothing to show. **Migrate the hot path first**: the route that dominates CPU, latency, or cost. That is where Rust's advantage is largest and where a win is easy to demonstrate. Identifying *which* path is hot must come from profiling, not hunches; measure honestly per [Measuring Performance Gains Honestly](/29-migration-guide/04-performance-gains/).

### Pitfall 2: Carrying Node's `String`-everywhere habits into Rust

In JavaScript you pass strings and objects around freely because everything is a shared reference and the garbage collector cleans up. Porting that style literally fights the borrow checker. A function that takes ownership of a `String` consumes it, so a TS dev who "reuses the request body" gets a real error:

```rust
fn process(payload: String) -> usize {
    payload.len()
}

fn main() {
    let payload = String::from("{\"id\":1}");
    let len = process(payload);   // payload MOVED here
    println!("len = {len}");
    println!("raw = {payload}");  // does not compile (error[E0382]: borrow of moved value: `payload`)
}
```

The real compiler output:

```text
error[E0382]: borrow of moved value: `payload`
 --> src/main.rs:9:22
  |
6 |     let payload = String::from("{\"id\":1}");
  |         ------- move occurs because `payload` has type `String`, which does not implement the `Copy` trait
7 |     let len = process(payload);   // payload MOVED here
  |                       ------- value moved here
8 |     println!("len = {len}");
9 |     println!("raw = {payload}");  // does not compile (error[E0382]: borrow of moved value: `payload`)
  |                      ^^^^^^^ value borrowed here after move
  |
note: consider changing this parameter type in function `process` to borrow instead if owning the value isn't necessary
 --> src/main.rs:1:21
  |
1 | fn process(payload: String) -> usize {
  |    -------          ^^^^^^ this parameter takes ownership of the value
  |    |
  |    in this function
  = note: this error originates in the macro `$crate::format_args_nl` which comes from the expansion of the macro `println` (in Nightly builds, run with -Z macro-backtrace for more info)
help: consider cloning the value if the performance cost is acceptable
  |
7 |     let len = process(payload.clone());   // payload MOVED here
  |                              ++++++++
```

Note the two suggestions the compiler offers: a `note` to change `process` to borrow, and a `help` to insert `.clone()`. Borrowing is the better fix here — make `process` take `&str`. Do not reach for the `.clone()` suggestion reflexively just to silence the borrow checker; that reintroduces the allocations you migrated to Rust to avoid. The ownership learning curve is the single most common drag on migration velocity; it is covered in depth in [Common Migration Challenges](/29-migration-guide/05-common-challenges/) and the [ownership section](/05-ownership/).

### Pitfall 3: Using stale framework syntax from blog posts

Axum changed its route-parameter syntax in 0.8: the old `:symbol` colon form is gone, replaced by `{symbol}` braces. If you copy a pre-0.8 example, it compiles fine and then **panics at startup**:

```rust
// panics at startup on axum 0.8 — old 0.7 colon syntax
let app: Router = Router::new().route("/price/:symbol", get(handler));
```

The real panic:

```text
thread 'main' panicked at src/main.rs:8:37:
Path segments must not start with `:`. For capture groups, use `{capture}`. If you meant to literally match a segment starting with a colon, call `without_v07_checks` on the router.
```

Use `"/price/{symbol}"`. The broader lesson for a migration: **resolve the current API before you write it.** Crates move fast (the axum 0.7→0.8 break is a perfect example), so `cargo add <crate>` in a scratch project to pin the live version and check docs.rs rather than trusting a search-result snippet.

### Pitfall 4: No rollback path

A slice you cannot instantly route back to Node is not an incremental migration; it is a series of small big-bang releases. Always keep the legacy handler in place (or one commit away) and gate the cutover behind a proxy rule or feature flag until the Rust slice has proven itself in production over time.

---

## Best Practices

- **Profile, then pick the slice.** Let CPU/latency/cost data choose the first target. The best slice is hot, has a stable contract, and is loosely coupled to the rest of the system.
- **Keep slices small and shippable.** A slice should be a single route or a small cohesive service you can deploy, observe, and roll back on its own. If it takes a month to carve out, it is too big.
- **Make the contract explicit and tested.** The Node↔Rust boundary is the wire format. Pin it with golden-JSON fixtures and contract tests so a refactor on either side cannot silently break the shape; details in [Maintaining API Compatibility During Migration](/29-migration-guide/02-api-compatibility/).
- **Tag responses by backend.** A `computedBy` / `served-by` field (or a response header) makes "which implementation answered?" observable per request and is gold for debugging a partial migration.
- **Ship behind a flag; shadow before you cut over.** Run the Rust slice in shadow mode (mirror traffic, compare outputs, ignore its responses) until it matches Node, then flip real traffic.
- **Share the database during the transition.** Two services hitting one schema is usually the right intermediate state; do not migrate the data store and the language at the same time. See [Data Migration Strategies](/29-migration-guide/03-data-migration/).
- **Pin live crate versions.** `cargo add` in a probe project and check docs.rs so you write against the current API, not a stale one.

---

## Real-World Example

A team is strangling the order-checkout path out of a Node monolith. The Rust slice must accept the **exact** JSON the Node endpoint accepted and emit the **exact** JSON it emitted — clients and the proxy must not be able to tell the difference. Serde's `rename` attributes bridge idiomatic Rust field names to the legacy `camelCase` wire shape.

```rust playground
// order-service/src/main.rs — a strangled checkout slice, wire-compatible with the old Node route
// Cargo.toml: serde = { version = "1", features = ["derive"] }, serde_json = "1"
use serde::{Deserialize, Serialize};

// Request shape: must match what the legacy Node service already accepts.
#[derive(Debug, Deserialize)]
struct OrderRequest {
    #[serde(rename = "userId")]
    user_id: u64,
    items: Vec<LineItem>,
}

#[derive(Debug, Deserialize)]
struct LineItem {
    sku: String,
    qty: u32,
}

// Response shape: must serialize to the SAME JSON the Node service produced.
#[derive(Debug, Serialize)]
struct OrderResponse {
    #[serde(rename = "orderId")]
    order_id: String,
    total: u64, // cents
    status: &'static str,
}

fn price_of(sku: &str) -> u64 {
    match sku {
        "BOOK-1" => 1500,
        "PEN-9" => 200,
        _ => 0,
    }
}

fn handle_order(body: &str) -> Result<String, serde_json::Error> {
    let req: OrderRequest = serde_json::from_str(body)?;
    let total = req.items.iter().map(|i| price_of(&i.sku) * i.qty as u64).sum();
    let resp = OrderResponse {
        order_id: format!("ord_{}", req.user_id),
        total,
        status: "confirmed",
    };
    serde_json::to_string(&resp)
}

fn main() {
    // The exact bytes the proxy forwards from a real client request.
    let incoming = r#"{"userId": 42, "items": [{"sku": "BOOK-1", "qty": 2}, {"sku": "PEN-9", "qty": 3}]}"#;
    let out = handle_order(incoming).unwrap();
    println!("{out}");
}
```

Real output, confirming the wire shape matches what clients expect:

```text
{"orderId":"ord_42","total":3600,"status":"confirmed"}
```

Note the asymmetry that makes this safe: the Rust code reads naturally (`user_id`, `order_id`) while the JSON on the wire is byte-for-byte what the Node service produced. A contract test that asserts on that exact output string is what lets you flip the proxy from Node to Rust with confidence — and flip it back just as fast. Going from this single function to a full Axum endpoint that mirrors status codes and headers is the worked example in [Porting a Node.js Service to Rust](/29-migration-guide/01-node-to-rust/) and [Maintaining API Compatibility During Migration](/29-migration-guide/02-api-compatibility/).

---

## Further Reading

- Martin Fowler, ["StranglerFigApplication"](https://martinfowler.com/bliki/StranglerFigApplication.html): the original pattern writeup.
- [Axum documentation](https://docs.rs/axum/latest/axum/): the framework used for the Rust slices here.
- [The Tokio runtime](https://tokio.rs/): why CPU work no longer blocks unrelated requests.
- Sibling pages in this section:
  - [Porting a Node.js Service to Rust](/29-migration-guide/01-node-to-rust/) — the mechanical Express→Axum port for a single endpoint.
  - [Maintaining API Compatibility During Migration](/29-migration-guide/02-api-compatibility/) — matching JSON shapes, status codes, and headers exactly.
  - [Data Migration Strategies](/29-migration-guide/03-data-migration/) — shared DB, dual-write, and backfill during a rewrite.
  - [Measuring Performance Gains Honestly](/29-migration-guide/04-performance-gains/) — measuring the win honestly (latency percentiles, memory).
  - [Common Migration Challenges](/29-migration-guide/05-common-challenges/) — the ownership learning curve, ecosystem gaps, and when *not* to migrate.
- Foundations referenced above: [Section 05: Ownership](/05-ownership/), [Section 21: Performance](/21-performance/), and the [Getting Started intro](/01-getting-started/) for `cargo` and `cargo add` basics.
- Apply the pattern end to end in [Section 30: Projects](/30-projects/).

---

## Exercises

### Exercise 1: Pick the first slice

**Difficulty:** Beginner

**Objective:** Practice choosing a migration target by value, not by ease.

**Instructions:** Given the route table below from a Node service, rank the routes by how good a *first* strangler-fig slice they make, and justify the top choice in one or two sentences. Consider request volume, CPU cost, and how stable/coupled the route's contract is.

| Route | Calls/sec | Per-call CPU | Contract |
| --- | --- | --- | --- |
| `GET /health` | 5 | trivial | frozen |
| `GET /users/:id` | 200 | low (one DB read) | stable |
| `POST /reports/render` | 8 | very high (PDF generation) | stable |
| `GET /feed` | 150 | medium, changes often | churning weekly |

<details>
<summary>Solution</summary>

Ranking (best first slice first): **`POST /reports/render`**, then `GET /users/:id`, then `GET /health`, and *not* `GET /feed` yet.

- **`POST /reports/render`** is the best first slice: it is the most CPU-heavy route, so Rust's advantage is largest and a win is easy to demonstrate, and its contract is stable so the wire format is a fixed target. Low call volume even makes it low-risk to flip.
- **`GET /users/:id`** is high-volume but cheap I/O. Rust helps less here, and it touches the database, so you would also be dragging in [Data Migration Strategies](/29-migration-guide/03-data-migration/) concerns. A reasonable *second* slice, not the first.
- **`GET /health`** is trivial and frozen: zero value to migrate; leave it on Node.
- **`GET /feed`** has a churning contract. Migrating a moving target means re-porting weekly. Migrate it *after* its contract settles, never first.

The headline rule from this page: migrate the **hot path first**, provided its contract is stable enough to pin with tests.

</details>

### Exercise 2: Make the borrowing version compile

**Difficulty:** Intermediate

**Objective:** Fix the ownership pitfall from this page the idiomatic way (borrow, do not clone).

**Instructions:** The following snippet fails with `error[E0382]: borrow of moved value`. Change it so it compiles and prints both lines, *without* using `.clone()` and without changing `main`'s body beyond what is necessary. Then explain in one sentence why borrowing is the right fix during a migration.

```rust
fn process(payload: String) -> usize {
    payload.len()
}

fn main() {
    let payload = String::from("{\"id\":1}");
    let len = process(payload);   // moves payload
    println!("len = {len}");
    println!("raw = {payload}");
}
```

<details>
<summary>Solution</summary>

Change the parameter to a borrow (`&str`) and pass `&payload`. The caller keeps ownership, so `payload` is still usable afterward.

```rust playground
fn process(payload: &str) -> usize {
    payload.len()
}

fn main() {
    let payload = String::from("{\"id\":1}");
    let len = process(&payload);  // borrow — payload not moved
    println!("len = {len}");
    println!("raw = {payload}");  // still valid
}
```

Output:

```text
len = 8
raw = {"id":1}
```

Why borrowing, not cloning: in JavaScript every value is effectively a shared reference, so the natural port is "pass a reference," which in Rust is `&str`/`&T`. Reaching for `.clone()` instead reintroduces the heap allocations and copies you migrated to Rust to eliminate — it makes the borrow checker happy while quietly throwing away the performance win.

</details>

### Exercise 3: Add a rollback switch

**Difficulty:** Advanced

**Objective:** Build the "way back" that makes a strangler-fig migration safe.

**Instructions:** Extend the proxy snippet from the Detailed Explanation so that, instead of an all-or-nothing `PRICING_BACKEND=rust|node`, you can send a **percentage** of `/price/*` traffic to Rust (canary rollout) and keep the rest on the in-process Node handler. Sketch the routing logic in TypeScript and describe how you would roll back. (Pseudocode for the percentage decision is fine.)

<details>
<summary>Solution</summary>

Read a percentage from config and make a per-request decision; everything below the threshold goes to Rust, everything else falls through to the legacy handler.

```typescript
// legacy/server.ts — canary rollout for the strangled /price slice
import { createProxyMiddleware } from "http-proxy-middleware";

// 0 = all Node, 100 = all Rust. Change this value (or back it with a feature flag) to roll forward/back.
const RUST_PCT = Number(process.env.PRICING_RUST_PCT ?? "0");

const toRust = createProxyMiddleware({
  target: "http://127.0.0.1:3001",
  changeOrigin: true,
});

app.use("/price", (req, res, next) => {
  // Per-request canary decision. For sticky routing, hash a stable key
  // (e.g. the symbol or a user id) instead of Math.random().
  const pick = Math.random() * 100;
  if (pick < RUST_PCT) {
    return toRust(req, res, next); // send this request to the Rust service
  }
  return next(); // fall through to the legacy in-process handler below
});

// Legacy handler stays mounted as the fallback for the non-canary share.
app.get("/price/:symbol", legacyPriceHandler);
```

**Rollback:** set `PRICING_RUST_PCT=0` (or flip the feature flag). Every request immediately falls through to `legacyPriceHandler` with no redeploy of either service. Because the Node handler was never removed, the way back is always one config change away, which is the property that makes the whole incremental migration safe.

**Roll forward:** ramp `PRICING_RUST_PCT` from 1 → 5 → 25 → 100 while watching latency percentiles and error rates ([Measuring Performance Gains Honestly](/29-migration-guide/04-performance-gains/)), and confirm via the `computedBy` field that the expected share is actually being served by Rust.

> **Tip:** For a *consistent* user experience, hash a stable key (symbol, user id) instead of `Math.random()` so a given key always lands on the same backend within a rollout step.

</details>
