---
title: "Concurrent Awaiting: `join!`, `try_join!`, and `select!`"
description: "Rust's join!, try_join!, and select! map to Promise.all and Promise.race, but futures are lazy and select! cancels the losers instead of letting them run."
---

In JavaScript you reach for `Promise.all` to wait for several async operations at once and `Promise.race` to take whichever finishes first. Rust gives you the same shapes — `tokio::join!`, `tokio::try_join!`, and `tokio::select!` — but because Rust **futures are lazy**, *building* the futures is not enough. These macros are what actually drive several futures forward on the same task. This page maps each JavaScript combinator to its Rust equivalent and shows where the analogy breaks down.

---

## Quick Overview

Awaiting futures one after another is **sequential** in Rust: the second future does not even start until the first finishes (see [Promises vs Futures](/11-async/00-promises-vs-futures/)). To run multiple operations *concurrently on one task*, you combine their futures with a macro: **`join!`** waits for all of them (like `Promise.all`), **`try_join!`** waits for all but short-circuits on the first error (like `Promise.all` with rejection), and **`select!`** returns as soon as the *first* one finishes and **cancels the rest** (like `Promise.race`). These run concurrently *without* spawning tasks or threads: a single task interleaves the futures.

> **Note:** This page is about combining futures *on the current task*. If you want them to run on *separate* tasks (true parallelism on a multi-thread runtime, fire-and-forget, `JoinHandle`s), that is [`tokio::spawn`](/11-async/09-spawning-tasks/). For the eager-vs-lazy foundation this all rests on, read [Promises vs Futures](/11-async/00-promises-vs-futures/) first.

---

## TypeScript/JavaScript Example

A typical "load everything a page needs" function fans out several requests and waits for all of them with `Promise.all`. To guard a slow call, you `Promise.race` it against a timeout.

```typescript
// TypeScript / JavaScript (Node v22)

const slow = (ms: number, label: string): Promise<string> =>
  new Promise((resolve) => setTimeout(() => resolve(label), ms));

// Promise.all: run all concurrently, wait for ALL, get an array in order.
const all = await Promise.all([
  slow(120, "user"),
  slow(90, "posts"),
  slow(150, "profile"),
]);
console.log("all:", all);

// Promise.race: first to SETTLE (resolve OR reject) wins; the others keep running.
const winner = await Promise.race([
  slow(150, "primary"),
  slow(80, "replica"),
]);
console.log("race winner:", winner);
```

Real output (Node v22):

```
all: [ 'user', 'posts', 'profile' ]
race winner: replica
```

Two JavaScript subtleties to keep in mind, because they differ in Rust:

- `Promise.all` rejects on the **first** rejection (fail-fast), but the other promises **keep running** in the background; JavaScript has no built-in cancellation.

  ```typescript
  // Promise.all is fail-fast on rejection:
  try {
    await Promise.all([slow(120, "ok"), reject(40, "boom"), slow(150, "ok2")]);
  } catch (e) {
    console.log("all rejected with:", e); // -> "boom"
  }
  ```

- `Promise.race` settles on the first *settled* promise; the losers are **not** cancelled — their timers and side effects continue. This is the single biggest behavioral difference from Rust's `select!`, which **drops** (cancels) the losing futures.

---

## Rust Equivalent

The same fan-out in Rust uses `tokio::join!`. Each `fetch_*` call builds a lazy future; `join!` polls all of them concurrently on the current task and hands back a **tuple** (not an array; the results can have different types).

```rust playground
use std::time::Instant;
use tokio::time::{sleep, Duration};

#[derive(Debug)]
struct User { id: u64, name: String }
#[derive(Debug)]
struct Posts { count: u32 }
#[derive(Debug)]
struct Profile { bio: String }

async fn fetch_user(id: u64) -> User {
    sleep(Duration::from_millis(120)).await;
    User { id, name: format!("user-{id}") }
}

async fn fetch_posts(_id: u64) -> Posts {
    sleep(Duration::from_millis(90)).await;
    Posts { count: 7 }
}

async fn fetch_profile(_id: u64) -> Profile {
    sleep(Duration::from_millis(150)).await;
    Profile { bio: "Rustacean".to_string() }
}

#[tokio::main]
async fn main() {
    let start = Instant::now();

    // Like Promise.all: run all three concurrently, wait for ALL, get a tuple.
    let (user, posts, profile) = tokio::join!(
        fetch_user(42),
        fetch_posts(42),
        fetch_profile(42),
    );

    println!("user: {} (id {})", user.name, user.id);
    println!("posts: {}", posts.count);
    println!("profile: {}", profile.bio);
    println!("all done in ~{} ms", start.elapsed().as_millis());
}
```

