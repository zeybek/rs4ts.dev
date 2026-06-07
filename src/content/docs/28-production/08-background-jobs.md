---
title: "Background Job Processing"
description: "Run work outside the request cycle in Rust with tokio::spawn, mpsc channels, and a runner task: bounded, backpressured, drainable jobs without BullMQ or Redis."
---

Most production services need to do work *outside* the request/response cycle: send a welcome email, resize an uploaded image, recompute a recommendation, retry a failed webhook. In Node you reach for a worker queue like BullMQ or a detached `setImmediate`/`Promise` that you forget to await. In Rust the building blocks are `tokio::spawn`, channels (`tokio::sync::mpsc`), and a dedicated runner task, all giving you bounded, observable, gracefully-shutdown-able background work without a second process.

---

## Quick Overview

A **background job** is a unit of work you want to run asynchronously, decoupled from the HTTP request that triggered it, so the client gets a fast response while the work happens later. This page covers three layers, from simplest to most reliable: spawning a fire-and-forget Tokio task, building an in-process queue with a bounded channel feeding a dedicated **runner** task, and adding the production concerns a TypeScript developer expects: backpressure, bounded concurrency, retries, and clean draining on shutdown. The single most important mental shift is that an in-process queue lives and dies with your process; for jobs that must survive a crash or restart you still need a durable backend (Postgres, Redis, or a real queue), and we show where that line falls.

---

## TypeScript/JavaScript Example

A common Express pattern: accept a request, kick off background work, respond immediately. The naive version uses a floating promise; the better version uses BullMQ (Redis-backed) with a dedicated worker.

```typescript
// --- Naive: fire-and-forget floating promise (works, but fragile) ---
import express from "express";

const app = express();
app.use(express.json());

async function sendWelcomeEmail(to: string): Promise<void> {
  await new Promise((r) => setTimeout(r, 200)); // pretend SMTP call
  console.log(`sent welcome email to ${to}`);
}

app.post("/signup", (req, res) => {
  const { email } = req.body as { email: string };
  // The promise is fire-and-forget: we catch errors here, but the work is
  // still tied to the web process's lifetime and is invisible to any
  // queue/observability — and forgetting the .catch would surface as an
  // unhandled rejection.
  void sendWelcomeEmail(email).catch((err) =>
    console.error("email failed", err),
  );
  res.status(202).json({ status: "accepted" });
});
```

```typescript
// --- Robust: BullMQ, a Redis-backed queue with a separate worker ---
// npm install bullmq ioredis
import { Queue, Worker, type Job } from "bullmq";

const connection = { host: "127.0.0.1", port: 6379 };

interface EmailJob {
  to: string;
  subject: string;
}

// Producers (your web process) enqueue jobs.
const emailQueue = new Queue<EmailJob>("emails", { connection });

export async function enqueueWelcome(to: string): Promise<void> {
  await emailQueue.add(
    "welcome",
    { to, subject: "Welcome!" },
    { attempts: 3, backoff: { type: "exponential", delay: 1000 } },
  );
}

// A consumer (often a *separate* process) drains the queue with limited
// concurrency. BullMQ handles retries, backoff, and persistence in Redis.
const worker = new Worker<EmailJob>(
  "emails",
  async (job: Job<EmailJob>) => {
    await new Promise((r) => setTimeout(r, 200));
    console.log(`delivered to ${job.data.to}: ${job.data.subject}`);
  },
  { connection, concurrency: 8 },
);

worker.on("failed", (job, err) => console.error(`job ${job?.id} failed`, err));
```

**Key points a TypeScript developer relies on:**

- A floating promise is the quick path but loses errors and ties background work to the web process's lifetime.
- BullMQ gives durability (Redis), retries with backoff, and bounded `concurrency`.
- Producers and consumers can be separate processes; the queue is the boundary.

---

## Rust Equivalent

For **in-process** jobs that don't need to survive a restart, Rust needs no extra crate beyond Tokio: a bounded `mpsc` channel is your queue and a spawned task is your worker. The current stable toolchain is Rust 1.96.0 on the 2024 edition; `cargo new` selects it automatically.

```bash
cargo new email-service
cd email-service
cargo add tokio --features full
cargo add tokio-util          # CancellationToken for graceful shutdown
cargo add anyhow              # ergonomic job-error handling
```

