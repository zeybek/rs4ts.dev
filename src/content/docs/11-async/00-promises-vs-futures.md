---
title: "Rust Futures vs JavaScript Promises"
description: "A JavaScript Promise is eager and runs on creation; a Rust Future is lazy until awaited. Rust ships no runtime, so you bring your own (Tokio)."
---

If you know JavaScript `Promise`s, you already understand 80% of Rust's async model, and the remaining 20% is exactly where most TypeScript/JavaScript developers get burned. This page is about that 20%: the single most important difference is that a JavaScript **Promise is eager** (it starts doing work the moment it is created), while a Rust **`Future` is lazy** (it does *nothing at all* until you `.await` it or hand it to a runtime).

---

## Quick Overview

A **`Future`** is Rust's equivalent of a JavaScript `Promise`: a value that represents an asynchronous computation that will produce a result later. The key difference is timing: a `Promise` begins executing the instant you create it, whereas a `Future` is an inert description of work that runs only when something *polls* it. On top of that, Rust ships with **no built-in async runtime**: there is no event loop hiding in the language, so you must bring your own executor (almost always [Tokio](/11-async/02-tokio-intro/)).

> **Note:** This page focuses on the *concept*: eager vs lazy, and the runtime requirement. The mechanics of `async`/`await` syntax live in [Async/Await Syntax](/11-async/01-async-await/), and runtime setup lives in [Setting Up Tokio](/11-async/03-tokio-setup/).

---

## TypeScript/JavaScript Example

In JavaScript, calling an `async` function (or constructing a `Promise`) starts the work **immediately**. The `await` only controls when you *wait for the result*, not when the work *begins*.

```typescript
// TypeScript / JavaScript (Node v22) — Promises are EAGER

function makeRequest(id: number): Promise<number> {
  console.log(`  -> running request ${id}`);
  return Promise.resolve(id * 10);
}

console.log("Before creating the promise");
const p = makeRequest(1); // the body runs RIGHT NOW, before the next line
console.log("After creating the promise");

const result = await p; // we only WAIT here; work already happened
console.log(`Got result: ${result}`);
```

Running this with Node v22 prints:

```
Before creating the promise
  -> running request 1
After creating the promise
Got result: 10
```

Notice the `-> running request 1` line appears **before** `After creating the promise`. The work started at creation. The same is true for the `Promise` constructor itself: the executor callback runs synchronously the moment you write `new Promise(...)`.

```typescript
// The executor runs immediately, even with no .then/.await
console.log("before new Promise");
const eager = new Promise<number>((resolve) => {
  console.log("  executor running immediately");
  resolve(42);
});
console.log("after new Promise");
```

Output (Node v22):

```
before new Promise
  executor running immediately
after new Promise
```

---

## Rust Equivalent

The same shape of code in Rust behaves differently: building the future runs **none** of its body. Only `.await` triggers execution.

```rust
// Rust — Futures are LAZY

async fn make_request(id: u32) -> u32 {
    println!("  -> running request {id}");
    id * 10
}

#[tokio::main]
async fn main() {
    println!("Before creating the future");
    let fut = make_request(1); // nothing printed yet — body has NOT run
    println!("After creating the future (nothing ran yet)");

    let result = fut.await; // NOW the body runs
    println!("Got result: {result}");
}
```

Real output (Rust 1.96.0, Tokio 1.x):

```
Before creating the future
After creating the future (nothing ran yet)
  -> running request 1
Got result: 10
```

Look at the order: `-> running request 1` appears **after** `After creating the future`, the exact opposite of JavaScript. Creating `fut` produced an inert value; the body only ran at `.await`.

> **Note:** `#[tokio::main]` is a macro that sets up a Tokio runtime and runs your `async fn main` on it. We use it here so the example is complete; it is covered in depth in [Setting Up Tokio](/11-async/03-tokio-setup/). Without *some* runtime, there is nothing to drive the future at all.

---

## Detailed Explanation

### What `async fn` actually returns

When you write `async fn make_request(id: u32) -> u32`, Rust does **not** create a function that returns `u32`. It creates a function that returns an anonymous type implementing the `Future` trait, conceptually `impl Future<Output = u32>`. The function body is compiled into a **state machine** that knows how to make progress one step at a time.

You can see this by writing the future-returning function explicitly:

```rust
use std::future::Future;

fn build_task(n: u32) -> impl Future<Output = u32> {
    // The async block evaluates to a Future; the body runs only when awaited.
    async move {
        println!("  computing for {n}");
        n + 1
    }
}

#[tokio::main]
async fn main() {
    let task = build_task(10); // nothing computed yet
    println!("task built");
    let out = task.await;
    println!("out = {out}");
}
```

Output:

```
task built
  computing for 10
out = 11
```

`build_task(10)` returns a value. The `println!("  computing for {n}")` inside it does not run until `task.await`. An `async fn` is sugar for "a function whose body is an `async` block."

### Lazy means "polled to completion"

A `Future` is defined by one method, `poll`. The runtime calls `poll`; the future does a chunk of work and returns either `Poll::Ready(value)` (done) or `Poll::Pending` (not done yet, wake me later). Nothing happens until *someone calls `poll`*, and `.await` is the syntax that arranges for that to happen.

Here is a hand-written future so you can see `poll` directly:

```rust
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

// A future that is "ready" the first time it is polled.
struct ReadyValue(u32);

impl Future for ReadyValue {
    type Output = u32;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        println!("  poll() called");
        Poll::Ready(self.0)
    }
}

#[tokio::main]
async fn main() {
    let fut = ReadyValue(99); // constructing it does NOT call poll()
    println!("future built, poll not called yet");
    let value = fut.await; // .await drives poll() for us
    println!("value = {value}");
}
```

Output:

```
future built, poll not called yet
  poll() called
value = 99
```

Constructing `ReadyValue(99)` does nothing. The `poll() called` line proves the body of a future only executes when an executor drives it.

> **Tip:** You will almost never implement `Future` by hand. `async`/`await` generates the state machine and the `poll` implementation for you. This example exists purely to make "lazy" concrete.

### The eager/lazy difference is visible in concurrency

Because JavaScript Promises are already running, `Promise.all([a, b])` runs `a` and `b` concurrently for free; they both started when you created them. In Rust, two futures awaited one after another run **sequentially**, because the first does not even begin until you `.await` it. To get concurrency you must combine them with something like [`join!`](/11-async/07-select-join/).

```rust
use std::time::Instant;
use tokio::time::{sleep, Duration};

async fn fetch(id: u32, ms: u64) -> u32 {
    sleep(Duration::from_millis(ms)).await;
    println!("  fetched {id}");
    id
}

#[tokio::main]
async fn main() {
    // Sequential: each .await waits for the previous to finish.
    let start = Instant::now();
    let a = fetch(1, 100).await;
    let b = fetch(2, 100).await;
    println!("sequential took ~{} ms (a={a}, b={b})", start.elapsed().as_millis());

    // Concurrent: join! polls both on the same task, interleaving them.
    let start = Instant::now();
    let (a, b) = tokio::join!(fetch(3, 100), fetch(4, 100));
    println!("join! took ~{} ms (a={a}, b={b})", start.elapsed().as_millis());
}
```

Real output:

```
  fetched 1
  fetched 2
sequential took ~204 ms (a=1, b=2)
  fetched 4
  fetched 3
join! took ~102 ms (a=3, b=4)
```

The sequential version takes ~200 ms (100 + 100); `join!` takes ~100 ms because both futures make progress together. In JavaScript the result depends on when the promises are *created*: `await fetch1(); await fetch2();` takes ~200 ms just like Rust, but `const a = fetch1(); const b = fetch2(); await a; await b;` finishes in ~100 ms, because each promise eagerly starts its work the moment it is created. Rust has no such hidden head start: a future does nothing until polled. The key Rust takeaway: **awaiting in sequence is sequential, and there is no hidden concurrency.**

### There is no built-in executor

This is the second half of the story. JavaScript has a built-in event loop (in the browser, and libuv in Node). It is always there; you never "start" it. Rust deliberately ships **no** runtime in its standard library. The `Future` trait and `async`/`await` are in `std`, but the thing that *drives* futures — the executor — is a library you choose.

The most common is [Tokio](/11-async/02-tokio-intro/). But any executor works; here is the same idea using a minimal one from the `futures` crate, with no Tokio at all:

```rust
use futures::executor::block_on;

async fn greet(name: &str) -> String {
    format!("Hello, {name}!")
}

fn main() {
    // `block_on` is an executor: it drives the future to completion on this thread.
    let message = block_on(greet("Rust"));
    println!("{message}");
}
```

Output:

```
Hello, Rust!
```

`block_on` is the executor: it repeatedly polls the future until it returns `Poll::Ready`, blocking the current thread in the meantime. The point is that *something* — `block_on`, `#[tokio::main]`, a runtime you build by hand — must call `poll`. Futures cannot run themselves.

### Building a runtime explicitly

`#[tokio::main]` is convenient, but it is only a macro that expands to "build a runtime and call `block_on`." You can do it yourself, which makes the "you supply the runtime" point unmistakable:

```rust
use tokio::time::{sleep, Duration};

async fn do_work() -> &'static str {
    sleep(Duration::from_millis(50)).await;
    "work complete"
}

fn main() {
    // The runtime is something YOU build — there is no implicit global executor.
    let runtime = tokio::runtime::Runtime::new().expect("failed to build runtime");
    let result = runtime.block_on(do_work());
    println!("{result}");
}
```

Output:

```
work complete
```

---

## Key Differences

| Aspect | JavaScript `Promise` | Rust `Future` |
| --- | --- | --- |
| **Execution start** | Eager: runs on creation | Lazy: runs only when `.await`ed/polled |
| **What creation does** | Starts the work, returns a handle | Builds an inert state machine, runs nothing |
| **Runtime / event loop** | Built in (browser / Node libuv) | None in `std`; you choose one (Tokio, `futures`, ...) |
| **Core mechanism** | Microtask queue managed by the engine | `poll` returning `Ready`/`Pending`, driven by an executor |
| **Re-running** | Cannot re-run; a settled Promise is final | Re-buildable: a closure can produce a fresh future each call |
| **Cancellation** | Hard: a started Promise keeps going | Drop an *unspawned* future and its work stops (a `tokio::spawn`ed task keeps running if you drop only its `JoinHandle` — use `.abort()`) |
| **Concurrency for free** | `Promise.all` of already-running promises | None; combine with `join!`/`select!` or spawn tasks |
| **Overhead of unused one** | Work already happened (wasted) | Zero — an unawaited future never ran |

### Why lazy is a feature, not a quirk

Laziness gives Rust three things JavaScript cannot offer easily:

1. **Zero-cost composition.** You can build a big tree of combined futures (`join!`, `select!`, adapters) and the work only happens when the *whole* thing is awaited. There is no partial, wasted execution.
2. **Real cancellation.** Because a future is just a value, dropping it before it finishes stops its work cleanly. A JavaScript Promise, once started, runs to completion even if nobody is listening; there is no built-in `.cancel()`. (One caveat: once you hand a future to `tokio::spawn`, the *runtime* owns it — dropping the returned `JoinHandle` detaches the task rather than cancelling it; call `handle.abort()` to cancel. See [select! and join!](/11-async/07-select-join/).)
3. **Backpressure and control.** The executor decides *when* and *how often* to poll, enabling sophisticated scheduling that an always-running model can't express.

> **Warning:** The flip side is the classic beginner trap: if you build a future and never `.await` it, **none of its code runs**. In JavaScript a "fire-and-forget" Promise still executes; in Rust it silently does nothing. The compiler warns you about this (see Pitfalls).

---

## Common Pitfalls

### Pitfall 1: Forgetting `.await` — the future never runs

