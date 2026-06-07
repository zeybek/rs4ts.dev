---
title: "Async Channels"
description: "Tokio's mpsc, oneshot, broadcast, and watch channels let async tasks talk without shared memory, replacing EventEmitter and queues with compile-time guarantees."
---

Channels are how independent async tasks talk to each other without sharing memory directly. If you have reached for `EventEmitter`, a `MessageChannel`, an RxJS `Subject`, or a hand-rolled queue in Node.js, Tokio's channel family covers the same ground, but with compile-time guarantees about who can send, who can receive, and what happens on shutdown.

---

## Quick Overview

A **channel** is a typed pipe: one or more producers push values in, one or more consumers pull them out. Tokio ships four flavors: **`mpsc`** (multi-producer, single-consumer queue), **`oneshot`** (a single value, once), **`broadcast`** (every subscriber sees every message), and **`watch`** (subscribers see only the latest value). They are the idiomatic way to coordinate tasks, because passing ownership of a value through a channel sidesteps the shared-mutable-state problems that `Arc<Mutex<T>>` solves at a higher cost.

The important contrast for a JavaScript developer: these are **async** channels whose `recv`/`send` operations `.await` and yield to the runtime instead of blocking a thread. The standard library's `std::sync::mpsc` looks similar but **blocks the OS thread**, which would stall a Tokio worker, so inside async code you almost always want the Tokio versions.

> **Note:** Every runnable Rust snippet on this page was compiled and executed with `rustc`/`cargo` 1.96.0 (current stable; 2024 edition). Examples use `tokio = { version = "1.52", features = ["full"] }`. Rust ships **no built-in async runtime** — see [Tokio Setup](/11-async/03-tokio-setup/).

---

## TypeScript/JavaScript Example

JavaScript has no single "channel" primitive, so the same job is done with a grab-bag of tools. A common pattern is an async queue built on Promises, or an `EventEmitter` for fan-out. Here is a producer/consumer queue and a pub/sub emitter, the two shapes you will most want to translate:

```typescript
import { EventEmitter } from "node:events";

// --- Producer/consumer: a hand-rolled async queue (mpsc-ish) ---
class AsyncQueue<T> {
  private items: T[] = [];
  private resolvers: Array<(value: T | null) => void> = [];
  private closed = false;

  send(item: T): void {
    if (this.closed) throw new Error("queue closed");
    const resolve = this.resolvers.shift();
    if (resolve) resolve(item);
    else this.items.push(item);
  }

  close(): void {
    this.closed = true;
    // Wake any pending consumers with `null` to signal end-of-stream.
    for (const resolve of this.resolvers) resolve(null);
    this.resolvers = [];
  }

  // Resolves to the next item, or null once closed and drained.
  recv(): Promise<T | null> {
    const item = this.items.shift();
    if (item !== undefined) return Promise.resolve(item);
    if (this.closed) return Promise.resolve(null);
    return new Promise((resolve) => this.resolvers.push(resolve));
  }
}

// --- Pub/sub fan-out: every listener sees every event (broadcast-ish) ---
const bus = new EventEmitter();
bus.on("user.created", (id: number) => console.log(`audit log: ${id}`));
bus.on("user.created", (id: number) => console.log(`send welcome email: ${id}`));
bus.emit("user.created", 42);
```

Notice what JavaScript does *not* give you: there is no static guarantee that exactly one consumer drains the queue, no automatic "the channel is closed because every producer went away," and `EventEmitter` is untyped (any payload, any event name). Rust's channels encode all of that in the type system.

---

## Rust Equivalent

The `mpsc` channel is the workhorse and maps directly onto the producer/consumer queue above:

```rust
use tokio::sync::mpsc;

#[derive(Debug)]
struct Job {
    id: u32,
    payload: String,
}

#[tokio::main]
async fn main() {
    // Bounded channel: at most 8 messages buffered before send() awaits.
    let (tx, mut rx) = mpsc::channel::<Job>(8);

    // Producer task.
    let producer = tokio::spawn(async move {
        for id in 1..=3 {
            let job = Job {
                id,
                payload: format!("data-{id}"),
            };
            // send() is async: it waits if the buffer is full (backpressure).
            if tx.send(job).await.is_err() {
                eprintln!("receiver dropped, stopping producer");
                break;
            }
        }
        // tx dropped here when the task ends -> channel closes.
    });

    // Consumer: recv() yields None once all senders are dropped and the buffer drains.
    while let Some(job) = rx.recv().await {
        println!("processing job {} with {}", job.id, job.payload);
    }
    println!("channel closed, all jobs done");

    producer.await.unwrap();
}
```

Real output:

```
processing job 1 with data-1
processing job 2 with data-2
processing job 3 with data-3
channel closed, all jobs done
```

Two structural facts fall out of the type system for free: `mpsc::Receiver` is **not** `Clone`, so the compiler guarantees a single consumer; and when the last `Sender` is dropped, `recv()` returns `None`, so the `while let` loop ends on its own. No `closed` flag, no sentinel `null`.

---

## Detailed Explanation

### `mpsc`: multi-producer, single-consumer

`mpsc::channel::<T>(capacity)` returns a `(Sender<T>, Receiver<T>)` pair. The name encodes the contract: the **Sender is `Clone`** (many producers), the **Receiver is not** (one consumer).

- `tx.send(value).await` moves `value` into the channel. On a **bounded** channel it `.await`s when the buffer is full. This is **backpressure**, a built-in flow-control mechanism that JavaScript queues lack by default. It returns `Result<(), SendError<T>>`; the `Err` arm hands your value back if the receiver is gone.
- `rx.recv().await` returns `Option<T>`: `Some(value)` for each message, then `None` once **all** senders are dropped and the buffer is empty.

To get multiple producers, clone the sender, but remember to drop the original so the channel can actually close:

```rust
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    let (tx, mut rx) = mpsc::channel::<String>(32);

    // Spawn 3 producers, each gets its own clone of the sender.
    for worker_id in 1..=3 {
        let tx = tx.clone();
        tokio::spawn(async move {
            tx.send(format!("worker {worker_id} reporting in"))
                .await
                .unwrap();
        });
    }

    // Drop the original sender so the channel can close once the clones finish.
    drop(tx);

    let mut received = Vec::new();
    while let Some(msg) = rx.recv().await {
        received.push(msg);
    }

    received.sort(); // task ordering is nondeterministic; sort for a stable print.
    for msg in received {
        println!("{msg}");
    }
}
```

Real output:

```
worker 1 reporting in
worker 2 reporting in
worker 3 reporting in
```

> **Tip:** There is also `mpsc::unbounded_channel()`. Its `send()` is **synchronous** (no `.await`, never waits) because the buffer is unlimited, but that means no backpressure, so a fast producer can grow the queue until you run out of memory. Prefer the bounded `channel(capacity)` unless you have a specific reason not to.

### `oneshot`: exactly one value, once

`oneshot::channel::<T>()` is for a single reply. The `Sender::send` method **takes `self` by value**, so it can only ever be called once. The type system enforces "exactly one message."

```rust
use tokio::sync::oneshot;

#[tokio::main]
async fn main() {
    let (tx, rx) = oneshot::channel::<u64>();

    // Worker computes a single value and sends it back.
    tokio::spawn(async move {
        let result = (1..=100).sum::<u64>();
        // send() takes self by value -> can only be called once. Not async.
        let _ = tx.send(result);
    });

    // Await the single reply. Errors only if the sender was dropped without sending.
    match rx.await {
        Ok(value) => println!("computed sum = {value}"),
        Err(_) => println!("worker dropped the sender without replying"),
    }
}
```

Real output:

```
computed sum = 5050
```