Real output (compiled with Rust 1.96, Tokio 1.52):

```
user: user-42 (id 42)
posts: 7
profile: Rustacean
all done in ~152 ms
```

The total is ~150 ms — the duration of the *slowest* branch — not 120 + 90 + 150. That is concurrency: all three timers count down together. And for `Promise.race`, the Rust analogue is `tokio::select!`:

```rust playground
use tokio::time::{sleep, Duration};

async fn primary() -> &'static str {
    sleep(Duration::from_millis(150)).await;
    "primary"
}

async fn replica() -> &'static str {
    sleep(Duration::from_millis(80)).await;
    "replica"
}

#[tokio::main]
async fn main() {
    // Like Promise.race: whichever finishes FIRST wins; the loser is DROPPED.
    let winner = tokio::select! {
        v = primary() => v,
        v = replica() => v,
    };
    println!("answered by: {winner}");
}
```

Real output:

```
answered by: replica
```

> **Note:** These examples use the repository's [pinned verification toolchain](/00-introduction/05-version-policy/), the 2024 edition (selected automatically by `cargo new`), and Tokio 1.x: `cargo add tokio --features full`. See [Setting Up Tokio](/11-async/03-tokio-setup/).

---

## Detailed Explanation

### `join!` — wait for all, keep every result

`tokio::join!(a, b, c)` takes one or more futures, polls them concurrently on the current task, and completes when **all** have finished, returning a tuple `(A, B, C)` of their outputs in argument order.

```rust
async fn a() -> i32 { 1 }
async fn b() -> String { "two".to_string() }
async fn c() -> bool { true }

#[tokio::main]
async fn main() {
    // Heterogeneous results come back as a typed tuple, not an array.
    let (x, y, z): (i32, String, bool) = tokio::join!(a(), b(), c());
    println!("{x} {y} {z}");
}
```

Real output:

```
1 two true
```

This is the first place the JavaScript analogy bends. `Promise.all([a, b, c])` returns an **array**, and TypeScript leans on tuple types to track the (possibly different) element types. Rust's `join!` returns an honest, statically-typed **tuple**: no casting, no `as const`.

> **Tip:** `join!` interleaves futures on *one* task. It does **not** use multiple threads, so it will not speed up CPU-bound work — only IO-bound work where each future spends most of its time waiting. For CPU parallelism, spawn tasks (`tokio::spawn`) on a multi-thread runtime or use threads. See [Async vs Sync](/11-async/13-async-vs-sync/).

### `try_join!` — wait for all, but short-circuit on the first error

When every future returns `Result<T, E>`, `tokio::try_join!` waits for all of them to succeed and gives you `Ok((T1, T2, ...))`. But the moment any future returns `Err`, it stops and returns that error immediately. This is the `Promise.all`-with-rejection pattern, made explicit by Rust's `Result` type.

```rust playground
use std::time::Instant;
use tokio::time::{sleep, Duration};

async fn fetch_user(id: u64) -> Result<String, String> {
    sleep(Duration::from_millis(120)).await;
    Ok(format!("user-{id}"))
}

async fn fetch_posts(_id: u64) -> Result<u32, String> {
    sleep(Duration::from_millis(40)).await;
    Err("posts service is down".to_string()) // fails fast
}

async fn fetch_profile(_id: u64) -> Result<String, String> {
    sleep(Duration::from_millis(150)).await;
    Ok("Rustacean".to_string())
}

#[tokio::main]
async fn main() {
    let start = Instant::now();

    // Short-circuits on the FIRST Err — does not wait for the slow profile fetch.
    let result = tokio::try_join!(
        fetch_user(42),
        fetch_posts(42),
        fetch_profile(42),
    );

    match result {
        Ok((user, posts, profile)) => println!("ok: {user}, {posts} posts, {profile}"),
        Err(e) => println!("failed after ~{} ms: {e}", start.elapsed().as_millis()),
    }
}
```

Real output:

```
failed after ~41 ms: posts service is down
```

