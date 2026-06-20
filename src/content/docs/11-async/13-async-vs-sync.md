---
title: "Async vs Sync: Choosing the Right Concurrency Model"
description: "Node makes everything async; Rust lets you choose. Match the workload: async Tokio tasks for I/O, threads or rayon for CPU, plain sync when nothing overlaps."
---

## Quick Overview

In Node.js you rarely choose: almost everything is `async`, because the single-threaded event loop punishes any code that blocks it. Rust gives you a genuine menu: **async tasks** for I/O-bound concurrency, **OS threads** (or [`rayon`](https://docs.rs/rayon)) for CPU-bound parallelism, and **plain synchronous code** when there is nothing to overlap. This page is about making that choice deliberately, the I/O-bound-versus-CPU-bound distinction that drives it, and the "function coloring" problem that `async` introduces in both languages.

> **Note:** This page assumes you have met Rust's lazy futures and the Tokio runtime. If not, read [Promises vs Futures](/11-async/00-promises-vs-futures/) and [The Tokio Runtime](/11-async/02-tokio-intro/) first. The mechanics of `tokio::spawn`, `spawn_blocking`, and OS threads live in [Spawning Tasks](/11-async/09-spawning-tasks/); this page is about *when* to reach for each.

---

## TypeScript/JavaScript Example

A typical Node.js service mixes two kinds of work: waiting on the network or disk (**I/O-bound**) and crunching numbers (**CPU-bound**). The event loop handles the first beautifully and the second terribly:

```typescript
import { createHash } from "node:crypto";

// I/O-bound: mostly waiting. async/await + the event loop excels here.
async function fetchUser(id: number): Promise<{ id: number; name: string }> {
  const res = await fetch(`https://api.example.com/users/${id}`);
  return res.json();
}

// I/O-bound work overlaps perfectly: three "requests" take as long as one.
async function loadUsers(): Promise<unknown[]> {
  return Promise.all([fetchUser(1), fetchUser(2), fetchUser(3)]);
}

// CPU-bound: a synchronous hash loop. This BLOCKS the single event-loop thread.
function hashPasswords(passwords: string[]): string[] {
  // While this runs, NO other callback, timer, or awaited promise can proceed.
  return passwords.map((p) => {
    let h = createHash("sha256");
    for (let i = 0; i < 200_000; i++) h.update(p); // deliberately heavy
    return h.digest("hex");
  });
}
```

The hidden trap: because Node runs your JavaScript on one thread, `hashPasswords` freezes the entire process: every pending request, timer, and `await` stalls until it returns. We can demonstrate the freeze precisely with a busy loop and a timer that should fire at 50 ms:

```javascript
// coloring.mjs
const start = Date.now();
setTimeout(() => {
  console.log(`timer fired at ${Date.now() - start} ms (wanted 50)`);
}, 50);

// Busy-loop the single thread for ~300 ms.
const spinUntil = start + 300;
while (Date.now() < spinUntil) {
  /* burn CPU */
}
console.log(`busy loop done at ${Date.now() - start} ms`);
```

Running it under Node v22:

```
busy loop done at 309 ms
timer fired at 316 ms (wanted 50)
```

The timer was due at 50 ms but did not fire until 316 ms; the CPU loop held the thread hostage. Node's only escape hatch for CPU work is `worker_threads`, a separate, heavier mechanism with message-passing serialization. Rust faces the *same* physics but hands you cleaner, first-class tools for both halves of the problem.

---

## Rust Equivalent

Rust makes you pick the tool that matches the workload. For **I/O-bound** work, async tasks on a runtime overlap waits exactly like the event loop: three 100 ms "requests" finish in about 100 ms total. This example uses two crates, the `tokio` runtime and `futures` (for `join_all`), so add them first with `cargo add tokio --features full` and `cargo add futures`:

```rust playground
use std::time::{Duration, Instant};
use tokio::time::sleep;

/// I/O-bound: mostly waiting. async + a runtime is the right tool.
async fn fetch(url: &str) -> usize {
    sleep(Duration::from_millis(100)).await; // stands in for a network round-trip
    url.len()
}

#[tokio::main]
async fn main() {
    let urls = ["https://a.example", "https://b.example", "https://c.example"];
    let start = Instant::now();

    // Run all three "requests" concurrently. They overlap because each .await
    // yields the worker while it waits — so total time ~= one request, not three.
    let results = futures::future::join_all(urls.iter().map(|u| fetch(u))).await;

    println!("results = {results:?}");
    println!("elapsed = {} ms", start.elapsed().as_millis());
}
```

Real output:

```
results = [17, 17, 17]
elapsed = 101 ms
```

For **CPU-bound** work, async does *nothing*: there is no waiting to overlap, only computation to spread across cores. That is a job for threads. The data-parallel crate [`rayon`](https://docs.rs/rayon) turns a sequential iterator into a parallel one with a one-word change, and on a multi-core machine the speedup is real:

```rust playground
use std::time::Instant;
use rayon::prelude::*;

/// CPU-bound: a deliberately heavy, purely synchronous computation.
fn heavy(seed: u64) -> u64 {
    let mut acc = seed;
    for _ in 0..50_000_000u64 {
        acc = acc.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    }
    acc
}

fn main() {
    let inputs: Vec<u64> = (0..8).collect();

    // Sequential baseline.
    let start = Instant::now();
    let seq: Vec<u64> = inputs.iter().map(|&s| heavy(s)).collect();
    println!("sequential: {} ms", start.elapsed().as_millis());

    // Rayon: a parallel iterator spreads the work across CPU cores.
    let start = Instant::now();
    let par: Vec<u64> = inputs.par_iter().map(|&s| heavy(s)).collect();
    println!("rayon par:  {} ms", start.elapsed().as_millis());

    assert_eq!(seq, par);
    println!("results match: {}", seq == par);
}
```

Real output (on an 8-core machine, `--release`):

```
sequential: 830 ms
rayon par:  83 ms
results match: true
```

That is roughly a 10× speedup, and notice there is no `async`, no `.await`, and no Tokio anywhere. CPU-bound work wants cores, not an event loop. The art is knowing which world you are in.

> **Tip:** `rayon` needs no runtime and no `Cargo.toml` features beyond `rayon = "1"`. It manages its own thread pool sized to your CPU. Tokio is for *concurrency over I/O*; rayon is for *parallelism over data*. They compose; see the Real-World Example.

---

## Detailed Explanation

### Two axes: concurrency vs parallelism, I/O-bound vs CPU-bound

Two independent distinctions drive every decision here.

**Concurrency vs parallelism.** *Concurrency* is dealing with many things at once by interleaving them (one cook juggling several pans). *Parallelism* is doing many things at literally the same instant (several cooks). Async gives you concurrency cheaply; whether it also gives you parallelism depends on the runtime's scheduler (the multi-thread vs current-thread choice in [The Tokio Runtime](/11-async/02-tokio-intro/)). JavaScript gives you concurrency but *never* parallelism for your own JS code: one thread, always. The deeper treatment is in [Concurrency vs Parallelism](/11-async/10-concurrency/); here we care only about its consequence for *choosing a tool*.

**I/O-bound vs CPU-bound.** This is the question to ask first about any task:

- **I/O-bound**: the task spends most of its time *waiting*, on a socket, a disk, a database, a timer. The CPU is idle during the wait. Overlapping the waits is the whole win.
- **CPU-bound**: the task spends most of its time *computing*: hashing, parsing, compressing, rendering, number-crunching. There is nothing to overlap; you only win by using more cores.

Async is built to overlap waits. It does not — cannot — make computation faster. Pointing async at a CPU-bound problem is like hiring a faster waiter to cook the food.

### Why async wins for I/O and loses for CPU

When an async task hits `.await` on something that is not ready (a socket with no data yet), it *yields* the worker thread back to the runtime, which runs other tasks meanwhile. Thousands of mostly-idle connections can therefore share a handful of threads. That is exactly the I/O-bound sweet spot.

A CPU-bound loop never hits an `.await`; it just computes. So it never yields. On the multi-thread runtime it merely pins one worker (wasting the runtime's lightweight scheduling); on the single-thread runtime it freezes everything, just like Node. The yield points that make async efficient simply do not exist in a tight compute loop.

### The runtime starvation trap (the Rust mirror of blocking the event loop)

This is the single most important practical consequence. A blocking or CPU-heavy synchronous call inside an async task starves its sibling tasks, because cooperative scheduling only hands off control at `.await`. On a single-thread runtime it is dramatic and deterministic:

```rust playground
use std::time::Duration;
use tokio::time::{sleep, Instant};

// A blocking wait that NEVER yields the async worker: it sleeps the OS thread.
// (A long CPU loop would behave the same way — neither hits an .await.)
fn blocking_work() {
    std::thread::sleep(Duration::from_millis(300));
}

// Single-thread runtime so the starvation is deterministic and easy to see.
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let start = Instant::now();

    // A "heartbeat" task that SHOULD tick every 50 ms.
    let heartbeat = tokio::spawn(async move {
        for n in 1..=3 {
            sleep(Duration::from_millis(50)).await;
            println!("heartbeat {n} at {} ms", start.elapsed().as_millis());
        }
    });

    // This blocking call hogs the single worker thread for 300 ms. The heartbeat
    // cannot run until this returns — its timers all fire late, bunched up.
    blocking_work();
    println!("blocking work done at {} ms", start.elapsed().as_millis());

    heartbeat.await.unwrap();
}
```

Real output:

```
blocking work done at 305 ms
heartbeat 1 at 356 ms
heartbeat 2 at 409 ms
heartbeat 3 at 461 ms
```

The heartbeat was supposed to tick at 50, 100, and 150 ms. Instead it does not fire at all until 356 ms, fully blocked until the 300 ms call returns, then it catches up. This is byte-for-byte the same failure as the Node busy-loop above; cooperative scheduling has the same Achilles' heel everywhere.

The fix is to move the blocking work off the async workers with `tokio::task::spawn_blocking`, which runs it on a dedicated blocking-thread pool:

```rust playground
use std::time::Duration;
use tokio::time::{sleep, Instant};

