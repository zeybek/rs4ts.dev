---
title: "Spawning Tasks"
description: "JavaScript never spawns; Rust's tokio::spawn runs a future as an independent task on a thread pool. Use JoinHandle, abort, and spawn_blocking for real parallelism."
---

In JavaScript you never "spawn" anything — every async operation runs on the one event loop, and "concurrency" is just interleaving on a single thread. Rust's Tokio gives you `tokio::spawn`, which hands a future to the runtime as an independent **task** that can run *in parallel* on a thread pool. This page is about spawning those tasks, awaiting their results through a `JoinHandle`, how tasks differ from OS threads, and when to reach for `spawn_blocking`.

---

## Quick Overview

A **task** is Tokio's unit of concurrency: a future that the runtime owns and drives independently, like a green thread. `tokio::spawn` starts one immediately and returns a `JoinHandle<T>` you can `.await` to get its result. Tasks are far cheaper than OS threads and, on a multi-thread runtime, can run truly in parallel, but CPU-heavy or blocking synchronous work must go to `spawn_blocking` so it does not stall the async workers.

> **Note:** This page assumes you already have a runtime wired up. If `#[tokio::main]` and the laziness of futures are new to you, read [Setting Up Tokio](/11-async/03-tokio-setup/) and [Promises vs Futures](/11-async/00-promises-vs-futures/) first.

---

## TypeScript/JavaScript Example

In Node.js there is no spawning. You start async operations and they run concurrently on the single event loop; you compose their results with `Promise.all` or by awaiting them. "Background work" is just a promise you do not await yet:

```typescript
// A "background task" in Node is just an un-awaited promise.
async function fetchUser(id: number): Promise<string> {
  // Stand-in for a DB/network call.
  await new Promise((resolve) => setTimeout(resolve, 50));
  return `user-${id}`;
}

async function main(): Promise<void> {
  // Kick off five fetches "concurrently". They interleave on ONE thread —
  // the event loop. There is no parallelism here; CPU work would block them all.
  const promises = [1, 2, 3, 4, 5].map((id) => fetchUser(id));

  console.log("all fetches kicked off, event loop continues");

  // Promise.all gathers the results.
  const users = await Promise.all(promises);
  console.log(users);
}

main();
```

There is one important limitation hiding here: because Node runs your JavaScript on a single thread, a CPU-bound function (say, hashing a megabyte synchronously) **blocks the entire event loop**: every other "concurrent" promise stalls until it returns. Node's answer is `worker_threads`, a separate, heavier mechanism. Keep that distinction in mind; Rust splits the same problem into `tokio::spawn` (concurrency) and `spawn_blocking` (offloading blocking work).

---

## Rust Equivalent

`tokio::spawn` takes a future and hands it to the runtime as a task. It returns a `JoinHandle<T>`, roughly the analogue of the `Promise` you would have gotten back from an async call, except the work is now an independently scheduled task:

```rust
use tokio::task::JoinHandle;

#[tokio::main]
async fn main() {
    // spawn returns a JoinHandle<u64> immediately; the task starts running
    // concurrently right away (unlike a bare future, which is lazy).
    let handle: JoinHandle<u64> = tokio::spawn(async {
        // pretend this is a network call or heavy async work
        let mut total = 0u64;
        for i in 1..=100 {
            total += i;
        }
        total
    });

    // Do other work here while the task runs...
    println!("task spawned, main continues");

    // .await on the handle yields the task's output, wrapped in Result.
    let sum = handle.await.expect("the task panicked");
    println!("sum = {sum}");
}
```

Real output:

```
task spawned, main continues
sum = 5050
```

The shape mirrors `const p = fetchUser(1); const u = await p;` in JavaScript — but with two differences that matter: spawning a task makes it eligible to run on **any** worker thread (real parallelism), and `handle.await` returns a `Result`, because the task could have panicked. We will unpack both below.

To fetch many things concurrently, spawn in a loop and collect the handles:

```rust
use std::time::Duration;
use tokio::time::sleep;

async fn fetch_user(id: u32) -> String {
    sleep(Duration::from_millis(50)).await; // stands in for a DB/network call
    format!("user-{id}")
}

#[tokio::main]
async fn main() {
    let mut handles = Vec::new();
    for id in 1..=5 {
        // Each spawned task runs concurrently. The loop does not block.
        handles.push(tokio::spawn(fetch_user(id)));
    }

    // Await each handle to collect the results.
    let mut users = Vec::new();
    for handle in handles {
        users.push(handle.await.unwrap());
    }

    println!("{users:?}");
}
```

Real output:

```
["user-1", "user-2", "user-3", "user-4", "user-5"]
```

This is the rough equivalent of the `Promise.all` example above — five 50 ms sleeps overlap, so the whole thing finishes in about 50 ms.

> **Tip:** For "fetch a fixed set of things and await all of them," [`join!`/`try_join!`](/11-async/07-select-join/) are often cleaner than spawning, because they run the futures concurrently *without* requiring `Send + 'static`. Reach for `spawn` when you need tasks to outlive the current scope, run on other threads, or be cancellable independently.

---

## Detailed Explanation

### What `tokio::spawn` returns and when the task starts

`tokio::spawn(future)` does two things synchronously: it registers the future with the current runtime as a task, and it returns a `JoinHandle<T>` where `T` is the future's output type. Importantly, **the task is now scheduled and will make progress on its own**, even before you await the handle.

This is a subtle but important contrast with a bare future. A plain `async { ... }` block does nothing until something polls it (see [Promises vs Futures](/11-async/00-promises-vs-futures/)). Once you `spawn` it, the runtime is the thing polling it, so a spawned task behaves much more like an eager JavaScript promise. You can think of `tokio::spawn` as the operation that converts a lazy future into an actively-running task.

### Awaiting the handle yields a `Result`

`handle.await` produces a `Result<T, JoinError>`, not just `T`. The `Err` case exists because a task is isolated: if its future panics, the runtime catches the panic and reports it back through the handle instead of unwinding your whole program.

```rust
#[tokio::main]
async fn main() {
    let handle = tokio::spawn(async {
        panic!("something broke inside the task");
    });

    match handle.await {
        Ok(()) => println!("task finished cleanly"),
        Err(join_err) => {
            // The runtime catches the panic; the rest of the program survives.
            println!("task failed: is_panic = {}", join_err.is_panic());
        }
    }

    println!("main is still alive");
}
```

Real output:

```

thread 'tokio-rt-worker' panicked at src/main.rs:4:9:
something broke inside the task
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
task failed: is_panic = true
main is still alive
```