It returned after ~40 ms (the moment `fetch_posts` failed) without waiting for the 150 ms profile fetch. **Importantly, the still-running futures are dropped (cancelled).** Unlike JavaScript, where the other promises in a rejected `Promise.all` keep executing, Rust cancels them because `try_join!` simply stops polling them and they go out of scope. All branches must share the **same error type `E`** (or one that the others convert into via `?`/`From`), which is exactly the discipline Rust's `Result` already enforces. See [Section 08: Error Handling](/08-error-handling/).

### `select!` — first to finish wins, the rest are cancelled

`tokio::select!` polls several futures and runs the body of **whichever finishes first**, then **drops all the other futures**. Each arm is `pattern = future => expression`, and `select!` evaluates to the chosen arm's expression, so all arms must produce a compatible type.

```rust playground
use tokio::time::{sleep, Duration};

async fn from_cache() -> Option<String> {
    sleep(Duration::from_millis(120)).await;
    Some("cached value".to_string())
}

async fn from_origin() -> Option<String> {
    sleep(Duration::from_millis(50)).await;
    Some("origin value".to_string())
}

#[tokio::main]
async fn main() {
    // First arm to complete provides the value; the other future is dropped.
    let value = tokio::select! {
        v = from_cache() => v,
        v = from_origin() => v,
    };
    println!("served: {value:?}");
}
```

Real output:

```
served: Some("origin value")
```

The pattern position (`v = ...`) can destructure, which lets a `select!` arm fire only on a *matching* result. A common idiom is `Some(x) = stream_or_channel.recv() => { ... }`, which simply skips that arm (and considers it permanently disabled if all arms become disabled) when the future yields a non-matching value.

### `select!` actually *cancels* the losers — and that has consequences

This is the headline difference from `Promise.race`. When a `select!` arm wins, the other futures are **dropped mid-flight**: any work they had not yet completed never runs. The following makes that visible. Each branch logs every step it completes:

```rust playground
use tokio::time::{sleep, Duration};

async fn step_writer(label: &str, steps: u32) -> &str {
    for i in 1..=steps {
        sleep(Duration::from_millis(40)).await;
        println!("  {label}: step {i}");
    }
    label
}

#[tokio::main]
async fn main() {
    let winner = tokio::select! {
        v = step_writer("fast", 2) => v,
        v = step_writer("slow", 10) => v,
    };
    // The "slow" future is DROPPED partway through — steps 3..=10 never run.
    println!("winner: {winner}");
}
```

Real output:

```
  slow: step 1
  fast: step 1
  slow: step 2
  fast: step 2
winner: fast
```

`slow` got through step 2, then `fast` finished and won, and `slow`'s remaining eight steps **never executed**. In JavaScript, the losing promise would have run all ten "steps" to completion regardless. This cancellation is a feature (it is how timeouts and graceful shutdown work) but it means a `select!` branch can be interrupted at any `.await` point. That property is called **cancellation safety**; not every future is safe to cancel mid-operation, so consult a type's docs before racing it.

### `select!` in a loop — the async event loop

Because `select!` returns after a single event, it is almost always used inside a `loop` to build an event-driven task that reacts to whichever source is ready: a channel message, a timer tick, a shutdown signal. This is the closest Rust comes to Node's "one event loop handling many sources."

```rust playground
use tokio::sync::{mpsc, oneshot};
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    let (work_tx, mut work_rx) = mpsc::channel::<u32>(8);
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

    // Producer: send three jobs, then signal shutdown.
    tokio::spawn(async move {
        for i in 1..=3 {
            work_tx.send(i).await.unwrap();
            sleep(Duration::from_millis(20)).await;
        }
        sleep(Duration::from_millis(20)).await;
        let _ = shutdown_tx.send(());
    });

    let mut processed = 0u32;
    loop {
        tokio::select! {
            // Fires when a job arrives; the `Some(...)` pattern disables this
            // arm if the channel closes (recv() returns None).
            Some(job) = work_rx.recv() => {
                processed += 1;
                println!("processing job {job}");
            }
            // Fires once, when the shutdown signal is sent.
            _ = &mut shutdown_rx => {
                println!("shutdown signal received; processed {processed} jobs");
                break;
            }
        }
    }
}
```

Real output:

```
processing job 1
processing job 2
processing job 3
shutdown signal received; processed 3 jobs
```