fn blocking_work() {
    std::thread::sleep(Duration::from_millis(300));
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let start = Instant::now();

    let heartbeat = tokio::spawn(async move {
        for n in 1..=3 {
            sleep(Duration::from_millis(50)).await;
            println!("heartbeat {n} at {} ms", start.elapsed().as_millis());
        }
    });

    // spawn_blocking moves the blocking call to a dedicated thread pool, so the
    // async worker stays free and the heartbeat ticks on time.
    let work = tokio::task::spawn_blocking(blocking_work);

    work.await.unwrap();
    println!("blocking work done at {} ms", start.elapsed().as_millis());

    heartbeat.await.unwrap();
}
```

Real output:

```
heartbeat 1 at 54 ms
heartbeat 2 at 108 ms
heartbeat 3 at 161 ms
blocking work done at 302 ms
```

Now the heartbeat ticks on time (54 / 108 / 161 ms) while the blocking work runs concurrently on its own thread. `spawn_blocking` is the moral equivalent of Node's `worker_threads`, but far lighter to use. The full mechanics are in [Spawning Tasks](/11-async/09-spawning-tasks/).

### When you do not need async at all

A point that surprises Node developers: lots of excellent Rust programs use *no* async whatsoever. A CLI that reads a file, transforms it, and writes it out has nothing to overlap: synchronous `std::fs` is simpler, faster to compile, and easier to reason about. A CPU-bound batch job wants threads, not a runtime. Reaching for `#[tokio::main]` reflexively (because that is what Node trained you to do) often adds a dependency and a layer of complexity you will never use.

