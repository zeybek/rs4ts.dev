---
title: "Distributed Tracing"
description: "Trace one request across services and await points with Rust's tracing crate and OpenTelemetry. Spans close automatically via RAII, unlike Node's OTel SDK."
---

Follow a single request as it hops across services, threads, and `await` points using the `tracing` crate and OpenTelemetry — the Rust equivalent of OpenTelemetry's Node SDK plus structured logging, but with spans woven into the type system.

---

## Quick Overview

**Distributed tracing** records the path of one logical request as a tree of **spans** (timed units of work) and stitches those spans together across process boundaries by passing a **trace context** in request headers. In Node you typically reach for `@opentelemetry/sdk-node` with auto-instrumentation; in Rust the `tracing` crate provides spans and structured events as a first-class facade, and `tracing-opentelemetry` exports them to any OpenTelemetry backend (Jaeger, Tempo, Honeycomb, Datadog).

For a TypeScript/JavaScript developer the key mental shift is that `tracing` unifies what you usually treat as two separate concerns, **structured logging** (`pino`/`winston`) and **tracing** (OpenTelemetry), behind one API. A `tracing` event is a log line; a `tracing` span is a trace span; both carry typed, structured fields.

> **Note:** This page covers tracing for observability — spans, context, and OpenTelemetry export. For numeric counters/gauges/histograms see [Metrics and Monitoring](/28-production/04-metrics/); for liveness/readiness probes see [Health and Readiness Endpoints](/28-production/03-health-checks/).

---

## TypeScript/JavaScript Example

A typical Node service wires up the OpenTelemetry SDK, then relies on auto-instrumentation plus a few manual spans. Context propagation across an outgoing HTTP call is handled by the instrumentation, but you often inject/extract headers by hand at service boundaries.

```typescript
// tracing.ts — initialize the OpenTelemetry SDK (run before anything else)
import { NodeSDK } from "@opentelemetry/sdk-node";
import { OTLPTraceExporter } from "@opentelemetry/exporter-trace-otlp-grpc";
import { resourceFromAttributes } from "@opentelemetry/resources";
import { ATTR_SERVICE_NAME } from "@opentelemetry/semantic-conventions";
import { getNodeAutoInstrumentations } from "@opentelemetry/auto-instrumentations-node";

const sdk = new NodeSDK({
  resource: resourceFromAttributes({ [ATTR_SERVICE_NAME]: "orders-api" }),
  traceExporter: new OTLPTraceExporter({ url: "http://localhost:4317" }),
  instrumentations: [getNodeAutoInstrumentations()],
});
sdk.start();
```

```typescript
// handler.ts — create a manual span and propagate context downstream
import { trace, context, propagation, SpanStatusCode } from "@opentelemetry/api";

const tracer = trace.getTracer("orders-api");

export async function getOrder(orderId: number): Promise<Order> {
  // Manual span around a unit of work
  return tracer.startActiveSpan("get_order", async (span) => {
    span.setAttribute("order.id", orderId);
    try {
      const order = await loadOrder(orderId);

      // Inject the active trace context into outgoing request headers
      const headers: Record<string, string> = {};
      propagation.inject(context.active(), headers);
      await fetch(`http://billing/charge/${orderId}`, { headers });

      return order;
    } catch (err) {
      span.setStatus({ code: SpanStatusCode.ERROR });
      throw err;
    } finally {
      span.end(); // easy to forget — leaks the span if you do
    }
  });
}
```

The ergonomics are familiar: you start a span, set attributes, and **must remember to `end()` it**. Propagation across a `fetch` is a manual `inject` into a plain headers object.

---

## Rust Equivalent

In Rust you annotate a function with `#[instrument]` and the span is created on entry and **closed automatically on return**. No `finally`, no leaked spans. Fields are typed and structured.