Note `&mut shutdown_rx`: the `oneshot::Receiver` is awaited *by mutable reference* so it survives across loop iterations rather than being moved (and dropped) on the first poll. The channels here are covered in [Async Channels](/11-async/08-channels/).

### `biased` — opt out of random polling order

By default, `select!` checks its arms in a **random** order on each poll to avoid starving any branch. When you want a deterministic priority (for example, "always drain pending work before checking the timer"), add `biased;` as the first line; arms are then polled top-to-bottom.

```rust playground
use tokio::sync::mpsc;

#[tokio::main]
async fn main() {
    let (tx, mut rx) = mpsc::unbounded_channel::<u32>();
    for i in 1..=3 { tx.send(i).unwrap(); }
    drop(tx);

    let mut count = 0;
    loop {
        tokio::select! {
            biased; // poll arms in written order, not randomly

            maybe = rx.recv() => match maybe {
                Some(n) => { count += 1; println!("drained {n}"); }
                None => break,
            }
        }
    }
    println!("drained {count} items");
}
```

Real output:

```
drained 1
drained 2
drained 3
drained 3 items
```

### `join_all` / `try_join_all` — when the count is dynamic

`join!` and `try_join!` are macros with a *fixed* number of arms known at compile time. When you have a **`Vec` of futures** whose length is decided at runtime (the most common real-world case), use the `futures` crate's `join_all` / `try_join_all`, which take an iterator of futures and return a `Vec` of results.

```rust playground
use futures::future::join_all;
use tokio::time::{sleep, Duration};

async fn fetch(id: u32) -> u32 {
    sleep(Duration::from_millis(50)).await;
    id * 10
}

#[tokio::main]
async fn main() {
    let ids = vec![1, 2, 3, 4, 5];

    // Promise.all over a dynamic array: build a Vec of futures, drive them together.
    let futures = ids.into_iter().map(fetch);
    let results: Vec<u32> = join_all(futures).await;

    println!("{results:?}");
}
```

Real output:

```
[10, 20, 30, 40, 50]
```

`try_join_all` is the fail-fast variant: it returns `Result<Vec<T>, E>`, short-circuiting on the first `Err` just like `try_join!`.

```rust playground
use futures::future::try_join_all;
use tokio::time::{sleep, Duration};

async fn fetch(id: u32) -> Result<u32, String> {
    sleep(Duration::from_millis(30)).await;
    if id == 3 { Err(format!("id {id} not found")) } else { Ok(id * 10) }
}

#[tokio::main]
async fn main() {
    let ids = vec![1, 2, 3, 4];
    let result: Result<Vec<u32>, String> = try_join_all(ids.into_iter().map(fetch)).await;
    println!("{result:?}");
}
```

Real output:

```
Err("id 3 not found")
```

> **Tip:** `join_all` runs everything concurrently *but still on one task* and unboundedly. If you have 10,000 futures you usually want bounded concurrency instead. Reach for [`futures::stream::StreamExt::buffer_unordered`](/11-async/06-streams/) (limit in-flight futures) or spawn a bounded number of [tasks](/11-async/09-spawning-tasks/) guarded by a [`Semaphore`](/11-async/11-sync-primitives/).

---

## Key Differences

| Concept | JavaScript | Rust (Tokio) | Notes |
| --- | --- | --- | --- |
| Wait for all | `Promise.all([...])` → array | `join!(a, b)` → tuple; `join_all(vec)` → `Vec` | Tuple keeps distinct types; macro arms are fixed-count |
| Wait for all, fail fast | `Promise.all` (rejects on first) | `try_join!` / `try_join_all` | All arms share one error type `E` |
| First to finish | `Promise.race([...])` | `select! { ... }` | `select!` **cancels losers**; `race` does not |
| First to *succeed* | `Promise.any([...])` | no direct macro; loop/`select!` or `select_ok` | See pitfalls |
| All settled (no fail) | `Promise.allSettled([...])` | `join!` of `Result`-returning futures | Each tuple slot is its own `Result` |
| Losers after winner | keep running | **dropped / cancelled** | The defining difference |
| Concurrency model | built-in event loop, microtasks | one task interleaves futures via `poll` | No threads unless you spawn |
| Eager vs lazy | promises already running | futures do nothing until the macro polls them | See [Promises vs Futures](/11-async/00-promises-vs-futures/) |

