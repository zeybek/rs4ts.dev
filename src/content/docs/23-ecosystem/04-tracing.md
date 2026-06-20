---
title: "Structured Logging and Spans with `tracing`"
description: "Move past flat logs: tracing adds structured fields and spans, the Rust answer to pino plus AsyncLocalStorage, with #[instrument] and JSON output."
---

## Quick Overview

`tracing` is Rust's structured, context-aware diagnostics framework: the natural step up from the [`log` facade](/23-ecosystem/03-logging/) once your program is asynchronous, concurrent, or producing logs that a machine has to read. Instead of flat lines of text, `tracing` records **events** (a point in time) that happen inside **spans** (a period of time, like "handling this request"), so every log line automatically carries the structured context of where it came from. For a TypeScript/JavaScript developer this is the Rust equivalent of moving from `console.log` strings to a structured logger like `pino` plus the request-scoped context you would otherwise thread through with `AsyncLocalStorage`.

> **Note:** `tracing` and the `log` crate interoperate. Libraries can keep emitting `log` records and your application can capture them through `tracing` (shown later), so adopting `tracing` is never an all-or-nothing rewrite. If you only need simple level-filtered text logs, start with [`log` + `env_logger`](/23-ecosystem/03-logging/); reach for `tracing` when you need spans, async context, or JSON.

---

## TypeScript/JavaScript Example