```rust
use tracing::{info, instrument, warn};
use tracing_subscriber::EnvFilter;

// Wrapping a function in a span: the span is named after the function and
// captures its arguments as fields. It is entered on call, closed on return.
#[instrument]
fn fetch_user(user_id: u64) -> String {
    info!("looking up user in database");
    if user_id == 0 {
        warn!("user id 0 is reserved");
    }
    format!("user-{user_id}")
}

// `skip(token)` keeps a secret out of the trace; we still log its length.
#[instrument(skip(token))]
fn authorize(user_id: u64, token: &str) -> bool {
    info!(token_len = token.len(), "checking token");
    !token.is_empty()
}

fn main() {
    // The "subscriber" is the global sink that collects spans and events.
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new("info"))
        .with_target(false)
        .init();

    let name = fetch_user(42);
    let ok = authorize(42, "secret-token");
    info!(user = %name, authorized = ok, "request handled");
}
```

```toml
# Cargo.toml
[dependencies]
tracing = "0.1.44"
tracing-subscriber = { version = "0.3.23", features = ["env-filter"] }
```

Real output (colors stripped via `NO_COLOR=1`):

```text
2026-06-02T06:37:43.077448Z  INFO fetch_user{user_id=42}: looking up user in database
2026-06-02T06:37:43.077529Z  INFO authorize{user_id=42}: checking token token_len=12
2026-06-02T06:37:43.077537Z  INFO request handled user=user-42 authorized=true
```

Notice how each event is prefixed with the span it occurred in (`fetch_user{user_id=42}:`). That span context is exactly what OpenTelemetry turns into a parent/child relationship in a trace.

---

## Detailed Explanation

**`#[instrument]` is the workhorse.** It is a procedural macro that wraps the function body in a span. The span name defaults to the function name, and every argument is recorded as a field using its `Debug` representation. On `async fn` it does the right thing automatically: the span follows the future across every `.await`, which is the part that is famously easy to get wrong by hand (see Common Pitfalls).

**Events vs. spans.** `info!`, `warn!`, `error!`, `debug!`, `trace!` emit *events*: point-in-time records, like log lines. A *span* (`#[instrument]`, `info_span!`, `span.enter()`) represents a *duration* of work with a start and end. Every event fires inside whatever spans are currently active, inheriting their context. This is the unification: one event is simultaneously a log line and a child of the current trace span.

**Field syntax.** Inside an event or `#[instrument(fields(...))]`:

- `field = value` records `value` via its `Display`/`Debug` as configured.
- `%value` forces the `Display` impl (`field = %name`).
- `?value` forces the `Debug` impl (`field = ?some_struct`).
- `field = expr` evaluates an arbitrary expression.

Unlike `console.log("user:", name)` where everything collapses to a string, these fields stay **structured** all the way to the backend, so you can query `token_len > 10` in Jaeger or Tempo.

**The subscriber is the sink.** `tracing` itself only emits; a `Subscriber` (here `tracing_subscriber::fmt()`) decides what to do with spans and events. This is the same split as a logging facade (`log`/`slf4j`) versus a concrete logger. Swapping the human-readable formatter for a JSON formatter or an OpenTelemetry exporter is a one-line change; your instrumentation never moves.

**`EnvFilter`** is the equivalent of `DEBUG=app:*` / log-level env vars. `EnvFilter::new("info")` shows `info` and above; `RUST_LOG=orders_api=debug,tower_http=warn` (read via `EnvFilter::from_default_env()`) lets you tune per-module verbosity at runtime without recompiling.

**Why no `span.end()`?** Rust's ownership model closes the span when its guard is dropped at the end of scope, the same RAII mechanism that frees memory. There is no `finally`, and there is no way to forget. This is a direct consequence of the [Drop trait](/05-ownership/08-drop-trait/) and [ownership rules](/05-ownership/).

---

## Key Differences

| Concern | TypeScript/JavaScript (OpenTelemetry SDK) | Rust (`tracing` + `tracing-opentelemetry`) |
| --- | --- | --- |
| Logging vs. tracing | Two libraries (`pino` + OTel SDK) | One facade: events *are* logs *and* trace data |
| Creating a span | `tracer.startActiveSpan(...)`, manual `span.end()` | `#[instrument]`, auto-closed on scope exit |
| Span fields | `span.setAttribute("k", v)`, stringly-typed | Typed structured fields, `Debug`/`Display` controlled |
| Async correctness | Relies on `AsyncLocalStorage` context | `#[instrument]` / `.instrument()` carry the span across `.await` |
| Forgetting to end | Leaks the span | Impossible — RAII closes it |
| Auto-instrumentation | Rich (http, pg, redis monkey-patched) | Library-provided spans (`tower-http`, `sqlx`) — opt in, no monkey-patching |
| Backend export | `NodeSDK` + exporter | A `tracing` layer + OpenTelemetry exporter |
| Sampling/perf when off | Runtime checks | Disabled spans compile to near-zero overhead |