For CPU-bound parallelism with no I/O, plain OS threads need no runtime at all:

```rust playground
use std::thread;
use std::time::Instant;

/// CPU-bound work: count primes below n with a naive trial-division loop.
fn count_primes(n: u64) -> u64 {
    (2..n).filter(|&x| (2..x).all(|d| x % d != 0)).count() as u64
}

fn main() {
    let ranges = [50_000u64, 50_000, 50_000, 50_000];
    let start = Instant::now();

    // Plain OS threads: no async, no runtime. Each thread runs on its own core.
    let handles: Vec<_> = ranges
        .into_iter()
        .map(|n| thread::spawn(move || count_primes(n)))
        .collect();

    let total: u64 = handles.into_iter().map(|h| h.join().unwrap()).sum();

    println!("total primes = {total}");
    println!("elapsed = {} ms", start.elapsed().as_millis());
}
```

Real output (`--release`):

```
total primes = 20532
elapsed = 682 ms
```

No `async`, no Tokio, no `.await`, just threads doing CPU work in parallel. For real data-parallel pipelines, prefer `rayon` (it handles work-stealing and pool sizing for you); use raw `std::thread` for a handful of long-lived, distinct jobs.

### Function coloring: the cost async imposes

There is a famous essay, *"What Color is Your Function?"*, describing how `async` splits a language's functions into two **colors**: async functions and sync functions. The rules are asymmetric and infectious:

1. An async function can call a sync function freely.
2. A sync function *cannot* simply call an async function and get its value; it must drive the future through a runtime.
3. Calling an async function gives you a future; you must `.await` it (only legal inside another async function), so async-ness propagates up the call stack.

JavaScript has exactly this problem: `await` is only legal inside `async function`, so one `await` deep in your code tends to turn every caller `async`. Rust has it too, but with a sharper edge: the boundary is enforced by the type system, and a bare future does nothing until polled.

In Rust, calling `.await` outside an async context is a hard compile error:

```rust
async fn fetch_count() -> u32 {
    42
}

// A plain synchronous function trying to call an async one.
fn summarize() -> u32 {
    // does not compile (error[E0728]): `.await` is only allowed inside
    // async fn / async block.
    let count = fetch_count().await;
    count * 2
}

fn main() {
    println!("{}", summarize());
}
```

Real compiler output:

```
error[E0728]: `await` is only allowed inside `async` functions and blocks
 --> src/main.rs:8:31
  |
6 | fn summarize() -> u32 {
  | --------------------- this is not `async`
7 |     // does not compile (error[E0728]): `.await` is only allowed inside
8 |     let count = fetch_count().await;
  |                               ^^^^^ only allowed inside `async` functions and blocks
```

The error names the cure: make `summarize` async too (and the coloring spreads), or bridge into the async world explicitly. Bridging from a synchronous function uses a runtime's `block_on`, which runs a future to completion on the current thread and returns its value:

```rust playground
use std::time::Duration;
use tokio::runtime::Runtime;
use tokio::time::sleep;

async fn fetch_count() -> u32 {
    sleep(Duration::from_millis(10)).await;
    42
}

// A plain synchronous main — no #[tokio::main]. We build a runtime by hand and
// use block_on as the bridge from the sync world into the async world.
fn main() {
    let rt = Runtime::new().expect("failed to build runtime");

    // block_on runs the future to completion on this thread and returns its value.
    let count = rt.block_on(fetch_count());

    println!("count = {count}");
}
```

Real output:

```
count = 42
```

`#[tokio::main]` is just sugar that builds a runtime and calls `block_on(main())` for you. Knowing the desugaring matters when you must call async code from a context you do not control — a `Drop` impl, a synchronous trait method, an FFI callback — where `block_on` is your bridge.

> **Warning:** Never call `block_on` (or any blocking call) from *inside* an async task; it blocks the worker and can deadlock the runtime. `block_on` is for crossing *into* async from genuinely synchronous code, not for nesting. If you are already async and just need to wait, use `.await`.

---

## Key Differences

| Question | JavaScript / Node.js | Rust |
| --- | --- | --- |
| Default model | Async everything (one event loop) | You choose: sync, threads, or async |
| I/O-bound concurrency | `async`/`await` on the event loop | async tasks on a runtime (Tokio) |
| CPU-bound parallelism | `worker_threads` (heavy, serialized messages) | OS threads / `rayon` (shared memory, cheap) |
| "No concurrency needed" | Still usually async out of habit | Plain synchronous code; no runtime |
| Blocking the worker | Freezes the whole event loop | Freezes the runtime's worker(s); use `spawn_blocking` |
| Offloading CPU work | `worker_threads` | `tokio::task::spawn_blocking`, threads, or `rayon` |
| Function coloring | Yes (`await` only in `async`) | Yes, type-enforced; futures are lazy |
| Sync→async bridge | Top-level `await` / an async IIFE | `Runtime::block_on` / `#[tokio::main]` |
| Cost of choosing wrong | Event-loop stalls; jank | Same stall, plus you may have pulled in a runtime you never needed |

The mental shift for a TypeScript developer is this: **async is not the default in Rust, it is a tool for I/O concurrency.** In Node you make everything async because the platform gives you no real alternative. In Rust, slapping `async` on a CPU-bound or do-one-thing program is often a mistake: it adds a runtime and the `Send + 'static` constraints of [Spawning Tasks](/11-async/09-spawning-tasks/) without buying you anything.

> **Note:** A handy decision rule. *Are you mostly waiting on many things at once?* → async (Tokio). *Are you mostly computing, and want more cores?* → threads / `rayon`. *Just doing one thing, or computing in sequence?* → plain synchronous code. *A bit of blocking inside an otherwise-async program?* → `spawn_blocking`.

---

## Common Pitfalls

### Pitfall 1: Using async for CPU-bound work and expecting a speedup

