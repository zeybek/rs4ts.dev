---
title: "The `Arc<Mutex<T>>` Pattern"
description: "JavaScript shares state across async work for free; Rust makes you opt in. Arc<Mutex<T>> gives several Tokio tasks one shared, compiler-checked, race-free value."
---

In JavaScript you never think twice about sharing state between async operations — every callback closes over the same variables and they all run on one thread, so a plain object *is* your shared state. Rust will not let you do that: sharing mutable data across tasks requires you to opt in, with a combination of `Arc` (shared ownership) and `Mutex`/`RwLock` (synchronized mutation). This page is about that combination: the single most common way to hold mutable state that several Tokio tasks touch at once.

---

## Quick Overview

`Arc<Mutex<T>>` is Rust's answer to "a shared, mutable variable that more than one task can read and write." **`Arc<T>`** (Atomically Reference-Counted) gives several owners a handle to the same heap value; **`Mutex<T>`** ensures only one of them mutates it at a time. Where JavaScript's single-threaded event loop makes shared state "safe" by accident, Rust makes the sharing explicit and compiler-checked, so you trade a little ceremony for a guarantee that you can never have a data race.

> **Note:** This page focuses on `std::sync::Mutex`/`RwLock`: the *blocking* locks you use for short, non-`await` critical sections. Tokio's async-aware `tokio::sync::Mutex` exists for the rarer case of holding a lock across an `.await`, and is covered in [Async Synchronization Primitives](/11-async/11-sync-primitives/). `Arc` itself is a smart pointer; see [Smart Pointers](/10-smart-pointers/) for the wider family (`Box`, `Rc`, `Arc`).

---

## TypeScript/JavaScript Example

In Node.js, shared mutable state between concurrent async operations is invisible plumbing. Every async function closes over the same variables, and because there is exactly one thread running your JavaScript, two callbacks can never *truly* run at the same instant. A shared counter is just a `let`:

```typescript
// A "shared" counter touched by many concurrent async operations.
let counter = 0;

async function bumpManyTimes(): Promise<void> {
  for (let i = 0; i < 1000; i++) {
    // Read-modify-write. On Node this is safe because no other code
    // can run between these statements unless we `await`.
    counter += 1;
  }
}

async function main(): Promise<void> {
  // Kick off 10 "concurrent" operations.
  const ops = Array.from({ length: 10 }, () => bumpManyTimes());
  await Promise.all(ops);

  // Always 10000 — there is no parallelism, so no interleaving mid-statement.
  console.log(`final count: ${counter}`);
}

main();
```

This "just works" because of two assumptions that **do not hold in Rust + Tokio**:

1. There is only one OS thread, so `counter += 1` is effectively atomic from JavaScript's point of view.
2. Closures freely capture and mutate outer variables; the language never asks who "owns" `counter`.

If you reached for `worker_threads` to get real parallelism, you would immediately discover that those assumptions break: workers do not share `let counter` at all; you would need `SharedArrayBuffer` and `Atomics`. Rust's `Arc<Mutex<T>>` is the everyday tool that `SharedArrayBuffer` + `Atomics` only hints at.

---

## Rust Equivalent

The same counter, shared across 10 Tokio tasks that genuinely may run on different threads:

```rust
use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() {
    // The counter starts at 0, wrapped so it can be shared and mutated.
    let counter = Arc::new(Mutex::new(0u64));

    let mut handles = Vec::new();
    for _ in 0..10 {
        // Clone the Arc: a cheap pointer copy that bumps the reference count.
        // Each task gets its own handle to the SAME underlying Mutex<u64>.
        let counter = Arc::clone(&counter);

        let handle = tokio::spawn(async move {
            for _ in 0..1_000 {
                // Lock to get exclusive access. The guard unlocks on drop.
                let mut n = counter.lock().unwrap();
                *n += 1;
            } // <- guard dropped here each iteration, so other tasks can proceed
        });
        handles.push(handle);
    }

    // Wait for every task to finish.
    for handle in handles {
        handle.await.unwrap();
    }

    // 10 tasks * 1000 increments = 10000, every time, with no data races.
    println!("final count: {}", *counter.lock().unwrap());
}
```

Real output:

```
final count: 10000
```