The two genuinely important differences for a Node developer:

1. **There is no global monkey-patching.** Node's auto-instrumentation rewrites `http`, `pg`, etc. at load time. Rust libraries instead *ship* `tracing` spans (axum via `tower-http::TraceLayer`, `sqlx` via its own spans) that you enable explicitly. More boilerplate, zero hidden magic.

2. **Disabled instrumentation is nearly free.** A span filtered out by the subscriber costs a single atomic check; in JS, instrumentation overhead is paid regardless. You can leave `#[instrument]` everywhere in production.

---

## Common Pitfalls

### Pitfall 1: `#[instrument]` on a function with a non-`Debug` argument

`#[instrument]` records every argument by default, which requires each one to implement `Debug`. Passing a connection pool or other opaque type fails to compile:

```rust
use tracing::instrument;

// A type that does NOT implement Debug.
struct DbPool;

#[instrument] // does not compile (error[E0277]: `DbPool` doesn't implement `Debug`)
fn query(pool: DbPool, sql: &str) {
    println!("{sql}");
}

fn main() {
    query(DbPool, "SELECT 1");
}
```

The real compiler error:

```text
error[E0277]: `DbPool` doesn't implement `Debug`
   --> src/main.rs:6:1
    |
  6 | #[instrument]
    | ^^^^^^^^^^^^^ the trait `Debug` is not implemented for `DbPool`
    |
    = note: add `#[derive(Debug)]` to `DbPool` or manually `impl Debug for DbPool`
    = note: required for `&DbPool` to implement `Debug`
note: required by a bound in `debug`
help: consider annotating `DbPool` with `#[derive(Debug)]`
```

**Fix:** skip the argument with `#[instrument(skip(pool))]` (you rarely want a pool in a trace anyway), or `skip_all` and re-add the fields you care about. Never put secrets (tokens, passwords, PII) in fields; `skip` them.

### Pitfall 2: holding a span guard across `.await`

You can enter a span manually with `let _guard = span.enter()`. This works for synchronous code, but it is a **logic bug** in async code, and notably one the compiler does *not* catch:

```rust
use tracing::info_span;

async fn fetch() -> u32 { 42 }

async fn handler() {
    let span = info_span!("handler");
    let _guard = span.enter();   // compiles fine, but WRONG in async
    let value = fetch().await;   // span stays "entered" while the task is suspended
    println!("{value}");
}
```

This compiles and runs, so it is easy to ship. The problem is semantic: when the future suspends at `.await`, the guard is still alive, so on a multi-threaded runtime the executor may run an *unrelated* task while your span is marked active, attributing that task's events to the wrong span. **Fix:** use `#[instrument]` on the `async fn`, or `future.instrument(span).await`, both of which correctly exit the span on every suspension and re-enter on resume:

```rust
use tracing::{info, info_span, Instrument};

async fn fetch() -> u32 { 42 }

async fn handler() {
    let span = info_span!("handler");
    async {
        let value = fetch().await;
        info!(value, "fetched");
    }
    .instrument(span) // correct: span follows the future across .await
    .await;
}
```

### Pitfall 3: forgetting to flush the exporter on shutdown

OpenTelemetry batches spans and exports them in the background. If your process exits without shutting the provider down, the **last batch of spans is lost**: traces just before a crash or a normal exit go missing. Always call `provider.shutdown()` before `main` returns (or wire it into your [graceful shutdown](/28-production/02-graceful-shutdown/) path). Unlike a forgotten `span.end()`, this one *will* silently drop data.

### Pitfall 4: assuming events without a span are correlated

A bare `info!("processing")` with no enclosing span has no trace context. In a request handler, make sure the handler itself is instrumented (or sits under `tower-http::TraceLayer`) so events inherit a span and a `trace_id`.

