---
title: "Metrics and Monitoring"
description: "Instrument an axum service with the Rust metrics crate and a Prometheus exporter: counters, gauges, histograms, and RED/USE, the counterpart to Node's prom-client."
---

Metrics are the numeric heartbeat of a production service: request rates, error counts, latency distributions, and resource saturation. In the Node ecosystem you reach for `prom-client`; in Rust the equivalent is the `metrics` facade plus a Prometheus exporter. This page shows how to instrument an Axum service, expose a `/metrics` endpoint, and choose *which* numbers to track using the RED and USE methods.

---

## Quick Overview

A metric is a cheap, aggregatable number you update on hot paths and scrape periodically into a time-series database (usually Prometheus). The `metrics` crate gives you a global recorder with three instrument types — **counters** (monotonically increasing), **gauges** (go up and down), and **histograms** (latency/size distributions) — exactly mirroring `prom-client`'s `Counter`, `Gauge`, and `Histogram`. For a TypeScript developer, the mental model is identical; the differences are that Rust's macros add labels with near-zero overhead and the exporter renders the same Prometheus text format your dashboards already understand.

---

## TypeScript/JavaScript Example

A typical Express service instrumented with `prom-client` (the de-facto Node Prometheus library):

```typescript
// npm install express prom-client
import express, { Request, Response, NextFunction } from "express";
import {
  collectDefaultMetrics,
  Counter,
  Gauge,
  Histogram,
  register,
} from "prom-client";

// Process-level metrics: heap, event-loop lag, CPU, GC.
collectDefaultMetrics();

const httpRequestsTotal = new Counter({
  name: "http_requests_total",
  help: "Total HTTP requests handled",
  labelNames: ["method", "path", "status"] as const,
});

const httpRequestsInFlight = new Gauge({
  name: "http_requests_in_flight",
  help: "Requests currently being processed",
});

const httpRequestDuration = new Histogram({
  name: "http_request_duration_seconds",
  help: "HTTP request latency in seconds",
  labelNames: ["method", "path"] as const,
  buckets: [0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1, 2.5, 5, 10],
});

const app = express();

// One middleware records the RED signals for every route.
app.use((req: Request, res: Response, next: NextFunction) => {
  const end = httpRequestDuration.startTimer();
  httpRequestsInFlight.inc();

  res.on("finish", () => {
    // req.route?.path is the *matched template* ("/users/:id"), not the raw URL.
    const path = req.route?.path ?? "unknown";
    httpRequestsInFlight.dec();
    httpRequestsTotal.inc({ method: req.method, path, status: String(res.statusCode) });
    end({ method: req.method, path });
  });

  next();
});

app.get("/users", (_req, res) => res.json([]));

// Prometheus scrapes this endpoint every 15s or so.
app.get("/metrics", async (_req, res) => {
  res.set("Content-Type", register.contentType);
  res.send(await register.metrics());
});

app.listen(3000);
```

**Key points:**

- `prom-client` keeps a global `register` that every metric attaches to.
- You construct metric objects up front and call `.inc()` / `.set()` / `.observe()` on hot paths.
- `/metrics` returns plain text in the Prometheus exposition format.
- `collectDefaultMetrics()` adds process/event-loop stats for free.

---

## Rust Equivalent

The idiomatic Rust stack is the [`metrics`](https://docs.rs/metrics) facade (a lightweight global API, analogous to how `log`/`tracing` are facades) plus the [`metrics-exporter-prometheus`](https://docs.rs/metrics-exporter-prometheus) recorder. The current stable toolchain is Rust 1.96.0 on the 2024 edition; `cargo new` selects it automatically.

```bash
cargo add metrics metrics-exporter-prometheus
cargo add axum
cargo add tokio --features full
```

```rust
use std::time::Instant;

use axum::{
    extract::{MatchedPath, Request},
    middleware::{self, Next},
    response::IntoResponse,
    routing::get,
    Router,
};
use metrics::{
    counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram, Unit,
};
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};

const DURATION_METRIC: &str = "http_request_duration_seconds";

// Install the global recorder and describe each metric once at startup.
fn setup_metrics_recorder() -> PrometheusHandle {
    // Latency buckets, in seconds, for RED-style dashboards.
    const BUCKETS: &[f64] = &[
        0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
    ];

    let handle = PrometheusBuilder::new()
        // Render this histogram as native Prometheus buckets, not a summary.
        .set_buckets_for_metric(Matcher::Full(DURATION_METRIC.to_string()), BUCKETS)
        .expect("invalid bucket configuration")
        .install_recorder()
        .expect("failed to install Prometheus recorder");

    // Help text + types, emitted as `# HELP` / `# TYPE` lines.
    describe_counter!("http_requests_total", "Total HTTP requests handled");
    describe_gauge!("http_requests_in_flight", "Requests currently in flight");
    describe_histogram!(DURATION_METRIC, Unit::Seconds, "HTTP request latency");
    handle
}

