---
title: "Setting Up Tokio"
description: "Rust ships no async runtime, so .await does nothing until you add one. Wire up Tokio: the dependency, #[tokio::main], and building a runtime by hand."
---

Rust has no built-in async runtime, so before any `.await` does anything you must add one. **Tokio** is the de-facto standard. This page is about the mechanics of *wiring it up*: adding the crate with the right features, the `#[tokio::main]` attribute, and building a runtime by hand.

---

## Quick Overview

In JavaScript the async runtime is invisible: the engine (V8) and its event loop are always present, so `async`/`await` "just works". In Rust, **futures are lazy** (they do nothing until polled) and there is **no executor in the standard library**. You bring your own runtime, and 95% of the time that runtime is **Tokio**.

This topic covers the three things you need to get async code running:

- Adding `tokio` to `Cargo.toml` with `features = ["full"]`.
- The `#[tokio::main]` attribute that turns an `async fn main` into a normal `main`.
- Building a runtime manually with `tokio::runtime::Builder` when you need more control.

> **Note:** *Why* Rust needs an explicit runtime at all is covered in [The Tokio Runtime](/11-async/02-tokio-intro/), and *why* futures are lazy in [Promises vs Futures](/11-async/00-promises-vs-futures/). This page assumes you accept that and want to set it up.

---

## TypeScript/JavaScript Example

In Node.js there is nothing to install or configure. The runtime ships with the engine, and `async`/`await` is part of the language. You write an entry point and call async functions directly:

```typescript
// index.ts — no runtime setup required; Node already has an event loop.

async function fetchData(): Promise<string> {
  // Simulate a 50 ms network call.
  await new Promise((resolve) => setTimeout(resolve, 50));
  return "data from the network";
}

async function main(): Promise<void> {
  console.log("Server starting...");
  const result = await fetchData();
  console.log(`Got: ${result}`);
}

// Top-level await also works in modern modules, but a main() is common.
main();
```

```bash
# You just run it. The runtime is built in.
node index.ts
```

There is no concept of "choosing a runtime" or "starting the event loop"; `setTimeout`, `fetch`, and `Promise` are all backed by libuv/V8 automatically.

---

## Rust Equivalent

In Rust you add the runtime as a dependency and annotate `main` so Tokio sets up the event loop for you:

```bash
cargo new async_demo
cd async_demo
cargo add tokio --features full
```

```rust
// src/main.rs
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main] // sets up the Tokio runtime and runs the async body to completion
async fn main() {
    println!("Server starting...");
    let result = fetch_data().await;
    println!("Got: {result}");
}

async fn fetch_data() -> String {
    // Tokio's async sleep — NOT std::thread::sleep, which would block the runtime.
    sleep(Duration::from_millis(50)).await;
    "data from the network".to_string()
}
```

```bash
cargo run
```

Real output:

```
Server starting...
Got: data from the network
```

The `#[tokio::main]` attribute is the entire difference from JavaScript: it is the line that says "create an event loop and drive my futures to completion."

---

## Detailed Explanation

### Adding the dependency

`cargo add tokio --features full` writes this into `Cargo.toml`:

```toml
[dependencies]
tokio = { version = "1.52.3", features = ["full"] }
```

> **Note:** `cargo add` is built into Cargo (since 1.62); you do **not** need `cargo-edit`. The latest stable Rust is **1.96.0**; the latest stable edition is **2024**, which `cargo new` selects for you automatically.

The `features = ["full"]` part is the one you must not skip while learning. Tokio is heavily **feature-gated**: by default almost nothing is enabled, so individual capabilities (timers, TCP, the multi-thread scheduler, the `#[tokio::main]` macro) each live behind a feature flag. `"full"` turns them all on, which is exactly what you want while you are getting started.

Compare to how Node bundles everything: there is no `--features` equivalent because the JS runtime is monolithic. Tokio's à la carte design lets a production binary shrink to only what it uses, but that optimization comes *later*. (More on trimming features under [Best Practices](#best-practices).)

