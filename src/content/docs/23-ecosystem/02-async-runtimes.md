---
title: "Async Runtimes: Tokio, async-std, and smol"
description: "Rust has no built-in event loop like Node. Compare Tokio, async-std, and smol, why async fns are lazy, and why Tokio became the ecosystem default."
---

## Quick Overview

In Node.js the async runtime is the platform itself: V8 plus libuv give you an event loop you never install or start. In Rust, `async`/`await` is **only language syntax**: the compiler turns an `async fn` into a state machine that does nothing until some **executor** polls it. You choose and bring that executor as a crate. This page surveys the three runtimes a Node developer will encounter (**Tokio**, **async-std**, and **smol**) and explains why Tokio has become the default that nearly the entire ecosystem builds on.

> **Note:** This page is the ecosystem-level "which runtime and why" overview. The mechanics of starting Tokio and the eager-vs-lazy mental model live in section 11: [The Tokio Runtime](/11-async/02-tokio-intro/), [Tokio Setup](/11-async/03-tokio-setup/), and [Promises vs Futures](/11-async/00-promises-vs-futures/).

---

## TypeScript/JavaScript Example

A Node developer never thinks about "which runtime." There is one event loop, it is always running, and `async`/`await` plus `Promise.all` just work. You install libraries (axios, pg, ioredis) and they all cooperate on that single shared loop without coordination:

```typescript
// node app.ts — the runtime is built in; nothing to choose or start.
import { setTimeout as sleep } from "node:timers/promises";

async function callService(name: string, ms: number): Promise<string> {
  await sleep(ms);
  return `${name} responded after ${ms}ms`;
}

async function main(): Promise<void> {
  const start = Date.now();

  // All three run concurrently on the one event-loop thread.
  const results = await Promise.all([
    callService("auth", 120),
    callService("inventory", 90),
    callService("pricing", 150),
  ]);

  for (const r of results) console.log(r);
  console.log(`all done in ${Date.now() - start}ms`); // ~150ms, not 360ms
}

main();
```

Three facts a Node developer takes for granted:

- There is exactly **one** runtime, and it ships with Node. You cannot pick a different event loop, and every library targets the same one.
- The loop is **always on**. Forgetting to `await main()` still "works" because the loop keeps draining its queue.
- Every async library — database drivers, HTTP clients, WebSocket servers — automatically interoperates because they all use the same loop.

In Rust, none of these are free. You opt into a runtime, you start it, and (historically) libraries had to agree on *which* runtime they target.

---

## Rust Equivalent

A future in Rust is inert. Calling an `async fn` returns a value that has done **no work**. Only an executor that repeatedly `poll`s it makes it progress. The most common executor is Tokio, started with the `#[tokio::main]` attribute:

```rust playground
// Cargo.toml: cargo add tokio --features full
use std::time::{Duration, Instant};
use tokio::time::sleep;

async fn call_service(name: &str, ms: u64) -> String {
    sleep(Duration::from_millis(ms)).await;
    format!("{name} responded after {ms}ms")
}

#[tokio::main] // expands to: fn main() { Runtime::new().block_on(async { ... }) }
async fn main() {
    let start = Instant::now();

    // Spawn three tasks onto the runtime; it drives them concurrently.
    let h1 = tokio::spawn(async { call_service("auth", 120).await });
    let h2 = tokio::spawn(async { call_service("inventory", 90).await });
    let h3 = tokio::spawn(async { call_service("pricing", 150).await });

    // JoinHandle::await yields Result<T, JoinError>.
    let results = [h1.await.unwrap(), h2.await.unwrap(), h3.await.unwrap()];
    for r in &results {
        println!("{r}");
    }
    println!("all done in {}ms", start.elapsed().as_millis());
}
```

Real output (Tokio 1.52.3, multi-thread runtime):

```text
auth responded after 120ms
inventory responded after 90ms
pricing responded after 150ms
all done in 152ms
```

The same program written against **smol** chooses a different executor and a different startup style. There is no `#[main]` macro, you call `block_on` directly:

```rust
// Cargo.toml: cargo add smol
use std::time::{Duration, Instant};

fn main() {
    // smol::block_on starts a tiny executor on the current thread.
    smol::block_on(async {
        let start = Instant::now();

        let a = smol::spawn(async {
            smol::Timer::after(Duration::from_millis(100)).await;
            "fast"
        });
        let b = smol::spawn(async {
            smol::Timer::after(Duration::from_millis(150)).await;
            "slow"
        });

        println!("{} then {}", a.await, b.await);
        println!("done in {}ms", start.elapsed().as_millis());
    });
}
```

Real output (smol 2.0.2):

```text
fast then slow
done in 152ms
```

The third historical option, **async-std**, mirrored the standard library's API surface. It still compiles, but its own README now says it is discontinued:

```rust
// Cargo.toml: cargo add async-std --features attributes
use std::time::{Duration, Instant};
use async_std::task;

#[async_std::main]
async fn main() {
    let start = Instant::now();
    let a = task::spawn(async {
        task::sleep(Duration::from_millis(100)).await;
        "fast"
    });
    let b = task::spawn(async {
        task::sleep(Duration::from_millis(150)).await;
        "slow"
    });
    println!("{} then {}", a.await, b.await);
    println!("done in {}ms", start.elapsed().as_millis());
}
```

Real output (async-std 1.13.2):

```text
fast then slow
done in 151ms
```

> **Warning:** As of 2025 the async-std project is **discontinued**. Its README states plainly: "`async-std` has been discontinued; use `smol` instead." Do not start new projects on it. It appears here only so you recognize it in older code and migration guides.

---

## Detailed Explanation

**Why does Rust need a runtime at all?** Because `async`/`await` is a zero-cost language feature that compiles to a state machine implementing the `Future` trait. `Future` has one method, `poll`, which the compiler-generated code uses to advance to the next `.await` point. Something has to call `poll` in a loop, wake the future when its I/O is ready, and schedule the thousands of small tasks that make up a real server. That "something" is the runtime. Rust deliberately left it out of the standard library so that embedded, kernel, browser/WASM, and server use cases could each pick an appropriate executor. But the cost is that **you must choose one**.

**What a runtime actually provides.** A production async runtime is more than a `poll` loop. Tokio bundles:

- A **scheduler** (multi-thread work-stealing, or single current-thread).
- A **reactor** (an epoll/kqueue/IOCP event loop, via the `mio` crate) that turns OS readiness notifications into task wakeups.
- A **timer wheel** for `sleep`, `timeout`, and intervals.
- Async-aware **synchronization** (`Mutex`, `RwLock`, `Semaphore`, channels) and **I/O types** (`TcpStream`, `tokio::fs::File`).

`smol` provides the same capabilities but assembled from small, independently usable crates (`async-io`, `async-executor`, `async-channel`, `futures-lite`). `async-std` provided a near drop-in mirror of `std`'s blocking API but in async form.

**The two Tokio flavors map cleanly onto the Node mental model.** A **current-thread** runtime is the closest analogue to Node's single-threaded event loop: one thread, concurrent but never parallel:

```rust playground
use std::time::Duration;
use tokio::time::sleep;

// The closest analogue to Node's single-threaded event loop.
#[tokio::main(flavor = "current_thread")]
async fn main() {
    let a = tokio::spawn(async {
        sleep(Duration::from_millis(50)).await;
        "a"
    });
    let b = tokio::spawn(async {
        sleep(Duration::from_millis(50)).await;
        "b"
    });
    // One thread, yet still concurrent: while one task awaits its timer,
    // the other makes progress.
    println!("{} {}", a.await.unwrap(), b.await.unwrap());
}
```

Real output:

```text
a b
```

The default `#[tokio::main]` uses the **multi-thread** flavor, which is something Node simply does not have for your JavaScript: a work-stealing scheduler that runs your tasks across all CPU cores in parallel. That is why CPU-bound async work can scale on Tokio in ways it cannot in a single Node process.

**You can build the runtime by hand** instead of using the macro. This is what you do for fine-grained control (worker count, naming threads) or when async is only part of a larger sync program:

```rust playground
use tokio::runtime::Builder;

fn main() {
    let rt = Builder::new_multi_thread()
        .worker_threads(4)
        .enable_all() // turn on the I/O reactor AND the time driver
        .build()
        .unwrap();

    let answer = rt.block_on(async {
        // spawn_blocking moves a synchronous/CPU-heavy computation onto a
        // dedicated blocking pool so it never stalls the async workers.
        tokio::task::spawn_blocking(|| (1..=1_000_000u64).sum::<u64>())
            .await
            .unwrap()
    });

    println!("sum = {answer}");
}
```

Real output:

```text
sum = 500000500000
```

`enable_all()` matters: the multi-thread *scheduler* alone does not include the I/O reactor or the timer. The `full` feature plus `enable_all` (or the `#[tokio::main]` macro, which does it for you) turns them on.

**Library code can — and should — stay runtime-agnostic.** A function that only uses `async`/`await` and `std` types names no runtime, so any executor can drive it:

```rust
use std::future::Future;

// Runtime-AGNOSTIC: never mentions Tokio or smol.
async fn add_async(a: u32, b: u32) -> u32 {
    a + b
}

fn doubled(x: u32) -> impl Future<Output = u32> {
    async move { x * 2 }
}

fn main() {
    // Drive it on the futures crate's minimal executor — no Tokio at all.
    let r1 = futures::executor::block_on(add_async(2, 3));
    println!("futures executor: {r1}");

    // Drive the SAME logic on a Tokio runtime.
    let rt = tokio::runtime::Runtime::new().unwrap();
    let r2 = rt.block_on(doubled(21));
    println!("tokio runtime:    {r2}");
}
```

Real output:

```text
futures executor: 5
tokio runtime:    42
```

The catch, and the entire reason Tokio dominates, is that the moment your code touches async *I/O* (`TcpStream`, timers, a database driver), it stops being agnostic and becomes tied to whatever runtime provides that I/O. Tokio's reactor is not interchangeable with smol's, so a `tokio::net::TcpStream` needs a Tokio runtime to be polled.

---

## Key Differences

| Aspect | Node.js | Tokio | smol | async-std |
| --- | --- | --- | --- | --- |
| How you get it | Built into the platform | `cargo add tokio` | `cargo add smol` | `cargo add async-std` (discontinued) |
| Started by | Always running | `#[tokio::main]` / `Runtime::block_on` | `smol::block_on` | `#[async_std::main]` |
| Threading model | Single-threaded loop | Multi-thread work-stealing (default) or current-thread | Multi-thread executor or current-thread | Multi-thread (thread-per-core-ish) |
| I/O backend | libuv | `mio` (epoll/kqueue/IOCP) | `polling` + `async-io` | `async-io` (same family as smol) |
| Ecosystem reach | n/a (one runtime) | Dominant — axum, hyper, tonic, sqlx, reqwest | Small but growing | Legacy only |
| Verified version (2026) | Node v22 | 1.52.3 | 2.0.2 | 1.13.2 (discontinued) |

**The decisive difference from Node is the missing universal loop.** In Node, axios and pg never have to agree on a runtime: there is only one. In Rust, an HTTP client built on Tokio's reactor and a database driver built on smol's reactor would each demand their own runtime, and gluing them together means running two runtimes or bridging with a shim like `async-compat`. **Tokio wins precisely because it solved this coordination problem by becoming the Schelling point**: when nearly every async library targets Tokio, picking Tokio makes everything interoperate the way Node libraries "just work."

**Why Tokio specifically?**

- **Network effect.** `hyper` (the HTTP backbone), `tower` (middleware), `tonic` (gRPC), `axum`, `sqlx`, `reqwest`, and the AWS SDK all build on Tokio. Choosing it means your dependencies already agree.
- **Maturity and funding.** Tokio is the oldest, most battle-tested runtime, with a work-stealing scheduler tuned for real server workloads, plus utilities like `tokio-console` for live task inspection.
- **One coherent toolbox.** Channels, sync primitives, timers, signals, and async file/network I/O all ship under one umbrella with consistent semantics.

smol's pitch is the opposite: a **small, modular, easy-to-read** core you can compose from `async-io`, `async-executor`, and friends. It is excellent for lightweight tools, learning, and cases where you want minimal dependencies. But for a typical server or anything pulling in mainstream crates, Tokio is the path of least resistance.