Note that the **`Receiver` is itself a future**: you `rx.await` it directly rather than calling `.recv()`. A `oneshot` is the Rust analogue of a single `Promise` whose `resolve` you hand to another task. Its most useful application is embedding the reply channel *inside* a request message, which gives you request/response over an `mpsc` (see the actor pattern below).

### `broadcast`: every subscriber sees every message

`broadcast::channel::<T>(capacity)` is fan-out. Clone-free subscription via `tx.subscribe()`, and **each** receiver gets a copy of **every** value sent after it subscribed. The value type must be `Clone`.

```rust
use tokio::sync::broadcast;

#[tokio::main]
async fn main() {
    // Capacity 16: each receiver has its own 16-slot ring buffer.
    let (tx, _rx) = broadcast::channel::<String>(16);

    // Two independent subscribers. Each gets EVERY message sent after it subscribed.
    let mut rx1 = tx.subscribe();
    let mut rx2 = tx.subscribe();

    let sub1 = tokio::spawn(async move {
        while let Ok(event) = rx1.recv().await {
            println!("sub1 saw: {event}");
        }
    });
    let sub2 = tokio::spawn(async move {
        while let Ok(event) = rx2.recv().await {
            println!("sub2 saw: {event}");
        }
    });

    // send() returns the number of currently-subscribed receivers.
    tx.send("user.created".to_string()).unwrap();
    tx.send("user.updated".to_string()).unwrap();

    // Dropping the sender closes the channel; receivers then get RecvError::Closed.
    drop(tx);

    sub1.await.unwrap();
    sub2.await.unwrap();
}
```

Real output:

```
sub1 saw: user.created
sub1 saw: user.updated
sub2 saw: user.created
sub2 saw: user.updated
```

This is the typed, ownership-aware replacement for `EventEmitter`. The capacity matters: `broadcast` uses a **ring buffer per receiver**, so a slow consumer that falls more than `capacity` messages behind will *lag* and skip the oldest messages (covered under Common Pitfalls).

### `watch`: only the latest value matters

`watch::channel(initial)` is for state that changes over time where consumers only care about the **current** value, not the history: config reloads, a "current health" flag, the latest sensor reading. The sender overwrites; receivers `borrow()` the newest value.

```rust
use tokio::sync::watch;
use tokio::time::{Duration, sleep};

#[derive(Clone, Debug, PartialEq)]
struct Config {
    log_level: String,
}

#[tokio::main]
async fn main() {
    // watch holds a single latest value; receivers see the most recent one.
    let (tx, mut rx) = watch::channel(Config {
        log_level: "info".to_string(),
    });

    let watcher = tokio::spawn(async move {
        loop {
            // changed() resolves when the value is updated since the last observation.
            if rx.changed().await.is_err() {
                println!("config sender dropped, watcher exiting");
                break;
            }
            // borrow() gives a read guard to the latest value.
            let cfg = rx.borrow();
            println!("config changed -> log_level = {}", cfg.log_level);
        }
    });

    sleep(Duration::from_millis(20)).await;
    tx.send(Config { log_level: "debug".to_string() }).unwrap();
    sleep(Duration::from_millis(20)).await;
    tx.send(Config { log_level: "trace".to_string() }).unwrap();
    sleep(Duration::from_millis(20)).await;

    drop(tx); // closes the channel; watcher's changed() returns Err and it exits.
    watcher.await.unwrap();
}
```

Real output:

```
config changed -> log_level = debug
config changed -> log_level = trace
config sender dropped, watcher exiting
```

If you `send` three times before a watcher calls `changed()`, it only ever sees the *last* value; intermediate values are coalesced. That is the defining difference from `broadcast`, which delivers every message.

> **Warning:** Do not hold the guard returned by `watch::Receiver::borrow()` across an `.await`. It is a read lock; keeping it alive while suspended can block senders. Copy out what you need, drop the guard, then await. The same hazard applies to `tokio::sync::RwLock` — see [Async Synchronization Primitives](/11-async/11-sync-primitives/).