---

## Best Practices

- **Instrument at boundaries.** Put `#[instrument]` on request handlers, service-layer functions, and any function that does I/O. Skip trivial helpers; spans have a (small) cost and clutter traces.
- **`skip` noisy and sensitive arguments.** Use `#[instrument(skip(pool, password), fields(user_id = %user.id))]` to keep traces clean and safe. Never trace secrets or PII.
- **Prefer `#[instrument]`/`.instrument()` over `.enter()` in async code.** Reserve `.enter()` for purely synchronous scopes.
- **Use `EnvFilter` from the environment.** Wire `RUST_LOG` so operators can raise verbosity without a redeploy.
- **Set a `Resource` with `service.name`.** Without it, your service shows up as `unknown_service` in the backend.
- **Use a batch exporter in production**, a simple/synchronous one only in tests. Batch reduces export overhead dramatically.
- **Emit JSON in production** so a log pipeline can index fields; keep the pretty formatter for local dev.
- **Propagate W3C `traceparent`** (the default) so traces join across services, including non-Rust ones.

The JSON formatter is one line and gives a structured, machine-parseable stream:

```rust
use tracing::{info, instrument};
use tracing_subscriber::EnvFilter;

#[instrument(fields(request_id = %uuid_stub()))]
fn handle_request(path: &str) {
    info!("received request");
    let user = load_user(7);
    info!(user = %user, "request complete");
}

#[instrument]
fn load_user(id: u64) -> String {
    info!("querying users table");
    format!("user-{id}")
}

fn uuid_stub() -> &'static str {
    "req-abc123"
}

fn main() {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(EnvFilter::new("info"))
        .with_current_span(true)
        .with_span_list(true)
        .init();

    handle_request("/users/7");
}
```

Real output (one JSON object per line; `spans` shows the full ancestry):

```text
{"timestamp":"2026-06-02T06:37:54.222585Z","level":"INFO","fields":{"message":"received request"},"target":"probe","span":{"path":"/users/7","request_id":"req-abc123","name":"handle_request"},"spans":[{"path":"/users/7","request_id":"req-abc123","name":"handle_request"}]}
{"timestamp":"2026-06-02T06:37:54.224232Z","level":"INFO","fields":{"message":"querying users table"},"target":"probe","span":{"id":7,"name":"load_user"},"spans":[{"path":"/users/7","request_id":"req-abc123","name":"handle_request"},{"id":7,"name":"load_user"}]}
{"timestamp":"2026-06-02T06:37:54.224512Z","level":"INFO","fields":{"message":"request complete","user":"user-7"},"target":"probe","span":{"path":"/users/7","request_id":"req-abc123","name":"handle_request"},"spans":[{"path":"/users/7","request_id":"req-abc123","name":"handle_request"}]}
```

---

## Real-World Example

A production axum service that (a) exports spans to an OpenTelemetry Collector over OTLP/gRPC, (b) **extracts** the upstream W3C trace context from incoming request headers so this service's spans become children of the caller's trace, and (c) uses `tower-http`'s `TraceLayer` to span every request.

The current OpenTelemetry Rust API is builder-based: `SpanExporter::builder().with_tonic().build()` creates the OTLP exporter, and `SdkTracerProvider::builder()...build()` wires it up. The repository's [pinned verification toolchain](/00-introduction/05-version-policy/) uses the 2024 edition; `cargo new` selects that edition automatically.

```toml
# Cargo.toml
[dependencies]
axum = "0.8.9"
tokio = { version = "1.52.3", features = ["full"] }
anyhow = "1.0.102"
tower-http = { version = "0.6.11", features = ["trace"] }
tracing = "0.1.44"
tracing-subscriber = { version = "0.3.23", features = ["env-filter", "json"] }
tracing-opentelemetry = "0.33.0"
opentelemetry = { version = "0.32.0", features = ["trace"] }
opentelemetry_sdk = { version = "0.32.1", features = ["rt-tokio"] }
opentelemetry-otlp = { version = "0.32.0", features = ["grpc-tonic"] }
```