```rust
use std::time::Duration;

use tokio::sync::mpsc;

// Jobs are a plain enum — the closest Rust analog to BullMQ's named job types.
#[derive(Debug)]
enum Job {
    SendEmail { to: String, subject: String },
    ResizeImage { id: u64 },
}

async fn handle(job: Job) {
    match job {
        Job::SendEmail { to, subject } => {
            tokio::time::sleep(Duration::from_millis(20)).await; // pretend SMTP
            println!("sent email to {to}: {subject}");
        }
        Job::ResizeImage { id } => {
            tokio::time::sleep(Duration::from_millis(20)).await;
            println!("resized image {id}");
        }
    }
}

#[tokio::main]
async fn main() {
    // A *bounded* queue: capacity 100. Senders await when it is full, which
    // is automatic backpressure — the equivalent of BullMQ's rate controls.
    let (tx, mut rx) = mpsc::channel::<Job>(100);

    // The dedicated worker task owns the receiver and processes one job
    // at a time, decoupled from whoever enqueues.
    let worker = tokio::spawn(async move {
        while let Some(job) = rx.recv().await {
            handle(job).await;
        }
        println!("worker: channel closed, draining done");
    });

    // A "request handler" enqueues a job and returns immediately.
    tx.send(Job::SendEmail {
        to: "ada@example.com".into(),
        subject: "Welcome".into(),
    })
    .await
    .unwrap();
    tx.send(Job::ResizeImage { id: 42 }).await.unwrap();

    // Dropping the last sender lets the worker's loop end after draining.
    drop(tx);
    worker.await.unwrap();
    println!("all jobs processed");
}
```

Running it prints (real output):

```text
sent email to ada@example.com: Welcome
resized image 42
worker: channel closed, draining done
all jobs processed
```

The `Sender` is cheaply `Clone`-able, so every Axum handler can hold a copy and enqueue without contention. The single `Receiver` lives in the worker; that asymmetry (many producers, one consumer) is exactly the queue shape you want.

---

## Detailed Explanation

### The channel *is* the queue

`tokio::sync::mpsc::channel::<Job>(100)` creates a **m**ulti-**p**roducer, **s**ingle-**c**onsumer queue with a buffer of 100 items. This maps onto BullMQ's queue cleanly: `tx.send(job).await` is `queue.add(...)`, and `rx.recv().await` is what the worker loop does internally. The key difference from a JavaScript array is that the capacity is *bounded*; see backpressure below.

> **Note:** `mpsc` is "multi-producer, single-consumer." If you want many workers, you either share one receiver behind a `Mutex` (a worker pool, shown later) or use a multi-consumer crate like `async-channel`. Cloning a `Receiver` is intentionally not allowed.

### The worker loop

```rust
while let Some(job) = rx.recv().await {
    handle(job).await;
}
```

`rx.recv()` returns `Some(job)` while senders exist and items arrive, and `None` once *all* senders are dropped and the buffer is empty. That `None` is the natural "shutdown" signal: the `while let` ends, the loop drains everything queued before exit, and the task returns. There is no busy-waiting: `recv().await` parks the task until a job arrives. (See [Section 11: Channels](/11-async/08-channels/) for the channel mechanics in depth.)

### `tokio::spawn` returns a handle you should usually keep

`tokio::spawn(future)` schedules the future on the runtime and returns a `JoinHandle<T>`. Unlike a JavaScript `Promise`, a Tokio task is **eager** in the sense that it starts running as soon as it is spawned (the future itself is lazy, but spawning polls it on the runtime). Awaiting the handle gives you `Result<T, JoinError>`, where the `Err` arm tells you the task **panicked or was cancelled**. In our `main` we `worker.await.unwrap()` precisely so a worker panic surfaces instead of vanishing.

### Backpressure for free

Because the channel is bounded, `tx.send(job).await` *suspends* the caller when the buffer is full and resumes only when the worker drains a slot. This is the opposite of a JavaScript array `push`, which grows without limit until you run out of memory. If you do not want to wait, `tx.try_send(job)` returns `Err(TrySendError::Full(job))` immediately so an HTTP handler can reply `503` rather than hang, exactly what a "queue full" policy needs.