### Request/response: `oneshot` inside an `mpsc` message (the actor pattern)

This is the single most useful composition. One task owns some state; everyone else talks to it by sending a request that carries its own private `oneshot` reply channel:

```rust
use tokio::sync::{mpsc, oneshot};

/// A request carries a oneshot sender for its individual reply.
struct Request {
    key: String,
    reply_to: oneshot::Sender<Option<String>>,
}

#[tokio::main]
async fn main() {
    let (tx, mut rx) = mpsc::channel::<Request>(32);

    // The "actor": owns the state, processes requests one at a time.
    let actor = tokio::spawn(async move {
        let store = std::collections::HashMap::from([
            ("alice".to_string(), "admin".to_string()),
            ("bob".to_string(), "user".to_string()),
        ]);
        while let Some(req) = rx.recv().await {
            let answer = store.get(&req.key).cloned();
            // Reply on this request's private oneshot channel.
            let _ = req.reply_to.send(answer);
        }
    });

    // A client sends a request and awaits its dedicated reply.
    let role = lookup(&tx, "alice").await;
    println!("alice -> {role:?}");
    let missing = lookup(&tx, "carol").await;
    println!("carol -> {missing:?}");

    drop(tx);
    actor.await.unwrap();
}

async fn lookup(tx: &mpsc::Sender<Request>, key: &str) -> Option<String> {
    let (reply_to, reply_rx) = oneshot::channel();
    tx.send(Request {
        key: key.to_string(),
        reply_to,
    })
    .await
    .expect("actor is alive");
    reply_rx.await.expect("actor replied")
}
```

Real output:

```
alice -> Some("admin")
carol -> None
```

Because the actor is the *only* task that touches `store`, there is no lock and no data race: the channel serializes access. This is how you replace `Arc<Mutex<HashMap<..>>>` with message passing.

---

## Key Differences

| Concern | JavaScript | Rust / Tokio |
| --- | --- | --- |
| Producer/consumer queue | hand-rolled, or a userland library | `tokio::sync::mpsc` (built in) |
| One-shot reply | a single `Promise` you resolve | `tokio::sync::oneshot` |
| Pub/sub fan-out | `EventEmitter`, RxJS `Subject` | `tokio::sync::broadcast` |
| "Latest value" state | a variable + manual notify | `tokio::sync::watch` |
| Type safety of payloads | none (`any`) | fully typed `T`, checked at compile time |
| Who can receive | not enforced | `mpsc::Receiver` is **not `Clone`** → single consumer guaranteed |
| Backpressure | manual | bounded `mpsc::send().await` waits when full |
| "Channel closed" signal | manual flag / sentinel | automatic when all senders or the receiver drop |
| Blocking vs async | event loop never blocks | Tokio channels `.await`; `std::sync::mpsc` **blocks the thread** |

### `std::sync::mpsc` vs `tokio::sync::mpsc`

The standard library has its own `mpsc` that *looks* like Tokio's, and reaching for it inside async code is a classic mistake.

```rust
use std::sync::mpsc; // standard library, NOT tokio
use std::thread;

fn main() {
    // std channel: multi-producer, single-consumer, BLOCKING (no async).
    let (tx, rx) = mpsc::channel::<i32>();

    for n in 0..3 {
        let tx = tx.clone();
        thread::spawn(move || {
            tx.send(n * 10).unwrap(); // blocks the OS thread if needed
        });
    }
    drop(tx);

    let mut total = 0;
    // recv() blocks the thread until a value arrives or all senders drop.
    for value in rx {
        total += value;
    }
    println!("total = {total}");
}
```

Real output:

```
total = 30
```

The decision rule:

| | `std::sync::mpsc` | `tokio::sync::mpsc` |
| --- | --- | --- |
| `recv` | **blocks the OS thread** | `.await`s, yields to the runtime |
| `send` | synchronous | `.await` on bounded; sync on unbounded |
| Use it for | plain threads ([05-ownership](/05-ownership/) territory) | async tasks under a runtime |
| Inside a Tokio task? | **no** — it stalls a worker thread | **yes** |

