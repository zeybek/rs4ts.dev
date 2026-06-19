---
title: "The Tokio Runtime: Rust's Async Engine"
description: "Node's event loop is always running; Rust ships no runtime, so Tokio drives your futures. Compare its current-thread and multi-thread schedulers."
---

## Quick Overview

In Node.js, the **event loop** that drives your `async` code is built into the runtime; you never install it or start it. In Rust, there is **no built-in async runtime**: `async` is just language syntax, and you must bring your own executor to actually run futures. **Tokio** is the de-facto standard async runtime, and this page explains what it is, why Rust needs it, and how its **current-thread** and **multi-thread** schedulers compare to Node's single-threaded event loop.

> **Note:** This page focuses on the runtime itself. The *language-level* difference between eager Promises and lazy futures lives in [Promises vs Futures](/11-async/00-promises-vs-futures/), and the practical "how do I add and start Tokio" steps live in [Tokio Setup](/11-async/03-tokio-setup/).

---

## TypeScript/JavaScript Example

In Node.js, the runtime is the environment. You write `async`/`await`, and V8 plus libuv provide a single-threaded event loop that schedules your callbacks, timers, and I/O. There is nothing to install or initialize:

```typescript
// node app.ts — the event loop already exists; you just use it.

async function callService(name: string, ms: number): Promise<string> {
  await new Promise((resolve) => setTimeout(resolve, ms));
  return `${name} responded after ${ms}ms`;
}

async function main(): Promise<void> {
  const start = Date.now();

  // Promise.all runs these concurrently on the single event-loop thread.
  const results = await Promise.all([
    callService("auth", 120),
    callService("inventory", 90),
    callService("pricing", 150),
  ]);

  for (const r of results) console.log(r);
  // ~150ms total (the slowest call), NOT 360ms — the loop interleaves the waits.
  console.log(`all done in ${Date.now() - start}ms`);
}

main();
```

Key facts about the Node model that a TypeScript/JavaScript developer relies on every day:

- There is exactly **one** JavaScript thread. Your code never runs in parallel with other JS code.
- The event loop is **always on**. Calling `main()` and forgetting to `await` it still "works" because the loop keeps running until the queue drains.
- Concurrency comes from **not blocking** the loop: timers and I/O happen off-thread (in libuv's thread pool or the kernel) and resume your code via callbacks.

---

## Rust Equivalent

Rust gives you the same `async`/`await` syntax, but **no event loop is running unless you start one**. Tokio provides that loop. The most common way to start it is the `#[tokio::main]` attribute, which wraps your `async fn main` in a runtime:

```rust
use std::time::{Duration, Instant};
use tokio::time::sleep;

/// Simulates calling a downstream service that takes `ms` milliseconds.
async fn call_service(name: &str, ms: u64) -> String {
    sleep(Duration::from_millis(ms)).await;
    format!("{name} responded after {ms}ms")
}

#[tokio::main] // <-- starts a Tokio runtime and runs the async body on it
async fn main() {
    let start = Instant::now();

    // Spawn three independent tasks. The runtime drives them concurrently.
    let h1 = tokio::spawn(call_service("auth", 120));
    let h2 = tokio::spawn(call_service("inventory", 90));
    let h3 = tokio::spawn(call_service("pricing", 150));

    // JoinHandle::await yields Result<T, JoinError>.
    let results = [
        h1.await.unwrap(),
        h2.await.unwrap(),
        h3.await.unwrap(),
    ];

    for r in &results {
        println!("{r}");
    }
    // Total ~= the SLOWEST call, not the sum.
    println!("all done in {}ms", start.elapsed().as_millis());
}
```

Real output (compile-verified with Tokio 1.52 on Rust 1.96):

```text
auth responded after 120ms
inventory responded after 90ms
pricing responded after 150ms
all done in 152ms
```

The 152ms result is the whole point: three "150ms-ish" calls finished in roughly the time of the slowest one, because the runtime interleaved their waits, exactly like Node's `Promise.all`. The important difference is the `#[tokio::main]` line. Remove it, and there is no engine to drive any of this.

> **Note:** This snippet needs Tokio in `Cargo.toml` (`tokio = { version = "1", features = ["full"] }`). The exact `cargo add` invocation is covered in [Tokio Setup](/11-async/03-tokio-setup/).

---

## Detailed Explanation

### Why does Rust need an explicit runtime at all?

In JavaScript, `async`/`await` and the event loop are inseparable: the loop ships inside V8/Node. In Rust, the language team made a deliberate, different choice:

1. **`async fn` is pure syntax sugar.** When you write `async fn f() -> T`, the compiler rewrites it into a state machine that implements the `Future` trait. Calling `f()` does **not** run anything; it just *constructs* that state machine. (This laziness is the headline difference from JS Promises — see [Promises vs Futures](/11-async/00-promises-vs-futures/).)
2. **A future only makes progress when something `poll`s it.** A `poll` either returns `Poll::Ready(value)` or `Poll::Pending`. The thing that repeatedly calls `poll`, parks pending tasks, and wakes them when their I/O is ready is called an **executor** (or **runtime**).
3. **The standard library ships the `Future` trait and `async`/`await`, but no executor.** This keeps `std` small and lets Rust run on environments with wildly different needs: a Linux server, an embedded microcontroller with no heap, a browser via WebAssembly. Each picks a runtime that fits (Tokio for servers, Embassy for embedded, etc.).

So in Rust the "event loop" is a library you choose, not a built-in you inherit. **Tokio** is that library for the vast majority of networked and server-side code.

### What is in a Tokio runtime?

A Tokio runtime bundles three things that Node fuses into one opaque "event loop":

- A **scheduler** (also called the executor): decides which ready task to poll next, and on which thread.
- An **I/O driver / reactor**: registers sockets, timers, and other OS resources with the operating system (via `epoll`/`kqueue`/IOCP) and wakes the corresponding task when they become ready.
- A **timer** and a **blocking thread pool** (for `spawn_blocking`, covered in [Spawning Tasks](/11-async/09-spawning-tasks/)).

When you write `#[tokio::main]`, the macro expands to roughly this:

```rust
// What `#[tokio::main]` generates (conceptually) — not something you type by hand.
fn main() {
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(async {
            // ... your async main body ...
        });
}
```

`Runtime::new()` builds the multi-thread scheduler, and `block_on` blocks the **current** thread and drives the given future to completion, returning its output (spawned tasks may run on the runtime's worker threads). `block_on` is the bridge from the ordinary synchronous world (`fn main`) into async-land.

### Building a runtime by hand

The attribute is convenience; the `Builder` is the real API and is worth seeing once, because it makes the "you must start a runtime" point concrete:

```rust
fn main() {
    // Build a CURRENT-THREAD runtime: one OS thread runs the event loop,
    // exactly like Node's single-threaded model.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        let handle = tokio::spawn(async { 21 * 2 });
        let answer = handle.await.unwrap();
        println!("current-thread runtime computed: {answer}");
    });
}
```

Real output:

```text
current-thread runtime computed: 42
```

`enable_all()` turns on both the I/O and timer drivers; without it, things like `tokio::net` sockets and `tokio::time::sleep` would panic at runtime. `block_on` then drives the async block on this thread until it finishes.

### The two schedulers

Tokio ships two scheduler "flavors":

```rust
// 1) Multi-thread (the default for #[tokio::main]):
let rt = tokio::runtime::Builder::new_multi_thread()
    .worker_threads(4) // optional; defaults to the number of CPU cores
    .enable_all()
    .build()
    .unwrap();

// 2) Current-thread (one OS thread; closest to Node's model):
let rt = tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()
    .unwrap();