A realistic Node service wants two things flat `console.log` cannot easily give: **structured fields** (so a log aggregator can index `orderId`) and **request-scoped context** (so every line emitted while handling one order is tagged with that order's id, even across `await` points). Teams reach for `pino` for the first and `AsyncLocalStorage` for the second:

```typescript
// orders.ts — structured logging with pino + request context via AsyncLocalStorage
import pino from "pino";
import { AsyncLocalStorage } from "node:async_hooks";

const log = pino({ level: process.env.LOG_LEVEL ?? "info" });

// Holds per-request context that should appear on every log line.
const requestContext = new AsyncLocalStorage<{ orderId: number; userId: number }>();

function ctxLog() {
  // Merge the active request context into each log call.
  const store = requestContext.getStore();
  return store ? log.child(store) : log;
}

async function processOrder(orderId: number, userId: number, amountCents: number): Promise<void> {
  // Everything awaited inside this callback sees the same context.
  await requestContext.run({ orderId, userId }, async () => {
    ctxLog().info("processing order");

    if (amountCents === 0) {
      ctxLog().error("empty cart");
      return;
    }

    ctxLog().debug({ gateway: "stripe" }, "submitting charge");
    await new Promise((r) => setTimeout(r, 5)); // simulate async I/O

    if (amountCents > 1_000_000) {
      ctxLog().warn({ threshold: 1_000_000 }, "charge exceeds manual-review threshold");
    }

    ctxLog().info({ txnId: 0xabcd }, "charge confirmed");
  });
}

await processOrder(1001, 42, 4_999);
```

Two things stand out. First, `pino` emits structured JSON where fields like `orderId` are real keys, not interpolated into a string. Second, `AsyncLocalStorage` is the only way to make context "follow" the logic across `await` without manually passing it to every function. Rust's `tracing` gives you both, structured fields and span context that survives `.await`, as first-class, compiler-checked features.

---

## Rust Equivalent

Add the framework (`tracing`) and a **subscriber** that decides what to do with the data (`tracing-subscriber`). The `env-filter` feature gives you `RUST_LOG`-style filtering:

```bash
cargo add tracing
cargo add tracing-subscriber --features env-filter
```

```rust
// src/main.rs
use tracing::{debug, error, info, instrument, warn};
use tracing_subscriber::EnvFilter;

#[derive(Debug)]
struct Order {
    id: u64,
    amount_cents: u64,
}

#[derive(Debug)]
enum CheckoutError {
    EmptyCart,
}

// `#[instrument]` wraps the function body in a span. `skip(order)` keeps the
// whole struct out of the span; `fields(...)` records just the parts we want.
#[instrument(skip(order), fields(order_id = order.id, amount = order.amount_cents))]
fn process_order(order: &Order, user_id: u64) -> Result<u64, CheckoutError> {
    info!("processing order");

    if order.amount_cents == 0 {
        error!("empty cart");
        return Err(CheckoutError::EmptyCart);
    }

    debug!(gateway = "stripe", "submitting charge");

    if order.amount_cents > 1_000_000 {
        warn!(threshold = 1_000_000, "charge exceeds manual-review threshold");
    }

    let txn_id = 0xABCD;
    info!(txn_id, "charge confirmed");
    Ok(txn_id)
}

fn main() {
    // Install a subscriber: text output, RUST_LOG override, default "info".
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let orders = [
        (Order { id: 1001, amount_cents: 4_999 }, 42u64),
        (Order { id: 1002, amount_cents: 0 }, 43),
        (Order { id: 1003, amount_cents: 5_000_000 }, 44),
    ];

    for (order, user_id) in &orders {
        match process_order(order, *user_id) {
            Ok(txn) => info!(txn, "order completed"),
            Err(e) => warn!(error = ?e, "order rejected"),
        }
    }
}
```

Running it produces real, span-tagged output (the timestamps and the `process_order{...}` prefix are emitted automatically):

```text
$ cargo run
2026-06-02T06:29:35.618823Z  INFO process_order{user_id=42 order_id=1001 amount=4999}: probe: processing order
2026-06-02T06:29:35.618840Z  INFO process_order{user_id=42 order_id=1001 amount=4999}: probe: charge confirmed txn_id=43981
2026-06-02T06:29:35.618863Z  INFO probe: order completed txn=43981
2026-06-02T06:29:35.619681Z  INFO process_order{user_id=43 order_id=1002 amount=0}: probe: processing order
2026-06-02T06:29:35.619699Z ERROR process_order{user_id=43 order_id=1002 amount=0}: probe: empty cart
2026-06-02T06:29:35.619710Z  WARN probe: order rejected error=EmptyCart
2026-06-02T06:29:35.619721Z  INFO process_order{user_id=44 order_id=1003 amount=5000000}: probe: processing order
2026-06-02T06:29:35.619727Z  WARN process_order{user_id=44 order_id=1003 amount=5000000}: probe: charge exceeds manual-review threshold threshold=1000000
2026-06-02T06:29:35.620940Z  INFO process_order{user_id=44 order_id=1003 amount=5000000}: probe: charge confirmed txn_id=43981
2026-06-02T06:29:35.621000Z  INFO probe: order completed txn=43981
```

Notice what came for free: every line emitted *inside* `process_order` is automatically prefixed with `process_order{user_id=… order_id=… amount=…}` — the span context — while the `order completed` / `order rejected` lines emitted from `main` are not, because they are outside the span. That is the Rust analogue of `AsyncLocalStorage`, but achieved by one attribute and checked at compile time. The default `fmt` subscriber colorizes this output in a terminal; piped to a file (as above) it is plain text.

---

## Detailed Explanation

### Events vs. spans: the core idea

`tracing` has two primitives, and grasping the distinction is the whole game:

- An **event** is a single moment, the direct equivalent of one `console.log`/`info!` call. You emit events with the `event!` macro or the level shortcuts `error!`, `warn!`, `info!`, `debug!`, `trace!` (deliberately the same names as the `log` crate).
- A **span** represents a *period of time* with a beginning and an end — "while we were handling request 7", "while this DB query ran". A span can have **fields** (structured key-values), and while a span is **entered**, every event recorded (and every child span opened) is associated with it.

In Node you simulate span context with `AsyncLocalStorage`; in `tracing` it is the foundational data model. A span is opened, entered (made the current context), exited, and eventually closed.

### Structured fields, not string interpolation

Each macro accepts structured fields *before* the message:

```rust playground
use tracing::info;

fn main() {
    tracing_subscriber::fmt().init();
    let txn_id = 0xABCDu64;
    info!(txn_id, gateway = "stripe", "charge confirmed");
}
```

- `txn_id` uses **field shorthand**: a bare identifier records a field named `txn_id` with that variable's value (the same ergonomic capture you get in `format!("{txn_id}")`).
- `gateway = "stripe"` records an explicit key-value pair.
- The final string literal is the event's `message` field.

Fields are captured by their `Display` representation by default. Two sigils change that:

- `?value` captures via `Debug` (`info!(order = ?order)` records the `#[derive(Debug)]` form).
- `%value` captures via `Display` explicitly (useful when the default would otherwise pick something else).

This is why the order example wrote `warn!(error = ?e, ...)`: `CheckoutError` has a `Debug` impl but no `Display`, so `?e` selects `Debug`.

### `#[instrument]`: a span for a whole function

The `#[instrument]` attribute is the most ergonomic way to create spans. It wraps the function body in a span named after the function and **records every argument as a field automatically**. That convenience comes with a constraint and some knobs:

- Because arguments become fields, each argument type must implement `Debug`, or you must exclude it. `skip(order)` drops an argument from the span entirely (essential for large structs, secrets, or non-`Debug` types).
- `fields(order_id = order.id)` adds computed fields, letting you record `order.id` even though you skipped the whole `order`.
- It works on `async fn` too, and there it does something `.enter()` alone cannot do correctly, covered next.

### Spans and `async`: why `.enter()` is not enough

This is the single most important async subtlety. A naive approach uses a span guard:

```rust
use tracing::info_span;

fn sync_work() {
    let span = info_span!("work");
    let _guard = span.enter(); // guard exits the span when dropped
    // ... synchronous work; every event here is inside "work" ...
}
```

For synchronous code this is correct. For `async` code it is **wrong**: if the future yields at an `.await` while the guard is held, the span stays "entered" on that thread while a *different* task runs there, leaking context across tasks. The fix is to attach the span to the *future* so it is entered only while that future is actively polled:

```rust
use tracing::{info, info_span, Instrument};

async fn fetch(id: u64) -> String {
    // The `Instrument` trait adds `.instrument(span)` to any future.
    async {
        info!("fetching");
        format!("user-{id}")
    }
    .instrument(info_span!("fetch", id))
    .await
}
```

`#[instrument]` on an `async fn` does exactly this for you, which is why it is the recommended default for async functions. The takeaway: use `#[instrument]` (or `.instrument(span)`), **not** a bare `span.enter()`, inside `async`.

### Recording fields after the fact

Sometimes a field's value is unknown when the span opens. Declare it as empty and fill it in later:

```rust
use tracing::{field, info, info_span};

fn run() {
    let span = info_span!("db_query", table = "orders", rows = field::Empty);
    let _guard = span.enter();
    info!("executing query");
    let row_count = 17;
    span.record("rows", row_count); // now the span carries rows=17
    info!("query finished");
}
```

Real output:

```text
2026-06-02T06:30:24.683965Z  INFO db_query{table="orders"}: probe: executing query
2026-06-02T06:30:24.684439Z  INFO db_query{table="orders" rows=17}: probe: query finished
```

`field::Empty` reserves the slot; `record` populates it. (A field never declared on the span cannot be recorded later.)

### Subscriber, layers, and `init()`

Like `log`, `tracing` separates *emitting* data from *consuming* it. The consumer is a **`Subscriber`**; until one is installed as the default, every event and span is a cheap no-op. `tracing_subscriber::fmt()` builds the common case: a subscriber that formats events to stderr. The more composable form is a **`Registry`** with **layers** stacked on top:

```rust
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

fn main() {
    tracing_subscriber::registry()
        .with(EnvFilter::new("info"))      // filtering layer
        .with(fmt::layer())                 // formatting layer
        .init();
}
```

A `Layer` is a composable piece of subscriber behavior: a formatter, a filter, an OpenTelemetry exporter, a metrics collector. You stack the ones you need. This layering is `tracing`'s superpower over `log`: you can send the same events to a human-readable console *and* a JSON file *and* a distributed-tracing backend simultaneously.

### `EnvFilter` and `RUST_LOG`

`EnvFilter` understands the same `RUST_LOG` directive grammar as `env_logger`, including per-module targets:

```text
$ RUST_LOG=info cargo run
2026-06-02T06:33:47.384482Z  INFO probe: app started
2026-06-02T06:33:47.384533Z  INFO probe::db: running query

$ RUST_LOG="warn,probe::db=debug" cargo run
2026-06-02T06:33:47.872126Z  INFO probe::db: running query
2026-06-02T06:33:47.872164Z DEBUG probe::db: connection pool: 3/10 in use
```

The second invocation sets a global floor of `warn` (silencing the top-level `info!`/`debug!`) while raising the `probe::db` module to `debug`: independent dials per subsystem, exactly as with `env_logger`. `EnvFilter` additionally supports span-aware filtering (e.g. `[span_name]=level`) that `env_logger` cannot express.

---

## Key Differences

| Concern | Node.js (`pino` + `AsyncLocalStorage`) | Rust (`tracing` + `tracing-subscriber`) |
| --- | --- | --- |
| Primitive | Log calls; context via `AsyncLocalStorage` | **Events** (points) and **spans** (periods) as first-class data |
| Structured fields | `log.info({ key }, "msg")` | `info!(key = value, "msg")`, typed, compiler-checked |
| Request context across `await` | Manual `AsyncLocalStorage.run` | `#[instrument]` / `.instrument(span)`, automatic |
| Backend selection | Pick `pino`/`winston` at import | Install one `Subscriber`; stack composable `Layer`s |
| Output formats | JSON (pino) or pretty (`pino-pretty`) | Pretty, compact, or JSON via `fmt` layer; swap freely |
| Filtering | `level` option / transport config | `EnvFilter` with `RUST_LOG` grammar + span filters |
| Cost when disabled | You guard expensive work yourself | Disabled spans/events compile to near-nothing |
| Interop with other loggers | N/A | Captures `log`-crate records via `tracing-log` |

The mental shift: in Node, "context that follows execution" is a bolt-on (`AsyncLocalStorage`) and structured logging is a library choice (`pino`). In Rust, both are the native data model: a span *is* the context, and fields *are* structured by construction.

> **Note:** `tracing` is the foundation of Rust's observability story, not just logging. The same spans you emit for logs can be exported to distributed-tracing systems (Jaeger, Tempo, any OpenTelemetry backend) by adding a layer; no code change to your `info!` calls. That is why async-heavy crates like `tokio`, `hyper`, and `axum` are instrumented with `tracing` out of the box.

---

## Common Pitfalls

### Forgetting to install a subscriber

The most common surprise mirrors the `log` facade: events compile fine but produce **no output** because no subscriber was installed.

```rust playground
use tracing::info;

fn main() {
    // No subscriber installed — every event is a silent no-op.
    info!("you will never see this");
    println!("program finished");
}
```

```text
$ RUST_LOG=trace cargo run
program finished
```

This is by design (libraries must stay silent when no consumer is present). The fix is one line early in `main`: `tracing_subscriber::fmt().init();`.

### Using `span.enter()` inside `async`

Holding a span guard across an `.await` is the classic `tracing` bug. It compiles and may even *look* right in single-task tests, but under real concurrency the span leaks onto whatever task happens to be polled next. Clippy will **not** catch this for you: unlike a `std::sync::MutexGuard` (which `clippy::await_holding_lock` flags), a `tracing` span's `Entered` guard is not detected, so this stays a silent logic bug.

The rule is simple: never hold a `span.enter()` guard across `.await`. Use `#[instrument]` on the `async fn`, or attach the span to the future with `.instrument(span)`. Both enter the span only while the future is actively polled.

### Spawned tasks do not inherit the current span

`tokio::spawn` starts an independent task; the new future does **not** automatically inherit the span that was current at the spawn site. The result is a log line missing its request context:

```rust playground
use tracing::{info, info_span, Instrument};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_max_level(tracing::Level::INFO).init();

    let span = info_span!("request", request_id = "req-001");
    let _enter = span.enter();

    // BUG: the spawned task does NOT inherit the current span.
    let bug = tokio::spawn(async {
        info!("work in spawned task (no span)");
    });

    // FIX: capture the current span and attach it to the future before spawning.
    let fixed = tokio::spawn(
        async {
            info!("work in spawned task (span attached)");
        }
        .in_current_span(),
    );

    bug.await.unwrap();
    fixed.await.unwrap();
}
```

```text
2026-06-02T06:31:26.089010Z  INFO probe: work in spawned task (no span)
2026-06-02T06:31:26.089085Z  INFO request{request_id="req-001"}: probe: work in spawned task (span attached)
```

The first line lost its `request_id`; the second kept it. The fix is `.in_current_span()` (or `.instrument(span)`) on the future passed to `tokio::spawn`.

### `#[instrument]` requires `Debug` on every captured argument

Because `#[instrument]` records arguments as fields, each argument type must implement `Debug`. Forgetting this produces a real compiler error:

```rust
use tracing::instrument;

struct Connection {
    _handle: u64,
}

#[instrument] // does not compile (error[E0277]: `Connection` doesn't implement `Debug`)
fn run_query(conn: &Connection, sql: &str) {
    println!("{sql}");
    let _ = conn;
}

fn main() {
    tracing_subscriber::fmt().init();
    run_query(&Connection { _handle: 1 }, "SELECT 1");
}
```

The real message:

```text
error[E0277]: `Connection` doesn't implement `Debug`
  --> src/main.rs:7:1
   |
 7 | #[instrument]
   | ^^^^^^^^^^^^^ the trait `Debug` is not implemented for `Connection`
   |
   = note: add `#[derive(Debug)]` to `Connection` or manually `impl Debug for Connection`
   = note: required for `&Connection` to implement `Debug`
```

Two fixes: `#[instrument(skip(conn))]` to exclude the argument, or `#[derive(Debug)]` on `Connection`. Always `skip` large structs, secrets, and database connections.

### Installing the subscriber after the first events

`tracing_subscriber`'s `init()` sets the *global default*. Any event emitted before that call is dropped silently. Initialize the subscriber at the very top of `main`, before anything else logs.

### Calling `init()` twice

Like installing a global logger, the global default subscriber can be set only once. A second `init()` (or two components racing to install one) panics with `SetGlobalDefaultError`. Use `try_init()` and handle the `Result` when something else might also initialize.

---

## Best Practices

- **Libraries depend only on `tracing`; the binary installs the subscriber.** Exactly like the `log` facade convention: a reusable crate emits spans and events and never picks a subscriber. (See [Popular Crates and the npm Packages They Replace](/23-ecosystem/00-popular-crates/) for where `tracing` sits in the ecosystem.)
- **Initialize the subscriber once, at the top of `main`.** Prefer `tracing_subscriber::fmt().with_env_filter(...).init()` for simple apps, or a layered `registry()` when you need multiple outputs.
- **Default to `#[instrument]` for functions worth tracing.** Add `skip(...)` for big or sensitive arguments and `fields(...)` for the specific values you want. On `async fn`, it is the *correct* way to scope a span across `.await`.
- **Never hold a `span.enter()` guard across `.await`.** Use `#[instrument]` or `.instrument(span)` instead.
- **Propagate context into spawned tasks** with `.in_current_span()` or `.instrument(span)`.
- **Pick the right level.** `error!` for failures an operator must act on; `warn!` for recoverable anomalies; `info!` for lifecycle/request events; `debug!` for developer diagnostics; `trace!` for high-volume detail.
- **Use `EnvFilter` with a sensible default** (e.g. `EnvFilter::new("info")` when `RUST_LOG` is unset) so a fresh checkout shows useful output.
- **Emit JSON in production, pretty text in development.** Drive the choice from configuration; the rest of your instrumentation does not change.
- **Bridge the `log` crate** with the `tracing-log` feature so records from dependencies that use `log` (and there are many) appear in your unified pipeline.

---

## JSON Logs

Production log aggregators (Loki, Elasticsearch, Datadog, CloudWatch) want **newline-delimited JSON**, not pretty text; the same reason Node services run `pino` in JSON mode. `tracing-subscriber` produces it with the `json` feature:

```bash
cargo add tracing-subscriber --features env-filter,json
```

```rust
use tracing::{info, instrument, warn};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[derive(Debug)]
struct Job {
    id: u64,
    payload_bytes: usize,
}

#[instrument(skip(job), fields(job_id = job.id, bytes = job.payload_bytes))]
async fn process_job(job: &Job) -> bool {
    info!("job started");
    if job.payload_bytes > 1_000 {
        warn!(limit = 1_000, "payload exceeds size limit");
        return false;
    }
    info!("job completed");
    true
}

#[tokio::main]
async fn main() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(filter)
        // The JSON layer emits one line of newline-delimited JSON per event.
        .with(fmt::layer().json().with_current_span(true).with_span_list(false))
        .init();

    for job in &[Job { id: 1, payload_bytes: 512 }, Job { id: 2, payload_bytes: 8_192 }] {
        process_job(job).await;
    }
}
```

Real output — one JSON object per line, with the active span's fields nested under `"span"`:

```text
{"timestamp":"2026-06-02T06:32:35.279209Z","level":"INFO","fields":{"message":"job started"},"target":"probe","span":{"bytes":512,"job_id":1,"name":"process_job"}}
{"timestamp":"2026-06-02T06:32:35.279285Z","level":"INFO","fields":{"message":"job completed"},"target":"probe","span":{"bytes":512,"job_id":1,"name":"process_job"}}
{"timestamp":"2026-06-02T06:32:35.279310Z","level":"INFO","fields":{"message":"job started"},"target":"probe","span":{"bytes":8192,"job_id":2,"name":"process_job"}}
{"timestamp":"2026-06-02T06:32:35.279323Z","level":"WARN","fields":{"message":"payload exceeds size limit","limit":1000},"target":"probe","span":{"bytes":8192,"job_id":2,"name":"process_job"}}
```

Each line is valid JSON your aggregator can index by `level`, `target`, `span.job_id`, or any custom field. Useful knobs on the JSON formatter:

- `.with_current_span(true)` — include the immediately-active span's fields under `"span"`.
- `.with_span_list(true)` — include the *full stack* of open spans under `"spans"` (set `false`, as above, when you only want the leaf).
- `.flatten_event(true)` — lift the event's own fields to the top level instead of nesting them under `"fields"`.

The decisive advantage over Node here is that you flip between human-readable and JSON output by swapping one layer; your `info!`/`#[instrument]` instrumentation is identical in both modes.

---

## Real-World Example

A production-flavored batch worker: each job runs inside its own span (so a log search by `job_id` reconstructs that job's entire lifecycle), spans nest to show causality, and the subscriber is a composable registry honoring `RUST_LOG`. The `validate` step is its own child span, demonstrating the nesting that flat logging cannot express.

```bash
cargo add tracing
cargo add tracing-subscriber --features env-filter
cargo add tokio --features rt-multi-thread,macros,time
```

```rust
use std::time::Duration;
use tracing::{error, info, instrument, warn};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[derive(Debug)]
struct Job {
    id: u64,
    payload_bytes: usize,
}

#[derive(Debug)]
enum JobError {
    TooLarge,
}

/// Process a single job. The span carries `job_id`; every event emitted while
/// this function (and anything it calls) runs is tagged with that context.
#[instrument(skip(job), fields(job_id = job.id, bytes = job.payload_bytes))]
async fn process_job(job: &Job) -> Result<(), JobError> {
    info!("job started");

    if job.payload_bytes > 1_000 {
        error!(limit = 1_000, "payload exceeds size limit");
        return Err(JobError::TooLarge);
    }

    validate(job).await;

    tokio::time::sleep(Duration::from_millis(2)).await; // simulate work
    info!("job completed");
    Ok(())
}

#[instrument(skip(job), fields(job_id = job.id))]
async fn validate(job: &Job) {
    if job.payload_bytes == 0 {
        warn!("empty payload, treating as no-op");
    }
    info!("validated");
}

#[tokio::main]
async fn main() {
    // Compose a layered subscriber: a filter layer + a text fmt layer.
    // RUST_LOG overrides the "info" default if set.
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(false))
        .init();

    info!(worker = "batch-1", "worker online");

    let jobs = [
        Job { id: 1, payload_bytes: 512 },
        Job { id: 2, payload_bytes: 0 },
        Job { id: 3, payload_bytes: 8_192 },
    ];

    let mut ok = 0u32;
    let mut failed = 0u32;
    for job in &jobs {
        match process_job(job).await {
            Ok(()) => ok += 1,
            Err(e) => {
                warn!(error = ?e, job_id = job.id, "job failed");
                failed += 1;
            }
        }
    }

    info!(ok, failed, "batch finished");
}
```

Real output (`with_target(false)` drops the `probe:` column to reduce noise):

```text
2026-06-02T06:31:46.616151Z  INFO worker online worker="batch-1"
2026-06-02T06:31:46.616314Z  INFO process_job{job_id=1 bytes=512}: job started
2026-06-02T06:31:46.616328Z  INFO process_job{job_id=1 bytes=512}:validate{job_id=1}: validated
2026-06-02T06:31:46.620742Z  INFO process_job{job_id=1 bytes=512}: job completed
2026-06-02T06:31:46.620937Z  INFO process_job{job_id=2 bytes=0}: job started
2026-06-02T06:31:46.621000Z  WARN process_job{job_id=2 bytes=0}:validate{job_id=2}: empty payload, treating as no-op
2026-06-02T06:31:46.621055Z  INFO process_job{job_id=2 bytes=0}:validate{job_id=2}: validated
2026-06-02T06:31:46.623881Z  INFO process_job{job_id=2 bytes=0}: job completed
2026-06-02T06:31:46.623980Z  INFO process_job{job_id=3 bytes=8192}: job started
2026-06-02T06:31:46.624009Z ERROR process_job{job_id=3 bytes=8192}: payload exceeds size limit limit=1000
2026-06-02T06:31:46.624039Z  WARN job failed error=TooLarge job_id=3
2026-06-02T06:31:46.624055Z  INFO batch finished ok=2 failed=1
```

The nesting `process_job{...}:validate{...}:` is the payoff: a log line from deep inside `validate` still tells you which job it belongs to, with zero manual context-passing. Swap `fmt::layer()` for `fmt::layer().json()` and the very same instrumentation produces machine-readable logs for your aggregator: the instrumentation is the asset, the output format is a configuration choice.

### Capturing the `log` crate into `tracing`

Many dependencies emit `log`-crate records rather than `tracing` events. Enable the `tracing-log` feature so `tracing_subscriber`'s `init()` installs the bridge automatically, and those records flow into the same pipeline:

```bash
cargo add tracing-subscriber --features env-filter,tracing-log
cargo add log
```

```rust playground
use tracing::info;
use tracing_subscriber::fmt::format::FmtSpan;

#[tracing::instrument]
fn unit_of_work(n: u64) {
    info!("doing work");
    std::thread::sleep(std::time::Duration::from_millis(3));
}

fn main() {
    // With the `tracing-log` feature on, `init()` also installs the
    // log -> tracing bridge, so plain `log` records flow into this pipeline.
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        // Emit a synthetic event when each span closes, including its busy time.
        .with_span_events(FmtSpan::CLOSE)
        .init();

    log::info!("hello from the log crate"); // a record via the plain `log` facade
    unit_of_work(7);
}
```

```text
2026-06-02T06:32:22.591891Z  INFO probe: hello from the log crate
2026-06-02T06:32:22.591946Z  INFO unit_of_work{n=7}: probe: doing work
2026-06-02T06:32:22.595853Z  INFO unit_of_work{n=7}: probe: close time.busy=3.87ms time.idle=34.2µs
```

Two things to note: the `log::info!` record appears alongside native events, and `FmtSpan::CLOSE` adds a `close` line reporting how long the span was *busy* (actively on a thread) versus *idle* (suspended at an `.await`): free, accurate timing for every instrumented function.

> **Warning:** Do not *also* call `tracing_log::LogTracer::init()` yourself when the subscriber's `tracing-log` feature is enabled: both try to install the global `log` logger, and the second one panics with `SetLoggerError`. Let `init()` handle the bridge.

---

## Further Reading

- [`tracing` crate documentation](https://docs.rs/tracing) — the `span!`/`event!` macros, `#[instrument]`, fields, and the `Instrument`/`WithSubscriber` traits.
- [`tracing-subscriber` crate documentation](https://docs.rs/tracing-subscriber) — `fmt`, `EnvFilter`, the `Layer` trait, `Registry`, and the JSON formatter.
- [The `tracing` documentation portal](https://tracing.rs) — conceptual guides on spans, subscribers, and instrumenting async code.
- [`tokio` tracing tutorial](https://tokio.rs/tokio/topics/tracing) — instrumenting an async service end to end.
- Within this guide:
  - [Logging with the `log` Facade and `env_logger`](/23-ecosystem/03-logging/) — the simpler `log` + `env_logger` stack; start there if you do not yet need spans or JSON.
  - [Async Runtimes](/23-ecosystem/02-async-runtimes/) — Tokio and the async model that makes span-aware context essential.
  - [Web Frameworks](/23-ecosystem/01-web-frameworks/) — where request-scoped spans (and `tower-http`'s trace layer) shine.
  - [Popular Crates and the npm Packages They Replace](/23-ecosystem/00-popular-crates/) — where `tracing` sits among the most-used crates and their npm equivalents.
  - [Documentation with `rustdoc`](/23-ecosystem/05-documentation/) — documenting the instrumented APIs you build.
  - [Section 02: Comments and Output](/02-basics/04-output/) — `println!`/formatting that the event macros build on.
  - [Section 00: Introduction](/00-introduction/) and [Section 01: Getting Started](/01-getting-started/) — Cargo and project setup if `cargo add` is new to you.
  - [Section 24: Tooling](/24-tooling/) — complementary developer tooling.

---

## Exercises

### Exercise 1: Predict the span context

**Difficulty:** Beginner

**Objective:** Cement the events-inside-spans mental model and the no-subscriber-means-silence rule.

**Instructions:** Consider the program below. Without running it, answer: (a) what is printed if you remove the `tracing_subscriber::fmt().init()` line entirely? (b) With the subscriber present, which of the two `info!` lines will carry the `outer{user="ada"}` span context, and which will not? Then run it to confirm.

```rust playground
use tracing::{info, info_span};

fn main() {
    tracing_subscriber::fmt().init();

    info!("before span");
    let span = info_span!("outer", user = "ada");
    {
        let _guard = span.enter();
        info!("inside span");
    }
    info!("after span");
}
```

<details>
<summary>Solution</summary>

(a) With no subscriber installed, every event is a silent no-op: the program prints nothing at all (it does not even error).

(b) Only `"inside span"` is emitted while the span guard is held, so only it carries the `outer{user="ada"}` prefix. `"before span"` (guard not yet created) and `"after span"` (guard already dropped at the end of the inner block) are outside the span. Real output:

```text
2026-06-02T06:58:04.991510Z  INFO probe: before span
2026-06-02T06:58:04.991558Z  INFO outer{user="ada"}: probe: inside span
2026-06-02T06:58:04.991567Z  INFO probe: after span
```

The lesson: a span is a *period* delimited by when it is entered and exited; only events within that period inherit its fields. (This `span.enter()` guard pattern is correct here precisely because the code is synchronous — never hold such a guard across `.await`.)

</details>

### Exercise 2: Capture return values and errors automatically

**Difficulty:** Intermediate

**Objective:** Use `#[instrument]`'s `ret` and `err` arguments to record outcomes without writing explicit `info!`/`error!` calls, and learn the `Display`-vs-`Debug` requirement for `err`.

**Instructions:** Write a function `parse_amount(input: &str) -> Result<u64, ParseError>` that parses a `u64`, returning a custom `ParseError` enum (deriving `Debug` but **not** `Display`) on failure. Instrument it so the `Ok` value is logged on success and the error is logged at `ERROR` level on failure. First try `#[instrument(ret, err)]`, observe the compiler error, then fix it. Call it once with `"4999"` and once with `"oops"`.

<details>
<summary>Solution</summary>

`#[instrument(ret, err)]` fails to compile because `err` records the error via `Display` by default, and `ParseError` does not implement `Display`:

```text
error[E0277]: `ParseError` doesn't implement `std::fmt::Display`
  --> src/main.rs:8:1
   |
 8 | #[instrument(ret, err)]
   | ^^^^^^^^^^^^^^^^^^^^^^^ the trait `std::fmt::Display` is not implemented for `ParseError`
```

The fix is `err(Debug)`, which records the error via its `Debug` impl instead:

```rust playground
use tracing::instrument;

#[derive(Debug)]
enum ParseError {
    NotANumber,
}

// `ret` logs the Ok value at the span's level; `err(Debug)` logs the Err via
// Debug at ERROR level. Plain `err` would require a `Display` impl.
#[instrument(ret, err(Debug))]
fn parse_amount(input: &str) -> Result<u64, ParseError> {
    input.parse::<u64>().map_err(|_| ParseError::NotANumber)
}

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let _ = parse_amount("4999");
    let _ = parse_amount("oops");
}
```

Real output:

```text
2026-06-02T06:33:03.883288Z  INFO parse_amount{input="4999"}: probe: return=4999
2026-06-02T06:33:03.883316Z ERROR parse_amount{input="oops"}: probe: error=NotANumber
```

`ret` produced the `return=4999` event; `err(Debug)` produced the `ERROR`-level `error=NotANumber` event, both automatically, with no manual logging inside the function body.

</details>

### Exercise 3: Propagate request context into a spawned task and emit JSON

**Difficulty:** Advanced

**Objective:** Combine `#[instrument]`, async span propagation across `tokio::spawn`, and a JSON subscriber: the realistic shape of production request logging.

**Instructions:** Write an async `handle(request_id: u64, path: &str)` instrumented so its span records `request_id`. Inside it, log a "request received" event, then `tokio::spawn` a background subtask that logs "processing in background task" and must still carry the `request_id` context, then log "request complete". Configure a JSON subscriber (layered `registry()` with an `EnvFilter` defaulting to `info`). Call `handle(42, "/orders").await`. Verify the background task's JSON line includes `request_id`.

<details>
<summary>Solution</summary>

```bash
cargo add tracing
cargo add tracing-subscriber --features env-filter,json
cargo add tokio --features rt-multi-thread,macros
```

```rust
use tracing::{info, instrument, Instrument};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

#[instrument(fields(request_id = %request_id))]
async fn handle(request_id: u64, path: &str) {
    info!(path, "request received");

    // Offload a subtask to another tokio task, carrying the span with it
    // via `.in_current_span()` so it keeps the request_id context.
    let child = tokio::spawn(
        async {
            info!("processing in background task");
        }
        .in_current_span(),
    );
    child.await.unwrap();

    info!("request complete");
}

#[tokio::main]
async fn main() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().json().with_current_span(true).with_span_list(false))
        .init();

    handle(42, "/orders").await;
}
```

Real output (three JSON lines, all tagged with `request_id`):

```text
{"timestamp":"2026-06-02T06:33:31.948971Z","level":"INFO","fields":{"message":"request received","path":"/orders"},"target":"probe","span":{"path":"/orders","request_id":"42","name":"handle"}}
{"timestamp":"2026-06-02T06:33:31.949095Z","level":"INFO","fields":{"message":"processing in background task"},"target":"probe","span":{"path":"/orders","request_id":"42","name":"handle"}}
{"timestamp":"2026-06-02T06:33:31.949160Z","level":"INFO","fields":{"message":"request complete"},"target":"probe","span":{"path":"/orders","request_id":"42","name":"handle"}}
```

The middle line — emitted from a *different* tokio task — still carries `"request_id":"42"` because `.in_current_span()` attached the span to the spawned future. Remove `.in_current_span()` and that line loses its context entirely, the exact bug from the Common Pitfalls section. This combination (`#[instrument]` for the request span, `.in_current_span()`/`.instrument()` for spawned work, and a JSON layer) is the backbone of production-grade request logging in Rust.

</details>