### Where the analogy breaks down

Unlike BullMQ, this queue is **in memory and in process**. If the process crashes or restarts, every queued and in-flight job is lost. For jobs that must not be lost, the channel becomes a buffer in front of a durable store (insert a row into a Postgres `jobs` table, push to a Redis list, publish to NATS/Kafka), and the runner reads from *that*. The Tokio primitives here are still how you consume the durable queue: you just add persistence behind them.

---

## Key Differences

| Concern | Node (BullMQ / floating promise) | Rust (Tokio) |
| --- | --- | --- |
| Queue primitive | Redis list (BullMQ) or none (floating promise) | `mpsc::channel` (in-process) or a durable store |
| Concurrency control | `concurrency` option | bounded channel + `Semaphore` / worker pool |
| Backpressure | rate limiter / Redis growth | `send().await` suspends; `try_send` rejects |
| Error handling | `worker.on("failed")` | `JoinHandle` returns `Result`; `JoinSet` collects |
| Lost on crash? | No (Redis persists) | Yes, unless backed by a durable store |
| Separate worker process | Common and idiomatic | Optional; one process is fine to start |
| Retries / backoff | Built in | You write a small helper (shown in Exercises) |
| CPU-bound work | Blocks the event loop (use Worker Threads) | Use `spawn_blocking` / a thread pool |

### Eager Promises vs lazy futures

A JavaScript `Promise` begins executing the moment it is created, whether or not you `await` it; that is why a floating promise still does its work. A Rust `async` block does **nothing** until it is polled, which only happens once you `.await` it or hand it to `tokio::spawn`. So "fire and forget" in Rust *requires* `tokio::spawn`; simply calling an `async fn` and ignoring the returned future runs zero code. This is the reverse of the JavaScript footgun.

### One process or two?

In Node, a dedicated worker process is the norm because a CPU-bound job blocks the single-threaded event loop. Tokio's multi-threaded runtime can interleave thousands of async jobs across cores in the *same* process, so you can start with an in-process runner and split it into its own binary later, only when you need independent scaling or isolation. The job enum and `handle` function move unchanged.

---

## Common Pitfalls

### Pitfall 1: Spawning and never awaiting the handle (a swallowed panic)

A detached task that panics does **not** crash your process and does not propagate to anyone: the panic just prints to stderr.

```rust
#[tokio::main]
async fn main() {
    // A detached task: nobody holds or awaits the handle.
    tokio::spawn(async {
        panic!("this panic vanishes");
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    println!("main still alive — the detached panic did not crash us");
}
```

Real output:

```text
thread 'tokio-rt-worker' panicked at src/main.rs:5:9:
this panic vanishes
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
main still alive — the detached panic did not crash us
```

The work was lost and only a stderr line marks it: the moral equivalent of an unhandled promise rejection. **Fix:** keep the `JoinHandle` and `.await` it, or use `JoinSet` (below) so failures are observable, and wrap fallible job bodies in code that logs/retries.

### Pitfall 2: Capturing non-`Send` data in a spawned task

The multi-threaded runtime may move a task between threads, so its future must be `Send`. Capturing an `Rc` (which is not `Send`) fails to compile:

```rust
use std::rc::Rc;

#[tokio::main]
async fn main() {
    let data = Rc::new(vec![1, 2, 3]);
    // does not compile (future is not `Send` because `Rc` is not `Send`)
    tokio::spawn(async move {
        println!("{}", data.len());
    });
}
```

Real compiler error (trimmed):

```text
error: future cannot be sent between threads safely
   --> src/main.rs:7:5
    |
  7 | /     tokio::spawn(async move {
  8 | |         println!("{}", data.len());
  9 | |     });
    | |______^ future created by async block is not `Send`
    |
    = help: within `{async block@src/main.rs:7:18: 7:28}`, the trait `Send`
            is not implemented for `Rc<Vec<i32>>`
note: captured value is not `Send`
note: required by a bound in `tokio::spawn`
```

**Fix:** use `Arc` instead of `Rc` for data shared across tasks. See [Section 11: Concurrency](/11-async/10-concurrency/) and [Section 05: Reference Counting](/05-ownership/07-reference-counting/).