> **Note:** If you genuinely must call a blocking `std` channel from async (e.g. bridging a synchronous library), do it on a thread that is allowed to block via `tokio::task::spawn_blocking` — see [Spawning Tasks](/11-async/09-spawning-tasks/).

---

## Common Pitfalls

### 1. Forgetting to drop the extra `Sender`, so `recv()` never returns `None`

`recv()` returns `None` only when **every** sender is dropped. If you clone a sender for workers but keep the original alive in scope, the consumer loop hangs forever waiting for more. The fix is the explicit `drop(tx)` shown in the multi-producer example above (or letting the original go out of scope before the loop).

### 2. Trying to clone the `mpsc::Receiver`

`mpsc` is single-consumer by design. Cloning the receiver to fan work out to several tasks does not compile:

```rust
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    let (_tx, rx) = mpsc::channel::<i32>(8);
    // mpsc is multi-producer, SINGLE-consumer: the Receiver is not Clone.
    let rx2 = rx.clone(); // does not compile (error[E0599]: no method named `clone`)
    drop((rx, rx2));
}
```

Real compiler error:

```
error[E0599]: no method named `clone` found for struct `tokio::sync::mpsc::Receiver` in the current scope
 --> src/bin/err_clone_rx.rs:7:18
  |
7 |     let rx2 = rx.clone();
  |                  ^^^^^
  |
help: there is a method `close` with a similar name
```

To share one receiver across several worker tasks, wrap it in `Arc<tokio::sync::Mutex<Receiver<T>>>` (the worker pool below does exactly that), or restructure so each worker has its own channel.

### 3. Calling `oneshot::Sender::send` twice

`send` consumes the sender, so a second call references a moved value:

```rust
use tokio::sync::oneshot;

#[tokio::main]
async fn main() {
    let (tx, _rx) = oneshot::channel::<i32>();
    tx.send(1).unwrap(); // send consumes `tx` by value
    tx.send(2).unwrap(); // does not compile (error[E0382]: use of moved value)
}
```

Real compiler error (abridged):

```
error[E0382]: use of moved value: `tx`
   --> src/bin/err_send_twice.rs:7:5
    |
  5 |     let (tx, _rx) = oneshot::channel::<i32>();
    |          -- move occurs because `tx` has type `tokio::sync::oneshot::Sender<i32>`, which does not implement the `Copy` trait
  6 |     tx.send(1).unwrap(); // send consumes `tx` by value
    |        ------- `tx` moved due to this method call
  7 |     tx.send(2).unwrap(); // ERROR: tx already moved
    |     ^^ value used here after move
    |
note: `tokio::sync::oneshot::Sender::<T>::send` takes ownership of the receiver `self`, which moves `tx`
```

If you need to send more than once, you wanted `mpsc` (a stream of messages) or `watch` (the latest value), not `oneshot`.

### 4. A slow `broadcast` receiver lagging and skipping messages

`broadcast` keeps a fixed-size ring buffer per receiver. A consumer that falls behind by more than the capacity loses the oldest messages and `recv()` returns `Err(RecvError::Lagged(n))`. Unlike a closed channel, **lag is recoverable**: you keep receiving after it:

```rust
use tokio::sync::broadcast::{self, error::RecvError};

#[tokio::main]
async fn main() {
    // Tiny capacity of 2 to force lag.
    let (tx, mut rx) = broadcast::channel::<u32>(2);

    // Send 4 values before the slow receiver reads anything.
    for n in 1..=4 {
        tx.send(n).unwrap();
    }

    // The first two values were overwritten in the ring buffer -> Lagged(2).
    loop {
        match rx.recv().await {
            Ok(value) => println!("received {value}"),
            Err(RecvError::Lagged(skipped)) => {
                println!("lagged: skipped {skipped} messages");
            }
            Err(RecvError::Closed) => {
                println!("channel closed");
                break;
            }
        }
    }
}
```