The most common reflex from Node: wrapping a heavy computation in `async fn` and `tokio::spawn`, expecting it to "run in the background faster." It does not. Async adds yield points for *waiting*; a compute loop has none, so it just pins a worker. Worse, on a single-thread runtime it starves everything (shown above). Async never makes computation faster; only more cores do.

**Fix:** for CPU-bound work use `rayon` (data parallelism) or `std::thread` / `spawn_blocking` (to offload from the async workers). Reserve async for I/O.

### Pitfall 2: Calling a blocking API inside an async task

This compiles and runs, so the compiler will not save you, which makes it especially dangerous:

```rust
// Anti-pattern (compiles, but misbehaves):
// std::thread::sleep, std::fs, reqwest::blocking, a synchronous DB driver, or a
// long CPU loop inside an async task all block the worker thread — no yield.
tokio::spawn(async {
    std::thread::sleep(std::time::Duration::from_secs(5)); // blocks the worker!
    // Every other task on this worker stalls for 5 seconds.
});
```

**Fix:** use the async-aware equivalent (`tokio::time::sleep(...).await`, `tokio::fs`, an async DB driver like `sqlx`), or offload the genuinely-blocking call with `tokio::task::spawn_blocking`. The earlier heartbeat experiment shows both the failure and the fix. Tokio can detect *some* long stalls and log a warning, but it cannot fix them for you.

### Pitfall 3: Calling `block_on` from inside the runtime

Bridging is one-directional. `block_on` enters the async world from sync code; calling it while you are *already* on a runtime thread blocks that worker and can panic or deadlock:

```rust
// Anti-pattern: block_on inside an async context.
#[tokio::main]
async fn main() {
    let rt = tokio::runtime::Handle::current();
    // Calling block_on on the current runtime from within it panics:
    // "Cannot start a runtime from within a runtime."
    rt.block_on(async { 1 + 1 }); // panics at runtime
}
```

Running it produces a real panic (`Cannot start a runtime from within a runtime. This happens because a function (like 'block_on') attempted to block the current thread while the thread is being used to drive asynchronous tasks.`).

**Fix:** if you are already async, just `.await`. Use `block_on` only from genuinely synchronous entry points.

### Pitfall 4: Adding `#[tokio::main]` to a program with no I/O concurrency

A CLI that processes one file, or a batch job that crunches numbers, gains nothing from a runtime, but pays for it in a dependency, slower compiles, and the `Send + 'static` rules that async-ness forces on spawned work. New Rustaceans coming from Node often async-ify everything by habit.

**Fix:** start synchronous. Add Tokio only when you have *concurrent I/O* to overlap. For CPU parallelism, reach for `rayon`, which needs no runtime at all.

### Pitfall 5: Forgetting `.await` and getting a `Future` instead of a value

A coloring side effect: a bare async call returns a lazy future, so forgetting `.await` is a type error, not a silent no-op (unlike JS, where a forgotten `await` gives you a `Promise` that may still run):

```rust
async fn fetch_count() -> u32 {
    42
}

#[tokio::main]
async fn main() {
    // does not compile (error[E0308]): forgot `.await`, so this is a Future.
    let count: u32 = fetch_count();
    println!("{count}");
}
```

Real compiler output (trimmed):

```
error[E0308]: mismatched types
 --> src/main.rs:8:22
  |
8 |     let count: u32 = fetch_count();
  |                ---   ^^^^^^^^^^^^^ expected `u32`, found future
  |                |
  |                expected due to this
  |
note: calling an async function returns a future
help: consider `await`ing on the `Future`
  |
8 |     let count: u32 = fetch_count().await;
  |                                   ++++++
```

**Fix:** add `.await`. The compiler even suggests it. Because Rust futures are *lazy* (see [Promises vs Futures](/11-async/00-promises-vs-futures/)), a forgotten `.await` means the work never even starts, but the type system catches it long before that becomes a runtime mystery.

---

## Best Practices

### Classify the workload before choosing a tool

Ask "I/O-bound or CPU-bound?" first, every time. I/O-bound and many-at-once → async. CPU-bound → threads / `rayon`. One sequential thing → plain sync. This single question prevents the majority of mismatched-tool mistakes.

### Keep async functions free of blocking and heavy CPU work

Treat an async task like the Node event loop: anything that does not yield is a liability. Use async-aware I/O (`tokio::fs`, `tokio::net`, `sqlx`, `reqwest` non-blocking), and push blocking calls and CPU loops to `spawn_blocking` or a thread pool. A good rule: every code path in an `async fn` should reach an `.await` "soon."