Note the panic message is still printed (Rust's default panic hook runs), but `main` keeps going because the panic was contained in the task. In JavaScript, an unhandled rejection in one promise does not crash unrelated promises either — but the *mechanism* is different: here a thread genuinely panicked and the runtime turned that into a `JoinError`.

### The `'static` and `Send` bounds

The signature of `tokio::spawn` is, simplified:

```rust
pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
```

Two bounds drive almost every error you will hit:

- **`'static`**: a spawned task may outlive the function that created it, so it cannot borrow local variables. You must move owned data in (with `async move`) or share it via `Arc`.
- **`Send`**: on a multi-thread runtime, a task can be moved between worker threads, so its future (and every value held across an `.await`) must be safe to send across threads.

These are the compile-time price of real parallelism. JavaScript needs neither because everything lives on one thread and the closure captures by reference into the same heap. Both bounds are shown as real compiler errors under [Common Pitfalls](#common-pitfalls).

### Detaching: dropping the handle does *not* cancel the task

Unlike some task systems, dropping a `JoinHandle` does **not** stop the task; it simply detaches it, and the task runs to completion in the background. You just lose the ability to await its result.

```rust
use tokio::sync::oneshot;

#[tokio::main]
async fn main() {
    let (tx, rx) = oneshot::channel();

    // We spawn and immediately drop the JoinHandle (the `_` discards it).
    let _ = tokio::spawn(async move {
        // This still runs to completion despite no handle being held.
        let _ = tx.send("background work finished");
    });

    // Prove the task ran by receiving its message.
    let msg = rx.await.expect("sender dropped");
    println!("{msg}");
}
```

Real output:

```
background work finished
```

> **Warning:** Detached tasks are the Rust equivalent of "fire and forget" promises. They keep running, but they are *not* awaited at shutdown. If `main` (or the runtime) ends first, in-flight detached tasks are dropped mid-flight. Keep handles (or use a [`JoinSet`](#use-a-joinset-for-dynamic-groups-of-tasks)) when you need to wait for completion.

### Cancelling with `abort`

To actually stop a task, call `abort()` on its handle. The task is cancelled at its next `.await` point, and awaiting the handle then returns a `JoinError` for which `is_cancelled()` is true:

```rust
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() {
    let handle = tokio::spawn(async {
        sleep(Duration::from_secs(60)).await; // long-running
        "done"
    });

    // Decide we no longer need the result and cancel the task.
    handle.abort();

    match handle.await {
        Ok(value) => println!("completed: {value}"),
        Err(e) if e.is_cancelled() => println!("task was cancelled"),
        Err(e) => println!("task failed: {e}"),
    }
}
```

Real output:

```
task was cancelled
```

JavaScript has no built-in promise cancellation (you reach for `AbortController`); Tokio bakes cancellation into the handle. Cancellation semantics in depth — what "cancel at an await point" really means — live in [Concurrency vs Parallelism](/11-async/10-concurrency/).

### `spawn_blocking`: the escape hatch for synchronous work

Async tasks are **cooperative**: a task only yields control at an `.await`. If a task runs a long synchronous computation or calls a blocking API (a CPU-bound loop, `std::thread::sleep`, a synchronous file or database call), it never yields, and it starves every other task sharing that worker thread. This is the same failure mode as blocking Node's event loop, but Tokio gives you a first-class fix.

`tokio::task::spawn_blocking` moves a synchronous closure onto a **separate, dedicated thread pool** reserved for blocking work, so the async workers stay free:

```rust
use tokio::task;

// A CPU-bound, synchronous function. It blocks the thread it runs on.
fn fibonacci(n: u64) -> u64 {
    if n < 2 { n } else { fibonacci(n - 1) + fibonacci(n - 2) }
}

#[tokio::main]
async fn main() {
    // spawn_blocking moves this work to a dedicated blocking-thread pool,
    // so it does NOT stall the async worker threads.
    let handle = task::spawn_blocking(|| {
        // Heavy synchronous computation lives here.
        fibonacci(35)
    });

    // Meanwhile, async work on the runtime keeps making progress.
    println!("computing fib(35) on a blocking thread...");

    let result = handle.await.unwrap();
    println!("fib(35) = {result}");
}
```

Real output:

```
computing fib(35) on a blocking thread...
fib(35) = 9227465
```

`spawn_blocking` returns a `JoinHandle<T>` just like `spawn`, so you `.await` it the same way. The closure is `FnOnce` and runs once on a blocking thread. This is the analogue of offloading work to a Node `worker_thread`, but with far less ceremony.

> **Note:** `spawn_blocking` is for *blocking* work (CPU-bound loops, synchronous I/O), not for parallelizing async work. For genuine CPU-bound parallelism across cores, a thread pool like [`rayon`](https://docs.rs/rayon) is often the better tool. See [Async vs Sync](/11-async/13-async-vs-sync/).

---

## Tasks vs OS Threads

It is worth being precise, because "task" and "thread" are casually conflated. Rust gives you both: `std::thread::spawn` for OS threads and `tokio::spawn` for async tasks.

An OS thread:

```rust
use std::thread;

fn main() {
    // An OS thread: ~megabytes of stack, scheduled by the kernel.
    let handle = thread::spawn(|| {
        let mut sum = 0u64;
        for i in 1..=100 { sum += i; }
        sum
    });

    // join() blocks the current thread until the spawned one finishes.
    let result = handle.join().expect("thread panicked");
    println!("sum = {result}");
}
```

Real output:

```
sum = 5050
```

The APIs look almost identical (`spawn` → handle → `join`/`await`), but the machinery underneath is very different:

| Aspect | OS thread (`std::thread::spawn`) | Async task (`tokio::spawn`) |
| --- | --- | --- |
| Created by | The operating system kernel | The Tokio runtime (in user space) |
| Stack | Large, fixed (often ~2–8 MB) | Tiny; grows from the heap as the state machine needs |
| Cost to create | Relatively expensive (syscall) | Very cheap (an allocation + queue push) |
| How many feasible | Thousands | Hundreds of thousands to millions |
| Scheduling | Pre-emptive, by the kernel | Cooperative, at `.await` points, by Tokio |
| Blocking is fine? | Yes — that is what threads are for | No — blocks the worker; use `spawn_blocking` |
| Get the result | `handle.join()` (blocks) | `handle.await` (yields) |
| Best for | CPU-bound work, blocking calls | I/O-bound concurrency at scale |

The mental model: a task is a lightweight, cooperatively-scheduled job that the runtime multiplexes onto a small pool of OS threads. You can have a million tasks running on, say, eight threads. You cannot have a million OS threads; you would exhaust memory on stacks alone. This is exactly why network servers (which juggle huge numbers of mostly-idle connections) are built on tasks, not threads.

> **Tip:** Choosing between async tasks, OS threads, and `spawn_blocking` is the central decision of [Async vs Sync](/11-async/13-async-vs-sync/). The short version: I/O-bound and high-concurrency → tasks; CPU-bound → threads / `rayon` / `spawn_blocking`.

---

## Key Differences

| Concept | JavaScript / Node.js | Rust + Tokio |
| --- | --- | --- |
| Unit of concurrency | A promise on the single event loop | A task on a (possibly multi-thread) runtime |
| Parallelism | None for JS code (one thread); `worker_threads` for parallel | Real parallelism across worker threads by default |
| Starting work | Calling an async fn (eager) | `tokio::spawn` (a bare future is lazy until spawned/awaited) |
| Handle to the result | The returned `Promise` | `JoinHandle<T>` |
| Result type | `T` (or a rejection) | `Result<T, JoinError>` |
| A panic / throw in one job | Unhandled rejection; isolated | Caught by runtime → `JoinError`; rest of program survives |
| Cancellation | Manual via `AbortController` | `handle.abort()` (cancels at next `.await`) |
| Offloading blocking/CPU work | `worker_threads` | `tokio::task::spawn_blocking` (or threads / `rayon`) |
| Capturing outside data | Closure captures by reference, same heap | Must be `Send + 'static`: `async move`, `Arc`, owned data |

The two rows that trip up TypeScript developers most are **"Result type"** and **"capturing outside data."** A `JoinHandle` is not a transparent `Promise<T>`; you must handle the `JoinError`. And you cannot casually close over a local `let` the way a JS arrow function does — the `'static`/`Send` bounds force you to move or share ownership.

> **Note:** "Concurrency" and "parallelism" are not the same thing, and Tokio gives you both depending on the scheduler. The distinction (and why a single-thread runtime is still concurrent) is covered in [Concurrency vs Parallelism](/11-async/10-concurrency/).

---

## Common Pitfalls

### Pitfall 1: Borrowing a local into a spawned task (missing `move`)

A spawned task is `'static`, so it cannot borrow `main`'s local variables; the task might outlive the stack frame they live in:

```rust
#[tokio::main]
async fn main() {
    let name = String::from("Ada");

    let handle = tokio::spawn(async {
        // does not compile (error[E0373]): borrows `name`, but the task
        // may outlive main's stack frame.
        println!("hello, {name}");
    });

    handle.await.unwrap();
}
```

Real compiler output:

```
error[E0373]: async block may outlive the current function, but it borrows `name`, which is owned by the current function
 --> src/main.rs:5:31
  |
5 |     let handle = tokio::spawn(async {
  |                               ^^^^^ may outlive borrowed value `name`
6 |         // does not compile (error[E0373]): borrows `name`, but the task
7 |         println!("hello, {name}");
  |                           ---- `name` is borrowed here
  |
  = note: async blocks are not executed immediately and must either take a reference or ownership of outside variables they use
help: to force the async block to take ownership of `name` (and any other referenced variables), use the `move` keyword
  |
5 |     let handle = tokio::spawn(async move {
  |                                     ++++
```

**Fix:** the compiler tells you exactly what to do. Add `move` so the task takes ownership: `tokio::spawn(async move { println!("hello, {name}"); })`. To share data across several tasks instead of moving it into one, wrap it in `Arc` and clone the `Arc` per task (see [The `Arc<Mutex<T>>` Pattern](/11-async/12-arc-mutex-pattern/)).

### Pitfall 2: Holding a non-`Send` value across an `.await` in a spawned task

A spawned future must be `Send`. If you hold a non-`Send` type — `Rc`, `RefCell`, a `MutexGuard` from `std::sync` — across an `.await`, the future becomes non-`Send` and `spawn` rejects it:

```rust
use std::rc::Rc;
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() {
    let handle = tokio::spawn(async {
        // does not compile: Rc is not Send; held across .await.
        let data = Rc::new(vec![1, 2, 3]);
        sleep(Duration::from_millis(10)).await;
        println!("{:?}", data);
    });

    handle.await.unwrap();
}
```

Real compiler output (trimmed):

```
error: future cannot be sent between threads safely
   --> src/main.rs:7:18
    |
  7 |       let handle = tokio::spawn(async {
    |  __________________^
    | |______^ future created by async block is not `Send`
    |
    = help: within `{async block@src/main.rs:7:31: 7:36}`, the trait `Send` is not implemented for `Rc<Vec<i32>>`
note: future is not `Send` as this value is used across an await
   --> src/main.rs:10:42
    |
  9 |         let data = Rc::new(vec![1, 2, 3]);
    |             ---- has type `Rc<Vec<i32>>` which is not `Send`
 10 |         sleep(Duration::from_millis(10)).await;
    |                                          ^^^^^ await occurs here, with `data` maybe used later
note: required by a bound in `tokio::spawn`
```

**Fix:** use a `Send` equivalent: `Arc` instead of `Rc`, the async-aware `tokio::sync::Mutex` instead of `std::sync::Mutex` if a guard must cross an `.await` (see [Async Synchronization Primitives](/11-async/11-sync-primitives/)). Or restructure so the non-`Send` value is dropped *before* the `.await`. (TypeScript has no analogue to this error class; single-threaded JS never asks whether a value is thread-safe.)

### Pitfall 3: Blocking the runtime with synchronous work

This one compiles and runs, so the compiler will not save you. A spawned task that runs a long synchronous loop or calls `std::thread::sleep` does not yield; it pins a worker thread and starves other tasks:

```rust
// Conceptual anti-pattern (compiles, but misbehaves):
// std::thread::sleep blocks the WHOLE worker thread, not just this task.
tokio::spawn(async {
    std::thread::sleep(std::time::Duration::from_secs(5)); // blocks the worker!
    // ... other tasks on this thread cannot run for 5 seconds.
});
```

**Fix:** for async waiting, use `tokio::time::sleep(...).await` (yields the worker). For genuinely blocking or CPU-bound code, use `tokio::task::spawn_blocking`. This is the same lesson as "never block the Node event loop," but Tokio gives you `spawn_blocking` as a clean offload. (Tokio can detect *some* long blocking stalls and log a warning, but it cannot fix them for you.)

### Pitfall 4: Spawning a future and forgetting it does nothing without a runtime, or expecting `T` instead of `Result<T, _>`

Two smaller traps:

- Calling `tokio::spawn` outside any runtime **panics at runtime** (`there is no reactor running...`), covered in [Setting Up Tokio](/11-async/03-tokio-setup/#common-pitfalls).
- `handle.await` gives a `Result`, so `let x: u64 = handle.await;` fails to compile (you get `Result<u64, JoinError>`). Use `handle.await?` (if your function returns a compatible error) or `handle.await.unwrap()` / `.expect(...)` while prototyping.

---

## Best Practices

### Reach for `join!` before `spawn` when awaiting a fixed set

If you simply want to run a known set of futures concurrently and wait for all of them, [`tokio::join!`](/11-async/07-select-join/) is lighter than spawning: it runs them on the *current* task with no `Send + 'static` requirement and no extra task allocation. Use `spawn` when a task must outlive the current scope, run on another thread, or be cancelled independently.

### Use a `JoinSet` for dynamic groups of tasks

When you spawn a variable number of tasks and want to collect results as they finish, `tokio::task::JoinSet` is cleaner than a `Vec<JoinHandle<_>>`. It owns the handles, hands you results in completion order, and can abort the whole group at once:

```rust
use tokio::task::JoinSet;

async fn work(id: u32) -> u32 {
    id * id
}

#[tokio::main]
async fn main() {
    let mut set = JoinSet::new();
    for id in 1..=5 {
        set.spawn(work(id));
    }

    let mut total = 0;
    // join_next yields results as tasks finish, in completion order.
    while let Some(res) = set.join_next().await {
        total += res.expect("task panicked");
    }
    println!("total = {total}");
}
```

Real output:

```
total = 55
```

Dropping a `JoinSet` aborts all its tasks, which makes it a good fit for structured, scoped concurrency.

### Always decide what a `JoinError` means

Do not reflexively `.unwrap()` every handle in production. Decide deliberately: propagate with `?`, retry, log and continue, or treat a panicked worker as fatal. The `Result<T, JoinError>` is there so you make that choice explicitly.

### Share state with `Arc`, not by capturing references

Because tasks are `'static`, share read-only data with `Arc<T>` (clone the `Arc` into each task) and shared mutable state with `Arc<Mutex<T>>` / `Arc<RwLock<T>>`. See [The `Arc<Mutex<T>>` Pattern](/11-async/12-arc-mutex-pattern/) for the full pattern and [Async Synchronization Primitives](/11-async/11-sync-primitives/) for choosing std vs Tokio locks.

### Keep `spawn_blocking` closures genuinely blocking-only

Put only synchronous, blocking code inside `spawn_blocking`. Do not call `.await` inside it (it is not an async context) and do not use it to "parallelize" async work; that is what `spawn` and `join!` are for. Bound the blocking pool with `max_blocking_threads` if you offload a lot.

### Name long-lived tasks for observability

The unstable `tokio::task::Builder` can name tasks (under `tokio_unstable`), and the [`tracing`](https://docs.rs/tracing) crate is the idiomatic way to instrument tasks in production. Even without that, prefer holding handles (or a `JoinSet`) for important background work so failures surface instead of vanishing into a detached task.

---

## Real-World Example

A production-flavored pattern: a small concurrent crawler. We spawn one task per URL, cap how many run at once with a `Semaphore` (like a connection-pool limit), fetch the body asynchronously, and offload the CPU-bound checksum to `spawn_blocking`. Each task returns its result through a `JoinHandle`.

```rust
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use tokio::time::sleep;

/// Simulates fetching a URL (I/O-bound, async).
async fn fetch(url: &str) -> String {
    sleep(Duration::from_millis(20)).await;
    format!("<html>{url}</html>")
}

/// Simulates parsing/hashing the body (CPU-bound, synchronous).
fn checksum(body: &str) -> u64 {
    body.bytes().fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64))
}

#[tokio::main]
async fn main() {
    let urls = vec![
        "https://a.example", "https://b.example",
        "https://c.example", "https://d.example",
    ];

    // Limit how many fetches run at once (like a connection-pool cap).
    let limit = Arc::new(Semaphore::new(2));

    let mut handles: Vec<JoinHandle<(String, u64)>> = Vec::new();
    for url in urls {
        let limit = Arc::clone(&limit);
        let url = url.to_string();
        handles.push(tokio::spawn(async move {
            // Hold a permit for the duration of the fetch.
            let _permit = limit.acquire().await.expect("semaphore closed");
            let body = fetch(&url).await;

            // Offload the CPU-bound hashing to the blocking pool.
            let hash = tokio::task::spawn_blocking(move || checksum(&body))
                .await
                .expect("blocking task panicked");

            (url, hash)
        }));
    }

    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.await.expect("worker panicked"));
    }
    results.sort();

    for (url, hash) in results {
        println!("{url} -> {hash}");
    }
}
```

Real output:

```
https://a.example -> 5843935939460435433
https://b.example -> 6847466025596709832
https://c.example -> 7850996111732984231
https://d.example -> 8854526197869258630
```

This one example exercises every idea on the page: independent tasks via `spawn`, shared state via `Arc`, bounded concurrency via a `Semaphore`, CPU offload via `spawn_blocking`, and result collection via `JoinHandle`. The `Semaphore` is covered in [Async Synchronization Primitives](/11-async/11-sync-primitives/).

> **Note:** In a real crawler you would use [`reqwest`](https://docs.rs/reqwest) for HTTP and propagate errors with `Result` and `?` rather than `expect`. Error handling inside async is covered in [Async/Await Syntax](/11-async/01-async-await/).

---

## Further Reading

- [Tokio Tutorial — Spawning](https://tokio.rs/tokio/tutorial/spawning): the official walkthrough of `tokio::spawn`.
- [`tokio::spawn` docs](https://docs.rs/tokio/latest/tokio/task/fn.spawn.html) — the function, its bounds, and detach behavior.
- [`tokio::task::spawn_blocking` docs](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html) — when and how to offload blocking work.
- [`JoinHandle` docs](https://docs.rs/tokio/latest/tokio/task/struct.JoinHandle.html) and [`JoinSet` docs](https://docs.rs/tokio/latest/tokio/task/struct.JoinSet.html).
- [Tokio Tutorial — Bridging with sync code](https://tokio.rs/tokio/topics/bridging): runtimes, blocking, and `spawn_blocking` in context.
- [`std::thread` docs](https://doc.rust-lang.org/std/thread/): OS threads, for the comparison above.

Related sections of this guide:

- [Promises vs Futures](/11-async/00-promises-vs-futures/) — why a bare future is lazy and `spawn` makes it run.
- [Setting Up Tokio](/11-async/03-tokio-setup/) — wiring up the runtime that `spawn` needs.
- [Async/Await Syntax](/11-async/01-async-await/): `async`/`await` syntax and `?` inside async.
- [Concurrent Awaiting](/11-async/07-select-join/) — `join!`/`try_join!`/`select!`, the lighter alternative to spawning.
- [Async Channels](/11-async/08-channels/): passing results out of tasks with mpsc/oneshot channels.
- [Concurrency vs Parallelism](/11-async/10-concurrency/): concurrency vs parallelism, structured patterns, cancellation.
- [Async Synchronization Primitives](/11-async/11-sync-primitives/) and [The `Arc<Mutex<T>>` Pattern](/11-async/12-arc-mutex-pattern/) — sharing state across tasks.
- [Async vs Sync](/11-async/13-async-vs-sync/) — tasks vs threads vs `spawn_blocking`, CPU-bound vs I/O-bound.
- [Understanding Cargo](/01-getting-started/03-cargo-basics/): `cargo add` and `Cargo.toml`.
- [Basics](/02-basics/) — Rust fundamentals refresher.
- Next section: [Modules & Packages](/12-modules-packages/): organizing crates and modules.

---

## Exercises

### Exercise 1: Spawn and sum

**Difficulty:** Easy

**Objective:** Spawn a batch of tasks, collect their results through `JoinHandle`s, and combine them.

**Instructions:**

1. In an `async` `main`, spawn one task per `n` in `1..=10`; each task should compute `n * n`.
2. Push each `JoinHandle` into a `Vec`.
3. Await every handle, summing the results, and print the total.
4. Remember you will need `async move` to capture `n` by value.

<details>
<summary>Solution</summary>

```rust
#[tokio::main]
async fn main() {
    let mut handles = Vec::new();
    for n in 1..=10u64 {
        handles.push(tokio::spawn(async move { n * n }));
    }

    let mut total = 0u64;
    for handle in handles {
        total += handle.await.expect("task panicked");
    }

    println!("sum of squares 1..=10 = {total}");
}
```

Output:

```
sum of squares 1..=10 = 385
```

`async move` is required because the task is `'static` and must own `n` (Pitfall 1). `handle.await` returns `Result<u64, JoinError>`, so we `.expect(...)` to get the `u64`.

</details>

### Exercise 2: Offload CPU work with `spawn_blocking`

**Difficulty:** Medium

**Objective:** Move a synchronous, CPU-bound computation off the async workers and await its result.

**Instructions:**

1. Write a **synchronous** `fn count_primes(n: u64) -> u64` that counts primes below `n` (a deliberately naive loop is fine).
2. In `async` `main`, run `count_primes(10_000)` via `tokio::task::spawn_blocking`.
3. Print a message *before* awaiting (to show the runtime is free), then await the handle and print the count.

<details>
<summary>Solution</summary>

```rust
use tokio::task;

/// Synchronous, CPU-bound: count primes up to n (a stand-in for heavy work).
fn count_primes(n: u64) -> u64 {
    (2..n).filter(|&x| (2..x).all(|d| x % d != 0)).count() as u64
}

#[tokio::main]
async fn main() {
    let handle = task::spawn_blocking(|| count_primes(10_000));

    println!("counting primes on the blocking pool...");
    let primes = handle.await.expect("blocking task panicked");
    println!("primes below 10000 = {primes}");
}
```

Output:

```
counting primes on the blocking pool...
primes below 10000 = 1229
```

Running this loop inside a plain `tokio::spawn` would pin an async worker for the whole computation. `spawn_blocking` puts it on the dedicated blocking pool so the async workers stay responsive.

</details>

### Exercise 3: Bounded batch with a deadline

**Difficulty:** Medium–Hard

**Objective:** Spawn a group of tasks into a `JoinSet`, collect whatever finishes within a time budget, and cancel the rest.

**Instructions:**

1. Write `async fn process(id: u32) -> String` that sleeps 200 ms for even `id`s and 20 ms for odd ones, then returns `format!("job-{id}")`.
2. Spawn `process(1..=4)` into a `tokio::task::JoinSet`.
3. Use `tokio::select!` with a 100 ms `sleep` deadline: drain `join_next()` until either all tasks finish or the deadline fires.
4. On deadline, call `abort_all()` and drain the set. Print the jobs that completed within budget (sorted).

<details>
<summary>Solution</summary>

```rust
use std::time::Duration;
use tokio::task::JoinSet;
use tokio::time::sleep;

async fn process(id: u32) -> String {
    // Tasks 2 and 4 are "slow"; the rest are quick.
    let delay = if id % 2 == 0 { 200 } else { 20 };
    sleep(Duration::from_millis(delay)).await;
    format!("job-{id}")
}

#[tokio::main]
async fn main() {
    let mut set = JoinSet::new();
    for id in 1..=4 {
        set.spawn(process(id));
    }

    // Give the whole batch a 100 ms budget; cancel whatever is left.
    let mut done = Vec::new();
    let deadline = sleep(Duration::from_millis(100));
    tokio::pin!(deadline);

    loop {
        tokio::select! {
            maybe = set.join_next() => {
                match maybe {
                    Some(res) => done.push(res.expect("task panicked")),
                    None => break, // all tasks finished
                }
            }
            _ = &mut deadline => {
                set.abort_all();      // cancel the unfinished slow tasks
                while set.join_next().await.is_some() {} // drain
                break;
            }
        }
    }

    done.sort();
    println!("completed within budget: {done:?}");
}
```

Output:

```
completed within budget: ["job-1", "job-3"]
```

The two fast (odd) jobs finish well under 100 ms; the slow (even) ones are cancelled by `abort_all()` when the deadline fires. `tokio::pin!` is needed because we poll the same `deadline` future across multiple `select!` iterations. `select!` is covered in [Concurrent Awaiting](/11-async/07-select-join/).

</details>