The result is just as deterministic as the JavaScript version — `10000` every run — but for a *stronger* reason. In Node it is deterministic because nothing ever runs in parallel. In Rust it is deterministic because the `Mutex` serializes the read-modify-write, even though the tasks really can be running on different cores at the same time. Remove the `Mutex` and the program would not compile at all, which is exactly the point: Rust refuses to let you write the racy version.

---

## Detailed Explanation

Let us walk the pattern apart, because each layer is doing a distinct job.

### `Arc<T>`: shared ownership

```rust
let counter = Arc::new(Mutex::new(0u64));
```

Rust's ownership rule is "exactly one owner." A spawned task needs to *own* whatever it captures (it might outlive the function that spawned it), so 10 tasks cannot all own the same `Mutex<u64>` directly. **`Arc`** breaks the tie: it is a reference-counted pointer where every clone is a co-owner of one heap allocation. The value is dropped only when the last `Arc` goes away.

```rust
let counter = Arc::clone(&counter); // NOT a deep copy — bumps a counter, returns a new handle
```

> **Tip:** `Arc::clone(&x)` and `x.clone()` do the same thing, but the explicit `Arc::clone(&x)` form is idiomatic here: it signals to the reader "this is a cheap refcount bump, not a deep clone of the data." Clippy and the Rust API guidelines both endorse it for `Arc`/`Rc`.