### Use `rayon` for data parallelism, Tokio for I/O concurrency — and compose them

These are not competitors. A server can accept connections with Tokio (I/O concurrency) and, inside a `spawn_blocking` closure, use `rayon` to parallelize a CPU-heavy transform across cores. Keep the two pools distinct: Tokio's workers for I/O, the blocking/`rayon` pool for computation.

### Do not reach for a runtime you do not need

Synchronous Rust is a feature, not a limitation. Libraries especially should think hard before becoming async-only; it colors every caller. Where practical, expose a sync core and let callers choose; or offer both, gated behind a [feature flag](/12-modules-packages/09-feature-flags/).

### Bridge sync↔async at the edges, deliberately

Use `block_on` (or `#[tokio::main]`) at the boundary where synchronous code must enter async: `main`, a sync trait impl, an FFI callback. Never nest `block_on` inside async. Within async, propagate with `.await` and `?` (see [Async/Await Syntax](/11-async/01-async-await/)).

### Right-size the parallelism to the cores

Spawning a million async tasks for I/O is fine; they are cheap and mostly idle. Spawning a million *threads* for CPU work is not; it thrashes the scheduler and exhausts memory on stacks. For CPU work, parallelism should track core count, which is exactly what `rayon`'s pool does by default.

---

## Real-World Example

A production-flavored pipeline that mixes both worlds: a batch image service that **downloads** images (I/O-bound → async, overlapped) and then **processes** each one (CPU-bound → offloaded with `spawn_blocking` so it never stalls the downloads). This is the canonical "concurrent I/O feeding parallel compute" shape.

```rust playground
use std::time::{Duration, Instant};
use tokio::task::JoinSet;
use tokio::time::sleep;

/// I/O-bound: download an image. Mostly waiting on the network → async.
async fn download(id: u32) -> Vec<u8> {
    sleep(Duration::from_millis(80)).await; // network round-trip
    vec![(id % 256) as u8; 1_000_000]       // pretend this is a 1 MB image
}

/// CPU-bound: a synchronous transform (resize + checksum). No .await here.
fn process(bytes: &[u8]) -> u64 {
    // Stand-in for real image work: a heavy fold over every byte.
    bytes
        .iter()
        .fold(0u64, |acc, &b| acc.wrapping_mul(1099511628211).wrapping_add(b as u64))
}

#[tokio::main]
async fn main() {
    let start = Instant::now();
    let mut set = JoinSet::new();

    for id in 0..4u32 {
        set.spawn(async move {
            // 1. Await the I/O concurrently with the other tasks.
            let bytes = download(id).await;

            // 2. Offload the CPU-bound transform to the blocking pool so it does
            //    not stall the async workers driving the other downloads.
            let checksum = tokio::task::spawn_blocking(move || process(&bytes))
                .await
                .expect("processing task panicked");

            (id, checksum)
        });
    }

    let mut results = Vec::new();
    while let Some(joined) = set.join_next().await {
        results.push(joined.expect("worker panicked"));
    }
    results.sort();

    for (id, checksum) in results {
        println!("image {id}: checksum {checksum}");
    }
    println!("elapsed = {} ms", start.elapsed().as_millis());
}
```

Real output (`--release`):

```
image 0: checksum 0
image 1: checksum 15279771427360356480
image 2: checksum 12112798781011161344
image 3: checksum 8945826134661966208
elapsed = 91 ms
```

Four 80 ms downloads overlap (so they cost ~80 ms together, not 320 ms), and each CPU-bound `process` runs on the blocking pool without freezing the async workers: total ~91 ms. The same pipeline written as "async everything" would block a worker during each `process`; written as "threads everything" it would waste threads sitting idle during each download. Matching the tool to the workload-half is the whole point.