### Why these are macros, not functions

`join!`, `try_join!`, and `select!` are **macros** because they need to expand into a single state machine that owns and polls every branch *in place*, on the current task, without heap allocation or boxing. A plain function could not accept a variable list of differently-typed futures and unwrap them into a tuple while polling them together. The macro generates the `poll` loop that drives all branches and wakes correctly when any one of them is ready.

### `select!` is not `Promise.race` — it cancels

Internalize this: **`Promise.race` lets the losers run; `select!` drops them.** That makes `select!` the right tool for timeouts and cancellation (the loser *should* stop), but a hazard when a losing branch was halfway through something that must not be abandoned (a partial database write, a half-consumed message). If a branch is not cancellation-safe, do its work on a [spawned task](/11-async/09-spawning-tasks/) and `select!` on the task's `JoinHandle` instead, so the work continues even if `select!` moves on.

---

## Common Pitfalls

### Pitfall 1: Reusing a future after it was moved into `select!`/`join!`

These macros **take ownership** of the futures you pass in. A future is not `Copy`, so you cannot use it again afterward: the loser of a `select!` is gone.

```rust
use tokio::time::{sleep, Duration};

async fn work(label: &str) -> &str {
    sleep(Duration::from_millis(50)).await;
    label
}

#[tokio::main]
async fn main() {
    let a = work("a");
    let b = work("b");

    let first = tokio::select! {
        v = a => v,
        v = b => v,
    };
    println!("first: {first}");

    let second = b.await; // does not compile (error[E0382]: use of moved value: `b`)
    println!("second: {second}");
}
```

Real compiler error:

```
error[E0382]: use of moved value: `b`
  --> src/main.rs:20:18
   |
11 |     let b = work("b");
   |         - move occurs because `b` has type `impl Future<Output = &str>`, which does not implement the `Copy` trait
...
15 |         v = b => v,
   |             - value moved here
...
20 |     let second = b.await;
   |                  ^ value used here after move
```

If you genuinely need to keep awaiting a branch across iterations (the event-loop case), pass it **by mutable reference** — `v = &mut b => ...` — and ensure it is `Unpin` or pinned (e.g. `tokio::pin!(b)`), as shown with `&mut shutdown_rx` earlier.

### Pitfall 2: Expecting `select!`'s losers to keep running (the `Promise.race` mental model)

A JavaScript developer naturally assumes the slow branch finishes "in the background." It does not — it is cancelled. If you wrote a `select!` to *kick off* two operations and only *report* the first, but you actually needed both to complete, `select!` is the wrong tool.

```rust
// Compiles and runs, but the slow branch is CANCELLED, not backgrounded.
let _first = tokio::select! {
    v = important_write_then_value() => v, // if this loses, the write never happens!
    v = fast_value() => v,
};
```

If both operations matter, spawn them as tasks and either `join!` their handles or `select!` on the handles (so the loser keeps making progress on its own task). See [Spawning Tasks](/11-async/09-spawning-tasks/).

### Pitfall 3: Blocking inside a branch stalls *every* branch

`join!`/`select!` interleave futures on **one** task, so a synchronous, blocking call inside any arm (a `std::thread::sleep`, a CPU-heavy loop, blocking file IO) freezes *all* the concurrent branches, and on a current-thread runtime, the whole runtime. The fix is to keep arms `.await`-friendly and offload blocking work with [`tokio::task::spawn_blocking`](/11-async/09-spawning-tasks/), or use the async timer `tokio::time::sleep` rather than `std::thread::sleep`. This is a *runtime-stall* trap, not a compile error, which makes it easy to miss. See [Async vs Sync](/11-async/13-async-vs-sync/).

### Pitfall 4: There is no built-in `Promise.any`/`Promise.allSettled` macro

Rust's three macros do not cover *every* JavaScript combinator one-to-one:

- **`Promise.any`** (first to *succeed*, ignore failures): there is no `try_any!` macro. Use `futures::future::select_ok` for a collection, or a `select!`/loop that keeps going until an arm yields `Ok`.
- **`Promise.allSettled`** (wait for all, never fail, collect every outcome): use `join!`/`join_all` over futures that each return `Result<T, E>`; you get a tuple/`Vec` of `Result`s, each independently `Ok` or `Err`.

Reaching for a non-existent macro and then assuming `try_join!` "ignores errors" (it does the opposite: it fails fast) is a common early mistake.