### What `#[tokio::main]` actually does

`#[tokio::main]` is a **procedural macro**, not magic syntax. Rust does not allow `main` itself to be `async` (you will see the exact error in [Common Pitfalls](#common-pitfalls)). The macro rewrites your `async fn main` into a normal synchronous `main` that builds a runtime and blocks on your async body. Roughly, this:

```rust
#[tokio::main]
async fn main() {
    println!("hello");
}
```

expands into something equivalent to this, which compiles and runs on its own:

```rust
fn main() {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            println!("this is roughly what #[tokio::main] expands to");
        });
}
```

```
this is roughly what #[tokio::main] expands to
```

So the macro is pure convenience: it hides the `Builder`/`block_on` boilerplate. Understanding the expansion is what makes the manual `Builder` approach (below) feel familiar instead of mysterious.

> **Tip:** Run `cargo expand` (install with `cargo install cargo-expand`) on a file using `#[tokio::main]` to see the *real* generated code for your version of Tokio.

### `block_on`: the bridge between sync and async

`block_on` is the one function that crosses from the synchronous world into the asynchronous one. It takes a future, drives it to completion on the current thread, and returns its output. Every async program has exactly one such bridge at the top: either written by `#[tokio::main]` for you, or written by hand.

This is the inverse of JavaScript: there, the whole program runs *inside* the event loop and you can `await` anywhere. In Rust, the program starts synchronous and you explicitly *enter* async via `block_on`.

### `enable_all`

`.enable_all()` turns on Tokio's **I/O driver** (for sockets, files) and **time driver** (for `sleep`, `timeout`, `interval`). Without it, timer- and I/O-based APIs will panic at runtime even though the code compiles. `#[tokio::main]` calls `enable_all()` for you; when you build a runtime manually you must call it yourself.

---

## Building a Runtime Manually

`#[tokio::main]` is great for binaries, but sometimes you need to construct the runtime yourself:

- A **synchronous** `main` (or library code) that needs to run *some* async work.
- Fine control over thread count, thread names, or the scheduler flavor.
- Running async code inside a context you do not own (a test harness, an FFI callback, a plugin).

### The two scheduler flavors

```rust
use tokio::runtime::Builder;

fn main() {
    // Current-thread: a single-threaded scheduler. Closest to Node's model.
    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    runtime.block_on(async {
        println!("current-thread runtime is running");
    });

    // Multi-thread: a work-stealing thread pool (the default for #[tokio::main]).
    let pool = Builder::new_multi_thread()
        .worker_threads(4)
        .thread_name("api-worker")
        .enable_all()
        .build()
        .unwrap();

    pool.block_on(async {
        println!("multi-thread runtime with 4 workers is running");
    });
}
```

```
current-thread runtime is running
multi-thread runtime with 4 workers is running
```

The difference between these schedulers — and which to choose — is the subject of [The Tokio Runtime](/11-async/02-tokio-intro/). Here the point is purely *how* to build each one.

### The simplest manual form

If you do not need any custom options, `Runtime::new()` gives you a default multi-thread runtime (equivalent to `Builder::new_multi_thread().enable_all().build()`):

```rust
fn main() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        println!("Running inside a manually built runtime");
        let n = compute().await;
        println!("computed = {n}");
    });
}

async fn compute() -> u32 {
    40 + 2
}
```

```
Running inside a manually built runtime
computed = 42
```

### Configuring `#[tokio::main]` without the builder

The attribute accepts arguments that map onto the builder, so you often do not need the manual form even when you want a non-default scheduler:

```rust
// A single-threaded runtime, the lightweight choice for simple CLIs.
#[tokio::main(flavor = "current_thread")]
async fn main() {
    println!("single-threaded tokio runtime");
}
```

```
single-threaded tokio runtime
```

```rust
// Multi-thread with a fixed worker count.
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    println!("multi-threaded runtime, 2 workers");
}
```

```
multi-threaded runtime, 2 workers
```

### Returning a `Result` from `main`

Because `#[tokio::main]` produces an ordinary `main`, your async `main` can return a `Result`, which lets you use the `?` operator at the top level, handy for prototypes and CLIs:

```rust
use std::time::Duration;
use tokio::time::{sleep, timeout};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // `?` works here because main returns a Result.
    let value = timeout(Duration::from_millis(100), slow_task()).await?;
    println!("task returned: {value}");
    Ok(())
}

async fn slow_task() -> u32 {
    sleep(Duration::from_millis(10)).await;
    7
}
```

```
task returned: 7
```

> **Note:** Error handling with `?` inside async functions is covered fully in [Async/Await Syntax](/11-async/01-async-await/). The takeaway *here* is that the setup (`#[tokio::main]`) does not get in the way of returning errors.

---

## Key Differences

| Concept | JavaScript / Node.js | Rust + Tokio |
| --- | --- | --- |
| Runtime source | Built into the engine (V8 + libuv) | A crate you add (`tokio`) |
| Starting the loop | Automatic, invisible | Explicit: `#[tokio::main]` or `block_on` |
| Where you can `await` | Anywhere inside `async`; top-level in modules | Only inside `async`; the top is sync until `block_on` |
| Choosing threads | Single-threaded event loop, fixed | Pick `current_thread` or `multi_thread`, set worker count |
| Enabling features | All capabilities always present | Feature-gated; use `features = ["full"]` to start |
| Multiple runtimes | One event loop per process | You may build several runtimes (rarely needed) |
| Cost of "no runtime" | Impossible state | Compiles fine, then **panics at runtime** |

The single most important row is the last one. In JavaScript "no runtime" cannot happen. In Rust, code that uses Tokio APIs *compiles* without a runtime present and only fails when it runs — a class of bug that does not exist in Node. The setup step exists precisely to avoid it.

> **Warning:** Unlike Node's single fixed event loop, a multi-thread Tokio runtime runs your tasks across several OS threads by default. This means values shared between tasks must be `Send`/`Sync`, and shared mutable state needs `Arc<Mutex<...>>` rather than a bare variable. See [The `Arc<Mutex<T>>` Pattern](/11-async/12-arc-mutex-pattern/). Rust's concurrency is opt-in and compiler-checked; it is *not* "multi-threaded by default" in the sense of unguarded shared memory.

---

## Common Pitfalls

### Pitfall 1: `async fn main` without the attribute

If you forget `#[tokio::main]`, Rust rejects an `async` main outright: `main` is special and may not be async:

```rust
async fn main() { // does not compile (error[E0752])
    println!("hi");
}
```

Real compiler output:

```
error[E0752]: `main` function is not allowed to be `async`
 --> src/main.rs:1:1
  |
1 | async fn main() {
  | ^^^^^^^^^^^^^^^ `main` function is not allowed to be `async`

For more information about this error, try `rustc --explain E0752`.
error: could not compile `probe` (bin "probe") due to 1 previous error
```

**Fix:** add `#[tokio::main]` above the function (or build a runtime manually).

### Pitfall 2: Using a Tokio API with no runtime present

This is the bug that has no JavaScript analogue. The code compiles, but at runtime there is no event loop to register the task with, so it **panics**:

```rust
fn main() {
    // compiles, but panics at runtime: no runtime in scope.
    tokio::spawn(async {
        println!("never runs");
    });
}
```

Real output when run:

```
thread 'main' panicked at src/main.rs:3:5:
there is no reactor running, must be called from the context of a Tokio 1.x runtime
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

**Fix:** call the Tokio API from inside `#[tokio::main]`, inside a `block_on`, or inside a task spawned on a runtime. (`tokio::spawn` is covered in [Spawning Tasks](/11-async/09-spawning-tasks/).)

### Pitfall 3: Calling `block_on` inside an async context

`block_on` blocks the current thread until the future finishes. Calling it from *within* a task that is already being driven by the runtime would deadlock the worker thread, so Tokio detects it and panics:

```rust
#[tokio::main]
async fn main() {
    let rt = tokio::runtime::Handle::current();
    // compiles, but panics at runtime: block_on inside a runtime.
    rt.block_on(async {
        println!("never reached");
    });
}
```

Real output when run:

```
thread 'main' panicked at src/main.rs:5:8:
Cannot start a runtime from within a runtime. This happens because a function (like `block_on`) attempted to block the current thread while the thread is being used to drive asynchronous tasks.
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

**Fix:** inside async code, use `.await`, not `block_on`. `block_on` belongs only at the synchronous boundary (the top of your program or a sync helper).

### Pitfall 4: Forgetting `features = ["full"]` (or `enable_all`)

If you add Tokio with a trimmed feature set and then reach for an API that lives behind a feature you did not enable, you get a *compile* error, not a runtime one. Here Tokio was added with `default-features = false, features = ["rt", "macros", "rt-multi-thread"]`, no `time` feature:

```rust
use std::time::Duration;

#[tokio::main]
async fn main() {
    // does not compile: the `time` module is gated behind the "time" feature.
    tokio::time::sleep(Duration::from_millis(10)).await;
    println!("done");
}
```

Real compiler output:

```
error[E0433]: failed to resolve: could not find `time` in `tokio`
   --> src/main.rs:6:12
    |
  6 |     tokio::time::sleep(Duration::from_millis(10)).await;
    |            ^^^^ could not find `time` in `tokio`
    |
note: found an item that was configured out
   --> .../tokio-1.52.3/src/lib.rs:566:13
    |
565 | / cfg_time! {
566 | |     pub mod time;
    | |             ^^^^
567 | | }
    | |_- the item is gated behind the `time` feature
```

**Fix while learning:** use `features = ["full"]`. The error message conveniently names the missing feature (`time`) when you are ready to trim.

> **Warning:** A separate but related trap is enabling the feature but forgetting `.enable_all()` on a manually built runtime; then timer/I/O APIs panic at runtime with a message about the driver not being enabled. `#[tokio::main]` calls `enable_all()` for you; manual builders must do it explicitly.

---

## Best Practices

### Start with `features = ["full"]`, trim later

While learning and prototyping, `features = ["full"]` removes a whole category of "which feature was that behind?" friction. Once your binary is stable and you care about compile time and binary size, trim to exactly what you use:

```toml
[dependencies]
# Production: only what this service actually needs.
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "time"] }
```

> **Tip:** A bare version string like `"1"` is a **caret** requirement (`>=1.0.0, <2.0.0`), not an exact pin. That is what you want for Tokio: you get compatible bug-fix and minor releases automatically. Use `"=1.52.3"` only if you truly need to freeze the exact version.

### Use `#[tokio::main]` for binaries, manual builders for libraries

Application entry points should use `#[tokio::main]` — it is clear and idiomatic. **Libraries should not pick a runtime for the caller**; expose `async fn`s and let the binary supply the runtime. Build a runtime by hand only when you are the one running the show (a CLI bridging into async, a test, an FFI boundary).

### Pick `current_thread` for simple, mostly-I/O CLIs

A single-threaded runtime has lower overhead and avoids `Send` bounds on your futures. For a CLI that makes a few network calls, `#[tokio::main(flavor = "current_thread")]` is often the better default; reach for `multi_thread` when you have many concurrent tasks or CPU-bound work spread across `spawn_blocking`. The trade-offs are explored in [Async vs Sync](/11-async/13-async-vs-sync/).

### Build the runtime once and reuse it

Constructing a runtime allocates threads and drivers; do not create one per operation. Build it once, then call `block_on` (or spawn tasks) as many times as you like:

```rust
use tokio::runtime::Builder;

async fn square(n: u64) -> u64 {
    n * n
}

fn main() {
    // One runtime, reused for several independent operations.
    let rt = Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(8) // cap the blocking thread pool
        .build()
        .unwrap();

    for n in 1..=4 {
        let result = rt.block_on(square(n));
        println!("{n}^2 = {result}");
    }
}
```

```
1^2 = 1
2^2 = 4
3^2 = 9
4^2 = 16
```

### Use `#[tokio::test]` for async tests

The plain `#[test]` attribute cannot run an `async fn`. Tokio ships `#[tokio::test]`, which builds a runtime around each test (it accepts the same `flavor`/`worker_threads` arguments as `#[tokio::main]`):

```rust
use std::time::Duration;
use tokio::time::sleep;

async fn double(n: u32) -> u32 {
    sleep(Duration::from_millis(5)).await;
    n * 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn doubles_the_input() {
        assert_eq!(double(21).await, 42);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn runs_on_multi_thread() {
        assert_eq!(double(5).await, 10);
    }
}
```

```
running 2 tests
test tests::doubles_the_input ... ok
test tests::runs_on_multi_thread ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

---

## Real-World Example

A common production pattern: a service starts a Tokio runtime *explicitly* (rather than via `#[tokio::main]`) so it can name threads for observability and pin a worker count, then runs concurrent I/O on it. Here we fetch several pages of a paginated API concurrently and gather the results.

```rust
use std::time::Duration;
use tokio::runtime::Builder;
use tokio::time::sleep;

/// Simulates fetching one page of results from a paginated API.
async fn fetch_page(page: u32) -> Vec<String> {
    sleep(Duration::from_millis(20)).await; // stands in for a network round-trip
    (0..3).map(|i| format!("page {page} item {i}")).collect()
}

fn main() {
    // Build the runtime explicitly: named threads show up in profilers/crash
    // dumps, and a fixed worker count keeps resource use predictable.
    let runtime = Builder::new_multi_thread()
        .worker_threads(4)
        .thread_name("fetch-worker")
        .enable_all()
        .build()
        .expect("failed to build Tokio runtime");

    let all_items = runtime.block_on(async {
        // Fetch three pages concurrently; join! awaits all of them together.
        let (a, b, c) = tokio::join!(fetch_page(1), fetch_page(2), fetch_page(3));
        let mut items = Vec::new();
        items.extend(a);
        items.extend(b);
        items.extend(c);
        items
    });

    println!("fetched {} items", all_items.len());
    for item in &all_items {
        println!("  - {item}");
    }
}
```

```
fetched 9 items
  - page 1 item 0
  - page 1 item 1
  - page 1 item 2
  - page 2 item 0
  - page 2 item 1
  - page 2 item 2
  - page 3 item 0
  - page 3 item 1
  - page 3 item 2
```

Because all three pages sleep 20 ms *concurrently*, the whole block finishes in roughly 20 ms rather than 60 ms, the same intuition as `Promise.all`. (`join!` and friends are covered in [Concurrent Awaiting](/11-async/07-select-join/).)

---

## Further Reading

- [Tokio Tutorial — Setup](https://tokio.rs/tokio/tutorial/setup) — official getting-started guide.
- [`#[tokio::main]` macro docs](https://docs.rs/tokio/latest/tokio/attr.main.html) — every attribute argument.
- [`tokio::runtime::Builder` docs](https://docs.rs/tokio/latest/tokio/runtime/struct.Builder.html): all configuration options.
- [Tokio feature flags](https://docs.rs/tokio/latest/tokio/index.html#feature-flags) — what each feature enables, for trimming `"full"`.
- [The Async Book](https://rust-lang.github.io/async-book/) — runtime-agnostic foundations.

Related sections of this guide:

- [Promises vs Futures](/11-async/00-promises-vs-futures/) — why Rust futures are lazy and need a runtime at all.
- [The Tokio Runtime](/11-async/02-tokio-intro/) — Node's event loop vs Tokio; current-thread vs multi-thread schedulers in depth.
- [Async/Await Syntax](/11-async/01-async-await/): `async`/`await` syntax and `?` inside async.
- [Spawning Tasks](/11-async/09-spawning-tasks/) — `tokio::spawn`, `JoinHandle`, and `spawn_blocking`.
- [Async vs Sync](/11-async/13-async-vs-sync/) — choosing a scheduler flavor and when to go async at all.
- [Understanding Cargo](/01-getting-started/03-cargo-basics/) — `cargo add`, features, and `Cargo.toml`.
- [Basics](/02-basics/): Rust fundamentals if you need a refresher.
- Next section: [Modules & Packages](/12-modules-packages/) — organizing crates and modules.

---

## Exercises

### Exercise 1: First Tokio program

**Difficulty:** Easy

**Objective:** Wire up Tokio from scratch and confirm an async function runs.

**Instructions:**

1. Run `cargo new tokio_hello` and `cargo add tokio --features full`.
2. Write an `async fn greet(name: &str) -> String` that returns `"Hello, {name}!"`.
3. In an `async` `main`, `.await` it and print the result.
4. Make `main` use the Tokio runtime so it compiles and runs.

<details>
<summary>Solution</summary>

```rust
// src/main.rs
#[tokio::main]
async fn main() {
    let message = greet("Ada").await;
    println!("{message}");
}

async fn greet(name: &str) -> String {
    format!("Hello, {name}!")
}
```

Output:

```
Hello, Ada!
```

`#[tokio::main]` builds the runtime and drives the async body; without it the `async fn main` would fail to compile with `error[E0752]`.

</details>

### Exercise 2: Bridge sync code into async

**Difficulty:** Medium

**Objective:** Run an async function from a **synchronous** `main` by building the runtime by hand.

**Instructions:**

1. Keep `main` as a plain (non-async) `fn`.
2. Write `fn get_greeting(name: &str) -> String` that builds a runtime, runs an `async` helper to completion, and returns the `String`.
3. Call it from the synchronous `main` and print the result.
4. Use `tokio::runtime::Runtime::new()` and `block_on`.

<details>
<summary>Solution</summary>

```rust
// src/main.rs
use tokio::runtime::Runtime;

/// A synchronous function that needs one async result.
fn get_greeting(name: &str) -> String {
    let rt = Runtime::new().expect("failed to create runtime");
    rt.block_on(async { build_greeting(name).await })
}

async fn build_greeting(name: &str) -> String {
    format!("Hello, {name}!")
}

fn main() {
    // main itself is fully synchronous.
    let msg = get_greeting("Ada");
    println!("{msg}");
}
```

Output:

```
Hello, Ada!
```

`block_on` is the bridge from sync to async. Note it is called from a *synchronous* context — calling it from inside async code would panic (see Pitfall 3).

</details>

### Exercise 3: Configure a custom runtime and run an async test

**Difficulty:** Medium–Hard

**Objective:** Build a `current_thread` runtime with custom options via `Builder`, reuse it for several operations, and verify behavior with `#[tokio::test]`.

**Instructions:**

1. Write `async fn square(n: u64) -> u64`.
2. In `main`, build a **current-thread** runtime with `enable_all()` and `max_blocking_threads(8)`, then loop over `1..=4` calling `block_on(square(n))` and printing each result. Build the runtime once, outside the loop.
3. Add a `#[tokio::test]` asserting `square(9).await == 81`.

<details>
<summary>Solution</summary>

```rust
// src/main.rs
use tokio::runtime::Builder;

async fn square(n: u64) -> u64 {
    n * n
}

fn main() {
    // Build ONE runtime and reuse it for every operation.
    let rt = Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(8)
        .build()
        .unwrap();

    for n in 1..=4 {
        let result = rt.block_on(square(n));
        println!("{n}^2 = {result}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn squares_correctly() {
        assert_eq!(square(9).await, 81);
    }
}
```

`cargo run` output:

```
1^2 = 1
2^2 = 4
3^2 = 9
4^2 = 16
```

`cargo test` output:

```
running 1 test
test tests::squares_correctly ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

Building the runtime once (rather than per loop iteration) avoids repeatedly allocating threads and drivers, the practice from [Best Practices](#best-practices). `#[tokio::test]` supplies a fresh runtime for the test so you do not build one yourself there.

</details>