> **Note:** In production you would download with [`reqwest`](https://docs.rs/reqwest), and if `process` were itself data-parallel you could use `rayon` *inside* the `spawn_blocking` closure. Sharing state across the tasks (a counter, a cache) uses the `Arc<Mutex<_>>` pattern in [The `Arc<Mutex<T>>` Pattern](/11-async/12-arc-mutex-pattern/). Error handling with `?` across async boundaries is covered in [Async/Await Syntax](/11-async/01-async-await/).

---

## Further Reading

- [Tokio Tutorial — Bridging with sync code](https://tokio.rs/tokio/topics/bridging) — `block_on`, `spawn_blocking`, and mixing sync/async.
- [Tokio — CPU-bound tasks and blocking code](https://docs.rs/tokio/latest/tokio/index.html#cpu-bound-tasks-and-blocking-code) — the official guidance on what *not* to run on the runtime.
- [The `rayon` crate](https://docs.rs/rayon) — data parallelism with parallel iterators, the go-to for CPU-bound work.
- ["What Color is Your Function?"](https://journal.stuffwithstuff.com/2015/02/01/what-color-is-your-function/) — the essay that named the function-coloring problem.
- [`std::thread` documentation](https://doc.rust-lang.org/std/thread/) — OS threads, the right tool when async is overkill.
- [`tokio::runtime::Runtime::block_on`](https://docs.rs/tokio/latest/tokio/runtime/struct.Runtime.html#method.block_on) — the sync→async bridge.

Related sections of this guide:

- [Promises vs Futures](/11-async/00-promises-vs-futures/) — why Rust futures are lazy and need a runtime.
- [The Tokio Runtime](/11-async/02-tokio-intro/) — single-thread vs multi-thread schedulers (concurrency vs parallelism in the runtime).
- [Async/Await Syntax](/11-async/01-async-await/) — `async`/`await` syntax and `?` inside async.
- [Spawning Tasks](/11-async/09-spawning-tasks/) — `tokio::spawn`, `spawn_blocking`, and tasks-vs-threads mechanics.
- [Concurrency vs Parallelism](/11-async/10-concurrency/) — concurrency vs parallelism in depth, structured patterns, cancellation.
- [The `Arc<Mutex<T>>` Pattern](/11-async/12-arc-mutex-pattern/) — sharing state across tasks and threads.
- [Async Synchronization Primitives](/11-async/11-sync-primitives/) — std vs Tokio locks, and holding locks across `.await`.
- [Understanding Cargo](/01-getting-started/03-cargo-basics/) — `cargo add` and dependencies.
- [Basics](/02-basics/) — Rust fundamentals refresher.
- [Feature Flags and Conditional Compilation](/12-modules-packages/09-feature-flags/) — exposing sync and async APIs behind features.
- Next section: [Modules & Packages](/12-modules-packages/) — organizing crates and modules.

---

## Exercises

### Exercise 1: Recognize and parallelize CPU-bound work

**Difficulty:** Easy

**Objective:** Identify a workload as CPU-bound and reach for data parallelism instead of async.

**Instructions:**

1. Write a synchronous `fn collatz_steps(n: u64) -> u64` that counts how many steps the Collatz sequence takes to reach 1 (even → `n/2`, odd → `3n+1`).
2. Over the range `1..100_000`, find the number with the longest chain.
3. Use `rayon`'s parallel iterator (`into_par_iter`) — *not* async — to spread the work across cores. Print the winning number and its step count.
4. In a comment, state why async would not help here.

<details>
<summary>Solution</summary>

```rust playground
use rayon::prelude::*;

/// CPU-bound: count Collatz steps to reach 1. Pure computation, no waiting.
fn collatz_steps(mut n: u64) -> u64 {
    let mut steps = 0;
    while n != 1 {
        n = if n % 2 == 0 { n / 2 } else { 3 * n + 1 };
        steps += 1;
    }
    steps
}

fn main() {
    // CPU-bound: there is no I/O to overlap, so async buys nothing. Only more
    // cores help — that is exactly what rayon's parallel iterator gives us.
    let (best_n, best_steps) = (1..100_000u64)
        .into_par_iter()
        .map(|n| (n, collatz_steps(n)))
        .max_by_key(|&(_, steps)| steps)
        .unwrap();

    println!("n = {best_n} has {best_steps} steps");
}
```

Output (`--release`):

```
n = 77031 has 350 steps
```

The computation is pure CPU work with no waiting, so `async`/Tokio would add overhead without speeding anything up. `rayon` parallelizes across cores with a one-word change from `into_iter` to `into_par_iter`.

</details>

### Exercise 2: Keep the runtime responsive by offloading a blocking call

**Difficulty:** Medium

**Objective:** Fix a blocking call inside an async program so other tasks stay responsive.

**Instructions:**

1. Write a synchronous `fn slow_hash(password: &str) -> u64` that calls `std::thread::sleep` for 150 ms (standing in for a deliberately slow password hash) and then folds the bytes into a `u64`.
2. In a `current_thread` Tokio runtime, spawn a "heartbeat" task that prints twice, 50 ms apart.
3. Compute the hash *without* starving the heartbeat — offload it with `spawn_blocking` — then await both.
4. Verify from the timing that the heartbeat ticked on schedule.

<details>
<summary>Solution</summary>

```rust playground
use std::time::Duration;
use tokio::time::{sleep, Instant};

/// A synchronous, blocking "hash" of a password (stands in for bcrypt/argon2).
fn slow_hash(password: &str) -> u64 {
    std::thread::sleep(Duration::from_millis(150)); // CPU-bound + blocking
    password.bytes().fold(0u64, |a, b| a.wrapping_mul(31).wrapping_add(b as u64))
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let start = Instant::now();

    // A heartbeat proving the runtime stays responsive.
    let heartbeat = tokio::spawn(async move {
        for n in 1..=2 {
            sleep(Duration::from_millis(50)).await;
            println!("heartbeat {n} at {} ms", start.elapsed().as_millis());
        }
    });

    let password = String::from("hunter2");
    // Offload the blocking hash so it does not stall the single async worker.
    let hash = tokio::task::spawn_blocking(move || slow_hash(&password))
        .await
        .expect("hashing task panicked");

    println!("hash = {hash} at {} ms", start.elapsed().as_millis());
    heartbeat.await.unwrap();
}
```

Output:

```
heartbeat 1 at 52 ms
heartbeat 2 at 104 ms
hash = 95755137202 at 152 ms
```

The heartbeat ticks at 52 and 104 ms — right on time — because `spawn_blocking` moved the 150 ms blocking call to a separate thread pool. Calling `slow_hash(...)` directly in `main` (without `spawn_blocking`) would have frozen the single worker and pushed the first heartbeat past 150 ms.

</details>

### Exercise 3: A mixed pipeline — concurrent I/O feeding parallel compute

**Difficulty:** Medium–Hard

**Objective:** Combine async I/O concurrency with `rayon` CPU parallelism in one program, putting each tool where it belongs.

**Instructions:**

1. Write `async fn fetch_shard(id: u64) -> Vec<u64>` that sleeps 50 ms (I/O) then returns 250,000 numbers.
2. Write a synchronous `fn sum_of_squares(data: &[u64]) -> u64` that uses `rayon`'s `par_iter` to sum the squares (CPU-bound).
3. In `async main`, fetch four shards *concurrently*, flatten them, then offload the CPU-bound reduction to the blocking pool (where `rayon` parallelizes it). Print the total and elapsed time.
4. The downloads should overlap (≈50 ms, not 200 ms) and the reduction should not stall the runtime.

<details>
<summary>Solution</summary>

```rust playground
use std::time::{Duration, Instant};
use rayon::prelude::*;
use tokio::time::sleep;

/// I/O-bound: fetch a "shard" of numbers (async).
async fn fetch_shard(id: u64) -> Vec<u64> {
    sleep(Duration::from_millis(50)).await; // network wait
    (0..250_000).map(|x| x + id * 250_000).collect()
}

/// CPU-bound: sum the squares of a slice (synchronous, parallelizable).
fn sum_of_squares(data: &[u64]) -> u64 {
    data.par_iter().map(|&x| x.wrapping_mul(x)).sum()
}

#[tokio::main]
async fn main() {
    let start = Instant::now();

    // 1. Fetch four shards concurrently (I/O overlaps → ~50 ms, not 200).
    let shards = futures::future::join_all((0..4u64).map(fetch_shard)).await;

    // 2. Flatten, then offload the CPU-bound reduction to the blocking pool,
    //    where rayon spreads it across cores.
    let all: Vec<u64> = shards.into_iter().flatten().collect();
    let total = tokio::task::spawn_blocking(move || sum_of_squares(&all))
        .await
        .expect("compute task panicked");

    println!("sum of squares = {total}");
    println!("elapsed = {} ms", start.elapsed().as_millis());
}
```

Output (`--release`):

```
sum of squares = 333332833333500000
elapsed = 62 ms
```

The four 50 ms fetches overlap via `join_all` (I/O concurrency, ~50 ms total), and the CPU-bound reduction runs on the blocking pool with `rayon` spreading it across cores, so the async runtime never stalls. This is the production shape: Tokio for the waiting, `rayon`/threads for the computing, `spawn_blocking` as the seam between them. `join_all` comes from the `futures` crate (`futures = "0.3"`); for a fixed, small set you could equally use [`tokio::join!`](/11-async/07-select-join/).

</details>