// One middleware records the RED signals for every route.
async fn track_metrics(req: Request, next: Next) -> impl IntoResponse {
    let start = Instant::now();

    // Use the matched route template ("/users/{id}"), never the raw path,
    // to keep label cardinality bounded.
    let path = req
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str().to_owned())
        .unwrap_or_else(|| "unknown".to_owned());
    let method = req.method().clone();

    let in_flight = gauge!("http_requests_in_flight");
    in_flight.increment(1.0);

    let response = next.run(req).await;

    in_flight.decrement(1.0);
    let latency = start.elapsed().as_secs_f64();
    let status = response.status().as_u16().to_string();

    let labels = [
        ("method", method.to_string()),
        ("path", path),
        ("status", status),
    ];
    counter!("http_requests_total", &labels).increment(1);
    histogram!(DURATION_METRIC, &labels[..2]).record(latency);

    response
}

async fn list_users() -> &'static str {
    "[]"
}

#[tokio::main]
async fn main() {
    let recorder = setup_metrics_recorder();

    let app = Router::new()
        .route("/users", get(list_users))
        // Prometheus scrapes this; `render()` produces the exposition text.
        .route("/metrics", get(move || std::future::ready(recorder.render())))
        .layer(middleware::from_fn(track_metrics));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

After hitting `/users` twice and a missing route once, `curl localhost:3000/metrics` returns real Prometheus text:

```text
# HELP http_requests_total Total HTTP requests handled
# TYPE http_requests_total counter
http_requests_total{method="GET",path="unknown",status="404"} 1
http_requests_total{method="GET",path="/users",status="200"} 2

# HELP http_requests_in_flight Requests currently in flight
# TYPE http_requests_in_flight gauge
http_requests_in_flight 1

# HELP http_request_duration_seconds HTTP request latency
# TYPE http_request_duration_seconds histogram
http_request_duration_seconds_bucket{method="GET",path="unknown",le="0.005"} 1
http_request_duration_seconds_bucket{method="GET",path="unknown",le="0.01"} 1
http_request_duration_seconds_bucket{method="GET",path="unknown",le="0.025"} 1
http_request_duration_seconds_bucket{method="GET",path="unknown",le="0.05"} 1
http_request_duration_seconds_bucket{method="GET",path="unknown",le="0.1"} 1
http_request_duration_seconds_bucket{method="GET",path="unknown",le="0.25"} 1
http_request_duration_seconds_bucket{method="GET",path="unknown",le="0.5"} 1
http_request_duration_seconds_bucket{method="GET",path="unknown",le="1"} 1
http_request_duration_seconds_bucket{method="GET",path="unknown",le="2.5"} 1
http_request_duration_seconds_bucket{method="GET",path="unknown",le="5"} 1
http_request_duration_seconds_bucket{method="GET",path="unknown",le="10"} 1
http_request_duration_seconds_bucket{method="GET",path="unknown",le="+Inf"} 1
http_request_duration_seconds_sum{method="GET",path="unknown"} 0.000043833
http_request_duration_seconds_count{method="GET",path="unknown"} 1
http_request_duration_seconds_bucket{method="GET",path="/users",le="0.005"} 2
http_request_duration_seconds_bucket{method="GET",path="/users",le="0.01"} 2
http_request_duration_seconds_bucket{method="GET",path="/users",le="0.025"} 2
http_request_duration_seconds_bucket{method="GET",path="/users",le="0.05"} 2
http_request_duration_seconds_bucket{method="GET",path="/users",le="0.1"} 2
http_request_duration_seconds_bucket{method="GET",path="/users",le="0.25"} 2
http_request_duration_seconds_bucket{method="GET",path="/users",le="0.5"} 2
http_request_duration_seconds_bucket{method="GET",path="/users",le="1"} 2
http_request_duration_seconds_bucket{method="GET",path="/users",le="2.5"} 2
http_request_duration_seconds_bucket{method="GET",path="/users",le="5"} 2
http_request_duration_seconds_bucket{method="GET",path="/users",le="10"} 2
http_request_duration_seconds_bucket{method="GET",path="/users",le="+Inf"} 2
http_request_duration_seconds_sum{method="GET",path="/users"} 0.00010016699999999999
http_request_duration_seconds_count{method="GET",path="/users"} 2
```

> **Note:** The 404 request matched no route, so its latency lands in the `path="unknown"` histogram series, emitted before the `path="/users"` series. The `_sum` values are real but timing-dependent; yours will differ run to run. The gauge reads `1`, not `0`, because the `/metrics` request itself is still in flight while `render()` runs: a small but real detail of measuring yourself.

---

## Detailed Explanation

### The facade pattern

`metrics` is a *facade*, just like the `log` and `tracing` crates. Your code calls `counter!`, `gauge!`, and `histogram!` against a global recorder, but the facade does not decide how those numbers are stored or exported. You install exactly one recorder at startup — here `metrics-exporter-prometheus` — and the macros route to it. Swap to StatsD, OTLP, or a test recorder by changing only the install line, never the call sites. In `prom-client` terms, the macros are the metric objects and the `PrometheusHandle` is the `register`.

### Describing metrics

`describe_counter!`, `describe_gauge!`, and `describe_histogram!` attach help text and an optional `Unit`. They are the moral equivalent of the `help:` field you pass to a `prom-client` constructor, and they produce the `# HELP` / `# TYPE` comment lines in the exposition output. Unlike `prom-client`, describing is *optional* and *decoupled* from first use: you can record a metric before it is described, and the exporter still emits it (without help text). Calling the describe macros once in `setup_metrics_recorder` keeps documentation in one place.

### Recording values

Each macro returns a lightweight handle:

- `counter!("name").increment(1)`: add to a monotonic counter (`u64` delta).
- `gauge!("name").set(x)` / `.increment(x)` / `.decrement(x)`: set or adjust an `f64`.
- `histogram!("name").record(x)`: observe an `f64` sample into the configured buckets.

Labels are key/value pairs passed inline (`"method" => "GET"`) or as a slice of `(&str, String)` tuples, as in the middleware. Note the slice syntax `&labels[..2]`: the histogram only needs `method` and `path`, so we reuse the first two labels and drop `status`.

### Counters vs histograms in the wire format

A counter renders as a single `_total` series per label set. A histogram is special: Prometheus represents it as **cumulative buckets** (`_bucket{le="..."}`), plus a `_sum` and a `_count`. Each `le` ("less than or equal") bucket counts every observation at or below that boundary, which is why the counts are non-decreasing and the last real bucket equals `+Inf`. From these buckets, PromQL's `histogram_quantile()` estimates p50/p95/p99 latency across all your instances, something a per-instance summary cannot do.

### Buckets vs summaries

By default `metrics-exporter-prometheus` renders a histogram as a Prometheus **summary** (client-side quantiles). For aggregatable latency you almost always want **histograms with explicit buckets**, which is exactly what `set_buckets_for_metric` configures. The `Matcher` lets you target metrics by `Full` name, `Prefix`, or `Suffix`, so you can apply one bucket scheme to every `*_duration_seconds` metric at once.

### Bounding label cardinality

The single most important line in the middleware is the `MatchedPath` extraction. Axum's `MatchedPath` is the route *template* (`/users/{id}`), not the concrete URL (`/users/42`). Every distinct label combination becomes its own time series; using raw paths or user IDs as labels creates unbounded series and will eventually take down Prometheus. This is the Rust equivalent of using `req.route.path` instead of `req.url` in the Node example.

---

## Key Differences

| Aspect | TypeScript (`prom-client`) | Rust (`metrics` + exporter) |
| --- | --- | --- |
| Global registry | `register` singleton | The installed recorder (`PrometheusHandle`) |
| Define a metric | `new Counter({...})` object | `counter!("name")` macro, described separately |
| Label declaration | `labelNames` array, checked at runtime | Inline `k => v` pairs, no fixed schema |
| Recording overhead | JS object allocation + map lookup | Static key interning, atomic update |
| Default process metrics | `collectDefaultMetrics()` (heap, event loop) | Opt-in via [`metrics-process`](https://docs.rs/metrics-process) (CPU, RSS, FDs) |
| Histogram default | Real buckets | **Summary** unless you set buckets |
| Pluggable backend | Prometheus-only | Facade: Prometheus, StatsD, OTLP, test recorders |
| Async runtime needed | No | Only for the HTTP scrape endpoint |

### Pull, not push

Like `prom-client`, the `metrics` + Prometheus model is **pull-based**: your process holds the current values in memory, and Prometheus scrapes `/metrics` on its own schedule (typically every 15–60 seconds). Counters never reset on scrape; Prometheus computes rates from successive scrapes. This is the opposite of fire-and-forget StatsD/DogStatsD, where the app pushes individual events. If you must push (short-lived jobs, serverless), `metrics-exporter-prometheus` also supports a **push gateway** via `.with_push_gateway(...)`.

### RED and USE: which metrics to collect

Instrumenting is easy; choosing *what* to measure is the skill. Two complementary frameworks:

- **RED** (for request-driven services — your API):
  - **R**ate — requests per second (`rate(http_requests_total[5m])`).
  - **E**rrors — failed requests per second (filter `status=~"5.."`).
  - **D**uration — latency distribution (`histogram_quantile(0.99, ...)`).
- **USE** (for resources — pools, queues, CPU, memory):
  - **U**tilization — fraction of a resource busy (a gauge, e.g. connection-pool usage).
  - **S**aturation — work that is queued/waiting (a gauge or counter).
  - **E**rrors — error events for that resource.

The middleware above gives you all three RED signals from one place. USE signals are gauges you update where the resource lives: a DB pool, a Tokio task queue, a Redis client. The three instrument types map cleanly: **counters** give you Rate and Errors, **histograms** give you Duration, and **gauges** give you Utilization and Saturation.

---

## Common Pitfalls

### Pitfall 1: Forgetting to install a recorder

If you call `counter!(...)` without first installing a recorder, the facade silently routes to a no-op recorder: your metrics simply never appear, with no error. There is no panic and no warning. Always call `install_recorder()` (or `install()`) once at the very start of `main`, before serving traffic, and verify a metric shows up at `/metrics` in a smoke test.

### Pitfall 2: High-cardinality labels

Putting unbounded values into labels is the classic Prometheus footgun:

```rust
use metrics::counter;

fn handle_request(user_id: u64, raw_path: &str) {
    // Anti-pattern: user_id and raw_path explode cardinality.
    // Each unique user creates a brand-new time series that lives forever.
    counter!("http_requests_total",
        "user_id" => user_id.to_string(),
        "path" => raw_path.to_string(), // e.g. "/users/42", "/users/43", ...
    ).increment(1);
}
```

This compiles and runs: that is what makes it dangerous. Keep labels low-cardinality: HTTP method, matched route template, status code, region. Put per-user detail in logs or traces (see [Distributed Tracing](/28-production/05-distributed-tracing/)), never in metric labels.

### Pitfall 3: Expecting histograms to render as buckets by default

A common surprise: you record a histogram, open `/metrics`, and see `_sum`/`_count` with `quantile="..."` lines (a summary) instead of `_bucket{le="..."}` lines. `histogram_quantile()` in PromQL needs buckets. Always configure buckets with `set_buckets_for_metric` (or a global `set_buckets`) for latency metrics, as shown in the main example.

### Pitfall 4: Type mismatches in the macros

The macro arguments are typed, and the compiler enforces it. Counters take an unsigned **integer** delta (`u64`); gauges and histograms take an `f64`. Reaching for the wrong one — say, incrementing a counter by a fraction — does not compile:

```rust
fn main() {
    metrics::counter!("jobs_total").increment(1.5); // does not compile
}
```

The real `cargo check` error (Rust 1.96.0) is:

```text
error[E0308]: mismatched types
   --> src/main.rs:2:47
    |
  2 |     metrics::counter!("jobs_total").increment(1.5); // does not compile
    |                                     --------- ^^^ expected `u64`, found floating-point number
    |                                     |
    |                                     arguments to this method are incorrect
    |
note: method defined here
   --> .../metrics-0.24.6/src/handles.rs:102:12
    |
102 |     pub fn increment(&self, value: u64) {
    |            ^^^^^^^^^
```

Use `counter!(...).increment(1)` for whole events, and reach for a `gauge!` or `histogram!` (both `f64`) when you genuinely need fractional values. Note that a *literal* like `gauge!("x").set(5)` compiles fine — `5` is inferred as `f64` — so the trap mainly bites when you pass an already-typed integer.

### Pitfall 5: Securing the scrape endpoint

`/metrics` leaks operational detail (route names, error rates, internal counters). Do not expose it publicly. Bind it to an internal interface, gate it behind your service mesh / firewall, or require an auth header. See [Security](/27-security/) and the [Production Readiness Checklist](/28-production/09-production-checklist/).

---

## Best Practices

- **Name by convention.** Use `snake_case`, a unit suffix, and `_total` for counters: `http_requests_total`, `http_request_duration_seconds`, `db_pool_connections_in_use`. Prometheus tooling assumes these conventions.
- **Describe once, record everywhere.** Call the `describe_*!` macros in one startup function so help text and units stay consistent.
- **Centralize RED in middleware.** One Tower/Axum layer instruments every route; see the main example. Do not sprinkle `counter!` calls into each handler.
- **Pick buckets that match your SLO.** If your latency SLO is 200ms, ensure a bucket boundary sits near `0.2` so the dashboard can show your SLO compliance precisely.
- **Add process metrics.** `cargo add metrics-process` and register a collector to get CPU, resident memory, and file-descriptor counts: the analogue of `collectDefaultMetrics()`.
- **Keep cardinality bounded.** Audit every label: is its set of possible values small and stable? If not, it does not belong in a metric.
- **Separate metrics from traces and logs.** Metrics answer "how many / how fast" in aggregate; traces answer "what happened to *this* request." They complement, not replace, each other.

---

## Real-World Example

A worker that processes jobs from a queue, instrumented with both RED (per-job rate/errors/duration) and USE (pool utilization) signals, and exposing metrics on a dedicated port via the exporter's own HTTP listener, handy for background workers that have no web framework:

```rust
// cargo add metrics metrics-exporter-prometheus
// cargo add tokio --features full
use std::net::SocketAddr;
use std::time::Instant;

use metrics::{counter, describe_counter, describe_gauge, describe_histogram, gauge, histogram, Unit};
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder};

const JOB_DURATION: &str = "job_processing_duration_seconds";

struct Job {
    id: u64,
    kind: &'static str,
    will_fail: bool,
}

async fn process(job: &Job) -> Result<(), String> {
    if job.will_fail {
        Err(format!("job {} failed", job.id))
    } else {
        Ok(())
    }
}

// RED for jobs + USE for the worker pool, all in one place.
async fn run_job(job: &Job, pool_in_use: &mut usize, pool_capacity: usize) {
    *pool_in_use += 1;
    gauge!("worker_pool_utilization_ratio")
        .set(*pool_in_use as f64 / pool_capacity as f64);

    let start = Instant::now();
    let result = process(job).await;
    let elapsed = start.elapsed().as_secs_f64();

    let outcome = if result.is_ok() { "success" } else { "error" };
    counter!("jobs_processed_total", "kind" => job.kind, "outcome" => outcome).increment(1);
    histogram!(JOB_DURATION, "kind" => job.kind).record(elapsed);

    *pool_in_use -= 1;
    gauge!("worker_pool_utilization_ratio")
        .set(*pool_in_use as f64 / pool_capacity as f64);
}

#[tokio::main]
async fn main() {
    // The exporter runs its own HTTP server on :9000/metrics — no web framework needed.
    let addr: SocketAddr = "0.0.0.0:9000".parse().unwrap();
    PrometheusBuilder::new()
        .with_http_listener(addr)
        .set_buckets_for_metric(
            Matcher::Full(JOB_DURATION.to_string()),
            &[0.01, 0.05, 0.1, 0.5, 1.0, 5.0],
        )
        .expect("invalid bucket configuration")
        .install()
        .expect("failed to install Prometheus exporter");

    describe_counter!("jobs_processed_total", "Jobs processed by kind and outcome");
    describe_gauge!("worker_pool_utilization_ratio", "Fraction of workers busy (USE)");
    describe_histogram!(JOB_DURATION, Unit::Seconds, "Per-job processing time (RED)");

    let pool_capacity = 4;
    let mut pool_in_use = 0;

    let jobs = [
        Job { id: 1, kind: "email", will_fail: false },
        Job { id: 2, kind: "email", will_fail: true },
        Job { id: 3, kind: "report", will_fail: false },
    ];

    for job in &jobs {
        run_job(job, &mut pool_in_use, pool_capacity).await;
    }

    println!("metrics live at http://{addr}/metrics");
    // In a real worker you would loop forever pulling jobs; here we keep the
    // process alive briefly so the endpoint can be scraped.
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
}
```

This pattern composes with the rest of the section: pair it with [Graceful Shutdown](/28-production/02-graceful-shutdown/) so in-flight jobs drain cleanly, [Health and Readiness Endpoints](/28-production/03-health-checks/) for liveness, and [Background Job Processing](/28-production/08-background-jobs/) for the queue itself. The corresponding Grafana dashboard would chart `rate(jobs_processed_total{outcome="error"}[5m])` for the error signal and `histogram_quantile(0.99, sum(rate(job_processing_duration_seconds_bucket[5m])) by (le, kind))` for p99 duration.

---

## Further Reading

### Official Documentation

- [`metrics` crate docs](https://docs.rs/metrics): the facade, macros, and recorder trait.
- [`metrics-exporter-prometheus` docs](https://docs.rs/metrics-exporter-prometheus): `PrometheusBuilder`, matchers, buckets, push gateway.
- [Prometheus exposition format](https://prometheus.io/docs/instrumenting/exposition_formats/): the wire format `/metrics` returns.
- [Prometheus metric and label naming](https://prometheus.io/docs/practices/naming/): naming conventions and cardinality guidance.
- [The RED Method (Grafana)](https://grafana.com/blog/2018/08/02/the-red-method-how-to-instrument-your-services/) and [The USE Method (Brendan Gregg)](https://www.brendangregg.com/usemethod.html).

### Related Guide Sections

- [Health and readiness checks](/28-production/03-health-checks/): the other half of "is my service up?"
- [Distributed tracing](/28-production/05-distributed-tracing/): per-request detail that complements aggregate metrics.
- [Graceful shutdown](/28-production/02-graceful-shutdown/): keep metrics accurate while draining.
- [Production checklist](/28-production/09-production-checklist/): where metrics fit in overall readiness.
- [Rate limiting](/28-production/06-rate-limiting/) and [Caching](/28-production/07-caching/) — resources worth measuring with USE signals.
- [Section 16: Web APIs](/16-web-apis/) — the Axum foundation these examples build on.
- [Migration guide](/29-migration-guide/) — moving a Node observability stack to Rust.
- [Section 02: Basic Types](/02-basics/01-types/) — why `f64` vs integer matters in the macro signatures.

---

## Exercises

### Exercise 1: Instrument database query outcomes

**Difficulty:** Beginner

**Objective:** Add a counter that records database queries split by success and error, the Errors signal of RED for your data layer.

**Instructions:** Write a `run_query(ok: bool)` function that increments a `db_queries_total` counter with an `outcome` label of either `"success"` or `"error"`. Install a Prometheus recorder, describe the metric, simulate two successes and one error, and print the rendered output. Confirm the output contains two series with the correct counts.

<details>
<summary>Solution</summary>

```rust
// cargo add metrics metrics-exporter-prometheus
use metrics::{counter, describe_counter};
use metrics_exporter_prometheus::PrometheusBuilder;

fn run_query(ok: bool) {
    let outcome = if ok { "success" } else { "error" };
    counter!("db_queries_total", "outcome" => outcome).increment(1);
}

fn main() {
    let handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install recorder");
    describe_counter!("db_queries_total", "Database queries by outcome");

    run_query(true);
    run_query(true);
    run_query(false);

    print!("{}", handle.render());
}
```

Running this prints the real output:

```text
# HELP db_queries_total Database queries by outcome
# TYPE db_queries_total counter
db_queries_total{outcome="success"} 2
db_queries_total{outcome="error"} 1
```

> **Note:** The two series carry the correct counts (`success` 2, `error` 1), but the order of series *within* a metric is not guaranteed: it reflects internal hash-map iteration, so a later run may print `error` before `success`. Never depend on series ordering.

</details>

### Exercise 2: A saturation gauge for a worker pool

**Difficulty:** Intermediate

**Objective:** Expose the Utilization signal of the USE method as a gauge.

**Instructions:** Model a fixed-size worker pool with a `capacity` and an `in_use` count. Each time a worker is acquired, set a `worker_pool_utilization_ratio` gauge to `in_use / capacity`. Acquire two workers from a pool of capacity 4 and verify the gauge reads `0.5` in the exposition output.

<details>
<summary>Solution</summary>

```rust
// cargo add metrics metrics-exporter-prometheus
use metrics::{describe_gauge, gauge};
use metrics_exporter_prometheus::PrometheusBuilder;

struct Pool {
    capacity: usize,
    in_use: usize,
}

impl Pool {
    fn acquire(&mut self) {
        self.in_use += 1;
        let utilization = self.in_use as f64 / self.capacity as f64;
        gauge!("worker_pool_utilization_ratio").set(utilization);
    }
}

fn main() {
    let handle = PrometheusBuilder::new()
        .install_recorder()
        .expect("failed to install recorder");
    describe_gauge!("worker_pool_utilization_ratio", "Fraction of workers busy");

    let mut pool = Pool { capacity: 4, in_use: 0 };
    pool.acquire();
    pool.acquire();

    print!("{}", handle.render());
}
```

Real output:

```text
# HELP worker_pool_utilization_ratio Fraction of workers busy
# TYPE worker_pool_utilization_ratio gauge
worker_pool_utilization_ratio 0.5
```

</details>

### Exercise 3: Latency histogram with SLO-aligned buckets

**Difficulty:** Advanced

**Objective:** Configure a histogram whose buckets are tuned to a 200ms SLO and verify the bucket layout.

**Instructions:** Build a recorder that renders `request_latency_seconds` as a histogram (not a summary) with buckets that include a boundary at `0.2` (your SLO). Record a few latencies straddling 200ms, render the output, and confirm you see `_bucket{le="0.2"}` plus `_sum` and `_count` lines. Explain in a comment why a bucket boundary at the SLO matters.

<details>
<summary>Solution</summary>

```rust
// cargo add metrics metrics-exporter-prometheus
use metrics::{describe_histogram, histogram, Unit};
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder};

const METRIC: &str = "request_latency_seconds";

fn main() {
    // A boundary exactly at the 200ms SLO lets a dashboard compute
    // "fraction of requests under SLO" precisely from `le="0.2"`,
    // instead of interpolating between coarser buckets.
    let handle = PrometheusBuilder::new()
        .set_buckets_for_metric(
            Matcher::Full(METRIC.to_string()),
            &[0.05, 0.1, 0.2, 0.5, 1.0],
        )
        .expect("invalid buckets")
        .install_recorder()
        .expect("failed to install recorder");

    describe_histogram!(METRIC, Unit::Seconds, "Request latency");

    for latency in [0.04, 0.18, 0.25, 0.6] {
        histogram!(METRIC).record(latency);
    }

    print!("{}", handle.render());
}
```

Real output:

```text
# HELP request_latency_seconds Request latency
# TYPE request_latency_seconds histogram
request_latency_seconds_bucket{le="0.05"} 1
request_latency_seconds_bucket{le="0.1"} 1
request_latency_seconds_bucket{le="0.2"} 2
request_latency_seconds_bucket{le="0.5"} 3
request_latency_seconds_bucket{le="1"} 4
request_latency_seconds_bucket{le="+Inf"} 4
request_latency_seconds_sum 1.0699999999999998
request_latency_seconds_count 4
```

Two of the four requests (`0.04` and `0.18`) are at or under the 200ms SLO, which `le="0.2"` reports as `2`, exactly the number you can divide by `_count` to get SLO compliance.

</details>