> **Tip:** "Which runtime?" usually answers itself: pick the runtime your biggest async dependency requires. In practice that is almost always Tokio. See [Web Frameworks](/23-ecosystem/01-web-frameworks/): Axum, Actix Web, Rocket, and Poem are all Tokio-based.

---

## Common Pitfalls

### Spawning or awaiting with no runtime running

A future does nothing on its own, and `tokio::spawn` needs a runtime context. Calling it from plain `fn main` panics at runtime (not a compile error):

```rust
fn main() {
    // Panics: there is no Tokio runtime running here.
    tokio::spawn(async {
        println!("never runs");
    });
}
```

Real panic message:

```text
thread 'main' panicked at src/main.rs:3:5:
there is no reactor running, must be called from the context of a Tokio 1.x runtime
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

The fix is to start a runtime first (`#[tokio::main]` or `Runtime::new()?.block_on(...)`). This trips up Node developers because in Node the loop is always there.

### Blocking the runtime thread

Calling `std::thread::sleep`, doing heavy CPU work, or making a synchronous (blocking) I/O call inside an async task ties up the OS thread instead of yielding it. On a current-thread runtime (the one closest to Node's model) that single thread is *the entire runtime*, so everything else stalls:

```rust playground
use std::time::{Duration, Instant};
use tokio::time::sleep;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let start = Instant::now();

    let blocker = tokio::spawn(async {
        // std::thread::sleep blocks the OS thread, not just this task.
        std::thread::sleep(Duration::from_millis(200));
        "blocking task done"
    });
    let other = tokio::spawn(async {
        sleep(Duration::from_millis(50)).await; // wants to finish in 50ms
        "timer task done"
    });

    println!("{}", other.await.unwrap());
    println!("{}", blocker.await.unwrap());
    println!("elapsed: {}ms", start.elapsed().as_millis());
}
```

Real output. Note the timer task that "should" take 50ms is held hostage until the blocking task releases the thread at ~200ms:

```text
timer task done
blocking task done
elapsed: 258ms
```

The fix is `tokio::task::spawn_blocking` for blocking/CPU work, or the async equivalent (`tokio::time::sleep` instead of `std::thread::sleep`). This is the Rust echo of "don't block the Node event loop," but Rust gives you a dedicated blocking pool to escape to.

### Mixing runtimes by accident

Pulling in a library that runs its own runtime, or calling a Tokio-only API from inside a smol executor, fails because reactors are not interchangeable. A Tokio `TcpStream` polled outside a Tokio context panics much like the spawn example above. The fix: standardize on one runtime project-wide, and reach for `async-compat` only when you genuinely must bridge a Tokio-only crate into a smol program.

### Assuming `async` means parallel (or even running)

`async fn` returns a lazy future. Unlike a JavaScript `Promise`, which starts executing the moment it is created, a Rust future makes **zero** progress until it is awaited or spawned. Forgetting to `.await` a future is a common bug; the compiler helpfully warns:

```text
warning: unused implementer of `Future` that must be used
note: futures do nothing unless you `.await` or poll them
```

See [Promises vs Futures](/11-async/00-promises-vs-futures/) for the full lazy-vs-eager comparison.

---

## Best Practices

- **Default to Tokio.** Unless you have a specific reason (minimal dependencies, an embedded-ish tool, a teaching example), use Tokio. The ecosystem assumes it, so you get the most interoperability for the least friction.
- **Enable only the features you need in libraries.** Applications can use `cargo add tokio --features full`. Library crates should request narrow features (`rt`, `macros`, `net`, `time`, `sync`) to keep downstream builds lean.
- **Keep CPU and blocking work off the async workers.** Use `tokio::task::spawn_blocking` for synchronous/CPU-heavy work, or `rayon` for data parallelism, so the scheduler's threads stay free to drive I/O. See [Useful Crates](/23-ecosystem/10-useful-crates/) for rayon.
- **Write runtime-agnostic logic where possible.** Pure-computation `async fn`s that don't touch I/O stay portable; push the runtime-specific I/O to the edges of your crate.
- **Don't pick a runtime per library — pick one per application.** Let your dependencies' runtime requirement decide, and standardize the whole project on it.
- **Reach for Tokio's built-in tools** (`tokio::sync::Semaphore`, `tokio::time::timeout`, `tokio::select!`) before hand-rolling concurrency primitives. See [Select & Join](/11-async/07-select-join/) and [Sync Primitives](/11-async/11-sync-primitives/).

> **Tip:** To inspect a running Tokio app's tasks live (stuck tasks, busy workers), add `console-subscriber` and run `tokio-console`. It is the async analogue of the Node `--inspect` profiler. More tooling in [Section 24: Tooling](/24-tooling/).

---

## Real-World Example

A common production task: fan out many network requests but **cap concurrency** so you don't overwhelm a downstream service. In Node you might reach for `p-limit`; in Tokio a `Semaphore` is the idiomatic tool. This mirrors a real URL health-checker, with `fetch` standing in for a `reqwest` call (see [HTTP Clients](/23-ecosystem/06-http-clients/)).

```rust playground
// Cargo.toml: cargo add tokio --features full
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tokio::time::sleep;

/// Stand-in for a real HTTP GET (would be a reqwest call in production).
async fn fetch(url: &str, latency_ms: u64) -> Result<usize, String> {
    sleep(Duration::from_millis(latency_ms)).await;
    if url.contains("bad") {
        Err(format!("{url}: connection refused"))
    } else {
        Ok(url.len()) // pretend "bytes downloaded"
    }
}

#[tokio::main]
async fn main() {
    let urls = vec![
        ("https://a.example", 120),
        ("https://b.example", 80),
        ("https://bad.example", 50),
        ("https://c.example", 200),
        ("https://d.example", 60),
    ];

    // At most 2 requests in flight at once — the Tokio answer to p-limit(2).
    let limit = Arc::new(Semaphore::new(2));
    let start = Instant::now();

    let mut handles = Vec::new();
    for (url, latency) in urls {
        let limit = Arc::clone(&limit);
        handles.push(tokio::spawn(async move {
            // Permit is held until `_permit` drops at the end of the task.
            let _permit = limit.acquire().await.unwrap();
            (url, fetch(url, latency).await)
        }));
    }

    for h in handles {
        match h.await.unwrap() {
            (url, Ok(bytes)) => println!("OK   {url} ({bytes} bytes)"),
            (url, Err(e)) => println!("FAIL {url} -> {e}"),
        }
    }
    println!("scanned in {}ms", start.elapsed().as_millis());
}
```

Real output:

```text
OK   https://a.example (17 bytes)
OK   https://b.example (17 bytes)
FAIL https://bad.example -> https://bad.example: connection refused
OK   https://c.example (17 bytes)
OK   https://d.example (17 bytes)
scanned in 325ms
```

The total time (~325ms) reflects the semaphore: with only two permits, the five requests queue into staged batches instead of all firing at once. Swap `fetch` for a real `reqwest::Client::get`, and this is a production-ready bounded crawler running on Tokio's multi-thread scheduler.

---

## Further Reading

- [Tokio: Getting Started](https://tokio.rs/tokio/tutorial) — the official tutorial.
- [`tokio` on docs.rs](https://docs.rs/tokio/latest/tokio/) — runtime flavors and feature flags.
- [`smol` on docs.rs](https://docs.rs/smol/latest/smol/) — the modular small runtime.
- [The Async Book](https://rust-lang.github.io/async-book/) — how `Future` and executors work under the hood.
- Section cross-links: [The Tokio Runtime](/11-async/02-tokio-intro/) · [Tokio Setup](/11-async/03-tokio-setup/) · [Promises vs Futures](/11-async/00-promises-vs-futures/) · [Spawning Tasks](/11-async/09-spawning-tasks/) · [Select & Join](/11-async/07-select-join/) · [Sync Primitives](/11-async/11-sync-primitives/)
- Ecosystem siblings: [Popular Crates](/23-ecosystem/00-popular-crates/) · [Web Frameworks](/23-ecosystem/01-web-frameworks/) · [HTTP Clients](/23-ecosystem/06-http-clients/) · [Useful Crates](/23-ecosystem/10-useful-crates/)
- Foundations: [Why Rust](/00-introduction/) · [Getting Started](/01-getting-started/) · [Basics](/02-basics/) · [Tooling](/24-tooling/)

---

## Exercises

### Exercise 1: Run a future on two different runtimes

**Difficulty:** Beginner

**Objective:** See firsthand that a pure-computation future is runtime-agnostic.

**Instructions:** Write an `async fn greet(name: &str) -> String` that returns `"Hello, {name}!"`. Run it once with `futures::executor::block_on` and once with a hand-built Tokio runtime (`tokio::runtime::Runtime::new()`), printing both results. (`cargo add tokio --features rt,rt-multi-thread` and `cargo add futures`.)

<details>
<summary>Solution</summary>

```rust
// Cargo.toml: cargo add tokio --features rt,rt-multi-thread ; cargo add futures
async fn greet(name: &str) -> String {
    format!("Hello, {name}!")
}

fn main() {
    let a = futures::executor::block_on(greet("futures"));
    println!("{a}");

    let rt = tokio::runtime::Runtime::new().unwrap();
    let b = rt.block_on(greet("tokio"));
    println!("{b}");
}
```

Real output:

```text
Hello, futures!
Hello, tokio!
```

The same `greet` function ran on two unrelated executors because it touches no async I/O: only `async`/`await` and `std`.

</details>

### Exercise 2: Move blocking work off the async workers

**Difficulty:** Intermediate

**Objective:** Fix a task that blocks the runtime by relocating it to the blocking pool.

**Instructions:** Start from this code, which blocks an async worker with a synchronous loop. Rewrite the heavy computation so it runs via `tokio::task::spawn_blocking`, then `.await` the result. Print the sum.

```rust
#[tokio::main]
async fn main() {
    let sum = tokio::spawn(async {
        // TODO: this synchronous loop blocks the async worker — move it off.
        let mut total: u64 = 0;
        for i in 0..50_000_000u64 {
            total = total.wrapping_add(i);
        }
        total
    })
    .await
    .unwrap();
    println!("sum = {sum}");
}
```

<details>
<summary>Solution</summary>

```rust
// Cargo.toml: cargo add tokio --features full
#[tokio::main]
async fn main() {
    // spawn_blocking runs the CPU-heavy loop on Tokio's dedicated blocking
    // pool, leaving the async worker threads free to drive I/O.
    let sum = tokio::task::spawn_blocking(|| {
        let mut total: u64 = 0;
        for i in 0..50_000_000u64 {
            total = total.wrapping_add(i);
        }
        total
    })
    .await
    .unwrap();

    println!("sum = {sum}");
}
```

Real output:

```text
sum = 1249999975000000
```

`spawn_blocking` returns a `JoinHandle`, so you `.await` it just like a normal task, but the work happened on a separate pool, never stalling the scheduler.

</details>

### Exercise 3: Enforce a deadline with `tokio::time::timeout`

**Difficulty:** Advanced

**Objective:** Bound a slow operation so it cannot hang forever: the async equivalent of `Promise.race` against a timer.

**Instructions:** Write `async fn slow_query(ms: u64) -> String` that sleeps `ms` milliseconds then returns a message. Using `tokio::time::timeout`, run it twice: once with a 100ms deadline against a 250ms query (should time out) and once against a 40ms query (should succeed). Print which case happened. (`cargo add tokio --features full`.)

<details>
<summary>Solution</summary>

```rust playground
// Cargo.toml: cargo add tokio --features full
use std::time::Duration;
use tokio::time::{sleep, timeout};

async fn slow_query(ms: u64) -> String {
    sleep(Duration::from_millis(ms)).await;
    format!("query finished after {ms}ms")
}

#[tokio::main]
async fn main() {
    // Wrap any future in a deadline; Err(Elapsed) if it overruns.
    match timeout(Duration::from_millis(100), slow_query(250)).await {
        Ok(result) => println!("got: {result}"),
        Err(_elapsed) => println!("timed out after 100ms"),
    }

    match timeout(Duration::from_millis(100), slow_query(40)).await {
        Ok(result) => println!("got: {result}"),
        Err(_elapsed) => println!("timed out after 100ms"),
    }
}
```

Real output:

```text
timed out after 100ms
got: query finished after 40ms
```

`timeout` returns `Result<T, Elapsed>`: `Ok` if the inner future completed in time, `Err(Elapsed)` if the deadline fired first. Unlike `Promise.race`, the losing future is dropped (cancelled) rather than left running.

</details>