```rust
use axum::extract::Path;
use axum::http::HeaderMap;
use axum::routing::get;
use axum::Router;
use opentelemetry::global;
use opentelemetry::propagation::Extractor;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::KeyValue;
use opentelemetry_otlp::SpanExporter;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry_sdk::Resource;
use tower_http::trace::TraceLayer;
use tracing::{info, info_span, instrument, Instrument};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

// Read W3C trace headers (traceparent/tracestate) off the incoming request.
struct HeaderExtractor<'a>(&'a HeaderMap);
impl Extractor for HeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|v| v.to_str().ok())
    }
    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|k| k.as_str()).collect()
    }
}

#[instrument(skip(pool))]
async fn load_order(pool: &str, order_id: u64) -> String {
    info!("querying database");
    format!("order #{order_id} from {pool}")
}

async fn get_order(headers: HeaderMap, Path(order_id): Path<u64>) -> String {
    // Continue the distributed trace started by the upstream caller.
    let parent_cx =
        global::get_text_map_propagator(|prop| prop.extract(&HeaderExtractor(&headers)));

    let span = info_span!("get_order", %order_id);
    // Make the extracted remote context this span's parent.
    let _ = span.set_parent(parent_cx);

    async move {
        info!("handling order request");
        load_order("pg-pool", order_id).await
    }
    .instrument(span)
    .await
}

fn init_telemetry() -> anyhow::Result<SdkTracerProvider> {
    // Use the W3C TraceContext propagator (the de-facto standard, matches Node).
    global::set_text_map_propagator(TraceContextPropagator::new());

    // OTLP/gRPC exporter — default endpoint http://localhost:4317.
    let exporter = SpanExporter::builder().with_tonic().build()?;

    let resource = Resource::builder()
        .with_service_name("orders-api")
        .with_attribute(KeyValue::new("deployment.environment", "production"))
        .build();

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(resource)
        .build();

    let tracer = provider.tracer("orders-api");

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_opentelemetry::layer().with_tracer(tracer))
        .init();

    Ok(provider)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let provider = init_telemetry()?;

    let app = Router::new()
        .route("/orders/{id}", get(get_order))
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    info!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;

    // Flush any buffered spans before exit so the last batch is not lost.
    provider.shutdown()?;
    Ok(())
}
```

> **Note:** This uses the current axum 0.8 path syntax `{id}` (not the old `:id`) and `axum::serve(listener, app)` (not the removed `Server::bind().serve()`). The OTLP exporter targets a running OpenTelemetry Collector or a Jaeger instance with OTLP enabled on port 4317; without one, `send()` simply fails to export and logs a warning rather than crashing your service.

The matching outbound side — **injecting** context into a downstream call so the trace continues — uses the symmetric `inject_context` and an `Injector`. Here is a self-contained, runnable demo of both halves using a plain `HashMap` as the carrier, which makes the actual `traceparent` header visible:

```rust
use std::collections::HashMap;

use opentelemetry::global;
use opentelemetry::propagation::{Extractor, Injector};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing::{info, info_span, instrument, Span};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

// Writes trace headers into an outgoing request.
struct HeaderInjector<'a>(&'a mut HashMap<String, String>);
impl Injector for HeaderInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        self.0.insert(key.to_string(), value);
    }
}

// Reads trace headers from an incoming request.
struct HeaderExtractor<'a>(&'a HashMap<String, String>);
impl Extractor for HeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(|s| s.as_str())
    }
    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|s| s.as_str()).collect()
    }
}

// Service A: starts a span, injects its context into outgoing headers.
#[instrument]
fn service_a_handler() -> HashMap<String, String> {
    info!("service A handling request");
    let mut headers = HashMap::new();
    let cx = Span::current().context();
    global::get_text_map_propagator(|prop| {
        prop.inject_context(&cx, &mut HeaderInjector(&mut headers));
    });
    headers
}

// Service B: extracts the upstream context and adopts it as the parent.
fn service_b_handler(headers: &HashMap<String, String>) {
    let parent_cx =
        global::get_text_map_propagator(|prop| prop.extract(&HeaderExtractor(headers)));
    let span = info_span!("service_b_handler");
    let _ = span.set_parent(parent_cx);
    let _guard = span.enter(); // synchronous code — .enter() is fine here
    info!("service B continued the upstream trace");
}

fn main() {
    global::set_text_map_propagator(TraceContextPropagator::new());

    let provider = SdkTracerProvider::builder().build();
    let tracer = provider.tracer("propagation-demo");

    tracing_subscriber::registry()
        .with(EnvFilter::new("info"))
        .with(tracing_subscriber::fmt::layer().without_time())
        .with(tracing_opentelemetry::layer().with_tracer(tracer))
        .init();

    let headers = service_a_handler();
    println!("--- headers passed between services ---");
    for (k, v) in &headers {
        println!("{k}: {v}");
    }
    println!("---------------------------------------");
    service_b_handler(&headers);
}
```