### Pitfall 3: Running blocking or CPU-bound work directly in a task

A synchronous, long-running call (`std::thread::sleep`, a heavy hash, an image filter, a blocking DB driver) inside an async task occupies a runtime worker thread and starves other tasks. Offload it:

```rust
use std::time::Duration;

fn hash_password(password: &str) -> String {
    std::thread::sleep(Duration::from_millis(20)); // pretend argon2
    format!("hashed:{password}")
}

async fn handle_signup(password: String) -> String {
    // Runs on Tokio's dedicated blocking pool; the async workers stay free.
    tokio::task::spawn_blocking(move || hash_password(&password))
        .await
        .expect("hashing task panicked")
}

#[tokio::main]
async fn main() {
    println!("{}", handle_signup("hunter2".into()).await);
}
```

Output: `hashed:hunter2`. This mirrors moving CPU work off Node's event loop into Worker Threads. See [Section 11: async vs sync](/11-async/13-async-vs-sync/).

### Pitfall 4: An unbounded queue (no backpressure)

`mpsc::unbounded_channel()` never makes the sender wait, so a fast producer can outrun a slow worker and exhaust memory: the same failure mode as a JavaScript array you keep `push`ing to. Prefer a bounded channel and decide explicitly what to do when it is full:

```rust
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    let (tx, _rx) = mpsc::channel::<u32>(2);
    tx.try_send(1).unwrap();
    tx.try_send(2).unwrap(); // buffer now full
    match tx.try_send(3) {
        Ok(()) => println!("queued"),
        Err(mpsc::error::TrySendError::Full(job)) => {
            println!("queue full, rejected job {job}");
        }
        Err(mpsc::error::TrySendError::Closed(job)) => {
            println!("queue closed, dropped job {job}");
        }
    }
}
```

Output: `queue full, rejected job 3`. A web handler turns that into a `503 Service Unavailable`: a clear, honest signal instead of unbounded memory growth.

### Pitfall 5: Forgetting to drain on shutdown

If you `process::exit` (or let `main` return) the instant a shutdown signal arrives, in-flight and queued jobs die. A naive worker that only loops on `rx.recv()` also never notices a shutdown request distinct from "channel closed." The fix is to `select!` over both the channel and a cancellation signal and drain before exiting — shown in the next section and integrated with [graceful shutdown](/28-production/02-graceful-shutdown/).

---

## Best Practices

- **Always keep job handles observable.** Use `JoinSet` for a dynamic set of jobs, or store `JoinHandle`s and await them, so panics and failures are logged or retried — never swallowed (Pitfall 1).
- **Bound everything.** A bounded channel for the queue and a `Semaphore` (or a fixed worker pool) for concurrency. Decide the full-queue policy explicitly (`await` for backpressure, or `try_send` + `503`).
- **Make jobs idempotent.** Any reliable runner may run a job more than once (a retry after a partial failure, or a redelivery from a durable backend). Design `handle` so a duplicate run is harmless.
- **Separate "enqueue" from "run."** Handlers should only validate and enqueue; the runner owns execution. This keeps request latency low and centralizes retry/timeout/observability.
- **Add per-job timeouts and retries with backoff.** Wrap each job body in `tokio::time::timeout` and a small retry helper (see Exercises) rather than letting one stuck job block forever.
- **Drain on shutdown.** Wire the runner to a `CancellationToken` (or the same shutdown future your server uses) and finish in-flight work before exit — see [graceful shutdown](/28-production/02-graceful-shutdown/).
- **Use the durable backend only when loss is unacceptable.** In-process is simpler and faster; reach for Postgres/Redis/Kafka when a job *must* survive a crash. Many services use both: in-process for best-effort work, durable for must-not-lose work.
- **Instrument the queue.** Export queue depth, in-flight count, and per-job duration/outcome so you can see saturation; pair with [metrics](/28-production/04-metrics/) and [distributed tracing](/28-production/05-distributed-tracing/).

---

## Real-World Example

A production-flavored Axum service: HTTP handlers validate and enqueue email jobs, returning `202 Accepted` immediately; a dedicated **job runner** processes them with bounded concurrency via a `Semaphore`, observable failures via `JoinSet`, and a clean drain on shutdown via a `CancellationToken`.

