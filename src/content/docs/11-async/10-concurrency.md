---
title: "Concurrency vs Parallelism"
description: "Where Node interleaves tasks on one thread, Rust splits concurrency from parallelism: cheap M:N Tokio tasks, real parallel threads, and futures you can cancel."
---

Coming from Node.js, you have lived your whole career inside a single-threaded event loop: lots of things happen "at once," but only one line of your JavaScript ever executes at a time. Rust forces you to be precise about a distinction Node mostly hides — the difference between **concurrency** (juggling many tasks) and **parallelism** (actually running them simultaneously on multiple cores) — and gives you safe tools for both.

---

## Quick Overview

**Concurrency** is dealing with many things at once (structure); **parallelism** is doing many things at once (execution). Node.js gives you concurrency on one thread via the event loop; Rust gives you concurrency *and* opt-in, compiler-checked parallelism. This page covers how Rust's async **tasks** relate to OS **threads**, how to structure groups of tasks so they don't leak, and how to **cancel** work cleanly, something that is genuinely hard in JavaScript.

> **Note:** This page is about the *concepts and patterns* of concurrency. The mechanics live in sibling pages: [spawning tasks](/11-async/09-spawning-tasks/), [`select!`/`join!`](/11-async/07-select-join/), [channels](/11-async/08-channels/), and [sync primitives](/11-async/11-sync-primitives/). The deeper "async vs threads vs blocking" decision is its own page: [Async vs Sync](/11-async/13-async-vs-sync/).

---

## TypeScript/JavaScript Example

In Node.js, you achieve concurrency by kicking off multiple promises and awaiting them together. This *looks* parallel, but the JavaScript callbacks never run simultaneously: the event loop interleaves them on one thread. The waiting (I/O, timers) happens off-thread; your code does not.

```typescript
// TypeScript / JavaScript (Node v22)
// Concurrency on one thread: three "downloads" overlap because the WAITING
// is offloaded to libuv, but our JS code runs one statement at a time.

function fetchUser(id: number): Promise<string> {
  return new Promise((resolve) => {
    setTimeout(() => resolve(`user-${id}`), 100); // simulated network latency
  });
}

async function main() {
  const start = Date.now();

  // Promise.all runs all three concurrently — total ~100ms, not ~300ms.
  const users = await Promise.all([fetchUser(1), fetchUser(2), fetchUser(3)]);

  console.log(`fetched ${JSON.stringify(users)} in ~${Date.now() - start} ms`);
}

main();
// fetched ["user-1","user-2","user-3"] in ~10X ms
```

For **CPU-bound** work, Node cannot help you with `Promise.all`: a heavy `for` loop blocks the event loop and everything else stalls. To get real parallelism you must reach for **Worker Threads** (or child processes) and pass messages across a serialization boundary:

```typescript
// CPU-bound parallelism in Node requires Worker Threads — a separate V8 isolate.
import { Worker } from "node:worker_threads";

function countPrimes(maxN: number): Promise<number> {
  return new Promise((resolve, reject) => {
    const worker = new Worker("./prime-worker.js", { workerData: maxN });
    worker.on("message", resolve);
    worker.on("error", reject);
  });
}

// Each worker is a heavyweight thread with its OWN heap; data is copied, not shared.
```

---

## Rust Equivalent

Rust draws the same picture, but the pieces are explicit. A **task** (`tokio::spawn`) is the lightweight, async analogue of "kick off a promise." On Tokio's default multi-thread runtime, those tasks are scheduled across a pool of worker threads, so I/O-bound tasks run **concurrently** and CPU work spread across them can run **in parallel**: no Worker-Thread ceremony, no copying.

```rust
// Rust + Tokio 1.52 — concurrency via tasks.
use std::time::Instant;
use tokio::time::{sleep, Duration};

// Simulated I/O-bound work: fetch a user record from a remote service.
async fn fetch_user(id: u32) -> String {
    sleep(Duration::from_millis(100)).await; // pretend this is a network round-trip
    format!("user-{id}")
}

#[tokio::main]
async fn main() {
    let start = Instant::now();

    // Spawn three tasks. Each runs CONCURRENTLY on the runtime.
    // On the multi-thread scheduler they may also run in PARALLEL.
    let handles: Vec<_> = (1..=3)
        .map(|id| tokio::spawn(fetch_user(id)))
        .collect();

    // Await all of them (like Promise.all).
    let mut users = Vec::new();
    for handle in handles {
        users.push(handle.await.unwrap());
    }

    println!("fetched {:?} in ~{} ms", users, start.elapsed().as_millis());
}
```

Real output (Rust 1.96, Tokio 1.52):

```
fetched ["user-1", "user-2", "user-3"] in ~102 ms
```

For **CPU-bound** parallelism Rust does not need a separate isolate or message passing. You can use plain OS threads with shared, borrow-checked memory:

```rust
use std::thread;

// CPU-bound work split across OS threads — true parallelism, no async needed.
fn sum_range(lo: u64, hi: u64) -> u64 {
    (lo..hi).fold(0u64, |a, x| a.wrapping_add(x.wrapping_mul(x)))
}

fn main() {
    // thread::scope lets borrowed data cross into threads safely (stable since 1.63).
    let total = thread::scope(|s| {
        let h1 = s.spawn(|| sum_range(0, 50_000_000));
        let h2 = s.spawn(|| sum_range(50_000_000, 100_000_000));
        h1.join().unwrap().wrapping_add(h2.join().unwrap())
    });
    println!("sum = {total}");
}
```

> **Note:** Mixing the two is also fine and idiomatic: run CPU-bound work on threads via [`spawn_blocking`](/11-async/09-spawning-tasks/) *from inside* async code, so it does not stall the event loop. More on that choice in [Async vs Sync](/11-async/13-async-vs-sync/).

---

## Detailed Explanation

### Concurrency is structure; parallelism is execution

The canonical phrasing (from Rob Pike) is: *"Concurrency is about dealing with lots of things at once. Parallelism is about doing lots of things at once."* Concurrency is how you **structure** a program as independently-progressing tasks; parallelism is a **runtime property**: whether those tasks happen to execute at the same instant on different cores.

- A Node.js server is **concurrent** (thousands of in-flight requests) but **not parallel** (one thread runs your JS).
- A Rust `tokio` server on the multi-thread runtime is **both**: thousands of concurrent tasks, executing across N worker threads in parallel.
- A single-threaded `tokio` runtime (`flavor = "current_thread"`) is concurrent but **not** parallel, the closest match to Node's model.

You write the *same* `async` code either way; the runtime flavor decides whether parallelism is on the table. That separation is the whole point: you express concurrency once, and choose the execution strategy separately.

### Tasks are not threads

A `tokio::spawn`ed **task** is *not* an OS thread. It is a future that the runtime schedules cooperatively onto a small pool of worker threads (by default, one per CPU core). This is "M:N" scheduling: M tasks multiplexed onto N threads.

The practical consequence is that tasks are *cheap*. Spawning a hundred thousand of them is routine; spawning a hundred thousand OS threads would exhaust memory because each thread reserves megabytes of stack.

```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

// Tasks are cheap: spawning 100,000 of them is fine. 100,000 OS THREADS
// would exhaust memory (each thread reserves ~MBs of stack).
#[tokio::main]
async fn main() {
    let counter = Arc::new(AtomicU64::new(0));
    let mut handles = Vec::with_capacity(100_000);

    for _ in 0..100_000 {
        let counter = Arc::clone(&counter);
        handles.push(tokio::spawn(async move {
            counter.fetch_add(1, Ordering::Relaxed);
        }));
    }
    for h in handles {
        h.await.unwrap();
    }

    println!("100,000 tasks all ran: counter = {}", counter.load(Ordering::Relaxed));
}
```

Real output:

```
100,000 tasks all ran: counter = 100000
```

> **Tip:** Use a **task** for I/O-bound concurrency (waiting on sockets, timers, databases). Use a **thread** (or [`spawn_blocking`](/11-async/09-spawning-tasks/)) for CPU-bound or blocking work. The litmus test: does it spend its time *waiting*, or *computing*? Waiting → task; computing → thread. See [Async vs Sync](/11-async/13-async-vs-sync/).

### The runtime flavor decides parallelism

Tasks only run in parallel if the runtime has more than one worker thread. Compare the two flavors by counting the distinct OS threads that actually execute the tasks.

On the **multi-thread** runtime (the default for `#[tokio::main]`), tasks spread across worker threads:

```rust
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

#[tokio::main] // multi-thread: one worker per CPU core by default
async fn main() {
    let threads = Arc::new(Mutex::new(HashSet::new()));
    let mut handles = Vec::new();

    for _ in 0..8 {
        let threads = Arc::clone(&threads);
        handles.push(tokio::spawn(async move {
            // A CPU loop so a worker stays busy long enough to spread the load.
            let mut acc = 0u64;
            for i in 0..20_000_000u64 {
                acc = acc.wrapping_add(i);
            }
            threads.lock().unwrap().insert(format!("{:?}", std::thread::current().id()));
            acc
        }));
    }
    for h in handles {
        h.await.unwrap();
    }

    let count = threads.lock().unwrap().len();
    println!("8 tasks ran across {count} distinct OS threads");
}
```

Real output on an 8+ core machine:

```
8 tasks ran across 8 distinct OS threads
```

> **Note:** That `8` is the worker-thread count, which equals the number of CPU cores on the test machine. On a 4-core box you would see `4`; the exact number is hardware-dependent. The point is **more than one**: real parallelism.

The **current-thread** runtime (`flavor = "current_thread"`) runs everything on one thread: concurrent, never parallel, exactly like Node's event loop:

```rust
use std::collections::HashSet;
use std::sync::{Arc, Mutex};

// current_thread flavor: everything runs on ONE thread (like Node's event loop).
// Tasks are CONCURRENT (interleaved) but never PARALLEL.
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let threads = Arc::new(Mutex::new(HashSet::new()));
    let mut handles = Vec::new();

    for _ in 0..8 {
        let threads = Arc::clone(&threads);
        handles.push(tokio::spawn(async move {
            tokio::task::yield_now().await; // let the single thread interleave tasks
            threads.lock().unwrap().insert(format!("{:?}", std::thread::current().id()));
        }));
    }
    for h in handles {
        h.await.unwrap();
    }

    let count = threads.lock().unwrap().len();
    println!("8 tasks ran across {count} distinct OS thread(s)");
}
```

Real output:

```
8 tasks ran across 1 distinct OS thread(s)
```

This is why the multi-thread runtime requires your tasks to be `Send`: a task may be moved to a different worker thread at an `.await` point. The single-thread runtime has no such requirement. (See the Pitfalls section for the `!Send` error this produces.) Runtime flavors are covered in depth in [The Tokio Runtime](/11-async/02-tokio-intro/).

### Concurrency on one thread is still a win for I/O

Even with no parallelism, concurrency speeds up I/O-bound work, because waiting overlaps. Here three async sleeps run on a single thread and still finish in ~100ms, not ~300ms; the thread is free to advance other tasks while each one waits:

```rust
use std::time::Instant;

// tokio::time::sleep yields to the runtime, so other tasks run during the wait.
async fn good_task(id: u32) {
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    println!("  good_task {id} done");
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let start = Instant::now();
    tokio::join!(good_task(1), good_task(2), good_task(3));
    println!("non-blocking version took ~{} ms", start.elapsed().as_millis());
}
```

Real output (ordering of the "done" lines varies):

```
  good_task 2 done
  good_task 3 done
  good_task 1 done
non-blocking version took ~102 ms
```

This is the heart of why async exists: a single thread can serve thousands of waiting connections. Parallelism adds throughput on top; concurrency alone already wins for I/O.

### Structured concurrency: don't leak tasks

A loose `tokio::spawn` is fire-and-forget: if the parent finishes, the child may be silently abandoned, and errors inside it vanish. **Structured concurrency** keeps a group of tasks tied to a scope: you spawn them into a container, await them together, and when the container is dropped any stragglers are cancelled. Tokio's [`JoinSet`](https://docs.rs/tokio/latest/tokio/task/struct.JoinSet.html) is the workhorse:

```rust
use tokio::task::JoinSet;

async fn process(item: u32) -> u32 {
    tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    item * item
}

#[tokio::main]
async fn main() {
    let mut set = JoinSet::new();

    // Spawn a dynamic number of tasks into a structured set.
    for item in 1..=5 {
        set.spawn(process(item));
    }

    // join_next waits for the next task to finish, in completion order.
    // When `set` is dropped, any still-running tasks are aborted automatically.
    let mut total = 0;
    while let Some(res) = set.join_next().await {
        total += res.unwrap();
    }

    println!("sum of squares = {total}");
}
```

Real output:

```
sum of squares = 55
```

The key safety property: `JoinSet` owns the tasks. If `main` returns early, panics, or you `break` out of the loop, the set's `Drop` aborts the unfinished tasks. There are no orphaned, still-running tasks logging into the void: the failure mode you get with bare `spawn`. This is the Rust answer to a JavaScript codebase littered with un-awaited promises.

### Cancellation: a first-class concept

This is where Rust pulls decisively ahead of JavaScript. A `Promise`, once started, runs to completion even if nobody is listening; there is no built-in `.cancel()`. (The `AbortController`/`AbortSignal` API helps, but only for APIs that explicitly opt in, like `fetch`.) In Rust, a future is just a value, so **dropping it stops its work**, and its destructors run for clean teardown. Three layers, from coarse to fine:

1. **Drop the future** → its `poll` is never called again; its `Drop` impls run.
2. **`JoinHandle::abort()` / `JoinSet::abort_all()`** → ask the runtime to stop a spawned task at its next `.await`.
3. **Cooperative cancellation** with a [`CancellationToken`](https://docs.rs/tokio-util/latest/tokio_util/sync/struct.CancellationToken.html) → the task checks a signal and winds down gracefully.

The simplest form is a timeout, which drops the loser:

```rust
use tokio::time::{timeout, sleep, Duration};

async fn slow_query() -> &'static str {
    sleep(Duration::from_millis(500)).await;
    "query result"
}

#[tokio::main]
async fn main() {
    // Like Promise.race([query, timer]) — but the losing future is actually
    // DROPPED (cancelled), not left running in the background.
    match timeout(Duration::from_millis(100), slow_query()).await {
        Ok(value) => println!("got: {value}"),
        Err(_elapsed) => println!("timed out — the query future was dropped/cancelled"),
    }
}
```

Real output:

```
timed out — the query future was dropped/cancelled
```

Because cancellation runs destructors, you get RAII-style cleanup for free. A guard's `Drop` fires whether the task completes or is aborted:

```rust
use tokio::time::{sleep, Duration};

// A guard whose Drop runs even when the future is cancelled (dropped).
struct CleanupGuard;
impl Drop for CleanupGuard {
    fn drop(&mut self) {
        println!("  cleanup: releasing resources");
    }
}

async fn job() {
    let _guard = CleanupGuard; // Drop runs if the future is cancelled mid-flight
    println!("  job started");
    sleep(Duration::from_secs(10)).await; // never completes in this demo
    println!("  job finished"); // unreachable here
}

#[tokio::main]
async fn main() {
    let handle = tokio::spawn(job());
    sleep(Duration::from_millis(50)).await; // let the job start

    handle.abort(); // request cancellation
    let result = handle.await;
    println!("task was cancelled: {}", result.unwrap_err().is_cancelled());
}
```

Real output:

```
  job started
  cleanup: releasing resources
task was cancelled: true
```

Notice the `CleanupGuard::drop` ran even though the job never reached its own end: that is the cancellation path executing your teardown. In JavaScript there is no equivalent: a Promise that "loses" a `Promise.race` keeps executing, and any cleanup you wrote after its `await` simply never fires.

> **Warning:** Cancellation in Rust happens **only at `.await` points**. A task that enters a long synchronous CPU loop with no `.await` cannot be aborted until it next yields: the `abort` is recorded but only takes effect at the next suspension. This is why CPU-bound work belongs on a thread / [`spawn_blocking`](/11-async/09-spawning-tasks/), not inline in a task.

### Cooperative cancellation with `CancellationToken`

For graceful shutdown — letting a worker finish its current unit and clean up — use a `CancellationToken` from `tokio-util`. Cloning the token shares one cancellation state, so a single `.cancel()` signals every holder at once. This is the idiomatic shape for a long-lived worker loop, combining it with [`select!`](/11-async/07-select-join/):

```rust
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;
use tokio::task::JoinSet;

// Three workers that share one CancellationToken. When the token fires,
// they all wind down — structured, parent-driven cancellation.
async fn worker(id: u32, token: CancellationToken) {
    let mut done = 0;
    loop {
        tokio::select! {
            // Branch 1: cancellation requested — exit the loop cleanly.
            _ = token.cancelled() => {
                println!("  worker {id}: stopping (did {done} units)");
                return;
            }
            // Branch 2: do one unit of work.
            _ = sleep(Duration::from_millis(30)) => {
                done += 1;
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let token = CancellationToken::new();
    let mut set = JoinSet::new();
    for id in 1..=3 {
        set.spawn(worker(id, token.clone())); // clones share one cancellation state
    }

    // Run for a bit, then signal shutdown to all workers at once.
    sleep(Duration::from_millis(100)).await;
    println!("main: broadcasting shutdown");
    token.cancel();

    // Wait for every worker to finish its cleanup before exiting.
    while set.join_next().await.is_some() {}
    println!("main: all workers stopped cleanly");
}
```

Real output:

```
main: broadcasting shutdown
  worker 1: stopping (did 3 units)
  worker 2: stopping (did 3 units)
  worker 3: stopping (did 3 units)
main: all workers stopped cleanly
```

This is the difference between *forced* cancellation (`abort`: stop at the next `.await`, no chance to react) and *cooperative* cancellation (the worker observes the token and decides how to wind down). For graceful shutdown you almost always want the cooperative version, because the worker controls its own teardown.

---

## Key Differences

| Aspect | Node.js / TypeScript | Rust + Tokio |
| --- | --- | --- |
| **Concurrency unit** | Promise on the event loop | Task (`tokio::spawn`) or future |
| **Parallelism unit** | Worker Thread (separate isolate) | OS thread / multi-thread runtime workers |
| **Threads under the hood** | One JS thread + libuv pool | M:N, many tasks over N worker threads |
| **Default execution** | Concurrent, single-threaded | Concurrent + parallel (multi-thread runtime) |
| **Single-threaded mode** | The only mode for JS code | Opt-in (`flavor = "current_thread"`) |
| **Sharing data across workers** | Copy / structured clone / `SharedArrayBuffer` | Borrow-checked shared memory (`Arc`, `Mutex`) |
| **Task cost** | Promise is cheap; Worker is heavy | Task is very cheap; thread is heavy |
| **Cancellation** | No built-in cancel; `AbortController` opt-in | Drop the future, `abort()`, or `CancellationToken` |
| **Cleanup on cancel** | Code after a lost `await` never runs | `Drop` impls run on cancellation |
| **Leak protection** | Un-awaited promises run unsupervised | `JoinSet` cancels stragglers on drop |

### Why Rust makes you choose

Node's "everything is one thread" model is simpler but limiting: CPU-bound work blocks everything, and parallelism means crossing a serialization boundary into a Worker. Rust separates the two axes — you write concurrent `async` code, then pick a runtime flavor for the parallelism you want — and the borrow checker lets parallel threads share memory *safely* (data races are a compile error). This is "fearless concurrency": the cost is that the compiler insists you make `Send`/`Sync` correct up front, which can feel strict at first.

### The "cancellation gap" closed

The most underappreciated win is cancellation. In JavaScript, "stop this in-flight work" ranges from awkward to impossible. In Rust it is the *default consequence of dropping a value*, with destructors guaranteeing cleanup. This makes timeouts, "race and discard the loser," and graceful shutdown straightforward and correct, patterns that are perennially buggy in Node services.

---

## Common Pitfalls

### Pitfall 1: Non-`Send` data held across `.await` in a spawned task

The multi-thread runtime may move a task between worker threads at any `.await`, so spawned futures must be `Send`. Holding a `!Send` type (like `Rc`, the non-atomic reference counter; see [Section 05: reference counting](/05-ownership/07-reference-counting/)) across an `.await` makes the whole future `!Send`, and `tokio::spawn` rejects it.

```rust
use std::rc::Rc;

#[tokio::main]
async fn main() {
    tokio::spawn(async {
        let data = Rc::new(5); // Rc is !Send
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
        println!("{}", data); // data held across .await -> future is !Send
    });
}
```

Real compiler error (`cargo build`):

```
error: future cannot be sent between threads safely
   --> src/main.rs:5:5
    |
  5 | /     tokio::spawn(async {
  6 | |         let data = Rc::new(5); // Rc is !Send
  7 | |         tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
  8 | |         println!("{}", data); // data held across .await -> future is !Send
  9 | |     });
    | |______^ future created by async block is not `Send`
    |
    = help: within `{async block@src/main.rs:5:18: 5:23}`, the trait `Send` is not implemented for `Rc<i32>`
note: future is not `Send` as this value is used across an await
   --> src/main.rs:7:67
    |
  6 |         let data = Rc::new(5); // Rc is !Send
    |             ---- has type `Rc<i32>` which is not `Send`
  7 |         tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
    |                                                                   ^^^^^ await occurs here, with `data` maybe used later
note: required by a bound in `tokio::spawn`
```

The fixes: use `Arc` instead of `Rc` (it is the atomic, `Send` reference counter), or restructure so the `!Send` value is dropped *before* the `.await` (e.g., enclose it in a `{ }` block that ends first). If the work genuinely must stay on one thread, use a `LocalSet` or the `current_thread` runtime.

### Pitfall 2: Blocking the runtime with synchronous waits

The number-one async footgun for Node developers: calling a *blocking* function (`std::thread::sleep`, synchronous file/network I/O, a tight CPU loop) directly inside a task. It does not yield, so it freezes the worker thread, starving every other task scheduled there. This compiles and runs; it is a performance bug, which makes it sneaky.

```rust
use std::time::Instant;

// Anti-pattern: std::thread::sleep BLOCKS the worker thread, so other tasks
// on that thread cannot make progress.
async fn bad_task(id: u32) {
    std::thread::sleep(std::time::Duration::from_millis(100)); // blocks the thread!
    println!("  bad_task {id} done");
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let start = Instant::now();
    // Even though we "join" them, on a single-threaded runtime they run serially
    // because each one monopolizes the only worker thread.
    tokio::join!(bad_task(1), bad_task(2), bad_task(3));
    println!("blocking version took ~{} ms", start.elapsed().as_millis());
}
```

Real output, ~300ms (serial) instead of ~100ms (overlapping):

```
  bad_task 1 done
  bad_task 2 done
  bad_task 3 done
blocking version took ~306 ms
```

The fix for waiting is the async equivalent (`tokio::time::sleep(...).await`, async I/O). The fix for genuine blocking/CPU work is [`tokio::task::spawn_blocking`](/11-async/09-spawning-tasks/), which moves it onto a dedicated blocking-thread pool so the async workers stay free.

### Pitfall 3: Confusing "concurrent" with "parallel"

Two sequential `.await`s are *not* concurrent: the second future has not even started while the first runs (Rust futures are lazy; see [Promises vs Futures](/11-async/00-promises-vs-futures/)). And concurrent is not the same as parallel: on a `current_thread` runtime, `join!`/`select!` interleave tasks on one thread with zero parallelism. If you measured `join!` of CPU-bound tasks on the single-thread runtime expecting a speedup, you would get none — there is only one core in play. Match the tool to the goal: `join!` for overlapping *waiting*, the multi-thread runtime or threads for overlapping *computing*.

### Pitfall 4: Assuming a Promise-style "cancel" cancels nothing

Because JavaScript Promises cannot really be cancelled, developers sometimes assume Rust's are the same and leave work running. The opposite is true: in Rust, *forgetting* to keep a future alive cancels it. If you `tokio::spawn` a task and drop the `JoinHandle` without awaiting it, the task keeps running (spawn detaches it). But if you build a future and never spawn or `.await` it, it never runs at all. And dropping a `JoinSet` aborts everything inside. The rule of thumb: a future's lifetime *is* its work's lifetime, unless you explicitly `spawn` to detach it.

---

## Best Practices

- **Pick the runtime flavor deliberately.** Default `#[tokio::main]` (multi-thread) for servers and parallel workloads; `flavor = "current_thread"` for CLIs, tests, or when every task is `!Send` and you want Node-like single-threaded semantics. See [Setting Up Tokio](/11-async/03-tokio-setup/).
- **Prefer structured concurrency.** Reach for [`JoinSet`](/11-async/09-spawning-tasks/) (or `join!`/`try_join!`) over scattered `tokio::spawn` so tasks are owned, awaited, and cancelled together. A bare `spawn` is "detach this from supervision"; use it intentionally.
- **Keep tasks non-blocking.** Never call blocking code inside a task. Use async APIs for I/O and `spawn_blocking` for CPU/blocking work, so the scheduler can keep the event loop responsive.
- **Bound your concurrency.** Unbounded `spawn` can overwhelm a database or remote API. Gate in-flight work with a [`Semaphore`](/11-async/11-sync-primitives/) (shown below): the safe analogue of a connection pool.
- **Design for cancellation from the start.** Thread a `CancellationToken` through long-lived workers and select on it, so graceful shutdown is built in rather than bolted on. Let `Drop` handle resource cleanup.
- **Use `Arc<Mutex<T>>` for shared mutable state across tasks**, and `Arc` alone for shared read-only data. The pattern is detailed in [The `Arc<Mutex<T>>` Pattern](/11-async/12-arc-mutex-pattern/); when to use the async-aware `tokio::sync::Mutex` vs `std::sync::Mutex` is in [Async Synchronization Primitives](/11-async/11-sync-primitives/).

---

## Real-World Example

A concurrent fetcher that processes a list of URLs with **bounded concurrency**: the bread-and-butter of crawlers, batch importers, and fan-out API clients. It combines everything on this page: tasks for concurrency, a `Semaphore` to cap in-flight work (like a connection pool), and a `JoinSet` for structured collection so nothing leaks.

```rust
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::{sleep, Duration};

/// Fetch one URL (simulated). Returns (url, byte length).
async fn fetch(url: String) -> (String, usize) {
    sleep(Duration::from_millis(100)).await; // simulated network latency
    (url.clone(), url.len() * 10)
}

#[tokio::main]
async fn main() {
    let urls: Vec<String> = (1..=10)
        .map(|i| format!("https://example.com/page/{i}"))
        .collect();

    // Limit concurrency to 3 in-flight requests at a time (like a connection pool).
    let limit = Arc::new(Semaphore::new(3));
    let mut set: JoinSet<(String, usize)> = JoinSet::new();
    let start = Instant::now();

    for url in urls {
        let limit = Arc::clone(&limit);
        set.spawn(async move {
            // Acquire a permit; this await blocks until a slot is free.
            let _permit = limit.acquire_owned().await.unwrap();
            fetch(url).await
            // _permit dropped here -> slot returned to the pool
        });
    }

    // Collect results as tasks finish. Dropping `set` would abort any stragglers.
    let mut total_bytes = 0;
    let mut count = 0;
    while let Some(res) = set.join_next().await {
        let (_url, bytes) = res.unwrap();
        total_bytes += bytes;
        count += 1;
    }

    println!(
        "fetched {count} pages ({total_bytes} bytes) in ~{} ms with max 3 concurrent",
        start.elapsed().as_millis()
    );
}
```

Real output (10 pages, 3 at a time, ~100ms each → ~4 batches):

```
fetched 10 pages (2610 bytes) in ~411 ms with max 3 concurrent
```

The timing tells the story: 10 requests at 100ms each, run three-wide, take roughly `ceil(10 / 3) × 100 ≈ 400 ms` rather than 1000ms (fully serial) or 100ms (fully unbounded). The `Semaphore` is the throttle, the `JoinSet` is the structure, and dropping either cleanly cancels everything in flight.

> **Note:** In real code, `fetch` would use an HTTP client like `reqwest` and return a `Result`, which you would collect with `try_join!` or by matching in the loop. See [Concurrent Awaiting](/11-async/07-select-join/) for the error-aware combinators and [Async Channels](/11-async/08-channels/) for streaming results back as they arrive rather than collecting at the end.

---

## Further Reading

### Official Documentation

- [The Rust Book — Fearless Concurrency](https://doc.rust-lang.org/book/ch16-00-concurrency.html) — threads, message passing, shared state
- [The Rust Book — Async, Tasks, and Futures](https://doc.rust-lang.org/book/ch17-00-async-await.html)
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial) — especially [Spawning](https://tokio.rs/tokio/tutorial/spawning) and [Graceful Shutdown](https://tokio.rs/tokio/topics/shutdown)
- [`tokio::task::JoinSet`](https://docs.rs/tokio/latest/tokio/task/struct.JoinSet.html) and [`tokio_util::sync::CancellationToken`](https://docs.rs/tokio-util/latest/tokio_util/sync/struct.CancellationToken.html)
- [`std::thread::scope`](https://doc.rust-lang.org/std/thread/fn.scope.html) — scoped threads for CPU parallelism

### Related Sections in This Guide

- [Promises vs Futures](/11-async/00-promises-vs-futures/) — eager Promises vs lazy futures (read this first)
- [Async/Await Syntax](/11-async/01-async-await/) — the `async`/`await` syntax and `?` error handling
- [The Tokio Runtime](/11-async/02-tokio-intro/) — Node's event loop vs the Tokio runtime; multi-thread vs current-thread
- [Setting Up Tokio](/11-async/03-tokio-setup/) — adding Tokio, `#[tokio::main]`, the runtime builder
- [Spawning Tasks](/11-async/09-spawning-tasks/) — `tokio::spawn`, `JoinHandle`, `spawn_blocking`, tasks vs threads
- [Concurrent Awaiting](/11-async/07-select-join/) — `Promise.all`/`Promise.race` → `join!`/`try_join!`/`select!`
- [Async Channels](/11-async/08-channels/) — moving data between tasks with `mpsc`/`oneshot`/`broadcast`/`watch`
- [Async Synchronization Primitives](/11-async/11-sync-primitives/) — async vs `std` `Mutex`/`RwLock`/`Semaphore`
- [The `Arc<Mutex<T>>` Pattern](/11-async/12-arc-mutex-pattern/) — shared mutable state across tasks
- [Async vs Sync](/11-async/13-async-vs-sync/) — CPU-bound vs I/O-bound; when to use async, threads, or blocking
- [Section 05: Ownership](/05-ownership/) and [reference counting](/05-ownership/07-reference-counting/) — `Arc` vs `Rc`, `Send`/`Sync` foundations
- [Section 01: Getting Started](/01-getting-started/) and [Section 02: Basics](/02-basics/) — the fundamentals these examples assume
- [Section 12: Modules & Packages](/12-modules-packages/) — adding `tokio`/`tokio-util` and bringing them into scope

---

## Exercises

### Exercise 1: Concurrent, not sequential

**Difficulty:** Beginner

**Objective:** Turn a sequential set of I/O-bound calls into concurrent tasks and observe the timing collapse.

**Instructions:** The program below checks three services one after another, taking ~250ms. Rewrite it to run all three checks concurrently using `tokio::spawn` (or `JoinSet`), so the total time drops to roughly the slowest single check (~120ms). Print how many of the three are "healthy" (latency under 100ms).

```rust
use std::time::Instant;
use tokio::time::{sleep, Duration};

async fn check_health(service: &str, ms: u64) -> (String, bool) {
    sleep(Duration::from_millis(ms)).await;
    (service.to_string(), ms < 100)
}

#[tokio::main]
async fn main() {
    let start = Instant::now();
    // TODO: run these three concurrently instead of awaiting one at a time.
    let a = check_health("auth", 50).await;
    let b = check_health("db", 80).await;
    let c = check_health("cache", 120).await;
    let healthy = [a, b, c].iter().filter(|(_, ok)| *ok).count();
    println!("{healthy}/3 healthy in ~{} ms", start.elapsed().as_millis());
}
```

<details>
<summary>Solution</summary>

Spawn each check into a `JoinSet` so they overlap; collect as they complete. (You could also use `tokio::join!` since the count is fixed.)

```rust
use std::time::Instant;
use tokio::task::JoinSet;
use tokio::time::{sleep, Duration};

async fn check_health(service: &str, ms: u64) -> (String, bool) {
    sleep(Duration::from_millis(ms)).await;
    (service.to_string(), ms < 100)
}

#[tokio::main]
async fn main() {
    let start = Instant::now();
    let services = [("auth", 50u64), ("db", 80), ("cache", 120)];

    let mut set = JoinSet::new();
    for (name, ms) in services {
        set.spawn(check_health(name, ms));
    }

    let mut healthy = 0;
    let mut total = 0;
    while let Some(res) = set.join_next().await {
        let (name, ok) = res.unwrap();
        total += 1;
        if ok { healthy += 1; }
        println!("  {name}: {}", if ok { "healthy" } else { "DEGRADED" });
    }
    println!("{healthy}/{total} healthy in ~{} ms", start.elapsed().as_millis());
}
```

Real output (the per-service lines arrive in completion order, ~123ms total: the slowest check, not the sum):

```
  auth: healthy
  db: healthy
  cache: DEGRADED
2/3 healthy in ~123 ms
```

The three checks overlap, so the total is governed by the slowest one (~120ms), not their sum (~250ms). See [Concurrent Awaiting](/11-async/07-select-join/) for the `join!` alternative.

</details>

### Exercise 2: Fail fast and cancel the rest

**Difficulty:** Intermediate

**Objective:** Use structured concurrency to abort outstanding tasks the moment one fails.

**Instructions:** Three downloads run concurrently; one of them (`id == 3`) fails fast. Complete the program so that, on the first error, it prints the error, **aborts the remaining in-flight downloads**, and stops. Use a `JoinSet` and its `abort_all()` method.

```rust
use tokio::task::JoinSet;
use tokio::time::{sleep, Duration};

async fn download(id: u32, ms: u64) -> Result<String, String> {
    sleep(Duration::from_millis(ms)).await;
    if id == 3 {
        Err(format!("download {id} failed"))
    } else {
        Ok(format!("file-{id}"))
    }
}

#[tokio::main]
async fn main() {
    let mut set = JoinSet::new();
    set.spawn(download(1, 80));
    set.spawn(download(2, 40));
    set.spawn(download(3, 20)); // fails fastest

    // TODO: collect results; on the first Err, abort the rest and stop.
    while let Some(joined) = set.join_next().await {
        let _ = joined; // replace with real handling
    }
    println!("done");
}
```

<details>
<summary>Solution</summary>

```rust
use tokio::task::JoinSet;
use tokio::time::{sleep, Duration};

async fn download(id: u32, ms: u64) -> Result<String, String> {
    sleep(Duration::from_millis(ms)).await;
    if id == 3 {
        Err(format!("download {id} failed"))
    } else {
        Ok(format!("file-{id}"))
    }
}

#[tokio::main]
async fn main() {
    let mut set = JoinSet::new();
    set.spawn(download(1, 80));
    set.spawn(download(2, 40));
    set.spawn(download(3, 20)); // this one fails fastest

    // Stop at the first error; abort the remaining tasks.
    while let Some(joined) = set.join_next().await {
        match joined.unwrap() {
            Ok(file) => println!("ok: {file}"),
            Err(e) => {
                println!("error: {e} -> aborting remaining downloads");
                set.abort_all();
                break;
            }
        }
    }
    println!("done");
}
```

Real output (the fastest task fails first, so the others are aborted before they print):

```
error: download 3 failed -> aborting remaining downloads
done
```

This is the `try_join!` philosophy generalized to a dynamic set: the first failure short-circuits the group. `abort_all()` plus `break` ensures no orphaned tasks survive. Compare with [Concurrent Awaiting](/11-async/07-select-join/)'s `try_join!`.

</details>

### Exercise 3: Graceful shutdown with a cancellation token

**Difficulty:** Advanced

**Objective:** Build a long-running worker that shuts down cooperatively, finishing cleanly rather than being killed mid-step.

**Instructions:** Spawn a worker that processes "jobs" every 30ms in a loop. From `main`, let it run ~100ms, then signal a `CancellationToken`. The worker must detect the signal, print how many jobs it completed, and return. `main` should wait for the worker to finish before exiting. Add `tokio-util` to your `Cargo.toml` (`cargo add tokio-util`).

```rust
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;

async fn worker(token: CancellationToken) {
    let mut jobs = 0;
    loop {
        // TODO: select between the cancellation signal and doing one job.
        // On cancellation: print the job count and return.
        todo!()
    }
}

#[tokio::main]
async fn main() {
    let token = CancellationToken::new();
    let handle = tokio::spawn(worker(token.clone()));
    sleep(Duration::from_millis(100)).await;
    // TODO: request shutdown, then wait for the worker to finish.
    todo!()
}
```

<details>
<summary>Solution</summary>

```rust
use tokio::time::{sleep, Duration};
use tokio_util::sync::CancellationToken;

async fn worker(token: CancellationToken) {
    let mut jobs = 0;
    loop {
        tokio::select! {
            // Cancellation requested: report progress and exit cleanly.
            _ = token.cancelled() => {
                println!("worker: shutting down after {jobs} jobs");
                return;
            }
            // Otherwise, do one unit of work.
            _ = sleep(Duration::from_millis(30)) => {
                jobs += 1;
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let token = CancellationToken::new();
    let handle = tokio::spawn(worker(token.clone()));

    sleep(Duration::from_millis(100)).await;
    println!("main: requesting shutdown");
    token.cancel();              // cooperative signal
    handle.await.unwrap();       // wait for clean teardown
    println!("main: worker stopped");
}
```

Real output (about three 30ms jobs fit into the 100ms window):

```
main: requesting shutdown
worker: shutting down after 3 jobs
main: worker stopped
```

The important detail is the `select!` between `token.cancelled()` and the work branch: the worker chooses *when* to stop (after the current job), so cleanup is graceful, not forced. This is the production pattern for shutting down servers on `SIGTERM`/Ctrl+C; you would replace the timer in `main` with `tokio::signal::ctrl_c().await`. See [Concurrent Awaiting](/11-async/07-select-join/) and the Tokio graceful-shutdown guide linked above.

</details>