This is the number one mistake for JavaScript developers. In JavaScript, dropping the `await` still runs the work (you just don't wait for it). In Rust, dropping `.await` means the work **never happens**.

```rust
async fn save_to_db(name: &str) {
    println!("saving {name}");
}

#[tokio::main]
async fn main() {
    save_to_db("Alice"); // forgot .await — body never runs
    println!("done");
}
```

The compiler catches this with a warning (it does not stop compilation, so watch for it):

```
warning: unused implementer of `Future` that must be used
 --> src/main.rs:7:5
  |
7 |     save_to_db("Alice"); // forgot .await — body never runs
  |     ^^^^^^^^^^^^^^^^^^^
  |
  = note: futures do nothing unless you `.await` or poll them
  = note: `#[warn(unused_must_use)]` on by default
```

The note — *"futures do nothing unless you `.await` or poll them"* — is the lazy model spelled out. The fix is to add `.await` (or [spawn a task](/11-async/09-spawning-tasks/) if you genuinely want fire-and-forget):

```rust
async fn save_to_db(name: &str) {
    println!("saving {name}");
}

#[tokio::main]
async fn main() {
    save_to_db("Alice").await; // now the body runs
    println!("done");
}
```

`cargo clippy` reports the same thing, so a linted CI will not let this slip through:

```
warning: unused implementer of `std::future::Future` that must be used
 --> src/main.rs:7:5
  |
7 |     save_to_db("Alice"); // forgot .await — body never runs
  |     ^^^^^^^^^^^^^^^^^^^
  |
  = note: futures do nothing unless you `.await` or poll them
  = note: `#[warn(unused_must_use)]` on by default
```

### Pitfall 2: Using `.await` outside an async context

You can only `.await` inside an `async fn` or `async` block. Trying to await in a plain `fn` (including a non-async `main`) is a hard error.

```rust
async fn fetch() -> u32 {
    42
}

fn main() {
    let x = fetch().await; // does not compile (error[E0728])
    println!("{x}");
}
```

Real compiler error:

```
error[E0728]: `await` is only allowed inside `async` functions and blocks
 --> src/main.rs:6:21
  |
5 | fn main() {
  | --------- this is not `async`
6 |     let x = fetch().await; // await outside async
  |                     ^^^^^ only allowed inside `async` functions and blocks
```

The fix is to enter an async context: add `#[tokio::main]` to make `main` async, or drive the future with an executor like `runtime.block_on(...)` / `futures::executor::block_on(...)` as shown earlier.

### Pitfall 3: Expecting work to start before `.await`

Because of eager Promises, JavaScript developers often write code assuming a "kick off now, await later" pattern:

```rust
use tokio::time::{sleep, Duration};

async fn slow(label: &str) -> &str {
    sleep(Duration::from_millis(100)).await;
    label
}

#[tokio::main]
async fn main() {
    // This does NOT start both timers — `b` hasn't begun yet.
    let a = slow("a");
    let b = slow("b");
    // Total wait is ~200 ms, not ~100 ms:
    let (ra, rb) = (a.await, b.await);
    println!("{ra} {rb}");
}
```

The two `slow(...)` calls just build futures; neither timer starts. By the time you `a.await`, only `a` is running; `b` doesn't begin until `a` finishes. If you wanted them concurrent, use [`tokio::join!`](/11-async/07-select-join/) or [spawn tasks](/11-async/09-spawning-tasks/). This compiles and runs fine. It's a *correctness/performance* trap, not a compile error, which makes it especially sneaky.

### Pitfall 4: Thinking there's a default runtime

Coming from Node, it is tempting to assume "async just works." It does not: without a runtime, `.await` has nothing to drive it. Calling `block_on` from inside an already-running Tokio runtime, or forgetting the runtime entirely, leads to panics or the "await outside async" error from Pitfall 2. The mental model to internalize: **`async`/`await` is syntax; the runtime is a dependency you add.** See [Setting Up Tokio](/11-async/03-tokio-setup/).

---

## Best Practices

- **Always `.await` (or deliberately spawn).** Treat the "unused `Future`" warning as an error in CI. If you truly want background work, use [`tokio::spawn`](/11-async/09-spawning-tasks/); that is the explicit, intentional way to "fire and forget."
- **Reach for combinators to recover JavaScript-style concurrency.** Use [`join!`/`try_join!`](/11-async/07-select-join/) for the `Promise.all` pattern and [`select!`](/11-async/07-select-join/) for `Promise.race`. Don't fake concurrency with sequential `.await`s.
- **Let `async fn` write the `Future` for you.** Return `impl Future<Output = T>` only when you need to (e.g., conditionally choosing between futures); otherwise just write `async fn`. Never hand-implement the `Future` trait unless you are writing a low-level primitive.
- **Pick one runtime and stick to it.** Mixing executors (e.g., `futures::executor::block_on` inside a Tokio task) causes confusing behavior. For applications, standardize on Tokio; see [The Tokio Runtime](/11-async/02-tokio-intro/).
- **Exploit laziness for retries and timeouts.** Because a future is a re-buildable value, a closure `|| do_work()` can produce a fresh future on each attempt, impossible with an already-settled Promise.

---

## Real-World Example

A retry helper shows why laziness is a genuine advantage. In JavaScript, once you call an `async` function you get a `Promise` that is already running and can only be awaited once. Retrying means *calling the function again*. Rust makes this explicit and clean: you pass a closure that builds a **fresh future** for each attempt.

```rust
use std::future::Future;
use tokio::time::{sleep, Duration};

/// Runs an async operation, retrying up to `max_attempts` times.
/// `op` is a closure that builds a FRESH future each call — possible only
/// because Rust futures are lazy and can be created on demand.
async fn with_retry<F, Fut, T, E>(max_attempts: u32, mut op: F) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let mut attempt = 1;
    loop {
        match op().await {
            Ok(value) => return Ok(value),
            Err(_) if attempt < max_attempts => {
                println!("  attempt {attempt} failed, retrying...");
                attempt += 1;
                sleep(Duration::from_millis(20)).await;
            }
            Err(err) => return Err(err),
        }
    }
}

// A flaky "API call" that succeeds on the 3rd try.
async fn flaky_fetch(counter: &std::cell::Cell<u32>) -> Result<String, String> {
    let n = counter.get() + 1;
    counter.set(n);
    if n < 3 {
        Err(format!("network error (call {n})"))
    } else {
        Ok(format!("payload from call {n}"))
    }
}

#[tokio::main]
async fn main() {
    let counter = std::cell::Cell::new(0);
    let result = with_retry(5, || flaky_fetch(&counter)).await;
    match result {
        Ok(body) => println!("success: {body}"),
        Err(e) => println!("gave up: {e}"),
    }
}
```

Real output:

```
  attempt 1 failed, retrying...
  attempt 2 failed, retrying...
success: payload from call 3
```

Each call to `op()` builds a brand-new future; `with_retry` polls it to completion via `.await`, and on failure simply builds the next one. Because the future is lazy and re-buildable, the retry logic is just an ordinary loop: no `Promise` juggling, no double-execution.

> **Note:** Error handling here uses `Result<T, E>` rather than thrown exceptions. The `?` operator integrates with `.await` to make this ergonomic; that is covered in [Async/Await Syntax](/11-async/01-async-await/) and builds on [Section 08: Error Handling](/08-error-handling/).

---

## Further Reading

### Official Documentation

- [The Rust Book — Futures and the Async Syntax](https://doc.rust-lang.org/book/ch17-01-futures-and-syntax.html)
- [`std::future::Future`](https://doc.rust-lang.org/std/future/trait.Future.html) — the trait at the heart of all this
- [Asynchronous Programming in Rust (the "async book")](https://rust-lang.github.io/async-book/) — especially the chapter on why futures are lazy
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial) — the de facto runtime

### Related Sections in This Guide

- [Async/Await Syntax](/11-async/01-async-await/) — the `async`/`await` syntax, `?` error handling
- [The Tokio Runtime](/11-async/02-tokio-intro/) — why Rust needs an explicit runtime; Node's event loop compared
- [Setting Up Tokio](/11-async/03-tokio-setup/) — adding Tokio, `#[tokio::main]`, the runtime builder
- [Async Functions and Async Blocks](/11-async/04-async-functions/) — `async` blocks, capturing, returning futures
- [Concurrent Awaiting](/11-async/07-select-join/) — `Promise.all`/`Promise.race` → `join!`/`select!`
- [Spawning Tasks](/11-async/09-spawning-tasks/) — `tokio::spawn` for intentional fire-and-forget
- [Async vs Sync](/11-async/13-async-vs-sync/) — when async is the right tool at all
- [Section 00: Introduction](/00-introduction/) and [Section 01: Getting Started](/01-getting-started/) — setting up Rust
- [Section 02: Basics](/02-basics/) — variables, types, the fundamentals these examples assume
- [Section 12: Modules & Packages](/12-modules-packages/) — how `tokio`/`futures` crates are added and imported

---

## Exercises

### Exercise 1: Make the work actually happen

**Difficulty:** Beginner

**Objective:** Internalize that a future does nothing until `.await`ed.

**Instructions:** The program below compiles with a warning and prints only `done`. Fix it so it also logs the event. Do not change the body of `log_event`.

```rust
use tokio::time::{sleep, Duration};

async fn log_event(name: &str) {
    sleep(Duration::from_millis(10)).await;
    println!("logged: {name}");
}

#[tokio::main]
async fn main() {
    log_event("startup"); // TODO: why doesn't this print anything?
    println!("done");
}
```

<details>
<summary>Solution</summary>

The future returned by `log_event(...)` is never polled, so its body never runs. Add `.await`:

```rust
use tokio::time::{sleep, Duration};

async fn log_event(name: &str) {
    sleep(Duration::from_millis(10)).await;
    println!("logged: {name}");
}

#[tokio::main]
async fn main() {
    log_event("startup").await; // the fix: add .await
    println!("done");
}
```

Output:

```
logged: startup
done
```

</details>

### Exercise 2: From sequential to concurrent

**Difficulty:** Intermediate

**Objective:** See that two futures only run concurrently when you combine them; building them is not enough.

**Instructions:** This program fetches two prices sequentially, taking ~160 ms. Rewrite it so both fetches run concurrently and the total wait is ~80 ms. Keep both results.

```rust
use std::time::Instant;
use tokio::time::{sleep, Duration};

async fn fetch_price(symbol: &str, ms: u64) -> f64 {
    sleep(Duration::from_millis(ms)).await;
    println!("got price for {symbol}");
    100.0
}

#[tokio::main]
async fn main() {
    let start = Instant::now();
    let apple = fetch_price("AAPL", 80).await; // TODO: make these
    let google = fetch_price("GOOG", 80).await; // TODO: concurrent
    println!(
        "total {:.1} in ~{} ms",
        apple + google,
        start.elapsed().as_millis()
    );
}
```

<details>
<summary>Solution</summary>

Build both futures and drive them together with `tokio::join!`. (Awaiting them one at a time is sequential precisely because the second future hasn't started yet.)

```rust
use std::time::Instant;
use tokio::time::{sleep, Duration};

async fn fetch_price(symbol: &str, ms: u64) -> f64 {
    sleep(Duration::from_millis(ms)).await;
    println!("got price for {symbol}");
    100.0
}

#[tokio::main]
async fn main() {
    let start = Instant::now();
    // Concurrent: build both futures, then join! drives them together.
    let (apple, google) = tokio::join!(
        fetch_price("AAPL", 80),
        fetch_price("GOOG", 80),
    );
    println!(
        "total {:.1} in ~{} ms",
        apple + google,
        start.elapsed().as_millis()
    );
}
```

Output (the two "got price" lines may appear in either order):

```
got price for GOOG
got price for AAPL
total 200.0 in ~82 ms
```

`join!` is the `Promise.all` analogue; see [Concurrent Awaiting](/11-async/07-select-join/).

</details>

### Exercise 3: Implement `Future` by hand

**Difficulty:** Advanced

**Objective:** Prove to yourself that "lazy" means "polled," by writing a `Future` whose `poll` you control.

**Instructions:** Implement a `PendingOnce` future that returns `Poll::Pending` the first time it is polled and `Poll::Ready("ready now")` the second time. Remember to wake the task before returning `Pending`, or the executor will never poll you again. Await it from `main` and print the result.

```rust
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

struct PendingOnce {
    polled_before: bool,
}

impl Future for PendingOnce {
    type Output = &'static str;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // TODO: Pending first, Ready second; wake before returning Pending.
        todo!()
    }
}

#[tokio::main]
async fn main() {
    let result = PendingOnce { polled_before: false }.await;
    println!("{result}");
}
```

<details>
<summary>Solution</summary>

```rust
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

// Returns Pending the first poll, Ready the second. Demonstrates the poll loop.
struct PendingOnce {
    polled_before: bool,
}

impl Future for PendingOnce {
    type Output = &'static str;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.polled_before {
            Poll::Ready("ready now")
        } else {
            self.polled_before = true;
            cx.waker().wake_by_ref(); // ask the executor to poll us again
            Poll::Pending
        }
    }
}

#[tokio::main]
async fn main() {
    let result = PendingOnce { polled_before: false }.await;
    println!("{result}");
}
```

Output:

```
ready now
```

The `cx.waker().wake_by_ref()` call is essential: returning `Poll::Pending` tells the runtime "not done," and the waker is how you signal "poll me again." Without it, the task would sleep forever. This is the machinery `async`/`await` generates for you automatically, which is why you should almost never write it by hand.

</details>