The "A" in `Arc` stands for **Atomic**: the reference count is updated with atomic CPU instructions, so cloning and dropping handles is safe across threads. Its non-atomic cousin `Rc` is faster but single-threaded only — we will see exactly how the compiler enforces that in [Common Pitfalls](#common-pitfalls).

### `Mutex<T>`: synchronized mutation

`Arc` alone gives you *shared read-only* access. `Arc<u64>` would let all 10 tasks read the number, but none could change it, because shared references (`&T`) are immutable. To mutate shared data you need **interior mutability** plus **synchronization** — that is `Mutex`.

```rust
let mut n = counter.lock().unwrap(); // blocks until this task holds the lock
*n += 1;                              // `n` is a guard that derefs to the inner u64
```

`lock()` returns a `Result<MutexGuard<T>, _>` (the `Err` case is *poisoning*, covered below). The **`MutexGuard`** is a smart pointer: deref it with `*` to reach the `u64`. Importantly, the guard releases the lock automatically when it is dropped: there is no `unlock()` to forget. This is RAII, the same mechanism that frees memory; see [Ownership](/05-ownership/).

### Why the inner scope matters

In the counter loop the guard is created and dropped on every iteration. That tight scope is deliberate: the lock is held for as little time as possible, so other tasks spend less time waiting. The general rule — **hold the lock for the shortest critical section you can** — matters far more in Rust than the equivalent ever did in single-threaded JavaScript, where there was no lock to hold.

### `unwrap()` on the lock

`counter.lock().unwrap()` unwraps the `Result`. We will explain *when* this can be `Err` (poisoning) and how to handle it gracefully in [Common Pitfalls](#common-pitfalls). For now, read `.unwrap()` as "give me the guard, and panic if a previous holder panicked mid-lock."

---

## `Arc<RwLock<T>>`: many readers, occasional writer

When the data is **read far more often than it is written**, a `Mutex` is wasteful: it forces readers to queue even though concurrent reads cannot conflict. **`RwLock`** (read-write lock) splits locking into two modes: any number of readers may hold a *shared* lock simultaneously, but a writer needs an *exclusive* lock and waits for all readers to leave.

```rust
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

#[tokio::main]
async fn main() {
    // A shared read-mostly config map. Many readers, occasional writer.
    let config: Arc<RwLock<HashMap<String, String>>> =
        Arc::new(RwLock::new(HashMap::new()));

    // One writer task seeds the map.
    {
        let config = Arc::clone(&config);
        tokio::spawn(async move {
            let mut map = config.write().unwrap(); // exclusive write lock
            map.insert("region".to_string(), "us-east-1".to_string());
            map.insert("tier".to_string(), "premium".to_string());
        })
        .await
        .unwrap();
    }

    // Many reader tasks share the lock simultaneously.
    let mut handles = Vec::new();
    for id in 0..4 {
        let config = Arc::clone(&config);
        handles.push(tokio::spawn(async move {
            let map = config.read().unwrap(); // shared read lock
            let region = map.get("region").cloned().unwrap_or_default();
            format!("reader {id} sees region={region}")
        }));
    }

    for h in handles {
        println!("{}", h.await.unwrap());
    }
}
```

Real output:

```
reader 0 sees region=us-east-1
reader 1 sees region=us-east-1
reader 2 sees region=us-east-1
reader 3 sees region=us-east-1
```

`read()` returns a shared `RwLockReadGuard` (deref to read the map); `write()` returns an exclusive `RwLockWriteGuard` (deref-mut to modify it). The API mirrors `Mutex` almost exactly — you are just choosing which kind of access you need each time.

> **Warning:** `RwLock` is not automatically faster than `Mutex`. Acquiring a read lock still does atomic bookkeeping, and a constant stream of writers can *starve* readers (or vice versa, depending on the platform's policy). Use `RwLock` when reads genuinely dominate and the critical sections are non-trivial; otherwise a plain `Mutex` is simpler and often just as fast.

---

## Key Differences

| Concept | TypeScript/JavaScript | Rust (`Arc<Mutex<T>>`) |
| --- | --- | --- |
| Sharing mutable state | A captured `let`; implicit | Explicit: wrap in `Arc<Mutex<T>>` |
| Concurrency model | One thread; statements never interleave mid-line | Tasks may run on multiple threads in parallel |
| Data races | Impossible by construction (single thread) | Prevented by the compiler + the lock, not by luck |
| Releasing a lock | No locks to release | Automatic when the guard is dropped (RAII) |
| Copying the handle | Reference assignment shares the object | `Arc::clone` bumps an atomic refcount |
| Read-only vs read-write split | No distinction | `Mutex` (one writer) vs `RwLock` (many readers) |
| Forgetting synchronization | Silently fine on one thread | Won't compile across tasks |

The deepest difference is **who guarantees safety**. In JavaScript, the runtime guarantees it by never running two pieces of your code at once. In Rust, the *type system* guarantees it: the `Send` and `Sync` marker traits decide what may cross a thread boundary, and `Arc<Mutex<T>>` is the building block that makes ordinary data satisfy those bounds. You are not asked to be careful — you are asked to encode your sharing in the type, after which the compiler is careful for you.

> **Note:** `Send` means "safe to move to another thread"; `Sync` means "safe to share by reference between threads." `Mutex<T>` is `Sync` (it adds the synchronization), and `Arc<T>` is `Send + Sync` when `T` is. `Rc` is neither, which is why the next section's pitfall fails to compile.

---

## Common Pitfalls

### Pitfall 1: Forgetting to clone the `Arc` (moving it instead)

A task that captures `counter` directly *moves* it, so it is gone for the next task:

```rust
use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() {
    let counter = Arc::new(Mutex::new(0));

    let h1 = tokio::spawn(async move {
        *counter.lock().unwrap() += 1; // `counter` MOVED into this task
    });

    let h2 = tokio::spawn(async move {
        *counter.lock().unwrap() += 1; // does not compile (error[E0382]: use of moved value)
    });

    h1.await.unwrap();
    h2.await.unwrap();
}
```

The real compiler error:

```text
error[E0382]: use of moved value: `counter`
  --> src/main.rs:11:27
   |
 5 |     let counter = Arc::new(Mutex::new(0));
   |         ------- move occurs because `counter` has type `Arc<std::sync::Mutex<i32>>`, which does not implement the `Copy` trait
 6 |
 7 |     let h1 = tokio::spawn(async move {
   |                           ---------- value moved here
 8 |         *counter.lock().unwrap() += 1; // `counter` MOVED into this task
   |          ------- variable moved due to use in coroutine
...
11 |     let h2 = tokio::spawn(async move {
   |                           ^^^^^^^^^^ value used here after move
12 |         *counter.lock().unwrap() += 1; // counter already moved above
   |          ------- use occurs due to use in coroutine
   |
help: consider cloning the value before moving it into the closure
```

**Fix:** create a fresh `Arc::clone(&counter)` *before* each `tokio::spawn`, exactly as the working example does. This is the single most common stumbling block for newcomers; the compiler even suggests the fix.

### Pitfall 2: Holding a `std::sync::MutexGuard` across an `.await`

This is the big one, and it is unique to async Rust. If you take a `std::sync::Mutex` lock and then `.await` while still holding the guard, the future stops being `Send` and `tokio::spawn` rejects it:

```rust
use std::sync::{Arc, Mutex};
use std::time::Duration;

async fn bump(counter: Arc<Mutex<i32>>) {
    let mut n = counter.lock().unwrap();
    *n += 1;
    // does not compile when spawned: holding a std MutexGuard across .await
    tokio::time::sleep(Duration::from_millis(10)).await;
    *n += 1;
}

#[tokio::main]
async fn main() {
    let c = Arc::new(Mutex::new(0));
    tokio::spawn(bump(c)).await.unwrap();
}
```

The real error:

```text
error: future cannot be sent between threads safely
   --> src/main.rs:15:18
    |
 15 |     tokio::spawn(bump(c)).await.unwrap();
    |                  ^^^^^^^ future returned by `bump` is not `Send`
    |
    = help: within `impl Future<Output = ()>`, the trait `Send` is not implemented for `std::sync::MutexGuard<'_, i32>`
note: future is not `Send` as this value is used across an await
   --> src/main.rs:8:51
    |
  5 |     let mut n = counter.lock().unwrap();
    |         ----- has type `std::sync::MutexGuard<'_, i32>` which is not `Send`
...
  8 |     tokio::time::sleep(Duration::from_millis(10)).await;
    |                                                   ^^^^^ await occurs here, with `mut n` maybe used later
```

**Why:** a guard pins the lock to the thread that took it, but `.await` may resume the task on a *different* worker thread. Tokio's multi-thread scheduler refuses that.

**Fix A (preferred):** release the lock *before* you await. Put the critical section in its own scope so the guard drops:

```rust
use std::sync::{Arc, Mutex};
use std::time::Duration;

async fn bump(counter: Arc<Mutex<i32>>) {
    // Open a small scope: compute and release the lock BEFORE awaiting.
    {
        let mut n = counter.lock().unwrap();
        *n += 1;
    } // guard dropped here -> the future no longer holds a non-Send value
    tokio::time::sleep(Duration::from_millis(10)).await;
    *counter.lock().unwrap() += 1;
}

#[tokio::main]
async fn main() {
    let c = Arc::new(Mutex::new(0));
    tokio::spawn(bump(Arc::clone(&c))).await.unwrap();
    println!("value = {}", *c.lock().unwrap());
}
```

Real output:

```text
value = 2
```

**Fix B (only if you must hold the lock across the await):** switch to `tokio::sync::Mutex`, whose guard *is* `Send` and which yields the task instead of blocking the thread. That is the subject of [Async Synchronization Primitives](/11-async/11-sync-primitives/). For the vast majority of code, Fix A is correct and cheaper: keep critical sections short and synchronous.

### Pitfall 3: Deadlocking yourself by locking twice

`std::sync::Mutex` is **not reentrant**. If a task locks it and then tries to lock the *same* mutex again before releasing the first guard, it blocks forever waiting for itself:

```rust
// Pseudocode — this would HANG at runtime, so it is described, not run.
let guard1 = data.lock().unwrap();
let guard2 = data.lock().unwrap(); // deadlock: the lock is already held by us
```

This compiles fine — the compiler cannot see it — so it is a *runtime* hang, not a compile error. The usual culprit is calling a helper that also locks while you already hold the guard. **Fix:** drop the first guard before calling code that re-locks, or refactor so only one place locks. The cache exercise at the end shows the disciplined "lock, copy out, unlock, then work" pattern that avoids this.

### Pitfall 4: Panicking with the lock held (poisoning)

If a thread panics while holding a `std::sync::Mutex`, the lock becomes **poisoned**: subsequent `lock()` calls return `Err(PoisonError)` so other threads know the protected data might be in a broken, half-updated state. Blindly `.unwrap()`-ing then turns one panic into many.

```rust
use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() {
    let data = Arc::new(Mutex::new(vec![1, 2, 3]));

    // This task panics WHILE holding the lock -> the Mutex becomes "poisoned".
    let d = Arc::clone(&data);
    let _ = tokio::spawn(async move {
        let _guard = d.lock().unwrap();
        panic!("boom inside critical section");
    })
    .await;

    // Any later lock() returns Err(PoisonError); unwrap() then panics.
    match data.lock() {
        Ok(_) => println!("lock acquired cleanly"),
        Err(poisoned) => {
            // You can still recover the data via into_inner().
            let guard = poisoned.into_inner();
            println!("recovered from poison: {:?}", *guard);
        }
    }
}
```

Real output:

```text
thread 'tokio-rt-worker' panicked at src/main.rs:11:9:
boom inside critical section
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
recovered from poison: [1, 2, 3]
```

Most code keeps the `.unwrap()` (a poisoned lock usually means a bug already happened and you *want* the cascade), but for resilient services you can match on the error and call `into_inner()` to recover. JavaScript has no equivalent because a single-threaded model never has a "half-updated by another thread" hazard.

---

## Best Practices

- **Reach for `std::sync::Mutex` first, `tokio::sync::Mutex` only when you must hold a lock across `.await`.** The std lock is faster and the borrow checker will tell you (Pitfall 2) the moment you actually need the async one.
- **Keep critical sections tiny.** Lock, do the minimum read/modify/write, drop the guard. Compute expensive things and do I/O *outside* the lock.
- **Use `Arc::clone(&x)` explicitly** rather than `x.clone()` for `Arc`, so readers see it is a refcount bump, not a deep copy.
- **Prefer `RwLock` only for genuinely read-heavy data**; otherwise `Mutex` is simpler and avoids reader/writer starvation surprises.
- **Wrap the pattern in a domain type.** Instead of passing `Arc<Mutex<HashMap<K, V>>>` around, define a `struct Store { inner: Arc<Mutex<...>> }`, derive `Clone`, and expose intention-revealing methods (`get`, `insert`). This localizes the locking discipline and keeps call sites clean; the real-world example below does exactly this.
- **Consider atomics for single numbers.** A bare counter is better served by `Arc<AtomicU64>` (`std::sync::atomic`) than `Arc<Mutex<u64>>`: no locking at all. Use `Mutex`/`RwLock` when you must protect a *compound* invariant across several fields.
- **Let the guard's `Drop` release the lock.** There is no manual `unlock`; if you want to release early, call `drop(guard)` or use a `{ }` scope.

---

## Real-World Example

A small in-memory metrics registry — the kind a web service keeps to count requests per route. Many request-handling tasks increment counters concurrently; a background task or `/metrics` endpoint reads snapshots. It is read-and-write heavy with short critical sections, and it wraps the locking inside a tidy `Metrics` type that is cheap to `Clone` (it only clones the inner `Arc`).

```rust
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::time::sleep;

/// A small in-memory metrics registry shared across worker tasks.
/// Reads (snapshots) are frequent and cheap; writes (increments) are short.
#[derive(Clone)]
struct Metrics {
    // Arc makes the handle cloneable & shareable; RwLock guards the map.
    counters: Arc<RwLock<HashMap<String, u64>>>,
}

impl Metrics {
    fn new() -> Self {
        Self {
            counters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Increment a named counter. Takes a brief write lock.
    fn incr(&self, name: &str, by: u64) {
        let mut map = self.counters.write().expect("metrics lock poisoned");
        *map.entry(name.to_string()).or_insert(0) += by;
    }

    /// Take a read-only snapshot. Multiple readers can do this at once.
    fn snapshot(&self) -> HashMap<String, u64> {
        self.counters.read().expect("metrics lock poisoned").clone()
    }
}

async fn handle_request(metrics: Metrics, route: &'static str) {
    sleep(Duration::from_millis(5)).await; // simulate I/O
    metrics.incr("requests_total", 1);
    metrics.incr(route, 1);
}

#[tokio::main]
async fn main() {
    // `Metrics` is Clone (just clones the inner Arc), so each task gets a handle.
    let metrics = Metrics::new();
    let routes = ["/login", "/login", "/checkout", "/login", "/checkout"];

    let mut handles = Vec::new();
    for route in routes {
        let metrics = metrics.clone();
        handles.push(tokio::spawn(handle_request(metrics, route)));
    }

    for h in handles {
        h.await.unwrap();
    }

    // Stable iteration order for deterministic output.
    let snap = metrics.snapshot();
    let mut pairs: Vec<_> = snap.into_iter().collect();
    pairs.sort();
    for (name, count) in pairs {
        println!("{name} = {count}");
    }
}
```

Real output:

```text
/checkout = 2
/login = 3
requests_total = 5
```

Two things make this idiomatic. First, the `Metrics` wrapper hides `Arc<RwLock<...>>` so callers just see `metrics.incr(...)` and `metrics.snapshot()`: the locking discipline lives in one place. Second, every critical section is synchronous and short (`incr` holds the write lock just long enough to bump one entry; `snapshot` clones the map under a read lock and lets readers go), so no lock is ever held across the `.await` in `handle_request`. This is the shape you will reuse for shared caches, connection pools, in-memory session stores, and rate limiters.

> **Tip:** In a real Axum/Tokio service you would store this `Metrics` in the application state and clone it into each handler, the same `Arc`-clone-per-task pattern, just supplied by the framework. See [Web APIs](/16-web-apis/) for shared application state.

---

## Further Reading

- [`std::sync::Arc` — official docs](https://doc.rust-lang.org/std/sync/struct.Arc.html)
- [`std::sync::Mutex` — official docs](https://doc.rust-lang.org/std/sync/struct.Mutex.html)
- [`std::sync::RwLock` — official docs](https://doc.rust-lang.org/std/sync/struct.RwLock.html)
- [The Rust Book, Ch. 16.3 — Shared-State Concurrency](https://doc.rust-lang.org/book/ch16-03-shared-state.html)
- [Tokio tutorial — Shared state](https://tokio.rs/tokio/tutorial/shared-state)
- Related in this guide:
  - [Async Synchronization Primitives](/11-async/11-sync-primitives/): `tokio::sync::Mutex`/`RwLock`/`Semaphore`, and when async-aware locks are required
  - [Spawning Tasks](/11-async/09-spawning-tasks/) — `tokio::spawn`, `JoinHandle`, and the `Send + 'static` bound that drives this pattern
  - [Async Channels](/11-async/08-channels/): message passing, the *other* way to coordinate tasks (often preferable to shared state)
  - [Promises vs Futures](/11-async/00-promises-vs-futures/) — why Rust futures are lazy and need a runtime at all
  - [Ownership](/05-ownership/) — ownership, borrowing, and RAII (the foundation of guards)
  - [Smart Pointers](/10-smart-pointers/): `Box`, `Rc`, `Arc`, and interior mutability
  - [Modules & Packages](/12-modules-packages/) — organizing the types you build around shared state

---

## Exercises

### Exercise 1: A shared running total

**Difficulty:** Beginner

**Objective:** Get comfortable with the `Arc::clone`-per-task ritual and a `Mutex`-guarded number.

**Instructions:** Spawn 5 tasks, numbered `0..5`. Each task should add *its own id* to a shared `Arc<Mutex<u64>>`, 100 times. After all tasks finish, print the total. (With ids 0..4 each added 100 times, the total should be `1000`.)

```rust
use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() {
    let total = Arc::new(Mutex::new(0u64));
    // TODO: spawn 5 tasks, each adds its id 100 times, then print the total.
}
```

<details>
<summary>Solution</summary>

```rust
use std::sync::{Arc, Mutex};

#[tokio::main]
async fn main() {
    let total = Arc::new(Mutex::new(0u64));
    let mut handles = Vec::new();

    for worker in 0..5u64 {
        let total = Arc::clone(&total);
        handles.push(tokio::spawn(async move {
            // Each worker adds its own id, 100 times.
            for _ in 0..100 {
                *total.lock().unwrap() += worker;
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    // (0+1+2+3+4) * 100 = 1000
    println!("total = {}", *total.lock().unwrap());
}
```

Output:

```text
total = 1000
```

The key habit: a fresh `Arc::clone(&total)` *before* each `tokio::spawn`, so each task owns its own handle to the one shared `Mutex`.

</details>

### Exercise 2: Readers and a writer with `RwLock`

**Difficulty:** Intermediate

**Objective:** Use `Arc<RwLock<Vec<String>>>` with one writer task and several reader tasks, and observe that the final state is deterministic even though the readers' mid-flight observations are not.

**Instructions:** Create an `Arc<RwLock<Vec<String>>>`. Spawn one writer task that pushes `"event-1"`, `"event-2"`, `"event-3"` with a short `sleep` between each. Spawn 3 reader tasks that each sleep briefly, then read the current length. After everything finishes, print the final vector (which must be all three events).

```rust
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() {
    let log: Arc<RwLock<Vec<String>>> = Arc::new(RwLock::new(Vec::new()));
    // TODO: one writer task (write lock), three reader tasks (read lock),
    // then print the final log.
}
```

<details>
<summary>Solution</summary>

```rust
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() {
    let log: Arc<RwLock<Vec<String>>> = Arc::new(RwLock::new(Vec::new()));

    // Writer task: appends entries over time.
    let writer = {
        let log = Arc::clone(&log);
        tokio::spawn(async move {
            for i in 1..=3 {
                sleep(Duration::from_millis(10)).await;
                log.write().unwrap().push(format!("event-{i}"));
            }
        })
    };

    // Reader tasks: each reports how many entries it currently sees.
    // (The exact counts depend on timing and will vary between runs.)
    let mut readers = Vec::new();
    for id in 0..3 {
        let log = Arc::clone(&log);
        readers.push(tokio::spawn(async move {
            sleep(Duration::from_millis(15)).await;
            let len = log.read().unwrap().len();
            (id, len)
        }));
    }

    writer.await.unwrap();
    for r in readers {
        let (id, len) = r.await.unwrap();
        println!("reader {id} saw {len} entries (varies by run)");
    }
    // This line is deterministic: all writes have completed.
    println!("final log: {:?}", *log.read().unwrap());
}
```

The reader lines depend on timing and differ between runs, but the last line is always:

```text
final log: ["event-1", "event-2", "event-3"]
```

Note how `write()` takes the exclusive lock and `read()` takes the shared one, and that the writer holding the write lock briefly blocks readers, which is exactly the synchronization you want.

</details>

### Exercise 3: A concurrent memoization cache

**Difficulty:** Advanced

**Objective:** Build a thread-safe cache wrapped in a clean type, holding the lock for the shortest possible time and computing *outside* the lock (the discipline that avoids Pitfall 3).

**Instructions:** Define a `Cache` struct around `Arc<Mutex<HashMap<u64, u64>>>` that derives `Clone` and `Default`. Give it a method `get_or_compute(&self, key: u64) -> u64` that returns the cached value if present, otherwise computes `key * key`, stores it, and returns it. **Compute the value with the lock released**, then re-lock to insert using `entry(..).or_insert(..)` so a racing task's value is kept. Spawn 6 tasks requesting the overlapping keys `[2, 3, 2, 4, 3, 2]`, collect `(key, value)` results, and print the de-duplicated results plus the final cache size (which should be `3`).

```rust
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct Cache {
    inner: Arc<Mutex<HashMap<u64, u64>>>,
}

impl Cache {
    fn get_or_compute(&self, key: u64) -> u64 {
        // TODO: fast-path lock-check, compute outside the lock,
        // then re-lock to insert with entry().or_insert().
        todo!()
    }
}

#[tokio::main]
async fn main() {
    let cache = Cache::default();
    // TODO: spawn 6 tasks for keys [2, 3, 2, 4, 3, 2], collect & print results.
}
```

<details>
<summary>Solution</summary>

```rust
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct Cache {
    inner: Arc<Mutex<HashMap<u64, u64>>>,
}

impl Cache {
    /// Return the cached value, or compute it, store it, and return it.
    /// Note: the lock is released BEFORE the (pretend-expensive) compute,
    /// so other tasks are not blocked while we work.
    fn get_or_compute(&self, key: u64) -> u64 {
        // 1. Fast path: take the lock, check, release immediately.
        if let Some(v) = self.inner.lock().unwrap().get(&key) {
            return *v;
        }
        // 2. Compute outside the lock.
        let value = key * key;
        // 3. Re-lock to insert. Another task may have inserted meanwhile;
        //    entry().or_insert keeps the first writer's value.
        *self.inner.lock().unwrap().entry(key).or_insert(value)
    }
}

#[tokio::main]
async fn main() {
    let cache = Cache::default();
    let mut handles = Vec::new();

    // 6 tasks request overlapping keys concurrently.
    for key in [2u64, 3, 2, 4, 3, 2] {
        let cache = cache.clone();
        handles.push(tokio::spawn(async move { (key, cache.get_or_compute(key)) }));
    }

    let mut results: Vec<_> = Vec::new();
    for h in handles {
        results.push(h.await.unwrap());
    }
    results.sort();
    results.dedup();
    println!("{:?}", results);
    println!("cache size = {}", cache.inner.lock().unwrap().len());
}
```

Output:

```text
[(2, 4), (3, 9), (4, 16)]
cache size = 3
```

The pattern "lock → check → unlock → compute → lock → insert" keeps each critical section tiny and never holds the lock during the (here trivial, but in real life expensive) computation. Because two tasks could compute the same key concurrently, `entry(key).or_insert(value)` makes the insert idempotent: the first writer wins and both tasks return a consistent value.

> **Note:** In production you would reach for a purpose-built concurrent map like the `dashcache`/`dashmap` family or `moka` before hand-rolling this; the exercise is about understanding the mechanics underneath them.

</details>