Real output:

```
lagged: skipped 2 messages
received 3
received 4
```

Always handle `Lagged` explicitly: log it, increase the capacity, or speed up the consumer. Treating it as a fatal error is usually wrong.

### 5. Using `std::sync::mpsc::recv()` inside an async task

Because `std::sync::mpsc::recv()` blocks the calling OS thread, calling it directly in a Tokio task parks one of the runtime's worker threads. On a single-threaded runtime this **deadlocks**; on a multi-threaded runtime it silently degrades throughput. There is no compiler error — it just behaves badly. Use `tokio::sync::mpsc` in async code, or hop to `spawn_blocking` for genuinely blocking work.

---

## Best Practices

- **Pick the channel by communication shape, not habit.** Stream of work → `mpsc`; single reply → `oneshot`; fan-out events → `broadcast`; latest-value state → `watch`.
- **Prefer bounded `mpsc`.** Backpressure protects you from unbounded memory growth. Choose a capacity, and treat a full channel as a real signal about load.
- **Drop senders you do not need.** A lingering `Sender` clone keeps the channel open and hangs the consumer. Let clones move into tasks; `drop` the original explicitly when the borrow checker keeps it alive.
- **Always handle `broadcast`'s `Lagged`.** A slow subscriber should degrade gracefully, not crash.
- **Use message passing instead of shared state when you can.** The actor pattern (an `mpsc` of requests, each carrying a `oneshot` reply) replaces `Arc<Mutex<T>>` and removes a whole class of locking bugs. When you *do* need shared state, see [The `Arc<Mutex<T>>` Pattern](/11-async/12-arc-mutex-pattern/).
- **Keep `std::sync::mpsc` for plain threads.** Inside `async`, default to the Tokio family.
- **Combine channels with `select!` for shutdown.** A `watch<bool>` shutdown flag plus `tokio::select!` lets a task react to either work or a stop signal — see [Concurrent Awaiting](/11-async/07-select-join/).

---

## Real-World Example

A production graceful-shutdown worker pool brings the whole family together: an `mpsc` carries jobs (with backpressure), each job carries a `oneshot` for its individual reply, and a `watch<bool>` broadcasts the shutdown signal to every worker. The single `mpsc::Receiver` is shared across workers behind an `Arc<tokio::sync::Mutex<_>>`, and `tokio::select!` lets each worker react to *either* a new job *or* the shutdown flag.

```rust
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::{Duration, sleep};

/// A unit of work submitted to the pool, carrying a private reply channel.
struct Task {
    id: u32,
    reply: oneshot::Sender<String>,
}

#[tokio::main]
async fn main() {
    // Jobs flow worker-ward over a bounded mpsc (backpressure at 64).
    let (job_tx, job_rx) = mpsc::channel::<Task>(64);
    // A single shutdown flag broadcast to every worker via watch.
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // Spawn a small pool. mpsc is single-consumer, so we share the Receiver
    // behind an async Mutex; each worker pulls the next available job.
    let job_rx = std::sync::Arc::new(tokio::sync::Mutex::new(job_rx));
    let mut workers = Vec::new();
    for worker_id in 0..3 {
        let job_rx = job_rx.clone();
        let mut shutdown_rx = shutdown_rx.clone();
        workers.push(tokio::spawn(async move {
            loop {
                let task = {
                    let mut guard = job_rx.lock().await;
                    tokio::select! {
                        // Stop promptly when the shutdown flag flips to true.
                        _ = shutdown_rx.changed() => {
                            if *shutdown_rx.borrow() { break; }
                            continue;
                        }
                        maybe = guard.recv() => match maybe {
                            Some(task) => task,
                            None => break, // all senders dropped
                        },
                    }
                };
                // Simulate work, then answer this task's private oneshot.
                sleep(Duration::from_millis(5)).await;
                let _ = task
                    .reply
                    .send(format!("task {} handled by worker {worker_id}", task.id));
            }
        }));
    }

    // Submit 5 jobs and collect their individual replies.
    let mut replies = Vec::new();
    for id in 0..5 {
        let (reply, reply_rx) = oneshot::channel();
        job_tx.send(Task { id, reply }).await.unwrap();
        replies.push(reply_rx);
    }

    let mut results = Vec::new();
    for rx in replies {
        results.push(rx.await.unwrap());
    }
    results.sort();
    for line in &results {
        println!("{line}");
    }

    // Graceful shutdown: stop accepting work, signal workers, await them.
    drop(job_tx);
    shutdown_tx.send(true).unwrap();
    for w in workers {
        w.await.unwrap();
    }
    println!("pool drained and shut down cleanly");
}
```