```

You can also select the flavor on the attribute:

```rust
#[tokio::main(flavor = "current_thread")]
async fn main() { /* ... */ }

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() { /* ... */ }
```

The **multi-thread** scheduler runs a pool of worker threads and uses **work-stealing**: an idle worker can pull tasks from a busy worker's queue. This is genuine parallelism: two of your tasks can be executing Rust code *at the same instant* on different cores. The **current-thread** scheduler runs everything on the one thread that called `block_on`, interleaving tasks at `.await` points, just like Node.

> **Tip:** "Concurrency" (making progress on many things by interleaving) and "parallelism" (literally running at the same time) are different. The multi-thread scheduler gives you both; the current-thread scheduler gives you concurrency only. [Concurrency](/11-async/10-concurrency/) dives into this distinction.

### Seeing the single-thread model bite

Because the current-thread scheduler has exactly one thread, blocking that thread blocks *everything*, and this is where TypeScript/JavaScript intuition transfers directly (you already know not to run a `while(true)` loop on the event loop). The following compares a blocking sleep against an async sleep on a current-thread runtime:

```rust
use std::time::{Duration, Instant};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let start = Instant::now();

    // std::thread::sleep BLOCKS the OS thread the runtime is on.
    let a = tokio::spawn(async {
        std::thread::sleep(Duration::from_millis(100)); // blocks the whole runtime
        "a"
    });
    let b = tokio::spawn(async {
        std::thread::sleep(Duration::from_millis(100));
        "b"
    });

    let _ = a.await.unwrap();
    let _ = b.await.unwrap();
    println!("blocking sleeps took {}ms", start.elapsed().as_millis());
}
```

Real output:

```text
blocking sleeps took 209ms
```

The two 100ms sleeps could **not** overlap. They ran one after another (~200ms total) because each one held the single runtime thread hostage. Swap in Tokio's async timer, which yields control back to the runtime instead of blocking the thread:

```rust
use std::time::{Duration, Instant};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let start = Instant::now();

    let a = tokio::spawn(async {
        tokio::time::sleep(Duration::from_millis(100)).await; // yields to runtime
        "a"
    });
    let b = tokio::spawn(async {
        tokio::time::sleep(Duration::from_millis(100)).await;
        "b"
    });

    let _ = a.await.unwrap();
    let _ = b.await.unwrap();
    println!("async sleeps took {}ms", start.elapsed().as_millis());
}
```

Real output:

```text
async sleeps took 102ms
```

Now both timers overlap on a single thread (~100ms), because `.await` on `tokio::time::sleep` hands the thread back to the scheduler so the other task can run. This is the Rust equivalent of "don't do blocking work on the Node event loop."

---

## Key Differences

| Aspect                       | Node.js (TypeScript/JavaScript)        | Rust + Tokio                                                  |
| ---------------------------- | -------------------------------------- | ------------------------------------------------------------- |
| Event loop / runtime         | Built in (libuv + V8); always running  | **Not built in**; you choose and start one (e.g. Tokio)       |
| How async code starts        | Implicit; calling an async fn schedules it | Explicit; a future does nothing until a runtime polls it   |
| Threads running your code    | Exactly one JS thread                  | One (`current_thread`) or many (`multi_thread`) worker threads |
| Parallelism of your code     | None; single-threaded                  | Yes, on the multi-thread scheduler (work-stealing)            |
| Entry point                  | Top-level `await` / `main()`           | `#[tokio::main]` or `Runtime::block_on`                       |
| Choosing the runtime         | Not a choice (it's Node)               | A real decision: Tokio, `async-std`, `smol`, Embassy, ...     |
| Blocking the loop            | Freezes everything                     | Freezes the current worker (and the whole app on `current_thread`) |

### The mental-model shift

The single biggest adjustment for a TypeScript/JavaScript developer is this: **in Node the runtime owns your program; in Rust your program owns the runtime.** You decide when it starts, how many threads it has, and when it shuts down. `block_on` is the seam between sync `main` and the async world; there is no async code without a runtime underneath it.

> **Note:** Unlike Node, where every `async function` runs on the same single loop, Tokio's default (`multi_thread`) can run your tasks truly in parallel. That means shared mutable state needs synchronization — see [Arc + Mutex Pattern](/11-async/12-arc-mutex-pattern/) and [Sync Primitives](/11-async/11-sync-primitives/). This is the "fearless concurrency" payoff: the compiler forces you to handle it.

### Why two schedulers?

- Use **`multi_thread`** (the default) for servers and anything that benefits from using all cores; it is what `#[tokio::main]` gives you by default.
- Use **`current_thread`** for lightweight CLIs, tests, single-core/embedded-ish contexts, or when your futures are not `Send` and you want to avoid the overhead and constraints of a thread pool. It is also the closest 1:1 analogue to the Node model, which can make reasoning about ordering simpler.

---

## Common Pitfalls

### Pitfall 1: Using `.await` in a non-async `fn main`

A `Future`'s `.await` is only legal inside an `async` context. Forgetting `#[tokio::main]` (or otherwise making `main` async) is the most common first mistake:

```rust
use std::time::Duration;

fn main() {
    // does not compile (error[E0728]): `await` outside an async fn/block
    tokio::time::sleep(Duration::from_millis(1)).await;
}
```

Real compiler error:

```text
error[E0728]: `await` is only allowed inside `async` functions and blocks
 --> src/main.rs:5:50
  |
3 | fn main() {
  | --------- this is not `async`
4 |     // does not compile (error[E0728]): `await` outside an async fn/block
5 |     tokio::time::sleep(Duration::from_millis(1)).await;
  |                                                  ^^^^^ only allowed inside `async` functions and blocks
```

**Fix:** add `#[tokio::main]` and make `main` async, or wrap the work in `rt.block_on(async { ... })`.

### Pitfall 2: Using Tokio resources with no runtime active

Even in a `fn main`, if you build a future that needs the runtime (a timer, a socket, `tokio::spawn`) and try to run it without an active runtime, you get a **runtime panic**, not a compile error, because the type system can't see that a runtime is missing:

```rust
fn main() {
    // panics at runtime: no active Tokio runtime to register the task with.
    tokio::spawn(async { println!("hi"); });
}
```

Real panic:

```text
thread 'main' panicked at src/main.rs:3:5:
there is no reactor running, must be called from the context of a Tokio 1.x runtime
```

**Fix:** start a runtime first (`#[tokio::main]`, or build one and call `block_on`). "There is no reactor running" is the message to memorize; it almost always means "you forgot to start (or you already dropped) the runtime."

### Pitfall 3: Assuming "async" means "another thread"

Coming from Node, you might assume async work always happens "somewhere else." On the `current_thread` scheduler it does not; see the 209ms blocking example above. A long synchronous computation or a `std::thread::sleep` in an async task starves every other task on that worker. For CPU-bound work, offload it (see [Spawning Tasks](/11-async/09-spawning-tasks/)'s `spawn_blocking`, or the exercises below).

### Pitfall 4: Creating a runtime inside a runtime

Calling `Runtime::new().block_on(...)` from *inside* an already-running Tokio context (for example, inside an `async fn`) panics with "Cannot start a runtime from within a runtime." Build the runtime once, at the top of `main`. If you genuinely need to block on async code from a sync callback running on the runtime, use `tokio::task::block_in_place` or a dedicated thread instead.

---

## Best Practices

- **Prefer the `#[tokio::main]` attribute** for binaries. Reach for `runtime::Builder` only when you need to tune `worker_threads`, name threads, set a custom thread stack size, or own the runtime's lifetime explicitly.
- **Default to the multi-thread scheduler** for servers; choose `current_thread` deliberately (lightweight CLIs, tests, `!Send` futures). Make the choice consciously rather than copy-pasting.
- **Never block a worker thread.** Avoid `std::thread::sleep`, large synchronous CPU loops, and blocking file/database calls inside async tasks. Use async equivalents (`tokio::time::sleep`, `tokio::fs`) or `tokio::task::spawn_blocking` for unavoidably blocking code.
- **Create exactly one runtime per process** in normal applications. Multiple runtimes are an advanced, rare need.
- **Add Tokio with the features you actually use.** `features = ["full"]` is fine while learning; trim it later (`["rt-multi-thread", "macros", "net", "time"]`, etc.) to cut compile time. See [Tokio Setup](/11-async/03-tokio-setup/).
- **Use the latest stable Tokio (1.x).** The current stable Rust is 1.96.0 and the newest edition is 2024; `cargo new` auto-selects it. Avoid copying pre-1.0 or pre-`axum 0.8` tutorials whose runtime/server APIs have since changed.

---

## Real-World Example

A small task-runner that fans out work across the multi-thread scheduler. Each task records which OS worker thread executed it, illustrating that the runtime spreads work across the pool (true parallelism), unlike Node's single thread:

```rust
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    // Shared, thread-safe set of the worker thread IDs we observed.
    let seen: Arc<Mutex<HashSet<std::thread::ThreadId>>> =
        Arc::new(Mutex::new(HashSet::new()));

    let mut handles = Vec::new();
    for _ in 0..32 {
        let seen = Arc::clone(&seen);
        handles.push(tokio::spawn(async move {
            // Yield at an .await point so the scheduler can balance work
            // across the pool rather than finishing each task instantly.
            tokio::time::sleep(Duration::from_millis(5)).await;
            seen.lock().unwrap().insert(std::thread::current().id());
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let count = seen.lock().unwrap().len();
    println!("distinct worker threads used: {count}");
}
```

Example output (the exact count is nondeterministic and depends on the scheduler and machine; on a multi-core box it will be greater than 1):

```text
distinct worker threads used: 2
```

The `Arc<Mutex<...>>` is required precisely *because* the runtime may run those closures on different threads simultaneously — something the Node model never forces you to think about. The compiler would reject sharing a plain `HashSet` across tasks, which is "fearless concurrency" in action. The full pattern is covered in [Arc + Mutex Pattern](/11-async/12-arc-mutex-pattern/).

> **Warning:** Holding a `std::sync::Mutex` guard across an `.await` is a footgun. It is fine here because we lock and immediately drop the guard within one synchronous statement. When a lock must stay held across `.await`, use Tokio's async-aware [Sync Primitives](/11-async/11-sync-primitives/).

---

## Further Reading

- [Tokio Tutorial — Hello Tokio & Setup](https://tokio.rs/tokio/tutorial) — the official getting-started walkthrough.
- [Tokio `runtime` module docs](https://docs.rs/tokio/latest/tokio/runtime/index.html) — `Runtime`, `Builder`, and scheduler details.
- [The `async`/`await` chapter of the Rust Book](https://doc.rust-lang.org/book/ch17-00-async-await.html) — language-level treatment of futures.
- [The Async Book](https://rust-lang.github.io/async-book/) — deeper coverage of futures, executors, and `poll`.

Related sections in this guide:

- [Promises vs Futures](/11-async/00-promises-vs-futures/) — eager Promises vs lazy futures; *why* a runtime is even needed.
- [Async/Await Syntax](/11-async/01-async-await/) — `async fn`, `impl Future`, `.await`, and `?` in async code.
- [Tokio Setup](/11-async/03-tokio-setup/) — adding Tokio, features, and `#[tokio::main]` in detail.
- [Spawning Tasks](/11-async/09-spawning-tasks/) — `tokio::spawn`, `JoinHandle`, and `spawn_blocking`.
- [Concurrency vs Parallelism](/11-async/10-concurrency/) — tasks vs threads, structured patterns, cancellation.
- [Async vs Sync](/11-async/13-async-vs-sync/) — when async is the wrong tool (CPU-bound work) and the "function coloring" problem.
- Foundations: [Getting Started](/01-getting-started/) and [Basics](/02-basics/). Once you understand runtimes, [Modules & Packages](/12-modules-packages/) shows how to structure a larger async crate.

---

## Exercises

### Exercise 1: Start a runtime by hand

**Difficulty:** Easy

**Objective:** Run async code without the `#[tokio::main]` attribute, proving you understand what the macro does.

**Instructions:**

1. Start from a plain, synchronous `fn main`.
2. Build a **current-thread** runtime with `tokio::runtime::Builder`.
3. Use `block_on` to run three `tick(n)` futures concurrently with `tokio::join!`, where `tick(n)` sleeps for `10 * n` ms and then prints `tick {n}`.
4. Confirm they print in ascending order (`1`, `2`, `3`) because of the staggered delays.

<details>
<summary>Solution</summary>

```rust
use std::time::Duration;
use tokio::time::sleep;

async fn tick(n: u32) {
    sleep(Duration::from_millis(10 * n as u64)).await;
    println!("tick {n}");
}

fn main() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all() // turn on the timer driver so sleep() works
        .build()
        .expect("failed to build runtime");

    rt.block_on(async {
        // All three are driven concurrently on the single runtime thread.
        tokio::join!(tick(3), tick(1), tick(2));
    });
}
```

Real output:

```text
tick 1
tick 2
tick 3
```

`enable_all()` is what makes `sleep` work; without it the timer would panic. The futures finish in delay order even though they were listed `3, 1, 2`.

</details>

### Exercise 2: Keep the runtime responsive during CPU-bound work

**Difficulty:** Medium

**Objective:** Use `spawn_blocking` so a heavy synchronous computation does not stall the single-threaded runtime.

**Instructions:**

1. Write a synchronous `heavy_sum(n)` that sums `0..n` (a CPU-bound loop with no `.await`).
2. On a **current-thread** runtime, offload `heavy_sum(50_000_000)` with `tokio::task::spawn_blocking`.
3. At the same time, spawn an I/O task that sleeps 10ms and returns a message.
4. Await both and print the message and the sum, demonstrating the timer task still made progress.

<details>
<summary>Solution</summary>

```rust
fn heavy_sum(n: u64) -> u64 {
    // CPU-bound: no .await points, so it must NOT run on a runtime worker.
    (0..n).fold(0u64, |acc, x| acc.wrapping_add(x))
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Offload to the blocking thread pool so the single runtime thread
    // stays free to drive other tasks.
    let handle = tokio::task::spawn_blocking(|| heavy_sum(50_000_000));

    let timer = tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        "timer fired"
    });

    let total = handle.await.unwrap();
    let msg = timer.await.unwrap();
    println!("{msg}; sum = {total}");
}
```

Real output (built with `--release`):

```text
timer fired; sum = 1249999975000000
```

If you ran `heavy_sum` directly inside an async task instead of `spawn_blocking`, it would monopolize the only runtime thread and the timer could not fire until it finished. `spawn_blocking` moves it to a dedicated blocking pool. (See [Spawning Tasks](/11-async/09-spawning-tasks/).)

</details>

### Exercise 3: Observe the multi-thread scheduler

**Difficulty:** Hard

**Objective:** Demonstrate that the multi-thread scheduler runs tasks on more than one OS thread.

**Instructions:**

1. Use `#[tokio::main(flavor = "multi_thread", worker_threads = 4)]`.
2. Spawn 32 tasks. Each one `.await`s a short `tokio::time::sleep`, then records `std::thread::current().id()` into a shared `Arc<Mutex<HashSet<ThreadId>>>`.
3. Await all tasks, then print how many distinct worker thread IDs were seen.
4. Run it a few times and note the count varies but is greater than 1 on a multi-core machine: proof of real parallelism. Explain why an `Arc<Mutex<...>>` is required here but would be unnecessary in Node.

<details>
<summary>Solution</summary>

```rust
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    let seen: Arc<Mutex<HashSet<std::thread::ThreadId>>> =
        Arc::new(Mutex::new(HashSet::new()));

    let mut handles = Vec::new();
    for _ in 0..32 {
        let seen = Arc::clone(&seen);
        handles.push(tokio::spawn(async move {
            // The .await lets the scheduler move/spread work across workers.
            tokio::time::sleep(Duration::from_millis(5)).await;
            seen.lock().unwrap().insert(std::thread::current().id());
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    println!("distinct worker threads used: {}", seen.lock().unwrap().len());
}
```

Example output (nondeterministic; greater than 1 on multi-core hardware):

```text
distinct worker threads used: 2
```

`Arc` lets multiple tasks share ownership of the set; `Mutex` guards the concurrent mutation. They are required because, unlike Node's single JS thread, two of these closures can run on different OS threads at the same instant. The Rust compiler enforces this: a plain shared `HashSet` would not satisfy `Send`/`Sync` and would be rejected at compile time. That is "fearless concurrency" — the runtime gives you parallelism, and the type system makes sure you handle it safely.

</details>