```bash
cargo add tokio --features full
cargo add tokio-util axum anyhow
cargo add serde --features derive
cargo add serde_json reqwest --features json   # reqwest only for the demo client
```

```rust
use std::sync::Arc;
use std::time::Duration;

use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use serde::Deserialize;
use tokio::sync::{mpsc, Semaphore};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Deserialize)]
struct EmailRequest {
    to: String,
    subject: String,
}

#[derive(Debug)]
enum Job {
    SendEmail { to: String, subject: String },
}

// Every handler holds a cloned Sender; the queue is the boundary.
#[derive(Clone)]
struct AppState {
    tx: mpsc::Sender<Job>,
}

// HTTP handler: validate, enqueue, return 202 Accepted immediately.
async fn enqueue_email(
    State(state): State<AppState>,
    Json(req): Json<EmailRequest>,
) -> Result<(StatusCode, &'static str), (StatusCode, &'static str)> {
    let job = Job::SendEmail { to: req.to, subject: req.subject };
    // try_send => never block the request; reject with 503 if the queue is full.
    state
        .tx
        .try_send(job)
        .map_err(|_| (StatusCode::SERVICE_UNAVAILABLE, "queue full"))?;
    Ok((StatusCode::ACCEPTED, "accepted"))
}

async fn deliver_email(to: &str, subject: &str) -> anyhow::Result<()> {
    tokio::time::sleep(Duration::from_millis(10)).await; // pretend SMTP
    println!("delivered to {to}: {subject}");
    Ok(())
}

// The dedicated job runner: bounded concurrency + observable failures + drain.
async fn run_jobs(mut rx: mpsc::Receiver<Job>, shutdown: CancellationToken) {
    let limiter = Arc::new(Semaphore::new(8)); // at most 8 jobs in flight
    let mut in_flight = tokio::task::JoinSet::new();

    loop {
        tokio::select! {
            maybe = rx.recv() => {
                let Some(job) = maybe else { break }; // channel closed
                // Acquire a permit *before* spawning, so we never exceed 8.
                let permit = Arc::clone(&limiter).acquire_owned().await.unwrap();
                in_flight.spawn(async move {
                    let _permit = permit; // released when the job finishes
                    match job {
                        Job::SendEmail { to, subject } => {
                            if let Err(e) = deliver_email(&to, &subject).await {
                                eprintln!("email job failed: {e:#}");
                            }
                        }
                    }
                });
            }
            _ = shutdown.cancelled() => break,
        }
    }

    // Drain: wait for outstanding jobs to finish before returning.
    while in_flight.join_next().await.is_some() {}
    println!("job runner drained");
}

#[tokio::main]
async fn main() {
    let (tx, rx) = mpsc::channel::<Job>(1024);
    let shutdown = CancellationToken::new();
    let runner = tokio::spawn(run_jobs(rx, shutdown.clone()));

    let app = Router::new()
        .route("/emails", post(enqueue_email))
        .with_state(AppState { tx });

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // --- Demo: two real requests, then a graceful shutdown ---
    let client = reqwest::Client::new();
    for (to, subject) in [("a@x.com", "Hi"), ("b@x.com", "Yo")] {
        let resp = client
            .post(format!("http://{addr}/emails"))
            .json(&serde_json::json!({ "to": to, "subject": subject }))
            .send()
            .await
            .unwrap();
        println!("HTTP {} for {to}", resp.status().as_u16());
    }

    tokio::time::sleep(Duration::from_millis(50)).await;
    shutdown.cancel();        // simulate SIGTERM
    runner.await.unwrap();    // wait for the runner to drain
    server.abort();
    println!("done");
}
```

Real output (ordering between the runner and the HTTP responses varies; this is one run):

```text
delivered to a@x.com: Hi
HTTP 202 for a@x.com
delivered to b@x.com: Yo
HTTP 202 for b@x.com
job runner drained
done
```

In a real deployment the manual `shutdown.cancel()` is driven by a SIGTERM handler and the HTTP server uses `axum::serve(...).with_graceful_shutdown(...)`, so the listener stops accepting, in-flight requests finish, the queue stops, and the runner drains — all wired together in [graceful shutdown](/28-production/02-graceful-shutdown/). The `Semaphore` caps fan-out exactly like BullMQ's `concurrency: 8`, and `JoinSet` makes every job's panic or error visible instead of swallowed.