Real output (one representative run — which worker handles which task varies between runs, but the task ordering is deterministic because we sort by task id):

```
task 0 handled by worker 0
task 1 handled by worker 1
task 2 handled by worker 2
task 3 handled by worker 2
task 4 handled by worker 0
pool drained and shut down cleanly
```

> **Note:** This pattern — bounded `mpsc` for work, `oneshot` for replies, `watch` for shutdown, `select!` to combine them — is the backbone of countless Rust services. It composes from small, independently testable pieces and never touches a shared lock for the business data.

---

## Further Reading

- [Tokio Tutorial — Channels](https://tokio.rs/tokio/tutorial/channels): the official walkthrough this page parallels.
- [`tokio::sync` module docs](https://docs.rs/tokio/latest/tokio/sync/index.html): overview of all four channels and when to use each.
- [`tokio::sync::mpsc`](https://docs.rs/tokio/latest/tokio/sync/mpsc/index.html) · [`oneshot`](https://docs.rs/tokio/latest/tokio/sync/oneshot/index.html) · [`broadcast`](https://docs.rs/tokio/latest/tokio/sync/broadcast/index.html) · [`watch`](https://docs.rs/tokio/latest/tokio/sync/watch/index.html): per-channel API references.
- [`std::sync::mpsc`](https://doc.rust-lang.org/std/sync/mpsc/index.html): the blocking standard-library channel, for plain threads.
- [Actors with Tokio](https://ryhl.io/blog/actors-with-tokio/): Alice Ryhl's canonical write-up of the request/response actor pattern.

Related sections of this guide:

- [Promises vs Futures](/11-async/00-promises-vs-futures/): why Rust futures are lazy and need a runtime at all.
- [Async/Await Syntax](/11-async/01-async-await/): `async`/`await` syntax and `?` inside async, which `recv().await` builds on.
- [Setting Up Tokio](/11-async/03-tokio-setup/): adding `tokio` with `features = ["full"]` and `#[tokio::main]`.
- [Spawning Tasks](/11-async/09-spawning-tasks/) — `tokio::spawn`, `JoinHandle`, and `spawn_blocking` for the producer/consumer tasks here.
- [Concurrent Awaiting](/11-async/07-select-join/) — `tokio::select!` and `join!`, used in the worker pool for shutdown.
- [Streams](/11-async/06-streams/) — turning a channel into a `Stream` you can iterate with `while let`.
- [Async Synchronization Primitives](/11-async/11-sync-primitives/) — the async `Mutex`/`RwLock` used to share an `mpsc::Receiver`.
- [The `Arc<Mutex<T>>` Pattern](/11-async/12-arc-mutex-pattern/) — when shared state beats message passing, and how to do it safely.
- [Ownership](/05-ownership/) — move semantics, which explain why senders/receivers behave as they do.
- [Basics](/02-basics/) — Rust fundamentals if you need a refresher.
- Next section: [Modules & Packages](/12-modules-packages/) — organizing crates and modules.

---

## Exercises

### Exercise 1: Sum over an `mpsc`

**Difficulty:** Easy

**Objective:** Wire up a basic producer/consumer with a bounded `mpsc` channel.

**Instructions:**

1. Create a bounded `mpsc::channel::<i32>(16)`.
2. Spawn a producer task that sends the numbers `1..=5`.
3. In `main`, drain the receiver with `while let Some(n) = rx.recv().await` and accumulate a sum.
4. Make sure the consumer loop actually terminates, then print the total.

<details>
<summary>Solution</summary>

```rust
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    let (tx, mut rx) = mpsc::channel::<i32>(16);

    let producer = tokio::spawn(async move {
        for n in 1..=5 {
            tx.send(n).await.unwrap();
        }
        // tx moved into the task and dropped here -> channel closes,
        // so rx.recv() will return None and the loop below ends.
    });

    let mut sum = 0;
    while let Some(n) = rx.recv().await {
        sum += n;
    }
    producer.await.unwrap();
    println!("sum = {sum}");
}
```

Output:

```
sum = 15
```

The key insight: the producer task *owns* `tx` after the `async move`, so when the task finishes the sender is dropped and `recv()` returns `None`.

</details>

### Exercise 2: One-shot square

**Difficulty:** Medium

**Objective:** Use `oneshot` to get a single computed value back from a spawned task, wrapped in a reusable async function.

**Instructions:**

1. Write `async fn compute(input: u64) -> u64`.
2. Inside it, create a `oneshot` channel, spawn a task that computes `input * input` and sends it, and `.await` the receiver.
3. Call `compute(9)` from `main` and print the result.

<details>
<summary>Solution</summary>

```rust
use tokio::sync::oneshot;

async fn compute(input: u64) -> u64 {
    let (tx, rx) = oneshot::channel();
    tokio::spawn(async move {
        let _ = tx.send(input * input);
    });
    // The Receiver is itself a Future; await it directly (no .recv()).
    rx.await.expect("worker replied")
}

#[tokio::main]
async fn main() {
    let result = compute(9).await;
    println!("9 squared = {result}");
}
```

Output:

```
9 squared = 81
```

`rx.await` yields `Result<T, RecvError>`; `expect` is fine here because we know the task always sends before dropping its sender.

</details>

### Exercise 3: Fan-out with `broadcast`

**Difficulty:** Medium–Hard

**Objective:** Build a pub/sub where two independent subscribers each collect every message, then verify they saw the same sequence.

**Instructions:**

1. Create a `broadcast::channel::<i32>(8)`.
2. Subscribe twice (`tx.subscribe()`), and spawn a task per subscriber that loops on `recv().await`, pushing each value into a `Vec` until the channel closes.
3. From `main`, send `1..=3`, then drop the sender.
4. Await both tasks and print each subscriber's collected `Vec`. Both must be `[1, 2, 3]`.

<details>
<summary>Solution</summary>

```rust
use tokio::sync::broadcast;

#[tokio::main]
async fn main() {
    let (tx, _) = broadcast::channel::<i32>(8);
    let mut a = tx.subscribe();
    let mut b = tx.subscribe();

    let ta = tokio::spawn(async move {
        let mut seen = Vec::new();
        // recv() returns Err(Closed) once the sender is dropped -> loop ends.
        while let Ok(v) = a.recv().await {
            seen.push(v);
        }
        seen
    });
    let tb = tokio::spawn(async move {
        let mut seen = Vec::new();
        while let Ok(v) = b.recv().await {
            seen.push(v);
        }
        seen
    });

    for n in 1..=3 {
        tx.send(n).unwrap();
    }
    drop(tx); // close the channel so both receivers stop

    println!("a saw {:?}", ta.await.unwrap());
    println!("b saw {:?}", tb.await.unwrap());
}
```

Output:

```
a saw [1, 2, 3]
b saw [1, 2, 3]
```

Both subscribers must subscribe *before* any message is sent — `broadcast` only delivers messages sent after a receiver subscribes. With capacity 8 and only three messages, no lag occurs.

</details>
