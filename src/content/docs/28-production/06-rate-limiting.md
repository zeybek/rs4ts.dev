---
title: "Rate Limiting"
description: "Cap requests per client in Rust with tower-governor's GCRA token bucket, a Tower layer. Smoother than Express's express-rate-limit, with explicit per-IP keys."
---

A public endpoint that anyone can call as fast as they like is a denial-of-service waiting to happen. **Rate limiting** caps how many requests a given client may make in a window of time, protecting your service (and the databases and third-party APIs behind it) from both malicious floods and accidental hammering. In a Rust web service the idiomatic approach is a [Tower](https://docs.rs/tower) middleware layer (the same composable building block your other middleware uses), so the limiter slots in next to logging, tracing, and timeouts without touching your handlers.

---

## Quick Overview

Rate limiting answers one question: *has this client used up its allowance?* The dominant algorithm is a **token bucket**: each client gets a bucket of N tokens, every request spends one, and tokens drip back at a fixed rate; an empty bucket means a `429 Too Many Requests`. In Node you reach for `express-rate-limit`; in Rust you add the `tower-governor` crate, which wraps the high-performance [`governor`](https://docs.rs/governor) limiter in a `tower::Layer` you attach with `.layer(...)`. The big wins over the typical Node setup are that the limiter is **in-process, lock-light, and allocation-frugal** (no Redis round-trip needed for the common single-instance case), and that mistakes like forgetting per-IP context surface as obvious behavior rather than silent global limits.

> **Note:** This page covers *application-level* rate limiting inside your Rust service: per-IP limits, a global cap, and per-route policies via a Tower layer. For caching responses to reduce load (a complementary technique), see [Caching Strategies](/28-production/07-caching/); for the broader hardening picture, see [Production Readiness Checklist](/28-production/09-production-checklist/). Authentication-adjacent throttling (login brute-force protection) connects to [Security](/27-security/).

---

## TypeScript/JavaScript Example

In an Express service the standard tool is `express-rate-limit`. You configure a window and a limit, register it as middleware, and it tracks counts per client IP in an in-memory store by default.

```typescript
// server.ts
import express from "express";
import { rateLimit } from "express-rate-limit";

const app = express();

// 5 requests per minute per IP; replies 429 with a JSON body when exceeded.
const limiter = rateLimit({
  windowMs: 60_000, // 1 minute fixed window
  limit: 5,
  standardHeaders: "draft-7", // emit RateLimit-* headers
  legacyHeaders: false,
  message: { error: "rate_limited" },
});

app.use(limiter);
app.get("/", (_req, res) => res.send("hello"));

app.listen(3001);
```

Firing seven requests in quick succession from the same IP, the sixth and seventh are rejected:

```text
req 1: 200
req 2: 200
req 3: 200
req 4: 200
req 5: 200
req 6: 429 retry-after=60
req 7: 429 retry-after=60
blocked body: {"error":"rate_limited"}
```

This works, but it carries two quiet caveats a senior engineer learns the hard way. First, the default **in-memory store is per-process**: run two Node instances behind a load balancer and each enforces its own limit, so the *effective* limit doubles. You need a shared `RedisStore` to fix it. Second, `express-rate-limit` reads the client IP from the connection unless you set `app.set("trust proxy", ...)`; behind a reverse proxy, everyone shares the proxy's IP and a single client can starve the rest. Both traps exist in Rust too, and `tower-governor` makes the proxy case explicit via the key extractor you choose.

---

## Rust Equivalent

In Rust, rate limiting is a **Tower layer**. The `tower-governor` crate provides `GovernorLayer`, configured by a `GovernorConfigBuilder`, and attaches to an `axum` (or any Tower-based) router with `.layer(...)`. Start a fresh 2024-edition project (the repository's [pinned verification toolchain](/00-introduction/05-version-policy/) is the reproducible compiler baseline) and add the dependencies:

```toml
# Cargo.toml
[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
tower_governor = "0.8"
```

```rust
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::{routing::get, Router};
use tokio::net::TcpListener;
use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};

async fn hello() -> &'static str {
    "hello"
}

#[tokio::main]
async fn main() {
    // Allow a burst of 5 requests per client IP, replenishing one token every 2s.
    let governor_conf = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(2)
            .burst_size(5)
            .finish()
            .unwrap(),
    );

    // Periodically evict idle IP buckets so memory does not grow unbounded.
    let limiter = governor_conf.limiter().clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(60));
        loop {
            tick.tick().await;
            limiter.retain_recent();
        }
    });

    let app = Router::new()
        .route("/", get(hello))
        .layer(GovernorLayer::new(governor_conf));

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    // `with_connect_info` puts the peer SocketAddr into each request so the
    // default per-IP key extractor can read it.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
```

Firing eight requests rapidly from one machine, the first five pass and the rest are rejected with `429`:

```text
req 1 -> 200
req 2 -> 200
req 3 -> 200
req 4 -> 200
req 5 -> 200
req 6 -> 429
req 7 -> 429
req 8 -> 429
```

The body and headers of a blocked request, captured with `curl -i`:

```text
HTTP/1.1 429 Too Many Requests
x-ratelimit-after: 1
retry-after: 1
content-length: 30
date: Tue, 02 Jun 2026 06:48:14 GMT

Too Many Requests! Wait for 1s
```

`tower-governor` sends `retry-after` (and its own `x-ratelimit-after`) out of the box, telling clients exactly how long to back off. It's the same contract the Express version provides, but enforced by a layer rather than handler-adjacent middleware.

---

## Detailed Explanation

### The token bucket underneath: GCRA

`tower-governor` is a thin Tower adapter over the `governor` crate, which implements the **Generic Cell Rate Algorithm (GCRA)**, a precise, allocation-free variant of the token bucket. Rather than counting requests in fixed windows (the approach `express-rate-limit` uses by default, which allows a 2x burst at a window boundary), GCRA tracks a single timestamp per key and computes whether enough time has elapsed to permit the next request. It is smooth, has no boundary spikes, and updates with a couple of atomic operations.

You can see the core limiter on its own, without any HTTP, by depending on `governor` directly:

```toml
# Cargo.toml
[dependencies]
governor = "0.10"
```

```rust
use std::num::NonZeroU32;

use governor::{Quota, RateLimiter};

fn main() {
    // A quota of 3 requests, replenishing the full burst once per second.
    let quota = Quota::per_second(NonZeroU32::new(3).unwrap());
    let limiter = RateLimiter::direct(quota);

    // The first 3 checks pass (the burst), the 4th is denied.
    for i in 1..=4 {
        match limiter.check() {
            Ok(()) => println!("request {i}: allowed"),
            Err(_) => println!("request {i}: rate limited"),
        }
    }
}
```

This prints, deterministically (no waiting between checks):

```text
request 1: allowed
request 2: allowed
request 3: allowed
request 4: rate limited
```

`RateLimiter::direct` is a single, unkeyed bucket; `RateLimiter::keyed` (what `tower-governor` uses internally) maintains one bucket *per key* in a concurrent hash map. The `check()` returns a `Result` — `Ok` to proceed, `Err` carrying when the next request will be allowed — which is exactly the information `tower-governor` turns into a `retry-after` header.

### `GovernorConfigBuilder`: period and burst

Two numbers define a quota:

- **`burst_size(n)`**: the bucket capacity, i.e. how many requests may arrive back-to-back before throttling kicks in.
- **`per_second(s)` / `per_millisecond(ms)` / `period(Duration)`**: how often *one* token is replenished.

So `.per_second(2).burst_size(5)` means "up to 5 at once, then one more every 2 seconds." This pair maps onto the Express `windowMs`/`limit` mental model but expresses *sustained rate* and *burst tolerance* independently, which fixed windows cannot.

`.finish()` returns `Option<GovernorConfig>`: it is `None` if you pass a zero burst or zero period (an unsatisfiable quota), which is why the examples `.unwrap()` a known-good config. Wrap the result in `Arc` once and share it: constructing the same config twice creates two *independent* limiters, a subtle bug the crate's own docs warn about.

### Key extractors: who counts as "a client"?

The `GovernorConfigBuilder` carries a **key extractor** that decides what to bucket on. Three are built in:

- **`PeerIpKeyExtractor`** (the default): buckets by the TCP peer address. Correct only when clients connect to you directly.
- **`SmartIpKeyExtractor`**: reads `X-Forwarded-For`, then `X-Real-IP`, then the `Forwarded` header, falling back to the peer IP. This is what you want behind a load balancer or CDN.
- **`GlobalKeyExtractor`**: one bucket for *all* traffic, for a hard cap on total throughput.

The default extractor needs the peer `SocketAddr`, which axum only injects when you serve with `into_make_service_with_connect_info::<SocketAddr>()`. Forget that and every request fails to extract a key (see Pitfalls). Switching to a proxy-aware extractor with full rate-limit headers looks like this:

```rust
use std::sync::Arc;

use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::SmartIpKeyExtractor;

fn main() {
    let per_ip = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(2)
            .burst_size(5)
            .key_extractor(SmartIpKeyExtractor)
            .use_headers() // emit x-ratelimit-limit / x-ratelimit-remaining
            .finish()
            .unwrap(),
    );

    // `per_ip` is now ready to hand to `GovernorLayer::new(per_ip)`.
    println!("configured: {}", Arc::strong_count(&per_ip));
}
```

With `.use_headers()` enabled, a successful request now advertises the client's remaining allowance:

```text
HTTP/1.1 200 OK
content-type: text/plain; charset=utf-8
x-ratelimit-limit: 5
x-ratelimit-remaining: 4
content-length: 5

hello
```

and a blocked one reports zero remaining alongside the retry hint:

```text
HTTP/1.1 429 Too Many Requests
x-ratelimit-after: 1
retry-after: 1
x-ratelimit-limit: 5
x-ratelimit-remaining: 0

Too Many Requests! Wait for 1s
```

### Where the layer sits

`GovernorLayer` is an ordinary Tower layer, so it composes with everything else through `.layer(...)` (or a `ServiceBuilder`). Layers wrap **outermost-first**: the last layer you add is the first to see a request. Put rate limiting *before* expensive work (auth, DB queries) so rejected requests cost almost nothing, but *after* request-ID/tracing layers so even a `429` is logged with context. Because the bucket map lives in process memory, the `retain_recent` background task shown in the main example is important: without it, every distinct IP that ever connects leaves a bucket behind forever.

---

## Key Differences

| Concern | TypeScript / Express (`express-rate-limit`) | Rust (`tower-governor`) |
| --- | --- | --- |
| Integration point | `app.use(limiter)` middleware | `.layer(GovernorLayer::new(cfg))` Tower layer |
| Algorithm | fixed window (default) | GCRA token bucket (smooth, no boundary burst) |
| Quota model | `windowMs` + `limit` | `burst_size` + replenish `period` (independent) |
| Per-client key | client IP (needs `trust proxy`) | choice of `PeerIp` / `SmartIp` / `Global` / custom extractor |
| Proxy awareness | opt-in `trust proxy` setting | explicit `SmartIpKeyExtractor` |
| Multi-instance | per-process unless `RedisStore` | per-process unless you add a shared store |
| Rejection response | `429` + `RateLimit-*` headers | `429` + `retry-after` / `x-ratelimit-*` headers |
| Memory growth | store-dependent | manual `retain_recent()` to evict idle buckets |
| Performance | per-request object + map ops | lock-light atomics, no per-request allocation |

The deepest conceptual difference is the **algorithm**. A fixed window resets its counter at clock boundaries, so a client can fire `limit` requests at `00:59` and another `limit` at `01:00`, a 2x burst across the seam. GCRA has no seam: it enforces a steady rate with a configurable burst, which is both fairer and harder to game.

> **Note:** Unlike the Express middleware, which keeps a count *per process by default* and silently lets your effective limit scale with your replica count, `tower-governor`'s in-process limiter is the same trade-off: it is not a distributed limiter. The fix is identical in spirit (a shared store), but Rust makes the *key* you bucket on an explicit type-level choice rather than a config string, so "we forgot to trust the proxy" becomes "we chose `PeerIpKeyExtractor`," which is visible in the code.

---

## Common Pitfalls

### Pitfall 1: Forgetting `with_connect_info`, so every request 500s

The default `PeerIpKeyExtractor` needs the peer `SocketAddr` in the request extensions. axum only puts it there when you serve with `into_make_service_with_connect_info::<SocketAddr>()`. Use plain `into_make_service()` and the code still **compiles** — but at runtime every request fails key extraction:

```rust
// compiles, but breaks at runtime: no connect info means no key
axum::serve(listener, app.into_make_service()).await.unwrap();
```

The limiter cannot find an IP and returns a `GovernorError::UnableToExtractKey`, which surfaces as a `500`:

```text
HTTP/1.1 500 Internal Server Error
content-length: 22
date: Tue, 02 Jun 2026 06:50:51 GMT

Unable To Extract Key!
```

Because this is a runtime failure rather than a compile error, it is easy to ship. Always pair `PeerIpKeyExtractor`/`SmartIpKeyExtractor` with `into_make_service_with_connect_info::<SocketAddr>()`, and test a real request before trusting the limiter.

### Pitfall 2: Trusting `X-Forwarded-For` when you are *not* behind a trusted proxy

`SmartIpKeyExtractor` reads `X-Forwarded-For`, which the client fully controls. If your service is exposed directly (no proxy that overwrites the header), an attacker simply sends a different `X-Forwarded-For` per request and gets an unlimited number of fresh buckets — defeating the limit entirely. Only use `SmartIpKeyExtractor` when a trusted proxy/load balancer *sets* that header and strips any client-supplied value. When clients connect to you directly, use the default `PeerIpKeyExtractor`. This is the exact same hazard as Express's `trust proxy`, just made explicit by the extractor name.

### Pitfall 3: Building the config twice

Each call to `.finish()` builds a *new, independent* limiter with its own bucket map. If you write `GovernorLayer::new(GovernorConfigBuilder::default()....finish().unwrap())` inside a per-route closure or a loop, every route gets a separate limiter and the limits do not combine the way you expect. Build one `Arc<GovernorConfig>` and clone the `Arc` (cheap, just a refcount bump) wherever you need the layer.

### Pitfall 4: `finish()` returns `None`, and `.unwrap()` panics at startup

A zero `burst_size` or zero `period` is an impossible quota, so `.finish()` returns `None`. Calling `.unwrap()` on it panics — which is acceptable *at startup* (a misconfiguration should stop the process from booting, just like the config validation in [Environment-Based Configuration](/28-production/01-environment/)), but make sure those values come from validated config, not directly from unchecked user input.

### Pitfall 5: Unbounded memory from never evicting buckets

Every distinct key creates a bucket that lives until you remove it. On a public endpoint, that means one entry per IP that has *ever* connected. Spawn the `retain_recent()` cleanup task shown in the main example (or call it periodically) so idle buckets are reclaimed; otherwise a long-running service slowly leaks memory under a wide client base.

---

## Best Practices

- **Pick the key extractor that matches your deployment.** Direct exposure → `PeerIpKeyExtractor`. Behind a trusted proxy/CDN → `SmartIpKeyExtractor`. A coarse total-throughput cap → `GlobalKeyExtractor`. Per-account or per-API-key fairness → a custom `KeyExtractor`.
- **Layer global and per-IP limits together.** A `GlobalKeyExtractor` cap protects a shared downstream (a database, a paid third-party API) from total overload, while a per-IP limit keeps any single client fair. Apply both as stacked layers.
- **Set `burst_size` and the replenish rate from real traffic shapes,** not round numbers. Allow enough burst for legitimate clients (a page that fires several XHRs on load) while keeping the sustained rate tight.
- **Emit `retry-after` (and consider `.use_headers()`).** Well-behaved clients honor it and back off, smoothing load instead of retrying in a tight loop.
- **Build the config once, share it via `Arc`.** Never reconstruct it per request or per route.
- **Run `retain_recent()` on a timer** to bound memory.
- **Rate limit early in the layer stack** so rejected requests don't touch auth or the database, but keep tracing/request-ID layers outermost so `429`s are still observable.
- **For multiple replicas, move to a shared limiter** (e.g. a Redis-backed token bucket) when the per-process approximation is no longer acceptable. See [Caching Strategies](/28-production/07-caching/) for the Redis client patterns this builds on.

> **Tip:** Rate limiting and load shedding are different tools. A limiter rejects *too many requests from a client*; a `tower::limit::ConcurrencyLimitLayer` or a timeout rejects *too much work in flight on the server*. Production services usually want both — a per-client rate limit and a server-wide concurrency cap — composed as separate Tower layers.

---

## Real-World Example

A production API typically wants three things at once: a per-IP limit so no single caller dominates, full rate-limit headers so clients can self-throttle, and a `429` body in the same JSON shape as the rest of the API (not the crate's default plain-text message). `tower-governor` supports a custom error handler on the layer for exactly this. This self-contained server uses `SmartIpKeyExtractor` (assume a trusted proxy), evicts idle buckets, and returns JSON errors.

```toml
# Cargo.toml
[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
tower_governor = "0.8"
```

```rust
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{header, Response, StatusCode};
use axum::{routing::get, Router};
use tokio::net::TcpListener;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::SmartIpKeyExtractor;
use tower_governor::{GovernorError, GovernorLayer};

async fn hello() -> &'static str {
    "hello"
}

// Turn governor's errors into a JSON body matching the rest of our API.
fn json_error(err: GovernorError) -> Response<Body> {
    let (status, body, retry_after) = match err {
        GovernorError::TooManyRequests { wait_time, .. } => (
            StatusCode::TOO_MANY_REQUESTS,
            format!(r#"{{"error":"rate_limited","retry_after_seconds":{wait_time}}}"#),
            Some(wait_time),
        ),
        GovernorError::UnableToExtractKey => (
            StatusCode::INTERNAL_SERVER_ERROR,
            r#"{"error":"internal"}"#.to_string(),
            None,
        ),
        GovernorError::Other { code, msg, .. } => (
            code,
            format!(r#"{{"error":"{}"}}"#, msg.unwrap_or_default()),
            None,
        ),
    };

    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(secs) = retry_after {
        builder = builder.header(header::RETRY_AFTER, secs.to_string());
    }
    builder.body(Body::from(body)).unwrap()
}

#[tokio::main]
async fn main() {
    let conf = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(2)
            .burst_size(5)
            .key_extractor(SmartIpKeyExtractor) // trust the proxy's forwarded IP
            .finish()
            .unwrap(),
    );

    // Reclaim idle per-IP buckets every minute.
    let limiter = conf.limiter().clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(60));
        loop {
            tick.tick().await;
            limiter.retain_recent();
        }
    });

    let app = Router::new()
        .route("/", get(hello))
        .layer(GovernorLayer::new(conf).error_handler(json_error));

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
```

After exhausting the burst for one forwarded IP, a blocked request returns a JSON error with a `retry-after` header, captured with `curl -i -H 'X-Forwarded-For: 203.0.113.9'`:

```text
HTTP/1.1 429 Too Many Requests
content-type: application/json
retry-after: 1
content-length: 48
date: Tue, 02 Jun 2026 06:50:16 GMT

{"error":"rate_limited","retry_after_seconds":1}
```

Different forwarded IPs get independent buckets, so one noisy client never starves the rest: the property the whole exercise exists to guarantee.

> **Tip:** To rate-limit only *some* routes — say, throttle `/login` hard for brute-force protection while leaving `/health` untouched — attach the layer to a sub-router or an individual route rather than the whole app. A `Router::new().route("/login", get(login).layer(GovernorLayer::new(login_rl)))` merged with an unthrottled `.route("/health", get(health))` lets `/health` answer every request while `/login` enforces its quota. Keep health and readiness probes (see [Health and Readiness Endpoints](/28-production/03-health-checks/)) off the limiter so an outage's probe traffic is never itself rate limited.

---

## Further Reading

- [`tower-governor` on docs.rs](https://docs.rs/tower-governor): the Tower layer used throughout this page, including key extractors and the custom error handler.
- [`governor` on docs.rs](https://docs.rs/governor): the underlying GCRA rate limiter, usable on its own.
- [GCRA — Generic Cell Rate Algorithm](https://en.wikipedia.org/wiki/Generic_cell_rate_algorithm): the algorithm behind smooth, burst-tolerant limiting.
- [Tower `Layer` and `Service`](https://docs.rs/tower/latest/tower/): how middleware composes in the Rust async ecosystem.
- [`429 Too Many Requests` (MDN)](https://developer.mozilla.org/en-US/docs/Web/HTTP/Status/429) and the [`Retry-After` header](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Retry-After).
- Related guide sections:
  - [Caching Strategies](/28-production/07-caching/): reducing load with in-process and Redis caching; the Redis patterns also back a distributed rate limiter.
  - [Health and Readiness Endpoints](/28-production/03-health-checks/): keep liveness/readiness probes off the limiter.
  - [Metrics and Monitoring](/28-production/04-metrics/): emit a counter for rejected requests so you can see throttling in your dashboards.
  - [Distributed Tracing](/28-production/05-distributed-tracing/): keep tracing layers outermost so `429`s are still observable.
  - [Production Readiness Checklist](/28-production/09-production-checklist/): where rate limiting fits in the broader readiness picture.
  - [Environment-Based Configuration](/28-production/01-environment/): load burst/period values from validated configuration.
  - [Web APIs](/16-web-apis/): the axum and Tower foundations this page builds on.
  - [Security](/27-security/): rate limiting as a defense against brute-force and abuse.
  - [Async](/11-async/): the async runtime (`tokio`) and the background cleanup task.
  - [Understanding Cargo](/01-getting-started/03-cargo-basics/): adding dependencies with `cargo add`.
  - [Migration Guide](/29-migration-guide/): porting an Express service (including its `express-rate-limit` layer) to Rust.

---

## Exercises

### Exercise 1: A global throughput cap

**Difficulty:** Beginner

**Objective:** Configure a single, app-wide rate limit using `GlobalKeyExtractor` so the whole service never exceeds a fixed request rate, regardless of who is calling.

**Instructions:** Using `axum = "0.8"`, `tokio`, and `tower_governor = "0.8"`, build a router with one `GET /` route returning `"ok"`. Attach a `GovernorLayer` configured with `GlobalKeyExtractor`, a burst of 3, and one token replenished per second. Serve it (a global limiter does not need per-IP connect info, but serving with connect info is harmless). Verify that the 4th rapid request from any client returns `429` while the first three return `200`.

<details>
<summary>Solution</summary>

```toml
# Cargo.toml
[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
tower_governor = "0.8"
```

```rust
use std::net::SocketAddr;
use std::sync::Arc;

use axum::{routing::get, Router};
use tokio::net::TcpListener;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::GlobalKeyExtractor;
use tower_governor::GovernorLayer;

async fn ok() -> &'static str {
    "ok"
}

#[tokio::main]
async fn main() {
    let conf = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(1)
            .burst_size(3)
            .key_extractor(GlobalKeyExtractor)
            .finish()
            .unwrap(),
    );

    let app = Router::new()
        .route("/", get(ok))
        .layer(GovernorLayer::new(conf));

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
```

With a burst of 3, the first three rapid requests return `200` and the fourth returns `429`, no matter which IP they come from — because `GlobalKeyExtractor` uses one shared bucket (`type Key = ()`) for all traffic.

</details>

### Exercise 2: Per-route policies

**Difficulty:** Intermediate

**Objective:** Apply different limits to different routes — a strict cap on a sensitive endpoint and a looser one for read traffic — while leaving a health endpoint unthrottled.

**Instructions:** Build a router with three routes: `GET /login` (strict: burst 5, one token/minute), `GET /search` (loose: burst 30, one token/second), and `GET /health` (no limit). Attach a separate `GovernorLayer` to *each* of the first two routes (build one `Arc<GovernorConfig>` per policy), and add `/health` with no layer. Serve with per-IP connect info. Verify that `/login` returns `429` after its 5th rapid request while `/health` answers every request.

<details>
<summary>Solution</summary>

```toml
# Cargo.toml
[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
tower_governor = "0.8"
```

```rust
use std::net::SocketAddr;
use std::sync::Arc;

use axum::{routing::get, Router};
use tokio::net::TcpListener;
use tower_governor::{governor::GovernorConfigBuilder, GovernorLayer};

async fn login() -> &'static str {
    "login"
}
async fn search() -> &'static str {
    "results"
}
async fn health() -> &'static str {
    "ok"
}

#[tokio::main]
async fn main() {
    // Strict: brute-force protection on the login endpoint.
    let login_rl = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(60)
            .burst_size(5)
            .finish()
            .unwrap(),
    );
    // Loose: read-heavy search traffic.
    let search_rl = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(1)
            .burst_size(30)
            .finish()
            .unwrap(),
    );

    let limited = Router::new()
        .route("/login", get(login).layer(GovernorLayer::new(login_rl)))
        .route("/search", get(search).layer(GovernorLayer::new(search_rl)));

    let app = Router::new().merge(limited).route("/health", get(health));

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
```

Firing seven rapid requests at `/login` produces `200, 200, 200, 200, 200, 429, 429`, while ten rapid requests at `/health` all return `200` — the limiter only wraps the routes it is attached to.

</details>

### Exercise 3: A custom per-API-key extractor

**Difficulty:** Advanced

**Objective:** Implement a custom `KeyExtractor` that buckets by the `x-api-key` header, falling back to the peer IP for anonymous callers — so each API key gets its own fair allowance.

**Instructions:** Implement `KeyExtractor` for a unit struct `ApiKeyExtractor` with `type Key = String`. In `extract`, return `format!("key:{value}")` when an `x-api-key` header is present; otherwise read the peer IP from `axum::extract::ConnectInfo<SocketAddr>` in the request extensions and return `format!("ip:{ip}")`, or `GovernorError::UnableToExtractKey` if neither is available. Wire it into a `GovernorConfigBuilder` (burst 3, one token/2s) on a `GET /` route and verify that two different API keys get independent buckets.

<details>
<summary>Solution</summary>

```toml
# Cargo.toml
[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
tower_governor = "0.8"
```

```rust
use std::net::SocketAddr;
use std::sync::Arc;

use axum::http::Request;
use axum::{routing::get, Router};
use tokio::net::TcpListener;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::key_extractor::KeyExtractor;
use tower_governor::{GovernorError, GovernorLayer};

async fn hello() -> &'static str {
    "hello"
}

#[derive(Clone)]
struct ApiKeyExtractor;

impl KeyExtractor for ApiKeyExtractor {
    type Key = String;

    fn extract<T>(&self, req: &Request<T>) -> Result<Self::Key, GovernorError> {
        // Prefer the API key when present...
        if let Some(key) = req
            .headers()
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
        {
            return Ok(format!("key:{key}"));
        }
        // ...otherwise fall back to the peer IP.
        req.extensions()
            .get::<axum::extract::ConnectInfo<SocketAddr>>()
            .map(|ci| format!("ip:{}", ci.0.ip()))
            .ok_or(GovernorError::UnableToExtractKey)
    }
}

#[tokio::main]
async fn main() {
    let conf = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(2)
            .burst_size(3)
            .key_extractor(ApiKeyExtractor)
            .finish()
            .unwrap(),
    );

    let app = Router::new()
        .route("/", get(hello))
        .layer(GovernorLayer::new(conf));

    let listener = TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
```

Sending four rapid requests with `x-api-key: AAA` yields `200, 200, 200, 429`, while a request with `x-api-key: BBB` still returns `200` — each key has its own bucket because the extracted `Key` strings differ. The peer-IP fallback means anonymous callers are still limited, just grouped by source address instead of key.

> **Note:** The `KeyExtractor` trait also defines `name` and `key_name` methods, but those are gated behind the crate's `tracing` feature; with the default features the two members shown here are all you need to implement.

</details>