---

## Further Reading

### Official Documentation

- [`tokio::spawn` and `JoinHandle`](https://docs.rs/tokio/latest/tokio/task/fn.spawn.html): spawning tasks and observing their results.
- [`tokio::task::JoinSet`](https://docs.rs/tokio/latest/tokio/task/struct.JoinSet.html): manage a dynamic set of jobs and collect outcomes.
- [`tokio::sync::mpsc`](https://docs.rs/tokio/latest/tokio/sync/mpsc/index.html): bounded and unbounded channels (your in-process queue).
- [`tokio::sync::Semaphore`](https://docs.rs/tokio/latest/tokio/sync/struct.Semaphore.html): bound concurrency to N in-flight jobs.
- [`tokio::task::spawn_blocking`](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html): run blocking/CPU-bound jobs off the async workers.
- [`tokio_util::sync::CancellationToken`](https://docs.rs/tokio-util/latest/tokio_util/sync/struct.CancellationToken.html): cooperative shutdown signaling.
- [Tokio topic: graceful shutdown](https://tokio.rs/tokio/topics/shutdown): the canonical drain-on-shutdown pattern.

### Related Guide Sections

- [Graceful shutdown](/28-production/02-graceful-shutdown/) — drive the runner's `CancellationToken` from a real SIGTERM handler.
- [Metrics and monitoring](/28-production/04-metrics/) — export queue depth and per-job duration/outcome.
- [Distributed tracing](/28-production/05-distributed-tracing/) — propagate request context into a job and follow it across services.
- [Configuration](/28-production/00-configuration/) and [Environment-based config](/28-production/01-environment/) — make queue capacity and worker count configurable per environment.
- [Section 11: Spawning tasks](/11-async/09-spawning-tasks/), [Channels](/11-async/08-channels/), [select/join](/11-async/07-select-join/), [Concurrency](/11-async/10-concurrency/) — the async primitives this page builds on.
- [Section 08: anyhow and thiserror](/08-error-handling/06-anyhow-thiserror/) — modeling and propagating job errors.
- [Section 05: Reference Counting](/05-ownership/07-reference-counting/) — `Arc` vs `Rc` for data shared across tasks.
- [Section 02: Variables](/02-basics/00-variables/) and [Section 01: Why Rust](/01-getting-started/00-why-rust/) — foundations for newcomers.
- [Migration guide](/29-migration-guide/) — replacing a Node BullMQ worker with a Rust runner.

---

## Exercises

### Exercise 1: A retry helper with exponential backoff

**Difficulty:** Beginner–Intermediate

**Objective:** Wrap a fallible async job so transient failures are retried with growing delays, like BullMQ's `backoff: { type: "exponential" }`.

**Instructions:** Write `async fn retry<F, Fut, T, E>(max_attempts: u32, op: F) -> Result<T, E>` that calls `op` (a closure returning a future), and on `Err` waits `50ms * 2^(attempt-1)` and tries again, up to `max_attempts`. Demonstrate it with an operation that fails twice and then succeeds.

<details>
<summary>Solution</summary>

```rust
use std::time::Duration;

// Retry an async operation with exponential backoff, up to `max_attempts`.
async fn retry<F, Fut, T, E>(max_attempts: u32, mut op: F) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut attempt = 0;
    loop {
        attempt += 1;
        match op().await {
            Ok(value) => return Ok(value),
            Err(e) if attempt < max_attempts => {
                let delay = Duration::from_millis(50 * 2u64.pow(attempt - 1));
                eprintln!("attempt {attempt} failed ({e}); retrying in {delay:?}");
                tokio::time::sleep(delay).await;
            }
            Err(e) => return Err(e), // out of attempts
        }
    }
}

#[tokio::main]
async fn main() {
    let mut calls = 0;
    let result: Result<&str, String> = retry(4, || {
        calls += 1;
        let n = calls;
        async move {
            if n < 3 {
                Err(format!("transient error on call {n}"))
            } else {
                Ok("delivered")
            }
        }
    })
    .await;
    println!("result: {result:?}");
}
```

Real output:

```text
attempt 1 failed (transient error on call 1); retrying in 50ms
attempt 2 failed (transient error on call 2); retrying in 100ms
result: Ok("delivered")
```

</details>

### Exercise 2: A shutdown-aware worker that drains its queue

**Difficulty:** Intermediate

**Objective:** Build a worker that processes jobs from an `mpsc` channel but, when a `CancellationToken` fires, drains everything already queued and then exits cleanly — never dropping accepted work.

**Instructions:** Write `async fn run_worker(rx: mpsc::Receiver<Job>, shutdown: CancellationToken)` that uses `tokio::select!` over `rx.recv()` and `shutdown.cancelled()`. On cancellation, drain remaining jobs with `rx.try_recv()` before breaking. In `main`, enqueue three jobs, cancel after a short delay, and confirm all three run.

<details>
<summary>Solution</summary>

```rust
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
struct Job {
    id: u64,
}

async fn process(job: Job) {
    tokio::time::sleep(Duration::from_millis(15)).await;
    println!("processed job {}", job.id);
}

async fn run_worker(mut rx: mpsc::Receiver<Job>, shutdown: CancellationToken) {
    loop {
        tokio::select! {
            maybe_job = rx.recv() => match maybe_job {
                Some(job) => process(job).await,
                None => break, // all senders dropped
            },
            _ = shutdown.cancelled() => {
                println!("worker: shutdown signal received, draining remaining jobs");
                // Drain whatever is already queued, then exit.
                while let Ok(job) = rx.try_recv() {
                    process(job).await;
                }
                break;
            }
        }
    }
    println!("worker: exited cleanly");
}

#[tokio::main]
async fn main() {
    let (tx, rx) = mpsc::channel::<Job>(64);
    let shutdown = CancellationToken::new();
    let worker = tokio::spawn(run_worker(rx, shutdown.clone()));

    for id in 0..3 {
        tx.send(Job { id }).await.unwrap();
    }

    // Simulate SIGTERM arriving after work was queued.
    tokio::time::sleep(Duration::from_millis(5)).await;
    shutdown.cancel();

    worker.await.unwrap();
    drop(tx);
    println!("shutdown complete");
}
```

Real output (one run; the exact point at which the shutdown line appears depends on timing, but all three jobs always complete and the worker exits cleanly):

```text
processed job 0
worker: shutdown signal received, draining remaining jobs
processed job 1
processed job 2
worker: exited cleanly
shutdown complete
```

</details>

### Exercise 3: A periodic (cron-like) cleanup runner

**Difficulty:** Intermediate–Advanced

**Objective:** Run a recurring background job on a fixed interval — the equivalent of `setInterval` — without overlapping runs piling up if one pass runs long.

**Instructions:** Use `tokio::time::interval` with `MissedTickBehavior::Skip` to run a `cleanup` pass every 20ms. Stop after three passes. Explain why `MissedTickBehavior::Skip` matters here.

<details>
<summary>Solution</summary>

```rust
use std::time::Duration;

use tokio::time::{interval, MissedTickBehavior};

// A periodic runner: like setInterval, but a slow tick won't cause a burst
// of catch-up ticks afterward.
async fn run_periodic(mut counter: u32) {
    let mut ticker = interval(Duration::from_millis(20));
    // If a tick is delayed (e.g. a long cleanup pass), skip the missed
    // deadlines instead of firing several ticks back-to-back to "catch up".
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        ticker.tick().await;
        counter += 1;
        println!("cleanup pass {counter}");
        if counter >= 3 {
            break;
        }
    }
}

#[tokio::main]
async fn main() {
    run_periodic(0).await;
    println!("periodic runner stopped");
}
```

Real output:

```text
cleanup pass 1
cleanup pass 2
cleanup pass 3
periodic runner stopped
```

**Why `Skip`:** the default (`Burst`) behavior tries to "catch up" by firing every missed tick immediately, which can overload a slow job that already fell behind. `Skip` resyncs to the schedule and runs at most one tick per period — the behavior you almost always want for a cleanup or polling loop. In production this runner would also take a `CancellationToken` (Exercise 2) so it exits on shutdown.

</details>