---

## Best Practices

- **Use `join!` for "all of these, keep going on success."** When every branch returns a plain value (or you want every outcome regardless of error), `join!` is clearest. Use `try_join!` only when a single failure should abort the whole group.
- **Prefer `tokio::time::timeout(dur, fut)` over hand-rolling a `select!` timeout.** It is the idiomatic, readable way to bound a single future and returns `Result<T, Elapsed>`. Reserve a `select!` timeout branch for when you are already racing multiple things.
- **Wrap `select!` in a `loop` for long-lived, event-driven tasks** (servers, supervisors), and add a dedicated shutdown branch (a `oneshot`/`watch` channel) so the loop can exit cleanly. See [Async Channels](/11-async/08-channels/).
- **Reach for `join_all`/`try_join_all` when the future count is dynamic**, and bound the concurrency (`buffer_unordered`, a `Semaphore`) when the count can be large.
- **Know your cancellation safety.** Before putting a future in a `select!` arm, ask "is it OK if this is dropped at an `.await`?" If not, move it onto a spawned task and select on the `JoinHandle`.
- **Don't fake concurrency with sequential `.await`s.** Two `fut_a.await; fut_b.await;` run one after another. If you wanted them concurrent, that is precisely what these macros are for. See [Promises vs Futures](/11-async/00-promises-vs-futures/).

---

## Real-World Example

A pricing service queries several regional mirrors and uses the **fastest** response (a `select!` race with an overall deadline), while *simultaneously* fetching the current FX rate it needs to convert the price (a `join!` of the race against the rate fetch). This combines `select!` and `join!` in one realistic flow.

```rust playground
use std::time::Instant;
use tokio::time::{sleep, Duration};

#[derive(Debug)]
struct Quote { mirror: &'static str, price_cents: u64 }
#[derive(Debug)]
struct ExchangeRate { usd_to_eur: f64 }

/// One mirror's quote; different regions have different latencies.
async fn quote_from(mirror: &'static str, latency_ms: u64, price: u64) -> Quote {
    sleep(Duration::from_millis(latency_ms)).await;
    Quote { mirror, price_cents: price }
}

/// A separate dependency we also need.
async fn fetch_rate() -> ExchangeRate {
    sleep(Duration::from_millis(70)).await;
    ExchangeRate { usd_to_eur: 0.92 }
}

/// Race the mirrors (Promise.race style) but cap the whole thing with a deadline.
async fn fastest_quote() -> Option<Quote> {
    tokio::select! {
        q = quote_from("us-east", 130, 19_900) => Some(q),
        q = quote_from("eu-west", 60, 20_050) => Some(q),
        q = quote_from("ap-south", 200, 19_800) => Some(q),
        _ = sleep(Duration::from_millis(150)) => None, // deadline arm
    }
}

#[tokio::main]
async fn main() {
    let start = Instant::now();

    // Promise.all style: race the mirrors AND fetch the FX rate concurrently.
    let (quote, rate) = tokio::join!(fastest_quote(), fetch_rate());

    match quote {
        Some(q) => {
            let eur = q.price_cents as f64 / 100.0 * rate.usd_to_eur;
            println!(
                "best quote ${:.2} from {} -> {:.2} EUR (in ~{} ms)",
                q.price_cents as f64 / 100.0,
                q.mirror,
                eur,
                start.elapsed().as_millis()
            );
        }
        None => println!("all mirrors missed the 150 ms deadline"),
    }
}
```

Real output:

```
best quote $200.50 from eu-west -> 184.46 EUR (in ~72 ms)
```

The fastest mirror (`eu-west`, 60 ms) won the `select!` race; the slower mirrors were cancelled by the deadline-aware race; and because the rate fetch (70 ms) ran concurrently via `join!`, the whole operation finished in ~72 ms instead of 60 + 70 = 130 ms. This is exactly the kind of layered concurrency `Promise.race` inside `Promise.all` gives you in JavaScript, but with cancellation of the losing mirrors thrown in for free.

> **Note:** Real network calls would return `Result`, so you would likely use `try_join!` for the combine step and propagate errors with `?` (see [Async/Await Syntax](/11-async/01-async-await/) and [Section 08: Error Handling](/08-error-handling/)). Sleeps stand in for IO here to keep the example self-contained.

---

## Further Reading