Real output (`NO_COLOR=1`; the `traceparent` value is non-deterministic):

```text
 INFO service_a_handler: probe: service A handling request
--- headers passed between services ---
traceparent: 00-7c6ef9ae1f7cac5d3cb9108d400378ee-d2e7e5752bc0dca4-01
tracestate: 
---------------------------------------
 INFO service_b_handler: probe: service B continued the upstream trace
```

The `traceparent` header is the W3C standard format `version-trace_id-span_id-flags`. Because service B extracts that `trace_id` and calls `set_parent`, its span lands under the same trace in your backend: the request becomes one connected tree across both processes. This is identical to what OpenTelemetry's JS `propagation.inject`/`extract` produce, which is why a Rust service and a Node service interoperate out of the box.

---

## Further Reading

- [`tracing` crate documentation](https://docs.rs/tracing) — spans, events, the `#[instrument]` macro
- [`tracing-subscriber` documentation](https://docs.rs/tracing-subscriber) — formatters, `EnvFilter`, layered subscribers
- [`tracing-opentelemetry` documentation](https://docs.rs/tracing-opentelemetry) — the bridge layer used above
- [OpenTelemetry Rust](https://opentelemetry.io/docs/languages/rust/) — exporters, the SDK, and the Collector
- [W3C Trace Context spec](https://www.w3.org/TR/trace-context/) — the `traceparent`/`tracestate` header format
- [Tokio Tracing topic guide](https://tokio.rs/tokio/topics/tracing) — async-aware tracing in depth
- Related guide sections:
  - [Metrics and Monitoring](/28-production/04-metrics/) — numeric signals (RED/USE) to complement traces
  - [Health and Readiness Endpoints](/28-production/03-health-checks/) — liveness/readiness endpoints
  - [Graceful Shutdown](/28-production/02-graceful-shutdown/) — where to flush the exporter on exit
  - [Environment-Based Configuration](/28-production/01-environment/) and [Application Configuration](/28-production/00-configuration/) — supplying the OTLP endpoint and `RUST_LOG`
  - [The Drop trait](/05-ownership/08-drop-trait/) — why spans close without a manual `end()`
  - [Output and Formatting](/02-basics/04-output/) — `println!` vs. structured `tracing` events
  - [Migration guide](/29-migration-guide/) — moving a Node service (and its OTel setup) to Rust

---

## Exercises

### Exercise 1: Record a field you only know later

**Difficulty:** Beginner

**Objective:** Learn to declare a span field up front and fill it in once the value is computed: the pattern for recording a result (row count, status code) on the span that produced it.

**Instructions:** Write a function `run_query(sql: &str) -> usize` instrumented so its span declares an empty `rows` field. Inside, after "running" the query (just return a constant), record the row count onto the current span with `Span::current().record("rows", ...)`. Emit an `info!` before and after. Initialize a plain `fmt` subscriber.

<details>
<summary>Solution</summary>

```rust
use tracing::{info, instrument, Span};
use tracing_subscriber::EnvFilter;

// Declare an empty field, then fill it in once the value is known.
#[instrument(fields(rows = tracing::field::Empty))]
fn run_query(sql: &str) -> usize {
    info!("executing query");
    let rows = 7; // pretend we actually ran it
    Span::current().record("rows", rows);
    rows
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new("info"))
        .with_target(false)
        .without_time()
        .init();

    let n = run_query("SELECT * FROM users");
    info!(returned = n, "done");
}
```

Real output:

```text
 INFO run_query{sql="SELECT * FROM users"}: executing query
 INFO done returned=7
```

The `rows` field is attached to the span (visible when the span itself is reported by a backend or a span-aware formatter), while `returned=7` is a field on the final event. The key technique is `tracing::field::Empty` to reserve a slot plus `Span::current().record(...)` to populate it.

</details>

### Exercise 2: Inject trace context into an outgoing `reqwest` call

**Difficulty:** Intermediate

**Objective:** Implement the outbound half of context propagation so a downstream HTTP service joins the same trace.

**Instructions:** Implement an `Injector` for a `reqwest::header::HeaderMap`. Write `async fn call_downstream(client: &reqwest::Client, url: &str)` that grabs the current span's OpenTelemetry context, injects it into a fresh `HeaderMap` via the active text-map propagator, and sends a GET with those headers. (`reqwest` with `default-features = false, features = ["json"]`.)

<details>
<summary>Solution</summary>

```rust
use opentelemetry::global;
use opentelemetry::propagation::Injector;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

// Carrier that injects W3C trace headers into a reqwest header map.
struct ReqwestInjector<'a>(&'a mut reqwest::header::HeaderMap);
impl Injector for ReqwestInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        if let (Ok(name), Ok(val)) = (
            reqwest::header::HeaderName::from_bytes(key.as_bytes()),
            reqwest::header::HeaderValue::from_str(&value),
        ) {
            self.0.insert(name, val);
        }
    }
}

async fn call_downstream(client: &reqwest::Client, url: &str) -> reqwest::Result<()> {
    let mut headers = reqwest::header::HeaderMap::new();
    let cx = Span::current().context();
    global::get_text_map_propagator(|prop| {
        prop.inject_context(&cx, &mut ReqwestInjector(&mut headers));
    });
    let _resp = client.get(url).headers(headers).send().await?;
    Ok(())
}

fn main() {
    // Compile-only demo: shows the injection wiring.
    let _ = call_downstream;
}
```

```toml
# Cargo.toml additions
[dependencies]
reqwest = { version = "0.13.4", default-features = false, features = ["json"] }
opentelemetry = { version = "0.32.0", features = ["trace"] }
opentelemetry_sdk = { version = "0.32.1", features = ["rt-tokio"] }
tracing = "0.1.44"
tracing-opentelemetry = "0.33.0"
```

The `inject_context` call writes `traceparent` (and `tracestate`) into `headers`; the downstream service's `Extractor` (Exercise/Real-World pattern) reads them back and calls `set_parent`, joining both processes into one trace.

</details>

### Exercise 3: Per-module log levels with `EnvFilter`

**Difficulty:** Intermediate

**Objective:** Use `RUST_LOG`-style directives to silence a noisy dependency while keeping your own module verbose.

**Instructions:** Build a subscriber whose filter is read from the environment, falling back to a directive that sets your crate to `debug` but a (simulated) `noisy` module to `warn`. Emit a `debug!` from your code and a `warn!` "from" the noisy module (use `#[instrument]`/events inside a `mod noisy`), and confirm only the warning from the noisy module appears while your debug line shows.

<details>
<summary>Solution</summary>

```rust
use tracing::{debug, warn};
use tracing_subscriber::EnvFilter;

mod noisy {
    use tracing::{debug, warn};
    pub fn chatter() {
        debug!("noisy debug — should be filtered out");
        warn!("noisy warning — should appear");
    }
}

fn main() {
    // Operators can override via RUST_LOG; otherwise use this default.
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("exercise=debug,exercise::noisy=warn"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .without_time()
        .init();

    debug!("my own debug — should appear");
    noisy::chatter();
}
```

Run it with the crate named `exercise` (`cargo new --name exercise ...`). With no `RUST_LOG` set, real output is:

```text
DEBUG exercise: my own debug — should appear
 WARN exercise::noisy: noisy warning — should appear
```

The `exercise::noisy=warn` directive raises the threshold for just that module, so its `debug!` is dropped while your crate-level `debug!` survives, the same idea as `DEBUG=app:*,-app:db` in a Node app, but resolved per module path. Setting `RUST_LOG=trace` at runtime overrides the fallback entirely with no recompile.

</details>