### Official Documentation

- [`tokio::join!`](https://docs.rs/tokio/latest/tokio/macro.join.html): wait for all
- [`tokio::try_join!`](https://docs.rs/tokio/latest/tokio/macro.try_join.html) — wait for all, fail fast
- [`tokio::select!`](https://docs.rs/tokio/latest/tokio/macro.select.html): first to finish wins, the rest are cancelled
- [Tokio Tutorial — `select`](https://tokio.rs/tokio/tutorial/select) — cancellation safety, the `loop { select! }` pattern
- [`futures::future::join_all`](https://docs.rs/futures/latest/futures/future/fn.join_all.html) / [`try_join_all`](https://docs.rs/futures/latest/futures/future/fn.try_join_all.html): dynamic collections of futures
- [`tokio::time::timeout`](https://docs.rs/tokio/latest/tokio/time/fn.timeout.html) — the idiomatic single-future timeout

### Related Sections in This Guide

- [Promises vs Futures](/11-async/00-promises-vs-futures/): eager vs lazy; why building futures isn't enough
- [Async/Await Syntax](/11-async/01-async-await/) — `async`/`await` syntax and `?` error handling
- [The Tokio Runtime](/11-async/02-tokio-intro/) and [Setting Up Tokio](/11-async/03-tokio-setup/): adding and configuring the runtime
- [Streams](/11-async/06-streams/) — `buffer_unordered` for bounded concurrency over many futures
- [Async Channels](/11-async/08-channels/): `mpsc`/`oneshot`/`watch`, the sources you `select!` on
- [Spawning Tasks](/11-async/09-spawning-tasks/) — `tokio::spawn`, `JoinHandle`, `spawn_blocking`; run branches on separate tasks
- [Concurrency vs Parallelism](/11-async/10-concurrency/): concurrency vs parallelism; structured patterns; cancellation
- [Async vs Sync](/11-async/13-async-vs-sync/) — why blocking inside a branch stalls everything
- [Section 00: Introduction](/00-introduction/), [Section 01: Getting Started](/01-getting-started/), [Section 02: Basics](/02-basics/)
- [Section 08: Error Handling](/08-error-handling/): the `Result`/`?` model `try_join!` relies on
- [Section 12: Modules & Packages](/12-modules-packages/) — how `tokio` and `futures` crates are added

---

## Exercises

### Exercise 1: From sequential to concurrent with `join!`

**Difficulty:** Beginner

**Objective:** Replace two sequential `.await`s with a `join!` so both run concurrently.

**Instructions:** The program below fetches weather and news one after another, taking ~200 ms. Rewrite it so both run concurrently (~100 ms total) and keep both results. Do not change the two `async fn`s.

```rust playground
use std::time::Instant;
use tokio::time::{sleep, Duration};

async fn fetch_weather(city: &str) -> String {
    sleep(Duration::from_millis(100)).await;
    format!("{city}: sunny")
}

async fn fetch_news(city: &str) -> String {
    sleep(Duration::from_millis(100)).await;
    format!("{city}: all quiet")
}

#[tokio::main]
async fn main() {
    let start = Instant::now();
    let weather = fetch_weather("Berlin").await; // TODO: make these
    let news = fetch_news("Berlin").await;        // TODO: concurrent
    println!("{weather}");
    println!("{news}");
    println!("done in ~{} ms", start.elapsed().as_millis());
}
```

<details>
<summary>Solution</summary>

Combine the two futures with `tokio::join!`. Building them is not enough. The macro is what drives them together.

```rust playground
use std::time::Instant;
use tokio::time::{sleep, Duration};

async fn fetch_weather(city: &str) -> String {
    sleep(Duration::from_millis(100)).await;
    format!("{city}: sunny")
}

async fn fetch_news(city: &str) -> String {
    sleep(Duration::from_millis(100)).await;
    format!("{city}: all quiet")
}

#[tokio::main]
async fn main() {
    let start = Instant::now();
    let (weather, news) = tokio::join!(
        fetch_weather("Berlin"),
        fetch_news("Berlin"),
    );
    println!("{weather}");
    println!("{news}");
    println!("done in ~{} ms", start.elapsed().as_millis());
}
```

Output:

```
Berlin: sunny
Berlin: all quiet
done in ~102 ms
```

</details>

### Exercise 2: Bound a slow operation with a timeout

**Difficulty:** Intermediate

**Objective:** Use `select!` to race work against a timer so a slow operation cannot hang forever.

**Instructions:** `build_report` takes 300 ms. Use a `tokio::select!` with a 150 ms timer branch so that if the report is not ready in time, the program prints `report timed out` instead of waiting. Then, in the solution, note the idiomatic one-liner alternative.

```rust playground
use tokio::time::{sleep, Duration};

async fn build_report() -> String {
    sleep(Duration::from_millis(300)).await;
    "report ready".to_string()
}

#[tokio::main]
async fn main() {
    // TODO: race build_report() against a 150 ms timer with select!
    let report = build_report().await;
    println!("{report}");
}
```

<details>
<summary>Solution</summary>

Add a timer arm; whichever finishes first wins, and the loser is cancelled.

```rust playground
use tokio::time::{sleep, Duration};

async fn build_report() -> String {
    sleep(Duration::from_millis(300)).await;
    "report ready".to_string()
}

#[tokio::main]
async fn main() {
    let result = tokio::select! {
        report = build_report() => Ok(report),
        _ = sleep(Duration::from_millis(150)) => Err("report timed out"),
    };

    match result {
        Ok(r) => println!("{r}"),
        Err(e) => println!("{e}"),
    }
}
```

Output:

```
report timed out
```

The idiomatic shortcut for "bound a single future with a timeout" is `tokio::time::timeout`, which does exactly this and returns `Result<T, Elapsed>`:

```rust playground
use tokio::time::{sleep, timeout, Duration};

async fn build_report() -> String {
    sleep(Duration::from_millis(300)).await;
    "report ready".to_string()
}

#[tokio::main]
async fn main() {
    match timeout(Duration::from_millis(150), build_report()).await {
        Ok(report) => println!("{report}"),
        Err(_elapsed) => println!("report timed out"),
    }
}
```

Output:

```
report timed out
```

</details>

### Exercise 3: A `select!` loop with graceful shutdown

**Difficulty:** Advanced

**Objective:** Build an event-driven task that processes jobs from a channel and stops cleanly on a shutdown signal.

**Instructions:** A producer sends three jobs on an `mpsc` channel, then fires a `oneshot` shutdown signal. Write a `loop { tokio::select! { ... } }` that processes each job as it arrives and `break`s when the shutdown signal fires, printing how many jobs it processed. Remember that the `oneshot` receiver must be awaited by `&mut` so it survives across iterations.

```rust playground
use tokio::sync::{mpsc, oneshot};
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    let (work_tx, mut work_rx) = mpsc::channel::<u32>(8);
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

    tokio::spawn(async move {
        for i in 1..=3 {
            work_tx.send(i).await.unwrap();
            sleep(Duration::from_millis(20)).await;
        }
        sleep(Duration::from_millis(20)).await;
        let _ = shutdown_tx.send(());
    });

    let mut processed = 0u32;
    // TODO: loop { select! { job arm; shutdown arm } }
}
```

<details>
<summary>Solution</summary>

```rust playground
use tokio::sync::{mpsc, oneshot};
use tokio::time::{sleep, Duration};

#[tokio::main]
async fn main() {
    let (work_tx, mut work_rx) = mpsc::channel::<u32>(8);
    let (shutdown_tx, mut shutdown_rx) = oneshot::channel::<()>();

    tokio::spawn(async move {
        for i in 1..=3 {
            work_tx.send(i).await.unwrap();
            sleep(Duration::from_millis(20)).await;
        }
        sleep(Duration::from_millis(20)).await;
        let _ = shutdown_tx.send(());
    });

    let mut processed = 0u32;
    loop {
        tokio::select! {
            // `Some(...)` pattern: this arm disables itself if the channel closes.
            Some(job) = work_rx.recv() => {
                processed += 1;
                println!("processing job {job}");
            }
            // `&mut` so the receiver isn't moved/dropped on the first poll.
            _ = &mut shutdown_rx => {
                println!("shutdown signal received; processed {processed} jobs");
                break;
            }
        }
    }
}
```

Output:

```
processing job 1
processing job 2
processing job 3
shutdown signal received; processed 3 jobs
```

This is the canonical long-lived async task: react to whichever source is ready, and exit cleanly on a shutdown channel. The channel types are covered in [Async Channels](/11-async/08-channels/).

</details>
